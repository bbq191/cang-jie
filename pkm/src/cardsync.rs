//! B 模型卡片 merge 引擎（纯逻辑，host 可测）。
//!
//! 每本有 ★ 待办的书对应一份「《书名》- 总结卡片」笔记本。daemon 每次画星后：
//! 读回现有卡片全部打字 → `parse_card` 拆出「每页的用户批注 + 自由区」→ 按当前扫描到的星
//! `render_card` 重生成（老星保留批注、新星空批注、消失星的批注移到自由区不丢）→ build_rmdoc + /upload。
//!
//! 用户输入靠"读回文字→重建时保留"存活（本地版的旧墨香"云端往返"），故**只认打字（Text 工具）**，
//! 手写笔迹读不回不在此列。

use std::collections::BTreeMap;

pub const TODO_HEADER: &str = "━━ ★ 本书待办 ━━";
pub const FREE_HEADER: &str = "━━ 我的笔记（自由区）━━";
const STAR: char = '★';

/// 卡片的可保留状态：每页的用户批注行 + 自由区逐字。
#[derive(Debug, Default, PartialEq)]
pub struct CardModel {
    /// 页号 → 该 ★ 行下用户打的批注行（逐字，可多行；不含 ★ 行本身）。
    pub notes_by_page: BTreeMap<usize, Vec<String>>,
    /// 自由区（FREE_HEADER 之后）逐字保留。
    pub free_notes: Vec<String>,
}

/// 从 ★ 行抽取页号：匹配行尾 "第 N 页"（N 为数字）。抽不到返回 None。
/// 用 **rfind**（最后一个「页」）而非 find：页码标记恒在行尾「· 第 N 页」，而前缀 label 是
/// 章-节名、**可能自身含「页」字**（如章名"第三页的秘密"）——find 会误命中 label 里的「页」、
/// 收不到数字返 None → 该星批注在重建时被当无页号丢弃。rfind 只认行尾真页码。
pub fn extract_page(line: &str) -> Option<usize> {
    let idx = line.rfind('页')?;
    let before = &line[..idx];
    // 从 '页' 往前收集连续数字（跳过紧邻的空格）
    let digits: String = before.chars().rev().skip_while(|c| c.is_whitespace()).take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.chars().rev().collect::<String>().parse().ok()
}

/// 解析现有卡片全文（所有页 join("\n") 后传入）→ CardModel。
/// 未见 TODO_HEADER（如卡片被破坏/首建）时，除 FREE 区外都当自由文本收进 free_notes 兜底不丢。
pub fn parse_card(full_text: &str) -> CardModel {
    let lines: Vec<&str> = full_text.lines().collect();
    let mut model = CardModel::default();

    // 定位 FREE_HEADER（取最后一个，防用户在正文里也打了这串）。
    let free_at = lines.iter().rposition(|l| l.trim() == FREE_HEADER);
    let todo_at = lines.iter().position(|l| l.trim() == TODO_HEADER);

    // 自由区：FREE_HEADER 之后逐字（去掉尾部纯空行）。
    if let Some(f) = free_at {
        let mut free: Vec<String> = lines[f + 1..].iter().map(|s| s.to_string()).collect();
        while free.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
            free.pop();
        }
        model.free_notes = free;
    }

    // 待办区：TODO_HEADER 之后、FREE_HEADER 之前，按 ★ 行分块。
    let todo_end = free_at.unwrap_or(lines.len());
    if let Some(t) = todo_at {
        let mut cur_page: Option<usize> = None;
        let mut buf: Vec<String> = Vec::new();
        let flush = |page: Option<usize>, buf: &mut Vec<String>, m: &mut CardModel| {
            // 去掉批注尾部纯空行
            while buf.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
                buf.pop();
            }
            if let Some(p) = page {
                if !buf.is_empty() {
                    m.notes_by_page.entry(p).or_default().extend(buf.drain(..));
                } else {
                    buf.clear();
                }
            } else {
                buf.clear();
            }
        };
        for &line in &lines[t + 1..todo_end.min(lines.len())] {
            let trimmed = line.trim_start();
            if trimmed.starts_with(STAR) {
                flush(cur_page, &mut buf, &mut model);
                cur_page = extract_page(trimmed);
            } else {
                buf.push(line.to_string());
            }
        }
        flush(cur_page, &mut buf, &mut model);
    }
    model
}

/// 单条待办的页头文本："★ 章名 - 节名 · 第 N 页"（label 空时只 "★ 第 N 页"）。
fn star_header(page: usize, label: &str) -> String {
    if label.is_empty() {
        format!("{STAR} 第 {page} 页")
    } else {
        format!("{STAR} {label} · 第 {page} 页")
    }
}

