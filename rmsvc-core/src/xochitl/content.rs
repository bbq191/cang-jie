//! `<uuid>.content` 的页表：页 id 顺序与"文档页序 ↔ PDF 页"映射。此前 book-serve（找回阅读位置，`progress.rs` 的
//! `PageMap`）与 ink-serve（页 → 条目，`doc.rs` 的 `Content::page_ids`）各解析一份（2026-10-10 审计 X-2），这里是
//! 两份合并后的唯一实现，**两边原有语义逐条保留**（差异处见下，测试钉住）：
//!
//! - **formatVersion 1**：`pages`（页 id 数组，下标＝页序）+ `redirectionPageMap`（`[i]`＝文档第 i 页的 PDF 页，笔记页 -1）。
//! - **formatVersion 2**：`cPages.pages`（页对象数组）按 `idx.value`（分数索引字符串，字典序即页序）排、去掉
//!   `deleted.value ≠ 0` 的页；`id` 是页 id，`redir.value` 是 PDF 页（EPUB 的 v2 页对象带不带 `redir` 没在真机上确认过）。
//!
//! 两份旧实现在"两种形状同时出现"时的优先级不同，合并时**各自保留**：
//! - [`PageTable::page_ids`]（ink-serve 语义）：`pages` 非空就用它，否则才看 `cPages`；没有 `id` 的 v2 页对象跳过。
//! - [`PageTable::pdf_of`]/[`PageTable::doc_of`]/[`PageTable::pdf_total`]（book-serve 语义）：有 `cPages.pages` 数组就只看它
//!   （一页都没带 `redir` → 恒等），否则看 `redirectionPageMap`；v2 页对象缺 `id` 也算一页。
use std::path::Path;

/// 一份 `.content` 的页表（只读）。认不出 / 读不了 → 空表：没有页 id、映射恒等。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PageTable {
    /// v1 `pages`。
    v1_ids: Vec<String>,
    /// v2 `cPages.pages`（已去删除、已按 idx 排序）：(页 id, PDF 页)。`None`＝没有 `cPages.pages` 数组。
    v2: Option<Vec<(Option<String>, Option<u32>)>>,
    /// v1 `redirectionPageMap`（空数组当没有）。
    redirection: Option<Vec<Option<u32>>>,
    /// `pageCount`（缺或 0 → `None`）。
    page_count: Option<u32>,
}

impl PageTable {
    /// 读 `<dir>/<uuid>.content`；读不了或不是 JSON → `None`。调用方自己校验 uuid 形状。
    pub fn read(dir: &Path, uuid: &str) -> Option<PageTable> {
        let b = std::fs::read(dir.join(format!("{uuid}.content"))).ok()?;
        serde_json::from_slice::<serde_json::Value>(&b).ok().map(|v| PageTable::parse(&v))
    }

