//! 用量账本 `~/.local/state/notes/transcribe.json`：调用/成功/失败次数、token 累计、最近一次错误与一轮报告。
//! 只记数不记内容（不存 key、不存转写文本）。
//! **第二轮整理区反馈（2026-09-08，点 2）**：按模型分账（`by_model`，键是 `TranscribeConfig::usage_key()`——
//! 预置 id 或 `custom:<model>`）——同一个服务现在能在多家厂商之间切换预置，"各个模型的用量花费 profile"
//! 要求每个用过的模型各算各的，不能只有一份全局聚合数字（不然切个模型历史用量就混一起分不清了）。
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
    /// 这一轮成功调用累计花的 token（点「重转」弹出消耗要用，2026-09-08 第三轮反馈）——强制单条时
    /// 这轮只有一次成功调用，这两个数就是那一次调用的实际消耗；批量跑一轮时是整轮的累计。
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Usage {
    pub by_model: BTreeMap<String, ModelUsage>,
    pub last_run: Option<RunReport>,
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
    pub fn record_run(&self, r: RunReport) {
        self.edit(|u| u.last_run = Some(r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_counts_per_model() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("transcribe.json");
        let l = Ledger::open(&p);
        l.record_ok("qwen3-vl-plus", 100, 5, 1);
        l.record_fail("qwen3-vl-plus", "HTTP 401：bad key", 2);
        l.record_ok("gpt-5.6-terra", 50, 10, 3);
        l.record_run(RunReport { at: 2, scanned: 2, done: 1, failed: 1, ..Default::default() });
        let back = Ledger::open(&p).snapshot();
        let qwen = &back.by_model["qwen3-vl-plus"];
        assert_eq!((qwen.calls, qwen.ok, qwen.failed, qwen.prompt_tokens, qwen.completion_tokens), (2, 1, 1, 100, 5));
        assert_eq!(qwen.last_error, "HTTP 401：bad key");
        let gpt = &back.by_model["gpt-5.6-terra"];
        assert_eq!((gpt.calls, gpt.ok, gpt.prompt_tokens), (1, 1, 50), "不同模型各算各的，不会混到一起");
        assert_eq!(back.last_run.unwrap().done, 1);
    }
}
