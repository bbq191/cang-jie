//! 用量账本 `~/.local/state/notes/transcribe.json`：调用/成功/失败次数、token 累计、最近一次错误与一轮报告。
//! 只记数不记内容（不存 key、不存转写文本）。
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Usage {
    pub calls: u64,
    pub ok: u64,
    pub failed: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub last_at: u64,
    pub last_error: String,
    pub last_run: Option<RunReport>,
}

/// 一轮转写的结果（网页状态区显示）。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunReport {
    pub at: u64,
    pub scanned: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    pub left: usize,
    pub note: String,
}

pub struct Ledger {
    path: PathBuf,
    usage: Mutex<Usage>,
}

impl Ledger {
    pub fn open(path: &Path) -> Ledger {
        Ledger { path: path.to_path_buf(), usage: Mutex::new(shelf_core::config::load_or_default(path)) }
    }
    pub fn snapshot(&self) -> Usage {
        self.usage.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn edit(&self, f: impl FnOnce(&mut Usage)) {
        let mut u = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut u);
        let _ = shelf_core::config::save(&self.path, &*u, None);
    }
    pub fn record_ok(&self, prompt_tokens: u64, completion_tokens: u64, now: u64) {
        self.edit(|u| {
            u.calls += 1;
            u.ok += 1;
            u.prompt_tokens += prompt_tokens;
            u.completion_tokens += completion_tokens;
            u.last_at = now;
        });
    }
    pub fn record_fail(&self, err: &str, now: u64) {
        self.edit(|u| {
            u.calls += 1;
            u.failed += 1;
            u.last_at = now;
            u.last_error = err.chars().take(200).collect();
        });
    }
    pub fn record_run(&self, r: RunReport) {
        self.edit(|u| u.last_run = Some(r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_counts() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("transcribe.json");
        let l = Ledger::open(&p);
        l.record_ok(100, 5, 1);
        l.record_fail("HTTP 401：bad key", 2);
        l.record_run(RunReport { at: 2, scanned: 2, done: 1, failed: 1, ..Default::default() });
        let back = Ledger::open(&p).snapshot();
        assert_eq!((back.calls, back.ok, back.failed, back.prompt_tokens, back.completion_tokens), (2, 1, 1, 100, 5));
        assert_eq!(back.last_error, "HTTP 401：bad key");
        assert_eq!(back.last_run.unwrap().done, 1);
    }
}
