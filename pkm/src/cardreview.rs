//! 渐进总结 · 复盘队列（纯逻辑，host 可测）。
//!
//! Zettelkasten 的核心不是"捕获"而是"提炼"——把生摘录逐张用自己的话内化。荧光笔摄取只完成了捕获，
//! 卡片槽里躺着一堆书中原文（`· 高亮`），但**用户还没写自己的想法**=未消化。本能力扫全库卡片，
//! 挑出「有摘录、无提炼」的卡，汇成一本「📖 复盘队列」提醒定期回顾（对应方法论 §01 的 Review 环节）。
//! 用户在某张卡的槽下打字写下自己的话后，重扫检测到批注 → 该卡从队列消失（渐进驱动，队列会自然收敛）。
//!
//! 判据全部从**卡片文本**得（不需时间戳）：一张卡（一颗 ★ 块）=
//!   `has_highlight`（色槽下有 `· 高亮`）且 `!has_note`（除骨架/高亮外无用户打的字）→ 待消化。

use crate::cardagg::{book_of_title, page_from_id_line, SECTION_EMOJI, TITLE_SUFFIX};

/// 一张待消化卡。
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    pub book: String,
    pub page: String,
    pub hl_count: usize, // 摘录条数（· 高亮行数），提示分量
}

/// 卡片文本里的"骨架/摘录行"（非用户提炼想法）：标题/待办头/★行/[ID:]/槽标题/🔗关联/`· …`/空行。
/// 反之（非空且不匹配）= 用户打字写的提炼想法 → 该卡已开始消化。
fn is_scaffold_or_highlight(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return true;
    }
    if t.ends_with(TITLE_SUFFIX) || t.starts_with("━━") || t.starts_with('★') || t.starts_with("[ID:") || t.starts_with("🔗") || t.starts_with('·') {
        return true;
    }
    SECTION_EMOJI.iter().any(|e| t.starts_with(e))
}

/// 行是否是色槽标题（进入一个色槽上下文，其下的 `· ` 才算摘录高亮，🔗 下的 `· ` 不算）。
fn is_color_slot(line: &str) -> bool {
    let t = line.trim();
    SECTION_EMOJI.iter().any(|e| t.starts_with(e))
}

/// 解析一本卡片本 → 待消化卡列表。非总结卡片本返回空。按 ★ 块切分，逐块判 (has_highlight, has_note)。
pub fn parse_review(title: &str, text: &str) -> Vec<ReviewItem> {
    if !title.trim().ends_with(TITLE_SUFFIX) {
        return Vec::new();
    }
    let book = book_of_title(title);
    let mut out = Vec::new();
    // 逐块：遇 ★ 行开新块。块内累计 hl_count / has_note / 当前是否色槽上下文 / page。
    let mut in_block = false;
    let mut page = String::new();
    let mut hl = 0usize;
    let mut has_note = false;
    let mut in_color_slot = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('★') {
            if in_block && hl > 0 && !has_note {
                out.push(ReviewItem { book: book.clone(), page: page.clone(), hl_count: hl });
            }
            in_block = true;
            page = String::new();
            hl = 0;
            has_note = false;
            in_color_slot = false;
            continue;
        }
        if !in_block {
            continue; // 标题/待办头等块前内容
        }
        if t.starts_with("[ID:") {
            if let Some(p) = page_from_id_line(line) {
                page = p;
            }
            continue;
        }
        if t.starts_with("🔗") {
            in_color_slot = false; // 关联行之后 · 不算摘录
            continue;
        }
        if is_color_slot(line) {
            in_color_slot = true;
            continue;
        }
        if t.starts_with('·') {
            if in_color_slot {
                hl += 1; // 色槽下的 · = 摘录高亮
            }
            continue;
        }
        // 到这里：非空 && 非骨架/摘录 → 用户提炼想法。
        if !is_scaffold_or_highlight(line) {
            has_note = true;
        }
    }
    if in_block && hl > 0 && !has_note {
        out.push(ReviewItem { book: book.clone(), page, hl_count: hl });
    }
    out
}

/// 聚合全库卡片本 → 待消化卡（按书序×页序）。
pub fn build_review(docs: &[(String, String)]) -> Vec<ReviewItem> {
    let mut out = Vec::new();
    for (title, text) in docs {
        out.extend(parse_review(title, text));
    }
    out
}

