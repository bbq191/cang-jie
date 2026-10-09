//! 母版库（中间层暂存池）领域模块——三层架构（内容源 → **母版库** → 读器）的交汇点。
//! 两个动作各一个方法：**入库**（`stage_new` / `stage_from_path` / [`StagingStore`] 上传模板）、**落库**（`deliver` 投 xochitl）。
//! 落库＝纯复制母版字节，母版默认保留可反复落库。**书架不优化书**（2026-10-07 用户定）：书在电脑上用 sheng-ren 的 `xochitl`
//! 阅读模式优化好再上传，这里原样投进 xochitl；原来的「优化」、入库 PDF 转换、原 PDF 备份、抓网文、联网补封面都已删除，见 git 历史。
//! 目录 `$XDG_STATE_HOME/shelf/books/staging/`（/home 分区，重启/OTA 不丢；**不套 LRU 淘汰**，留住用户还没落库的书）。
//! 落库记录是同目录隐藏 sidecar `.<name>.delivered`（`sidecar` 模块管读写；本模块只在落库/删书时调它）。
use crate::mkdir::MkdirQueue;
use crate::ops::{OpGuard, OpRegistry};
use crate::render_check;
use rmsvc_core::fs::ScratchFile;
use crate::sidecar::{self, Delivered, RenderCheck};
use serde::Serialize;
use rmsvc_core::asset::{AssetItem, AssetStore};
use rmsvc_core::formats::{self, BOOK_EXTS};
use rmsvc_core::fs::{plain_name, same_content, unique_path, Content};
use rmsvc_core::xochitl::{Delivery, Xochitl};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// 按职责拆成子模块（原 `staging.rs` 一个文件 1800+ 行）：`Staging` 的方法按动作分散在各子模块的 `impl Staging` 里，
// 对外路径（`crate::staging::Staging` 等）不变；子模块内的私有项以 `pub(super)` 提供给兄弟模块与测试。
mod deliver;
/// 大文件通道的安全上限，直接导入（`crate::import`）也用同一个。
pub(crate) use self::deliver::MAX_DIRECT_BYTES;
/// 等代理建出文件夹的上限，直接导入逐级建文件夹也用同一个。
pub(crate) use self::deliver::FOLDER_WAIT_TIMEOUT;
mod intake;
mod library;
pub use self::library::{low_space, LOW_SPACE_BYTES};

use self::library::ListCaches;

#[cfg(test)]
pub(crate) mod tests;


/// 忙锁占用时的统一提示——落库/删除/改名几处几乎逐字重复过（2026-09-19 代码质量审计）。`extra`
/// 是各自独有的后缀（删除那处要额外提示"再删除"），其余传空串。
fn busy_err(name: &str, extra: &str) -> String {
    format!("《{name}》正在处理中，请稍候{extra}")
}

/// 母版库一本书的展示条目。`format`（epub / pdf / other）从扩展名判（2026-09-18 起只收 EPUB/PDF，other 只可能是更早的旧文件）。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StagingEntry {
    pub name: String,
    pub bytes: u64,
    pub format: &'static str,
    /// 入库时间（unix 秒），列表最新在前。
    pub mtime: u64,
    /// 落库记录。时间早于 `mtime`（之后母版又被替换过）= 母版已变，UI 标"旧"。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered: Option<Delivered>,
    /// 是否正有一个操作（落库、删除、改名）在这条目上跑——UI 据此禁用删除/落库等按钮，防止双击/并发操作同一条目
    /// （大书上传、等建文件夹能拖到分钟级）。
    #[serde(default)]
    pub busy: bool,
}

/// 投原生成功后交给自检线程的计划：投书时刻（毫秒，圈"之后进库"的候选）+ 文件名 / dc:title（认书用）+ 漫画页边距。
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPlan {
    pub name: String,
    pub title: Option<String>,
    pub since_ms: u64,
    /// 按页边距模式排的漫画：导入完成后登记"首次打开时设成这个页边距"（见 `comic_margins.rs`）；其余 `None`。
    pub comic_margins: Option<u32>,
}

/// `deliver` 的结果：回执文案 + （EPUB 才有）渲染自检计划。
#[derive(Debug, PartialEq)]
pub struct DeliverOutcome {
    pub message: String,
    pub render: Option<RenderPlan>,
}


#[derive(Clone)]
pub struct Staging {
    dir: PathBuf,
    xochitl: Arc<Xochitl>,
    /// 投原生体积门（字节，0=不拦）：xochitl `/upload` 超限会直接断连，先拦下来给指引。
    native_limit: u64,
    /// 正在处理的条目登记簿（忙锁），见 [`crate::ops`]。
    ops: OpRegistry,
    /// 列表的缓存（落库边车 / xochitl 页数），各按对应文件的戳失效，见 [`ListCaches`]。
    caches: Arc<ListCaches>,
    /// 漫画页边距待办（可选：测试里不装）。见 [`crate::comic_margins`]。
    comic_margins: Option<Arc<crate::comic_margins::ComicMargins>>,
    /// 母版库"落名"临界区：挑一个不撞名的文件名（`unique_path` 先查存在）再 rename/写入，两步之间不能插进别的落名，
    /// 否则两个同名书会挑到同一个名字、后到的把先到的覆盖掉。网页上传 / inbox 追平 / 改名
    /// 都从这里过。只包"挑名 + 落地"这一小段本地文件操作——此前网页上传是把 spool 锁一直攥到整个 multipart
    /// 请求体收完（WiFi 上传大书能到分钟级），期间别的上传和 inbox 追平全被卡住（2026-09-24 审计）。
    land: Arc<std::sync::Mutex<()>>,
}

