//! **投进 xochitl**（2026-10-10 抽出，审计 X-1）：母版库落库（`staging/deliver.rs`）与直接导入（`import.rs`）共用的那一层——
//! 建文件夹 → 上传（普通 `/upload` / 大文件通道）→ 认出新文档 → 登记漫画页边距。由 `State` 组装一份 [`XochitlDelivery`]，
//! `Staging` 与 `Importer` 各持一个 `Arc`；此前这些方法挂在 `Staging` 上，`Importer` 为了用它们克隆了整个 `Staging`。
//!
//! - **文件夹**：一律按层解析、按 uuid 上传（[`XochitlDelivery::ensure_folder_path`]）。网页「加入」的单段名字就是"在书库根下"
//!   （[`XochitlDelivery::ensure_folder`]），不再按名字在全库任意层找。
//! - **认领**：xochitl 的 `/upload` 不回 uuid，只能事后在书库里找：上传前给书库拍个快照（[`Claim::snapshot`]：这一刻已经在的
//!   近期文档），上传后找"快照里没有、新出现、`<uuid>.epub` 与上传的字节逐字节相同"的那份（[`Claim::find`]）。直接导入当场等它
//!   （[`XochitlDelivery::upload_and_claim`]），落库交给渲染自检线程慢慢认（`render_check.rs`）。此前渲染自检按"投书时刻之后进库
//!   + 书名相符，都不符时取最新一本"认，并发投递时会把别人刚投的书认成自己的，漫画还会把页边距登记到别人的书上。
//! - **串行**：落库与直接导入都排进同一个后台作业队列（`jobs.rs`，一次一个）；xochitl 的"设当前文件夹 → 上传"是全局状态，
//!   本来就只能一本一本来，串行没有吞吐损失。
use crate::comic_margins::ComicMargins;
use crate::mkdir::MkdirQueue;
use rmsvc_core::formats::{self, NATIVE_EXTS};
use rmsvc_core::xochitl::{Delivery, Xochitl};
use std::sync::Arc;
use std::time::Duration;
use rmsvc_core::fs::{same_content, Content};
use rmsvc_core::xochitl::{find_documents_since, DocInfo};
use std::collections::HashSet;
use std::path::Path;

/// 上传前拍的快照：候选只看 `createdTime >= since_ms` 的文档，`before` 里的（上传前就在的）不算。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Claim {
    pub since_ms: u64,
    pub before: HashSet<String>,
}

impl Claim {
    /// 上传前调。时刻往前留 2 秒：xochitl 的 `createdTime` 与本机时钟取整不同步，宁可多圈几份候选（`before` 和逐字节比对会排除）。
    pub fn snapshot(lib: &Path) -> Claim {
        let since_ms = rmsvc_core::clock::now_ms().saturating_sub(2_000);
        let before = find_documents_since(lib, since_ms).into_iter().map(|d| d.uuid).collect();
        Claim { since_ms, before }
    }

    /// 快照之后新出现的文档（新→旧）。
    pub fn fresh(&self, lib: &Path) -> Vec<DocInfo> {
        find_documents_since(lib, self.since_ms).into_iter().filter(|d| !self.before.contains(&d.uuid)).collect()
    }

    /// 新出现、`<uuid>.epub` 与 `part` 逐字节相同的那份（先比大小，一样才读）；还没有 → `None`。
    pub fn find(&self, lib: &Path, part: &Path) -> Option<String> {
        self.fresh(lib).into_iter().find(|d| same_content(&lib.join(format!("{}.epub", d.uuid)), Content::File(part))).map(|d| d.uuid)
    }
}

/// 等代理建出某一级文件夹的上限——`shelf-mkdir-agent.qmd` 是长轮询（入队即刻响应），20 秒足够留出建夹 + 落盘的余量；
/// 等不到不算错误，书落在已经有的那一级（网页「加入」的单段文件夹就是落书库根）。
pub const FOLDER_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
/// 等建文件夹时书库目录写入的防抖（xochitl 建文件夹连写 metadata/content）。
const FOLDER_DEBOUNCE: Duration = Duration::from_secs(3);
/// 普通上传回 2xx 之后，等 xochitl 在书库里建好这份文档的上限（真机导入当下同步渲染，回执时多半已经落盘）。
pub const CLAIM_WAIT: Duration = Duration::from_secs(20);
/// `/upload` 读超时 / 408（"很可能已送达"，见 `upload_likely_delivered`）时多等一会：xochitl 还在处理大书。
/// 2026-10-07 真机：《阿加莎全集》（30MB、85 册）排在几卷大漫画后面，xochitl 导入时排版前后 8 分多钟才落盘，原来的 120 秒认不出、
/// 回了失败（书其实已经加进去了）。放宽到 30 分钟。
pub const CLAIM_WAIT_SLOW: Duration = Duration::from_secs(30 * 60);
/// 认领时书库目录写入的防抖（xochitl 导入时连写 metadata/content/epub）。
const CLAIM_DEBOUNCE: Duration = Duration::from_millis(500);
/// 大文件通道的安全上限（1GiB）：再大 xochitl 首次渲染的内存/时间没有验证过。
pub const MAX_DIRECT_BYTES: u64 = 1 << 30;