/// 新出现的 ★ 页的初始模板 = **通用原子卡（Zettelkasten）** WHAT / SO WHAT / NOW WHAT 骨架
/// （见 PKM 白皮书 §06）。放弃对题材结构的预判（悬疑逻辑树 / 科幻阵营制衡等是特殊解、只适 20%
/// 硬核结构书）——回归原子化底层逻辑，一张卡只讲透一个概念/模型/共鸣点，「生吞」经管/心理/历史/
/// 文学各类文本。daemon 全自动、无法判题材，故默认注入通用模板；特殊模板留白皮书作手动骨架。
/// 只在**首次出现**该星时注入一次；之后用户在字段后打字，重建时整块作为「批注」逐字保留。
///
/// 无章节映射的书（公众号文章类 EPUB / 无 `.epubindex`）用**书名短前缀**给锚点消歧：否则 label 空
/// → 锚点退化成 `[ID: p页号]`，多本这类书都从 p1 起会跨书撞号（真机 2026-08-24 发现），拿 p1 做 MOC
/// 软链接目标就指不清哪本书。折叠空白 + 截前 16 字符成单 token（书名可能很长，截断求可读可打）。
/// 注：只兜底"无章节"这一撞号源；有章名的走 label（章名跨书也可能撞，但概率低、与 §07 既有软链接
/// 歧义同档，且改 label 格式会动全部旧锚点破坏兼容，故不动）。
fn book_short(title: &str) -> String {
    const MAX: usize = 16;
    title.split_whitespace().collect::<String>().chars().take(MAX).collect()
}

/// 卡片模板类型（社区 PKM 范式改编，见白皮书 §06）。默认 `General`——库中英混杂，默认须语言/题材
/// 无关；用户给书打原生 Tag 切特殊模板。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardTemplate {
    /// 通用·原子卡（默认）：任何书/语言，提炼一个概念/洞见（Matuschak Evergreen：原子/概念导向/密链）。
    #[default]
    General,
    /// 原文·英文原版：读英文原著的句子挖掘（Refold/Migaku sentence mining：抓整句上下文 + 生词 + 地道表达）。
    Original,
    /// 悬疑·推理：线索 / 时间线 / 诡计逻辑（小说阅读笔记 + 推理结构改编）。
    Mystery,
    /// 科幻·世界观制衡：势力 / 权力 / 设定规则 / 冲突（worldbuilding 模板改编）。
    SciFi,
}

impl CardTemplate {
    /// 原生 Tag 名 → 模板；无匹配返回 None（→ 调用方退默认 General）。收常见别名，容忍 `#` 前缀。
    pub fn from_tag(tag: &str) -> Option<CardTemplate> {
        match tag.trim().trim_start_matches('#').to_lowercase().as_str() {
            "通用" | "原子" | "永久" | "概念" | "general" => Some(CardTemplate::General),
            "原文" | "英文" | "英文原版" | "original" => Some(CardTemplate::Original),
            "悬疑" | "推理" | "侦探" | "mystery" => Some(CardTemplate::Mystery),
            "科幻" | "scifi" | "sf" => Some(CardTemplate::SciFi),
            _ => None,
        }
    }
}

/// `[ID: …]` 机器锚点（MOC 软链接目标 + cardindex 死链体检的键，保持确定性）。前缀取章名 label，
/// 无章节退回书名短前缀消歧；页号段 `- P N`。各模板共用此锚点作首行。
fn anchor_id(book_title: &str, page: usize, label: &str) -> String {
    let prefix = if label.is_empty() { book_short(book_title) } else { label.to_string() };
    if prefix.is_empty() {
        format!("[ID: P {page}]") // 书名也空（异常兜底）
    } else {
        format!("[ID: {prefix} - P {page}]")
    }
}

/// 卡片采**类型性 6 色槽**（2026-08-25 定稿）：荧光笔 6 色映射到 6 个**固定语义槽**，颜色语义跨模板
/// 不变（画什么色进哪个槽与 Tag 无关），Tag 只改各槽的**提示词**。槽序 + emoji 与 `cardhl::HlColor::slot`
/// 对齐（Yellow/Blue/Pink/Orange/Green/Gray）。多 Tag 时同槽提示词合并去重——不会像"位置性"那样把
/// 8 条含义挤进 6 笔或复制文字（颜色→槽是多对一收敛）。
const SLOT_EMOJI: [&str; 6] = ["🟡", "🔵", "🩷", "🟠", "🟢", "⚪"];

