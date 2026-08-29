//! 端化 cardhw（块⑥「手写识别」，代码骑 pkm daemon、概念归块⑥）：
//! 卡片手写批注 → **设备自己调云** vision 空间关联 → 内联注入书摘行。
//!
//! host 版（`pkm-semantic/handwriting/cardhw.py`）的设备端 Rust 移植。两块逻辑：
//!   - `transcribe_card`：整页缩略图 → 多模态 vision（通用调用在 `device_core::vision`）→ [{anchor,note}]。
//!     卡片专属 prompt + JSON 解析留本模块，四后端 HTTP 调用复用共享底座。
//!   - `inject_inline`：转写按 anchor 匹配到书摘 bullet 行、**内联拼接**行尾；
//!     **打印文字泄漏结构性过滤**（note≈已有 bullet 即丢）；幂等。
//!
//! 注入落回卡片必须走 `/upload` 重建（xochitl 直写不可见），编排在 bin/daemon，本模块只出新文本。

use device_core::vision::Usage;
use serde_json::Value;
use std::collections::HashSet;

/// 一条手写批注：note=手写转写，anchor=它所贴的黑色打印条目原文（仅用于定位）。
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub anchor: String,
    pub note: String,
}

// ── vision 适配器（通用调用在 `device_core::vision`，此处只留卡片专属 prompt + 解析）──────
const CARD_PROMPT: &str = "这是一张 reMarkable「总结卡片」的整页渲染，页面上有两类文字：\
（甲）印刷体——规整、等宽、横平竖直的机器字，是卡片已有内容（若干标题槽 + 每槽下以 · 开头的书摘条目）；\
（乙）手写批注——笔迹潦草、有连笔、粗细不均、大小歪斜的手写字，是用户贴着某条书摘条目旁边加的想法。\
手写可能是任意颜色（红/蓝等更明显，但也可能和印刷一样是黑色）——**别只靠颜色分，要靠字形：机器字规整、手写字潦草**。\
任务：只转写（乙）手写批注，逐条给出，并指出它贴着哪一条（甲）印刷的 · 书摘条目。\
铁律：note 只写手写的字；**绝不能把印刷体的字算进 note**——哪怕手写就写在某条印刷条目旁边或叠在其上，note 也只保留你判断为手写的那部分，印刷部分一个字都不许带。\
拿不准某段到底是印刷还是手写时，宁可漏掉不写，也绝不把印刷字当手写输出。\
anchor 写它所贴的那条印刷 · 条目的原文（去掉开头的 · 和空格，照抄印刷字，别加手写）。\
只输出一个 JSON 数组，每元素 {\"anchor\":\"印刷条目原文\",\"note\":\"手写逐字转写\"}；\
手写潦草认不准写最可能的字，不留空、不加问号占位；没有手写批注的条目不要列。\
不要输出 JSON 以外的任何字符（不要 markdown 代码围栏、不要解释）。";

/// 整页卡片缩略图 → 手写批注列表 [{anchor,note}] + token 消耗。通用 HTTP 调用复用 `device_core::vision::call_vision`。
pub fn transcribe_card(
    image_png: &[u8],
    provider: &str,
    model: Option<&str>,
    key: &str,
) -> Result<(Vec<Annotation>, Usage), String> {
    let resp = device_core::vision::call_vision(image_png, CARD_PROMPT, provider, model, key)?;
    Ok((parse_annotations(&resp.text)?, resp.usage))
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

/// note 若以某条印刷 bullet 原文开头（vision 把「印刷条目 + 手写」连成一串的泄漏），
/// 剥掉该印刷前缀只留手写部分。取能剥出非空手写、且剥掉的印刷前缀最长的那条。
/// 剥不出（note 不以任何印刷条目开头）→ 原样返回。
fn strip_printed_prefix(note: &str, bodies: &[&str]) -> String {
    let n = note.trim();
    let punct: &[char] = &['，', '。', ',', '.', '、', '；', ';', '：', ':', ' ', '\t'];
    let mut best: Option<(usize, String)> = None; // (剥掉的印刷前缀长度, 剩余手写)
    for b in bodies {
        let b = b.trim();
        if b.is_empty() { continue; }
        if let Some(rest) = n.strip_prefix(b) {
            let rest = rest.trim_start_matches(punct).trim();
            if !rest.is_empty() && best.as_ref().map_or(true, |(l, _)| b.chars().count() > *l) {
                best = Some((b.chars().count(), rest.to_string()));
            }
        }
    }
    best.map(|(_, r)| r).unwrap_or_else(|| n.to_string())
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
        // 先剥掉 note 里混入的印刷条目前缀（低分辨率下 vision 常把「印刷+手写」连成一串）
        let note = strip_printed_prefix(&a.note, &bref);
        // 剥完仍整体雷同某条印刷 bullet（或剥空）→ 判定纯泄漏，丢弃。
        if note.is_empty() || looks_printed(&note, &bref) {
            res.leaked.push(a.clone());
            continue;
        }
        match best_line(&lines, &a.anchor, &used) {
            None => res.unmatched.push(a.clone()),
            Some(i) => {
                used.insert(i);
                let trimmed = lines[i].trim_end().to_string();
                if trimmed.ends_with(&note) {
                    // 该行已以此 note 结尾：幂等重跑（之前已注入）或 note 本就是这行印刷内容（泄漏回环）
                    // → 不产生新变化，**不计入 applied**（否则误报"已注入"→ sync 误判/空转上传）。
                    res.leaked.push(a.clone());
                    continue;
                }
                lines[i] = format!("{} {}", trimmed, note);
                res.applied.push(bullet_body(&lines[i]).unwrap_or(lines[i].trim()).to_string());
            }
        }
    }
    res.text = lines.join("\n");
    res
}

