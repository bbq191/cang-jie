//! 条目库 → 设备笔记本投影（纯函数，零 I/O）：把一章的条目按分区铺成 `rmv6::write::Paragraph` 列表，
//! note-serve 拿它去打包 `.rmdoc`、走 `/upload`。这里只管"排成什么样"，不管"怎么传"（那是
//! `note-serve::publish` 的事，出于"笔记本只读、由条目库投影生成"的原则，本模块不产生任何副作用）。
//!
//! 布局：Title=章名 → 按 `Section.order` 排的各分区 Subheading 1（名 + 简述）→ 分区内条目（按
//! `page_index` 排，样式=`Entry.style`，文本=`display_text()`，没转写占位"（待转写）"）→ 条目下各跟一段
//! Body 摘录勾画原文（有的话）与 AI 回答（有的话）。指向未知/已删分区的条目落一个"未分区"兜底段。
//! 已撤销（`Status::Revoked`）条目不投影；一章零条目 → `None`（调用方不该为空章生成文档）。
//!
//! ⚠️ 已知限制（先记录、不在本模块解决）：xochitl 按"连续几个 NUMBERED 段落"自动编号，摘录/回答的
//! Body 段插在两条 NUMBERED 条目之间会打断连续、导致编号从 1 重来——真要连续编号的清单，条目之间
//! 目前不能有摘录/回答。留给以后有真机样本再决定要不要为此改变编排。
//! 小节（`Entry.subhead`，来自 epubmap 的 h2/h3）暂不参与分组，只是简单按分区铺开——细分小节头
//! （Subheading 2）留作以后的精修，不是这次的阻塞项。
use crate::hash::{fnv1a, hex};
use crate::model::{Book, Entry, Status};
use rmv6::v6::scene_item::text::ParagraphStyle;
use rmv6::write::Paragraph;
use std::collections::BTreeSet;

fn style_to_wire(s: crate::model::Style) -> ParagraphStyle {
    match s {
        crate::model::Style::Body => ParagraphStyle::PLAIN,
        crate::model::Style::Bullet => ParagraphStyle::BULLET,
        crate::model::Style::Numbered => ParagraphStyle::NUMBERED,
        crate::model::Style::Checkbox => ParagraphStyle::CHECKBOX,
    }
}

/// 参与投影的条目：本章、未撤销，按页序排。
fn live_entries(book: &Book, chapter_idx: usize) -> Vec<&Entry> {
    let mut v: Vec<&Entry> = book.entries.iter().filter(|e| e.chapter == Some(chapter_idx) && e.status != Status::Revoked).collect();
    v.sort_by_key(|e| e.page_index);
    v
}

fn push_entry(out: &mut Vec<Paragraph>, e: &Entry) {
    let text = e.display_text().map(str::to_string).unwrap_or_else(|| "（待转写）".to_string());
    out.push(Paragraph::new(style_to_wire(e.style), text));
    if let Some(q) = &e.quote {
        if !q.text.trim().is_empty() {
            out.push(Paragraph::new(ParagraphStyle::PLAIN, format!("〔原文〕{}", q.text)));
        }
    }
    if let Some(a) = &e.answer {
        if !a.text.trim().is_empty() {
            out.push(Paragraph::new(ParagraphStyle::PLAIN, format!("〔AI〕{}", a.text)));
        }
    }
}

