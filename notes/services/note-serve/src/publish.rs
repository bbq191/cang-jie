//! 生成编排（Strategy + 纯函数骨架，trait 全桩、可离线单测；真机网络只在 `XochitlUploader`/`BookServeTrash`/
//! `InkHttp` 三处生产实现里）：
//! 条目库取书 → `notecore::project` 投影每一章 → 指纹未变就跳过（不重传） → `rmv6::write` 打包 `.rmdoc` →
//! `/upload` 进《书名》文件夹 → 按 `visibleName`+时间窗认领刚生成的设备 uuid → 这一章如果之前生成过、
//! 且这次真的换了新文档，旧版本入 `book-serve` 回收站队列 → 记新记录（`notebooks.rs`）。
//! 一章失败不影响其它章；旧版本入队失败也不算这一章失败（新文档已经生成好了，旧的多留一份不是数据丢失）。
//!
//! 目标文件夹不存在时 `Xochitl::upload` 本身会 best-effort 落书库根——`generate_chapter` 上传前先调
//! `Uploader::ensure_folder` 请求建夹代理创建（2026-09-07 补，见 `mkdir.rs`/`shelf-mkdir-agent.qmd`）：
//! fire-and-forget，不等待、不影响本次结果，这次大概率还是落根目录，下次这本书再生成别的章节时
//! 文件夹多半已经建好了。
use crate::config::NoteConfig;
use crate::ink::EntryStore;
use crate::notebooks::{ChapterRecord, NotebookState};
use crate::rmdoc::{self, Page};
use crate::trash::TrashSink;
use notecore::model::Book;
use notecore::project::{fingerprint_chapter, project_chapter};
use rmv6::write::build_page_rm;
use serde::Serialize;

