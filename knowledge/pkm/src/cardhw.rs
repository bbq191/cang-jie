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

// ── 多槽：按槽裁横条、逐条喂 vision（区域隔离→不串槽）────────────────────────
/// 缩略图「文本行 → 像素 Y」映射（reMarkable 总结卡片固定字号 + 缩略图渲染，2026-08-31 真机
/// 384×512 实测：顶 70 + 行×20 → top_frac≈0.137、行高 frac≈0.039；按实际高缩放以耐尺寸变化）。
const THUMB_TOP_FRAC: f32 = 70.0 / 512.0;
const THUMB_LH_FRAC: f32 = 20.0 / 512.0;

/// 裁出来的横条区域用的转写 prompt（区域已隔离，只需按行认手写）。
/// 铁律「只彩色（非黑）手写」——印刷体一律黑色、手写是红/蓝等彩色，颜色是最干净的判据；
/// 放宽成「所有潦草笔迹」会把黑色印刷体也吞进来（真机实测泄漏一堆印刷字）。
const BAND_PROMPT: &str = "这是 reMarkable 卡片的一个横条区域。图里有**黑色印刷体**和少量**彩色手写**\
（红色或蓝色等，笔迹潦草、连笔、大小歪斜）。任务：**只转写彩色（非黑色）的手写笔迹**，\
**绝对不要**输出任何黑色印刷体文字。每一行彩色手写各输出一行、保持从上到下顺序；潦草认最可能的字、\
不留空。这一条里没有彩色手写就输出空。只输出彩色手写文字，不要黑色印刷体、不要序号、不要解释。";

/// 按槽把整页缩略图裁成横条：每个「有手写的槽」→ [该槽头行, 下一个槽头行] 的像素带（顶留半行余量），
/// 返回 (槽头行号, 裁出的 PNG)。区域隔离后单条喂 vision 才认得准（整页会串区/幻觉，真机实测）。
fn crop_slot_bands(thumb_png: &[u8], hw_slots: &[usize], all_slots: &[usize]) -> Vec<(usize, Vec<u8>)> {
    let img = match image::load_from_memory(thumb_png) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let (w, h) = (img.width(), img.height());
    let py = |line: usize| -> u32 {
        ((THUMB_TOP_FRAC + line as f32 * THUMB_LH_FRAC) * h as f32).round().clamp(0.0, h as f32) as u32
    };
    let margin = (THUMB_LH_FRAC * h as f32 * 0.4) as u32;
    let mut out = Vec::new();
    for &sl in hw_slots {
        let next = all_slots.iter().copied().find(|&s| s > sl); // 下一个槽头（不论有无手写）= 带底
        let y0 = py(sl).saturating_sub(margin);
        let y1 = next.map(py).unwrap_or(h).min(h);
        if y1 <= y0 {
            continue;
        }
        let band = img.crop_imm(0, y0, w, y1 - y0);
        let mut buf = std::io::Cursor::new(Vec::new());
        if band.write_to(&mut buf, image::ImageFormat::Png).is_ok() {
            out.push((sl, buf.into_inner()));
        }
    }
    out
}

/// 逐槽转写：`page.plans` 给出有手写的槽 → 每槽裁横条 → 单独 vision 按行转写。
/// 返回 (槽头行号, 该槽手写行列表) + 累计 token。区域隔离让 vision 只面对单槽、不串区。
pub fn transcribe_by_slots(
    thumb_png: &[u8],
    page: &CardPage,
    provider: &str,
    model: Option<&str>,
    key: &str,
) -> Result<(Vec<(usize, Vec<String>)>, Usage), String> {
    let all_slots: Vec<usize> = page
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| crate::cardanchor::is_slot_header(l))
        .map(|(i, _)| i)
        .collect();
    let hw_slots: Vec<usize> = page.plans.iter().map(|p| p.line).collect();
    let bands = crop_slot_bands(thumb_png, &hw_slots, &all_slots);
    let mut per_slot = Vec::new();
    let mut usage = Usage::default();
    for (sl, crop) in bands {
        let resp = device_core::vision::call_vision(&crop, BAND_PROMPT, provider, model, key)?;
        usage = Usage {
            input_tokens: usage.input_tokens + resp.usage.input_tokens,
            output_tokens: usage.output_tokens + resp.usage.output_tokens,
        };
        per_slot.push((sl, parse_notes(&resp.text)));
    }
    Ok((per_slot, usage))
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

