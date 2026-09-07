//! 用量账本 `~/.local/state/notes/mind.json`：调用/成功/失败次数、token 累计、最近一次错误。
//! 只记数不记内容（不存 key、不存问题/回答文本）。没有 transcribe-serve 那个 `last_run`（一轮批量的报告）——
//! mind-serve 没有批量轮次，每次调用都是独立的一问一答。
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_counts() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("mind.json");
        let l = Ledger::open(&p);
        l.record_ok(80, 20, 1);
        l.record_fail("HTTP 401：bad key", 2);
        let back = Ledger::open(&p).snapshot();
        assert_eq!((back.calls, back.ok, back.failed, back.prompt_tokens, back.completion_tokens), (2, 1, 1, 80, 20));
        assert_eq!(back.last_error, "HTTP 401：bad key");
        assert_eq!(back.last_at, 2);
    }
}
