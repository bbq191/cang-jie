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

/// 从 ★ 行抽取页号：匹配 "第 N 页"（N 为数字）。抽不到返回 None。
pub fn extract_page(line: &str) -> Option<usize> {
    let idx = line.find('页')?;
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

/// 新出现的 ★ 页的初始模板（总结卡片《13.67》三段式结构，见 PKM 白皮书 §06）：线索框 / 逻辑推演网 / 标签锚点。
/// 只在**首次出现**该星时注入一次；之后用户在字段后打字，重建时整块作为「批注」逐字保留。
/// ID 带章名（label，形如「章名」或「章名 - 节名」）便于跨页软链接检索；label 空则只用页号。
fn page_scaffold(page: usize, label: &str) -> Vec<String> {
    let id = if label.is_empty() {
        format!("[ID: p{page}]")
    } else {
        format!("[ID: {label}-p{page}]")
    };
    vec![
        id,
        "〔线索框〕".to_string(),
        "🔵 时间节点：".to_string(),
        "🔴 核心实体：".to_string(),
        "🟢 案件代号：".to_string(),
        "〔逻辑推演网〕".to_string(),
        "　".to_string(),
        "〔标签与锚点〕".to_string(),
        "🔖 Tags：".to_string(),
        "🟡 金句：".to_string(),
        "🔗 指向 → ID：".to_string(),
    ]
}

/// 按当前星集重生成卡片：**每个有 ★ 的页各占一张卡片页**（一主题一卡，留整页给你批注）。
/// 无自由区页（按用户要求删除）。`stars`:(页号,章节标签[可空]) 已排序去重；`prev`:上一版可保留状态。
/// 星被擦掉 → 该页连同其批注一起消失（star=待办本身，擦星即完成）。
///
/// 页文本结构（join("\n") 后是连续流，parse_card 按 ★ 切块，与分页无关）：
///   第一张 = 标题 + TODO_HEADER + 第一条 ★ + 其批注；其后每张 = 一条 ★ + 其批注。
pub fn render_card(book_title: &str, stars: &[(usize, String)], prev: &CardModel) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    for (i, (page, label)) in stars.iter().enumerate() {
        let mut block: Vec<String> = Vec::new();
        if i == 0 {
            // 第一张卡片页带标题 + 待办区头（parse 靠 TODO_HEADER 定位待办区起点）。
            block.push(format!("《{book_title}》- 总结卡片"));
            block.push(String::new());
            block.push(TODO_HEADER.to_string());
        }
        block.push(star_header(*page, label));
        match prev.notes_by_page.get(page) {
            Some(notes) => block.extend(notes.iter().cloned()), // 老星：保留用户编辑
            None => block.extend(page_scaffold(*page, label)),  // 新星：注入总结卡片模板
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
    fn idempotent_no_change() {
        let stars = vec![(3, "章".into())];
        let p1 = render_card("书", &stars, &CardModel::default());
        let prev = parse_card(&p1.join("\n"));
        let p2 = render_card("书", &stars, &prev);
        assert_eq!(p1.join("\n"), p2.join("\n"), "无改动重建应完全一致（幂等）");
    }
}