// ── 编排：处理一个卡片文档（daemon 与 bin 共用）─────────────────────────────
/// 一次卡片处理的结果。
pub struct ProcessOutcome {
    pub visible_name: String,
    pub page_id: String,
    pub applied: Vec<String>,      // 拼接后条目
    pub leaked: Vec<Annotation>,   // 打印泄漏丢弃
    pub unmatched: Vec<Annotation>,
    pub action: Option<String>,    // Some("创建"/"更新")=已 /upload；None=dry-run 或无变化
    pub usage: Usage,              // 本次调云 token 消耗（转写发生才非零；无手写页时零）
}

pub fn read_visible_name(dir: &str, uuid: &str) -> String {
    std::fs::read_to_string(format!("{dir}/{uuid}.metadata"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["visibleName"].as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// 找该卡片文档里含手写笔划（SceneLine>0）的页；返回 (页序 idx, page_id, 全页文本)。
pub fn find_handwritten_page(dir: &str, doc_uuid: &str) -> Option<(usize, String, Vec<String>)> {
    let pages = crate::cardnote::list_pages(dir, doc_uuid).ok()?; // (idx, page_id, text)
    let texts: Vec<String> = pages.iter().map(|(_, _, t)| t.clone()).collect();
    for (pi, (_, page_id, _)) in pages.iter().enumerate() {
        let rm = std::fs::read(format!("{dir}/{doc_uuid}/{page_id}.rm")).unwrap_or_default();
        let strokes = crate::stardetect::read_strokes(&rm).map(|s| s.len()).unwrap_or(0);
        if strokes > 0 {
            return Some((pi, page_id.clone(), texts));
        }
    }
    None
}

/// 处理一个卡片文档：找含手写页 → 设备调云转写 → 内联注入 →（apply 时）走 /upload 重建。
/// 无手写页返回 `Ok(None)`（daemon 增量事件里多数卡片走这条=零成本）。apply=false 只出方案不写。
pub fn process_card_doc(
    dir: &str,
    doc_uuid: &str,
    provider: &str,
    model: Option<&str>,
    key: &str,
    card_folder: &str,
    apply: bool,
) -> Result<Option<ProcessOutcome>, String> {
    let (page_idx, page_id, mut texts) = match find_handwritten_page(dir, doc_uuid) {
        Some(p) => p,
        None => return Ok(None),
    };
    let vn = read_visible_name(dir, doc_uuid);
    let thumb = std::fs::read(format!("{dir}/{doc_uuid}.thumbnails/{page_id}.png"))
        .map_err(|e| format!("读缩略图失败：{e}"))?;
    let (anns, usage) = transcribe_card(&thumb, provider, model, key)?;
    if anns.is_empty() {
        return Ok(Some(ProcessOutcome {
            visible_name: vn, page_id, applied: vec![], leaked: vec![], unmatched: vec![], action: None, usage,
        }));
    }
    let r = inject_inline(&texts[page_idx], &anns);
    let mut action = None;
    if apply && !r.applied.is_empty() {
        texts[page_idx] = r.text;
        action = crate::notebook_sync::sync_auto_notebook(dir, &vn, &texts, card_folder);
    }
    Ok(Some(ProcessOutcome {
        visible_name: vn, page_id,
        applied: r.applied, leaked: r.leaked, unmatched: r.unmatched, action, usage,
    }))
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
        // 第二遍该行已以此 note 结尾 → 归泄漏、**不再误报 applied**（旧 bug：applied 无条件 push
        // 致 sync 误判"有变化"却又内容相同 → 空转/None）。
        assert!(r2.applied.is_empty(), "幂等重跑不报 applied");
    }

    #[test]
    fn concatenated_printed_prefix_stripped() {
        // 低分辨率 vision 把「印刷条目 + 手写」连成一串：note="只想睡觉 但很累了"（印刷"只想睡觉"+手写"但很累了"）
        // → 剥掉印刷前缀，只把手写"但很累了"拼回该行。
        let r = inject_inline(CARD, &[ann("只想睡觉", "只想睡觉 但很累了")]);
        assert!(r.text.contains("· 只想睡觉 但很累了"), "剥印刷前缀后拼手写");
        assert!(!r.text.contains("只想睡觉 只想睡觉"), "印刷部分没被重复带进去");
        assert_eq!(r.applied.len(), 1);
        assert!(r.leaked.is_empty());
    }

    #[test]
    fn full_printed_note_roundtrip_leaked() {
        // note 整条就是某印刷 bullet（vision 把黑印刷字当手写读回来）→ 剥空/雷同 → 判泄漏、不注入、不报 applied。
        let r = inject_inline(CARD, &[ann("只想睡觉", "只想睡觉")]);
        assert!(r.applied.is_empty(), "纯印刷泄漏不产生注入");
        assert_eq!(r.leaked.len(), 1);
    }

    #[test]
    fn parse_annotations_strips_fences_and_punct() {
        let raw = "```json\n[{\"anchor\":\"她\",\"note\":\"，一脸茫然\"}]\n```";
        let v = parse_annotations(raw).unwrap();
        assert_eq!(v, vec![ann("她", "一脸茫然")], "剥围栏+清前导逗号");
    }
}
