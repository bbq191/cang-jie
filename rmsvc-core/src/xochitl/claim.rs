//! 上传并认领：`/upload` 不回新文档的 uuid，调用方要自己在书库里把它认出来。此前 4 处各写一套认领判据
//! （book-serve 直接导入：字节相同；渲染自检：书名相符、**都不符就取最新一本**；note-serve：visibleName；大文件通道：
//! 占位字节数），最弱的那套会把漫画页边距登记到别人刚投的书上（2026-10-10 审计 X-1）。这里收成一个入口
//! [`super::Xochitl::upload_and_claim`]：
//!
//! 1. 上传前快照"最近两秒内建的文档"（`createdTime` 早于此的本来就不算候选）；
//! 2. 上传（"设当前文件夹 → /upload"整对在跨进程锁里，见 [`UploadLock`]）；
//! 3. 等书库目录变化（[`crate::fswatch::wait_for`]），按调用方给的判据 [`ClaimBy`] 在**快照之外的新文档**里找；
//! 4. 找不到就报错，**绝不"取最新一本"兜底**——认错一本比认不出糟得多（认不出调用方会提示用户去设备上看一眼）。
//!
//! 同一进程内整段（快照 → 上传 → 认领）串行：两次上传同样字节时，后一次的快照一定包含前一次刚认领的文档，不会认重。
use super::{Delivery, Folder, Xochitl};
use crate::fs::{same_content, Content};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 上传的内容：磁盘文件（流式发送，不整体读进内存）或内存里的字节（几 KB 的笔记本、占位文档）。
#[derive(Clone, Copy, Debug)]
pub enum UploadBody<'a> {
    File(&'a Path),
    Bytes(&'a [u8]),
}

impl UploadBody<'_> {
    fn len(&self) -> Result<u64, String> {
        match self {
            UploadBody::File(p) => std::fs::metadata(p).map(|m| m.len()).map_err(|e| format!("读 {} 失败: {e}", p.display())),
            UploadBody::Bytes(b) => Ok(b.len() as u64),
        }
    }

    fn content(&self) -> Content<'_> {
        match *self {
            UploadBody::File(p) => Content::File(p),
            UploadBody::Bytes(b) => Content::Bytes(b),
        }
    }
}

/// 认领判据（Strategy）。都只在"快照之外、`createdTime` 不早于上传前两秒"的活文档里找。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimBy<'a> {
    /// 书库里 `<uuid>.<上传文件名的扩展名>` 与上传内容逐字节相同（先比大小）。EPUB/PDF 原样进库时最可靠。
    SameBytes,
    /// visibleName 与给定名字完全相同。xochitl 会改写内容的格式（`.rmdoc` 笔记本解包成页面）只能用这个。
    VisibleName(&'a str),
    /// `<uuid>.<扩展名>` 的大小等于上传内容的大小（大文件通道：占位文档很快就要被换成真文件，只认大小）。
    SameSize,
}

/// 认领最多等多久：xochitl 回了 2xx（`delivered`）与只是"很可能已送达"（`likely_delivered`，读超时/408：
/// 大书还在排版）分开给；`debounce` 是书库目录静默多久再查一次（xochitl 建条目时连写好几个文件，攒一下只查一次）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimWait {
    pub delivered: Duration,
    pub likely_delivered: Duration,
    pub debounce: Duration,
}

impl ClaimWait {
    /// 两种情况等同样久，防抖 150ms。
    pub fn new(timeout: Duration) -> ClaimWait {
        ClaimWait { delivered: timeout, likely_delivered: timeout, debounce: Duration::from_millis(150) }
    }
}

/// 认领到的新文档。
#[derive(Debug, PartialEq)]
pub struct Claimed {
    pub uuid: String,
    /// 上传本身的结果（回执文案要区分"设备处理较慢"时用）。
    pub delivery: Delivery,
}

/// [`super::Xochitl::upload_and_claim`] 的失败：上传就没成（可以重试）与"已上传但没认出来"（**别马上重试**，
/// 文档多半稍后会出现，重试就是两份）必须分开。
#[derive(Debug, PartialEq)]
pub enum ClaimError {
    Upload(String),
    NotFound { delivery: Delivery, waited: Duration },
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaimError::Upload(e) => write!(f, "上传给 xochitl 失败: {e}"),
            ClaimError::NotFound { waited, .. } => write!(f, "已上传给 xochitl，但 {} 秒内没在书库里认出它（可能稍后才出现；先在设备上看一眼，别马上重试，免得重复）", waited.as_secs()),
        }
    }
}

impl From<ClaimError> for String {
    fn from(e: ClaimError) -> String {
        e.to_string()
    }
}

/// 快照/候选的时间窗：`createdTime >= 上传前这么久` 的才算（与此前各处的 2 秒一致：xochitl 的 createdTime 取自
/// 设备时钟，与本进程的时钟同源，两秒余量够文件系统时间戳取整）。
const SINCE_MARGIN_MS: u64 = 2_000;

