//! 配置 `~/.config/notes/transcribe.json`（0600：含 key）。
//! **第二轮整理区反馈（2026-09-08，点 2「模型管理彻底重做」）**：一个服务不再只认一家厂商——预置表现在
//! 横跨 DashScope/OpenAI/Gemini/DeepSeek 四家，切换预置只是换"这次用哪个模型"，key 按厂商（`provider`）
//! 分开存（`keys` 表），不会出现"切到 OpenAI 却把 DashScope 的 key 发过去"这种事，也不用每切一次模型
//! 就重新粘贴 key。`custom` 转义阀单独占一格 key（跟四个已知厂商都不共用，防止乱填的地址意外收到
//! 别家的密钥）。
//! 对外（GET /config）永远只报 `hasKey`/`keySource`，**不回显 key**；写入走 PUT /config 的 `apiKey` 字段
//! （空串=不改，`clearKey`=清当前预置所属厂商的那把）。
//! **模型 id 是易变信息，不凭记忆写**：下表四家的 base_url／model 字符串都是 2026-09-08 当天过 WebSearch/
//! WebFetch 核实官方文档页给出的（OpenAI `developers.openai.com/api/docs/models`、Gemini
//! `ai.google.dev/gemini-api/docs/openai`、DeepSeek `api-docs.deepseek.com/quick_start/pricing`）——
//! 这类字符串官方随时会改名，写死进代码本身就是权宜之计；真跑不通了首选去官方文档核对是不是又改了，
//! 而不是怀疑这段注释。豆包（火山方舟）**没有**收进预置表：它的"模型"实际是账号自建的推理接入点 ID
//! （`ep-xxxxxxxx`），不是一个所有用户通用的固定字符串，硬填一个占位模型名到预置表里反而是在提供一个
//! 保真不了的"已知能用"承诺——用户要接豆包，走"自定义"，`baseUrl` 填 `https://ark.cn-beijing.volces.com/api/v3`，
//! `model` 填自己在方舟控制台建的 Endpoint ID。
//! **花费不做官方定价表**：第三方 API 定价比模型 id 还易变（区域/促销价随时变），写死一份价格表比模型 id
//! 写死更容易在用户不知情的情况下把"预估花费"这个数字做错、误导用户的实际支出判断——干脆不猜，改成让
//! 用户自己在模型面板填"每 1K token 输入/输出单价"（`prices`，缺省 0＝不计费，用量卡片只显示 token 数不
//! 显示金额），这是他们自己账户下的真实价格，比我这边任何时点的快照都准。
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

/// 视觉模型预置表（换厂商/加型号在这加一行，网页自动出现新选项；核实来源见模块文档）。
pub const PRESETS: &[Preset] = &[
    Preset { id: "qwen3-vl-plus", label: "Qwen3-VL-Plus（推荐，速度快）", model: "qwen3-vl-plus", base_url: DASHSCOPE, provider: "dashscope" },
    Preset { id: "qwen-vl-max", label: "Qwen-VL-Max（更准，稍慢）", model: "qwen-vl-max", base_url: DASHSCOPE, provider: "dashscope" },
    Preset { id: "qwen-vl-plus", label: "Qwen-VL-Plus（旧一代视觉模型）", model: "qwen-vl-plus", base_url: DASHSCOPE, provider: "dashscope" },
    Preset { id: "gpt-5.6-terra", label: "GPT-5.6 Terra（OpenAI，性价比）", model: "gpt-5.6-terra", base_url: OPENAI, provider: "openai" },
    Preset { id: "gpt-6-astra", label: "GPT-6 Astra（OpenAI，旗舰更贵）", model: "gpt-6-astra", base_url: OPENAI, provider: "openai" },
    Preset { id: "gemini-3.8-flash", label: "Gemini 3.8 Flash（Google）", model: "gemini-3.8-flash", base_url: GEMINI, provider: "gemini" },
    Preset { id: "deepseek-v4-flash-vision-exp", label: "DeepSeek V4 Flash Vision（实验性视觉）", model: "deepseek-v4-flash-vision-exp", base_url: DEEPSEEK, provider: "deepseek" },
];

