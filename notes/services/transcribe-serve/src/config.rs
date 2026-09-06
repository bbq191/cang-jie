//! 配置 `~/.config/notes/transcribe.json`（0600：含 key）。key 三来源按优先级：文件 `apiKey` → 环境 `DASHSCOPE_API_KEY` → 无。
//! 对外（GET /config）永远只报 `hasKey`/`keySource`，**不回显 key**；写入走 PUT /config 的 `apiKey` 字段（空串=不改，`clearKey`=清）。
use serde::{Deserialize, Serialize};

pub const KEY_ENV: &str = "DASHSCOPE_API_KEY";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TranscribeConfig {
    /// 后端标识（写进草稿 `backend` 字段，便于区分不同模型的建议）。
    pub backend: String,
    /// OpenAI 兼容口根地址（DashScope / 任何兼容服务）。
    pub base_url: String,
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// 单次请求超时（秒）。设备走自己的 WiFi 直连，国内 API 通常 5–15 s。
    pub timeout_secs: u64,
    /// 一轮最多转写多少条（防一次合书几十条把费用打爆；剩下的下一轮接着来）。
    pub max_per_run: usize,
    /// 两次请求间歇（毫秒），给限流留余量。
    pub pause_ms: u64,
    /// 条目库有新条目时自动转写；关掉则只在网页点「转写」时跑。
    pub auto: bool,
    /// 同一条目（同指纹）失败几次后不再自动重试（网页可手动重来）。
    pub max_attempts: u32,
    /// 自定义提示词（空 = 内置）。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prompt: String,
}

impl Default for TranscribeConfig {
    fn default() -> Self {
        TranscribeConfig {
            backend: "qwen".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            model: "qwen3-vl-plus".into(),
            api_key: String::new(),
            timeout_secs: 60,
            max_per_run: 20,
            pause_ms: 300,
            auto: true,
            max_attempts: 3,
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

impl TranscribeConfig {
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
    /// 对外视图：去 key、加 hasKey/keySource。
    pub fn public(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(TranscribeConfig { api_key: String::new(), ..self.clone() }).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.insert("hasKey".into(), serde_json::Value::Bool(self.key().is_some()));
            o.insert("keySource".into(), serde_json::to_value(self.key_source()).unwrap_or_default());
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
        if let Some(v) = j.get("maxPerRun").and_then(|v| v.as_u64()) { self.max_per_run = (v as usize).clamp(1, 500); }
        if let Some(v) = j.get("pauseMs").and_then(|v| v.as_u64()) { self.pause_ms = v.min(60_000); }
        if let Some(v) = j.get("auto").and_then(|v| v.as_bool()) { self.auto = v; }
        if let Some(v) = j.get("maxAttempts").and_then(|v| v.as_u64()) { self.max_attempts = (v as u32).clamp(1, 20); }
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
        let mut c = TranscribeConfig::default();
        assert_eq!(c.key_with_env(None), None);
        assert_eq!(c.key_with_env(Some(" env-k ".into())).as_deref(), Some("env-k"));
        c.api_key = "file-k".into();
        assert_eq!(c.key_with_env(Some("env-k".into())).as_deref(), Some("file-k"), "文件优先");
        let p = c.public();
        assert!(p.get("apiKey").is_none(), "对外不回显 key: {p}");
        assert_eq!(p["hasKey"], true);
        assert_eq!(p["model"], "qwen3-vl-plus");
    }

    #[test]
    fn apply_updates_only_given_fields() {
        let mut c = TranscribeConfig::default();
        c.apply(&serde_json::json!({"apiKey": "k1", "maxPerRun": 5, "baseUrl": "https://x.example/v1/"})).unwrap();
        assert_eq!((c.api_key.as_str(), c.max_per_run, c.base_url.as_str()), ("k1", 5, "https://x.example/v1"));
        c.apply(&serde_json::json!({"apiKey": "", "model": "m2"})).unwrap();
        assert_eq!(c.api_key, "k1", "空 apiKey 不改");
        assert_eq!(c.model, "m2");
        c.apply(&serde_json::json!({"clearKey": true})).unwrap();
        assert!(c.api_key.is_empty());
        assert!(c.apply(&serde_json::json!({"baseUrl": "dashscope"})).is_err());
        assert_eq!(c.timeout_secs, 60, "没给的字段不动");
    }
}
