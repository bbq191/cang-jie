//! 一页解析结果 → 条目草稿 → 按增量规则并入书的条目库。
//! 增量规则（用户 2026-09-06 的担心"二次识别把您好覆盖回你好"）：
//! 1. 簇指纹不变 → 条目原样（不重转写、不动 `text`）；
//! 2. 簇与旧条目共享笔画但指纹变了（补了几笔）→ 同一条目，更新 `ink`，等新草稿作建议，`text` 不动；
//! 3. 旧条目的笔画全没了 → `Revoked`（留痕不删）；
//! 4. 全新簇 → 新条目，id 由 (书, 页, 最小笔画 id) 一次算定。
use crate::geom::{cluster, pair, Thresholds};
use crate::hash::{cluster_hash, entry_id};
use crate::model::{Entry, Ink, Quote, Status, Style};
use rmv6::page::Page;
use rmv6::v6::crdt::CrdtId;

fn sid(id: &CrdtId) -> String {
    format!("{}:{}", id.part1, id.part2)
}

fn color_name(c: &rmv6::shared::pen_color::PenColor, rgba: Option<(u8, u8, u8, u8)>) -> String {
    match (c, rgba) {
        (rmv6::shared::pen_color::PenColor::Unknown(_), Some((r, g, b, _))) => format!("#{r:02x}{g:02x}{b:02x}"),
        (c, _) => format!("{c:?}").to_ascii_lowercase(),
    }
}

/// 一页里认出的"一片手写（+ 它旁边的勾画）"。
#[derive(Debug, Clone, PartialEq)]
pub struct PageDraft {
    pub ink: Ink,
    pub quote: Option<Quote>,
}

pub fn drafts_of_page(page: &Page, th: &Thresholds) -> Vec<PageDraft> {
    let clusters = cluster(&page.strokes, th);
    let pairs = pair(&clusters, &page.highlights, th);
    clusters
        .iter()
        .zip(pairs)
        .map(|(c, hl)| {
            let items: Vec<(String, usize, (f32, f32, f32, f32))> = c.strokes.iter().map(|&i| { let s = &page.strokes[i]; (sid(&s.id), s.points.len(), (s.bbox.x0, s.bbox.y0, s.bbox.x1, s.bbox.y1)) }).collect();
            let refs: Vec<(&str, usize, (f32, f32, f32, f32))> = items.iter().map(|(id, n, b)| (id.as_str(), *n, *b)).collect();
            let mut strokes: Vec<String> = items.iter().map(|(id, _, _)| id.clone()).collect();
            strokes.sort();
            PageDraft {
                ink: Ink { strokes, bbox: (c.bbox.x0, c.bbox.y0, c.bbox.x1, c.bbox.y1), hash: cluster_hash(&refs), crop: String::new() },
                quote: hl.map(|i| { let h = &page.highlights[i]; Quote { text: h.text.clone(), color: color_name(&h.color, h.rgba), rects: h.rects.clone() } }),
            }
        })
        .collect()
}

/// 页级上下文（摄取时由 ink-serve 从 epubmap 算好）。
pub struct PageCtx<'a> {
    pub book: &'a str,
    pub page: &'a str,
    pub page_index: usize,
    pub chapter: Option<usize>,
    pub chapter_title: &'a str,
    pub subhead: Option<&'a str>,
    pub now: u64,
}

/// 合并结果统计（日志/事件用）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MergeStats {
    pub added: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub revoked: usize,
}