/// 用户自填的每千 token 单价（缺省都是 0＝不计费）。分输入/输出两档是因为大多数厂商这两档价格不同。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Price {
    pub input_per1k: f64,
    pub output_per1k: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TranscribeConfig {
    /// 后端标识（写进草稿 `backend` 字段，便于区分不同模型的建议）。
    pub backend: String,
    /// 当前选中的预置 id；`"custom"` 走下面两个手填字段。
    pub preset: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub custom_base_url: String,
    /// 厂商（`Preset.provider`，或 `"custom"`）→ key。同厂商多个预置共用一把，切换预置不用重新粘贴。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
    /// 预置 id（或 `"custom"`）→ 用户自填单价，见模块文档"花费不做官方定价表"。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub prices: BTreeMap<String, Price>,
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

    /// 迁移专用（第二轮反馈重做前的老配置文件形状：单一 `model`/`baseUrl`/`apiKey` 三个平铺字段）——
    /// 只在反序列化时读一次，见 `migrate()`；新配置永远不再写这三个字段（`skip_serializing`）。
    #[serde(skip_serializing)]
    pub model: String,
    #[serde(skip_serializing)]
    pub base_url: String,
    #[serde(skip_serializing)]
    pub api_key: String,
}

impl Default for TranscribeConfig {
    fn default() -> Self {
        TranscribeConfig {
            backend: "qwen".into(),
            preset: PRESETS[0].id.to_string(),
            custom_model: String::new(),
            custom_base_url: String::new(),
            keys: BTreeMap::new(),
            prices: BTreeMap::new(),
            timeout_secs: 60,
            max_per_run: 20,
            pause_ms: 300,
            auto: true,
            max_attempts: 3,
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

impl TranscribeConfig {
    /// 老配置文件（重做预置表之前，2026-09-08 上午之前落盘的）搬进新形状——只在启动加载时调用一次。
    /// 判据：`preset` 是老形状里从没有过的字段，反序列化后拿到的是 `Default` 给的第一个预置 id；这时
    /// 如果老的 `model`/`base_url` 三件套有值，说明这是一份没升级过的旧文件，需要迁移。已经是新形状的
    /// 文件（`keys` 非空，或 `preset` 命中过真实保存动作）不会重复迁移——迁移只搬一次运行时状态，不改
    /// 落盘文件本身（下一次 PUT /config 保存就会是新形状，旧字段因为 `skip_serializing` 自然消失）。
    pub fn migrate(mut self) -> Self {
        if !self.keys.is_empty() {
            return self; // 已经是新形状（迁移过或本来就是新写入的），no-op
        }
        if self.api_key.is_empty() && self.model.is_empty() && self.base_url.is_empty() {
            return self; // 全新安装，没有老数据可迁
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
    /// 解析出可用的 key（不打印、不落日志）——按当前预置所属厂商去 `keys` 里找。
    pub fn key(&self) -> Option<String> {
        self.key_with_env(std::env::var(KEY_ENV).ok())
    }
    pub fn key_with_env(&self, env: Option<String>) -> Option<String> {
        let provider = self.provider();
        if let Some(k) = self.keys.get(provider).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(k.to_string());
        }
        // 环境变量只在缺省的 DashScope 组合下兜底——不该让 DashScope 的环境变量被误当成其它厂商的 key。
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
    /// 当前预置的用户自填单价（没填过就是全 0，网页不显示金额只显示 token 数）。
    pub fn price(&self) -> Price {
        self.prices.get(&self.preset).copied().unwrap_or_default()
    }
    /// 用量记账的分组键——按预置 id 分；自定义模型按 `"custom:<model>"` 分（不同自定义地址/模型各算各的）。
    pub fn usage_key(&self) -> String {
        if self.preset == "custom" { format!("custom:{}", self.custom_model) } else { self.preset.clone() }
    }
    /// 对外视图：去 key、加 hasKey/keySource/keyMasked/presets/activePreset/model/baseUrl/price。
    pub fn public(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.remove("keys");
            o.remove("model");
            o.remove("baseUrl");
            o.remove("apiKey");
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
    /// 套用 PUT /config 的 JSON：可改字段逐个覆盖；`apiKey` 非空才改（存到当前厂商名下）；
    /// `clearKey:true` 清当前厂商那把。`preset` 切换预置（未知预置名拒绝）；`preset:"custom"` 时
    /// `model`/`baseUrl` 才生效，写进 `customModel`/`customBaseUrl`。`price:{input,output}` 存到当前
    /// 预置名下。
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
    fn key_priority_and_masking_scoped_by_provider() {
        let mut c = TranscribeConfig::default();
        assert_eq!(c.key_with_env(None), None);
        assert_eq!(c.key_with_env(Some(" env-k ".into())).as_deref(), Some("env-k"), "缺省预置是 DashScope，环境变量兜底生效");
        c.keys.insert("dashscope".into(), "file-k".into());
        assert_eq!(c.key_with_env(Some("env-k".into())).as_deref(), Some("file-k"), "文件优先");
        let p = c.public();
        assert!(p.get("apiKey").is_none() && p.get("keys").is_none(), "对外不回显 key/keys 表: {p}");
        assert_eq!(p["hasKey"], true);
        assert_eq!(p["model"], "qwen3-vl-plus");
        assert_eq!(p["keyMasked"], "...le-k", "只露最后 4 位，不整串回显");
        c.keys.remove("dashscope");
        assert_eq!(c.public()["keyMasked"], serde_json::Value::Null, "没 key 时是 null");
    }

    #[test]
    fn switching_provider_does_not_leak_other_providers_key_and_env_fallback_is_dashscope_only() {
        let mut c = TranscribeConfig::default();
        c.keys.insert("dashscope".into(), "ds-key".into());
        c.apply(&serde_json::json!({"preset": "gpt-5.6-terra"})).unwrap();
        assert_eq!(c.provider(), "openai");
        assert_eq!(c.key_with_env(Some("env-should-not-apply".into())), None, "切到 OpenAI 后既没有 openai 的 key，也不该借用 DashScope 的环境变量兜底");
        c.apply(&serde_json::json!({"apiKey": "oa-key"})).unwrap();
        assert_eq!(c.key(), Some("oa-key".into()));
        c.apply(&serde_json::json!({"preset": "qwen-vl-max"})).unwrap();
        assert_eq!(c.key(), Some("ds-key".into()), "切回 DashScope 系预置，之前存的 key 还在，不用重新粘贴");
    }

    #[test]
    fn apply_updates_only_given_fields() {
        let mut c = TranscribeConfig::default();
        c.apply(&serde_json::json!({"apiKey": "k1", "maxPerRun": 5})).unwrap();
        assert_eq!((c.key().as_deref(), c.max_per_run), (Some("k1"), 5));
        c.apply(&serde_json::json!({"apiKey": ""})).unwrap();
        assert_eq!(c.key().as_deref(), Some("k1"), "空 apiKey 不改");
        c.apply(&serde_json::json!({"clearKey": true})).unwrap();
        assert!(c.key().is_none());
        assert_eq!(c.timeout_secs, 60, "没给的字段不动");
    }

    #[test]
    fn preset_selection_sets_model_and_base_url_atomically_and_rejects_unknown() {
        let mut c = TranscribeConfig::default();
        assert_eq!(c.preset, "qwen3-vl-plus", "缺省值就是第一个预置");
        c.apply(&serde_json::json!({"preset": "qwen-vl-max"})).unwrap();
        assert_eq!((c.model(), c.base_url()), ("qwen-vl-max", "https://dashscope.aliyuncs.com/compatible-mode/v1"));
        let err = c.apply(&serde_json::json!({"preset": "gpt-4o"})).unwrap_err();
        assert!(err.contains("未知的模型预置"), "{err}");
        assert_eq!(c.preset, "qwen-vl-max", "拒绝后不改动");
    }

    #[test]
    fn custom_preset_leaves_model_and_base_url_to_the_old_manual_fields() {
        let mut c = TranscribeConfig::default();
        c.apply(&serde_json::json!({"preset": "custom", "model": "my-model", "baseUrl": "https://x.example/v1"})).unwrap();
        assert_eq!((c.model(), c.base_url()), ("my-model", "https://x.example/v1"));
        assert!(c.apply(&serde_json::json!({"baseUrl": "dashscope"})).is_err());
    }

    #[test]
    fn public_exposes_presets_and_active_preset() {
        let c = TranscribeConfig::default();
        let p = c.public();
        assert_eq!(p["activePreset"], "qwen3-vl-plus");
        assert!(p["presets"].as_array().unwrap().len() >= 4, "四家厂商都要出现在下拉里");
        assert_eq!(p["presets"][0]["id"], "qwen3-vl-plus");
        assert_eq!(p["presets"][0]["provider"], "dashscope");
    }

    #[test]
    fn price_is_user_entered_per_model_and_defaults_to_zero() {
        let mut c = TranscribeConfig::default();
        assert_eq!(c.price(), Price::default(), "没填过就是 0，不计费");
        c.apply(&serde_json::json!({"price": {"input": 0.01, "output": 0.03}})).unwrap();
        assert_eq!(c.price(), Price { input_per1k: 0.01, output_per1k: 0.03 });
        c.apply(&serde_json::json!({"preset": "gpt-5.6-terra"})).unwrap();
        assert_eq!(c.price(), Price::default(), "换了个模型，价格各记各的，不共用");
    }

    #[test]
    fn usage_key_groups_by_preset_or_custom_model() {
        let mut c = TranscribeConfig::default();
        assert_eq!(c.usage_key(), "qwen3-vl-plus");
        c.apply(&serde_json::json!({"preset": "custom", "model": "foo", "baseUrl": "https://x.example/v1"})).unwrap();
        assert_eq!(c.usage_key(), "custom:foo");
    }

    #[test]
    fn migrate_carries_forward_legacy_flat_model_and_key_without_losing_it() {
        // 老形状（2026-09-08 之前）：apiKey/model/baseUrl 平铺，没有 preset/keys 字段。
        let old = serde_json::json!({"backend":"qwen","baseUrl":"https://dashscope.aliyuncs.com/compatible-mode/v1","model":"qwen-vl-max","apiKey":"real-device-key","timeoutSecs":60,"maxPerRun":20,"pauseMs":300,"auto":true,"maxAttempts":3});
        let c: TranscribeConfig = serde_json::from_value(old).unwrap();
        assert!(c.keys.is_empty(), "反序列化本身不做迁移，只是老字段读进了兼容字段");
        let c = c.migrate();
        assert_eq!(c.preset, "qwen-vl-max", "按老 model+baseUrl 匹配回对应预置");
        assert_eq!(c.key().as_deref(), Some("real-device-key"), "真机已保存的 key 没有因为升级配置形状而丢");
        // 已经是新形状（keys 非空）的文件，迁移是 no-op。
        let migrated_twice = c.clone().migrate();
        assert_eq!(migrated_twice, c);
    }

    #[test]
    fn migrate_unmatched_legacy_combo_falls_back_to_custom() {
        let old = serde_json::json!({"apiKey":"k","model":"some-unlisted-model","baseUrl":"https://x.example/v1"});
        let c: TranscribeConfig = serde_json::from_value(old).unwrap();
        let c = c.migrate();
        assert_eq!(c.preset, "custom");
        assert_eq!((c.custom_model.as_str(), c.custom_base_url.as_str()), ("some-unlisted-model", "https://x.example/v1"));
        assert_eq!(c.key().as_deref(), Some("k"));
    }

    #[test]
    fn migrate_is_noop_for_fresh_install_with_no_legacy_data() {
        let c = TranscribeConfig::default().migrate();
        assert_eq!(c, TranscribeConfig::default(), "全新安装没有老字段，迁移不该改任何东西");
    }
}
