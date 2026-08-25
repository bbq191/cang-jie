//! 按书 PKM 仪表（纯逻辑，host 可测）。
//!
//! 扫全库卡片本，逐书统计**星数（=卡片数，一星一卡）· 高亮数（各色分布）· 待消化数**，汇成一本
//! 「📊 阅读仪表」——一眼看全库知识积累状态。全部从**卡片本文本**统计（只读、不碰笔迹、不需时间戳），
//! 与索引/汇编/复盘同一条 collect + sync_auto_notebook 链。
//!
//! 「待消化」判据同 cardreview：该卡有色槽摘录（`· 高亮`）但无用户提炼批注。

use crate::cardagg::{book_of_title, SECTION_EMOJI, TITLE_SUFFIX};

/// 一本书的统计。
#[derive(Debug, Clone, PartialEq)]
pub struct BookStats {
    pub book: String,
    pub stars: usize,          // ★ 数（=卡片页数）
    pub highlights: usize,     // 摘录高亮总数
    pub by_color: [usize; 6],  // 各色分布（Yellow..Gray）
    pub pending: usize,        // 待消化卡数（有摘录·无提炼）
}

/// 全库汇总。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub books: usize,
    pub stars: usize,
    pub highlights: usize,
    pub pending: usize,
}

/// 色槽标题 → 槽序。
fn color_slot(line: &str) -> Option<usize> {
    let t = line.trim();
    SECTION_EMOJI.iter().position(|e| t.starts_with(e))
}

/// 卡片文本里的骨架/摘录行（判 has_note 用；同 cardreview）。
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

