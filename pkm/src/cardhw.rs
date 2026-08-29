//! 端化 cardhw（块⑥「手写识别」，代码骑 pkm daemon、概念归块⑥）：
//! 卡片手写批注 → **设备自己调云** vision 空间关联 → 内联注入书摘行。
//!
//! host 版（`pkm-semantic/handwriting/cardhw.py`）的设备端 Rust 移植。两块逻辑：
//!   - `transcribe_card`：整页缩略图 base64 → 多模态 vision（四后端可插拔）→ [{anchor,note}]。
//!     设备发 HTTPS（ureq+rustls，与 reading weread 同机制、生产已验证）。
//!   - `inject_inline`：转写按 anchor 匹配到书摘 bullet 行、**内联拼接**行尾；
//!     **打印文字泄漏结构性过滤**（note≈已有 bullet 即丢）；幂等。
//!
//! 注入落回卡片必须走 `/upload` 重建（xochitl 直写不可见），编排在 bin/daemon，本模块只出新文本。

use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashSet;

/// 一条手写批注：note=手写转写，anchor=它所贴的黑色打印条目原文（仅用于定位）。
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub anchor: String,
    pub note: String,
}

// ── vision 适配器 ───────────────────────────────────────────────────────────
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

const CARD_PROMPT: &str = "这是一张 reMarkable「总结卡片」的整页渲染。黑色打印是卡片已有内容\
（若干槽 + 每槽下 · 开头的书摘条目）；彩色手写（红/蓝等，非黑）是用户贴着某条书摘条目旁边加的批注。\
任务：只找彩色手写批注，把每段手写转写出来，并指出它贴着哪一条黑色打印的 · 条目。\
极其重要：note 只写手写的字，绝对不要把它旁边那条黑色打印文字算进去。\
anchor 写它所贴的那条黑色打印条目的原文（去掉开头的 · 和空格，照抄黑字，别改别加手写）。\
只输出一个 JSON 数组，每元素 {\"anchor\":\"黑色打印条目原文\",\"note\":\"手写逐字转写\"}；\
手写潦草认不准写最可能的字，不留空、不加问号占位。没有手写的条目不要列。\
不要输出 JSON 以外的任何字符（不要 markdown 代码围栏、不要解释）。";

