//! 用量账本 `~/.local/state/notes/mind.json`：调用/成功/失败次数、token 累计、最近一次错误。
//! 只记数不记内容（不存 key、不存问题/回答文本）。没有 transcribe-serve 那个 `last_run`（一轮批量的报告）——
//! mind-serve 没有批量轮次，每次调用都是独立的一问一答。
//! **第二轮整理区反馈（2026-09-08，点 2）**：按模型分账（`by_model`，见 `transcribe-serve::ledger` 同样的
//! 设计理由）。
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelUsage {
    pub calls: u64,
    pub ok: u64,
    pub failed: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub last_at: u64,
    pub last_error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Usage {
    pub by_model: BTreeMap<String, ModelUsage>,
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
    pub fn record_ok(&self, model_key: &str, prompt_tokens: u64, completion_tokens: u64, now: u64) {
        self.edit(|u| {
            let m = u.by_model.entry(model_key.to_string()).or_default();
            m.calls += 1;
            m.ok += 1;
            m.prompt_tokens += prompt_tokens;
            m.completion_tokens += completion_tokens;
            m.last_at = now;
        });
    }
    pub fn record_fail(&self, model_key: &str, err: &str, now: u64) {
        self.edit(|u| {
            let m = u.by_model.entry(model_key.to_string()).or_default();
            m.calls += 1;
            m.failed += 1;
            m.last_at = now;
            m.last_error = err.chars().take(200).collect();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_counts_per_model() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("mind.json");
        let l = Ledger::open(&p);
        l.record_ok("qwen-plus", 80, 20, 1);
        l.record_fail("qwen-plus", "HTTP 401：bad key", 2);
        l.record_ok("deepseek-v4-flash", 30, 5, 3);
        let back = Ledger::open(&p).snapshot();
        let qwen = &back.by_model["qwen-plus"];
        assert_eq!((qwen.calls, qwen.ok, qwen.failed, qwen.prompt_tokens, qwen.completion_tokens), (2, 1, 1, 80, 20));
        assert_eq!(qwen.last_error, "HTTP 401：bad key");
        assert_eq!(qwen.last_at, 2);
        assert_eq!(back.by_model["deepseek-v4-flash"].calls, 1, "不同模型各算各的");
    }
}