/// 每套模板对 6 个槽的提示词（索引 = 槽序）。类型性：语义固定，只是不同书用不同说法。
fn slot_hints(tmpl: CardTemplate) -> [&'static str; 6] {
    match tmpl {
        CardTemplate::General => ["金句·要记的句", "洞见", "疑问·存疑", "主题·概念", "可复用·关联", "人物·其它"],
        CardTemplate::Original => ["原句", "可意译/触发理解处", "语法/没读懂", "篇章主旨", "地道表达", "生词/短语"],
        CardTemplate::Mystery => ["金句", "主题线索", "待解谜团", "案件·诡计代号", "伏笔/破局链", "关键人物"],
        CardTemplate::SciFi => ["关键句", "主题/洞见触点", "存疑设定", "关键设定/技术", "权力/资源制衡", "势力阵营"],
    }
}

/// 枚举规范序——多模板合并按此固定序拼提示词，与用户打 Tag 的书写顺序无关（结果确定 = 幂等）。
const TEMPLATE_ORDER: [CardTemplate; 4] =
    [CardTemplate::General, CardTemplate::Original, CardTemplate::Mystery, CardTemplate::SciFi];

/// 新星骨架 = 锚点 + 6 固定色槽（提示词按匹配模板合并去重）+ 各槽**当前高亮**（画星那刻从该页 .rm 摘的，
/// 类型性归槽）+ 🔗关联行。`tmpls`=该书 Tag 匹配的模板集（空由调用方退默认 General）；
/// `hl_by_slot`=该页按槽分组的高亮文字（长度 6，空槽为空；整个空切片=无高亮）。
/// 骨架 + 高亮一次注入，之后用户在槽下打字，重建整块作「批注」逐字保留（A 式）。
fn page_scaffold(
    tmpls: &[CardTemplate],
    book_title: &str,
    page: usize,
    label: &str,
    hl_by_slot: &[Vec<String>],
) -> Vec<String> {
    let mut v = vec![anchor_id(book_title, page, label)];
    for slot in 0..6 {
        // 合并各匹配模板该槽提示词（规范序 + 去重）；空集兜底 General。
        let mut hints: Vec<&str> = Vec::new();
        for t in TEMPLATE_ORDER {
            if tmpls.contains(&t) {
                let h = slot_hints(t)[slot];
                if !hints.contains(&h) {
                    hints.push(h);
                }
            }
        }
        if hints.is_empty() {
            hints.push(slot_hints(CardTemplate::General)[slot]);
        }
        v.push(format!("{} {}：", SLOT_EMOJI[slot], hints.join("/")));
        // 填该槽高亮（同槽多段 = 多行，需求②）。
        if let Some(texts) = hl_by_slot.get(slot) {
            for t in texts {
                v.push(format!("   · {t}"));
            }
        }
    }
    v.push("🔗 关联 → ID：".to_string());
    v
}

/// 按当前星集重生成卡片：**每个有 ★ 的页各占一张卡片页**（一主题一卡，留整页给你批注）。
/// 无自由区页（按用户要求删除）。`stars`:(页号,章节标签[可空]) 已排序去重；`prev`:上一版可保留状态。
/// 星被擦掉 → 该页连同其批注一起消失（star=待办本身，擦星即完成）。
///
/// 页文本结构（join("\n") 后是连续流，parse_card 按 ★ 切块，与分页无关）：
///   第一张 = 标题 + TODO_HEADER + 第一条 ★ + 其批注；其后每张 = 一条 ★ + 其批注。
pub fn render_card(book_title: &str, stars: &[(usize, String)], prev: &CardModel) -> Vec<String> {
    render_card_with(book_title, stars, prev, &[CardTemplate::default()])
}

/// 同 `render_card`，但全书**统一**用一套模板集（每星相同）。空 `tmpls` 退默认 `General`。
pub fn render_card_with(book_title: &str, stars: &[(usize, String)], prev: &CardModel, tmpls: &[CardTemplate]) -> Vec<String> {
    let specs: Vec<(usize, String, Vec<CardTemplate>, Vec<Vec<String>>)> =
        stars.iter().map(|(p, l)| (*p, l.clone(), tmpls.to_vec(), Vec::new())).collect();
    render_card_starspecs(book_title, &specs, prev)
}

