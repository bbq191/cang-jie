//! 条目库（Repository）：一书一文件 `$XDG_STATE_HOME/notes/books/<uuid>.json`，原子写。ink-serve 是**唯一写者**——
//! 转写/脑/本三服务都通过它的 HTTP 改条目字段，避免多进程同时改一份 JSON。
use notecore::model::{Book, Status};
use rmsvc_core::fs::{plain_name, write_atomic};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct BookDb {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl BookDb {
    pub fn new(dir: PathBuf) -> BookDb {
        BookDb { dir, lock: Mutex::new(()) }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }
    /// `<dir>/<uuid>.json`。uuid 来自 URL 路径参数（网关解码后可含 `/`、`..`），过 `plain_name`
    /// 单段校验，防止读写条目库目录之外的 .json。
    fn path(&self, uuid: &str) -> Result<PathBuf, String> {
        Ok(self.dir.join(format!("{}.json", plain_name(uuid)?)))
    }

    /// 读一本书：文件不存在 → `Ok(None)`；读失败或 JSON 解析失败 → `Err`。**绝不能把"坏了"当成"没有"**：
    /// 此前两者都返回 `None`，`update` 随即用 `seed()` 的空书整本覆盖掉——降级部署遇到不认识的枚举值、
    /// 文件被改坏，校对文本/AI 回答就静默全丢（2026-09-24 审查）。解析失败时另存一份 `<uuid>.json.corrupt`
    /// 副本（已有就不重复拷），原文件原样不动，后续写入一律拒绝，直到人工处理。
    /// 非法 uuid（含 `/`、`..`）同样当作不存在。
    pub fn read(&self, uuid: &str) -> Result<Option<Book>, String> {
        let Ok(p) = self.path(uuid) else { return Ok(None) };
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("读条目库 {uuid} 失败: {e}")),
        };
        serde_json::from_slice(&bytes).map(Some).map_err(|e| {
            let bak = p.with_extension("json.corrupt");
            if !bak.exists() {
                let _ = std::fs::copy(&p, &bak);
            }
            let msg = format!("条目库 {uuid}.json 解析失败，已另存 .corrupt 副本、原文件未动、拒绝覆盖写入: {e}");
            eprintln!("[ink-serve] {msg}");
            msg
        })
    }

    #[cfg(test)]
    pub fn load(&self, uuid: &str) -> Option<Book> {
        self.read(uuid).ok().flatten()
    }

    pub fn save(&self, book: &Book) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?;
        write_atomic(&self.path(&book.uuid)?, &bytes).map_err(|e| format!("写条目库失败: {e}"))
    }

    /// 读—改—写（进程内串行化）。书不存在时以 `seed()` 起。
    pub fn update<T>(&self, uuid: &str, seed: impl FnOnce() -> Book, f: impl FnOnce(&mut Book) -> T) -> Result<T, String> {
        self.path(uuid)?; // 先验 key：非法 uuid 不跑 f、不落盘
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut book = self.read(uuid)?.unwrap_or_else(seed);
        let out = f(&mut book);
        self.save(&book)?;
        Ok(out)
    }

    /// 读—改—写**已存在**的书（进程内串行化）：书不存在（或 uuid 非法）→ `Ok(None)`，不跑 `f`、不落盘、不建空书。
    /// 取代此前"先 `load` 判存在、再 `update(.., || Default::default(), ..)`"的两步（多解析一遍整本 JSON，
    /// 且两步之间不在同一把锁里）。
    pub fn update_existing<T>(&self, uuid: &str, f: impl FnOnce(&mut Book) -> T) -> Result<Option<T>, String> {
        if self.path(uuid).is_err() {
            return Ok(None);
        }
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut book) = self.read(uuid)? else { return Ok(None) };
        let out = f(&mut book);
        self.save(&book)?;
        Ok(Some(out))
    }

    /// 所有书（按标题排序）——含条目已全部撤销的书（内部/调试用；网页列表用 `list_active`）。
    pub fn list(&self) -> Vec<Book> {
        let mut out: Vec<Book> = std::fs::read_dir(&self.dir)
            .map(|rd| rd.flatten().filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json")).filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()).collect())
            .unwrap_or_default();
        out.sort_by(|a, b| a.title.cmp(&b.title));
        out
    }

    /// 只列"现在还有活条目"的书（按标题排序）：摄取门槛看的是"页 .rm 文件存不存在"，笔画擦光了文件不会被删，
    /// 一本书清空所有勾画/手写后摄取仍会跑（正确把条目标成 Revoked），但如果不过滤，它会因为"曾经有过
    /// .rm 文件"永远赖在网页的书选择列表里——即使当下一条活条目都没有（2026-09-07 真机验证时发现）。
    pub fn list_active(&self) -> Vec<Book> {
        self.list().into_iter().filter(|b| b.entries.iter().any(|e| e.status != Status::Revoked)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecore::model::Entry;

    fn entry(id: &str, status: Status) -> Entry {
        Entry { id: id.into(), page: "p".into(), page_index: 0, chapter: None, chapter_title: String::new(), subhead: None, quote: None, ink: None, drafts: vec![], text: None, style: Default::default(), ask_ai: false, question: None, answer: None, status, destination: Default::default(), source: Default::default(), created: 0, updated: 0 }
    }

    #[test]
    fn list_active_hides_books_whose_entries_are_all_revoked() {
        let t = tempfile::tempdir().unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        db.update("live", || Book { uuid: "live".into(), title: "还有活条目".into(), ..Default::default() }, |b| b.entries.push(entry("e1", Status::Reviewed))).unwrap();
        db.update("cleared", || Book { uuid: "cleared".into(), title: "已清空".into(), ..Default::default() }, |b| b.entries.push(entry("e2", Status::Revoked))).unwrap();
        db.update("empty", || Book { uuid: "empty".into(), title: "从没标注过".into(), ..Default::default() }, |_| ()).unwrap();

        assert_eq!(db.list().len(), 3, "list() 不过滤，三本都在");
        let active = db.list_active();
        assert_eq!(active.iter().map(|b| b.uuid.as_str()).collect::<Vec<_>>(), ["live"], "已清空/从没标注过的书不该出现在 list_active");
    }

    #[test]
    fn update_seeds_saves_and_lists() {
        let t = tempfile::tempdir().unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        assert!(db.load("u1").is_none());
        let n = db.update("u1", || Book { uuid: "u1".into(), title: "乙".into(), ..Default::default() }, |b| { b.chapters.push("一".into()); b.chapters.len() }).unwrap();
        assert_eq!(n, 1);
        db.update("u2", || Book { uuid: "u2".into(), title: "甲".into(), ..Default::default() }, |_| ()).unwrap();
        assert_eq!(db.load("u1").unwrap().chapters, vec!["一"]);
        assert_eq!(db.list().iter().map(|b| b.title.as_str()).collect::<Vec<_>>(), ["乙", "甲"], "按标题码位排序（乙 U+4E59 < 甲 U+7532）");
    }

    #[test]
    fn update_existing_never_creates_a_book_and_handles_bad_uuid() {
        let t = tempfile::tempdir().unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let mut ran = false;
        assert_eq!(db.update_existing("ghost", |_| ran = true).unwrap(), None);
        assert_eq!(db.update_existing("../evil", |_| ran = true).unwrap(), None, "非法 uuid 同样当作不存在");
        assert!(!ran, "不存在的书不跑闭包");
        assert!(db.list().is_empty() && !t.path().join("books/ghost.json").exists(), "不能悄悄建出空书");
        db.update("u1", || Book { uuid: "u1".into(), title: "甲".into(), ..Default::default() }, |_| ()).unwrap();
        assert_eq!(db.update_existing("u1", |b| { b.chapters.push("一".into()); b.chapters.len() }).unwrap(), Some(1));
        assert_eq!(db.load("u1").unwrap().chapters, vec!["一"], "改动已落盘");
    }

    /// 回归：条目库 JSON 解析失败时不能被当成新书、用空书覆盖掉；原文件不动，另存 .corrupt 副本。
    #[test]
    fn corrupt_book_is_never_overwritten() {
        let t = tempfile::tempdir().unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        let f = t.path().join("books/u1.json");
        let bad = br#"{"uuid":"u1","title":"x","entries":[{"status":"FutureStatus"}]}"#;
        std::fs::write(&f, bad).unwrap();

        assert!(db.read("u1").unwrap_err().contains("解析失败"));
        let mut ran = false;
        assert!(db.update("u1", || Book { uuid: "u1".into(), ..Default::default() }, |_| ran = true).is_err());
        assert!(db.update_existing("u1", |_| ran = true).is_err());
        assert!(!ran, "坏文件不跑闭包");
        assert_eq!(std::fs::read(&f).unwrap(), bad, "原文件原样不动");
        assert_eq!(std::fs::read(t.path().join("books/u1.json.corrupt")).unwrap(), bad, "另存了副本");
        assert!(db.read("nope").unwrap().is_none(), "不存在仍是 Ok(None)");
    }

    /// 回归：uuid 带 `/`、`..` 不能读写条目库目录之外的文件。
    #[test]
    fn rejects_path_traversal_uuid() {
        let t = tempfile::tempdir().unwrap();
        let db = BookDb::new(t.path().join("books"));
        db.ensure().unwrap();
        std::fs::write(t.path().join("outside.json"), r#"{"uuid":"o","title":"外面"}"#).unwrap();
        assert!(db.load("../outside").is_none(), "不能读到目录之外的 json");
        let mut ran = false;
        let r = db.update("../evil", || Book { uuid: "../evil".into(), title: "x".into(), ..Default::default() }, |_| ran = true);
        assert!(r.is_err() && !ran, "非法 uuid 直接拒绝，不跑闭包");
        assert!(!t.path().join("evil.json").exists());
    }
}
