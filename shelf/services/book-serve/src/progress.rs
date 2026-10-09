//! 原地替换后**找回阅读位置**（2026-10-09）。
//!
//! 电脑上的 sheng-ren 经 `POST /import?uuid=` 原地替换一本已经在读的 EPUB（[`crate::import`]）：删掉渲染缓存
//! `<uuid>.pdf`/`<uuid>.epubindex`、换上新 `<uuid>.epub`，xochitl 下次打开时整本重排。重排后页数变了，`lastOpenedPage`
//! 指的还是旧排版的页号，xochitl 自己按"旧页 / 旧总页数"比例估一页（DocumentView 的 `onPageCountChanged`），
//! 书一改动大就偏得很远，甚至回到开头。
//!
//! 直接改 `.metadata`/`.content` 没用（运行中的 xochitl 会用内存状态盖回去，同漫画页边距那次的结论），只能让 xochitl 自己跳页：
//!
//! 1. **快照**（[`Progress::before_replace`]，`finish_replace` 删旧 `.epubindex` **之前**调）：读 `.metadata` 的 `lastOpenedPage`
//!    （文档页序，0 起）→ 按旧 `.content` 的页表换成 PDF 页（[`PageMap`]）→ 用旧 `.epubindex` 找它落在哪个 spine 文件、
//!    文件内比例 `(页 − 起始页) / 该文件页数`，另存全书比例兜底。写 `<状态目录>/books/progress/<uuid>.json`（原子写）。
//!    任何一步失败只记日志，绝不影响替换本身。
//! 2. **换算**（[`Progress::lookup`]，`GET /progress/{uuid}`）：新 `.epubindex` 还没出现（不在，或修改时间早于快照时刻）→
//!    [`Lookup::Pending`]（202，QML 代理过几秒再问）；出现了 → 新排版里同一文件的起始页 + 比例 × 该文件页数（四舍五入、夹在
//!    该文件范围内）；文件找不到（改名、拆分）→ 全书比例 × 新总页数。再按新 `.content` 页表换回文档页序。
//! 3. **跳页**：注入 DocumentView 的 `shelf/xovi/shelf-keep-progress.qmd` 在书打开、页数变、目录到达后各等 2 秒去问，
//!    拿到页号就调 xochitl 自己的 `sceneView.goToPage(n)`，再 `POST /progress/applied` 删快照（每本只跳一次）。
//!
//! **同一本书再次替换**：上一份快照的目标排版还没出现（用户还没打开过新版，旧 `.epubindex` 早被上次替换删了，`lastOpenedPage`
//! 也还是最初的位置）→ 保留上一份，不重新快照（也没法重新快照）；目标排版已经出现过（用户打开过、xochitl 的 `lastOpenedPage`
//! 已经是新排版里的位置）→ 按当前状态重新快照、覆盖上一份；这时 `lastOpenedPage` ≤ 0 就删掉上一份（已经过时）。
//!
//! **已知限制**：粒度是"文件内比例"，同一文件里内容增删多时会偏几页；EPUB 里插过笔记页时页序换算靠 `.content` 页表，
//! 重排后 xochitl 怎么安置这些笔记页没核实过；新排版的总页数只在最后一个文件用得到，取不到可靠值时按最后一个文件 1 页算。
//! 快照超过 [`MAX_AGE`] 或书已不在库里，启动时清掉。**全部未真机验证**（要核的点见书架白皮书 §03ar）。
use epubpkg::epubindex::{parse_epubindex, Section};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

/// 快照保留多久：超过就当用户不会再打开这本书（或代理从没生效），启动时清掉。
pub const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

/// 一份快照（`progress/<uuid>.json`）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub uuid: String,
    /// 快照时刻（unix 毫秒）：新 `.epubindex` 的修改时间不早于它才算"新排版已经出来了"。
    pub at_ms: u64,
    /// 旧排版里的 PDF 页（0 起），只供排查。
    pub old_page: u32,
    /// 所在 spine 文件 basename；旧 `.epubindex` 里找不到所在文件时为空。
    pub file: Option<String>,
    /// 文件内比例 `[0, 1)`。
    pub file_ratio: f64,
    /// 全书比例 `[0, 1]`（文件找不到时兜底）。
    pub book_ratio: f64,
}

