//! 配置 `~/.config/notes/mind.json`（0600：含 key）。
//! **第二轮整理区反馈（2026-09-08，点 2「模型管理彻底重做」）**：一个服务不再只认一家厂商——预置表现在
//! 横跨 DashScope/OpenAI/Gemini/DeepSeek 四家，切换预置只是换"这次用哪个模型"，key 按厂商（`provider`）
//! 分开存（`keys` 表），不会出现"切到 OpenAI 却把 DashScope 的 key 发过去"这种事，也不用每切一次模型
//! 就重新粘贴 key。`custom` 转义阀单独占一格 key。跟 `transcribe-serve::config` 是同一套设计，这边是
//! 文字模型表，跟视觉模型表分开维护——模型 id 核实来源/豆包为什么不进预置表/花费为什么不做官方定价表，
//! 见那边的模块文档，理由完全一样，不重复写。
//! **没有 `maxPerRun`/`pauseMs`/`auto`/`maxAttempts` 这些节流字段**——mind-serve 不跑批量循环，纯粹是
//! "问一条答一条"，见 `worker.rs`/`main.rs` 文档。
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY_ENV: &str = "DASHSCOPE_API_KEY";

const DASHSCOPE: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const OPENAI: &str = "https://api.openai.com/v1";
const GEMINI: &str = "https://generativelanguage.googleapis.com/v1beta/openai/";
const DEEPSEEK: &str = "https://api.deepseek.com/v1";

/// 一个预置模型选项：网页下拉给的都是"已知能用"的组合，不需要用户自己填 baseUrl。`provider` 决定这条
/// 预置的 key 存哪一格——同厂商换模型不用重新粘贴 key。
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub model: &'static str,
    pub base_url: &'static str,
    pub provider: &'static str,
}

/// 文字模型预置表（换厂商/加型号在这加一行，网页自动出现新选项；核实来源见 `transcribe-serve::config`）。
pub const PRESETS: &[Preset] = &[
    Preset { id: "qwen-plus", label: "Qwen-Plus（推荐）", model: "qwen-plus", base_url: DASHSCOPE, provider: "dashscope" },
    Preset { id: "qwen-max", label: "Qwen-Max（更强，更贵）", model: "qwen-max", base_url: DASHSCOPE, provider: "dashscope" },
    Preset { id: "qwen-turbo", label: "Qwen-Turbo（更快更便宜）", model: "qwen-turbo", base_url: DASHSCOPE, provider: "dashscope" },
    Preset { id: "gpt-5.6-luna", label: "GPT-5.6 Luna（OpenAI，便宜量大）", model: "gpt-5.6-luna", base_url: OPENAI, provider: "openai" },
    Preset { id: "gpt-5.6-terra", label: "GPT-5.6 Terra（OpenAI，性价比）", model: "gpt-5.6-terra", base_url: OPENAI, provider: "openai" },
    Preset { id: "gemini-3.8-flash", label: "Gemini 3.8 Flash（Google）", model: "gemini-3.8-flash", base_url: GEMINI, provider: "gemini" },
    Preset { id: "deepseek-v4-flash", label: "DeepSeek V4 Flash（快省）", model: "deepseek-v4-flash", base_url: DEEPSEEK, provider: "deepseek" },
    Preset { id: "deepseek-v4-pro", label: "DeepSeek V4 Pro（更强）", model: "deepseek-v4-pro", base_url: DEEPSEEK, provider: "deepseek" },
];