/// 同一进程内"快照 → 上传 → 认领"整段串行的锁（见模块文档）。只锁本进程：跨进程的调用方（book-serve 投书、note-serve
/// 投笔记本）判据互不相交（字节 / visibleName 不会撞），不需要把认领等待也做成跨进程互斥——那样 book-serve 等一本大书
/// 排版（最长 30 分钟）时 note-serve 的同步请求会被一起挡住。
static CLAIM: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Xochitl {
    /// 上传并认领新文档的 uuid（见模块文档）。`filename` 的扩展名决定 [`ClaimBy::SameBytes`]/[`ClaimBy::SameSize`] 比对书库里哪个文件。
    pub fn upload_and_claim(&self, body: UploadBody<'_>, filename: &str, content_type: &str, folder: &Folder, by: ClaimBy<'_>, wait: ClaimWait) -> Result<Claimed, ClaimError> {
        let _serial = crate::sync::lock(&CLAIM);
        let lib = self.library_dir.as_path();
        let ext = crate::formats::ext_of(filename);
        let body_len = body.len().map_err(ClaimError::Upload)?;
        let since = crate::clock::now_ms().saturating_sub(SINCE_MARGIN_MS);
        let before: std::collections::HashSet<String> = super::find_documents_since(lib, since).into_iter().map(|d| d.uuid).collect();
        let delivery = match body {
            UploadBody::File(p) => {
                let file = std::fs::File::open(p).map_err(|e| ClaimError::Upload(format!("打开 {}: {e}", p.display())))?;
                self.upload_body(std::io::BufReader::new(file), body_len, filename, content_type, folder)
            }
            UploadBody::Bytes(b) => self.upload_body(std::io::Cursor::new(b), body_len, filename, content_type, folder),
        }
        .map_err(ClaimError::Upload)?;
        let timeout = match delivery {
            Delivery::Delivered(_) => wait.delivered,
            Delivery::LikelyDelivered(_) => wait.likely_delivered,
        };
        let matches = |uuid: &str, visible_name: &str| -> bool {
            let file = || lib.join(format!("{uuid}.{ext}"));
            match by {
                ClaimBy::SameBytes => same_content(&file(), body.content()),
                ClaimBy::VisibleName(n) => visible_name == n,
                ClaimBy::SameSize => std::fs::metadata(file()).is_ok_and(|m| m.is_file() && m.len() == body_len),
            }
        };
        // 候选按 createdTime 新→旧；同一判据下多于一份新文档（别的进程/用户同时传了同样的东西）时取最早建的那份：
        // 本进程内串行，最早出现的最可能是我们这一份（后来者是在我们上传之后才建的）。
        let claim = || {
            let mut docs = super::find_documents_since(lib, since);
            docs.reverse();
            docs.into_iter().find(|d| !before.contains(&d.uuid) && matches(&d.uuid, &d.visible_name)).map(|d| d.uuid)
        };
        match crate::fswatch::wait_for(lib, wait.debounce, timeout, claim) {
            Some(uuid) => Ok(Claimed { uuid, delivery }),
            None => Err(ClaimError::NotFound { delivery, waited: timeout }),
        }
    }
}

/// "设当前文件夹 → /upload" 这一对的互斥：进程内一把锁 + 运行时目录下锁文件的 `flock`（跨进程：book-serve 投书与
/// note-serve 投笔记本此前只有各自进程内的锁，两边交错时书或笔记本会落进对方的文件夹，2026-09-24 审计记过）。
/// 锁文件在 `$XDG_RUNTIME_DIR/shelf/`（设备上缺省 `/tmp/shelf-0/shelf/`，tmpfs、重启即清，与服务注册表同目录、
/// 各服务都能写）；`flock` 随文件描述符关闭自动释放，进程崩了也不会留死锁。打不开锁文件（目录不可写）时退回只有
/// 进程内锁（与改动前一致），打一行日志。
pub(super) struct UploadLock {
    _file: Option<std::fs::File>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

static UPLOAD: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl UploadLock {
    pub(super) fn acquire(lock_file: Option<&Path>) -> UploadLock {
        let guard = crate::sync::lock(&UPLOAD);
        let file = lock_file.and_then(|p| match open_and_lock(p) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("[xochitl] 上传锁文件 {} 不可用（{e}），只做进程内互斥", p.display());
                None
            }
        });
        UploadLock { _file: file, _guard: guard }
    }
}

fn open_and_lock(p: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(p)?;
    f.lock()?;
    Ok(f)
}

/// 生产缺省的上传锁文件：书库目录就是本机真实的 xochitl 书库（`Paths::xochitl_dir()`）时用
/// `Paths::runtime_dir()/xochitl-upload.lock`；否则（单测的临时书库、host 上调试）不用锁文件——测试绝不碰开发机真实的运行时目录。
pub(super) fn default_lock_file(library_dir: &Path) -> Option<PathBuf> {
    let paths = crate::paths::Paths::from_env();
    (library_dir == paths.xochitl_dir()).then(|| paths.runtime_dir().join(LOCK_FILE_NAME))
}

/// 锁文件名（在运行时目录下）。
pub const LOCK_FILE_NAME: &str = "xochitl-upload.lock";