/// [`Progress::lookup`] 的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// 没有快照（不是替换过的书 / 已应用 / uuid 不对）→ 404。
    None,
    /// 有快照，新排版还没出来 → 202。
    Pending,
    /// 新排版里该跳到的文档页序（0 起）→ 200。
    Page(u32),
}

/// [`Progress::before_replace`] 做了什么（日志用）。
#[derive(Debug, Clone, PartialEq)]
pub enum Taken {
    /// 写了新快照。
    Saved(Snapshot),
    /// 上一份快照的目标排版还没出现，保留上一份。
    KeptPrevious,
    /// 不用存（没读过 / 在第一页 / 没有旧 `.epubindex` / 认不出页表）；有过期的上一份顺带删掉。
    Skipped,
}

pub struct Progress {
    dir: PathBuf,
    lib_dir: PathBuf,
}

impl Progress {
    pub fn new(state_books_dir: &Path, lib_dir: &Path) -> Progress {
        Progress { dir: state_books_dir.join("progress"), lib_dir: lib_dir.to_path_buf() }
    }

    fn file_of(&self, uuid: &str) -> PathBuf {
        self.dir.join(format!("{uuid}.json"))
    }

    fn lib_file(&self, uuid: &str, ext: &str) -> PathBuf {
        self.lib_dir.join(format!("{uuid}.{ext}"))
    }

    pub fn load(&self, uuid: &str) -> Option<Snapshot> {
        if !rmsvc_core::xochitl::is_uuid_shape(uuid) {
            return None;
        }
        let t = std::fs::read(self.file_of(uuid)).ok()?;
        serde_json::from_slice::<Snapshot>(&t).ok().filter(|s| s.uuid == uuid)
    }

    /// 原地替换前（旧 `.epubindex` 还在）调：按需写快照，策略见模块文档。失败回 `Err`，调用方只记日志。
    pub fn before_replace(&self, uuid: &str) -> Result<Taken, String> {
        if !rmsvc_core::xochitl::is_uuid_shape(uuid) {
            return Err("uuid 形状不对".into());
        }
        if let Some(prev) = self.load(uuid) {
            if self.new_index(&prev).is_none() {
                return Ok(Taken::KeptPrevious);
            }
        }
        match self.capture(uuid, now_ms()) {
            Some(s) => {
                let bytes = serde_json::to_vec(&s).map_err(|e| e.to_string())?;
                rmsvc_core::fs::write_atomic(&self.file_of(uuid), &bytes).map_err(|e| format!("写阅读位置快照失败: {e}"))?;
                Ok(Taken::Saved(s))
            }
            None => {
                self.remove(uuid);
                Ok(Taken::Skipped)
            }
        }
    }

    /// 按当前 `.metadata`/`.content`/`.epubindex` 算快照；不用存 → `None`。
    fn capture(&self, uuid: &str, at_ms: u64) -> Option<Snapshot> {
        let last = last_opened_page(&self.lib_file(uuid, "metadata"))?;
        if last <= 0 {
            return None;
        }
        let secs = parse_epubindex(&std::fs::read(self.lib_file(uuid, "epubindex")).ok()?);
        if secs.is_empty() {
            return None;
        }
        let pm = PageMap::read(&self.lib_file(uuid, "content"));
        let page = pm.pdf_of(last.min(u32::MAX as i64) as u32);
        let total = pm.pdf_total().filter(|&n| n > last_start(&secs)).unwrap_or(page.max(last_start(&secs)) + 1);
        let book_ratio = if total > 1 { (page as f64 / (total - 1) as f64).clamp(0.0, 1.0) } else { 0.0 };
        let (file, file_ratio) = match locate(&secs, page, Some(total)) {
            Some((i, start, len)) => (Some(secs[i].file.clone()), ((page - start) as f64 / len as f64).clamp(0.0, 1.0)),
            None => (None, 0.0),
        };
        Some(Snapshot { uuid: uuid.to_string(), at_ms, old_page: page, file, file_ratio, book_ratio })
    }