/// 整页卡片缩略图 → 手写批注列表。设备发 HTTPS 到对应后端。
pub fn transcribe_card(
    image_png: &[u8],
    provider: &str,
    model: Option<&str>,
    key: &str,
) -> Result<Vec<Annotation>, String> {
    let cfg = provider_cfg(provider).ok_or_else(|| format!("未知后端 {provider}"))?;
    let model = model.unwrap_or(cfg.default_model);
    let b64 = base64::engine::general_purpose::STANDARD.encode(image_png);
    let data_uri = format!("data:image/png;base64,{b64}");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build();

    let raw = if cfg.style == "anthropic" {
        let payload = json!({
            "model": model, "max_tokens": 2048,
            "messages": [{"role": "user", "content": [
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64}},
                {"type": "text", "text": CARD_PROMPT},
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
        resp["content"].as_array().map(|a| a.iter()
            .filter_map(|b| if b["type"] == "text" { b["text"].as_str() } else { None })
            .collect::<Vec<_>>().join(""))
            .ok_or_else(|| format!("应答无法解析：{}", resp))?
    } else {
        let payload = json!({
            "model": model, "temperature": 0,
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": CARD_PROMPT},
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
        resp["choices"][0]["message"]["content"].as_str()
            .ok_or_else(|| format!("应答无法解析：{}", resp))?
            .to_string()
    };
    parse_annotations(&raw)
}

/// 从模型应答里抠出 JSON 数组 → [{anchor,note}]，清洗 note 两端标点。
fn parse_annotations(raw: &str) -> Result<Vec<Annotation>, String> {
    let s = raw.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let (lb, rb) = (s.find('['), s.rfind(']'));
    let (lb, rb) = match (lb, rb) {
        (Some(l), Some(r)) if r > l => (l, r),
        _ => return Err(format!("应答非 JSON 数组：{}", &raw.chars().take(200).collect::<String>())),
    };
    let arr: Value = serde_json::from_str(&s[lb..=rb]).map_err(|e| format!("JSON 解析失败：{e}"))?;
    let trim_pat: &[char] = &['，', '。', ',', '.', '、', '；', ';', '：', ':', ' ', '\t'];
    Ok(arr.as_array().map(|a| a.iter().filter_map(|d| {
        let note = d["note"].as_str()?.trim().trim_matches(trim_pat).to_string();
        if note.is_empty() { return None; }
        let anchor = d["anchor"].as_str().unwrap_or("").trim().trim_start_matches('·').trim().to_string();
        Some(Annotation { anchor, note })
    }).collect()).unwrap_or_default())
}

// ── 注入（移植 host cardhw.py）─────────────────────────────────────────────
/// `   · 只想睡觉` → `只想睡觉`；非 bullet 行返回 None。
fn bullet_body(line: &str) -> Option<&str> {
    let s = line.trim();
    s.strip_prefix("· ").or_else(|| s.strip_prefix('·')).map(|b| b.trim())
}

fn lev(a: &[char], b: &[char]) -> usize {
    let (m, n) = (a.len(), b.len());
    let mut d: Vec<usize> = (0..=n).collect();
    for i in 1..=m {
        let mut prev = d[0];
        d[0] = i;
        for j in 1..=n {
            let cur = d[j];
            d[j] = (d[j] + 1).min(d[j - 1] + 1).min(prev + if a[i - 1] == b[j - 1] { 0 } else { 1 });
            prev = cur;
        }
    }
    d[n]
}

/// 归一化相似度 [0,1]（1 - 编辑距离/较长长度）。近似 Python difflib 用途。
fn sim(a: &str, b: &str) -> f64 {
    let (ca, cb): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let maxlen = ca.len().max(cb.len());
    if maxlen == 0 { return 1.0; }
    1.0 - lev(&ca, &cb) as f64 / maxlen as f64
}

/// note 是否与某条已有打印 bullet 高度雷同 → 疑似"打印文字泄漏"（vision 把黑字当手写）。
fn looks_printed(note: &str, bodies: &[&str]) -> bool {
    bodies.iter().any(|b| note == *b || sim(note, b) > 0.85)
}

/// 在 bullet 行里找与 anchor 最相似的行（阈值 0.5；子串直接给高分）；已用过的行跳过。
fn best_line(lines: &[String], anchor: &str, used: &HashSet<usize>) -> Option<usize> {
    let (mut best_i, mut best_r) = (None, 0.5_f64);
    for (i, line) in lines.iter().enumerate() {
        if used.contains(&i) { continue; }
        if let Some(body) = bullet_body(line) {
            let mut r = sim(anchor, body);
            if !anchor.is_empty() && body.contains(anchor) { r = r.max(0.9); }
            if r > best_r { best_i = Some(i); best_r = r; }
        }
    }
    best_i
}

/// 注入结果。
pub struct InjectResult {
    pub text: String,
    pub applied: Vec<String>,     // 拼接后的条目
    pub leaked: Vec<Annotation>,  // 疑似打印泄漏被丢弃
    pub unmatched: Vec<Annotation>,
}

/// 把每条 {anchor,note} 内联拼接到 anchor 所指 bullet 行尾。幂等；打印泄漏过滤。
pub fn inject_inline(text: &str, anns: &[Annotation]) -> InjectResult {
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let bodies: Vec<&str> = lines.iter().filter_map(|l| bullet_body(l)).collect();
    // 注：bodies 借用 lines，下面改 lines 前先算完 leak 判定所需，故先克隆一份 owned。
    let bodies_owned: Vec<String> = bodies.iter().map(|s| s.to_string()).collect();
    let mut res = InjectResult { text: String::new(), applied: vec![], leaked: vec![], unmatched: vec![] };
    let mut used: HashSet<usize> = HashSet::new();
    for a in anns {
        let bref: Vec<&str> = bodies_owned.iter().map(|s| s.as_str()).collect();
        if looks_printed(&a.note, &bref) {
            res.leaked.push(a.clone());
            continue;
        }
        match best_line(&lines, &a.anchor, &used) {
            None => res.unmatched.push(a.clone()),
            Some(i) => {
                used.insert(i);
                let trimmed = lines[i].trim_end().to_string();
                if !trimmed.ends_with(&a.note) {
                    lines[i] = format!("{} {}", trimmed, a.note);
                }
                res.applied.push(bullet_body(&lines[i]).unwrap_or(lines[i].trim()).to_string());
            }
        }
    }
    res.text = lines.join("\n");
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(anchor: &str, note: &str) -> Annotation {
        Annotation { anchor: anchor.into(), note: note.into() }
    }

    const CARD: &str = "🟡 金句·要记的句：\n   · 她\n   · 只想睡觉\n🔵 洞见：\n   · 她站在候车队伍中\n🔗 关联 → ID：";

    #[test]
    fn inject_inline_to_correct_bullet() {
        let anns = [ann("只想睡觉", "但很累了"), ann("她站在候车队伍中", "一脸茫然")];
        let r = inject_inline(CARD, &anns);
        assert!(r.text.contains("· 只想睡觉 但很累了"), "拼到对的行尾");
        assert!(r.text.contains("· 她站在候车队伍中 一脸茫然"));
        assert!(r.leaked.is_empty() && r.unmatched.is_empty());
    }

    #[test]
    fn anchor_short_not_mismatched() {
        // anchor "她" 应匹配 "· 她"（sim=1）而非 "· 她站在…"
        let r = inject_inline(CARD, &[ann("她", "买想睡觉")]);
        assert!(r.text.contains("· 她 买想睡觉"), "短 anchor 精确匹配");
        assert!(!r.text.contains("候车队伍中 买想睡觉"));
    }

    #[test]
    fn printed_leak_dropped() {
        // vision 误把打印 bullet "只想睡觉" 当手写挂给 anchor "她" → 应被过滤
        let r = inject_inline(CARD, &[ann("她", "只想睡觉")]);
        assert_eq!(r.leaked.len(), 1, "打印泄漏被丢");
        assert!(r.applied.is_empty());
    }

    #[test]
    fn idempotent() {
        let anns = [ann("只想睡觉", "但很累了")];
        let r1 = inject_inline(CARD, &anns);
        let r2 = inject_inline(&r1.text, &anns);
        assert_eq!(r1.text, r2.text, "再跑一次不重复拼接");
    }

    #[test]
    fn parse_annotations_strips_fences_and_punct() {
        let raw = "```json\n[{\"anchor\":\"她\",\"note\":\"，一脸茫然\"}]\n```";
        let v = parse_annotations(raw).unwrap();
        assert_eq!(v, vec![ann("她", "一脸茫然")], "剥围栏+清前导逗号");
    }
}