/// 统计一本卡片本。非总结卡片本返回 None。
pub fn parse_stats(title: &str, text: &str) -> Option<BookStats> {
    if !title.trim().ends_with(TITLE_SUFFIX) {
        return None;
    }
    let book = book_of_title(title);
    let mut stars = 0usize;
    let mut by_color = [0usize; 6];
    let mut pending = 0usize;
    // 块内累计
    let mut in_block = false;
    let mut cur_slot: Option<usize> = None;
    let mut block_hl = 0usize;
    let mut block_note = false;
    let mut close = |block_hl: usize, block_note: bool, pending: &mut usize| {
        if block_hl > 0 && !block_note {
            *pending += 1;
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('★') {
            if in_block {
                close(block_hl, block_note, &mut pending);
            }
            stars += 1;
            in_block = true;
            cur_slot = None;
            block_hl = 0;
            block_note = false;
            continue;
        }
        if !in_block {
            continue;
        }
        if t.starts_with("🔗") {
            cur_slot = None;
            continue;
        }
        if let Some(s) = color_slot(line) {
            cur_slot = Some(s);
            continue;
        }
        if t.starts_with('·') {
            if let Some(s) = cur_slot {
                by_color[s] += 1;
                block_hl += 1;
            }
            continue;
        }
        if !is_scaffold_or_highlight(line) {
            block_note = true;
        }
    }
    if in_block {
        close(block_hl, block_note, &mut pending);
    }
    let highlights: usize = by_color.iter().sum();
    Some(BookStats { book, stars, highlights, by_color, pending })
}

/// 全库统计：返回 (每书统计[按星数降序、同数按书名], 汇总)。
pub fn build_stats(docs: &[(String, String)]) -> (Vec<BookStats>, Totals) {
    let mut books: Vec<BookStats> = docs.iter().filter_map(|(t, x)| parse_stats(t, x)).filter(|b| b.stars > 0).collect();
    books.sort_by(|a, b| b.stars.cmp(&a.stars).then(a.book.cmp(&b.book)));
    let totals = Totals {
        books: books.len(),
        stars: books.iter().map(|b| b.stars).sum(),
        highlights: books.iter().map(|b| b.highlights).sum(),
        pending: books.iter().map(|b| b.pending).sum(),
    };
    (books, totals)
}

/// 渲染「📊 阅读仪表」页文本。**不含 `[ID:]`/触发词**（防自摄取，同 cardindex/cardagg/cardreview）。
pub fn render_notebook_pages(books: &[BookStats], totals: &Totals) -> Vec<String> {
    let mut s = String::new();
    s.push_str("📊 阅读仪表 · 全库 PKM 概览\n");
    s.push_str("（daemon 自动生成·只读）\n\n");
    s.push_str(&format!(
        "{} 本书 · {} 颗星 · {} 条高亮 · {} 张待消化\n",
        totals.books, totals.stars, totals.highlights, totals.pending
    ));
    if books.is_empty() {
        s.push_str("\n（暂无带 ★ 待办的书）\n");
        return vec![s];
    }
    for b in books {
        s.push_str(&format!("\n━━ 《{}》 ━━\n", b.book));
        s.push_str(&format!("★{} · 🖍{} 条高亮 · 📖{} 待消化\n", b.stars, b.highlights, b.pending));
        // 各色分布（只列非零，省空间）。
        let dist: Vec<String> = (0..6)
            .filter(|&i| b.by_color[i] > 0)
            .map(|i| format!("{}{}", SECTION_EMOJI[i], b.by_color[i]))
            .collect();
        if !dist.is_empty() {
            s.push_str(&format!("  {}\n", dist.join(" ")));
        }
    }
    vec![s]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards() -> Vec<(String, String)> {
        vec![
            (
                "《13 67》- 总结卡片".to_string(),
                "《13 67》- 总结卡片\n★ 黑与白 · 第 10 页\n[ID: 黑与白 - P 10]\n🟡 金句：\n   · 骆督察\n   · 一直\n🩷 疑问：\n   · 很讨厌\n🔗 关联 → ID：\n   · 某链接\n★ 第二章 · 第 20 页\n[ID: 第二章 - P 20]\n🟡 金句：\n   · 另一句\n这段是我的想法".to_string(),
            ),
            (
                "《赎罪》- 总结卡片".to_string(),
                "《赎罪》- 总结卡片\n★ 洋娃娃 · 第 8 页\n[ID: 洋娃娃 - P 8]\n🟠 主题：\n   · 无能为力".to_string(),
            ),
            ("000-全局MOC".to_string(), "非卡片本".to_string()),
        ]
    }

    #[test]
    fn per_book_stats() {
        let (books, _) = build_stats(&cards());
        // 《13 67》：2 星，高亮 = 黄2+粉1+黄1 = 4（🔗 下的 · 某链接不算），待消化 = 第1星(有摘录无提炼)=1（第2星有想法→不算）。
        let a = books.iter().find(|b| b.book == "13 67").unwrap();
        assert_eq!(a.stars, 2);
        assert_eq!(a.highlights, 4, "🔗 下的 · 不计入高亮");
        assert_eq!(a.by_color[0], 3); // 🟡 骆督察+一直+另一句
        assert_eq!(a.by_color[2], 1); // 🩷 很讨厌
        assert_eq!(a.pending, 1, "第1星待消化、第2星有想法不算");
    }

    #[test]
    fn totals_and_order() {
        let (books, totals) = build_stats(&cards());
        assert_eq!(totals.books, 2); // MOC 不算
        assert_eq!(totals.stars, 3); // 2+1
        assert_eq!(totals.highlights, 5); // 4+1
        assert_eq!(totals.pending, 2); // 13·67 第1星 + 赎罪
        assert_eq!(books[0].book, "13 67", "星多的书排前");
    }

    #[test]
    fn render_no_ingest_patterns() {
        let (books, totals) = build_stats(&cards());
        let p = render_notebook_pages(&books, &totals).join("\n");
        assert!(p.contains("2 本书 · 3 颗星 · 5 条高亮 · 2 张待消化"));
        assert!(p.contains("━━ 《13 67》 ━━\n★2 · 🖍4 条高亮 · 📖1 待消化"));
        assert!(p.contains("🟡3 🩷1"), "各色分布只列非零: {p}");
        assert!(!p.contains("[ID:"), "仪表正文不含 [ID:");
    }

    #[test]
    fn empty_library() {
        let (books, totals) = build_stats(&[]);
        let p = render_notebook_pages(&books, &totals).join("\n");
        assert!(p.contains("0 本书"));
        assert!(p.contains("暂无带 ★"));
    }
}