    /// 新排版的 `.epubindex`（已出现且不早于快照时刻、能认出条目）；还没有 → `None`。
    fn new_index(&self, s: &Snapshot) -> Option<Vec<Section>> {
        let p = self.lib_file(&s.uuid, "epubindex");
        if mtime_ms(&p)? < s.at_ms {
            return None;
        }
        Some(parse_epubindex(&std::fs::read(&p).ok()?)).filter(|v| !v.is_empty())
    }

    /// `GET /progress/{uuid}`。`pages_hint` 是 QML 代理带来的阅读器当前总页数（`?pages=`），新 `.content` 还没更新时
    /// 用来定最后一个文件有几页。
    pub fn lookup(&self, uuid: &str, pages_hint: Option<u32>) -> Lookup {
        let Some(s) = self.load(uuid) else { return Lookup::None };
        let Some(secs) = self.new_index(&s) else { return Lookup::Pending };
        // 新 `.content` 不早于快照才算新排版的页表；否则还是旧的，页序按恒等
        let content = self.lib_file(uuid, "content");
        let pm = if mtime_ms(&content).is_some_and(|m| m >= s.at_ms) { PageMap::read(&content) } else { PageMap::default() };
        let last = last_start(&secs);
        let total = pm.pdf_total().filter(|&n| n > last).or(pages_hint.filter(|&n| n > last));
        Lookup::Page(pm.doc_of(target_page(&s, &secs, total)))
    }

    /// `POST /progress/applied`：删快照（已跳过 / 代理放弃）。返回删没删到。
    pub fn applied(&self, uuid: &str) -> Result<bool, String> {
        if !rmsvc_core::xochitl::is_uuid_shape(uuid) {
            return Err("uuid 形状不对".into());
        }
        Ok(self.remove(uuid))
    }

    fn remove(&self, uuid: &str) -> bool {
        std::fs::remove_file(self.file_of(uuid)).is_ok()
    }

    /// 启动时清理：超过 `max_age`、读不了、书已不在库里的快照，以及上次被杀留下的半成品 `.tmp`。返回清掉几个。
    pub fn prune(&self, max_age: Duration) -> usize {
        let cutoff = now_ms().saturating_sub(max_age.as_millis() as u64);
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        let mut n = 0;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let stale = match name.strip_suffix(".json") {
                Some(uuid) => self.load(uuid).is_none_or(|s| s.at_ms < cutoff || !self.lib_file(uuid, "metadata").is_file()),
                None => name.ends_with(".tmp"),
            };
            if stale && std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
        n
    }
}

/// 新排版里的目标 PDF 页：同一文件按文件内比例，文件找不到按全书比例。
fn target_page(s: &Snapshot, secs: &[Section], total: Option<u32>) -> u32 {
    let last = last_start(secs);
    if let Some(i) = s.file.as_deref().and_then(|f| secs.iter().position(|x| x.file == f)) {
        let (start, len) = span(secs, i, total);
        let off = (s.file_ratio * len as f64).round() as u32;
        return start + off.min(len - 1);
    }
    match total {
        Some(n) => (s.book_ratio * (n - 1) as f64).round() as u32,
        None => (s.book_ratio * last as f64).round() as u32,
    }
}

fn last_start(secs: &[Section]) -> u32 {
    secs.last().map_or(0, |s| s.start_page)
}

/// 第 `i` 个文件的（起始页, 页数）：页数＝下一个文件的起始页 − 起始页；最后一个用 `total`，没有就按 1 页。至少 1。
fn span(secs: &[Section], i: usize, total: Option<u32>) -> (u32, u32) {
    let start = secs[i].start_page;
    let end = match secs.get(i + 1) {
        Some(n) => n.start_page,
        None => total.unwrap_or(start + 1),
    };
    (start, end.saturating_sub(start).max(1))
}

