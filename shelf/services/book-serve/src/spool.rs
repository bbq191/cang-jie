//! 自有 spool（`$XDG_STATE_HOME/shelf/books/{inbox,.work,done,failed}`）。
//! 与旧项目 `/home/root/weread/inbox` 无关。语义：
//! - `inbox/`  待处理（scp 丢进来的、或上传接收完成后暂放）——fswatch 追平；
//! - `.work/`  已认领、处理中（rename 原子独占，防并发重复处理）；
//! - `done/`   成功源归档（封顶 100MB，便于重投）；`failed/` 失败源（封顶 50MB，可重试/删除）。
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const DONE_CAP: u64 = 100 * 1024 * 1024;
const FAILED_CAP: u64 = 50 * 1024 * 1024;

pub struct Spool {
    root: PathBuf,
    lock: Mutex<()>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SpoolEntry {
    pub name: String,
    pub bytes: u64,
    pub state: &'static str,
    /// 失败原因（仅 failed 条目，读 `<name>.reason` sidecar）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 母版库（staging）一本书的展示条目。`format` 从扩展名判、`optimized` 从内埋标记判（轻量只读中央目录）。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct StagingEntry {
    pub name: String,
    pub bytes: u64,
    pub format: &'static str,
    pub optimized: bool,
    /// 入库时间（unix 秒），列表最新在前。
    pub mtime: u64,
}

/// 失败原因 sidecar 后缀。
const REASON_EXT: &str = ".reason";

impl Spool {
    pub fn new(root: PathBuf) -> Spool {
        Spool { root, lock: Mutex::new(()) }
    }
    pub fn inbox(&self) -> PathBuf {
        self.root.join("inbox")
    }
    pub fn work(&self) -> PathBuf {
        self.root.join(".work")
    }
    pub fn done(&self) -> PathBuf {
        self.root.join("done")
    }
    pub fn failed(&self) -> PathBuf {
        self.root.join("failed")
    }
    /// 母版库（中间层暂存池）：所有源原样落这里，用户再选优化/落库去向。**不套 done/ 的 LRU 淘汰**。
    /// 与 `shelf_core::Paths::staging_dir()` 同一路径（root=`state_dir()/books`）——koreader-serve adopt 时读同处。
    pub fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [self.inbox(), self.work(), self.done(), self.failed(), self.staging()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    /// 处理临界区（上传/fswatch/重试三条触发串行化）。
    pub fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 认领 inbox 里的文件：rename 进 .work（原子独占）。另一线程已搬走 → None。
    pub fn claim(&self, name: &str) -> Option<PathBuf> {
        let src = self.inbox().join(name);
        let dst = unique(&self.work(), name);
        std::fs::rename(&src, &dst).ok().map(|_| dst)
    }

    /// 上传路：直接在 .work 里开一个独占文件（不经 inbox）。
    pub fn stage(&self, name: &str) -> PathBuf {
        unique(&self.work(), name)
    }

    pub fn archive_done(&self, p: &Path) {
        archive(p, &self.done());
        prune(&self.done(), DONE_CAP);
    }
    /// 归档失败源到 `failed/`，并把 `reason` 写进 `<归档名>.reason` sidecar（供列表展示，重试不再靠猜）。
    pub fn archive_failed(&self, p: &Path, reason: &str) {
        let target = archive(p, &self.failed());
        if let Some(t) = &target {
            if !reason.trim().is_empty() {
                let _ = std::fs::write(reason_path(t), reason.trim());
            }
        }
        prune(&self.failed(), FAILED_CAP);
    }

    /// 崩溃恢复：.work 残留移回 inbox（只在启动时调用）。
    pub fn recover_orphans(&self) -> usize {
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(self.work()) {
            for e in rd.flatten() {
                if e.path().is_file() {
                    archive(&e.path(), &self.inbox());
                    n += 1;
                }
            }
        }
        n
    }

    pub fn list(&self) -> Vec<SpoolEntry> {
        let mut out = Vec::new();
        for (dir, state) in [(self.inbox(), "pending"), (self.work(), "working"), (self.failed(), "failed")] {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let fname = e.file_name().to_string_lossy().to_string();
                    if fname.ends_with(REASON_EXT) {
                        continue; // sidecar 不作为条目
                    }
                    if let Ok(md) = e.metadata() {
                        if md.is_file() {
                            let reason = if state == "failed" { std::fs::read_to_string(reason_path(&e.path())).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) } else { None };
                            out.push(SpoolEntry { name: fname, bytes: md.len(), state, reason });
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| (a.state, &a.name).cmp(&(b.state, &b.name)));
        out
    }

    /// failed/ → inbox/（重试）；连带清掉 `.reason` sidecar。
    pub fn retry(&self, name: &str) -> Result<(), String> {
        let src = self.failed().join(safe(name)?);
        if !src.is_file() {
            return Err("failed/ 里没有这个文件".into());
        }
        let _ = std::fs::remove_file(reason_path(&src));
        archive(&src, &self.inbox());
        Ok(())
    }
    pub fn delete_failed(&self, name: &str) -> Result<(), String> {
        let src = self.failed().join(safe(name)?);
        let _ = std::fs::remove_file(reason_path(&src));
        std::fs::remove_file(&src).map_err(|e| format!("删除失败: {e}"))
    }

    // ───────────────────────── 母版库（staging）操作 ─────────────────────────

    /// 新入库：原样原子写进母版库，同名加数字前缀不覆盖。返回落地文件名（供前端引用）。
    pub fn stage_new(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        let base = safe(name)?;
        let target = unique(&self.staging(), base);
        shelf_core::fs::write_atomic(&target, bytes).map_err(|e| format!("写母版库失败: {e}"))?;
        Ok(target.file_name().and_then(|s| s.to_str()).unwrap_or(base).to_string())
    }

    /// 覆盖写母版库同名文件（优化后回写产物）。原子。
    pub fn overwrite_staging(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let target = self.staging().join(safe(name)?);
        shelf_core::fs::write_atomic(&target, bytes).map_err(|e| format!("回写母版库失败: {e}"))
    }

    /// 母版库某文件的安全路径（校验文件名）。
    pub fn staging_path(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self.staging().join(safe(name)?))
    }

    pub fn read_staging(&self, name: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.staging_path(name)?).map_err(|e| format!("读母版库文件失败: {e}"))
    }

