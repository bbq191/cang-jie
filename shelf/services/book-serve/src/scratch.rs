//! 临时文件：名字 `<前缀><pid>.<序号>.<后缀>` 与书名无关，Drop 时删掉。母版库入库中转（`staging`）和直接导入的请求体
//! （`import`）共用（此前两边各写一份守卫和"pid + 静态序号"的取名）。
//!
//! - **名字与书名无关**：按书名拼的话，书名本身接近文件名 255 字节上限（中文 80 来个字）时临时文件名超长，
//!   直接报文件系统错误（ENAMETOOLONG）。
//! - **Drop 时删掉**：正常路径下文件早已被 rename 成正式文件（删不到，无害）；出错或 panic 时（后台线程 `catch_unwind`
//!   兜住、进程照常服务）不把半成品（大书可达数百 MB）一直留到下次启动才清。进程被杀时 Drop 来不及跑，由各自启动时的清理兜底。
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct ScratchFile(PathBuf);

impl ScratchFile {
    /// 在 `dir` 里取一个新名字（不建文件）：`<prefix><pid>.<序号>.<suffix>`。
    pub fn new(dir: &Path, prefix: &str, suffix: &str) -> ScratchFile {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        ScratchFile(dir.join(format!("{prefix}{}.{seq}.{suffix}", std::process::id())))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