/// `page` 落在哪个文件：（下标, 起始页, 页数）。在第一个文件之前（不该有）→ `None`。
fn locate(secs: &[Section], page: u32, total: Option<u32>) -> Option<(usize, u32, u32)> {
    let i = secs.partition_point(|s| s.start_page <= page).checked_sub(1)?;
    let (start, len) = span(secs, i, total);
    // 超出最后一个文件的已知页数（总页数不可靠）：夹回文件末页
    Some((i, start, len.max(page - start + 1)))
}

/// `.metadata` 的 `lastOpenedPage`（数字或数字字符串）；读不了 / 没有 → `None`。
fn last_opened_page(meta: &Path) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(meta).ok()?).ok()?;
    let x = v.get("lastOpenedPage")?;
    x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
}

fn now_ms() -> u64 {
    rmsvc_core::clock::now_ms()
}

fn mtime_ms(p: &Path) -> Option<u64> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0))
}

/// `.content` 里的页表：文档页序 ↔ PDF 页。EPUB 里插了笔记页时两者不同（笔记页没有 PDF 页）。
///
/// - formatVersion 1：`redirectionPageMap[i]` = 文档第 i 页的 PDF 页，笔记页是 -1；
/// - formatVersion 2：`cPages.pages` 按 `idx.value` 字典序排、去掉 `deleted.value` ≠ 0 的页；页对象带 `redir.value` 时那就是
///   PDF 页（EPUB 的 v2 页对象带不带 `redir` 没在真机上确认过），一页都不带就按恒等。
///
/// 认不出 / 读不了 → 恒等（`map` 为空）。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PageMap {
    map: Option<Vec<Option<u32>>>,
    page_count: Option<u32>,
}