    pub fn parse(v: &serde_json::Value) -> PageTable {
        let as_page = |x: &serde_json::Value| x.as_i64().filter(|&n| n >= 0 && n <= u32::MAX as i64).map(|n| n as u32);
        let page_count = v.get("pageCount").and_then(|x| x.as_u64()).filter(|&n| n > 0).map(|n| n.min(u32::MAX as u64) as u32);
        let v1_ids = v.get("pages").and_then(|p| p.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let redirection = v.get("redirectionPageMap").and_then(|m| m.as_array()).filter(|m| !m.is_empty()).map(|m| m.iter().map(as_page).collect());
        let v2 = v.pointer("/cPages/pages").and_then(|p| p.as_array()).map(|pages| {
            let mut live: Vec<(&str, Option<String>, Option<u32>)> = pages
                .iter()
                .filter(|p| p.pointer("/deleted/value").and_then(|d| d.as_i64()).unwrap_or(0) == 0)
                .map(|p| {
                    let idx = p.pointer("/idx/value").and_then(|i| i.as_str()).unwrap_or("");
                    let id = p.get("id").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
                    (idx, id, p.pointer("/redir/value").and_then(as_page))
                })
                .collect();
            // 稳定排序：idx 相同（不该有）时保持文件里的先后。
            live.sort_by(|a, b| a.0.cmp(b.0));
            live.into_iter().map(|(_, id, r)| (id, r)).collect()
        });
        PageTable { v1_ids, v2, redirection, page_count }
    }

    /// 按页序排列的页 id（下标＝页号，与 `.epubindex` 起始页对齐）。
    pub fn page_ids(&self) -> Vec<&str> {
        if !self.v1_ids.is_empty() {
            return self.v1_ids.iter().map(String::as_str).collect();
        }
        self.v2.iter().flatten().filter_map(|(id, _)| id.as_deref()).collect()
    }

    /// 文档页序 → PDF 页的表（`None`＝恒等）。
    fn pdf_map(&self) -> Option<Vec<Option<u32>>> {
        match &self.v2 {
            Some(v2) => v2.iter().any(|(_, r)| r.is_some()).then(|| v2.iter().map(|(_, r)| *r).collect()),
            None => self.redirection.clone(),
        }
    }

    /// 文档页序 → PDF 页；没有页表、越界、笔记页 → 原值。
    pub fn pdf_of(&self, doc: u32) -> u32 {
        self.pdf_map().and_then(|m| m.get(doc as usize).copied().flatten()).unwrap_or(doc)
    }

    /// PDF 页 → 文档页序（第一个映射到它的文档页）；没有页表或找不到 → 原值。
    pub fn doc_of(&self, pdf: u32) -> u32 {
        self.pdf_map().and_then(|m| m.iter().position(|&p| p == Some(pdf))).map_or(pdf, |i| i as u32)
    }

    /// PDF 总页数：有页表取最大 PDF 页 + 1，否则 `pageCount`。
    pub fn pdf_total(&self) -> Option<u32> {
        match self.pdf_map() {
            Some(m) => m.iter().flatten().max().map(|&p| p + 1),
            None => self.page_count,
        }
    }

    /// `pageCount`（缺或 0 → `None`）。
    pub fn page_count(&self) -> Option<u32> {
        self.page_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v1_ids_and_redirection_map() {
        let t = PageTable::parse(&json!({"pages": ["a", "n", "b"], "redirectionPageMap": [0, -1, 1], "pageCount": 3}));
        assert_eq!(t.page_ids(), ["a", "n", "b"]);
        assert_eq!((t.pdf_of(0), t.pdf_of(1), t.pdf_of(2), t.pdf_of(9)), (0, 1, 1, 9), "笔记页（-1）与越界 → 原值");
        assert_eq!((t.doc_of(1), t.doc_of(7)), (2, 7));
        assert_eq!(t.pdf_total(), Some(2));
        assert_eq!(t.page_count(), Some(3));
    }

    #[test]
    fn v2_sorted_by_idx_without_deleted() {
        let t = PageTable::parse(&json!({"cPages": {"pages": [
            {"id": "p2", "idx": {"value": "bb"}, "redir": {"value": 1}},
            {"id": "gone", "idx": {"value": "ab"}, "deleted": {"value": 1}, "redir": {"value": 9}},
            {"id": "p1", "idx": {"value": "ba"}, "redir": {"value": 0}},
            {"id": "note", "idx": {"value": "bc"}}
        ]}}));
        assert_eq!(t.page_ids(), ["p1", "p2", "note"]);
        assert_eq!((t.pdf_of(0), t.pdf_of(1), t.pdf_of(2)), (0, 1, 2), "笔记页没有 redir → 原值");
        assert_eq!(t.doc_of(1), 1);
        assert_eq!(t.pdf_total(), Some(2));
    }

    #[test]
    fn empty_or_unknown_is_identity() {
        for v in [json!({}), json!({"redirectionPageMap": []}), json!({"cPages": {"pages": [{"id": "x", "idx": {"value": "a"}}]}, "pageCount": 5})] {
            let t = PageTable::parse(&v);
            assert_eq!((t.pdf_of(3), t.doc_of(3)), (3, 3), "{v}");
        }
        assert_eq!(PageTable::parse(&json!({"pageCount": 5})).pdf_total(), Some(5), "没有页表 → pageCount");
        assert_eq!(PageTable::parse(&json!({"cPages": {"pages": []}, "pageCount": 5})).pdf_total(), Some(5));
        let t = tempfile::tempdir().unwrap();
        assert!(PageTable::read(t.path(), "nope").is_none());
        std::fs::write(t.path().join("u.content"), r#"{"pages":["a"]}"#).unwrap();
        assert_eq!(PageTable::read(t.path(), "u").unwrap().page_ids(), ["a"]);
    }

    /// 两份旧实现的差异处，合并后各自保留（钉住，第二阶段若要统一需要改这里的断言并说明理由）。
    #[test]
    fn divergences_between_the_two_legacy_parsers_are_preserved() {
        // 1. 两种形状同时出现：页 id 取 v1 `pages`（ink-serve），PDF 映射取 `cPages`（book-serve）。
        let both = PageTable::parse(&json!({
            "pages": ["v1a", "v1b"],
            "redirectionPageMap": [5, 6],
            "cPages": {"pages": [{"id": "v2a", "idx": {"value": "a"}, "redir": {"value": 0}}, {"id": "v2b", "idx": {"value": "b"}, "redir": {"value": 1}}]}
        }));
        assert_eq!(both.page_ids(), ["v1a", "v1b"]);
        assert_eq!(both.pdf_of(0), 0, "不是 redirectionPageMap 的 5");
        // 有 cPages.pages 数组但一页都没 redir：映射恒等，不回落到 redirectionPageMap
        let no_redir = PageTable::parse(&json!({"redirectionPageMap": [5], "cPages": {"pages": [{"id": "x", "idx": {"value": "a"}}]}}));
        assert_eq!(no_redir.pdf_of(0), 0);
        // 2. v2 页对象缺 id：页 id 表里跳过（ink-serve），PDF 映射里仍算一页（book-serve）。
        let no_id = PageTable::parse(&json!({"cPages": {"pages": [
            {"idx": {"value": "a"}, "redir": {"value": 0}},
            {"id": "b", "idx": {"value": "b"}, "redir": {"value": 1}}
        ]}}));
        assert_eq!(no_id.page_ids(), ["b"]);
        assert_eq!((no_id.pdf_of(1), no_id.doc_of(1)), (1, 1), "缺 id 的那页仍占第 0 页");
    }
}
