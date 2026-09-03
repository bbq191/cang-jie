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
}

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
    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [self.inbox(), self.work(), self.done(), self.failed()] {
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
    pub fn archive_failed(&self, p: &Path) {
        archive(p, &self.failed());
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
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    if let Ok(md) = e.metadata() {
                        if md.is_file() {
                            out.push(SpoolEntry { name: e.file_name().to_string_lossy().to_string(), bytes: md.len(), state });
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| (a.state, &a.name).cmp(&(b.state, &b.name)));
        out
    }

    /// failed/ → inbox/（重试）。
    pub fn retry(&self, name: &str) -> Result<(), String> {
        let src = self.failed().join(safe(name)?);
        if !src.is_file() {
            return Err("failed/ 里没有这个文件".into());
        }
        archive(&src, &self.inbox());
        Ok(())
    }
    pub fn delete_failed(&self, name: &str) -> Result<(), String> {
        let src = self.failed().join(safe(name)?);
        std::fs::remove_file(&src).map_err(|e| format!("删除失败: {e}"))
    }
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

fn archive(src: &Path, dest_dir: &Path) {
    let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let target = unique(dest_dir, name);
    if std::fs::rename(src, &target).is_err() && std::fs::copy(src, &target).is_ok() {
        let _ = std::fs::remove_file(src);
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
        s.archive_failed(&w);
        assert_eq!(s.list(), vec![SpoolEntry { name: "a.epub".into(), bytes: 1, state: "failed" }]);
        s.retry("a.epub").unwrap();
        assert_eq!(s.list()[0].state, "pending");
        assert!(s.retry("../x").is_err());
        // 同名再入 failed 不覆盖
        let w2 = s.claim("a.epub").unwrap();
        std::fs::write(s.failed().join("a.epub"), b"old").unwrap();
        s.archive_failed(&w2);
        assert_eq!(s.list().len(), 2);
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
