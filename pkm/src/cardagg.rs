//! 跨书按色聚合（纯逻辑，host 可测）。
//!
//! 全库每本「总结卡片」的 6 色槽里摘的高亮，按**颜色**横向汇成一本「🎨 高亮汇编」（1 本 6 节）：
//! 所有书的 🟡 金句汇一节、🩷 疑问汇一节……对齐用户方法论 §01「整本读完按标签 Filter 聚合」，
//! 但聚合的是**高亮文字**（原生 Tag Filter 只能筛文档、筛不到高亮内容）。daemon 只读卡片本文本、
//! 不碰笔迹；汇编本自身被 collect 排除、正文不含 `[ID:]`/触发词，双保险防自摄取（同 cardindex）。
//!
//! 只聚合 daemon 填的 `· 高亮` 行（书中原文），不动用户在槽下手写打字的私人批注。

/// 6 色节名（与 `cardhl::HlColor::slot` / `cardsync::SLOT_EMOJI` 同序）。
pub const SECTION_EMOJI: [&str; 6] = ["🟡", "🔵", "🩷", "🟠", "🟢", "⚪"];
pub const SECTION_NAME: [&str; 6] = ["金句", "洞见", "疑问", "主题", "关联", "人物"];

/// 卡片本标题后缀（只聚合总结卡片本，忽略 MOC / 汇编本自身）。
const CARD_SUFFIX: &str = "- 总结卡片";

/// 一条聚合项。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub slot: usize,
    pub text: String,
    pub book: String,
    pub page: String, // 显示用页号串（如 "10"），空则未知
}

/// 从卡片本标题取书名：`《13 67》- 总结卡片` → `13 67`。
fn book_of_title(title: &str) -> String {
    title
        .trim()
        .strip_suffix(CARD_SUFFIX)
        .unwrap_or(title)
        .trim()
        .trim_start_matches('《')
        .trim_end_matches('》')
        .trim()
        .to_string()
}

/// 行首 emoji → 槽序（非槽标题返回 None）。
fn slot_of_line(line: &str) -> Option<usize> {
    let t = line.trim_start();
    SECTION_EMOJI.iter().position(|e| t.starts_with(e))
}

/// 从 `[ID: 章名 - P 10]` 抽页号串（找 `- P ` 后到 `]` 或行尾的数字）。
fn page_from_id_line(line: &str) -> Option<String> {
    let p = line.find("- P ")?;
    let after = &line[p + "- P ".len()..];
    let end = after.find(']').unwrap_or(after.len());
    let s = after[..end].trim();
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        Some(s.to_string())
    } else {
        None
    }
}

/// 解析一本卡片本（全页 join 文本）→ 聚合项。非总结卡片本（无后缀）返回空。
pub fn parse_card(title: &str, text: &str) -> Vec<Entry> {
    if !title.trim().ends_with(CARD_SUFFIX) {
        return Vec::new();
    }
    let book = book_of_title(title);
    let mut page = String::new();
    let mut slot: Option<usize> = None;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.contains("[ID:") {
            if let Some(p) = page_from_id_line(line) {
                page = p;
            }
        }
        if let Some(s) = slot_of_line(line) {
            slot = Some(s);
            continue;
        }
        if line.trim_start().starts_with("🔗") {
            slot = None; // 关联行不是色槽
            continue;
        }
        // daemon 填的高亮行：`   · 文字`
        if let Some(rest) = line.trim_start().strip_prefix("· ") {
            if let Some(s) = slot {
                let c = rest.trim();
                if !c.is_empty() {
                    out.push(Entry { slot: s, text: c.to_string(), book: book.clone(), page: page.clone() });
                }
            }
        }
    }
    out
}

/// 聚合全库卡片本：`docs`=(笔记本名, 全页文本)。返回 6 槽各自的条目（按加入顺序，即书序×页序）。
pub fn build_agg(docs: &[(String, String)]) -> [Vec<Entry>; 6] {
    let mut slots: [Vec<Entry>; 6] = Default::default();
    for (title, text) in docs {
        for e in parse_card(title, text) {
            slots[e.slot].push(e);
        }
    }
    slots
}