/// [`XochitlDelivery::upload_large`] 的结果：新文档的 uuid；PDF 还有写进 `.content` 的页数（EPUB 首次打开才渲染，`None`）。
pub struct LargeUpload {
    pub uuid: String,
    pub pages: Option<usize>,
}

pub struct XochitlDelivery {
    xochitl: Arc<Xochitl>,
    /// 建文件夹队列（`shelf-mkdir-agent.qmd` 拉取执行，见 mkdir.rs）。
    mkdir: Arc<MkdirQueue>,
    /// 漫画页边距待办（可选：测试里不装）。见 [`crate::comic_margins`]。
    comic_margins: Option<Arc<ComicMargins>>,
    /// 普通 `/upload` 的体积门（字节，0=不拦），同 `BookConfig::native_upload_limit_bytes`：xochitl `/upload` 超限会直接断连，超了走大文件通道。
    native_limit: u64,
    /// 每一级文件夹等代理建出来的上限（线上 [`FOLDER_WAIT_TIMEOUT`]，单测改短）。
    folder_wait: Duration,
    /// 认领的等待上限（线上 [`CLAIM_WAIT`] / [`CLAIM_WAIT_SLOW`]，单测改短）。
    claim_wait: Duration,
    claim_wait_slow: Duration,
}

impl XochitlDelivery {
    pub fn new(xochitl: Arc<Xochitl>, mkdir: Arc<MkdirQueue>, native_limit: u64) -> XochitlDelivery {
        XochitlDelivery { xochitl, mkdir, comic_margins: None, native_limit, folder_wait: FOLDER_WAIT_TIMEOUT, claim_wait: CLAIM_WAIT, claim_wait_slow: CLAIM_WAIT_SLOW }
    }

    /// 接上漫画页边距待办队列（`State::new` 用）。
    pub fn with_comic_margins(mut self, q: Arc<ComicMargins>) -> XochitlDelivery {
        self.comic_margins = Some(q);
        self
    }

    /// 单测用：改短等文件夹 / 等认领的上限。
    #[cfg(test)]
    pub fn with_waits(mut self, folder: Duration, claim: Duration) -> XochitlDelivery {
        (self.folder_wait, self.claim_wait, self.claim_wait_slow) = (folder, claim, claim);
        self
    }

    pub fn xochitl(&self) -> &Xochitl {
        &self.xochitl
    }

    /// 书库目录（`<uuid>.{metadata,content,epub,pdf}` 所在）。
    pub fn library_dir(&self) -> &Path {
        self.xochitl.library_dir()
    }

    #[cfg(test)]
    pub fn mkdir(&self) -> &Arc<MkdirQueue> {
        &self.mkdir
    }

    /// 体积门（字节，0=不拦）。
    pub fn native_limit(&self) -> u64 {
        self.native_limit
    }

    /// 这本书超过体积门、得走大文件通道。
    pub fn over_limit(&self, size: u64) -> bool {
        self.native_limit > 0 && size > self.native_limit
    }

    /// 登记"这本书首次打开时设页边距 `margins`"。失败只记日志，不影响投书；没接队列（测试里）就不登记。
    /// 只登记按页边距模式排的漫画：`margins` 来自 sheng-ren 优化器写的 `META-INF/eink-reader-margins`
    /// （[`shelf_conv::epub::Book::reader_margins`]）。补白比例是按那个页边距算的，没按这个模式排的漫画设成 1 反而更糟
    /// （贴左、右侧空一块，文字贴屏幕边）；文字书 / PDF 完全不碰。本仓库旧版（v15、v16）优化出来的漫画不再认。
    pub fn register_comic_margins(&self, uuid: &str, name: &str, margins: u32) {
        let Some(q) = &self.comic_margins else { return };
        match q.add(uuid, margins) {
            Ok(_) => println!("[book-serve] 《{name}》是按页边距模式排的漫画，已登记首次打开时设页边距 {margins}"),
            Err(e) => println!("[book-serve] 《{name}》登记页边距失败（不影响投书）: {e}"),
        }
    }