    pub fn remove_staging(&self, name: &str) -> Result<(), String> {
        std::fs::remove_file(self.staging_path(name)?).map_err(|e| format!("删除失败: {e}"))
    }

    /// 列母版库：格式（epub/pdf/other）+ 是否带优化标记（`optimized_version_file` 轻量只读中央目录）。
    pub fn list_staging(&self) -> Vec<StagingEntry> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(self.staging()) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue; // .part 半成品 / 隐藏文件不列
                }
                let Ok(md) = e.metadata() else { continue };
                if !md.is_file() {
                    continue;
                }
                let lower = name.to_ascii_lowercase();
                let format = if lower.ends_with(".epub") {
                    "epub"
                } else if lower.ends_with(".pdf") {
                    "pdf"
                } else {
                    "other"
                };
                // 优化状态只对 EPUB 有意义（PDF/其它格式端上不优化）。
                let optimized = format == "epub" && e.path().to_str().map(|p| bookconv::optimize::optimized_version_file(p).is_some()).unwrap_or(false);
                let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                out.push(StagingEntry { name, bytes: md.len(), format, optimized, mtime });
            }
        }
        // 最新入库在前（同秒按名）。
        out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.name.cmp(&b.name)));
        out
    }
}

/// `<文件>.reason` sidecar 路径。
fn reason_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(REASON_EXT);
    PathBuf::from(s)
}

fn safe(name: &str) -> Result<&str, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("非法文件名".into());
    }
    Ok(name)
}

fn unique(dir: &Path, name: &str) -> PathBuf {
    let mut t = dir.join(name);
    let mut n = 1;
    while t.exists() {
        t = dir.join(format!("{n}_{name}"));
        n += 1;
    }
    t
}

/// 归档到 `dest_dir`（rename，跨设备回退 copy+rm）。返回落地后的唯一路径（失败 None）。
fn archive(src: &Path, dest_dir: &Path) -> Option<PathBuf> {
    let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let target = unique(dest_dir, name);
    if std::fs::rename(src, &target).is_ok() {
        Some(target)
    } else if std::fs::copy(src, &target).is_ok() {
        let _ = std::fs::remove_file(src);
        Some(target)
    } else {
        None
    }
}