/// 「所见即所得」多槽注入（用户 2026-08-31 定）：每个有手写的槽，把该槽 vision 按行转写的每条
/// **各占一行**插到**该槽头正下方**。分工铁律——**槽只信 anchor**（vision 猜槽/整页分块会错、串区，
/// 真机实测），**槽内行结构只信 vision**（对裁出的单槽横条按行转写；.rm 无 anchor_origin_y、
/// 几何恢复不了行）。多槽逐槽独立插；**按槽头行降序插**（先靠下的，避免行号位移影响靠上的）。
/// 幂等：已在该槽块中的行不重插。`per_slot`=(槽头行, 该槽手写行列表)，来自 `transcribe_by_slots`。
pub fn inject_slots(page: &CardPage, per_slot: &[(usize, Vec<String>)]) -> InjectResult {
    let mut res = InjectResult {
        text: page.full.clone(),
        applied: vec![],
        leaked: vec![],
        mismatch: None,
    };
    let bodies_owned: Vec<String> = page.lines.iter().filter_map(|l| bullet_body(l)).map(|s| s.to_string()).collect();
    let bref: Vec<&str> = bodies_owned.iter().map(|s| s.as_str()).collect();

    let mut lines: Vec<String> = page.lines.clone();
    // 按槽头行降序：先插靠下的槽，其插入不位移靠上槽的行号。
    let mut slots: Vec<(usize, Vec<String>)> = per_slot.iter().map(|(l, v)| (*l, v.clone())).collect();
    slots.sort_by(|a, b| b.0.cmp(&a.0));

    for (slot_line, notes) in &slots {
        // 清洗 + 泄漏过滤。
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
            continue;
        }
        // 槽块 = 槽头下一行 .. 下一个槽头之前（在当前 lines 上算，含之前靠下槽的插入）。幂等去重。
        let mut block_end = slot_line + 1;
        while block_end < lines.len() && !crate::cardanchor::is_slot_header(&lines[block_end]) {
            block_end += 1;
        }
        let existing: std::collections::HashSet<String> =
            lines[slot_line + 1..block_end].iter().map(|l| l.trim().to_string()).collect();
        let mut at = slot_line + 1;
        for c in &clean {
            if existing.contains(c.trim()) {
                res.leaked.push(c.clone());
                continue;
            }
            lines.insert(at, c.clone());
            at += 1;
            res.applied.push(c.clone());
        }
    }
    if res.applied.is_empty() && res.leaked.is_empty() {
        // 没关联到任何槽（plans 空）→ 表意未注入。
        res.mismatch = Some((0, 0));
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
    // anchor 定「哪些槽有手写」（plans）；每槽裁横条单独 vision 按行转写（区域隔离不串区）；
    // 逐槽把行插到各自槽头下。多槽同页一次处理。
    let page = CardPage::parse(&rm);
    let (per_slot, usage) = transcribe_by_slots(&thumb, &page, provider, model, key)?;
    let r = inject_slots(&page, &per_slot);
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
    fn inject_slots_single_slot_wysiwyg() {
        // 真机单槽卡：洞见下手写四行。anchor 定槽=洞见，vision 按行给 4 条 →
        // 各占一行插到洞见头行正下方（所见即所得）。
        let page = CardPage::parse(CALIB);
        assert_eq!(page.plans.len(), 1, "手写集中在单槽（洞见）");
        let sl = page.plans[0].line;
        assert!(page.lines[sl].trim().starts_with("🔵 洞见"));
        let notes: Vec<String> = ["第一行", "第二行", "第三行", "第四行"].iter().map(|s| s.to_string()).collect();
        let r = inject_slots(&page, &[(sl, notes.clone())]);
        assert_eq!(r.applied, notes, "四条各占一行、按序注入");
        // 洞见头行紧接着就是第一行→第四行。
        let out: Vec<&str> = r.text.lines().collect();
        let h = out.iter().position(|l| l.trim().starts_with("🔵 洞见")).unwrap();
        assert_eq!(&out[h + 1..h + 5], &["第一行", "第二行", "第三行", "第四行"], "洞见下依序四行");
    }

    #[test]
    fn inject_slots_idempotent() {
        // 重跑：行已在洞见块中 → 不重插（幂等）。
        let page = CardPage::parse(CALIB);
        let sl = page.plans[0].line;
        let notes: Vec<String> = ["第一行", "第二行"].iter().map(|s| s.to_string()).collect();
        let r1 = inject_slots(&page, &[(sl, notes.clone())]);
        let page2 = CardPage {
            lines: r1.text.split('\n').map(str::to_string).collect(),
            full: r1.text.clone(),
            ..CardPage::parse(CALIB)
        };
        let r2 = inject_slots(&page2, &[(sl, notes)]);
        assert!(r2.applied.is_empty(), "已存在的行不重插");
    }

    #[test]
    fn inject_slots_multi_each_under_own() {
        // renbone 多槽（金句+洞见）：逐槽各插各的、互不串；降序插不位移。
        let page = CardPage::parse(include_bytes!("../testdata/cardhw/renbone_slots.rm"));
        let jin = page.plans.iter().find(|p| page.lines[p.line].trim().starts_with("🟡 金句")).unwrap().line;
        let dong = page.plans.iter().find(|p| page.lines[p.line].trim().starts_with("🔵 洞见")).unwrap().line;
        let r = inject_slots(&page, &[(jin, vec!["甲".into()]), (dong, vec!["乙".into()])]);
        assert_eq!(r.applied.len(), 2, "两槽各注入一条");
        let out: Vec<&str> = r.text.lines().collect();
        let hj = out.iter().position(|l| l.trim().starts_with("🟡 金句")).unwrap();
        let hd = out.iter().position(|l| l.trim().starts_with("🔵 洞见")).unwrap();
        assert_eq!(out[hj + 1], "甲", "金句下=甲");
        assert_eq!(out[hd + 1], "乙", "洞见下=乙");
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