/// 上传模板适配：母版库作为 [`AssetStore`]——扩展名门＝书籍格式白名单，install＝同分区 rename 入库。
/// 暂存目录应传 spool 的 `.work/`（与母版库同分区），见 `AssetUploadFlow::in_dir`。
pub struct StagingStore<'a>(pub &'a Staging);

impl AssetStore for StagingStore<'_> {
    fn kind(&self) -> &'static str {
        "book"
    }
    fn allowed_ext(&self) -> &'static [&'static str] {
        BOOK_EXTS
    }
    fn install(&self, name: &str, staged: &Path) -> Result<AssetItem, String> {
        let landed = self.0.stage_from_path(name, staged)?;
        let bytes = std::fs::metadata(self.0.dir.join(&landed)).map(|m| m.len()).unwrap_or(0);
        Ok(AssetItem::plain(landed, bytes))
    }
    fn list(&self) -> Vec<AssetItem> {
        self.0.list().into_iter().map(|e| AssetItem::plain(e.name, e.bytes)).collect()
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        self.0.remove(name)
    }
    fn reject_message(&self) -> String {
        reject_message()
    }
    fn success_message(&self, requested: &str, item: &AssetItem) -> String {
        if item.name == requested {
            "已入母版库".into()
        } else {
            // 落地名与请求名不同有两种原因：EPUB 按 `书名 - N卷` 规范命名，或母版库里已有同名（加数字前缀）。
            // 规范命名是常态，不该说成"已有同名"；只有落地名不是规范名的改动才是撞名。
            if item.name == canonical_staged_name(requested) {
                format!("已入母版库（按规范命名存为 {}）", item.name)
            } else {
                format!("已入母版库（已有同名，存为 {}）", item.name)
            }
        }
    }
}

/// 非书籍文件的拒收文案（上传门与 inbox 追平同一句）。
pub fn reject_message() -> String {
    format!("不是书籍格式，母版库只收 {}", formats::dotted(BOOK_EXTS))
}

fn canonical_staged_name(name: &str) -> String {
    if formats::ext_of(name) == "epub" {
        shelf_conv::naming::canonical_file_name(name)
    } else {
        name.to_string()
    }
}

impl Staging {
    pub fn new(dir: PathBuf, xochitl: Arc<Xochitl>, native_limit: u64) -> Staging {
        Staging {
            dir,
            xochitl,
            native_limit,
            ops: OpRegistry::default(),
            caches: Arc::default(),
            comic_margins: None,
            land: Arc::new(std::sync::Mutex::new(())),
        }
    }
    /// 接上漫画页边距待办队列（`State::new` 用）。
    pub fn with_comic_margins(mut self, q: Arc<crate::comic_margins::ComicMargins>) -> Staging {
        self.comic_margins = Some(q);
        self
    }
    /// 登记"这本书首次打开时设页边距 `margins`"。失败只记日志，不影响投书；没接队列（测试里）就不登记。
    /// 只登记按页边距模式排的漫画：`margins` 来自 sheng-ren 优化器写的 `META-INF/eink-reader-margins`
    /// （[`shelf_conv::epub::Book::reader_margins`]）。补白比例是按那个页边距算的，没按这个模式排的漫画设成 1 反而更糟
    /// （贴左、右侧空一块，文字贴屏幕边）；文字书 / PDF 完全不碰。本仓库旧版（v15、v16）优化出来的漫画不再认。
    pub(crate) fn register_comic_margins(&self, uuid: &str, name: &str, margins: u32) {
        let Some(q) = &self.comic_margins else { return };
        match q.add(uuid, margins) {
            Ok(_) => println!("[book-serve] 《{name}》是按页边距模式排的漫画，已登记首次打开时设页边距 {margins}"),
            Err(e) => println!("[book-serve] 《{name}》登记页边距失败（不影响投书）: {e}"),
        }
    }
    /// 这条目当前是否有操作在跑。
    pub fn is_busy(&self, name: &str) -> bool {
        self.ops.is_busy(name)
    }
    /// 给条目加忙锁（见 [`OpGuard`]，离开作用域自动解锁）；已经忙着 → 统一的"正在处理中"提示，`extra` 是各自的后缀。
    pub(crate) fn busy_guard(&self, name: &str, extra: &str) -> Result<OpGuard, String> {
        self.ops.try_guard(name).ok_or_else(|| busy_err(name, extra))
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }

    /// 在母版库目录里要一份新的点前缀临时文件名 `.<pid>.<序号>.landing.tmp`（列表看不见，见 [`ScratchFile`]）；
    /// 上次进程留下的由 `recover_interrupted` 按"点前缀 + `.tmp` 结尾"清掉。
    pub(super) fn scratch(&self) -> ScratchFile {
        ScratchFile::new(&self.dir, ".", "landing.tmp")
    }

    /// 进入"落名"临界区（见 `land` 字段）。
    pub(super) fn land_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        rmsvc_core::sync::lock(&self.land)
    }

    /// 母版库里某本书的路径（校验单段文件名）。
    fn path_of(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self.dir.join(plain_name(name)?))
    }
    /// 母版库里是否还有这本书。
    pub fn has(&self, name: &str) -> bool {
        self.existing(name).is_ok()
    }
    fn existing(&self, name: &str) -> Result<PathBuf, String> {
        let p = self.path_of(name)?;
        if !p.is_file() {
            return Err("母版库里没有这本书".into());
        }
        Ok(p)
    }
    /// 改这本书的落库边车（读—改—原子写）；书已不在母版库 → Err（不给已删的书复活一份边车）。
    fn update_sidecar(&self, name: &str, f: impl FnOnce(&mut Delivered)) -> Result<(), String> {
        sidecar::update(&self.existing(name)?, f)
    }
}