/// 投影一章。`None` = 没有这一章、或这一章没有可投影的条目（全撤销/没条目）。
pub fn project_chapter(book: &Book, chapter_idx: usize) -> Option<Vec<Paragraph>> {
    let title = book.chapters.get(chapter_idx)?;
    let entries = live_entries(book, chapter_idx);
    if entries.is_empty() {
        return None;
    }

    let mut sections = book.sections.clone();
    if sections.is_empty() {
        sections = crate::model::default_sections();
    }
    sections.sort_by_key(|s| s.order);

    let mut out = vec![Paragraph::new(ParagraphStyle::HEADING, title.clone())];
    for sec in &sections {
        let in_sec: Vec<&&Entry> = entries.iter().filter(|e| e.section.as_deref() == Some(sec.id.as_str())).collect();
        if in_sec.is_empty() {
            continue;
        }
        let head = if sec.brief.is_empty() { sec.name.clone() } else { format!("{}（{}）", sec.name, sec.brief) };
        out.push(Paragraph::subheading1(head));
        for e in in_sec {
            push_entry(&mut out, e);
        }
    }
    let known: BTreeSet<&str> = sections.iter().map(|s| s.id.as_str()).collect();
    let stray: Vec<&&Entry> = entries.iter().filter(|e| !e.section.as_deref().map(|s| known.contains(s)).unwrap_or(false)).collect();
    if !stray.is_empty() {
        out.push(Paragraph::subheading1("未分区"));
        for e in stray {
            push_entry(&mut out, e);
        }
    }
    Some(out)
}

