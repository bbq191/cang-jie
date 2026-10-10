//! **直接导入 xochitl**（不进母版库，2026-10-07）：电脑上的 sheng-ren（`booklib sync`）经 SSH 端口转发直连本服务
//! （`ssh -L` 到设备 `127.0.0.1:8790`，不经网关、不带 `/api/books` 前缀），把已经按 `xochitl` 阅读模式优化好的 EPUB
//! 直接加入 xochitl，之后书变了就原地替换。三个动作：
//!
//! - **新导入**（[`Importer::accept_new`] + [`Importer::run`]）：请求体流式落到本服务状态目录下的临时文件（`books/import-tmp/`，**不是**
//!   母版库目录，不进母版库列表），再照落库那条路加入 xochitl——≤ 体积门走普通 `/upload`，超了走大文件通道（占位 + 磁盘替换）。
//!   `/upload` 不回 uuid：按"上传前没有、上传后新出现、`<uuid>.epub` 与上传的字节逐字节相同"认出这份新文档。
//! - **多级文件夹**（2026-10-07）：`folder` 是原件在用户书目录里的相对子目录，`/` 分隔（`漫画/死亡筆記(愛藏版)`）。按层建：
//!   先在书库根找（或建）「漫画」，再在「漫画」里找（或建）「死亡筆記(愛藏版)」，书放进最里层（[`XochitlDelivery::ensure_folder_path`]，
//!   按（上级 uuid, 名字）找，上传按 uuid 指定文件夹）。此前整串当一个名字，建出一个叫「漫画/死亡筆記(愛藏版)」的顶层文件夹。
//! - **原地替换**（[`Importer::accept_replace`] + [`Importer::run`]）：已有文档的 `<uuid>.epub` 换成新内容，uuid 不变（阅读进度 `lastOpenedPage`、
//!   所在文件夹、页边距等 `.content`/`.metadata` 里的东西都不动，漫画页边距也不重新登记——用户在阅读器里调过的不被覆盖）；
//!   删掉渲染缓存 `.pdf`/`.epubindex`，让 xochitl 下次打开时重排。
//! - **查询**（[`Importer::describe`]）：这份文档还在不在、叫什么、在哪个文件夹（客户端据此决定替换还是重新导入）。
//!
//! 删除不另设接口：客户端用已有的 `POST /trash/add {uuid, name}`（走 xochitl 自己的回收站代理，见 `trash.rs`）。
//!
//! **内存**：book-serve 的 systemd `MemoryMax=192M`，整本书绝不读进内存——请求体按块直接写盘，上传也是流式（`upload_file`）。
//! **异步处理**（2026-10-07 起，同网页「加入 xochitl」的 `spawn_deliver`）：HTTP 请求只做收体和快速校验（[`Importer::accept_new`] /
//! [`Importer::accept_replace`]），就把耗时部分（[`Importer::run`]：建文件夹、上传、等 xochitl 排版、认领、登记页边距 / 替换）
//! 交给后台作业队列（`jobs.rs`，一次一个、按提交顺序，与网页落库共用），立即回 202 + 任务 id，结果由客户端事后查。此前同步挂着等：
//! xochitl 导入时当场排版，《阿加莎全集》30MB 排了 8 分多钟，客户端 10 分钟超时、服务端认领 120 秒超时，书加进去了却回了失败，
//! 电脑上没记录、下次重复传。
use crate::delivery::{XochitlDelivery, MAX_DIRECT_BYTES};
use crate::ops::OpRegistry;
use rmsvc_core::formats::sniff;
use rmsvc_core::fs::{clean_dir, plain_name, same_content, Content, ScratchFile};
use rmsvc_core::xochitl::{is_uuid_shape, read_meta, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 直接导入的错误：HTTP 层按种类映射成 400 / 404 / 500（领域模块不碰 http 类型）。
#[derive(Debug, PartialEq)]
pub enum ImportError {
    /// 请求本身不对（文件名、格式、大小、uuid 形状）→ 400。
    Bad(String),
    /// 同一份文档正在替换中（上一次交的还没做完）→ 409；客户端看 `GET /import/{uuid}` 的 `replacing` 等它做完再交（2026-10-09）。
    Busy(String),
    /// 要替换的文档不在 / 已删除 / 在回收站 / 不是 EPUB → 404（客户端据此改成新导入）。
    NotFound(String),
    /// 设备这边出错（写盘、上传、认不出新文档）→ 500。
    Failed(String),
}

impl ImportError {
    /// 给人看的那句话（任务失败时原样放进 `message`）。
    pub fn message(&self) -> &str {
        match self {
            ImportError::Bad(m) | ImportError::Busy(m) | ImportError::NotFound(m) | ImportError::Failed(m) => m,
        }
    }
}

/// 导入 / 替换成功后、以及查询时回给客户端的文档状况。
#[derive(Debug, Clone, PartialEq)]
pub struct DocState {
    pub uuid: String,
    /// `.metadata` 的 `visibleName`（`POST /trash/add` 要用这个名字）。
    pub name: String,
    /// 所在文件夹从书库根起的完整路径（`漫画/死亡筆記(愛藏版)`）；书库根 / 已删除 → 空串。
    pub folder: String,
    /// `deleted` 为真或进了回收站。
    pub deleted: bool,
    /// 正在原地替换（`POST /import?uuid=` 交了、任务还没做完）。
    pub replacing: bool,
}

/// 收好、校验过的请求体：临时文件 + 字节数（Drop 时删临时文件——任务做完、失败、panic 都一样）。
pub struct Received {
    part: ScratchFile,
    size: u64,
}

/// 同步部分收好的一次导入，交给后台任务做完（[`Importer::run`]）。
pub struct Accepted {
    name: String,
    body: Received,
    kind: Kind,
}

enum Kind {
    /// 新导入到这个文件夹路径（`/` 分多级，原样，未拆）。
    New { folder: String },
    /// 原地替换；忙锁跟着任务走，做完（或失败）才放。
    Replace { uuid: String, _busy: crate::ops::OpGuard },
}

/// 字段都是共享句柄（`Arc` / `Clone` 的），`clone()` 出来的和原来的是同一个导入器（同一把替换忙锁），可以带进后台任务线程。
#[derive(Clone)]
pub struct Importer {
    /// 投进 xochitl 那一层（逐级建文件夹、上传 / 大文件通道、认领、登记页边距），与母版库落库共用一份。母版库本身不碰。
    delivery: Arc<XochitlDelivery>,
    /// 新导入和原地替换的请求体都先落在这（`$XDG_STATE_HOME/shelf/books/import-tmp/`，/home 分区，与 xochitl 书库同分区，
    /// 替换时收完再 rename 进书库）。进程中途被杀留下的半成品由 [`Importer::ensure`] 在启动时清掉——此前替换直接写书库里的
    /// `<uuid>.epub.new`，被杀就一直留在书库目录里（最大 1GB）。
    tmp_dir: PathBuf,
    /// 正在替换的 uuid：同一份文档同时只允许一个替换。
    replacing: OpRegistry,
    /// 原地替换前给阅读位置拍快照（见 progress.rs）；`None` 时不拍（单测）。
    progress: Option<Arc<crate::progress::Progress>>,
}

impl Importer {
    pub fn new(delivery: Arc<XochitlDelivery>, tmp_dir: PathBuf) -> Importer {
        Importer { delivery, tmp_dir, replacing: OpRegistry::default(), progress: None }
    }

    pub fn with_progress(mut self, p: Arc<crate::progress::Progress>) -> Importer {
        self.progress = Some(p);
        self
    }

    /// 建临时目录，并清掉上次进程留下的半成品（被杀时 Drop 来不及删）。返回清掉几个。
    pub fn ensure(&self) -> std::io::Result<usize> {
        std::fs::create_dir_all(&self.tmp_dir)?;
        Ok(clean_dir(&self.tmp_dir, |_| true))
    }

    fn lib(&self) -> Result<&Path, ImportError> {
        let lib = self.delivery.library_dir();
        if !lib.is_dir() {
            return Err(ImportError::Failed(format!("xochitl 书库目录不在（{}），直接导入只能在设备上用", lib.display())));
        }
        Ok(lib)
    }

    /// 新导入的同步部分：校验文件名、请求体流式落临时文件并做快速校验（非空、大小、zip 头）。耗时的建文件夹、上传、认领
    /// 留给 [`Self::run`]（HTTP 层放进后台任务队列，见 `jobs.rs`）。`folder` 原样带着，到 [`Self::run`] 里才拆、才建。
    pub fn accept_new(&self, name: &str, folder: &str, body: &mut dyn Read, declared_len: Option<usize>) -> Result<Accepted, ImportError> {
        check_name(name)?;
        self.lib()?;
        let part = ScratchFile::new(&self.tmp_dir, "", "epub.part");
        let size = receive(body, declared_len, part.path())?;
        Ok(Accepted { name: name.to_string(), body: Received { part, size }, kind: Kind::New { folder: folder.to_string() } })
    }

    /// 原地替换的同步部分：文件名、uuid 形状、能不能替换（不能 → [`ImportError::NotFound`]，HTTP 层回 404）、同一 uuid
    /// 没在替换中（忙锁随 [`Accepted`] 一起交给后台任务，任务做完才放），再收请求体。
    pub fn accept_replace(&self, uuid: &str, name: &str, body: &mut dyn Read, declared_len: Option<usize>) -> Result<Accepted, ImportError> {
        check_name(name)?;
        if !is_uuid_shape(uuid) {
            return Err(ImportError::Bad("uuid 格式不对".into()));
        }
        self.lib()?;
        self.replaceable(uuid)?;
        let busy = self.replacing.try_guard(uuid).ok_or_else(|| ImportError::Busy("这份文档正在替换中，请稍候".into()))?;
        let part = ScratchFile::new(&self.tmp_dir, "", "epub.part");
        let size = receive(body, declared_len, part.path())?;
        // 收体可能要好几分钟，期间用户可能在设备上把书删了：再确认一次（后台任务开头还会再确认一次）。
        self.replaceable(uuid)?;
        Ok(Accepted { name: name.to_string(), body: Received { part, size }, kind: Kind::Replace { uuid: uuid.to_string(), _busy: busy } })
    }

    /// 后台部分（**阻塞**，可达分钟级）：新导入走 [`Self::finish_new`]，替换走 [`Self::finish_replace`]。`stage` 报当前阶段
    /// （给人看的短语，任务查询接口原样回出去）。临时文件随 `a` 一起在这里用完删掉。
    pub fn run(&self, a: Accepted, stage: &dyn Fn(&str)) -> Result<DocState, ImportError> {
        let Accepted { name, body, kind } = a;
        match kind {
            Kind::New { folder } => self.finish_new(&name, &folder, &body, stage),
            Kind::Replace { uuid, _busy } => self.finish_replace(&uuid, &name, body, stage),
        }
    }

    /// 新导入：逐级确保文件夹 →（同字节幂等检查）→ 加入 xochitl → 认出 uuid → （漫画）登记页边距。
    /// `folder` 是 `/` 分隔的多级路径（去掉空段和各段首尾空白；空＝书库根）：逐级找，没有的经建文件夹队列在上一级里建，
    /// 某一级等不到就停在已经有的那一级（回执的 `folder` 照实写实际落进的路径）。
    fn finish_new(&self, name: &str, folder: &str, body: &Received, stage: &dyn Fn(&str)) -> Result<DocState, ImportError> {
        let lib = self.lib()?;
        let (part, size) = (body.part.path(), body.size);
        stage("建文件夹");
        let segments = folder_segments(folder);
        let (folder_uuid, _) = self.delivery.ensure_folder_path(&segments);
        let folder = folder_uuid.as_str();
        // 目标文件夹里已经有一份内容逐字节相同的活文档（上一次导入其实成功了、只是客户端没等到回执，再来一次）：认它，不再传一份重复的
        if let Some(uuid) = self.same_in_folder(lib, folder, part, size) {
            println!("[book-serve] 直接导入《{name}》：文件夹里已有内容相同的 {uuid}，不再重复加入");
            return self.describe(&uuid).ok_or_else(|| ImportError::Failed(format!("读不到 {uuid} 的 .metadata")));
        }
        let uuid = if self.delivery.over_limit(size) {
            // 大文件通道自己登记漫画页边距（同母版库落库那条路）。
            stage("上传给 xochitl（大文件通道）");
            let up = self.delivery.upload_large(part, name, folder).map_err(ImportError::Failed)?;
            up.ok_or_else(|| ImportError::Bad("读不了这本 EPUB，造不出大文件通道的占位文档".into()))?.uuid
        } else {
            let uuid = self.delivery.upload_and_claim(part, name, folder, stage).map_err(ImportError::Failed)?;
            if let Some(m) = shelf_conv::epub::Book::open(part).ok().and_then(|mut b| b.reader_margins()) {
                stage("登记页边距");
                self.delivery.register_comic_margins(&uuid, name, m);
            }
            uuid
        };
        println!("[book-serve] 直接导入《{name}》{} KB → {uuid}", size >> 10);
        self.describe(&uuid).ok_or_else(|| ImportError::Failed(format!("已加入 xochitl（{uuid}），但读不到它的 .metadata")))
    }

    /// 测试用：同步做完一次新导入（[`Self::accept_new`] + [`Self::run`]）。
    #[cfg(test)]
    pub fn import_new(&self, name: &str, folder: &str, body: &mut dyn Read, declared_len: Option<usize>) -> Result<DocState, ImportError> {
        let a = self.accept_new(name, folder, body, declared_len)?;
        self.run(a, &|_| {})
    }

    /// 测试用：同步做完一次原地替换（[`Self::accept_replace`] + [`Self::run`]）。
    #[cfg(test)]
    pub fn replace(&self, uuid: &str, name: &str, body: &mut dyn Read, declared_len: Option<usize>) -> Result<DocState, ImportError> {
        let a = self.accept_replace(uuid, name, body, declared_len)?;
        self.run(a, &|_| {})
    }

    /// 文件夹 `folder`（uuid，空串＝根）里内容和 `part` 逐字节相同的活文档。
    ///
    /// 先按 `<uuid>.epub` 的大小筛（目录项 stat，不打开文件），大小相同的才读 `.metadata` 看在不在这个文件夹、再逐字节比
    /// （2026-10-10）：此前每次导入先把全书库 `.metadata` 逐个读出来解析一遍（`live_entries`）再筛文件夹，sheng-ren 每轮同步
    /// 导入 N 本就是 N 遍全库解析。开发机 3000 本书、300 个文件夹的书库（release）：每次 10.8ms → 2.0ms。
    fn same_in_folder(&self, lib: &Path, folder: &str, part: &Path, size: u64) -> Option<String> {
        let rd = std::fs::read_dir(lib).ok()?;
        rd.flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let uuid = name.to_str()?.strip_suffix(".epub")?;
                (is_uuid_shape(uuid) && e.metadata().is_ok_and(|m| m.is_file() && m.len() == size)).then(|| uuid.to_string())
            })
            .find(|uuid| {
                read_meta(lib, uuid).ok().flatten().is_some_and(|m| m.is_live_document() && m.parent == folder) && same_content(&lib.join(format!("{uuid}.epub")), Content::File(part))
            })
    }

    /// 原地替换 `<uuid>.epub`，uuid 不变：请求体已在临时目录（0600，[`Self::accept_replace`] 收好）→ 给阅读位置拍快照
    /// （progress.rs，2026-10-09）→ 删渲染缓存 → rename 进书库
    /// （同分区，原子）。`.content`/`.metadata` 不动（保留 `lastOpenedPage`、页边距、所在文件夹），漫画页边距不重新登记。
    ///
    /// 删 `.pdf`/`.epubindex` 让 xochitl 下次打开时重排，与大文件通道替换占位的做法一致——**那条路（从没打开过的占位）2026-09-20/25
    /// 真机验证过；这条路对「已经打开过、已排版」的书 xochitl 会不会正确重排、`lastOpenedPage` 落在新排版的哪一页，尚未真机验证。**
    /// 书正开着时替换，xochitl 手里的旧排版要等关书再开才换。缩略图（`<uuid>.thumbnails/`）不动，封面变了要等 xochitl 自己刷新。
    fn finish_replace(&self, uuid: &str, name: &str, body: Received, stage: &dyn Fn(&str)) -> Result<DocState, ImportError> {
        let lib = self.lib()?;
        stage("替换文件");
        // 任务可能在队列里排了一阵，期间用户可能在设备上把书删了：别把新文件写进回收站里的条目。
        self.replaceable(uuid)?;
        // 删旧 .epubindex 之前给阅读位置拍快照，书下次打开时由 shelf-keep-progress.qmd 跳回去（progress.rs）。
        // 拍不成只记日志，不影响替换。
        if let Some(p) = &self.progress {
            match p.before_replace(uuid) {
                Ok(crate::progress::Taken::Saved(s)) => println!("[book-serve] 记下《{name}》的阅读位置：PDF 第 {} 页（{}）", s.old_page, s.file.as_deref().unwrap_or("?")),
                Ok(crate::progress::Taken::KeptPrevious) => println!("[book-serve] 《{name}》上次替换后还没打开过，沿用上次记下的阅读位置"),
                Ok(crate::progress::Taken::Skipped) => {}
                Err(e) => eprintln!("[book-serve] 记阅读位置失败（不影响替换）: {e}"),
            }
        }
        let _ = std::fs::remove_file(lib.join(format!("{uuid}.pdf")));
        let _ = std::fs::remove_file(lib.join(format!("{uuid}.epubindex")));
        std::fs::rename(body.part.path(), lib.join(format!("{uuid}.epub"))).map_err(|e| ImportError::Failed(format!("替换文件失败: {e}")))?;
        println!("[book-serve] 原地替换《{name}》→ {uuid}");
        // 这时替换的忙锁还没放（调用方的守卫），回执里的 `replacing` 按做完了写
        self.describe(uuid).map(|d| DocState { replacing: false, ..d }).ok_or_else(|| ImportError::Failed(format!("已替换（{uuid}），但读不到它的 .metadata")))
    }

    /// 能不能原地替换：`.metadata` 在、是文档、没删除没进回收站、有 `<uuid>.epub`。不能 → [`ImportError::NotFound`]。
    fn replaceable(&self, uuid: &str) -> Result<(), ImportError> {
        match self.describe(uuid) {
            None => Err(ImportError::NotFound("xochitl 书库里没有这份文档".into())),
            Some(d) if d.deleted => Err(ImportError::NotFound("这份文档已删除或在回收站里".into())),
            Some(_) if !self.delivery.library_dir().join(format!("{uuid}.epub")).is_file() => Err(ImportError::NotFound("这份文档不是 EPUB".into())),
            Some(_) => Ok(()),
        }
    }

    /// 查一份文档：`.metadata` 不在、读不了、不是文档（文件夹）、uuid 形状不对 → `None`。
    pub fn describe(&self, uuid: &str) -> Option<DocState> {
        if !is_uuid_shape(uuid) {
            return None;
        }
        let m = read_meta(self.delivery.library_dir(), uuid).ok().flatten().filter(Metadata::is_document)?;
        let deleted = !m.is_live();
        let folder = if deleted { String::new() } else { self.delivery.xochitl().folder_path(&m.parent) };
        Some(DocState { uuid: uuid.to_string(), name: m.visible_name, folder, deleted, replacing: self.replacing.is_busy(uuid) })
    }
}