/// 用户自填的每千 token 单价（缺省都是 0＝不计费）。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Price {
    pub input_per1k: f64,
    pub output_per1k: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MindConfig {
    /// 后端标识（写进 `Answer.backend`）。
    pub backend: String,
    /// 当前选中的预置 id；`"custom"` 走下面两个手填字段。
    pub preset: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_base_url: String,
    /// 厂商（`Preset.provider`，或 `"custom"`）→ key。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
    /// 预置 id（或 `"custom"`）→ 用户自填单价。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub prices: BTreeMap<String, Price>,
    /// 单次请求超时（秒）。
    pub timeout_secs: u64,
    /// 自定义提示词前缀（空 = 内置）。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prompt: String,

    /// 迁移专用（见 `transcribe-serve::config` 同名字段的说明，只在反序列化时读一次）。
    #[serde(skip_serializing)]
    pub model: String,
    #[serde(skip_serializing)]
    pub base_url: String,
    #[serde(skip_serializing)]
    pub api_key: String,
}

impl Default for MindConfig {
    fn default() -> Self {
        MindConfig {
            backend: "qwen".into(),
            preset: PRESETS[0].id.to_string(),
            custom_model: String::new(),
            custom_base_url: String::new(),
            keys: BTreeMap::new(),
            prices: BTreeMap::new(),
            timeout_secs: 60,
            prompt: String::new(),
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
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
    /// 老配置文件搬进新形状——只在启动加载时调用一次，见 `transcribe-serve::config::migrate` 的说明。
    pub fn migrate(mut self) -> Self {
        if !self.keys.is_empty() {
            return self;
        }
        if self.api_key.is_empty() && self.model.is_empty() && self.base_url.is_empty() {
            return self;
        }
        let matched = PRESETS.iter().find(|p| p.model == self.model && p.base_url == self.base_url);
        match matched {
            Some(p) => self.preset = p.id.to_string(),
            None => {
                self.preset = "custom".to_string();
                self.custom_model = self.model.clone();
                self.custom_base_url = self.base_url.clone();
            }
        }
        if !self.api_key.is_empty() {
            let provider = matched.map(|p| p.provider).unwrap_or("custom");
            self.keys.insert(provider.to_string(), self.api_key.clone());
        }
        self
    }
    fn active(&self) -> Option<&'static Preset> {
        PRESETS.iter().find(|p| p.id == self.preset)
    }
    pub fn provider(&self) -> &str {
        self.active().map(|p| p.provider).unwrap_or("custom")
    }
    pub fn model(&self) -> &str {
        self.active().map(|p| p.model).unwrap_or(self.custom_model.as_str())
    }
    pub fn base_url(&self) -> &str {
        self.active().map(|p| p.base_url).unwrap_or(self.custom_base_url.as_str())
    }
    /// 解析出可用的 key（不打印、不落日志）。
    pub fn key(&self) -> Option<String> {
        self.key_with_env(std::env::var(KEY_ENV).ok())
    }
    pub fn key_with_env(&self, env: Option<String>) -> Option<String> {
        let provider = self.provider();
        if let Some(k) = self.keys.get(provider).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(k.to_string());
        }
        if provider == "dashscope" {
            return env.map(|e| e.trim().to_string()).filter(|e| !e.is_empty());
        }
        None
    }
    pub fn key_source(&self) -> KeySource {
        let provider = self.provider();
        if self.keys.get(provider).map(|k| !k.trim().is_empty()).unwrap_or(false) {
            KeySource::Config
        } else if provider == "dashscope" && std::env::var(KEY_ENV).map(|e| !e.trim().is_empty()).unwrap_or(false) {
            KeySource::Env
        } else {
            KeySource::None
        }
    }
    /// 脱敏预览：只回最后 4 位（如 `...ab12`），服务端算，绝不整串回显。
    pub fn key_masked(&self) -> Option<String> {
        let k = self.key()?;
        let n = k.chars().count();
        if n <= 4 {
            return Some("*".repeat(n));
        }
        let tail: String = k.chars().skip(n - 4).collect();
        Some(format!("...{tail}"))
    }
    pub fn price(&self) -> Price {
        self.prices.get(&self.preset).copied().unwrap_or_default()
    }
    /// 用量记账的分组键（同 `transcribe-serve::config::usage_key`）。
    pub fn usage_key(&self) -> String {
        if self.preset == "custom" { format!("custom:{}", self.custom_model) } else { self.preset.clone() }
    }
    /// 对外视图：去 key、加 hasKey/keySource/keyMasked/presets/activePreset/model/baseUrl/price。
    pub fn public(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.remove("keys");
            o.insert("model".into(), serde_json::Value::String(self.model().to_string()));
            o.insert("baseUrl".into(), serde_json::Value::String(self.base_url().to_string()));
            o.insert("provider".into(), serde_json::Value::String(self.provider().to_string()));
            o.insert("hasKey".into(), serde_json::Value::Bool(self.key().is_some()));
            o.insert("keySource".into(), serde_json::to_value(self.key_source()).unwrap_or_default());
            o.insert("keyMasked".into(), serde_json::to_value(self.key_masked()).unwrap_or(serde_json::Value::Null));
            o.insert("presets".into(), serde_json::to_value(PRESETS).unwrap_or_default());
            o.insert("activePreset".into(), serde_json::Value::String(self.preset.clone()));
            o.insert("price".into(), serde_json::to_value(self.price()).unwrap_or_default());
        }
        v
    }
    /// 套用 PUT /config 的 JSON（同 `transcribe-serve::config::apply` 的规则，少了节流字段）。
    pub fn apply(&mut self, j: &serde_json::Value) -> Result<(), String> {
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        if let Some(v) = s("backend") { self.backend = v; }
        if let Some(v) = s("preset") {
            if v != "custom" && !PRESETS.iter().any(|p| p.id == v) {
                return Err(format!("未知的模型预置：{v}"));
            }
            self.preset = v;
        }
        if self.preset == "custom" {
            if let Some(v) = s("model") { self.custom_model = v; }
            if let Some(v) = s("baseUrl") {
                if !v.starts_with("http://") && !v.starts_with("https://") {
                    return Err("baseUrl 要以 http(s):// 开头".into());
                }
                self.custom_base_url = v.trim_end_matches('/').to_string();
            }
        }
        let provider = self.provider().to_string();
        if let Some(v) = s("apiKey") { self.keys.insert(provider.clone(), v); }
        if j.get("clearKey").and_then(|v| v.as_bool()).unwrap_or(false) { self.keys.remove(&provider); }
        if let Some(price) = j.get("price") {
            let mut p = self.price();
            if let Some(x) = price.get("input").and_then(|v| v.as_f64()) { p.input_per1k = x.max(0.0); }
            if let Some(x) = price.get("output").and_then(|v| v.as_f64()) { p.output_per1k = x.max(0.0); }
            self.prices.insert(self.preset.clone(), p);
        }
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
    fn key_priority_and_masking_scoped_by_provider() {
        let mut c = MindConfig::default();
        assert_eq!(c.key_with_env(None), None);
        assert_eq!(c.key_with_env(Some(" env-k ".into())).as_deref(), Some("env-k"));
        c.keys.insert("dashscope".into(), "file-k".into());
        assert_eq!(c.key_with_env(Some("env-k".into())).as_deref(), Some("file-k"), "文件优先");
        let p = c.public();
        assert!(p.get("apiKey").is_none() && p.get("keys").is_none(), "对外不回显 key/keys 表: {p}");
        assert_eq!(p["hasKey"], true);
        assert_eq!(p["model"], "qwen-plus");
        assert_eq!(p["keyMasked"], "...le-k", "只露最后 4 位，不整串回显");
        c.keys.remove("dashscope");
        assert_eq!(c.public()["keyMasked"], serde_json::Value::Null, "没 key 时是 null");
    }

    #[test]
    fn switching_provider_does_not_leak_other_providers_key() {
        let mut c = MindConfig::default();
        c.keys.insert("dashscope".into(), "ds-key".into());
        c.apply(&serde_json::json!({"preset": "gpt-5.6-luna"})).unwrap();
        assert_eq!(c.key(), None, "切到 OpenAI，还没存过它的 key");
        c.apply(&serde_json::json!({"apiKey": "oa-key"})).unwrap();
        assert_eq!(c.key(), Some("oa-key".into()));
        c.apply(&serde_json::json!({"preset": "qwen-max"})).unwrap();
        assert_eq!(c.key(), Some("ds-key".into()), "切回 DashScope 系预置，之前存的 key 还在");
    }

    #[test]
    fn apply_updates_only_given_fields() {
        let mut c = MindConfig::default();
        c.apply(&serde_json::json!({"apiKey": "k1", "model": "m2"})).unwrap();
        assert_eq!(c.key().as_deref(), Some("k1"), "preset 缺省不是 custom，model 字段不生效");
        c.apply(&serde_json::json!({"apiKey": ""})).unwrap();
        assert_eq!(c.key().as_deref(), Some("k1"), "空 apiKey 不改");
        c.apply(&serde_json::json!({"clearKey": true})).unwrap();
        assert!(c.key().is_none());
        assert_eq!(c.timeout_secs, 60, "没给的字段不动");
    }

    #[test]
    fn preset_selection_sets_model_and_base_url_atomically_and_rejects_unknown() {
        let mut c = MindConfig::default();
        assert_eq!(c.preset, "qwen-plus", "缺省值就是第一个预置");
        c.apply(&serde_json::json!({"preset": "qwen-max"})).unwrap();
        assert_eq!((c.model(), c.base_url()), ("qwen-max", "https://dashscope.aliyuncs.com/compatible-mode/v1"));
        let err = c.apply(&serde_json::json!({"preset": "gpt-4o"})).unwrap_err();
        assert!(err.contains("未知的模型预置"), "{err}");
        assert_eq!(c.preset, "qwen-max", "拒绝后不改动");
    }

    #[test]
    fn custom_preset_leaves_model_and_base_url_to_the_old_manual_fields() {
        let mut c = MindConfig::default();
        c.apply(&serde_json::json!({"preset": "custom", "model": "my-model", "baseUrl": "https://x.example/v1"})).unwrap();
        assert_eq!((c.model(), c.base_url()), ("my-model", "https://x.example/v1"));
    }

    #[test]
    fn public_exposes_presets_and_active_preset() {
        let c = MindConfig::default();
        let p = c.public();
        assert_eq!(p["activePreset"], "qwen-plus");
        assert!(p["presets"].as_array().unwrap().len() >= 4, "四家厂商都要出现在下拉里");
        assert_eq!(p["presets"][0]["id"], "qwen-plus");
    }

    #[test]
    fn price_is_user_entered_per_model_and_defaults_to_zero() {
        let mut c = MindConfig::default();
        assert_eq!(c.price(), Price::default());
        c.apply(&serde_json::json!({"price": {"input": 0.02, "output": 0.06}})).unwrap();
        assert_eq!(c.price(), Price { input_per1k: 0.02, output_per1k: 0.06 });
    }

    #[test]
    fn migrate_carries_forward_legacy_flat_model_and_key_without_losing_it() {
        let old = serde_json::json!({"backend":"qwen","baseUrl":"https://dashscope.aliyuncs.com/compatible-mode/v1","model":"qwen-max","apiKey":"real-device-key","timeoutSecs":60});
        let c: MindConfig = serde_json::from_value(old).unwrap();
        let c = c.migrate();
        assert_eq!(c.preset, "qwen-max");
        assert_eq!(c.key().as_deref(), Some("real-device-key"));
        assert_eq!(c.clone().migrate(), c, "已经迁移过是 no-op");
    }

    #[test]
    fn migrate_is_noop_for_fresh_install_with_no_legacy_data() {
        let c = MindConfig::default().migrate();
        assert_eq!(c, MindConfig::default());
    }
}