/// 一章的投影指纹：只要它不变，笔记本内容不变，note-serve 不必重新生成/重新上传。覆盖投影用到的
/// 全部输入（章名、分区名/简述/顺序、条目的样式/分区/文本/原文/回答），任何一项变了指纹就变。
pub fn fingerprint_chapter(book: &Book, chapter_idx: usize) -> Option<String> {
    let title = book.chapters.get(chapter_idx)?;
    let entries = live_entries(book, chapter_idx);
    if entries.is_empty() {
        return None;
    }
    let mut sections = book.sections.clone();
    if sections.is_empty() {
        sections = crate::model::default_sections();
    }
    sections.sort_by_key(|s| s.order);

    let mut buf = String::new();
    buf.push_str(title);
    buf.push('\u{1}');
    for s in &sections {
        buf.push_str(&format!("{}|{}|{}|{}", s.id, s.name, s.brief, s.order));
        buf.push('\u{1}');
    }
    buf.push('\u{2}');
    for e in &entries {
        let text = e.display_text().unwrap_or("");
        let quote = e.quote.as_ref().map(|q| q.text.as_str()).unwrap_or("");
        let answer = e.answer.as_ref().map(|a| a.text.as_str()).unwrap_or("");
        buf.push_str(&format!("{}|{:?}|{}|{}|{}|{}", e.id, e.style, e.section.as_deref().unwrap_or(""), text, quote, answer));
        buf.push('\u{1}');
    }
    Some(hex(fnv1a(buf.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Answer, Draft, Quote, Section, Style};

    fn entry(id: &str, chapter: usize, page_index: usize, section: &str, style: Style, text: &str) -> Entry {
        Entry {
            id: id.into(),
            page: "p".into(),
            page_index,
            chapter: Some(chapter),
            chapter_title: String::new(),
            subhead: None,
            quote: None,
            ink: None,
            drafts: vec![],
            text: (!text.is_empty()).then(|| text.to_string()),
            style,
            section: (!section.is_empty()).then(|| section.to_string()),
            answer: None,
            status: Status::Reviewed,
            created: 0,
            updated: 0,
        }
    }

    fn book() -> Book {
        Book {
            uuid: "u".into(),
            title: "人骨拼图".into(),
            author: String::new(),
            chapters: vec!["第一章".into(), "第二章".into()],
            sections: vec![
                Section { id: "lookup".into(), name: "查询".into(), brief: "查人名/术语".into(), ai: true, order: 0, triggers: vec![] },
                Section { id: "explain".into(), name: "解释".into(), brief: String::new(), ai: true, order: 1, triggers: vec![] },
            ],
            entries: vec![
                entry("e1", 0, 2, "lookup", Style::Body, "林肯·莱姆"),
                entry("e2", 0, 1, "explain", Style::Bullet, "为什么是纽约"),
                entry("e3", 0, 5, "", Style::Body, "没归类的一条"),
                entry("e4", 1, 0, "lookup", Style::Numbered, "第二章的条目"),
            ],
            page_mtimes: Default::default(),
        }
    }

    #[test]
    fn no_chapter_or_empty_chapter_yields_none() {
        let b = book();
        assert!(project_chapter(&b, 9).is_none(), "没有第 9 章");
        assert!(project_chapter(&b, 2).is_none(), "第 2 章（index）不存在，只有 0/1");
        assert!(fingerprint_chapter(&b, 9).is_none());
    }

    #[test]
    fn orders_sections_by_order_then_entries_by_page_and_falls_back_to_stray() {
        let b = book();
        let ps = project_chapter(&b, 0).expect("第一章有条目");
        let texts: Vec<&str> = ps.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["第一章", "查询（查人名/术语）", "林肯·莱姆", "解释", "为什么是纽约", "未分区", "没归类的一条"]);
        assert_eq!(ps[0].style, ParagraphStyle::HEADING);
        assert_eq!(ps[1].style, ParagraphStyle::BOLD, "分区头是 subheading1，wire 码与裸 BOLD 相同");
        assert_eq!(ps[2].style, ParagraphStyle::PLAIN, "e1 是 Style::Body");
        assert_eq!(ps[4].style, ParagraphStyle::BULLET, "e2 是 Style::Bullet");
        assert_eq!(ps[6].style, ParagraphStyle::PLAIN, "未分区条目 e3 也是 Style::Body");

        let ps2 = project_chapter(&b, 1).expect("第二章有条目");
        assert_eq!(ps2.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["第二章", "查询（查人名/术语）", "第二章的条目"]);
        assert_eq!(ps2[2].style, ParagraphStyle::NUMBERED);
    }

    #[test]
    fn revoked_entries_are_excluded_from_projection_and_fingerprint() {
        let mut b = book();
        let fp_before = fingerprint_chapter(&b, 1).unwrap();
        b.entries[3].status = Status::Revoked;
        assert!(project_chapter(&b, 1).is_none(), "唯一条目撤销后第二章没内容可投影");
        assert!(fingerprint_chapter(&b, 1).is_none());
        // 撤销前后指纹一定不同（这里只需确认"有值"变"无值"，上面两条已覆盖）
        assert!(!fp_before.is_empty());
    }

    #[test]
    fn fingerprint_changes_when_text_or_section_changes_but_not_on_no_op_rebuild() {
        let b = book();
        let fp1 = fingerprint_chapter(&b, 0).unwrap();
        let fp2 = fingerprint_chapter(&b, 0).unwrap();
        assert_eq!(fp1, fp2, "同样的书两次算出同一个指纹");

        let mut edited = b.clone();
        edited.entries[0].text = Some("换了校对文本".into());
        assert_ne!(fingerprint_chapter(&edited, 0).unwrap(), fp1, "校对文本变了指纹要变");

        let mut moved = b.clone();
        moved.entries[0].section = Some("explain".into());
        assert_ne!(fingerprint_chapter(&moved, 0).unwrap(), fp1, "换分区了指纹要变");

        let mut with_answer = b.clone();
        with_answer.entries[0].answer = Some(Answer { text: "AI 说的话".into(), backend: "x".into(), at: 0, brief: String::new() });
        assert_ne!(fingerprint_chapter(&with_answer, 0).unwrap(), fp1, "AI 回答变了也算数");

        let mut with_draft_only = b.clone();
        with_draft_only.entries[0].text = None;
        with_draft_only.entries[0].drafts.push(Draft { text: "草稿文本".into(), backend: "x".into(), at: 0, hash: "h".into() });
        assert_ne!(fingerprint_chapter(&with_draft_only, 0).unwrap(), fp1, "校对文本换成草稿，display_text 变了指纹也要变");

        // quote 是 Option<Quote>，构造函数里没给，补一个验证也参与指纹
        let mut with_quote = b.clone();
        with_quote.entries[0].quote = Some(Quote { id: "q".into(), text: "原文摘录".into(), color: "yellow".into(), rects: vec![] });
        assert_ne!(fingerprint_chapter(&with_quote, 0).unwrap(), fp1, "勾画原文变了指纹也要变");
    }
}