fn prune(dir: &Path, cap: u64) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    let mut total = 0u64;
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        total += md.len();
        files.push((e.path(), md.len(), md.modified().unwrap_or(std::time::UNIX_EPOCH)));
    }
    if total <= cap {
        return;
    }
    files.sort_by_key(|f| f.2);
    for (p, sz, _) in files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total = total.saturating_sub(sz);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_archive_retry_roundtrip() {
        let t = tempfile::tempdir().unwrap();
        let s = Spool::new(t.path().join("books"));
        s.ensure().unwrap();
        std::fs::write(s.inbox().join("a.epub"), b"x").unwrap();
        let w = s.claim("a.epub").unwrap();
        assert!(w.starts_with(s.work()));
        assert!(s.claim("a.epub").is_none(), "二次认领必须失败");
        s.archive_failed(&w, "质量门未过：双 id");
        let listed = s.list();
        assert_eq!(listed, vec![SpoolEntry { name: "a.epub".into(), bytes: 1, state: "failed", reason: Some("质量门未过：双 id".into()) }]);
        s.retry("a.epub").unwrap();
        assert_eq!(s.list()[0].state, "pending");
        assert!(s.list()[0].reason.is_none(), "重试后原因清掉");
        assert!(!reason_path(&s.failed().join("a.epub")).exists(), "reason sidecar 已删");
        assert!(s.retry("../x").is_err());
        // 同名再入 failed 不覆盖，reason 跟着归档名走
        let w2 = s.claim("a.epub").unwrap();
        std::fs::write(s.failed().join("a.epub"), b"old").unwrap();
        s.archive_failed(&w2, "转换失败");
        let l = s.list();
        assert_eq!(l.len(), 2);
        assert!(l.iter().any(|e| e.name == "1_a.epub" && e.reason.as_deref() == Some("转换失败")), "{l:?}");
    }

    #[test]
    fn staging_put_list_read_remove() {
        let t = tempfile::tempdir().unwrap();
        let s = Spool::new(t.path().join("books"));
        s.ensure().unwrap();
        assert_eq!(s.stage_new("a.epub", b"not-a-zip").unwrap(), "a.epub");
        assert_eq!(s.stage_new("a.epub", b"xx").unwrap(), "1_a.epub", "同名不覆盖");
        s.stage_new("doc.pdf", b"%PDF").unwrap();
        let list = s.list_staging();
        assert_eq!(list.len(), 3);
        let epub = list.iter().find(|e| e.name == "a.epub").unwrap();
        assert_eq!(epub.format, "epub");
        assert!(!epub.optimized, "非 zip 不该判已优化");
        assert_eq!(list.iter().find(|e| e.name == "doc.pdf").unwrap().format, "pdf");
        assert_eq!(s.read_staging("a.epub").unwrap(), b"not-a-zip");
        s.overwrite_staging("a.epub", b"new").unwrap();
        assert_eq!(s.read_staging("a.epub").unwrap(), b"new");
        s.remove_staging("a.epub").unwrap();
        assert_eq!(s.list_staging().len(), 2);
        assert!(s.stage_new("../x", b"y").is_err(), "非法名拒绝");
    }

    #[test]
    fn recover_orphans_moves_work_back() {
        let t = tempfile::tempdir().unwrap();
        let s = Spool::new(t.path().to_path_buf());
        s.ensure().unwrap();
        std::fs::write(s.work().join("half.azw3"), b"x").unwrap();
        assert_eq!(s.recover_orphans(), 1);
        assert!(s.inbox().join("half.azw3").is_file());
    }

    #[test]
    fn prune_keeps_newest_under_cap() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().to_path_buf();
        for (i, n) in ["old", "mid", "new"].iter().enumerate() {
            std::fs::write(d.join(n), vec![0u8; 10]).unwrap();
            let ft = std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i as u64));
            std::fs::File::options().write(true).open(d.join(n)).unwrap().set_times(ft).unwrap();
        }
        prune(&d, 20);
        assert!(!d.join("old").exists() && d.join("mid").exists() && d.join("new").exists());
    }
}
