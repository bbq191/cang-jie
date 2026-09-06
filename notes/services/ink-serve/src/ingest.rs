//! 一份文档的摄取编排：只扫**变更页**（页 `.rm` mtime 大于上次记录）→ rmv6 解析 → notecore 配对/合并 → 裁图 → 落条目库。
//! 只处理活的 EPUB 文档（首期）；页→章由 epubmap 给。
use crate::bookdb::BookDb;
use crate::config::IngestConfig;
use crate::crop::{crop_png, PageGeom};
use crate::doc::Doc;
use epubmap::BookMap;
use notecore::ingest::{drafts_of_page, merge_page, MergeStats, PageCtx};
use notecore::model::{default_sections, Book};
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DocStats {
    pub pages: usize,
    pub merge: MergeStats,
}

/// 摄取一份文档。返回 None = 不该管（非 EPUB / 回收站 / 没有手写页）。
pub fn ingest_doc(lib: &Path, crops_dir: &Path, db: &BookDb, cfg: &IngestConfig, uuid: &str, now: u64) -> Result<Option<DocStats>, String> {
    let doc = Doc::new(lib, uuid);
    let Some(meta) = doc.metadata().filter(|m| m.is_live_document()) else { return Ok(None) };
    let Some(content) = doc.content().filter(|c| c.file_type == "epub") else { return Ok(None) };
    let pages = doc.annotated_pages();
    if pages.is_empty() {
        return Ok(None);
    }
    let prev = db.load(uuid);
    let changed: Vec<(String, u64)> = pages.into_iter().filter(|(id, mt)| prev.as_ref().and_then(|b| b.page_mtimes.get(id)).map(|&old| *mt > old).unwrap_or(true)).collect();
    if changed.is_empty() {
        return Ok(Some(DocStats::default()));
    }
    // 页→章：每次摄取现读（.epubindex 在 xochitl 重排后会变）
    let map = match (doc.epub_bytes(), doc.epubindex_bytes()) {
        (Some(e), Some(i)) => BookMap::from_epub(&e, &i),
        _ => BookMap::default(),
    };
    let geom = PageGeom { page_w: cfg.page_width, page_h: cfg.page_height, x_origin_center: cfg.x_origin_center, margin: cfg.crop_margin };
    let th = cfg.thresholds();
    let title = meta.visible_name.clone();
    let chapters: Vec<String> = map.chapters().into_iter().map(|(_, t)| t.to_string()).collect();
    let mut stats = DocStats::default();
    let mut errors: Vec<String> = vec![];
    db.update(
        uuid,
        || Book { uuid: uuid.to_string(), title: title.clone(), sections: default_sections(), ..Default::default() },
        |book| {
            book.title = title.clone();
            book.chapters = chapters.clone();
            for (page_id, mtime) in &changed {
                let bytes = match std::fs::read(doc.page_rm(page_id)) {
                    Ok(b) => b,
                    Err(e) => {
                        errors.push(format!("{page_id}: 读 .rm 失败 {e}"));
                        continue;
                    }
                };
                let page = match rmv6::page::Page::parse(&bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(format!("{page_id}: 解析失败 {e:?}"));
                        continue;
                    }
                };
                let page_index = content.pages.iter().position(|p| p == page_id).unwrap_or(0);
                let ch = map.chapter_of(page_index);
                let ctx = PageCtx { book: uuid, page: page_id, page_index, chapter: ch.as_ref().map(|c| c.index), chapter_title: ch.as_ref().map(|c| c.title).unwrap_or(""), subhead: ch.as_ref().and_then(|c| c.subhead), now };
                let drafts = drafts_of_page(&page, &th);
                let m = merge_page(&mut book.entries, &ctx, drafts);
                stats.merge.added += m.added;
                stats.merge.changed += m.changed;
                stats.merge.unchanged += m.unchanged;
                stats.merge.revoked += m.revoked;
                stats.pages += 1;
                // 裁图：本页所有有手写、且裁图缺失或指纹变了的条目
                if let Ok(thumb) = std::fs::read(doc.page_thumb(page_id)) {
                    for e in book.entries.iter_mut().filter(|e| e.page == *page_id) {
                        let Some(ink) = e.ink.as_mut() else { continue };
                        let want = format!("{}-{}.png", e.id, ink.hash);
                        if ink.crop == want && crops_dir.join(&want).is_file() {
                            continue;
                        }
                        match crop_png(&thumb, &geom, ink.bbox) {
                            Ok(png) => match shelf_core::fs::write_atomic(&crops_dir.join(&want), &png) {
                                Ok(()) => ink.crop = want,
                                Err(err) => errors.push(format!("{page_id}: 写裁图失败 {err}")),
                            },
                            Err(err) => errors.push(format!("{page_id}: {err}")),
                        }
                    }
                }
                book.page_mtimes.insert(page_id.clone(), *mtime);
            }
        },
    )?;
    if !errors.is_empty() {
        println!("[ink-serve] {uuid} 摄取告警: {}", errors.join("; "));
    }
    Ok(Some(stats))
}

/// 书库里所有活的 EPUB 且有手写页的文档 uuid（启动追平用）。
pub fn candidate_docs(lib: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(lib) else { return vec![] };
    let mut out: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.is_dir() {
                return None;
            }
            let uuid = p.file_name()?.to_str()?.to_string();
            crate::doc::uuid_of_event(&uuid)?;
            let d = Doc::new(lib, &uuid);
            (d.metadata()?.is_live_document() && d.content()?.file_type == "epub" && !d.annotated_pages().is_empty()).then_some(uuid)
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_non_epub_and_ingests_only_changed_pages() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&crops).unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        // 一份 EPUB 文档：真机墓碑页当"变更页"（解析成功、零条目）
        let u = "3eb5dece-5e28-4969-8926-c8973d49020d";
        std::fs::create_dir_all(lib.join(u)).unwrap();
        std::fs::write(lib.join(format!("{u}.metadata")), r#"{"visibleName":"人骨拼圖","type":"DocumentType","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{u}.content")), include_str!("../../../testdata/renggu/book.content")).unwrap();
        std::fs::write(lib.join(format!("{u}.epubindex")), include_bytes!("../../../testdata/renggu/book.epubindex")).unwrap();
        std::fs::write(lib.join(u).join("c65fa2ae-7070-4b33-b367-1810bd35632c.rm"), include_bytes!("../../../testdata/renggu/page.rm")).unwrap();
        let cfg = IngestConfig::default();
        assert_eq!(candidate_docs(&lib), vec![u.to_string()]);
        let s = ingest_doc(&lib, &crops, &db, &cfg, u, 1).unwrap().unwrap();
        assert_eq!(s.pages, 1);
        let book = db.load(u).unwrap();
        assert_eq!((book.title.as_str(), book.chapters.len(), book.entries.len(), book.sections.len()), ("人骨拼圖", 0, 0, 4), "没 .epub 文件 → 无目录；墓碑页零条目；缺省分区");
        assert_eq!(book.page_mtimes.len(), 1);
        // 再来一次：页没变 → 零页
        let s2 = ingest_doc(&lib, &crops, &db, &cfg, u, 2).unwrap().unwrap();
        assert_eq!(s2, DocStats::default());
        // 非 EPUB / 回收站 → None
        std::fs::write(lib.join(format!("{u}.metadata")), r#"{"visibleName":"人骨拼圖","type":"DocumentType","parent":"trash"}"#).unwrap();
        assert!(ingest_doc(&lib, &crops, &db, &cfg, u, 3).unwrap().is_none());
    }
}
