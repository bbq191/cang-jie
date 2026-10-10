//! **投进 xochitl**（2026-10-10 抽出，审计 X-1）：母版库落库（`staging/deliver.rs`）与直接导入（`import.rs`）共用的那一层——
//! 建文件夹 → 上传（普通 `/upload` / 大文件通道）→ 认出新文档 → 登记漫画页边距。由 `State` 组装一份 [`XochitlDelivery`]，
//! `Staging` 与 `Importer` 各持一个 `Arc`；此前这些方法挂在 `Staging` 上，`Importer` 为了用它们克隆了整个 `Staging`。
//!
//! - **文件夹**：一律按层解析、按 [`Folder`] 上传（[`XochitlDelivery::ensure_folder_path`]）。网页「加入」的单段名字就是"在书库根下"
//!   （[`XochitlDelivery::ensure_folder`]），不再按名字在全库任意层找。
//! - **认领**：xochitl 的 `/upload` 不回 uuid，只能事后在书库里认。2026-10-10 第二阶段起交给基座的
//!   [`Xochitl::upload_and_claim`]（上传前快照 → 上传 → 在快照之外的新文档里找 `<uuid>.epub` 与上传字节逐字节相同的那份，
//!   [`ClaimBy::SameBytes`]；认不出就报错，**绝不取最新一本**）。第一阶段在这里自写的快照（`Claim`）与进程内认领锁随之删掉。
//!   落库与直接导入都当场等它认出来（[`XochitlDelivery::upload_and_claim`]），渲染自检线程拿到的是已经认好的 uuid，只等页数。
//! - **串行**：落库与直接导入都排进同一个后台作业队列（`jobs.rs`，一次一个）；xochitl 的"设当前文件夹 → 上传"是全局状态，
//!   本来就只能一本一本来，串行没有吞吐损失。"设当前文件夹 → 上传"这一对在基座里还有跨进程锁（运行时目录下锁文件的 flock），
//!   note-serve 同时投笔记本也不会让书落进它的文件夹。
use crate::comic_margins::ComicMargins;
use crate::mkdir::MkdirQueue;
use rmsvc_core::formats::{self, NATIVE_EXTS};
use rmsvc_core::xochitl::{ClaimBy, ClaimError, ClaimWait, Claimed, Delivery, Folder, UploadBody, Xochitl};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// 等代理建出某一级文件夹的上限——`shelf-mkdir-agent.qmd` 是长轮询（入队即刻响应），20 秒足够留出建夹 + 落盘的余量；
/// 等不到不算错误，书落在已经有的那一级（网页「加入」的单段文件夹就是落书库根）。
pub const FOLDER_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
/// 等建文件夹时书库目录写入的防抖（xochitl 建文件夹连写 metadata/content）。
const FOLDER_DEBOUNCE: Duration = Duration::from_secs(3);
/// 普通上传回 2xx 之后，等 xochitl 在书库里建好这份文档的上限（真机导入当下同步渲染，回执时多半已经落盘）。
pub const CLAIM_WAIT: Duration = Duration::from_secs(20);
/// `/upload` 读超时 / 408（"很可能已送达"，见基座 `Delivery::LikelyDelivered`）时多等一会：xochitl 还在处理大书。
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

    /// 网页「加入 xochitl → 文件夹」的目标（单段名字）→ [`Folder`]（空名＝书库根）。在**书库根正下方**按名字找，没有就经建文件夹队列请
    /// `shelf-mkdir-agent.qmd` 在根下建、等它真建出来（[`Self::ensure_folder_path`] 的单段情形）；等不到就落书库根（2026-09-19 起的行为，
    /// 不算错误）。名字里的 `/` 是普通字符（《乱马1/2》），不拆。
    ///
    /// 2026-10-10 改（审计 X-1）：此前按名字在**全库任意层**找（`Xochitl::find_folder`）再按名字上传，10-07 起直接导入会建出
    /// 「漫画/卷01」「小说/卷01」这种不同上级下的同名子文件夹，网页选"卷01"时书可能落进某个子文件夹（取 `read_dir` 先扫到的）。
    pub fn ensure_folder(&self, folder: &str) -> Folder {
        let folder = folder.trim();
        if folder.is_empty() {
            return Folder::Root;
        }
        self.ensure_folder_path(&[folder]).0
    }

    /// 按层确保多级文件夹（2026-10-07，直接导入用）：从书库根往下，每一级在上一级正下方按名字找（[`Xochitl::child_folder`]），
    /// 没有就入队请代理在上一级里建（`MkdirQueue::add_in`，代理调 `Library.createCollection(上一级 uuid, 名字)`），等它真建出来
    /// （每一级最多 `folder_wait`）。某一级等不到 / 入不了队就**停在已经有的那一级**，不往别处落。
    /// 返回（最里层拿到的文件夹；一共拿到了几级）。`segments` 由调用方拆好（不含空段）。
    pub fn ensure_folder_path(&self, segments: &[&str]) -> (Folder, usize) {
        let lib_dir = self.xochitl.library_dir();
        let mut parent = Folder::Root;
        for (depth, seg) in segments.iter().enumerate() {
            let find = || self.xochitl.child_folder(&parent, seg);
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
                Some(id) => parent = Folder::Id(id),
                None => {
                    println!("[book-serve] 文件夹「{}」第 {} 级《{seg}》{} 秒内没建出来，书落在上一级「{}」", segments.join("/"), depth + 1, self.folder_wait.as_secs(), segments[..depth].join("/"));
                    return (parent, depth);
                }
            }
        }
        (parent, segments.len())
    }

    /// 只上传、不认领（`/upload` 流式，不整本进内存）：PDF 落库用——PDF 没有渲染自检、不登记页边距，用不着 uuid，
    /// 也就不必为它拍书库快照、扫书库认领（第一阶段 PDF 落库也拍快照，结果没人用）。
    pub fn upload_only(&self, p: &Path, name: &str, folder: &Folder) -> Result<Delivery, String> {
        self.xochitl.upload_to(UploadBody::File(p), name, formats::mime_of(name), folder)
    }

    /// 普通 `/upload` + 当场认领（落库的 EPUB、直接导入）：基座 [`Xochitl::upload_and_claim`] 按 [`ClaimBy::SameBytes`] 认
    /// "上传前没有、上传后新出现、`<uuid>.epub` 与 `part` 逐字节相同"的那份。xochitl 回了 2xx 等 [`CLAIM_WAIT`]；`/upload` 判"很可能已送达"
    /// （读超时、408：大书还在排版）等 [`CLAIM_WAIT_SLOW`]。上传失败（[`ClaimError::Upload`]）与"已上传但没认出来"
    /// （[`ClaimError::NotFound`]，别马上重试）由调用方分开处理。
    pub fn upload_and_claim(&self, part: &Path, name: &str, folder: &Folder) -> Result<Claimed, ClaimError> {
        let wait = ClaimWait { delivered: self.claim_wait, likely_delivered: self.claim_wait_slow, debounce: CLAIM_DEBOUNCE };
        self.xochitl.upload_and_claim(UploadBody::File(part), name, formats::mime_of(name), folder, ClaimBy::SameBytes, wait)
    }

    /// 大文件通道（见 [`Xochitl::upload_large`]）：造占位 → 上传占位 → 磁盘上换成真文件 →（漫画）登记
    /// 页边距。条件不满足（非 EPUB/PDF、超过 [`MAX_DIRECT_BYTES`]、本机没有 xochitl 书库目录、读不了书造不出占位）→ `Ok(None)`；
    /// 占位已上传之后才出的错 → `Err`。
    pub fn upload_large(&self, p: &Path, name: &str, folder: &Folder) -> Result<Option<LargeUpload>, String> {
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
        let uuid = self.xochitl.upload_large(p, name, formats::mime_of(name), folder, &placeholder, pages)?;
        if let Some(m) = margins {
            self.register_comic_margins(&uuid, name, m);
        }
        Ok(Some(LargeUpload { uuid, pages }))
    }
}