    /// 网页「加入 xochitl → 文件夹」的目标（单段名字）→ uuid（空串＝书库根）。在**书库根正下方**按名字找，没有就经建文件夹队列请
    /// `shelf-mkdir-agent.qmd` 在根下建、等它真建出来（[`Self::ensure_folder_path`] 的单段情形）；等不到就落书库根（2026-09-19 起的行为，
    /// 不算错误）。名字里的 `/` 是普通字符（《乱马1/2》），不拆。
    ///
    /// 2026-10-10 改（审计 X-1）：此前按名字在**全库任意层**找（`Xochitl::find_folder`）再按名字上传，10-07 起直接导入会建出
    /// 「漫画/卷01」「小说/卷01」这种不同上级下的同名子文件夹，网页选"卷01"时书可能落进某个子文件夹（取 `read_dir` 先扫到的）。
    pub fn ensure_folder(&self, folder: &str) -> String {
        let folder = folder.trim();
        if folder.is_empty() {
            return String::new();
        }
        self.ensure_folder_path(&[folder]).0
    }

    /// 按层确保多级文件夹（2026-10-07，直接导入用）：从书库根往下，每一级在上一级正下方按名字找（[`rmsvc_core::xochitl::find_child_folder`]），
    /// 没有就入队请代理在上一级里建（`MkdirQueue::add_in`，代理调 `Library.createCollection(上一级 uuid, 名字)`），等它真建出来
    /// （每一级最多 `folder_wait`）。某一级等不到 / 入不了队就**停在已经有的那一级**，不往别处落。
    /// 返回（最里层拿到的文件夹 uuid，空串＝根；一共拿到了几级）。`segments` 由调用方拆好（不含空段）。
    pub fn ensure_folder_path(&self, segments: &[&str]) -> (String, usize) {
        let lib_dir = self.xochitl.library_dir();
        let mut parent = String::new();
        for (depth, seg) in segments.iter().enumerate() {
            let find = || self.xochitl.find_child_folder(&parent, seg);
            let found = find().or_else(|| match self.mkdir.add_in(&parent, seg) {
                // 两次查询之间刚被建出来了
                Ok(0) => find(),
                // `wait_for` 先挂监听再查一次：入队到挂上监听之间代理已经建好的，也不会白等满
                Ok(_) => rmsvc_core::fswatch::wait_for(lib_dir, FOLDER_DEBOUNCE, self.folder_wait, find),
                Err(e) => {
                    println!("[book-serve] 建文件夹《{seg}》入队失败: {e}");
                    find()
                }
            });
            match found {
                Some(uuid) => parent = uuid,
                None => {
                    println!("[book-serve] 文件夹「{}」第 {} 级《{seg}》{} 秒内没建出来，书落在上一级「{}」", segments.join("/"), depth + 1, self.folder_wait.as_secs(), segments[..depth].join("/"));
                    return (parent, depth);
                }
            }
        }
        (parent, segments.len())
    }

    /// 普通 `/upload`（流式，不整本进内存）：先拍书库快照再上传，快照留给调用方认领（[`Claim::find`]）。`folder_uuid` 空串＝根。
    pub fn upload(&self, p: &Path, name: &str, folder_uuid: &str) -> Result<(Delivery, Claim), String> {
        let claim = Claim::snapshot(self.xochitl.library_dir());
        let d = self.xochitl.upload_file_into(p, name, formats::mime_of(name), folder_uuid)?;
        Ok((d, claim))
    }