/// 渲染「📖 复盘队列」页文本（按书分组）。**不含 `[ID:]`/触发词**（防自摄取，同 cardindex/cardagg）；
/// 来源用 `《书》P页`。空队列打「✓ 全部已消化」。
pub fn render_notebook_pages(items: &[ReviewItem]) -> Vec<String> {
    let books: std::collections::BTreeSet<&str> = items.iter().map(|i| i.book.as_str()).collect();
    let mut s = String::new();
    s.push_str("📖 复盘队列 · 待消化卡片\n");
    s.push_str("🔔 DORAEMON 魔法生成 · 只读 —— 把某张卡的摘录用自己的话在槽下写下想法，它就把这张变没（其余别抢笔，一动就变回去）\n\n");
    s.push_str(&format!("{} 张待消化 · {} 本书\n", items.len(), books.len()));
    if items.is_empty() {
        s.push_str("\n✓ 全部已消化——没有「有摘录、无提炼」的卡片\n");
        return vec![s];
    }
    // 按书分组，保持首次出现顺序。
    let mut order: Vec<&str> = Vec::new();
    for i in items {
        if !order.contains(&i.book.as_str()) {
            order.push(&i.book);
        }
    }
    for book in order {
        s.push_str(&format!("\n━━ 《{book}》 ━━\n"));
        for i in items.iter().filter(|i| i.book == book) {
            let loc = if i.page.is_empty() { String::new() } else { format!("P{} ", i.page) };
            s.push_str(&format!("· {}{} 段摘录待提炼\n", loc, i.hl_count));
        }
    }
    vec![s]
}

#[cfg(test)]
mod tests {
    use super::*;

    // 一张"有摘录无提炼"的卡（待消化）。
    fn raw_card() -> (String, String) {
        (
            "《13 67》- 总结卡片".to_string(),
            "《13 67》- 总结卡片\n━━ ★ 本书待办 ━━\n★ 黑与白 · 第 10 页\n[ID: 黑与白 - P 10]\n🟡 金句：\n   · 骆督察\n   · 一直\n🔵 洞见：\n🔗 关联 → ID：".to_string(),
        )
    }

    #[test]
    fn raw_card_is_queued() {
        let (t, x) = raw_card();
        let items = parse_review(&t, &x);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], ReviewItem { book: "13 67".into(), page: "10".into(), hl_count: 2 });
    }

    #[test]
    fn digested_card_not_queued() {
        // 用户在洞见槽下写了想法 → 已消化，出队列。
        let (t, x) = raw_card();
        let digested = x.replace("🔵 洞见：", "🔵 洞见：\n这段说的是职业倦怠，和我上周想的一致");
        assert!(parse_review(&t, &digested).is_empty(), "有用户批注的卡不应进复盘队列");
    }

    #[test]
    fn no_highlight_card_not_queued() {
        // 纯空骨架卡（画星但没画高亮）→ 不算"待消化"（那是待读，不是待提炼）。
        let t = "《书》- 总结卡片".to_string();
        let x = "《书》- 总结卡片\n★ 章 · 第 1 页\n[ID: 章 - P 1]\n🟡 金句：\n🔵 洞见：\n🔗 关联 → ID：".to_string();
        assert!(parse_review(&t, &x).is_empty(), "无摘录的空卡不进队列");
    }

    #[test]
    fn link_line_dot_not_counted_as_highlight() {
        // 🔗 关联行下用户填的 · 链接不算摘录高亮。
        let t = "《书》- 总结卡片".to_string();
        let x = "《书》- 总结卡片\n★ 章 · 第 1 页\n[ID: 章 - P 1]\n🟡 金句：\n🔗 关联 → ID：\n   · 某链接".to_string();
        assert!(parse_review(&t, &x).is_empty(), "🔗 下的 · 不是摘录，无摘录不进队列");
    }

    #[test]
    fn render_groups_and_empty() {
        let items = vec![
            ReviewItem { book: "13 67".into(), page: "10".into(), hl_count: 6 },
            ReviewItem { book: "赎罪".into(), page: "8".into(), hl_count: 3 },
        ];
        let p = render_notebook_pages(&items).join("\n");
        assert!(p.contains("2 张待消化 · 2 本书"));
        assert!(p.contains("━━ 《13 67》 ━━\n· P10 6 段摘录待提炼"));
        assert!(p.contains("━━ 《赎罪》 ━━\n· P8 3 段摘录待提炼"));
        assert!(!p.contains("[ID:"), "复盘本正文不含 [ID:");
        assert!(render_notebook_pages(&[]).join("\n").contains("✓ 全部已消化"));
    }
}