/// 渲染成设备端「🎨 高亮汇编」笔记本页文本（当前单页，6 色节）。**刻意不含 `[ID:]` 括号 +
/// 不含「指向/链接…ID:」触发词** → 被 collect 按名排除 + 正文避开触发式样，双保险防自摄取/死链自指。
/// 来源标注用 `—《书》P页`（非 `[ID:]`）。
pub fn render_notebook_pages(slots: &[Vec<Entry>; 6]) -> Vec<String> {
    let total: usize = slots.iter().map(|v| v.len()).sum();
    let books: std::collections::BTreeSet<&str> =
        slots.iter().flatten().map(|e| e.book.as_str()).collect();
    let mut s = String::new();
    s.push_str("🎨 高亮汇编 · 全库同色聚合\n");
    s.push_str("（daemon 自动生成·只读；勿在此本手写，重建会覆盖）\n\n");
    s.push_str(&format!("{total} 条高亮 · {} 本书\n", books.len()));
    for slot in 0..6 {
        s.push_str(&format!("\n━━ {} {} ━━\n", SECTION_EMOJI[slot], SECTION_NAME[slot]));
        if slots[slot].is_empty() {
            s.push_str("（空）\n");
            continue;
        }
        for e in &slots[slot] {
            let src = if e.page.is_empty() {
                format!("—《{}》", e.book)
            } else {
                format!("—《{}》P{}", e.book, e.page)
            };
            s.push_str(&format!("· {}  {}\n", e.text, src));
        }
    }
    vec![s]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_card() -> (String, String) {
        (
            "《13 67》- 总结卡片".to_string(),
            "《13 67》- 总结卡片\n━━ ★ 本书待办 ━━\n★ 黑与白 · 第 10 页\n[ID: 黑与白 - P 10]\n🟡 金句·要记的句：\n   · 骆督察\n🔵 洞见：\n   · 一直\n⚪ 人物·其它：\n   · 就是那股消毒药水的气味\n🔗 关联 → ID：\n   · 指向 → ID：不该被当高亮".to_string(),
        )
    }

    #[test]
    fn parse_extracts_slot_book_page() {
        let (t, x) = sample_card();
        let es = parse_card(&t, &x);
        assert_eq!(es.len(), 3, "3 条高亮(关联行下的不算): {es:?}");
        assert_eq!(es[0], Entry { slot: 0, text: "骆督察".into(), book: "13 67".into(), page: "10".into() });
        assert_eq!(es[1].slot, 1);
        assert_eq!(es[2].slot, 5);
        // 🔗 关联行之后的 · 行不被当高亮（slot=None）。
        assert!(!es.iter().any(|e| e.text.contains("不该")), "关联行下不算高亮");
    }

    #[test]
    fn non_card_notebook_ignored() {
        assert!(parse_card("000-全局MOC", "🟡 金句：\n   · 混淆项").is_empty(), "非总结卡片本不聚合");
    }

    #[test]
    fn aggregate_and_render() {
        let (t, x) = sample_card();
        let book2 = ("《赎罪》- 总结卡片".to_string(), "《赎罪》- 总结卡片\n★ 洋娃娃 · 第 8 页\n[ID: 洋娃娃 - P 8]\n🟡 原句：\n   · 无能为力".to_string());
        let slots = build_agg(&[(t, x), book2]);
        // 🟡 槽跨两本书：骆督察 + 无能为力。
        assert_eq!(slots[0].len(), 2);
        let page = render_notebook_pages(&slots).join("\n");
        assert!(page.contains("━━ 🟡 金句 ━━"));
        assert!(page.contains("· 骆督察  —《13 67》P10"));
        assert!(page.contains("· 无能为力  —《赎罪》P8"));
        assert!(page.contains("· 就是那股消毒药水的气味  —《13 67》P10"));
        // 自摄取防线：正文不含 [ID: 括号。
        assert!(!page.contains("[ID:"), "汇编本正文不应含 [ID: 括号");
        // 空槽标「（空）」。
        assert!(page.contains("━━ 🟢 关联 ━━\n（空）"));
    }
}