impl PageMap {
    pub fn read(content: &Path) -> PageMap {
        std::fs::read(content).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).map(|v| PageMap::parse(&v)).unwrap_or_default()
    }

    pub fn parse(v: &serde_json::Value) -> PageMap {
        let page_count = v.get("pageCount").and_then(|x| x.as_u64()).filter(|&n| n > 0).map(|n| n.min(u32::MAX as u64) as u32);
        let as_page = |x: &serde_json::Value| x.as_i64().filter(|&n| n >= 0 && n <= u32::MAX as i64).map(|n| n as u32);
        let map = if let Some(pages) = v.pointer("/cPages/pages").and_then(|p| p.as_array()) {
            let val = |p: &serde_json::Value, k: &str| p.get(k).and_then(|o| o.get("value")).cloned();
            let mut live: Vec<(String, Option<u32>)> = pages
                .iter()
                .filter(|p| val(p, "deleted").and_then(|d| d.as_i64()).unwrap_or(0) == 0)
                .map(|p| (val(p, "idx").and_then(|i| i.as_str().map(str::to_string)).unwrap_or_default(), val(p, "redir").as_ref().and_then(as_page)))
                .collect();
            live.sort_by(|a, b| a.0.cmp(&b.0));
            live.iter().any(|(_, r)| r.is_some()).then(|| live.into_iter().map(|(_, r)| r).collect())
        } else {
            v.get("redirectionPageMap").and_then(|m| m.as_array()).filter(|m| !m.is_empty()).map(|m| m.iter().map(as_page).collect())
        };
        PageMap { map, page_count }
    }

    /// 文档页序 → PDF 页；没有页表、越界、笔记页 → 原值。
    pub fn pdf_of(&self, doc: u32) -> u32 {
        self.map.as_ref().and_then(|m| m.get(doc as usize).copied().flatten()).unwrap_or(doc)
    }

    /// PDF 页 → 文档页序（第一个映射到它的文档页）；没有页表或找不到 → 原值。
    pub fn doc_of(&self, pdf: u32) -> u32 {
        self.map.as_ref().and_then(|m| m.iter().position(|&p| p == Some(pdf))).map_or(pdf, |i| i as u32)
    }

    /// PDF 总页数：有页表取最大 PDF 页 + 1，否则 `pageCount`。
    pub fn pdf_total(&self) -> Option<u32> {
        match &self.map {
            Some(m) => m.iter().flatten().max().map(|&p| p + 1),
            None => self.page_count,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub const U: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    /// 真机《人骨拼圖》的 `.epubindex`（39 个 spine 文件、523 页；`1.xhtml` 起始 3，`7.xhtml` 起始 97，`37.xhtml` 起始 510）
    pub const REAL_INDEX: &[u8] = include_bytes!("../../../../rmsvc-core/epubpkg/testdata/renggu.epubindex");

    /// 按 `.epubindex` 的条目格式造一份（`u32 长度 + UTF-16BE 路径 + 3 × u32`，第一个 u32 是起始页）。
    pub fn fake_index(entries: &[(&str, u32)]) -> Vec<u8> {
        let mut b = vec![0u8; 16]; // 文件头随便放点东西，解析按条目形状扫
        for (name, start) in entries {
            let path: Vec<u8> = format!("OEBPS/{name}").bytes().flat_map(|c| [0, c]).collect();
            b.extend((path.len() as u32).to_be_bytes());
            b.extend(path);
            b.extend(start.to_be_bytes());
            b.extend([0u8; 8]);
        }
        b
    }

    /// 真样本"重排"成新版：每个文件起始页按 `f(旧起始页)` 变（模拟换字体 / 改版后页数整体变）。
    pub fn relaid(f: impl Fn(u32) -> u32, drop: &[&str]) -> Vec<u8> {
        let secs = parse_epubindex(REAL_INDEX);
        let owned: Vec<(String, u32)> = secs.iter().filter(|s| !drop.contains(&s.file.as_str())).map(|s| (s.file.clone(), f(s.start_page))).collect();
        fake_index(&owned.iter().map(|(n, p)| (n.as_str(), *p)).collect::<Vec<_>>())
    }

    pub fn setup(t: &tempfile::TempDir, last: i64, content: &serde_json::Value) -> (Progress, PathBuf) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(format!("{U}.metadata")), json!({"type":"DocumentType","visibleName":"人骨拼圖","parent":"","lastOpenedPage":last}).to_string()).unwrap();
        std::fs::write(lib.join(format!("{U}.content")), content.to_string()).unwrap();
        std::fs::write(lib.join(format!("{U}.epubindex")), REAL_INDEX).unwrap();
        (Progress::new(&t.path().join("state"), &lib), lib)
    }

    /// 模拟替换 + xochitl 重排：删旧索引，写新索引（修改时间晚于快照）。
    fn relayout(lib: &Path, idx: &[u8]) {
        let p = lib.join(format!("{U}.epubindex"));
        let _ = std::fs::remove_file(&p);
        std::thread::sleep(Duration::from_millis(5));
        std::fs::write(&p, idx).unwrap();
    }

    fn set_mtime(p: &Path, ms_ago: u64) {
        let t = std::time::SystemTime::now() - Duration::from_millis(ms_ago);
        std::fs::File::options().write(true).open(p).unwrap().set_modified(t).unwrap();
    }

    fn v1_identity(n: u32) -> serde_json::Value {
        json!({"fileType":"epub","formatVersion":1,"pageCount":n,"redirectionPageMap":(0..n).collect::<Vec<_>>()})
    }

    #[test]
    fn page_map_v1_redirection_with_note_pages_and_bad_entries() {
        let pm = PageMap::parse(&json!({"formatVersion":1,"pageCount":5,"redirectionPageMap":[0,1,-1,2,3]}));
        assert_eq!((pm.pdf_of(0), pm.pdf_of(3), pm.pdf_of(4)), (0, 2, 3), "笔记页之后的文档页往前挪一个 PDF 页");
        assert_eq!(pm.pdf_of(2), 2, "笔记页（-1）按原值");
        assert_eq!(pm.pdf_of(99), 99, "越界按原值");
        assert_eq!((pm.doc_of(2), pm.doc_of(3), pm.doc_of(77)), (3, 4, 77));
        assert_eq!(pm.pdf_total(), Some(4));
        let none = PageMap::parse(&json!({"pageCount":7}));
        assert_eq!((none.pdf_of(5), none.doc_of(5), none.pdf_total()), (5, 5, Some(7)), "没有页表 → 恒等，总数用 pageCount");
    }

    #[test]
    fn page_map_v2_sorted_by_idx_drops_deleted_and_uses_redir() {
        let page = |idx: &str, redir: Option<u32>, deleted: i64| {
            let mut p = json!({"id": idx, "idx": {"timestamp":"1:1","value": idx}});
            if let Some(r) = redir {
                p["redir"] = json!({"timestamp":"1:1","value": r});
            }
            if deleted != 0 {
                p["deleted"] = json!({"timestamp":"1:1","value": deleted});
            }
            p
        };
        // 乱序存放；"bb" 是插进来的笔记页（没有 redir）；"ab" 已删除
        let v = json!({"formatVersion":2,"pageCount":4,"cPages":{"pages":[page("bc",Some(2),0), page("ba",Some(0),0), page("ab",Some(9),1), page("bb",None,0), page("bd",Some(3),0)]}});
        let pm = PageMap::parse(&v);
        assert_eq!((pm.pdf_of(0), pm.pdf_of(1), pm.pdf_of(2), pm.pdf_of(3)), (0, 1, 2, 3), "第 1 页是笔记页 → 原值");
        assert_eq!((pm.doc_of(2), pm.doc_of(3)), (2, 3));
        assert_eq!(pm.pdf_total(), Some(4));
        // 一页都没有 redir → 恒等
        let plain = PageMap::parse(&json!({"formatVersion":2,"pageCount":3,"cPages":{"pages":[page("b",None,0), page("a",None,0)]}}));
        assert_eq!((plain.pdf_of(1), plain.doc_of(1), plain.pdf_total()), (1, 1, Some(3)));
    }

    #[test]
    fn same_file_ratio_maps_into_new_layout() {
        let t = tempfile::tempdir().unwrap();
        // 7.xhtml 起始 97，下一个文件起始页从真样本里取
        let secs = parse_epubindex(REAL_INDEX);
        let i7 = secs.iter().position(|s| s.file == "7.xhtml").unwrap();
        let next = secs[i7 + 1].start_page;
        let mid = 97 + (next - 97) / 2;
        let (p, lib) = setup(&t, mid as i64, &v1_identity(523));
        let Taken::Saved(s) = p.before_replace(U).unwrap() else { panic!("应写快照") };
        assert_eq!((s.file.as_deref(), s.old_page), (Some("7.xhtml"), mid));
        assert!((s.file_ratio - (mid - 97) as f64 / (next - 97) as f64).abs() < 1e-9);
        // 新版：每页字少了，页数 ×1.5
        relayout(&lib, &relaid(|x| x * 3 / 2, &[]));
        let (ns, nn) = (97 * 3 / 2, next * 3 / 2);
        let want = ns + (s.file_ratio * (nn - ns) as f64).round() as u32;
        assert_eq!(p.lookup(U, None), Lookup::Page(want));
        assert!(want >= ns && want < nn);
        assert_eq!(p.applied(U), Ok(true));
        assert_eq!(p.lookup(U, None), Lookup::None, "应用后不再返回");
    }

    #[test]
    fn note_pages_convert_doc_to_pdf_and_back() {
        let t = tempfile::tempdir().unwrap();
        // 第 5 页后插了一张笔记页：文档页 101 = PDF 页 100（7.xhtml 里）
        let mut map: Vec<i64> = (0..523).collect();
        map.insert(5, -1);
        let (p, lib) = setup(&t, 101, &json!({"formatVersion":1,"pageCount":524,"redirectionPageMap":map}));
        let Taken::Saved(s) = p.before_replace(U).unwrap() else { panic!() };
        assert_eq!((s.old_page, s.file.as_deref()), (100, Some("7.xhtml")));
        // 新排版页数不变、新 .content 还是同一份页表（同样有笔记页）：应回到文档页 101
        relayout(&lib, REAL_INDEX);
        std::fs::write(lib.join(format!("{U}.content")), json!({"formatVersion":1,"pageCount":524,"redirectionPageMap":map}).to_string()).unwrap();
        assert_eq!(p.lookup(U, None), Lookup::Page(101));
    }

    #[test]
    fn renamed_file_falls_back_to_book_ratio() {
        let t = tempfile::tempdir().unwrap();
        let (p, lib) = setup(&t, 261, &v1_identity(523));
        let Taken::Saved(s) = p.before_replace(U).unwrap() else { panic!() };
        let file = s.file.clone().unwrap();
        assert!((s.book_ratio - 261.0 / 522.0).abs() < 1e-9);
        // 新版把所在文件改了名（不在新索引里），总页数变成 1046
        relayout(&lib, &relaid(|x| x * 2, &[file.as_str()]));
        std::fs::write(lib.join(format!("{U}.content")), v1_identity(1046).to_string()).unwrap();
        let want = (s.book_ratio * 1045.0).round() as u32;
        assert_eq!(p.lookup(U, None), Lookup::Page(want), "全书比例 × (新总页数 − 1)");
        // 新 .content 还没写（早于快照）→ 用 QML 带来的总页数；也没有 → 用最后一个文件的起始页
        set_mtime(&lib.join(format!("{U}.content")), 60_000);
        assert_eq!(p.lookup(U, Some(1046)), Lookup::Page(want));
        let last = last_start(&parse_epubindex(&relaid(|x| x * 2, &[file.as_str()])));
        assert_eq!(p.lookup(U, None), Lookup::Page((s.book_ratio * last as f64).round() as u32));
    }

    #[test]
    fn last_file_uses_total_pages() {
        let t = tempfile::tempdir().unwrap();
        let (p, lib) = setup(&t, 520, &v1_identity(523)); // 第 520 页落在最末几个文件里（按真样本实际取）
        let secs = parse_epubindex(REAL_INDEX);
        let Taken::Saved(s) = p.before_replace(U).unwrap() else { panic!() };
        let li = secs.partition_point(|x| x.start_page <= 520) - 1;
        assert_eq!(s.file.as_deref(), Some(secs[li].file.as_str()));
        relayout(&lib, &relaid(|x| x * 2, &[]));
        std::fs::write(lib.join(format!("{U}.content")), v1_identity(1046).to_string()).unwrap();
        let Lookup::Page(n) = p.lookup(U, None) else { panic!() };
        assert!(n >= secs[li].start_page * 2 && n < 1046, "落在新版同一文件范围内：{n}");
    }

    #[test]
    fn pending_until_new_index_appears_then_page_then_404() {
        let t = tempfile::tempdir().unwrap();
        let (p, lib) = setup(&t, 200, &v1_identity(523));
        assert_eq!(p.lookup(U, None), Lookup::None, "没快照 → 404");
        assert!(matches!(p.before_replace(U).unwrap(), Taken::Saved(_)));
        // 替换删了旧索引 → pending
        std::fs::remove_file(lib.join(format!("{U}.epubindex"))).unwrap();
        assert_eq!(p.lookup(U, None), Lookup::Pending);
        // 出现了但修改时间早于快照（旧的没删掉）→ 仍 pending
        std::fs::write(lib.join(format!("{U}.epubindex")), REAL_INDEX).unwrap();
        set_mtime(&lib.join(format!("{U}.epubindex")), 60_000);
        assert_eq!(p.lookup(U, None), Lookup::Pending);
        relayout(&lib, REAL_INDEX);
        assert_eq!(p.lookup(U, None), Lookup::Page(200), "同一排版 → 原页");
        assert_eq!(p.applied(U), Ok(true));
        assert_eq!(p.applied(U), Ok(false));
        assert_eq!(p.lookup(U, None), Lookup::None);
        assert!(p.applied("../x").is_err());
        assert_eq!(p.lookup("../x", None), Lookup::None);
    }

    #[test]
    fn no_snapshot_for_first_page_or_missing_index() {
        let t = tempfile::tempdir().unwrap();
        let (p, lib) = setup(&t, 0, &v1_identity(523));
        assert_eq!(p.before_replace(U).unwrap(), Taken::Skipped, "在第一页不存");
        let (p, _) = setup(&t, 50, &v1_identity(523));
        std::fs::remove_file(lib.join(format!("{U}.epubindex"))).unwrap();
        assert_eq!(p.before_replace(U).unwrap(), Taken::Skipped, "没有旧索引不存");
        assert_eq!(p.lookup(U, None), Lookup::None);
    }

    #[test]
    fn replacing_again_keeps_pending_snapshot_or_overwrites_after_seen() {
        let t = tempfile::tempdir().unwrap();
        let (p, lib) = setup(&t, 200, &v1_identity(523));
        let Taken::Saved(first) = p.before_replace(U).unwrap() else { panic!() };
        std::fs::remove_file(lib.join(format!("{U}.epubindex"))).unwrap();
        // 用户还没打开新版又替换一次：保留第一份
        assert_eq!(p.before_replace(U).unwrap(), Taken::KeptPrevious);
        assert_eq!(p.load(U), Some(first.clone()));
        // 打开过（新索引出来了）但没应用，用户读到 300 后再替换：按当前状态重新快照
        relayout(&lib, &relaid(|x| x * 2, &[]));
        std::fs::write(lib.join(format!("{U}.metadata")), json!({"type":"DocumentType","lastOpenedPage":300}).to_string()).unwrap();
        let Taken::Saved(second) = p.before_replace(U).unwrap() else { panic!() };
        assert_eq!(second.old_page, 300);
        assert!(second.at_ms >= first.at_ms);
        // 第二次替换删了索引，用户打开过（新索引出来了）、lastOpenedPage 回到 0 后再替换：过时的快照删掉
        relayout(&lib, REAL_INDEX);
        std::fs::write(lib.join(format!("{U}.metadata")), json!({"type":"DocumentType","lastOpenedPage":0}).to_string()).unwrap();
        assert_eq!(p.before_replace(U).unwrap(), Taken::Skipped);
        assert_eq!(p.load(U), None);
    }

    #[test]
    fn prune_drops_old_broken_orphaned_and_tmp() {
        let t = tempfile::tempdir().unwrap();
        let (p, _lib) = setup(&t, 200, &v1_identity(523));
        p.before_replace(U).unwrap();
        assert_eq!(p.prune(MAX_AGE), 0, "新快照不清");
        let dir = t.path().join("state").join("progress");
        let other = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"; // 书不在库里
        std::fs::write(dir.join(format!("{other}.json")), serde_json::to_vec(&Snapshot { uuid: other.into(), ..p.load(U).unwrap() }).unwrap()).unwrap();
        std::fs::write(dir.join("ffffffff-ffff-4fff-8fff-ffffffffffff.json"), b"not json").unwrap();
        std::fs::write(dir.join("x.json.1.0.tmp"), b"half").unwrap();
        assert_eq!(p.prune(MAX_AGE), 3);
        assert!(p.load(U).is_some());
        // 31 天前的快照
        let old = Snapshot { at_ms: now_ms() - 31 * 24 * 3600 * 1000, ..p.load(U).unwrap() };
        std::fs::write(dir.join(format!("{U}.json")), serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(p.prune(MAX_AGE), 1);
        assert_eq!(p.load(U), None);
    }

    #[test]
    fn snapshot_write_failure_is_an_error_not_a_panic() {
        let t = tempfile::tempdir().unwrap();
        let (_, lib) = setup(&t, 200, &v1_identity(523));
        // 状态目录的 progress 位置被一个普通文件占着：写不进去
        std::fs::create_dir_all(t.path().join("state")).unwrap();
        std::fs::write(t.path().join("state").join("progress"), b"").unwrap();
        let p = Progress::new(&t.path().join("state"), &lib);
        assert!(p.before_replace(U).is_err());
    }
}