/// 把本页新草稿并入 `entries`（只动本页的条目）。
pub fn merge_page(entries: &mut Vec<Entry>, ctx: &PageCtx, drafts: Vec<PageDraft>) -> MergeStats {
    let mut st = MergeStats::default();
    let mut claimed = vec![false; entries.len()];
    for d in drafts {
        // 认领：先找指纹相同的，再找共享笔画的（本页、未撤销、未被本轮认领）
        let same_page = |e: &Entry| e.page == ctx.page && e.status != Status::Revoked;
        let hit = entries.iter().enumerate().find(|(i, e)| !claimed[*i] && same_page(e) && e.ink.as_ref().map(|k| k.hash == d.ink.hash).unwrap_or(false)).map(|(i, _)| i)
            .or_else(|| entries.iter().enumerate().find(|(i, e)| !claimed[*i] && same_page(e) && e.ink.as_ref().map(|k| k.strokes.iter().any(|s| d.ink.strokes.contains(s))).unwrap_or(false)).map(|(i, _)| i));
        match hit {
            Some(i) => {
                claimed[i] = true;
                let e = &mut entries[i];
                if e.ink.as_ref().map(|k| k.hash == d.ink.hash).unwrap_or(false) {
                    st.unchanged += 1;
                } else {
                    let crop = e.ink.as_ref().map(|k| k.crop.clone()).unwrap_or_default();
                    e.ink = Some(Ink { crop, ..d.ink });
                    e.updated = ctx.now;
                    st.changed += 1;
                }
                if e.quote.is_none() {
                    e.quote = d.quote; // 后补的勾画认上
                }
            }
            None => {
                let first = d.ink.strokes.first().cloned().unwrap_or_default();
                entries.push(Entry {
                    id: entry_id(ctx.book, ctx.page, &first),
                    page: ctx.page.to_string(),
                    page_index: ctx.page_index,
                    chapter: ctx.chapter,
                    chapter_title: ctx.chapter_title.to_string(),
                    subhead: ctx.subhead.map(str::to_string),
                    quote: d.quote,
                    ink: Some(d.ink),
                    drafts: vec![],
                    text: None,
                    style: Style::Body,
                    section: None,
                    answer: None,
                    status: Status::Pending,
                    created: ctx.now,
                    updated: ctx.now,
                });
                claimed.push(true);
                st.added += 1;
            }
        }
    }
    for (i, e) in entries.iter_mut().enumerate() {
        if !claimed[i] && e.page == ctx.page && e.status != Status::Revoked && e.ink.is_some() {
            e.status = Status::Revoked;
            e.updated = ctx.now;
            st.revoked += 1;
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::fixtures::*;
    use rmv6::shared::tool::Tool;

    fn page(strokes: Vec<rmv6::page::Stroke>, hls: Vec<rmv6::page::Highlight>) -> Page {
        Page { strokes, highlights: hls, text: None }
    }
    fn ctx<'a>(now: u64) -> PageCtx<'a> {
        PageCtx { book: "b", page: "p1", page_index: 7, chapter: Some(2), chapter_title: "三", subhead: None, now }
    }

    #[test]
    fn incremental_rules_keep_reviewed_text() {
        let th = Thresholds::default();
        let p1 = page(vec![stroke(1, Tool::BallPoint, 900.0, 300.0, 1000.0, 340.0)], vec![hl(9, "勾画", 100.0, 320.0, 780.0, 30.0)]);
        let mut entries = vec![];
        let st = merge_page(&mut entries, &ctx(10), drafts_of_page(&p1, &th));
        assert_eq!(st, MergeStats { added: 1, ..Default::default() });
        let id = entries[0].id.clone();
        assert_eq!(entries[0].quote.as_ref().map(|q| q.text.as_str()), Some("勾画"));
        // 人校对
        entries[0].text = Some("您好".into());
        entries[0].status = Status::Reviewed;
        // 同样的页再摄取一次 → 不变
        let st = merge_page(&mut entries, &ctx(20), drafts_of_page(&p1, &th));
        assert_eq!(st, MergeStats { unchanged: 1, ..Default::default() });
        assert_eq!(entries[0].updated, 10);
        // 补了一笔（共享笔画 1）→ 同一条目、ink 变、text 不动
        let p2 = page(vec![stroke(1, Tool::BallPoint, 900.0, 300.0, 1000.0, 340.0), stroke(2, Tool::BallPoint, 1005.0, 300.0, 1040.0, 340.0)], vec![]);
        let st = merge_page(&mut entries, &ctx(30), drafts_of_page(&p2, &th));
        assert_eq!(st, MergeStats { changed: 1, ..Default::default() });
        assert_eq!(entries.len(), 1);
        assert_eq!((entries[0].id.as_str(), entries[0].text.as_deref()), (id.as_str(), Some("您好")));
        assert!(entries[0].needs_transcribe(), "新指纹没有草稿 → 作为建议再转写");
        assert_eq!(entries[0].quote.as_ref().map(|q| q.text.as_str()), Some("勾画"), "已认的勾画不因这页少了高亮而丢");
        // 笔画全擦掉 → 撤销，不删
        let st = merge_page(&mut entries, &ctx(40), drafts_of_page(&page(vec![], vec![]), &th));
        assert_eq!(st, MergeStats { revoked: 1, ..Default::default() });
        assert_eq!(entries[0].status, Status::Revoked);
        // 别页的条目不受影响
        let mut other = entries.clone();
        other[0].page = "p2".into();
        other[0].status = Status::Reviewed;
        let st = merge_page(&mut other, &ctx(50), vec![]);
        assert_eq!(st, MergeStats::default());
    }

    #[test]
    fn two_clusters_one_with_quote_one_page_note() {
        let th = Thresholds::default();
        let p = page(vec![stroke(1, Tool::BallPoint, 900.0, 300.0, 1000.0, 340.0), stroke(5, Tool::BallPoint, 100.0, 1500.0, 200.0, 1530.0)], vec![hl(9, "第二段", 100.0, 320.0, 780.0, 30.0)]);
        let ds = drafts_of_page(&p, &th);
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].quote.as_ref().map(|q| q.text.as_str()), Some("第二段"));
        assert_eq!(ds[1].quote, None);
        assert_eq!(ds[0].ink.strokes, vec!["1:1"]);
        assert_eq!(ds[0].quote.as_ref().unwrap().color, "yellow");
    }
}