/// 老星页高亮**增量合并**：把 `hl`（按 6 色槽分组的当前高亮）里**块中还没有的**文字，补到对应颜色槽
/// 已有 bullet 之后（下一个槽头 / 🔗 关联行之前）；用户打字批注一行不动、已有高亮不重复。
/// 让"先画星→空模板→再画线"也能把新高亮流进卡片，同时护住批注。hl 全空（划线摘录关 / 无新高亮）→
/// 原样返回 → 无改动无上传（保持无 churn）。只并进块中**已存在**的颜色槽（用户删掉的槽不会被重建）。
fn merge_highlights(prev: &[String], hl: &[Vec<String>]) -> Vec<String> {
    if hl.iter().all(|v| v.is_empty()) {
        return prev.to_vec();
    }
    // 结算某颜色槽：把该槽里 seen 未覆盖的当前高亮追加到 out。
    fn flush(out: &mut Vec<String>, cur: Option<usize>, seen: &std::collections::HashSet<String>, hl: &[Vec<String>]) {
        if let Some(s) = cur {
            if let Some(texts) = hl.get(s) {
                for t in texts {
                    // 去重：书摘 t 已在？允许"t 后跟空格再接手写批注"的内联拼接形态
                    // （cardhw 把手写批注拼在书摘 bullet 行尾，如 "只想睡觉 但很累了"）也算已在，
                    // 免得重建时把被批注的那条书摘当"缺失"重复插。t 后必须跟空格才算，故
                    // "她" 不会误配 "她站在…"（无空格）。
                    let appended = format!("{t} ");
                    if !seen.iter().any(|s| s == t || s.starts_with(&appended)) {
                        out.push(format!("   · {t}"));
                    }
                }
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur: Option<usize> = None; // 当前所在颜色槽（跨槽内的批注/空行保持不变）
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in prev {
        let slot = SLOT_EMOJI.iter().position(|e| line.starts_with(e));
        if slot.is_some() || line.starts_with("🔗") {
            flush(&mut out, cur, &seen, hl); // 走出上一个槽前先补齐
            seen.clear();
            cur = slot; // Some(新槽) 或 None（🔗 关联行终止最后一个槽）
            out.push(line.clone());
            continue;
        }
        if cur.is_some() {
            if let Some(text) = line.trim_start().strip_prefix("· ") {
                seen.insert(text.to_string()); // 记录该槽已有 bullet（含用户手打的）
            }
        }
        out.push(line.clone());
    }
    flush(&mut out, cur, &seen, hl); // 收尾（无 🔗 时最后一个槽在此补齐）
    out
}

/// 每星**独立**模板集重生成卡片（daemon 按"文档级 tags + 该星页 pageTags"合并出每颗星自己的模板）。
/// `stars`:(1-based 页号, 章节标签, 该星模板集[空则退默认 General], 该页6色槽高亮) 已排序去重；`prev`:上一版可保留状态。
/// **每个有 ★ 的页各占一张卡片页**，新星注入其模板集骨架+高亮、老星保留批注并**增量合并新高亮**。
pub fn render_card_starspecs(
    book_title: &str,
    stars: &[(usize, String, Vec<CardTemplate>, Vec<Vec<String>>)],
    prev: &CardModel,
) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    for (i, (page, label, tmpls, hl)) in stars.iter().enumerate() {
        let mut block: Vec<String> = Vec::new();
        if i == 0 {
            // 第一张卡片页带标题 + 待办区头（parse 靠 TODO_HEADER 定位待办区起点）。
            block.push(format!("《{book_title}》- 总结卡片"));
            block.push(String::new());
            block.push(TODO_HEADER.to_string());
        }
        block.push(star_header(*page, label));
        match prev.notes_by_page.get(page) {
            Some(notes) => block.extend(merge_highlights(notes, hl)), // 老星：保留批注 + 增量合并新高亮
            None => {
                // 空集兜底退默认，保证任何星都有骨架。
                let default = [CardTemplate::default()];
                let t: &[CardTemplate] = if tmpls.is_empty() { &default } else { tmpls };
                block.extend(page_scaffold(t, book_title, *page, label, hl)); // 新星：注入 6 色槽 + 高亮
            }
        }
        pages.push(block.join("\n"));
    }
    if pages.is_empty() {
        pages.push(String::new()); // 兜底（正常不会：仅在有星时调用）
    }
    pages
}

/// 便捷：从多页 .rm 文本直接解析（各页文本 join）。
pub fn parse_pages(pages: &[String]) -> CardModel {
    parse_card(&pages.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_page_basic() {
        assert_eq!(extract_page("★ 第 3 页"), Some(3));
        assert_eq!(extract_page("★ 第一章《x》· 第 12 页"), Some(12));
        assert_eq!(extract_page("★ 第 7页"), Some(7)); // 无空格
        assert_eq!(extract_page("没有页号"), None);
        // label（章-节名）自身含「页」字：必须取行尾真页码，不被 label 里的「页」骗到。
        assert_eq!(extract_page("★ 第三页的秘密 · 第 5 页"), Some(5));
        assert_eq!(extract_page("★ 卷一 - 翻页术 · 第 42 页"), Some(42));
    }

    #[test]
    fn notes_preserved_when_label_contains_page_char() {
        // 章节名含「页」字的星，其用户批注必须跨重建保留（回归 extract_page rfind 修复）。
        let stars = vec![(5, "第三页的秘密".into())];
        let p1 = render_card("书", &stars, &CardModel::default());
        let text = p1.join("\n").replace("· 第 5 页", "· 第 5 页\n  这页的关键批注");
        let prev = parse_card(&text);
        assert!(
            prev.notes_by_page.get(&5).map(|v| v.join("\n")).unwrap_or_default().contains("这页的关键批注"),
            "含「页」字章名的星，批注应按页号保留而非丢弃: {:?}",
            prev.notes_by_page
        );
        let p2 = render_card("书", &stars, &prev);
        assert!(p2.join("\n").contains("这页的关键批注"), "重建后批注应仍在");
    }

    #[test]
    fn first_build_from_empty() {
        let prev = CardModel::default();
        let stars = vec![(3, "第一章《黑白》".into()), (8, "第二章《困境》".into())];
        let pages = render_card("13·67", &stars, &prev);
        let text = pages.join("\n");
        assert!(text.contains("《13·67》- 总结卡片"));
        assert!(text.contains("★ 第一章《黑白》 · 第 3 页"), "{text}");
        assert!(text.contains("★ 第二章《困境》 · 第 8 页"));
        assert!(!text.contains(FREE_HEADER), "不应有自由区页");
    }

    #[test]
    fn user_notes_preserved_across_regen() {
        // 首建
        let stars1 = vec![(3, "".into()), (8, "".into())];
        let pages1 = render_card("测试书", &stars1, &CardModel::default());
        // 用户在第 3 页待办下打字（模拟读回）
        let text = pages1.join("\n").replace("★ 第 3 页", "★ 第 3 页\n  这里是关键线索，记一下");
        // 重建（星集不变）
        let prev = parse_card(&text);
        assert!(
            prev.notes_by_page.get(&3).map(|v| v.join("\n")).unwrap_or_default().contains("这里是关键线索，记一下"),
            "第3页批注应被解析保留"
        );
        let pages2 = render_card("测试书", &stars1, &prev);
        let text2 = pages2.join("\n");
        assert!(text2.contains("这里是关键线索，记一下"), "第3页批注应保留: {text2}");
    }

    #[test]
    fn new_star_added_old_notes_kept() {
        let stars1 = vec![(3, "".into())];
        let p1 = render_card("书", &stars1, &CardModel::default());
        let text = p1.join("\n").replace("★ 第 3 页", "★ 第 3 页\n  批注A");
        let prev = parse_card(&text);
        // 新增第 5 页星
        let stars2 = vec![(3, "".into()), (5, "".into())];
        let p2 = render_card("书", &stars2, &prev);
        let t2 = p2.join("\n");
        assert!(t2.contains("★ 第 3 页"));
        assert!(t2.contains("批注A"), "老批注保留");
        assert!(t2.contains("★ 第 5 页"), "新星出现");
    }

    #[test]
    fn removed_star_page_disappears() {
        let stars1 = vec![(3, "".into()), (8, "".into())];
        let p1 = render_card("书", &stars1, &CardModel::default());
        let text = p1.join("\n").replace("★ 第 8 页", "★ 第 8 页\n  第8页的批注");
        let prev = parse_card(&text);
        // 第 8 页的星被擦掉 → 该页连同批注一起消失（star=待办本身）
        let stars2 = vec![(3, "".into())];
        let p2 = render_card("书", &stars2, &prev);
        let t2 = p2.join("\n");
        assert_eq!(p2.len(), 1, "只剩第3页一张");
        assert!(!t2.contains("★ 第 8 页"), "第8页应消失");
        assert!(t2.contains("★ 第 3 页"), "第3页还在");
    }

    #[test]
    fn one_page_per_star_no_free() {
        let stars = vec![(3, "第一章".into()), (8, "第二章".into()), (15, "".into())];
        let pages = render_card("书", &stars, &CardModel::default());
        assert_eq!(pages.len(), stars.len(), "应恰好 3 张星页、无自由页");
        // 第一张带标题+待办头+第一条星
        assert!(pages[0].contains("《书》- 总结卡片") && pages[0].contains(TODO_HEADER));
        assert!(pages[0].contains("★ 第一章 · 第 3 页"));
        // 中间每张一条星
        assert!(pages[1].starts_with("★ 第二章 · 第 8 页") && !pages[1].contains(TODO_HEADER));
        assert!(pages[2].starts_with("★ 第 15 页"));
        // 批注按页挂：模拟第8页打字后重建仍一星一页
        let text = pages.join("\n").replace("★ 第二章 · 第 8 页", "★ 第二章 · 第 8 页\n第8页批注");
        let prev = parse_card(&text);
        let p2 = render_card("书", &stars, &prev);
        assert_eq!(p2.len(), 3);
        assert!(p2[1].contains("第8页批注"), "第8页批注应在其卡片页: {}", p2[1]);
    }

    #[test]
    fn templates_render_distinct_fields() {
        let stars = &[(3, "第二章".to_string())];
        let empty = CardModel::default();
        // 默认（render_card）= 通用·原子卡。
        let gen = render_card("书", stars, &empty).join("\n");
        assert!(gen.contains("[ID: 第二章 - P 3]"), "锚点保留: {gen}");
        // 6 色槽都在（固定 emoji），提示词用 General 说法。
        for e in ["🟡", "🔵", "🩷", "🟠", "🟢", "⚪"] {
            assert!(gen.contains(e), "槽 {e} 应在: {gen}");
        }
        assert!(gen.contains("金句·要记的句") && gen.contains("洞见"), "通用槽提示词: {gen}");
        assert!(!gen.contains("势力阵营") && !gen.contains("案件·诡计代号") && !gen.contains("生词"), "默认不应带特殊模板提示词: {gen}");
        // 原文·英文原版（sentence mining）：槽提示词换成原文说法。
        let orig = render_card_with("书", stars, &empty, &[CardTemplate::Original]).join("\n");
        assert!(orig.contains("原句") && orig.contains("生词/短语") && orig.contains("地道表达"), "原文槽提示词: {orig}");
        // 悬疑·推理。
        let mys = render_card_with("书", stars, &empty, &[CardTemplate::Mystery]).join("\n");
        assert!(mys.contains("案件·诡计代号") && mys.contains("伏笔/破局链") && mys.contains("关键人物"), "悬疑槽提示词: {mys}");
        // 科幻·世界观制衡。
        let sf = render_card_with("书", stars, &empty, &[CardTemplate::SciFi]).join("\n");
        assert!(sf.contains("势力阵营") && sf.contains("权力/资源制衡") && sf.contains("关键设定/技术"), "科幻槽提示词: {sf}");
        // Tag → 模板映射（含别名、`#` 前缀、大小写）。
        assert_eq!(CardTemplate::from_tag("#悬疑"), Some(CardTemplate::Mystery));
        assert_eq!(CardTemplate::from_tag("原文"), Some(CardTemplate::Original));
        assert_eq!(CardTemplate::from_tag("SciFi"), Some(CardTemplate::SciFi));
        assert_eq!(CardTemplate::from_tag("通用"), Some(CardTemplate::General));
        assert_eq!(CardTemplate::from_tag("随便"), None);
    }

    #[test]
    fn multi_template_merge_dedups_and_is_order_stable() {
        let stars = &[(3, "".to_string())];
        let empty = CardModel::default();
        // 英文原版悬疑 = #原文 + #悬疑：同槽提示词合并（如 Yellow 槽「原句/金句」、Gray 槽「生词/短语/关键人物」）。
        let merged = render_card_with("书", stars, &empty, &[CardTemplate::Original, CardTemplate::Mystery]).join("\n");
        assert!(merged.contains("🟡 原句/金句："), "黄槽应合并原文+悬疑说法: {merged}");
        assert!(merged.contains("生词/短语/关键人物"), "灰槽应合并两模板说法: {merged}");
        // 仍是 6 个槽 + 1 条关联行（类型性收敛，不因多 Tag 增槽）。
        assert_eq!(merged.matches("🔗 关联 → ID：").count(), 1, "关联行仅 1 条: {merged}");
        assert_eq!(merged.matches("🟡").count(), 1, "黄槽仅 1 个（收敛不重复）: {merged}");
        // 规范序稳定：Tag 书写顺序颠倒，输出逐字相同（幂等）。
        let reversed = render_card_with("书", stars, &empty, &[CardTemplate::Mystery, CardTemplate::Original]).join("\n");
        assert_eq!(merged, reversed, "合并输出应与 Tag 顺序无关");
        // 空集退默认 General。
        let none = render_card_with("书", stars, &empty, &[]).join("\n");
        assert!(none.contains("金句·要记的句"), "空集应退默认通用: {none}");
    }

    #[test]
    fn highlights_fill_color_slots() {
        // 新星带高亮：黄槽 1 段、灰槽 2 段（非连续画线→同槽多行，需求②）。
        let empty = CardModel::default();
        let mut hl: Vec<Vec<String>> = vec![Vec::new(); 6];
        hl[0] = vec!["骆督察一直很讨厌医院的气味".to_string()];
        hl[5] = vec!["关键人物甲".to_string(), "关键人物乙".to_string()];
        let stars = vec![(10usize, "第一章".to_string(), vec![CardTemplate::Original], hl)];
        let out = render_card_starspecs("13·67", &stars, &empty).join("\n");
        // 黄槽（原文说法「原句」）下填入该段高亮。
        assert!(out.contains("🟡 原句：\n   · 骆督察一直很讨厌医院的气味"), "黄槽应填高亮: {out}");
        // 灰槽两段各占一行。
        assert!(out.contains("   · 关键人物甲") && out.contains("   · 关键人物乙"), "灰槽应多行: {out}");
        // 没画色的槽（蓝）不出现 · 行。
        assert!(out.contains("🔵 可意译/触发理解处："), "空蓝槽仍有标题: {out}");
        // 老星增量合并：先画星→空/半模板→再画线，新高亮补进对应槽，已有内容不动、不重复。
        let prev = parse_pages(&out.lines().map(String::from).collect::<Vec<_>>());
        let mut hl2: Vec<Vec<String>> = vec![Vec::new(); 6];
        hl2[0] = vec!["骆督察一直很讨厌医院的气味".to_string(), "新画的黄槽句".to_string()]; // 老1条+新1条
        hl2[5] = vec!["关键人物甲".to_string(), "新人物丙".to_string()];
        let stars2 = vec![(10usize, "第一章".to_string(), vec![CardTemplate::Original], hl2)];
        let out2 = render_card_starspecs("13·67", &stars2, &prev).join("\n");
        assert!(out2.contains("   · 新画的黄槽句"), "老星应把新黄槽高亮补进来: {out2}");
        assert!(out2.contains("   · 新人物丙"), "老星应把新灰槽高亮补进来: {out2}");
        assert_eq!(out2.matches("骆督察一直很讨厌医院的气味").count(), 1, "已有高亮不应重复: {out2}");
        assert_eq!(out2.matches("关键人物甲").count(), 1, "已有高亮不应重复: {out2}");
    }

    #[test]
    fn merge_preserves_typed_notes_between_highlights() {
        // 老星块里用户在槽内打了字（非 bullet 行）——合并新高亮时该行必须原样保留。
        let empty = CardModel::default();
        let mut hl: Vec<Vec<String>> = vec![Vec::new(); 6];
        hl[0] = vec!["原有高亮".to_string()];
        let stars = vec![(3usize, "".to_string(), vec![CardTemplate::General], hl)];
        let mut lines: Vec<String> = render_card_starspecs("书", &stars, &empty).join("\n").lines().map(String::from).collect();
        // 在黄槽已有 bullet 后插一行用户批注
        let pos = lines.iter().position(|l| l.contains("原有高亮")).unwrap();
        lines.insert(pos + 1, "这是我打的想法".to_string());
        let prev = parse_pages(&lines);
        let mut hl2: Vec<Vec<String>> = vec![Vec::new(); 6];
        hl2[0] = vec!["原有高亮".to_string(), "新高亮".to_string()];
        let stars2 = vec![(3usize, "".to_string(), vec![CardTemplate::General], hl2)];
        let out2 = render_card_starspecs("书", &stars2, &prev).join("\n");
        assert!(out2.contains("这是我打的想法"), "用户批注必须保留: {out2}");
        assert!(out2.contains("   · 新高亮"), "新高亮应补进黄槽: {out2}");
        assert_eq!(out2.matches("原有高亮").count(), 1, "已有高亮不重复: {out2}");
    }

    #[test]
    fn empty_slots_render_skeleton_only() {
        // 划线摘录=关：全空高亮槽 → 卡片仍出星骨架（6 色槽头 + 提示词），但无任何 · 高亮行。
        let empty = CardModel::default();
        let hl: Vec<Vec<String>> = vec![Vec::new(); 6]; // 空槽（daemon 在 collect_highlights=false 时传这个）
        let stars = vec![(10usize, "第一章".to_string(), vec![CardTemplate::Original], hl)];
        let out = render_card_starspecs("13·67", &stars, &empty).join("\n");
        assert!(out.contains("🟡 原句："), "空槽仍应有槽头: {out}");
        assert!(out.contains("[ID: 第一章 - P 10]"), "星骨架锚点应在: {out}");
        assert!(!out.contains("\n   · "), "划线摘录关：不应有任何 · 高亮行: {out}");
    }

    #[test]
    fn no_chapter_anchor_uses_book_prefix() {
        // 无章节（label 空）→ 锚点带书名短前缀消歧，不再是裸 [ID: p页号]。
        let a = render_card("我24岁患帕金森12年", &[(1, "".into())], &CardModel::default()).join("\n");
        assert!(a.contains("[ID: 我24岁患帕金森12年 - P 1]"), "无章节应带书名前缀: {a}");
        assert!(!a.contains("[ID: P 1]"), "不应退回裸页号（应带书名前缀）");
        // 另一本无章节书的 p1 前缀不同 → 跨书不撞号。
        let b = render_card("另一本文章", &[(1, "".into())], &CardModel::default()).join("\n");
        assert!(b.contains("[ID: 另一本文章 - P 1]"));
        // 超长书名截断到 16 字符（按字符非字节，不切坏多字节）。
        let long = "一二三四五六七八九十甲乙丙丁戊己庚辛";
        let c = render_card(long, &[(2, "".into())], &CardModel::default()).join("\n");
        assert!(c.contains("[ID: 一二三四五六七八九十甲乙丙丁戊己 - P 2]"), "应截前16字符: {c}");
        // 有章名时仍走 label，不加书名前缀（向后兼容旧锚点）。
        let d = render_card("任意书", &[(3, "第一章".into())], &CardModel::default()).join("\n");
        assert!(d.contains("[ID: 第一章 - P 3]"), "有章名走 label: {d}");
    }

    #[test]
    fn idempotent_no_change() {
        let stars = vec![(3, "章".into())];
        let p1 = render_card("书", &stars, &CardModel::default());
        let prev = parse_card(&p1.join("\n"));
        let p2 = render_card("书", &stars, &prev);
        assert_eq!(p1.join("\n"), p2.join("\n"), "无改动重建应完全一致（幂等）");
    }

    #[test]
    fn merge_inline_appended_annotation_not_duplicated() {
        // cardhw 把手写批注内联拼在书摘 bullet 行尾（"只想睡觉 但很累了"）。
        // 重建时该书摘仍在 hl 里 → 不能被当"缺失"重复插。t 后跟空格才算已在。
        let prev = vec![
            "🟡 金句·要记的句：".to_string(),
            "   · 只想睡觉 但很累了".to_string(),
            "🔵 洞见：".to_string(),
            "   · 她站在候车队伍中 一脸茫然".to_string(),
            "🔗 关联 → ID：".to_string(),
        ];
        let mut hl: Vec<Vec<String>> = vec![Vec::new(); 6];
        hl[0] = vec!["只想睡觉".into()]; // 🟡 槽书摘
        hl[1] = vec!["她站在候车队伍中".into()]; // 🔵 槽书摘
        let out = merge_highlights(&prev, &hl);
        let joined = out.join("\n");
        assert_eq!(joined.matches("只想睡觉").count(), 1, "被批注的书摘不该重复插");
        assert_eq!(joined.matches("她站在候车队伍中").count(), 1, "同上");
        assert!(joined.contains("· 只想睡觉 但很累了"), "内联批注原样保留");
    }

    #[test]
    fn merge_prefix_no_false_dedup() {
        // "她"（短书摘）不该因为存在 "她站在…"（无空格前缀）而被误判为已在。
        let prev = vec![
            "🟡 金句·要记的句：".to_string(),
            "   · 她站在候车队伍中".to_string(),
            "🔗 关联 → ID：".to_string(),
        ];
        let mut hl: Vec<Vec<String>> = vec![Vec::new(); 6];
        hl[0] = vec!["她站在候车队伍中".into(), "她".into()];
        let out = merge_highlights(&prev, &hl);
        assert!(out.iter().any(|l| l.trim() == "· 她"), "短书摘'她'仍应被补齐（无空格不误配）");
    }
}
