//! 端化 cardhw 的两个**旁路输出文件**（Phase C，daemon 写、设置面板/注入 QML 读）：
//!   - **usage 统计**（`cardhw-usage.json`）：按 provider 累计调云次数 + token 消耗，设置面板展示。
//!   - **status 状态**（`cardhw-status.json`）：一次处理的「处理中/完成/失败」+ 单调 seq，
//!     供注入 QML 观察器轮询 → `showNotification`（通知桥）。daemon（Rust）够不到 QML，靠文件桥接。
//!
//! 两文件与 `reading-qol.json` 同目录（`~/.local/share/cangjie-ime/`）。纯 read-modify-write JSON，
//! 累计逻辑（`merge_usage`）抽成纯函数 host 可测；IO 包装容错（写失败只忽略，不阻塞主流程）。

use device_core::vision::Usage;
use serde_json::{json, Value};
use std::path::Path;

/// 把一次调用的 token 消耗并进既有统计 JSON（不落盘，纯函数）。
/// 结构：`{ "<provider>": {"calls":N,"input_tokens":N,"output_tokens":N}, ... }`。
/// 按 provider 聚合（model 会漂移、对成本展示是噪声）；缺字段按 0 起算。
pub fn merge_usage(existing: &Value, provider: &str, u: Usage) -> Value {
    let mut root = existing.as_object().cloned().unwrap_or_default();
    let cur = root.get(provider).cloned().unwrap_or(Value::Null);
    let g = |k: &str| cur.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    root.insert(provider.to_string(), json!({
        "calls": g("calls") + 1,
        "input_tokens": g("input_tokens") + u.input_tokens,
        "output_tokens": g("output_tokens") + u.output_tokens,
    }));
    Value::Object(root)
}

/// 读统计文件 → 并进本次 usage → 写回。文件不存在/损坏按空起算。写失败只忽略（统计非关键路径）。
/// usage 全零（无手写页、没真调云）时跳过——不虚增 calls。
pub fn accumulate_usage(path: &Path, provider: &str, u: Usage) {
    if u.input_tokens == 0 && u.output_tokens == 0 {
        return;
    }
    let existing: Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let merged = merge_usage(&existing, provider, u);
    let _ = std::fs::write(path, serde_json::to_string_pretty(&merged).unwrap_or_default());
}

/// 写一次处理状态（通知桥）。`seq` 单调自增：读旧文件取其 seq+1，让 QML 观察器即使 state/book 相同
/// 也能凭 seq 变化认出「这是一条新通知」。写失败只忽略（通知非关键路径）。
/// state：`"processing"`（开工）/`"done"`（注入并 /upload）/`"failed"`（调云或上传出错）/`"idle"`。
pub fn write_status(path: &Path, state: &str, book: &str, count: usize, detail: &str) {
    let seq = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["seq"].as_u64())
        .unwrap_or(0)
        + 1;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let v = json!({ "state": state, "book": book, "count": count, "detail": detail, "seq": seq, "ts": ts });
    let _ = std::fs::write(path, v.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(i: u64, o: u64) -> Usage {
        Usage { input_tokens: i, output_tokens: o }
    }

    #[test]
    fn merge_from_empty() {
        let m = merge_usage(&Value::Null, "deepseek", u(384, 42));
        assert_eq!(m["deepseek"]["calls"], 1);
        assert_eq!(m["deepseek"]["input_tokens"], 384);
        assert_eq!(m["deepseek"]["output_tokens"], 42);
    }

    #[test]
    fn merge_accumulates_same_provider() {
        let m1 = merge_usage(&Value::Null, "deepseek", u(384, 42));
        let m2 = merge_usage(&m1, "deepseek", u(300, 10));
        assert_eq!(m2["deepseek"]["calls"], 2);
        assert_eq!(m2["deepseek"]["input_tokens"], 684);
        assert_eq!(m2["deepseek"]["output_tokens"], 52);
    }

    #[test]
    fn merge_keeps_providers_separate() {
        let m1 = merge_usage(&Value::Null, "deepseek", u(384, 42));
        let m2 = merge_usage(&m1, "gemini", u(500, 30));
        assert_eq!(m2["deepseek"]["calls"], 1);
        assert_eq!(m2["gemini"]["calls"], 1);
        assert_eq!(m2["gemini"]["input_tokens"], 500);
    }

    #[test]
    fn accumulate_skips_zero_usage() {
        let dir = std::env::temp_dir().join(format!("cardhw-usage-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("usage.json");
        accumulate_usage(&p, "deepseek", u(0, 0));
        assert!(!p.exists(), "全零 usage 不建文件");
        accumulate_usage(&p, "deepseek", u(100, 5));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["deepseek"]["calls"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_seq_increments() {
        let dir = std::env::temp_dir().join(format!("cardhw-status-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("status.json");
        write_status(&p, "processing", "人骨拼图", 0, "");
        let v1: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v1["seq"], 1);
        assert_eq!(v1["state"], "processing");
        write_status(&p, "done", "人骨拼图", 2, "");
        let v2: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v2["seq"], 2);
        assert_eq!(v2["state"], "done");
        assert_eq!(v2["count"], 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