/// 传书 + 认领 + 建夹三件事的抽象；生产实现包一层 `shelf_core::xochitl::Xochitl`，测试用内存桩——不真的碰网络。
pub trait Uploader: Send + Sync {
    /// 上传一份 `.rmdoc`，进 `folder_name`（找不到该文件夹 → best-effort 落书库根）。
    fn upload(&self, bytes: &[u8], filename: &str, folder_name: &str) -> Result<(), String>;
    /// 找 `createdTime >= since_ms` 且 `visibleName == visible_name` 的文档，返回设备分配的新 uuid。
    fn claim(&self, visible_name: &str, since_ms: u64) -> Result<String, String>;
    /// 确保 `folder_name` 存在；不存在就请求建夹代理创建。fire-and-forget——生产实现判断"已存在就不
    /// 重复请求"，失败只记日志不影响本次上传；缺省空实现方便不关心这件事的测试桩少写一个方法。
    fn ensure_folder(&self, _folder_name: &str) {}
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChapterOutcome {
    /// 真生成了新文档（含"第一次生成"）。
    Generated { doc_uuid: String },
    /// 指纹没变，跳过（没碰网络）。
    Unchanged,
    /// 这一章没有可投影的条目（全部待转写占位也算"有"，这里指真的一条都没有/全撤销）。
    Empty,
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterResult {
    pub chapter: usize,
    pub title: String,
    #[serde(flatten)]
    pub outcome: ChapterOutcome,
}

pub struct Ctx<'a> {
    pub store: &'a dyn EntryStore,
    pub uploader: &'a dyn Uploader,
    pub trash: &'a dyn TrashSink,
    pub state: &'a NotebookState,
    pub cfg: &'a NoteConfig,
    pub now_ms: u64,
}

/// 生成/校验一章。`book` 由调用方先取好（`generate_book` 一次取书、对每章调用它，避免重复请求 ink-serve）。
pub fn generate_chapter(c: &Ctx, book: &Book, idx: usize) -> ChapterResult {
    let Some(title) = book.chapters.get(idx).cloned() else {
        return ChapterResult { chapter: idx, title: String::new(), outcome: ChapterOutcome::Failed { error: "没有这一章".into() } };
    };
    let Some(paragraphs) = project_chapter(book, idx) else {
        // 这一章没有可投影的条目了（全撤销/归档/跳过）——清掉旧记录，不然 `generated_at` 永远非空，
        // 「整理」页的 `everExported` 会永久误判这一章"已导出"（幽灵已导出）。跟 export.rs::export_chapter
        // 同一处理，两条投影路径保持对称（2026-09-09 补，此前只有 export.rs 这么做，见 chapter_store.rs
        // 的 `clear()` 注释）。旧设备文档本身不联动清理——不是这次要解决的问题，且贸然删用户书库里的
        // 文档风险更高，只清本地记录不会丢用户数据。
        if let Err(e) = c.state.clear(&book.uuid, idx) {
            eprintln!("[note-serve] 清空第 {} 章旧生成记录失败，先留着，不阻塞本次结果: {e}", idx + 1);
        }
        return ChapterResult { chapter: idx, title, outcome: ChapterOutcome::Empty };
    };
    let fingerprint = fingerprint_chapter(book, idx).unwrap_or_default();
    let existing = c.state.get(&book.uuid, idx);
    if existing.as_ref().map(|r| r.fingerprint.as_str()) == Some(fingerprint.as_str()) {
        return ChapterResult { chapter: idx, title, outcome: ChapterOutcome::Unchanged };
    }

    let outcome = (|| -> Result<String, String> {
        let rm = build_page_rm(rmdoc::TEMPLATE, &paragraphs)?;
        let doc_uuid = uuid::Uuid::new_v4().to_string();
        let page = Page { uuid: uuid::Uuid::new_v4().to_string(), rm_bytes: rm };
        let visible_name = format!("第{}章 {}", idx + 1, title);
        let folder = c.cfg.folder_name(&book.title);
        let bytes = rmdoc::pack(&doc_uuid, &visible_name, "", &page, rmdoc::TEMPLATE_AUTHOR, c.now_ms)?;
        c.uploader.ensure_folder(&folder);
        c.uploader.upload(&bytes, &format!("{doc_uuid}.rmdoc"), &folder)?;
        let new_uuid = c.uploader.claim(&visible_name, c.now_ms)?;
        if let Some(old) = &existing {
            if old.doc_uuid != new_uuid {
                if let Err(e) = c.trash.add(&old.doc_uuid, &old.visible_name) {
                    eprintln!("[note-serve] 旧版本 {}（《{}》）入回收站队列失败，先留着，不阻塞本次生成: {e}", old.doc_uuid, old.visible_name);
                }
            }
        }
        c.state.set(&book.uuid, idx, ChapterRecord { doc_uuid: new_uuid.clone(), visible_name, fingerprint, generated_at: c.now_ms })?;
        Ok(new_uuid)
    })();

    let outcome = match outcome {
        Ok(doc_uuid) => ChapterOutcome::Generated { doc_uuid },
        Err(error) => ChapterOutcome::Failed { error },
    };
    ChapterResult { chapter: idx, title, outcome }
}

/// 一本书全部章节各生成/校验一遍。
pub fn generate_book(c: &Ctx, book_uuid: &str) -> Result<Vec<ChapterResult>, String> {
    let book = c.store.book(book_uuid)?;
    Ok((0..book.chapters.len()).map(|i| generate_chapter(c, &book, i)).collect())
}

/// 生产实现：包一层 `shelf_core::xochitl::Xochitl` + 建夹代理客户端。
pub struct XochitlUploader {
    xochitl: shelf_core::xochitl::Xochitl,
    mkdir: Box<dyn crate::mkdir::MkdirSink>,
}

impl XochitlUploader {
    pub fn new(host: &str, library_dir: &std::path::Path, timeout_secs: u64, mkdir: Box<dyn crate::mkdir::MkdirSink>) -> XochitlUploader {
        XochitlUploader { xochitl: shelf_core::xochitl::Xochitl::new(host, library_dir, timeout_secs), mkdir }
    }
}

impl Uploader for XochitlUploader {
    fn upload(&self, bytes: &[u8], filename: &str, folder_name: &str) -> Result<(), String> {
        self.xochitl.upload(bytes, filename, "application/zip", folder_name).map(|_| ())
    }
    fn claim(&self, visible_name: &str, since_ms: u64) -> Result<String, String> {
        shelf_core::xochitl::find_documents_since(self.xochitl.library_dir(), since_ms)
            .into_iter()
            .find(|d| d.visible_name == visible_name)
            .map(|d| d.uuid)
            .ok_or_else(|| format!("上传后没能在书库里认领到《{visible_name}》（createdTime>={since_ms}），也许还没渲染完，稍后在网页重试"))
    }
    fn ensure_folder(&self, folder_name: &str) {
        if folder_name.is_empty() || self.xochitl.find_folder(folder_name).is_some() {
            return; // 空名（落根目录）或已存在，都不用建
        }
        if let Err(e) = self.mkdir.request(folder_name) {
            eprintln!("[note-serve] 请求建夹《{folder_name}》失败（不阻塞本次上传，这次大概率落根目录）: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::BookBrief;
    use notecore::model::{Entry, Status, Style};
    use std::sync::Mutex;

    struct FakeStore(Book);
    impl EntryStore for FakeStore {
        fn list_books(&self) -> Result<Vec<BookBrief>, String> {
            Ok(vec![BookBrief { uuid: self.0.uuid.clone() }])
        }
        fn book(&self, uuid: &str) -> Result<Book, String> {
            if uuid == self.0.uuid { Ok(self.0.clone()) } else { Err("没有这本书".into()) }
        }
    }

    #[derive(Default)]
    struct FakeUploader {
        uploads: Mutex<Vec<(String, String)>>, // (filename, folder)
        ensure_folder_calls: Mutex<Vec<String>>,
        fail_upload: Mutex<bool>,
        fail_claim: Mutex<bool>,
    }
    impl Uploader for FakeUploader {
        fn upload(&self, _bytes: &[u8], filename: &str, folder_name: &str) -> Result<(), String> {
            if *self.fail_upload.lock().unwrap() {
                return Err("模拟上传失败".into());
            }
            self.uploads.lock().unwrap().push((filename.to_string(), folder_name.to_string()));
            Ok(())
        }
        fn claim(&self, visible_name: &str, since_ms: u64) -> Result<String, String> {
            if *self.fail_claim.lock().unwrap() {
                return Err("模拟认领失败".into());
            }
            Ok(format!("claimed-{visible_name}-{since_ms}"))
        }
        fn ensure_folder(&self, folder_name: &str) {
            self.ensure_folder_calls.lock().unwrap().push(folder_name.to_string());
        }
    }

    #[derive(Default)]
    struct FakeTrash(Mutex<Vec<(String, String)>>);
    impl TrashSink for FakeTrash {
        fn add(&self, uuid: &str, name: &str) -> Result<(), String> {
            self.0.lock().unwrap().push((uuid.to_string(), name.to_string()));
            Ok(())
        }
    }

    fn entry(id: &str, chapter: usize, text: &str) -> Entry {
        Entry { id: id.into(), page: "p".into(), page_index: 0, chapter: Some(chapter), chapter_title: String::new(), subhead: None, quote: None, ink: None, drafts: vec![], text: Some(text.into()), style: Style::Body, ask_ai: false, question: None, answer: None, status: Status::Reviewed, destination: Default::default(), created: 0, updated: 0 }
    }

    fn book() -> Book {
        Book {
            uuid: "book1".into(),
            title: "人骨拼图".into(),
            author: String::new(),
            chapters: vec!["第一章".into(), "空章".into()],
            entries: vec![entry("e1", 0, "第一条")],
            page_mtimes: Default::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ctx<'a>(store: &'a FakeStore, uploader: &'a FakeUploader, trash: &'a FakeTrash, state: &'a NotebookState, cfg: &'a NoteConfig, now_ms: u64) -> Ctx<'a> {
        Ctx { store, uploader, trash, state, cfg, now_ms }
    }

    #[test]
    fn first_generation_uploads_and_records_then_second_run_is_unchanged() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let cfg = NoteConfig::default();
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let results = generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 1000), "book1").unwrap();
        assert_eq!(results.len(), 2, "两章");
        match &results[0].outcome {
            ChapterOutcome::Generated { doc_uuid } => assert_eq!(doc_uuid, "claimed-第1章 第一章-1000"),
            other => panic!("应是 Generated: {other:?}"),
        }
        assert_eq!(results[1].outcome, ChapterOutcome::Empty, "空章没条目");
        assert_eq!(uploader.uploads.lock().unwrap().len(), 1);
        assert_eq!(uploader.uploads.lock().unwrap()[0].1, "《人骨拼图》");
        assert_eq!(uploader.ensure_folder_calls.lock().unwrap().as_slice(), ["《人骨拼图》"], "上传前应该先请求确保文件夹存在，空章不应该调（没走到上传那步）");
        assert!(trash.0.lock().unwrap().is_empty(), "第一次生成没有旧版本要清");

        // 再跑一遍、书没变 → 不重传
        let results2 = generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 2000), "book1").unwrap();
        assert_eq!(results2[0].outcome, ChapterOutcome::Unchanged);
        assert_eq!(uploader.uploads.lock().unwrap().len(), 1, "指纹没变不该再传一次");
    }

    #[test]
    fn text_change_regenerates_and_trashes_old_version() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let cfg = NoteConfig::default();
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 1000), "book1").unwrap();

        let mut edited = book();
        edited.entries[0].text = Some("校对过的新文本".into());
        let store2 = FakeStore(edited);
        let results = generate_book(&ctx(&store2, &uploader, &trash, &state, &cfg, 5000), "book1").unwrap();
        match &results[0].outcome {
            ChapterOutcome::Generated { doc_uuid } => assert_eq!(doc_uuid, "claimed-第1章 第一章-5000"),
            other => panic!("文本变了应该重新生成: {other:?}"),
        }
        assert_eq!(uploader.uploads.lock().unwrap().len(), 2);
        let trashed = trash.0.lock().unwrap();
        assert_eq!(trashed.len(), 1, "旧版本应入队一次");
        assert_eq!(trashed[0], ("claimed-第1章 第一章-1000".to_string(), "第1章 第一章".to_string()), "按生成时记录的旧 visibleName 入队，不是重算的新名字");
    }

    #[test]
    fn upload_failure_does_not_touch_state_so_retry_is_clean() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let cfg = NoteConfig::default();
        let uploader = FakeUploader::default();
        *uploader.fail_upload.lock().unwrap() = true;
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let results = generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 1000), "book1").unwrap();
        match &results[0].outcome {
            ChapterOutcome::Failed { error } => assert!(error.contains("模拟上传失败")),
            other => panic!("应失败: {other:?}"),
        }
        assert!(state.get("book1", 0).is_none(), "失败不留状态，下次还会照常重试");

        *uploader.fail_upload.lock().unwrap() = false;
        let results2 = generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 2000), "book1").unwrap();
        assert!(matches!(results2[0].outcome, ChapterOutcome::Generated { .. }), "重试应该正常成功");
    }

    #[test]
    fn chapter_emptied_after_generation_clears_stale_record_not_ghost_exported() {
        // 幽灵已导出回归：一章生成过笔记本，之后这一章所有条目撤销/归档/跳过（project_chapter 返回
        // None），第二次 generate 应该清掉旧记录（doc_uuid/generated_at 不再残留），不然「整理」页会
        // 永久误判这一章"已导出"。
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let cfg = NoteConfig::default();
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        let results = generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 1000), "book1").unwrap();
        assert!(matches!(results[0].outcome, ChapterOutcome::Generated { .. }), "先正常生成一次");
        assert!(state.get("book1", 0).is_some(), "生成后应该留下记录");

        let mut emptied = book();
        emptied.entries[0].status = Status::Archived; // 这一章唯一的条目被"不要了"，project_chapter 应返回 None
        let store2 = FakeStore(emptied);
        let results2 = generate_book(&ctx(&store2, &uploader, &trash, &state, &cfg, 2000), "book1").unwrap();
        assert_eq!(results2[0].outcome, ChapterOutcome::Empty, "章空了应该是 Empty 不是 Unchanged");
        assert!(state.get("book1", 0).is_none(), "旧记录应该被清掉，不然会幽灵已导出");
    }

    #[test]
    fn unknown_book_errors() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let cfg = NoteConfig::default();
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        assert!(generate_book(&ctx(&store, &uploader, &trash, &state, &cfg, 1000), "no-such-book").is_err());
    }
}
