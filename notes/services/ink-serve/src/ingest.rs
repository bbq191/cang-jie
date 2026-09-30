//! 一份文档的摄取编排：只扫**变更页**（页 `.rm` mtime 大于上次记录）→ rmv6 解析 → notecore 配对/合并 → 裁图 → 落条目库。
//! 只处理活的 EPUB 文档（首期）；页→章由 epubmap 给。
use crate::bookdb::BookDb;
use crate::config::IngestConfig;
use crate::crop::render_ink;
use crate::doc::Doc;
use epubmap::BookMap;
use notecore::ingest::{drafts_of_page, merge_page, MergeStats, PageCtx};
use notecore::model::{Book, Status};
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DocStats {
    pub pages: usize,
    pub merge: MergeStats,
}

/// 摄取一份文档。返回 None = 不该管（非 EPUB / 回收站 / 没有手写页 / KOReader 摄取线的 Book）。
pub fn ingest_doc(lib: &Path, crops_dir: &Path, db: &BookDb, cfg: &IngestConfig, uuid: &str, now: u64) -> Result<Option<DocStats>, String> {
    // KOReader 高亮/生词回流线的 Book（`uuid` 形如 `koreader:...`/`koreader-vocab`，见 `koreader.rs`）
    // 不是 xochitl 设备文档——不能走下面的 `Doc::new` 找不到就当"书被删了"那条路径，不然每次启动追平
    // （`main.rs` 的 catchup 会把 `db.list()` 里所有已知 uuid 都过一遍这个函数）都会把它们的条目
    // 整批标 `Revoked`，2026-09-16 设计阶段发现的坑，写进白皮书 §03al。
    if uuid.starts_with("koreader:") || uuid == "koreader-vocab" {
        return Ok(None);
    }
    let doc = Doc::new(lib, uuid);
    let Some(meta) = doc.read_metadata()?.filter(|m| m.is_live_document()) else {
        // 书被移进回收站，或彻底删除（连 .metadata 都没了）：撤销条目库里这本书还没撤销的条目。
        // 不这么做的话 `BookDb::list_active` 找不到理由把它从列表摘掉——它只看条目状态，
        // 从没在这条路径上被通知过"书本身没了"（真机验证时发现，2026-09-07；清空勾画走
        // `merge_page` 那条撤销路径管的是"页还在、笔画没了"，这里是另一条"书不见了"的路径）。
        return revoke_stale(db, uuid, now);
    };
    // 先看有没有 `.rm` 页（只 read_dir 一个目录），没有手写页就不必解析 `.content`（长书几百上千个页 id 的 JSON）：
    // xochitl 翻页/读书会不停改写 `.content`/`.metadata`，每次都会触发一次摄取，绝大多数书根本没有手写页。
    let pages = doc.annotated_pages();
    if pages.is_empty() {
        return Ok(None);
    }
    // 同理先比页 mtime 再解析 `.content`：有手写页的书读书时也会被 xochitl 反复改写 `.content`，但页没变就没事可做
    // （条目库这一读命中 `BookDb` 的解析缓存，几乎零成本）。只有条目库里已追平过的 EPUB 才可能走到"没变化"这条早退，
    // 没追平过的书一律算变更、照旧在下面核对 fileType。
    let prev = db.read(uuid)?;
    let changed: Vec<(String, u64)> = pages.into_iter().filter(|(id, mt)| prev.as_ref().and_then(|b| b.page_mtimes.get(id)).map(|&old| *mt > old).unwrap_or(true)).collect();
    if changed.is_empty() {
        return Ok(Some(DocStats::default()));
    }
    let Some(content) = doc.content().filter(|c| c.file_type == "epub") else { return Ok(None) };
    // 页→章：每次摄取现读（.epubindex 在 xochitl 重排后会变）
    let map = match (doc.epub_file(), doc.epubindex_bytes()) {
        (Some(e), Some(i)) => BookMap::from_epub_reader(e, &i),
        _ => BookMap::default(),
    };
    let th = cfg.thresholds();
    let title = meta.visible_name.clone();
    let chapters: Vec<String> = map.chapters().into_iter().map(|(_, t)| t.to_string()).collect();
    let mut stats = DocStats::default();
    let mut errors: Vec<String> = vec![];
    db.update(
        uuid,
        || Book { uuid: uuid.to_string(), title: title.clone(), ..Default::default() },
        |book| {
            book.title = title.clone();
            // 这次没读到目录（`.epub`/`.epubindex` 暂时读不了）就保留上次的章表，不把整本书的章清空——条目的
            // `chapter` 下标指着它，清空后两处投影一章都找不到（`merge_page` 同理不清已有条目的章）。
            if !chapters.is_empty() || book.chapters.is_empty() {
                book.chapters = chapters.clone();
            }
            for (page_id, mtime) in &changed {
                let bytes = match read_settled(&doc.page_rm(page_id)) {
                    Ok(b) => b,
                    Err(e) => {
                        errors.push(format!("{page_id}: {e}"));
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
                stats.merge.revived += m.revived;
                stats.pages += 1;
                refresh_crops(crops_dir, page_id, &page.strokes, &mut book.entries, cfg.crop_margin, &mut errors);
                // mtime 只到秒：这一秒里读完之后 xochitl 又写了一次，mtime 还是同一个秒数，下次比较"没变"就漏了。
                // 页的修改时间落在当前这一秒（或更晚）时少记一秒，下次事件再扫一遍（合并幂等），之后正常记下
                // （2026-09-25 第四轮审计；条目库格式不变）。
                book.page_mtimes.insert(page_id.clone(), if *mtime >= now { mtime.saturating_sub(1) } else { *mtime });
            }
        },
    )?;
    if !errors.is_empty() {
        println!("[ink-serve] {uuid} 摄取告警: {}", errors.join("; "));
    }
    Ok(Some(stats))
}

/// 文件的 (长度, 修改时间)：判断读的过程中有没有被改写。
type Stamp = (u64, Option<std::time::SystemTime>);

/// 读一页 `.rm`，并确认读的过程中它没被改写：读前读后各取一次同一个 fd 的 (长度, mtime)，对不上、或读到的字节数
/// 跟长度对不上，就当"正在写入"报错跳过（不记 mtime，下次事件再扫）。v6 是一串块，截在块边界上的半截文件照样
/// 能解析成功、只是少了后面的笔画——当成真的会把那些笔画对应的条目撤销（下次读全了虽会复活，但 `Pending`
/// 复活只回 `Mined`，用户"转入笔记"的决定就丢了；2026-09-30 第五轮审计）。
fn read_settled(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let stamp = |f: &std::fs::File| -> std::io::Result<Stamp> { f.metadata().map(|m| (m.len(), m.modified().ok())) };
    let mut f = std::fs::File::open(path).map_err(|e| format!("读 .rm 失败 {e}"))?;
    let before = stamp(&f).map_err(|e| format!("读 .rm 失败 {e}"))?;
    let mut bytes = Vec::with_capacity(before.0 as usize);
    f.read_to_end(&mut bytes).map_err(|e| format!("读 .rm 失败 {e}"))?;
    let after = stamp(&f).map_err(|e| format!("读 .rm 失败 {e}"))?;
    if !settled(before, after, bytes.len()) {
        return Err("页正在写入，下次再扫".into());
    }
    Ok(bytes)
}

fn settled(before: Stamp, after: Stamp, read: usize) -> bool {
    before == after && after.0 == read as u64
}

/// 裁图：本页所有有手写、且裁图缺失或指纹变了的条目。自渲染（`render_ink`）直接吃这一页已经解析好的
/// `strokes`，不依赖 xochitl 缩略图——写多靠下都画得出来，也不会混进印刷体（2026-09-07 二期真机验证发现的
/// 两个问题，见白皮书 §03o）。补了几笔 → 指纹变了 → 新裁图换了名字，旧的那张再没人引用，写好新的就删掉
/// （此前一直留着，裁图目录只增不减；2026-09-30 第五轮审计）。
fn refresh_crops(crops_dir: &Path, page_id: &str, strokes: &[rmv6::page::Stroke], entries: &mut [notecore::model::Entry], margin: f32, errors: &mut Vec<String>) {
    for e in entries.iter_mut().filter(|e| e.page == page_id) {
        let Some(ink) = e.ink.as_mut() else { continue };
        let want = format!("{}-{}.png", e.id, ink.hash);
        if ink.crop == want && crops_dir.join(&want).is_file() {
            continue;
        }
        let png = match render_ink(strokes, &ink.strokes, ink.bbox, margin) {
            Ok(png) => png,
            Err(err) => {
                errors.push(format!("{page_id}: {err}"));
                continue;
            }
        };
        if let Err(err) = rmsvc_core::fs::write_atomic(&crops_dir.join(&want), &png) {
            errors.push(format!("{page_id}: 写裁图失败 {err}"));
            continue;
        }
        let old = std::mem::replace(&mut ink.crop, want);
        // 文件名来自条目库，过一遍单段校验再拼路径。
        if let Ok(name) = rmsvc_core::fs::plain_name(&old) {
            if name != ink.crop {
                let _ = std::fs::remove_file(crops_dir.join(name));
            }
        }
    }
}

/// 书不再活了（回收站/已删）：条目库里如果还有没撤销的条目，全标 `Revoked`（不物理删，历史留痕）。
/// 没追平摄取过的书（条目库压根没有）是 no-op；已经全撤销过也是 no-op（幂等，事件重复触发不白做功、不写盘）。
/// 同时清掉页 mtime 记录：书从回收站恢复后页文件原样不动，不清的话"页没变"早退，条目永远停在 `Revoked`；
/// 清了之后恢复时整本重扫一遍，`merge_page` 按笔画把原条目复活（2026-09-25 第四轮审计）。
fn revoke_stale(db: &BookDb, uuid: &str, now: u64) -> Result<Option<DocStats>, String> {
    let Some(revoked) = db.update_existing(uuid, |b| {
        b.page_mtimes.clear();
        let mut n = 0usize;
        // 同一个"排除法"漏洞（见 notecore::ingest::merge_page 的注释）：`Skipped`/`Archived` 也是
        // 终态，书被删/进回收站不该把它们悄悄改判成 `Revoked`——那样以后 `restore()` 会走错分支。
        // 用 `is_terminal()` 排除全部三种终态，只把还活着（Mined/Pending/Draft/Reviewed）的条目
        // 因"书不在了"而转 Revoked。
        for e in b.entries.iter_mut().filter(|e| !e.is_terminal()) {
            e.status = Status::Revoked;
            e.updated = now;
            n += 1;
        }
        n
    })? else { return Ok(None) };
    Ok((revoked > 0).then(|| DocStats { pages: 0, merge: MergeStats { revoked, ..Default::default() } }))
}

/// 启动追平前调用：还活着却没有章的 xochitl 条目，把它们所在页的 mtime 记录忘掉，让这次追平重扫一遍——
/// 页→章的规则会变（2026-09-29 起不在目录里的 spine 文件归到前面最近的目录章，配合书架把一章拆成多个文件），
/// 而摄取只扫 mtime 变了的页：规则改之前摄取的条目章一直是空的，两处投影都不收，页不动就永远不会被重新归章。
/// 重扫时 `merge_page` 认领原条目、只刷新页级上下文，别的都不动。真在第一个目录条目之前的页（封面等）每次启动
/// 多扫一遍，代价可忽略。返回忘掉了几页。
pub fn forget_unmapped_pages(db: &BookDb, uuid: &str) -> Result<usize, String> {
    Ok(db
        .update_existing(uuid, |b| {
            let pages: std::collections::BTreeSet<String> = b.entries.iter().filter(|e| e.chapter.is_none() && !e.is_terminal() && e.source == notecore::model::Source::Xochitl).map(|e| e.page.clone()).collect();
            pages.iter().filter(|p| b.page_mtimes.remove(*p).is_some()).count()
        })?
        .unwrap_or(0))
}

/// 清空一本书的回收站（`Book::purge_terminal`），被清掉的条目的裁图一并删——此前裁图留在目录里再没人引用，
/// 只增不减。书不在条目库 → `Ok(None)`。
pub fn purge_terminal(db: &BookDb, crops_dir: &Path, uuid: &str) -> Result<Option<usize>, String> {
    let Some((removed, crops)) = db.update_existing(uuid, |b| {
        let crops: Vec<String> = b.entries.iter().filter(|e| e.is_terminal()).filter_map(|e| e.ink.as_ref().map(|k| k.crop.clone())).filter(|c| !c.is_empty()).collect();
        (b.purge_terminal(), crops)
    })?
    else {
        return Ok(None);
    };
    for c in crops.iter().filter_map(|c| rmsvc_core::fs::plain_name(c).ok()) {
        let _ = std::fs::remove_file(crops_dir.join(c));
    }
    Ok(Some(removed))
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
            // 由便宜到贵：先看有没有 `.rm`（一次 read_dir），再读 `.metadata`，最后才碰 `.content`（只取 fileType）——
            // 书库里绝大多数文档没有手写页，此前每次启动都要把每份文档的 `.content`（长 PDF 上千个页 id）整份解析一遍。
            (d.has_annotated_pages() && d.metadata()?.is_live_document() && d.file_type()? == "epub").then_some(uuid)
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
        let t0 = Doc::new(&lib, u).annotated_pages()[0].1 + 10; // "现在"晚于页修改时间（同一秒的情形见下一条测试）
        let s = ingest_doc(&lib, &crops, &db, &cfg, u, t0).unwrap().unwrap();
        assert_eq!(s.pages, 1);
        let book = db.load(u).unwrap();
        assert_eq!((book.title.as_str(), book.chapters.len(), book.entries.len()), ("人骨拼圖", 0, 0), "没 .epub 文件 → 无目录；墓碑页零条目");
        assert_eq!(book.page_mtimes.len(), 1);
        // 再来一次：页没变 → 零页
        let s2 = ingest_doc(&lib, &crops, &db, &cfg, u, t0 + 1).unwrap().unwrap();
        assert_eq!(s2, DocStats::default());
        // 非 EPUB / 回收站 → None（这本书条目库里本来就是 0 条，revoke_stale 无事可做）
        std::fs::write(lib.join(format!("{u}.metadata")), r#"{"visibleName":"人骨拼圖","type":"DocumentType","parent":"trash"}"#).unwrap();
        assert!(ingest_doc(&lib, &crops, &db, &cfg, u, 3).unwrap().is_none());
    }

    /// 页在当前这一秒刚改过：先少记一秒，下次事件再扫一遍（防同一秒内的第二次写入被漏掉），之后正常记下。
    #[test]
    fn page_modified_in_the_current_second_is_rescanned_once() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&crops).unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let u = "3eb5dece-5e28-4969-8926-c8973d49020d";
        std::fs::create_dir_all(lib.join(u)).unwrap();
        std::fs::write(lib.join(format!("{u}.metadata")), r#"{"visibleName":"x","type":"DocumentType","parent":""}"#).unwrap();
        std::fs::write(lib.join(format!("{u}.content")), include_str!("../../../testdata/renggu/book.content")).unwrap();
        std::fs::write(lib.join(u).join("c65fa2ae-7070-4b33-b367-1810bd35632c.rm"), include_bytes!("../../../testdata/renggu/page.rm")).unwrap();
        let mt = Doc::new(&lib, u).annotated_pages()[0].1;
        let cfg = IngestConfig::default();
        assert_eq!(ingest_doc(&lib, &crops, &db, &cfg, u, mt).unwrap().unwrap().pages, 1);
        assert_eq!(ingest_doc(&lib, &crops, &db, &cfg, u, mt + 5).unwrap().unwrap().pages, 1, "同一秒记的，下次再扫一遍");
        assert_eq!(ingest_doc(&lib, &crops, &db, &cfg, u, mt + 6).unwrap().unwrap().pages, 0, "之后正常跳过");
        assert_eq!(db.load(u).unwrap().page_mtimes.values().copied().collect::<Vec<_>>(), [mt]);
    }

    /// 读的过程中文件被改写（长度/mtime 变了、读到的字节数对不上）→ 当"正在写入"跳过，不拿半截内容去合并。
    #[test]
    fn half_written_page_is_skipped_not_merged() {
        let t0 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let t1 = t0 + std::time::Duration::from_secs(1);
        assert!(settled((10, Some(t0)), (10, Some(t0)), 10));
        assert!(!settled((10, Some(t0)), (20, Some(t1)), 20), "读的过程中追加了");
        assert!(!settled((10, Some(t0)), (10, Some(t1)), 10), "原地改写，长度没变");
        assert!(!settled((10, Some(t0)), (10, Some(t0)), 6), "读到的字节数对不上");
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("p.rm");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(read_settled(&p).unwrap(), b"abc");
        assert!(read_settled(&d.path().join("nope.rm")).unwrap_err().contains("读 .rm 失败"));
    }

    fn seeded_entry(id: &str, status: Status) -> notecore::model::Entry {
        notecore::model::Entry { id: id.into(), page: "p".into(), page_index: 0, chapter: None, chapter_title: String::new(), subhead: None, quote: None, ink: None, drafts: vec![], text: None, style: Default::default(), ask_ai: false, question: None, answer: None, status, destination: Default::default(), source: Default::default(), created: 0, updated: 0 }
    }

    fn ink_entry(id: &str, hash: &str, crop: &str, status: Status) -> notecore::model::Entry {
        let mut e = seeded_entry(id, status);
        e.ink = Some(notecore::model::Ink { strokes: vec!["1:1".into()], bbox: (0.0, 0.0, 40.0, 20.0), hash: hash.into(), crop: crop.into() });
        e
    }

    /// 启动追平前：活着却没章的 xochitl 条目所在页忘掉 mtime（让追平重扫、按新规则归章）；有章的页、终态条目、
    /// KOReader 条目的页不动；没变化不写盘。
    #[test]
    fn forget_unmapped_pages_only_touches_live_chapterless_xochitl_pages() {
        let t = tempfile::tempdir().unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let on = |id: &str, page: &str, chapter: Option<usize>, status: Status| {
            let mut e = seeded_entry(id, status);
            e.page = page.into();
            e.chapter = chapter;
            e
        };
        let mut ko = on("k", "pk", None, Status::Reviewed);
        ko.source = notecore::model::Source::KoreaderHighlight;
        db.update(
            "u",
            || Book { uuid: "u".into(), ..Default::default() },
            |b| {
                b.entries = vec![on("a", "p1", None, Status::Mined), on("b", "p2", Some(0), Status::Reviewed), on("c", "p3", None, Status::Archived), ko];
                b.page_mtimes = [("p1", 5), ("p2", 5), ("p3", 5), ("pk", 5)].into_iter().map(|(p, m)| (p.to_string(), m)).collect();
            },
        )
        .unwrap();
        assert_eq!(forget_unmapped_pages(&db, "u").unwrap(), 1);
        assert_eq!(db.load("u").unwrap().page_mtimes.keys().map(String::as_str).collect::<Vec<_>>(), ["p2", "p3", "pk"]);
        assert_eq!(forget_unmapped_pages(&db, "u").unwrap(), 0, "再来一次没事可做");
        assert_eq!(forget_unmapped_pages(&db, "ghost").unwrap(), 0);
    }

    /// 回归（2026-09-30）：指纹变了重画裁图后删掉旧的那张；清空回收站时被清条目的裁图一并删，活条目的留着。
    #[test]
    fn superseded_and_purged_crops_are_deleted() {
        let t = tempfile::tempdir().unwrap();
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&crops).unwrap();
        let pts = [(0.0, 0.0), (40.0, 20.0)];
        let stroke = rmv6::page::Stroke { id: rmv6::v6::crdt::CrdtId { part1: 1, part2: 1 }, parent: Default::default(), tool: rmv6::shared::tool::Tool::BallPoint, color: rmv6::shared::pen_color::PenColor::Black, thickness: 1.0, points: pts.to_vec(), bbox: rmv6::page::BBox::of_points(pts).unwrap() };
        std::fs::write(crops.join("e1-old.png"), b"x").unwrap();
        let mut entries = vec![ink_entry("e1", "new", "e1-old.png", Status::Pending)];
        let mut errors = vec![];
        refresh_crops(&crops, "p", std::slice::from_ref(&stroke), &mut entries, 4.0, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(entries[0].ink.as_ref().unwrap().crop, "e1-new.png");
        assert!(crops.join("e1-new.png").is_file() && !crops.join("e1-old.png").exists(), "旧裁图删掉");

        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        std::fs::write(crops.join("gone-h.png"), b"x").unwrap();
        db.update("u", || Book { uuid: "u".into(), ..Default::default() }, |b| b.entries = vec![entries[0].clone(), ink_entry("gone", "h", "gone-h.png", Status::Archived)]).unwrap();
        assert_eq!(purge_terminal(&db, &crops, "u").unwrap(), Some(1));
        assert!(!crops.join("gone-h.png").exists() && crops.join("e1-new.png").is_file(), "只删被清条目的裁图");
        assert_eq!(purge_terminal(&db, &crops, "nope").unwrap(), None);
    }

    /// 真机验证时发现的 bug（2026-09-07）：书被移进回收站、甚至彻底删除，条目库里的旧条目
    /// 一直没人管，`BookDb::list_active` 就一直找得到理由把这本书留在网页选择器里。
    #[test]
    fn trashed_or_deleted_book_revokes_its_stale_entries_but_untracked_book_is_a_no_op() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(&crops).unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let cfg = IngestConfig::default();

        // 场景一：条目库里已经追平过、有活条目的书，被移进回收站（.metadata 还在，parent=trash）。
        let u1 = "11111111-5e28-4969-8926-c8973d49020d";
        db.update(u1, || Book { uuid: u1.into(), title: "测试书一".into(), ..Default::default() }, |b| b.entries.push(seeded_entry("e1", Status::Reviewed))).unwrap();
        std::fs::write(lib.join(format!("{u1}.metadata")), r#"{"visibleName":"测试书一","type":"DocumentType","parent":"trash"}"#).unwrap();
        let s1 = ingest_doc(&lib, &crops, &db, &cfg, u1, 10).unwrap().unwrap();
        assert_eq!((s1.pages, s1.merge.revoked), (0, 1));
        assert_eq!(db.load(u1).unwrap().entries[0].status, Status::Revoked);
        // 再摄取一遍（比如又收到一次事件）：已经全撤销 → 幂等 no-op，不重复计数
        assert!(ingest_doc(&lib, &crops, &db, &cfg, u1, 11).unwrap().is_none());

        // 场景二：另一本书直接被彻底删除——.metadata 都没写过（真机上是文件被删掉）。
        let u2 = "22222222-5e28-4969-8926-c8973d49020d";
        db.update(u2, || Book { uuid: u2.into(), title: "测试书二".into(), ..Default::default() }, |b| b.entries.push(seeded_entry("e2", Status::Pending))).unwrap();
        let s2 = ingest_doc(&lib, &crops, &db, &cfg, u2, 12).unwrap().unwrap();
        assert_eq!((s2.pages, s2.merge.revoked), (0, 1));
        assert_eq!(db.load(u2).unwrap().entries[0].status, Status::Revoked);

        // 场景三：条目库里压根没追平过的书被删——no-op，不建幽灵记录、不 panic。
        let u3 = "33333333-5e28-4969-8926-c8973d49020d";
        assert!(ingest_doc(&lib, &crops, &db, &cfg, u3, 13).unwrap().is_none());
        assert!(db.load(u3).is_none());
    }

    /// 回归（2026-09-25）：书进回收站 → 条目撤销；从回收站恢复（页文件原样没变）→ 整本重扫，原条目复活、不重复建。
    #[test]
    fn book_restored_from_trash_revives_its_entries() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&crops).unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let u = "3eb5dece-5e28-4969-8926-c8973d49020d";
        std::fs::create_dir_all(lib.join(u)).unwrap();
        let meta = |parent: &str| format!(r#"{{"visibleName":"人骨拼圖","type":"DocumentType","parent":"{parent}"}}"#);
        std::fs::write(lib.join(format!("{u}.metadata")), meta("")).unwrap();
        std::fs::write(lib.join(format!("{u}.content")), include_str!("../../../testdata/renggu/book.content")).unwrap();
        std::fs::write(lib.join(u).join("c65fa2ae-7070-4b33-b367-1810bd35632c.rm"), include_bytes!("../../../testdata/renggu_marks/page.rm")).unwrap();
        let cfg = IngestConfig::default();
        ingest_doc(&lib, &crops, &db, &cfg, u, 1).unwrap().unwrap();
        let before = db.load(u).unwrap().entries;
        assert!(!before.is_empty());

        std::fs::write(lib.join(format!("{u}.metadata")), meta("trash")).unwrap();
        ingest_doc(&lib, &crops, &db, &cfg, u, 2).unwrap().unwrap();
        let trashed = db.load(u).unwrap();
        assert!(trashed.entries.iter().all(|e| e.status == Status::Revoked) && trashed.page_mtimes.is_empty());
        assert!(ingest_doc(&lib, &crops, &db, &cfg, u, 3).unwrap().is_none(), "再来一次：无事可做");

        std::fs::write(lib.join(format!("{u}.metadata")), meta("")).unwrap();
        let s = ingest_doc(&lib, &crops, &db, &cfg, u, 4).unwrap().unwrap();
        assert_eq!((s.merge.revived, s.merge.added), (before.len(), 0));
        let after = db.load(u).unwrap().entries;
        assert_eq!(after.iter().map(|e| (&e.id, e.status)).collect::<Vec<_>>(), before.iter().map(|e| (&e.id, Status::Mined)).collect::<Vec<_>>());
    }

    /// 回归：`.metadata` 读到半截/解析失败不能当成"书没了"——此前会把整本书的活条目全标 Revoked。
    #[test]
    fn unreadable_metadata_skips_instead_of_revoking() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(&crops).unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let u = "55555555-5e28-4969-8926-c8973d49020d";
        db.update(u, || Book { uuid: u.into(), title: "测试书五".into(), ..Default::default() }, |b| b.entries.push(seeded_entry("e1", Status::Reviewed))).unwrap();
        std::fs::write(lib.join(format!("{u}.metadata")), r#"{"visibleName":"测试书"#).unwrap();
        let err = ingest_doc(&lib, &crops, &db, &IngestConfig::default(), u, 10).unwrap_err();
        assert!(err.contains("解析失败"), "{err}");
        assert_eq!(db.load(u).unwrap().entries[0].status, Status::Reviewed, "条目不动");
    }

    /// 回归：书被删/进回收站时，`Skipped`/`Archived` 这两种终态不该被"排除法"漏判成 `Revoked`
    /// （只有 Mined/Pending/Draft/Reviewed 这些还活着的条目才该因为"书不在了"转 Revoked）。
    #[test]
    fn revoke_stale_leaves_already_terminal_entries_alone() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().join("xochitl");
        let crops = t.path().join("crops");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(&crops).unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let cfg = IngestConfig::default();

        let u = "44444444-5e28-4969-8926-c8973d49020d";
        db.update(
            u,
            || Book { uuid: u.into(), title: "测试书四".into(), ..Default::default() },
            |b| {
                b.entries.push(seeded_entry("skipped", Status::Skipped));
                b.entries.push(seeded_entry("archived", Status::Archived));
                b.entries.push(seeded_entry("pending", Status::Pending));
            },
        )
        .unwrap();
        let s = ingest_doc(&lib, &crops, &db, &cfg, u, 10).unwrap().unwrap();
        assert_eq!(s.merge.revoked, 1, "只有 pending 那条该被转 Revoked");
        let entries = db.load(u).unwrap().entries;
        assert_eq!(entries[0].status, Status::Skipped, "Skipped 不该被改判");
        assert_eq!(entries[1].status, Status::Archived, "Archived 不该被改判");
        assert_eq!(entries[2].status, Status::Revoked, "真正活着的条目该转 Revoked");
    }
}
