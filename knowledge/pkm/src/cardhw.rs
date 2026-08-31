//! 端化 cardhw（块⑥「手写识别」，代码骑 pkm daemon、概念归块⑥）：
//! 卡片手写批注 → **设备自己调云按行转写** → 落到 anchor 定的槽下、每行各占一行（所见即所得）。
//!
//! **分工铁律（用户 2026-08-31 定，真机验证）**——两个信号各用其长：
//!   - **槽只信 `.rm` anchor**（vision 猜槽会错：实测把洞见的字归到金句，是「杜纂」根因）。
//!     `cardanchor::CardPage.plans` 给出「有手写的槽」。
//!   - **行结构只信 vision**（`.rm` 无 `anchor_origin_y`、空行无锚，几何恢复不了「第几行」；
//!     多模态 OCR 天然保留手写的行布局）。
//! 本模块只负责：
//!   - `transcribe_notes`：整页缩略图 → vision（`device_core::vision`）→ **只认手写、逐行、按序**。
//!   - `inject_lines`：把 vision 的每行手写**各占一行插到 anchor 定的槽头正下方**；打印泄漏过滤
//!     + 幂等。v1 仅支持单槽有手写（多槽 → 宁缺勿造不注入）。
//!
//! 注入落回卡片必须走 `/upload` 重建（xochitl 直写不可见），编排在 bin/daemon，本模块只出新文本。
//! 史：曾试「.rm 几何反推每片手写落哪一行」判死（无 anchor_origin_y+空行无锚→行位 collapse），
//! 又试「vision 直接猜槽」判死（把洞见归金句）；终定「anchor 管槽 + vision 管行」。

use crate::cardanchor::CardPage;
use device_core::vision::Usage;
use serde_json::Value;

// ── vision 纯转写（关联交给 .rm anchor，vision 只认字）──────────────────────
/// 纯转写 prompt——**只**要手写逐字、top-to-bottom、一行一条，**不做**任何空间关联。
/// 关联由 `cardanchor` 从 .rm 的 anchor 机制确定性导出，故此处剥离 vision 的猜位职责
/// （杜纂根因）。要求排除印刷体（书摘条目），只出手写。
const NOTES_PROMPT: &str = "这是一张 reMarkable「总结卡片」整页渲染。页面有（甲）印刷体书摘条目\
（规整、等宽的机器字，多以 · 开头）和（乙）少量手写批注（笔迹潦草、有连笔、粗细不均、大小歪斜）。\
任务：**只**把（乙）手写批注逐字转写出来，按它们在页面上从上到下的顺序、一行输出一条。\
【铁律】① 只转写手写笔迹，**绝不**输出任何印刷体书摘文字（哪怕手写就叠在某条书摘旁，也只留手写那几个字）；\
② 一处手写笔迹只输出一行，不要把一条手写拆成多行、也不要合并两处手写；\
③ 潦草认不准写最可能的字，不留空、不加问号方括号占位。\
只输出转写正文本身，一行一条，不要序号、不要 markdown、不要解释、不要描述图片、不要任何其它字符。";

/// 整页卡片缩略图 → **有序手写转写**（每行一条，top-to-bottom）+ token。纯转写、不做关联。
/// 关联由调用方用 `inject_by_anchor`（读 .rm）确定性完成。
/// `expect_n`：.rm 已知手写块数（Some 时拼进 prompt 硬约束条数，稳住整页转写的条数抖动——
/// DeepSeek 真机实测同页 2↔3 不稳，护栏会误挡；给准数后 vision 输出条数才对得齐）。
pub fn transcribe_notes(
    image_png: &[u8],
    provider: &str,
    model: Option<&str>,
    key: &str,
    expect_n: Option<usize>,
) -> Result<(Vec<String>, Usage), String> {
    let prompt = match expect_n {
        Some(n) if n > 0 => format!(
            "{NOTES_PROMPT}\n这一页上一共有 {n} 处手写批注，请**不多不少**正好输出 {n} 行、每行一条，按从上到下顺序。"
        ),
        _ => NOTES_PROMPT.to_string(),
    };
    let resp = device_core::vision::call_vision(image_png, &prompt, provider, model, key)?;
    Ok((parse_notes(&resp.text), resp.usage))
}

/// 把纯转写应答拆成有序 note 列表：去代码围栏、按行切、去序号/项目符号前缀、去空行。
fn parse_notes(raw: &str) -> Vec<String> {
    let s = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    // 只剥前导空白 + 单个项目符号（不碰数字——批注可能以数字开头，如「3人行」；序号靠 prompt 约束）。
    let strip_lead: &[char] = &['-', '*', '·', '•', ' ', '\t'];
    s.lines()
        .map(|l| l.trim().trim_start_matches(strip_lead).trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

// ── 注入辅助（bullet 解析 + 打印泄漏过滤，anchor 注入复用）──────────────────
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
            if !rest.is_empty() && best.as_ref().is_none_or(|(l, _)| b.chars().count() > *l) {
                best = Some((b.chars().count(), rest.to_string()));
            }
        }
    }
    best.map(|(_, r)| r).unwrap_or_else(|| n.to_string())
}