/// 导入的 `folder` 拆成各级名字：按 `/` 拆，去掉各段首尾空白和空段（`" 漫画//死亡筆記 /"` → `["漫画", "死亡筆記"]`）。
fn folder_segments(folder: &str) -> Vec<&str> {
    folder.split('/').map(str::trim).filter(|s| !s.is_empty()).collect()
}

/// 文件名：单段、扩展名 `.epub`（不区分大小写）。只收 EPUB——优化好的书都是 EPUB，xochitl 也只认 EPUB/PDF。
fn check_name(name: &str) -> Result<(), ImportError> {
    plain_name(name).map_err(ImportError::Bad)?;
    if rmsvc_core::formats::ext_of(name) != "epub" {
        return Err(ImportError::Bad("直接导入只收 .epub".into()));
    }
    Ok(())
}

/// 把请求体按块写进 `dest`（0600，覆盖旧的），不整本进内存。校验：非空、不超过 [`MAX_DIRECT_BYTES`]、收到的字节数等于
/// `Content-Length`（客户端中途断开时 tiny_http 的定长读取会提前结束）、开头是 zip 本地文件头（`PK\3\4`，EPUB 是 zip）。
/// 返回字节数；出错时由调用方的 [`ScratchFile`] 删掉半成品。
fn receive(body: &mut dyn Read, declared_len: Option<usize>, dest: &Path) -> Result<u64, ImportError> {
    let too_big = || ImportError::Bad(format!("超过 {} MB 上限", MAX_DIRECT_BYTES >> 20));
    match declared_len {
        Some(0) => return Err(ImportError::Bad("请求体是空的".into())),
        Some(n) if n as u64 > MAX_DIRECT_BYTES => return Err(too_big()),
        _ => {}
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let f = o.open(dest).map_err(|e| ImportError::Failed(format!("建临时文件 {} 失败: {e}", dest.display())))?;
    let mut w = std::io::BufWriter::with_capacity(64 * 1024, f);
    let n = std::io::copy(&mut body.take(MAX_DIRECT_BYTES + 1), &mut w).map_err(|e| ImportError::Bad(format!("接收请求体中断: {e}")))?;
    let f = w.into_inner().map_err(|e| ImportError::Failed(format!("写临时文件失败: {}", e.error())))?;
    f.sync_all().map_err(|e| ImportError::Failed(format!("写临时文件失败: {e}")))?;
    if n == 0 {
        return Err(ImportError::Bad("请求体是空的".into()));
    }
    if n > MAX_DIRECT_BYTES {
        return Err(too_big());
    }
    if let Some(d) = declared_len.filter(|&d| d as u64 != n) {
        return Err(ImportError::Bad(format!("请求体不完整（收到 {n} 字节，Content-Length 是 {d}）")));
    }
    let mut magic = [0u8; 4];
    let head_ok = std::fs::File::open(dest).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && sniff(&magic) == Some("zip");
    if !head_ok {
        return Err(ImportError::Bad("不是 EPUB（不是 zip 文件）".into()));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkdir::MkdirQueue;
    use crate::staging::tests::{fake_xochitl, mini_epub, spawn_agent};
    use rmsvc_core::xochitl::Xochitl;
    use std::time::Duration;

    const U: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

    /// 书库目录 + 指向 `host` 的 xochitl 客户端 + 体积门 `limit` + 每级文件夹等 `folder_wait`；返回（导入器, 书库目录, 建文件夹队列）。
    fn importer_with(t: &tempfile::TempDir, host: &str, limit: u64, folder_wait: Duration) -> (Importer, PathBuf, Arc<MkdirQueue>) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(Xochitl::new(host, &lib, 10));
        let q = Arc::new(MkdirQueue::new(&t.path().join("state"), &lib));
        let d = Arc::new(XochitlDelivery::new(x, q.clone(), limit).with_waits(folder_wait, Duration::from_secs(3)));
        let im = Importer::new(d, t.path().join("import-tmp"));
        im.ensure().unwrap();
        (im, lib, q)
    }

    /// 同 [`importer_with`]，不关心建文件夹（没有代理，等 1 秒就放弃）。
    fn importer(t: &tempfile::TempDir, host: &str, limit: u64) -> (Importer, PathBuf) {
        let (im, lib, _) = importer_with(t, host, limit, Duration::from_secs(1));
        (im, lib)
    }

    fn put_doc(lib: &Path, uuid: &str, meta: &str) {
        std::fs::write(lib.join(format!("{uuid}.metadata")), meta).unwrap();
        std::fs::write(lib.join(format!("{uuid}.epub")), b"PK\x03\x04old").unwrap();
        std::fs::write(lib.join(format!("{uuid}.pdf")), b"render-cache").unwrap();
        std::fs::write(lib.join(format!("{uuid}.epubindex")), b"idx").unwrap();
        std::fs::write(lib.join(format!("{uuid}.content")), r#"{"fileType":"epub","lastOpenedPage":42}"#).unwrap();
    }

    #[test]
    fn import_new_claims_the_uuid_of_the_uploaded_bytes_and_cleans_tmp() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let (im, lib) = importer(&t, &fake_xochitl(lib), 1 << 20);
        let a = mini_epub(&[("OEBPS/a.xhtml", "<p>甲</p>")]);
        let d = im.import_new("甲.epub", "", &mut a.as_slice(), Some(a.len())).unwrap();
        assert_eq!(std::fs::read(lib.join(format!("{}.epub", d.uuid))).unwrap(), a, "认出的是自己上传的那份");
        assert_eq!((d.name.as_str(), d.folder.as_str(), d.deleted), ("甲.epub", "", false));
        // 同一份字节往同一文件夹再导入一次（上次其实成功了、客户端没等到回执）：认回那份，不重复加入
        let d2 = im.import_new("甲.epub", "", &mut a.as_slice(), Some(a.len())).unwrap();
        assert_eq!(d.uuid, d2.uuid);
        // 内容不同的才另加一本
        let b2 = mini_epub(&[("OEBPS/a.xhtml", "<p>乙</p>")]);
        let d3 = im.import_new("甲.epub", "", &mut b2.as_slice(), Some(b2.len())).unwrap();
        assert_ne!(d.uuid, d3.uuid);
        assert_eq!(std::fs::read_dir(t.path().join("import-tmp")).unwrap().count(), 0, "临时文件用完就删");
        assert!(!t.path().join("staging").exists() || std::fs::read_dir(t.path().join("staging")).unwrap().count() == 0, "不进母版库");
    }

    /// 书库里（上级, 名字）的活文件夹 uuid。
    fn folder(lib: &Path, parent: &str, name: &str) -> String {
        rmsvc_core::xochitl::find_child_folder(lib, parent, name).unwrap_or_else(|| panic!("没有 {parent}/{name}"))
    }

    /// 带假 xochitl 和模拟建夹代理的一套（每级文件夹最多等 `folder_wait`）：返回（导入器, 书库目录, 代理建过的记录, 停止开关）。
    #[allow(clippy::type_complexity)]
    fn with_agent(t: &tempfile::TempDir, only_root: bool, folder_wait: Duration) -> (Importer, PathBuf, Arc<std::sync::Mutex<Vec<(String, String)>>>, Arc<std::sync::atomic::AtomicBool>) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let (im, lib, q) = importer_with(t, &fake_xochitl(lib), 1 << 20, folder_wait);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let made = spawn_agent(q.clone(), lib.clone(), only_root, stop.clone());
        (im, lib, made, stop)
    }

    #[test]
    fn folder_segments_split_trim_and_drop_empty() {
        assert_eq!(folder_segments(" 漫画//死亡筆記(愛藏版) /"), ["漫画", "死亡筆記(愛藏版)"]);
        assert_eq!(folder_segments("小说"), ["小说"]);
        assert!(folder_segments(" / ").is_empty());
    }

    /// 两级路径逐级建：先在根建「漫画」，再在「漫画」里建「死亡筆記(愛藏版)」，书落进最里层，回执给完整路径。
    #[test]
    fn import_new_builds_nested_folders_level_by_level() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib, made, stop) = with_agent(&t, false, Duration::from_secs(5));
        let a = mini_epub(&[("OEBPS/a.xhtml", "<p>死</p>")]);
        let d = im.import_new("死亡筆記01.epub", "漫画/死亡筆記(愛藏版)", &mut a.as_slice(), Some(a.len())).unwrap();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let top = folder(&lib, "", "漫画");
        let inner = folder(&lib, &top, "死亡筆記(愛藏版)");
        assert_eq!(*made.lock().unwrap(), [(String::new(), "漫画".to_string()), (top.clone(), "死亡筆記(愛藏版)".to_string())], "逐级建、第二级建在第一级里");
        assert_eq!(rmsvc_core::xochitl::parent_folder_of(&lib, &d.uuid), Some(inner));
        assert_eq!(d.folder, "漫画/死亡筆記(愛藏版)");
        assert_eq!(im.describe(&d.uuid).unwrap().folder, "漫画/死亡筆記(愛藏版)");
        assert!(rmsvc_core::xochitl::find_child_folder(&lib, "", "漫画/死亡筆記(愛藏版)").is_none(), "不再建压平的顶层文件夹");
    }

    /// 上级已有只建下级；不同上级下的同名子文件夹（「漫画/卷01」「小说/卷01」）各是各的，书各落各的。
    #[test]
    fn import_new_reuses_existing_parent_and_keeps_same_named_children_apart() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib, made, stop) = with_agent(&t, false, Duration::from_secs(5));
        let manga = "aaaaaaaa-0000-4000-8000-000000000001";
        std::fs::write(lib.join(format!("{manga}.metadata")), r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#).unwrap();
        let a = mini_epub(&[("OEBPS/a.xhtml", "<p>甲</p>")]);
        let b = mini_epub(&[("OEBPS/a.xhtml", "<p>乙</p>")]);
        let da = im.import_new("甲.epub", "漫画/卷01", &mut a.as_slice(), Some(a.len())).unwrap();
        assert_eq!(*made.lock().unwrap(), [(manga.to_string(), "卷01".to_string())], "已有的上级不重建，只建下级");
        let db = im.import_new("乙.epub", "小说/卷01", &mut b.as_slice(), Some(b.len())).unwrap();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let novel = folder(&lib, "", "小说");
        let (v1m, v1n) = (folder(&lib, manga, "卷01"), folder(&lib, &novel, "卷01"));
        assert_ne!(v1m, v1n);
        assert_eq!(rmsvc_core::xochitl::parent_folder_of(&lib, &da.uuid), Some(v1m));
        assert_eq!(rmsvc_core::xochitl::parent_folder_of(&lib, &db.uuid), Some(v1n));
        assert_eq!((da.folder.as_str(), db.folder.as_str()), ("漫画/卷01", "小说/卷01"));
        // 再导入到已经全有的路径：一个都不建
        let n = made.lock().unwrap().len();
        let dc = im.import_new("丙.epub", "小说/卷01", &mut a.as_slice(), Some(a.len())).unwrap();
        assert_eq!((made.lock().unwrap().len(), dc.folder.as_str()), (n, "小说/卷01"));
    }

    /// 单段路径与原来一样：建在根、书落进去（首尾空白去掉）；空＝根。
    #[test]
    fn import_new_single_segment_matches_old_behavior() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib, _made, stop) = with_agent(&t, false, Duration::from_secs(5));
        let a = mini_epub(&[("OEBPS/a.xhtml", "<p>甲</p>")]);
        let d = im.import_new("甲.epub", " 小说 ", &mut a.as_slice(), Some(a.len())).unwrap();
        let d0 = im.import_new("乙.epub", "", &mut a.as_slice(), Some(a.len())).unwrap();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(rmsvc_core::xochitl::parent_folder_of(&lib, &d.uuid), Some(folder(&lib, "", "小说")));
        assert_eq!((d.folder.as_str(), d0.folder.as_str()), ("小说", ""));
    }

    /// 某一级等不到：停在已经建好的那一级（不落到根以外的别处），回执照实写。
    #[test]
    fn import_new_stops_at_last_folder_that_exists() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib, _made, stop) = with_agent(&t, true, Duration::from_millis(500));
        let a = mini_epub(&[("OEBPS/a.xhtml", "<p>甲</p>")]);
        let d = im.import_new("甲.epub", "漫画/卷01", &mut a.as_slice(), Some(a.len())).unwrap();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(rmsvc_core::xochitl::parent_folder_of(&lib, &d.uuid), Some(folder(&lib, "", "漫画")));
        assert_eq!(d.folder, "漫画");
    }

    #[test]
    fn import_new_over_limit_goes_through_large_file_channel() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let (im, lib) = importer(&t, &fake_xochitl(lib), 100);
        let opf = r#"<package version="2.0"><metadata><dc:title>大书</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let container = r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#;
        let big = mini_epub(&[("META-INF/container.xml", container), ("content.opf", opf), ("c1.xhtml", &"字".repeat(500))]);
        let d = im.import_new("大书.epub", "", &mut big.as_slice(), Some(big.len())).unwrap();
        assert_eq!(std::fs::read(lib.join(format!("{}.epub", d.uuid))).unwrap(), big, "占位已换成真书");
        // 大文件通道也按 uuid 落进多级文件夹的最里层（两级都已存在，不用代理）；另一个上级下的同名文件夹不会被认错
        let (top, inner, other) = ("aaaaaaaa-0000-4000-8000-00000000000a", "aaaaaaaa-0000-4000-8000-00000000000b", "aaaaaaaa-0000-4000-8000-00000000000c");
        std::fs::write(lib.join(format!("{top}.metadata")), r#"{"type":"CollectionType","visibleName":"漫画","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{other}.metadata")), r#"{"type":"CollectionType","visibleName":"大","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{inner}.metadata")), format!(r#"{{"type":"CollectionType","visibleName":"大","parent":"{top}"}}"#)).unwrap();
        let d = im.import_new("大书.epub", "漫画/大", &mut big.as_slice(), Some(big.len())).unwrap();
        assert_eq!((rmsvc_core::xochitl::parent_folder_of(&lib, &d.uuid).as_deref(), d.folder.as_str()), (Some(inner), "漫画/大"));
    }

    #[test]
    fn import_rejects_non_epub_bad_body_and_cleans_up() {
        let t = tempfile::tempdir().unwrap();
        let (im, _) = importer(&t, "127.0.0.1:9", 1 << 20);
        let ok = mini_epub(&[]);
        for (name, body, len, hint) in [
            ("a.pdf", ok.clone(), Some(ok.len()), "只收 .epub"),
            ("../a.epub", ok.clone(), Some(ok.len()), "非法文件名"),
            ("a.epub", vec![], Some(0), "空的"),
            ("a.epub", vec![], None, "空的"),
            ("a.epub", b"not a zip".to_vec(), Some(9), "不是 EPUB"),
            ("a.epub", ok.clone(), Some(ok.len() + 5), "不完整"),
            ("a.epub", ok.clone(), Some((MAX_DIRECT_BYTES + 1) as usize), "上限"),
        ] {
            match im.import_new(name, "", &mut body.as_slice(), len) {
                Err(ImportError::Bad(e)) => assert!(e.contains(hint), "{name}: {e}"),
                other => panic!("{name}: {other:?}"),
            }
        }
        assert_eq!(std::fs::read_dir(t.path().join("import-tmp")).unwrap().count(), 0, "出错也不留临时文件");
    }

    #[test]
    fn replace_keeps_uuid_and_metadata_drops_render_cache() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        std::fs::write(lib.join("ffffffff-ffff-4fff-8fff-ffffffffffff.metadata"), r#"{"type":"CollectionType","visibleName":"小说","parent":""}"#).unwrap();
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"白夜行","parent":"ffffffff-ffff-4fff-8fff-ffffffffffff","deleted":false}"#);
        let new = mini_epub(&[("OEBPS/a.xhtml", "<p>新版</p>")]);
        let d = im.replace(U, "白夜行.epub", &mut new.as_slice(), Some(new.len())).unwrap();
        assert_eq!(d, DocState { uuid: U.into(), name: "白夜行".into(), folder: "小说".into(), deleted: false, replacing: false });
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), new);
        assert!(!lib.join(format!("{U}.pdf")).exists() && !lib.join(format!("{U}.epubindex")).exists(), "渲染缓存删掉，下次打开重排");
        assert!(!lib.join(format!("{U}.epub.new")).exists());
        assert!(std::fs::read_to_string(lib.join(format!("{U}.content"))).unwrap().contains("\"lastOpenedPage\":42"), ".content 原样");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(lib.join(format!("{U}.epub"))).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // 坏体：原文件不动，不留 .new
        assert!(matches!(im.replace(U, "白夜行.epub", &mut &b"junk"[..], Some(4)), Err(ImportError::Bad(_))));
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), new);
        assert!(!lib.join(format!("{U}.epub.new")).exists());
    }

    #[test]
    fn replace_missing_deleted_trashed_or_non_epub_is_not_found() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        let body = mini_epub(&[]);
        let del = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let trash = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        let pdf = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
        put_doc(&lib, del, r#"{"type":"DocumentType","visibleName":"x","parent":"","deleted":true}"#);
        put_doc(&lib, trash, r#"{"type":"DocumentType","visibleName":"x","parent":"trash"}"#);
        put_doc(&lib, pdf, r#"{"type":"DocumentType","visibleName":"x","parent":""}"#);
        std::fs::remove_file(lib.join(format!("{pdf}.epub"))).unwrap();
        for u in [U, del, trash, pdf] {
            assert!(matches!(im.replace(u, "x.epub", &mut body.as_slice(), Some(body.len())), Err(ImportError::NotFound(_))), "{u}");
        }
        assert!(matches!(im.replace("../../etc", "x.epub", &mut body.as_slice(), None), Err(ImportError::Bad(_))));
        assert_eq!(std::fs::read(lib.join(format!("{del}.epub"))).unwrap(), b"PK\x03\x04old", "没动已删的书");
    }

    #[test]
    fn describe_reports_name_folder_and_deleted() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"书","parent":"trash"}"#);
        assert_eq!(im.describe(U), Some(DocState { uuid: U.into(), name: "书".into(), folder: String::new(), deleted: true, replacing: false }));
        assert_eq!(im.describe("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"), None);
        assert_eq!(im.describe("not-a-uuid"), None);
    }

    /// 同一份文档正在替换：再交回 Busy（HTTP 409），`describe` 的 `replacing` 为真，做完放开（2026-10-09）。
    #[test]
    fn replace_while_replacing_is_busy_and_reported() {
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"书","parent":""}"#);
        let new = mini_epub(&[("OEBPS/a.xhtml", "<p>新版</p>")]);
        let guard = im.replacing.try_guard(U).unwrap();
        assert!(im.describe(U).unwrap().replacing);
        assert!(matches!(im.accept_replace(U, "书.epub", &mut new.as_slice(), Some(new.len())), Err(ImportError::Busy(_))));
        drop(guard);
        assert!(!im.describe(U).unwrap().replacing);
        assert!(im.accept_replace(U, "书.epub", &mut new.as_slice(), Some(new.len())).is_ok());
    }

    /// 原地替换不重新登记漫画页边距：书一直是同一个文档，`.content` 里的页边距原样保留，用户在阅读器里调过的不被覆盖；
    /// 请求体先落临时目录，收完才进书库，书库目录里不留 `.epub.new` 之类的半成品。
    #[test]
    fn replace_keeps_margins_untouched_and_leaves_no_temp_in_library() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(Xochitl::new("127.0.0.1:9", &lib, 1));
        let q = Arc::new(crate::comic_margins::ComicMargins::new(&t.path().join("state"), &lib));
        let mk = Arc::new(MkdirQueue::new(&t.path().join("state"), &lib));
        let im = Importer::new(Arc::new(XochitlDelivery::new(x, mk, 0).with_comic_margins(q.clone())), t.path().join("import-tmp"));
        im.ensure().unwrap();
        put_doc(&lib, U, r#"{"type":"DocumentType","visibleName":"漫画","parent":""}"#);
        let manga = mini_epub(&[(shelf_conv::epub::READER_MARGINS_MARKER, "1")]);
        im.replace(U, "漫画.epub", &mut manga.as_slice(), Some(manga.len())).unwrap();
        assert_eq!(q.get(U), None, "替换不重新登记");
        assert_eq!(std::fs::read(lib.join(format!("{U}.epub"))).unwrap(), manga);
        let stray: Vec<_> = std::fs::read_dir(&lib).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with(U) || n.ends_with(".new")).collect();
        assert!(stray.is_empty(), "书库里不该有别的文件：{stray:?}");
        assert_eq!(std::fs::read_dir(t.path().join("import-tmp")).unwrap().count(), 0, "临时文件用完即走");
    }
    /// 原地替换前给阅读位置拍快照（删旧 `.epubindex` 之前，progress.rs）；快照写不进去也照常替换。
    #[test]
    fn replace_snapshots_reading_position_and_survives_snapshot_failure() {
        use crate::progress::{tests as pt, Lookup, Progress};
        for broken in [false, true] {
            let t = tempfile::tempdir().unwrap();
            let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
            put_doc(&lib, pt::U, r#"{"type":"DocumentType","visibleName":"人骨拼圖","parent":"","lastOpenedPage":200}"#);
            std::fs::write(lib.join(format!("{}.epubindex", pt::U)), pt::REAL_INDEX).unwrap();
            let state = t.path().join("state");
            std::fs::create_dir_all(&state).unwrap();
            if broken {
                std::fs::write(state.join("progress"), b"").unwrap(); // 快照目录的位置被普通文件占着
            }
            let p = Arc::new(Progress::new(&state, &lib));
            let im = im.with_progress(p.clone());
            let new = mini_epub(&[("OEBPS/a.xhtml", "<p>新版</p>")]);
            let d = im.replace(pt::U, "人骨拼圖.epub", &mut new.as_slice(), Some(new.len())).unwrap();
            assert_eq!(d.uuid, pt::U);
            assert_eq!(std::fs::read(lib.join(format!("{}.epub", pt::U))).unwrap(), new, "替换照常完成（broken={broken}）");
            assert!(!lib.join(format!("{}.epubindex", pt::U)).exists());
            if broken {
                assert_eq!(p.lookup(pt::U, None), Lookup::None);
            } else {
                assert_eq!(p.load(pt::U).map(|s| s.old_page), Some(200));
                assert_eq!(p.lookup(pt::U, None), Lookup::Pending, "新排版还没出来");
            }
        }
    }

    /// 同文件夹幂等检查只认"这个文件夹里、活的、字节相同"的文档：别的文件夹里、回收站里的同字节文档都不认。
    #[test]
    fn same_in_folder_only_matches_live_doc_in_that_folder() {
        const F: &str = "ffffffff-0000-4000-8000-000000000001";
        let t = tempfile::tempdir().unwrap();
        let (im, lib) = importer(&t, "127.0.0.1:9", 1 << 20);
        let part = t.path().join("p.epub");
        std::fs::write(&part, b"PK\x03\x04same").unwrap();
        let doc = |uuid: &str, parent: &str, bytes: &[u8]| {
            std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"x","parent":"{parent}"}}"#)).unwrap();
            std::fs::write(lib.join(format!("{uuid}.epub")), bytes).unwrap();
        };
        doc("aaaaaaaa-0000-4000-8000-000000000001", "", b"PK\x03\x04same");
        doc("aaaaaaaa-0000-4000-8000-000000000002", "trash", b"PK\x03\x04same");
        doc("aaaaaaaa-0000-4000-8000-000000000003", F, b"PK\x03\x04diff");
        assert_eq!(im.same_in_folder(&lib, F, &part, 8), None, "别的文件夹 / 回收站 / 同大小不同字节都不认");
        doc("aaaaaaaa-0000-4000-8000-000000000004", F, b"PK\x03\x04same");
        assert_eq!(im.same_in_folder(&lib, F, &part, 8).as_deref(), Some("aaaaaaaa-0000-4000-8000-000000000004"));
        assert_eq!(im.same_in_folder(&lib, "", &part, 8).as_deref(), Some("aaaaaaaa-0000-4000-8000-000000000001"));
    }
}