    /// 普通 `/upload` + 当场认领（直接导入用）：上传后等书库里新出现、内容与 `part` 逐字节相同的那份。`/upload` 判"很可能已送达"
    /// （超时、408）时多等（[`CLAIM_WAIT_SLOW`]）。整段串行（进程内一把锁）：两次导入同一份字节时，后一次不会把前一次刚建的文档认成自己的。
    /// 上传失败 → `Err` 以"上传给 xochitl 失败"开头；认不出 → `Err` 说明可能稍后才出现。
    pub fn upload_and_claim(&self, part: &Path, name: &str, folder_uuid: &str, stage: &dyn Fn(&str)) -> Result<String, String> {
        static CLAIM: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _serial = rmsvc_core::sync::lock(&CLAIM);
        let lib = self.xochitl.library_dir();
        // xochitl 收 `/upload` 时当场排版，回 2xx 前大书能挂好几分钟（2026-10-07《阿加莎全集》8 分多钟）。
        stage("上传给 xochitl");
        let (wait, snap) = match self.upload(part, name, folder_uuid) {
            Ok((Delivery::Delivered(_), c)) => (self.claim_wait, c),
            Ok((Delivery::LikelyDelivered(_), c)) => (self.claim_wait_slow, c),
            Err(e) => return Err(format!("上传给 xochitl 失败: {e}")),
        };
        stage("等 xochitl 排版");
        // 先查一次（真机导入当下同步渲染，回执时多半已经落盘），没有再等书库目录的变化（事件驱动，此前每 200ms 扫一遍书库）。
        rmsvc_core::fswatch::wait_for(lib, CLAIM_DEBOUNCE, wait, || snap.find(lib, part))
            .ok_or_else(|| format!("已上传给 xochitl，但 {} 秒内没在书库里认出《{name}》（可能稍后才出现；先在设备上看一眼，别马上重试，免得重复）", wait.as_secs()))
    }

    /// 大文件通道（见 [`rmsvc_core::xochitl::Xochitl::upload_large_file_into`]）：造占位 → 上传占位 → 磁盘上换成真文件 →（漫画）登记
    /// 页边距。条件不满足（非 EPUB/PDF、超过 [`MAX_DIRECT_BYTES`]、本机没有 xochitl 书库目录、读不了书造不出占位）→ `Ok(None)`；
    /// 占位已上传之后才出的错 → `Err`。目标文件夹按 **uuid** 给（`folder_uuid`，空串＝根）。
    pub fn upload_large(&self, p: &Path, name: &str, folder_uuid: &str) -> Result<Option<LargeUpload>, String> {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if !formats::has_ext(name, NATIVE_EXTS) || size > MAX_DIRECT_BYTES || !self.xochitl.library_dir().is_dir() {
            return Ok(None);
        }
        let (placeholder, pages, margins) = if formats::ext_of(name) == "epub" {
            // 显示名沿用书自己的 dc:title（sheng-ren 优化时已写好规范书名）；书名、封面、页边距标记同一次打开 zip 取完。
            let Ok(mut book) = shelf_conv::epub::Book::open(p) else { return Ok(None) };
            let Ok(ph) = shelf_conv::placeholder::epub_placeholder(&mut book) else { return Ok(None) };
            (ph, None, book.reader_margins())
        } else {
            // 第三方 PDF 常是交叉引用流/对象流、页树根不在对象 2：走通用的有界读取（`pdfmeta`），不整本读进内存。
            let Ok(pages) = shelf_conv::pdfmeta::page_count(p) else { return Ok(None) };
            (shelf_conv::placeholder::pdf_placeholder(), Some(pages), None)
        };
        let uuid = self.xochitl.upload_large_file_into(p, name, formats::mime_of(name), folder_uuid, &placeholder, pages)?;
        if let Some(m) = margins {
            self.register_comic_margins(&uuid, name, m);
        }
        Ok(Some(LargeUpload { uuid, pages }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(lib: &Path, uuid: &str, bytes: &[u8]) {
        std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"x","parent":"","createdTime":"{}"}}"#, rmsvc_core::clock::now_ms())).unwrap();
        std::fs::write(lib.join(format!("{uuid}.epub")), bytes).unwrap();
    }

    /// 快照前就在的（哪怕字节相同）不认；快照后新出现的只认字节相同的那份。
    #[test]
    fn claims_only_new_byte_identical_document() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        let part = t.path().join("a.epub");
        std::fs::write(&part, b"PK-ours").unwrap();
        doc(&lib, "old-same", b"PK-ours");
        let c = Claim::snapshot(&lib);
        assert!(c.before.contains("old-same"));
        assert_eq!(c.find(&lib, &part), None, "上传前就在的不认");
        doc(&lib, "new-other", b"PK-them");
        assert_eq!(c.find(&lib, &part), None, "别人的新书不认");
        doc(&lib, "new-ours", b"PK-ours");
        assert_eq!(c.find(&lib, &part).as_deref(), Some("new-ours"));
    }
}
