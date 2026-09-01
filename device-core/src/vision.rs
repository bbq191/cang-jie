//! 通用多模态 vision 适配器（共享底座）——块⑥手写识别用，未来结构识别也可复用。
//!
//! 四后端可插拔（gemini/deepseek/openai/anthropic），一条 OpenAI-兼容路径覆盖前三 + 原生
//! anthropic 一条。raw HTTP 走 `ureq`（与 reading weread 同机制、设备端 HTTPS 生产已验证），
//! 只做「一张图 + 一段 prompt → 模型文本应答」，**不含任何卡片/业务/prompt 逻辑**（那在调用方）。

use base64::Engine as _;
use serde_json::{json, Value};

struct ProviderCfg {
    style: &'static str, // "openai" | "anthropic"
    base_url: &'static str,
    default_model: &'static str,
}

fn provider_cfg(p: &str) -> Option<ProviderCfg> {
    Some(match p {
        "gemini" => ProviderCfg {
            style: "openai",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
            default_model: "gemini-3.6-flash",
        },
        "qwen" => ProviderCfg {
            // 阿里云 DashScope OpenAI-兼容端点（北京域，**国内直连无需代理**，同 DeepSeek 定位）。
            // qwen3-vl-plus：通用多模态、原生高分辨率（远超 DeepSeek 384-token 上限）、跟得住 JSON 指令，
            // 比纯 OCR 模型（qwen-vl-ocr）更适合 cardhw 的「转写+空间关联+JSON」结构化任务。生产默认。
            style: "openai",
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            default_model: "qwen3-vl-plus",
        },
        "deepseek" => ProviderCfg {
            style: "openai",
            base_url: "https://api.deepseek.com",
            default_model: "deepseek-v4-flash-vision-exp",
        },
        "openai" => ProviderCfg {
            style: "openai",
            base_url: "https://api.openai.com/v1",
            default_model: "gpt-4o",
        },
        "anthropic" => ProviderCfg {
            style: "anthropic",
            base_url: "https://api.anthropic.com/v1",
            default_model: "claude-sonnet-4-5",
        },
        _ => return None,
    })
}

/// 一次调用的 token 消耗（各家 API 回传 `usage`，字段名不同，归一到 input/output）。缺字段=0。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl Usage {
    /// 从应答 JSON 的 `usage` 对象抽 token 数。openai 系 prompt_tokens/completion_tokens，
    /// anthropic 系 input_tokens/output_tokens；两套字段名都试，取到哪套用哪套。
    fn from_resp(resp: &Value) -> Usage {
        let u = &resp["usage"];
        let pick = |a: &str, b: &str| u[a].as_u64().or_else(|| u[b].as_u64()).unwrap_or(0);
        Usage {
            input_tokens: pick("prompt_tokens", "input_tokens"),
            output_tokens: pick("completion_tokens", "output_tokens"),
        }
    }
}

/// 一次 vision 调用的应答：模型文本 + token 消耗。
pub struct VisionResponse {
    pub text: String,
    pub usage: Usage,
}

/// 各后端的 API key 环境变量（供 bin/daemon 取 key）。未知后端返回空切片。
pub fn key_envs(provider: &str) -> &'static [&'static str] {
    match provider {
        "gemini" => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        "qwen" => &["DASHSCOPE_API_KEY", "QWEN_API_KEY"],
        "deepseek" => &["DEEPSEEK_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "anthropic" => &["ANTHROPIC_API_KEY"],
        _ => &[],
    }
}

/// 一张图（PNG）+ 一段 prompt → 模型文本应答 + token 消耗。四后端可插拔。缺 key/网络/应答异常返回 Err。
pub fn call_vision(
    image_png: &[u8],
    prompt: &str,
    provider: &str,
    model: Option<&str>,
    key: &str,
) -> Result<VisionResponse, String> {
    let cfg = provider_cfg(provider).ok_or_else(|| format!("未知后端 {provider}"))?;
    let model = model.unwrap_or(cfg.default_model);
    let b64 = base64::engine::general_purpose::STANDARD.encode(image_png);
    // 300s：DeepSeek 系（deepseek-v4-flash-vision-exp）是重推理模型，单次输出常 1w~1.6w+ reasoning
    // token，真机实测耗时在 42s~152s 剧烈波动、偶尔 >180s（打满旧 180s 上限 → 假性失败）。故放宽到
    // 300s 兜住它的慢运行。Qwen 类简洁模型（~百 token）秒回、远用不满。宁可等，也别让慢后端假性超时。
    let agent = crate::http_agent(300);

    if cfg.style == "anthropic" {
        let payload = json!({
            "model": model, "max_tokens": 2048,
            "messages": [{"role": "user", "content": [
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64}},
                {"type": "text", "text": prompt},
            ]}],
        });
        let body = agent.post(&format!("{}/messages", cfg.base_url))
            .set("x-api-key", key)
            .set("anthropic-version", "2023-06-01")
            .set("Content-Type", "application/json")
            .send_string(&payload.to_string())
            .map_err(|e| format!("请求失败：{e}"))?
            .into_string().map_err(|e| format!("读应答失败：{e}"))?;
        let resp: Value = serde_json::from_str(&body).map_err(|e| format!("应答非 JSON：{e}；{body:.200}"))?;
        let text = resp["content"].as_array().map(|a| a.iter()
            .filter_map(|b| if b["type"] == "text" { b["text"].as_str() } else { None })
            .collect::<Vec<_>>().join(""))
            .ok_or_else(|| format!("应答无法解析：{}", resp))?;
        Ok(VisionResponse { text, usage: Usage::from_resp(&resp) })
    } else {
        let data_uri = format!("data:image/png;base64,{b64}");
        let payload = json!({
            "model": model, "temperature": 0,
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": data_uri}},
            ]}],
        });
        let body = agent.post(&format!("{}/chat/completions", cfg.base_url))
            .set("Authorization", &format!("Bearer {key}"))
            .set("Content-Type", "application/json")
            .send_string(&payload.to_string())
            .map_err(|e| format!("请求失败：{e}"))?
            .into_string().map_err(|e| format!("读应答失败：{e}"))?;
        let resp: Value = serde_json::from_str(&body).map_err(|e| format!("应答非 JSON：{e}；{body:.200}"))?;
        let text = resp["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| format!("应答无法解析：{}", resp))?
            .to_string();
        Ok(VisionResponse { text, usage: Usage::from_resp(&resp) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usage_openai_field_names() {
        let resp = json!({"usage": {"prompt_tokens": 384, "completion_tokens": 42, "total_tokens": 426}});
        assert_eq!(Usage::from_resp(&resp), Usage { input_tokens: 384, output_tokens: 42 });
    }

    #[test]
    fn usage_anthropic_field_names() {
        let resp = json!({"usage": {"input_tokens": 500, "output_tokens": 30}});
        assert_eq!(Usage::from_resp(&resp), Usage { input_tokens: 500, output_tokens: 30 });
    }

    #[test]
    fn usage_missing_is_zero() {
        assert_eq!(Usage::from_resp(&json!({})), Usage::default());
    }
}