/// 注入结果。
pub struct InjectResult {
    pub text: String,
    pub applied: Vec<String>,   // 拼接后的条目
    pub leaked: Vec<String>,    // 疑似打印泄漏 / 幂等重复被丢弃的 note 文本（诊断用）
    /// 计数护栏：Some((识别条数, 手写块数))=条数不符、未注入（宁缺勿造）。
    pub mismatch: Option<(usize, usize)>,
}

/// 「所见即所得」注入（用户 2026-08-31 定，取代 zip-to-bucket）：vision 按行转写的每条手写
/// **各占一行**、插到 **anchor 定的那个槽头正下方**。分工铁律——
/// **槽只信 anchor**（vision 猜槽会错：实测把洞见的字归到金句），**行结构只信 vision**
/// （.rm 无 anchor_origin_y、空行无锚，几何恢复不了「第几行」；vision OCR 天然保留行布局）。
/// v1 仅支持「手写集中在单个槽」；`page.plans` 恰好 1 个槽才注入，多槽/无槽 → 不注入
/// （宁缺勿造，`mismatch` 表意「未注入」，提示一次只在一个槽下写）。幂等：已在该槽块中的行不重插。
pub fn inject_lines(page: &CardPage, notes: &[String]) -> InjectResult {
    let mut res = InjectResult {
        text: page.full.clone(),
        applied: vec![],
        leaked: vec![],
        mismatch: None,
    };

    let bodies_owned: Vec<String> = page.lines.iter().filter_map(|l| bullet_body(l)).map(|s| s.to_string()).collect();
    let bref: Vec<&str> = bodies_owned.iter().map(|s| s.as_str()).collect();

    // 清洗 note + 过滤打印泄漏（保序）。
    let mut clean: Vec<String> = Vec::new();
    for n in notes {
        let c = strip_printed_prefix(n, &bref);
        if c.is_empty() || looks_printed(&c, &bref) {
            res.leaked.push(n.clone());
            continue;
        }
        clean.push(c);
    }
    if clean.is_empty() {
        return res;
    }

    // 槽只认 anchor：plans 给出「有手写的槽」。v1 仅单槽；≠1 → 不注入（宁缺勿造）。
    if page.plans.len() != 1 {
        res.mismatch = Some((clean.len(), page.plans.len()));
        return res;
    }
    let slot_line = page.plans[0].line;

    // 槽块 = 槽头下一行 .. 下一个槽头之前。幂等：块中已存在的行不重插。
    let mut block_end = slot_line + 1;
    while block_end < page.lines.len() && !crate::cardanchor::is_slot_header(&page.lines[block_end]) {
        block_end += 1;
    }
    let existing: std::collections::HashSet<String> =
        page.lines[slot_line + 1..block_end].iter().map(|l| l.trim().to_string()).collect();
    let to_insert: Vec<String> = clean
        .into_iter()
        .filter(|c| !existing.contains(c.trim()))
        .collect();
    if to_insert.is_empty() {
        for n in notes {
            res.leaked.push(n.clone());
        }
        return res;
    }

    // 每条各占一行，插到槽头正下方（top-to-bottom = vision 行序）。
    let mut lines: Vec<String> = page.lines.clone();
    let mut at = slot_line + 1;
    for c in &to_insert {
        lines.insert(at, c.clone());
        at += 1;
        res.applied.push(c.clone());
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
    pub leaked: Vec<String>,       // 打印泄漏 / 幂等重复丢弃的 note 文本
    pub action: Option<String>,    // Some("创建"/"更新")=已 /upload；None=dry-run 或无变化
    pub usage: Usage,              // 本次调云 token 消耗（转写发生才非零；无手写页时零）
    /// 计数护栏：Some((识别条数, 手写块数))=条数不符未注入（识别不稳，请重开卡片手动核对）。
    pub mismatch: Option<(usize, usize)>,
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

/// 处理一个卡片文档（**anchor 管槽 + vision 管行**，所见即所得）：
///   找含手写页 → 设备调云**按行转写**（`transcribe_notes`，vision 只认手写、逐行）
///   → 读该页 .rm，`inject_lines` 把每行手写各占一行插到 anchor 定的槽头下（v1 单槽）
///   →（apply 且有注入）写回真卡、走 `/upload` 重建（xochitl 直写不可见）。
/// 无手写页返回 `Ok(None)`（daemon 增量事件多数卡片走这条=零成本）。apply=false 只出方案不写。
/// 关联不再由 vision 猜（杜纂根因根除）；计数不符时 `mismatch=Some`、不注入。
#[allow(clippy::too_many_arguments)] // 参数都语义独立（dir/uuid/后端三件套/folder/apply），拆 struct 反而绕
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
    let rm = std::fs::read(format!("{dir}/{doc_uuid}/{page_id}.rm"))
        .map_err(|e| format!("读手写页 .rm 失败：{e}"))?;
    let thumb = std::fs::read(format!("{dir}/{doc_uuid}.thumbnails/{page_id}.png"))
        .map_err(|e| format!("读缩略图失败：{e}"))?;
    // .rm 只解析一次：anchor 定「哪个槽」（plans）；vision 按行转写定「写了什么、分几行」。
    // 不再传 expect_n——行数由 vision 的行布局决定、不由 .rm 槽数约束（旧 zip 时代才用计数提示）。
    let page = CardPage::parse(&rm);
    let (notes, usage) = transcribe_notes(&thumb, provider, model, key, None)?;
    let r = inject_lines(&page, &notes);
    let mut action = None;
    if apply && !r.applied.is_empty() {
        texts[page_idx] = r.text;
        action = crate::notebook_sync::sync_auto_notebook(dir, vn.as_str(), &texts, card_folder);
    }
    Ok(Some(ProcessOutcome {
        visible_name: vn, page_id,
        applied: r.applied, leaked: r.leaked, action, usage,
        mismatch: r.mismatch,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 真机单槽卡（洞见下写了 第一行/第二行/第三行/第四行，帕金森笔迹）。
    const CALIB: &[u8] = include_bytes!("../testdata/cardhw/calib_wysiwyg.rm");

    #[test]
    fn parse_notes_basic() {
        // 去围栏、按行切、剥项目符号、去空行；数字开头不误剥。
        let raw = "```\n- 甲\n· 乙\n\n  丙  \n```";
        assert_eq!(parse_notes(raw), vec!["甲", "乙", "丙"]);
        assert_eq!(parse_notes("3人行"), vec!["3人行"], "数字开头保留");
        assert!(parse_notes("").is_empty());
    }

    #[test]
    fn inject_lines_single_slot_wysiwyg() {
        // 真机单槽卡：洞见下手写四行。anchor 定槽=洞见（单槽），vision 按行给 4 条 →
        // 各占一行插到洞见头行正下方（所见即所得）。
        let page = CardPage::parse(CALIB);
        assert_eq!(page.plans.len(), 1, "手写集中在单槽（洞见）");
        let slot = page.lines[page.plans[0].line].trim().to_string();
        assert!(slot.starts_with("🔵 洞见"), "槽=洞见，实得 {slot:?}");
        let notes: Vec<String> = ["第一行", "第二行", "第三行", "第四行"].iter().map(|s| s.to_string()).collect();
        let r = inject_lines(&page, &notes);
        assert!(r.mismatch.is_none(), "单槽应注入");
        assert_eq!(r.applied, notes, "四条各占一行、按序注入");
        // 洞见头行紧接着就是第一行→第四行。
        let out: Vec<&str> = r.text.lines().collect();
        let h = out.iter().position(|l| l.trim().starts_with("🔵 洞见")).unwrap();
        assert_eq!(&out[h + 1..h + 5], &["第一行", "第二行", "第三行", "第四行"], "洞见下依序四行");
    }

    #[test]
    fn inject_lines_idempotent() {
        // 重跑：四行已在洞见块中 → 不重插（幂等）。
        let page = CardPage::parse(CALIB);
        let notes: Vec<String> = ["第一行", "第二行", "第三行", "第四行"].iter().map(|s| s.to_string()).collect();
        let r1 = inject_lines(&page, &notes);
        let page2 = CardPage {
            lines: r1.text.split('\n').map(str::to_string).collect(),
            full: r1.text.clone(),
            ..CardPage::parse(CALIB)
        };
        let r2 = inject_lines(&page2, &notes);
        assert!(r2.applied.is_empty(), "已存在的行不重插");
    }

    #[test]
    fn inject_lines_multi_slot_skips() {
        // renbone：手写分布在金句+洞见两槽 → plans=2 → v1 不注入（宁缺勿造），mismatch 表意未注入。
        let page = CardPage::parse(include_bytes!("../testdata/cardhw/renbone_slots.rm"));
        assert!(page.plans.len() > 1, "renbone 是多槽");
        let r = inject_lines(&page, &["甲".to_string()]);
        assert!(r.mismatch.is_some(), "多槽不注入");
        assert!(r.applied.is_empty());
        assert_eq!(r.text, page.full, "text 不变");
    }

    #[test]
    fn leak_filter_helpers() {
        // inject_by_anchor 复用的泄漏过滤 helper 直接对账。
        let bodies = ["只想睡觉 但醒了", "她"];
        // looks_printed：整体雷同某 bullet（相同或高相似）→ true。
        assert!(looks_printed("她", &bodies), "== bullet 原文");
        assert!(!looks_printed("好句", &bodies), "真手写不算泄漏");
        // strip_printed_prefix：note 以印刷 bullet 原文开头 → 剥掉只留手写；剥不出则原样。
        assert_eq!(strip_printed_prefix("只想睡觉 但醒了 新想法", &bodies), "新想法", "剥印刷前缀留手写");
        assert_eq!(strip_printed_prefix("独立想法", &bodies), "独立想法", "不以印刷开头则原样");
    }
}
