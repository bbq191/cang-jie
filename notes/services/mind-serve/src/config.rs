//! 配置 `~/.config/notes/mind.json`（0600：含 key）。key 三来源按优先级：文件 `apiKey` → 环境 `DASHSCOPE_API_KEY` → 无
//! （跟 transcribe-serve 共用同一个环境变量名——同一个 DashScope 账号的 key，视觉/文字两个模型都认，图省事）。
//! 对外（GET /config）永远只报 `hasKey`/`keySource`，**不回显 key**；写入走 PUT /config 的 `apiKey` 字段（空串=不改，`clearKey`=清）。
//! **没有 `maxPerRun`/`pauseMs`/`auto`/`maxAttempts` 这些节流字段**——mind-serve 不跑批量循环，纯粹是"问一条答一条"，
//! 见 `worker.rs`/`main.rs` 文档：耗电评估已经算过这条（笔记线白皮书 §03n 第 5 点），比 transcribe-serve 还轻。
use serde::{Deserialize, Serialize};

pub const KEY_ENV: &str = "DASHSCOPE_API_KEY";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MindConfig {
    /// 后端标识（写进 `Answer.backend`）。
    pub backend: String,
    /// OpenAI 兼容口根地址（DashScope / 任何兼容服务）。
    pub base_url: String,
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// 单次请求超时（秒）。
    pub timeout_secs: u64,
    /// 自定义提示词前缀（空 = 内置）。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prompt: String,
}

impl Default for MindConfig {
    fn default() -> Self {
        MindConfig {
            backend: "qwen".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            model: "qwen-plus".into(),
            api_key: String::new(),
            timeout_secs: 60,
            prompt: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Config,
    Env,
    None,
}

impl MindConfig {
    /// 解析出可用的 key（不打印、不落日志）。
    pub fn key(&self) -> Option<String> {
        self.key_with_env(std::env::var(KEY_ENV).ok())
    }
    pub fn key_with_env(&self, env: Option<String>) -> Option<String> {
        let k = self.api_key.trim();
        if !k.is_empty() {
            return Some(k.to_string());
        }
        env.map(|e| e.trim().to_string()).filter(|e| !e.is_empty())
    }
    pub fn key_source(&self) -> KeySource {
        if !self.api_key.trim().is_empty() {
            KeySource::Config
        } else if std::env::var(KEY_ENV).map(|e| !e.trim().is_empty()).unwrap_or(false) {
            KeySource::Env
        } else {
            KeySource::None
        }
    }
    /// 脱敏预览：只回最后 4 位（如 `...ab12`），服务端算，绝不整串回显（二期步骤 5）。
    pub fn key_masked(&self) -> Option<String> {
        let k = self.key()?;
        let n = k.chars().count();
        if n <= 4 {
            return Some("*".repeat(n));
        }
        let tail: String = k.chars().skip(n - 4).collect();
        Some(format!("...{tail}"))
    }
    /// 对外视图：去 key、加 hasKey/keySource/keyMasked。
    pub fn public(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(MindConfig { api_key: String::new(), ..self.clone() }).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.insert("hasKey".into(), serde_json::Value::Bool(self.key().is_some()));
            o.insert("keySource".into(), serde_json::to_value(self.key_source()).unwrap_or_default());
            o.insert("keyMasked".into(), serde_json::to_value(self.key_masked()).unwrap_or(serde_json::Value::Null));
        }
        v
    }
    /// 套用 PUT /config 的 JSON：可改字段逐个覆盖；`apiKey` 非空才改；`clearKey:true` 清 key。
    pub fn apply(&mut self, j: &serde_json::Value) -> Result<(), String> {
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        if let Some(v) = s("backend") { self.backend = v; }
        if let Some(v) = s("baseUrl") {
            if !v.starts_with("http://") && !v.starts_with("https://") {
                return Err("baseUrl 要以 http(s):// 开头".into());
            }
            self.base_url = v.trim_end_matches('/').to_string();
        }
        if let Some(v) = s("model") { self.model = v; }
        if let Some(v) = s("apiKey") { self.api_key = v; }
        if j.get("clearKey").and_then(|v| v.as_bool()).unwrap_or(false) { self.api_key.clear(); }
        if let Some(v) = j.get("timeoutSecs").and_then(|v| v.as_u64()) { self.timeout_secs = v.clamp(5, 600); }
        if let Some(v) = j.get("prompt") {
            self.prompt = v.as_str().unwrap_or("").trim().to_string();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_priority_and_masking() {
        let mut c = MindConfig::default();
        assert_eq!(c.key_with_env(None), None);
        assert_eq!(c.key_with_env(Some(" env-k ".into())).as_deref(), Some("env-k"));
        c.api_key = "file-k".into();
        assert_eq!(c.key_with_env(Some("env-k".into())).as_deref(), Some("file-k"), "文件优先");
        let p = c.public();
        assert!(p.get("apiKey").is_none(), "对外不回显 key: {p}");
        assert_eq!(p["hasKey"], true);
        assert_eq!(p["model"], "qwen-plus");
        assert_eq!(p["keyMasked"], "...le-k", "只露最后 4 位，不整串回显");
        c.api_key.clear();
        assert_eq!(c.public()["keyMasked"], serde_json::Value::Null, "没 key 时是 null");
    }

    #[test]
    fn apply_updates_only_given_fields() {
        let mut c = MindConfig::default();
        c.apply(&serde_json::json!({"apiKey": "k1", "model": "m2", "baseUrl": "https://x.example/v1/"})).unwrap();
        assert_eq!((c.api_key.as_str(), c.model.as_str(), c.base_url.as_str()), ("k1", "m2", "https://x.example/v1"));
        c.apply(&serde_json::json!({"apiKey": ""})).unwrap();
        assert_eq!(c.api_key, "k1", "空 apiKey 不改");
        c.apply(&serde_json::json!({"clearKey": true})).unwrap();
        assert!(c.api_key.is_empty());
        assert!(c.apply(&serde_json::json!({"baseUrl": "dashscope"})).is_err());
        assert_eq!(c.timeout_secs, 60, "没给的字段不动");
    }
}
