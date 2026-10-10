//! 生成编排（Strategy + 纯函数骨架，trait 全桩、可离线单测；真机网络只在 `XochitlUploader`/`BookServeTrash`/
//! `notesvc::InkClient` 三处生产实现里）：
//! 条目库取书 → `notecore::project` 投影每一章 → 指纹未变就跳过（不重传） → `rmv6::write` 打包 `.rmdoc` →
//! `/upload` 进书本自己已经在的设备文件夹 → 按 `visibleName`+时间窗认领刚生成的设备 uuid → 这一章如果
//! 之前生成过、且这次真的换了新文档，旧版本入 `book-serve` 回收站队列 → 记新记录（`notebooks.rs`）。
//! 一章失败不影响其它章；旧版本入队失败也不算这一章失败（新文档已经生成好了，旧的多留一份不是数据丢失）。
//!
//! **不再新建/确保任何文件夹**（2026-09-09 改）：笔记本直接落进书本自己所在的设备文件夹（读书本文档
//! `.metadata` 的 `parent` 字段，空串＝书库根），查不到就 best-effort 落根——比此前"转发 book-serve
//! 排队、靠真机 qmd 每 8 秒轮询 `Library.createCollection` 兜底建《书名》文件夹"那条 fire-and-forget
//! 链路更直接、也没有重名建夹的风险。笔记本名字直接用章节标题（不再加"第N章"前缀），撞名在同一文件夹
//! 范围内加数字后缀（`unique_document_name`）；**只有首次生成才走去重**，同一章重新生成时沿用当时定
//! 下来的名字（否则旧文档还没来得及入回收站，会把自己也判成"重名"，错误多加一次后缀）。
use crate::ink::EntryStore;
use crate::notebooks::{ChapterRecord, NotebookState};
use crate::rmdoc::{self, Page};
use crate::trash::TrashSink;
use notecore::model::Book;
use notecore::project::{fingerprint_chapter, project_chapter};
use rmsvc_core::xochitl::{ClaimBy, ClaimWait, Folder, UploadBody};
use rmv6::write::{build_page_rm, Paragraph};
use serde::Serialize;

/// 传书 + 认领 + 查文件夹/去重三件事的抽象；生产实现包一层 `rmsvc_core::xochitl::Xochitl`，测试用
/// 内存桩——不真的碰网络。
pub trait Uploader: Send + Sync {
    /// 上传一份 `.rmdoc` 进 `folder`（即 [`Self::folder_of_document`] 给的书本文件夹），再认领设备新分配给它的
    /// uuid（`visibleName == visible_name`、上传前不在书库里的那份）。2026-10-10 起"上传前快照 → 上传 → 认领"整段
    /// 由基座 `Xochitl::upload_and_claim` 一次做完（此前拆成 `existing`/`upload`/`claim` 三个方法，调用方自己拼）；
    /// 生产实现内部会短暂等待设备建好条目，见 `XochitlUploader`。
    fn upload_and_claim(&self, bytes: &[u8], filename: &str, folder: &Folder, visible_name: &str) -> Result<String, String>;
    /// 书本自己在设备上所在的文件夹（根或某个文件夹）。查不到（书不在库里/已删除）→ `None`，
    /// 调用方 best-effort 落根目录，不新建/确保任何文件夹。缺省 `None` 方便不关心这件事的测试桩。
    fn folder_of_document(&self, _book_uuid: &str) -> Option<Folder> {
        None
    }
    /// 在 `folder` 范围内，如果 `base_name` 已被占用就加数字后缀直到不冲突。缺省原样返回，方便
    /// 不关心去重的测试桩。
    fn unique_name(&self, _folder: &Folder, base_name: &str) -> String {
        base_name.to_string()
    }
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
    pub now_ms: u64,
}

/// 生成/校验一章。`book` 由调用方先取好（`generate_book` 一次取书、对每章调用它，避免重复请求 ink-serve）。
pub fn generate_chapter(c: &Ctx, book: &Book, idx: usize) -> ChapterResult {
    let Some(title) = book.chapters.get(idx).cloned() else {
        return ChapterResult { chapter: idx, title: String::new(), outcome: ChapterOutcome::Failed { error: "没有这一章".into() } };
    };
    let Some(paragraphs) = project_chapter(book, idx) else {
        // `project_chapter` 返回 `None` 有两种不同的原因，处理方式不能一样（2026-09-17 真机 bug
        // 修复）：①这一章真的没有活条目了（全撤销/归档/跳过）——历史记录该清，不然 `generated_at`
        // 永远非空，「整理」页的 `everExported` 会永久误判这一章"已导出"（幽灵已导出）；②条目还活着，
        // 只是这次这一章没有条目的去处想要笔记本了（比如唯一一条从 `Notebook` 切到 `Obsidian`）——
        // 设备上的笔记本文档本身没删（下面这条 `clear()` 从来不碰真文档，只清本地记录），这种情况下
        // 清掉记录就是把"曾经真的生成过"这个事实凭空抹掉，「整理」页的笔记本徽章会跟着消失，用户
        // 反馈"切换去处后另一个去处的状态丢了"就是这个根因。`chapter_has_live_entries` 忽略去处、
        // 只看条目死没死，用它区分这两种情况，只在真是①的时候才清。
        if !book.chapter_has_live_entries(idx) {
            if let Err(e) = c.state.clear(&book.uuid, idx) {
                eprintln!("[note-serve] 清空第 {} 章旧生成记录失败，先留着，不阻塞本次结果: {e}", idx + 1);
            }
        }
        return ChapterResult { chapter: idx, title, outcome: ChapterOutcome::Empty };
    };
    let fingerprint = fingerprint_chapter(book, idx).unwrap_or_default();
    let existing = c.state.get(&book.uuid, idx);
    if existing.as_ref().map(|r| r.fingerprint.as_str()) == Some(fingerprint.as_str()) {
        return ChapterResult { chapter: idx, title, outcome: ChapterOutcome::Unchanged };
    }

    let outcome = (|| -> Result<String, String> {
        let folder = c.uploader.folder_of_document(&book.uuid).unwrap_or_default();
        // 首次生成才去重；重新生成沿用当时定下来的名字——这时候旧文档还占着这个名字（要等上传成功
        // 才会入回收站队列），如果重新去重会把自己也判成"重名"，白白多加一次后缀。
        let visible_name = match &existing {
            Some(old) => old.visible_name.clone(),
            None => c.uploader.unique_name(&folder, &title),
        };
        let new_uuid = upload_page(c, &paragraphs, &folder, &visible_name)?;
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

/// 一本书全部章节各生成/校验一遍（只给测试用：网页按章推送，整本生成的端点 2026-10-09 已删）。
#[cfg(test)]
pub fn generate_book(c: &Ctx, book_uuid: &str) -> Result<Vec<ChapterResult>, String> {
    let book = c.store.book(book_uuid)?;
    Ok((0..book.chapters.len()).map(|i| generate_chapter(c, &book, i)).collect())
}

/// 单篇 markdown → 一个新的设备笔记本文档（独立于条目库的生成编排，不经 `notecore::project`）。
/// `title` 只当设备文档名用（撞名走跟章节生成同一套 `unique_name` 后缀规则），不会额外注入成
/// 页面里的标题段落——markdown 正文自己的 `#` 标题（如果有）才决定页面里显示什么。不写
/// `NotebookState`：这是一次性导入，没有"源数据"可供下次比对指纹、也就没有增量重传的概念，
/// 重复导入同一段内容会各自生成一份新文档（靠去重后缀区分）。
/// 返回 `(实际用上的设备文档名, 设备分配的 uuid)`——名字可能因为撞名被 `unique_name` 加了后缀，
/// 调用方（网页）该显示这个真名，不是自己传进来的 `title` 原样，不然用户以为存的是自己起的名字，
/// 实际设备上是带后缀的另一个名字，对不上。
pub fn import_markdown(c: &Ctx, book_uuid: &str, title: &str, markdown: &str) -> Result<(String, String), String> {
    c.store.book(book_uuid)?; // 只为确认这本书存在，错的 uuid 早点报错，比 best-effort 落根更清楚
    let paragraphs = notecore::mdimport::markdown_to_paragraphs(markdown);
    let folder = c.uploader.folder_of_document(book_uuid).unwrap_or_default();
    let visible_name = c.uploader.unique_name(&folder, title);
    let new_uuid = upload_page(c, &paragraphs, &folder, &visible_name)?;
    Ok((visible_name, new_uuid))
}

/// 一页段落 → 单页 `.rmdoc` → 上传进 `folder` → 认领设备新分配的 uuid。章节生成与 markdown 导入共用
/// （此前两处各写一遍逐行相同的"建页/打包/上传/认领"四步）。
fn upload_page(c: &Ctx, paragraphs: &[Paragraph], folder: &Folder, visible_name: &str) -> Result<String, String> {
    let rm = build_page_rm(rmdoc::TEMPLATE, paragraphs)?;
    let doc_uuid = uuid::Uuid::new_v4().to_string();
    let page = Page { uuid: uuid::Uuid::new_v4().to_string(), rm_bytes: rm };
    let bytes = rmdoc::pack(&doc_uuid, visible_name, "", &page, rmdoc::TEMPLATE_AUTHOR, c.now_ms)?;
    c.uploader.upload_and_claim(&bytes, &format!("{doc_uuid}.rmdoc"), folder, visible_name)
}

/// 认领最长等多久——作为一次同步 HTTP 请求内的等待，不宜太长；覆盖"设备处理稍慢"这类常见的秒级延迟就够
/// （与此前"每 1.5 秒查一次、共 4 次"的上限相当），真的卡住等多久都没用。
const CLAIM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// 笔记本 `.rmdoc` 的上传 MIME（`formats::mime_of` 不认 `.rmdoc` 扩展名，会给 octet-stream；保持一直以来真机用的 zip）。
const RMDOC_MIME: &str = "application/zip";

/// 生产实现：包一层 `rmsvc_core::xochitl::Xochitl`。
pub struct XochitlUploader {
    xochitl: rmsvc_core::xochitl::Xochitl,
}

impl XochitlUploader {
    pub fn new(host: &str, library_dir: &std::path::Path, timeout_secs: u64) -> XochitlUploader {
        XochitlUploader { xochitl: rmsvc_core::xochitl::Xochitl::new(host, library_dir, timeout_secs) }
    }
}

impl Uploader for XochitlUploader {
    /// 按 [`Folder`] 投（2026-09-09 的事故：此前把文件夹 uuid 交给按**名字**找文件夹的旧接口，必然落空、落书库根，
    /// 书放在文件夹里时笔记本一直生成到根目录；类型化之后名字传不进来）。认领判据只能是 visibleName：xochitl 会把
    /// `.rmdoc` 解包成页面，书库里没有与上传字节相同的文件可比。
    ///
    /// 等待：2026-09-09 审计发现上传成功那一刻设备已经真实建好文档，认领只是"回查"，两者不是一个事务——认领失败
    /// （常见原因是设备处理还没跟上，不是真的丢了）以前直接让本次生成整体判失败，用户手动点「推送本章」重试时，
    /// `unique_name` 去重会把这次已经建好、只是没认领到的文档当成"重名"，另建一份带后缀的新文档——旧的那份永远
    /// 追踪不到，变孤儿。所以放弃前短暂等一会（基座监听书库目录，文件一落盘就查），绝大多数情况下几秒内就能认领到。
    fn upload_and_claim(&self, bytes: &[u8], filename: &str, folder: &Folder, visible_name: &str) -> Result<String, String> {
        let wait = ClaimWait::new(CLAIM_TIMEOUT);
        Ok(self.xochitl.upload_and_claim(UploadBody::Bytes(bytes), filename, RMDOC_MIME, folder, ClaimBy::VisibleName(visible_name), wait)?.uuid)
    }
    fn folder_of_document(&self, book_uuid: &str) -> Option<Folder> {
        self.xochitl.folder_of_document(book_uuid)
    }
    fn unique_name(&self, folder: &Folder, base_name: &str) -> String {
        self.xochitl.unique_name(folder.as_parent_str(), base_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecore::model::{Destination, Entry, Status, Style};
    use std::sync::Mutex;

    struct FakeStore(Book);
    impl EntryStore for FakeStore {
        fn book(&self, uuid: &str) -> Result<Book, String> {
            if uuid == self.0.uuid { Ok(self.0.clone()) } else { Err("没有这本书".into()) }
        }
    }

    // 不覆盖 `folder_of_document`/`unique_name`——用 trait 缺省（`None`/原样返回），等价于"书查不到就落根、
    // 不去重"，大部分测试不关心这两件事，只有下面 `FolderAwareUploader` 专门测这块。
    // 认领到的 uuid 是 `claimed-<名字>-<第几次上传>`：重新生成必然换一个新 uuid（旧版本才会入回收站）。
    #[derive(Default)]
    struct FakeUploader {
        uploads: Mutex<Vec<(String, String)>>, // (filename, folder 的 parent 写法)
        fail_upload: Mutex<bool>,
        fail_claim: Mutex<bool>,
    }
    impl Uploader for FakeUploader {
        fn upload_and_claim(&self, _bytes: &[u8], filename: &str, folder: &Folder, visible_name: &str) -> Result<String, String> {
            if *self.fail_upload.lock().unwrap() {
                return Err("模拟上传失败".into());
            }
            let mut uploads = self.uploads.lock().unwrap();
            uploads.push((filename.to_string(), folder.as_parent_str().to_string()));
            // 上传已经发生、只是没认出来（真机上设备已经建好文档）：这就是孤儿文档风险的来源。
            if *self.fail_claim.lock().unwrap() {
                return Err("模拟认领失败".into());
            }
            Ok(format!("claimed-{visible_name}-{}", uploads.len()))
        }
    }

    /// 书本所在文件夹（uuid 形状，`Folder` 只收这种）。
    const BOOK_FOLDER: &str = "0a1b2c3d-4e5f-6789-abcd-ef0123456789";

    /// 专测"复用书本文件夹 + 只在首次生成去重"：`folder_of_document` 固定返回一个非根文件夹，`unique_name`
    /// 每次被调用都会改变返回值（模拟"真的查了一次去重"），配合 `unique_name_calls` 计数断言
    /// 重新生成时**不会**再调一次（否则会把自己刚生成的旧文档也当成"重名"）。
    #[derive(Default)]
    struct FolderAwareUploader {
        uploads: Mutex<Vec<(String, String)>>,
        unique_name_calls: Mutex<u32>,
    }
    impl Uploader for FolderAwareUploader {
        fn upload_and_claim(&self, _bytes: &[u8], filename: &str, folder: &Folder, visible_name: &str) -> Result<String, String> {
            let mut uploads = self.uploads.lock().unwrap();
            uploads.push((filename.to_string(), folder.as_parent_str().to_string()));
            Ok(format!("claimed-{visible_name}-{}", uploads.len()))
        }
        fn folder_of_document(&self, _book_uuid: &str) -> Option<Folder> {
            Folder::from_parent_str(BOOK_FOLDER)
        }
        fn unique_name(&self, folder: &Folder, base_name: &str) -> String {
            assert_eq!(folder.as_parent_str(), BOOK_FOLDER, "去重查的是书本所在的文件夹");
            *self.unique_name_calls.lock().unwrap() += 1;
            format!("{base_name} (deduped)")
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
        Entry { id: id.into(), page: "p".into(), page_index: 0, chapter: Some(chapter), chapter_title: String::new(), subhead: None, quote: None, ink: None, drafts: vec![], text: Some(text.into()), style: Style::Body, ask_ai: false, question: None, answer: None, status: Status::Reviewed, destination: Default::default(), source: Default::default(), created: 0, updated: 0 }
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

    fn ctx<'a>(store: &'a FakeStore, uploader: &'a dyn Uploader, trash: &'a FakeTrash, state: &'a NotebookState, now_ms: u64) -> Ctx<'a> {
        Ctx { store, uploader, trash, state, now_ms }
    }

    #[test]
    fn first_generation_uploads_and_records_then_second_run_is_unchanged() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let results = generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();
        assert_eq!(results.len(), 2, "两章");
        match &results[0].outcome {
            ChapterOutcome::Generated { doc_uuid } => assert_eq!(doc_uuid, "claimed-第一章-1", "名字直接用章节标题，不再带「第N章」前缀"),
            other => panic!("应是 Generated: {other:?}"),
        }
        assert_eq!(results[1].outcome, ChapterOutcome::Empty, "空章没条目");
        assert_eq!(uploader.uploads.lock().unwrap().len(), 1);
        assert_eq!(uploader.uploads.lock().unwrap()[0].1, "", "查不到书本父文件夹（FakeUploader 缺省 None），best-effort 落根（根的 parent 写法是空串）");
        assert!(trash.0.lock().unwrap().is_empty(), "第一次生成没有旧版本要清");

        // 再跑一遍、书没变 → 不重传
        let results2 = generate_book(&ctx(&store, &uploader, &trash, &state, 2000), "book1").unwrap();
        assert_eq!(results2[0].outcome, ChapterOutcome::Unchanged);
        assert_eq!(uploader.uploads.lock().unwrap().len(), 1, "指纹没变不该再传一次");
    }

    #[test]
    fn text_change_regenerates_and_trashes_old_version() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();

        let mut edited = book();
        edited.entries[0].text = Some("校对过的新文本".into());
        let store2 = FakeStore(edited);
        let results = generate_book(&ctx(&store2, &uploader, &trash, &state, 5000), "book1").unwrap();
        match &results[0].outcome {
            ChapterOutcome::Generated { doc_uuid } => assert_eq!(doc_uuid, "claimed-第一章-2", "重新生成沿用记录的名字，不是重新算的（这里两次都一样，是因为 FakeUploader 去重恒等）"),
            other => panic!("文本变了应该重新生成: {other:?}"),
        }
        assert_eq!(uploader.uploads.lock().unwrap().len(), 2);
        let trashed = trash.0.lock().unwrap();
        assert_eq!(trashed.len(), 1, "旧版本应入队一次");
        assert_eq!(trashed[0], ("claimed-第一章-1".to_string(), "第一章".to_string()), "按生成时记录的旧 visibleName 入队，不是重算的新名字");
    }

    #[test]
    fn folder_reused_from_book_and_dedup_only_runs_once_not_on_regenerate() {
        // 用 FolderAwareUploader：folder_of_document 固定返回一个非根文件夹（验证真的把书本的文件夹传给了
        // upload），unique_name 每次调用都变一次返回值（模拟"真的查了一次去重"）。核心断言：重新生成
        // 同一章不会再调 unique_name——否则这次调用会把"上一次生成、还没来得及入回收站"的旧文档也判成
        // 重名，多加一次没必要的后缀。
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FolderAwareUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let results = generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();
        assert_eq!(*uploader.unique_name_calls.lock().unwrap(), 1, "第一次生成该查一次去重");
        assert_eq!(uploader.uploads.lock().unwrap()[0].1, BOOK_FOLDER, "传的是书本自己的文件夹，不是新建的《书名》");
        match &results[0].outcome {
            ChapterOutcome::Generated { doc_uuid } => assert_eq!(doc_uuid, "claimed-第一章 (deduped)-1"),
            other => panic!("应是 Generated: {other:?}"),
        }

        let mut edited = book();
        edited.entries[0].text = Some("改过的文本".into());
        let store2 = FakeStore(edited);
        let results2 = generate_book(&ctx(&store2, &uploader, &trash, &state, 5000), "book1").unwrap();
        assert_eq!(*uploader.unique_name_calls.lock().unwrap(), 1, "重新生成不该再查一次去重，得沿用记录的名字");
        match &results2[0].outcome {
            ChapterOutcome::Generated { doc_uuid } => assert_eq!(doc_uuid, "claimed-第一章 (deduped)-2", "沿用第一次去重后定下的名字"),
            other => panic!("应是 Generated: {other:?}"),
        }
    }

    #[test]
    fn upload_failure_does_not_touch_state_so_retry_is_clean() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        *uploader.fail_upload.lock().unwrap() = true;
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let results = generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();
        match &results[0].outcome {
            ChapterOutcome::Failed { error } => assert!(error.contains("模拟上传失败")),
            other => panic!("应失败: {other:?}"),
        }
        assert!(state.get("book1", 0).is_none(), "失败不留状态，下次还会照常重试");

        *uploader.fail_upload.lock().unwrap() = false;
        let results2 = generate_book(&ctx(&store, &uploader, &trash, &state, 2000), "book1").unwrap();
        assert!(matches!(results2[0].outcome, ChapterOutcome::Generated { .. }), "重试应该正常成功");
    }

    #[test]
    fn claim_failure_uploads_a_document_that_becomes_untracked_orphan_this_is_the_known_risk() {
        // `fail_claim` 字段一直存在但从没被真正设过 true 写测试（2026-09-09 审计发现的空白）——
        // 这条测试补上，同时如实记录：`upload()` 已经成功执行过（真机上设备已经真实建好文档），
        // 只是 `claim()` 没认领到，本次生成整体判失败、`NotebookState` 不落记录。生产实现
        // （`XochitlUploader::upload_and_claim`）针对这个已知风险加了短暂等待来降低实际发生概率，
        // 但 `FakeUploader`（测试桩）不模拟等待，这里直接测最坏情况：认领从头到尾都失败。
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        *uploader.fail_claim.lock().unwrap() = true;
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let results = generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();
        match &results[0].outcome {
            ChapterOutcome::Failed { error } => assert!(error.contains("模拟认领失败")),
            other => panic!("应失败: {other:?}"),
        }
        assert_eq!(uploader.uploads.lock().unwrap().len(), 1, "upload() 已经真实执行过一次——这就是孤儿文档风险的来源，不是没发生任何事");
        assert!(state.get("book1", 0).is_none(), "认领失败不留状态，下次会被当成'首次生成'重新走一遍去重");
    }

    #[test]
    fn chapter_emptied_after_generation_clears_stale_record_not_ghost_exported() {
        // 幽灵已导出回归：一章生成过笔记本，之后这一章所有条目撤销/归档/跳过（project_chapter 返回
        // None），第二次 generate 应该清掉旧记录（doc_uuid/generated_at 不再残留），不然「整理」页会
        // 永久误判这一章"已导出"。
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        let results = generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();
        assert!(matches!(results[0].outcome, ChapterOutcome::Generated { .. }), "先正常生成一次");
        assert!(state.get("book1", 0).is_some(), "生成后应该留下记录");

        let mut emptied = book();
        emptied.entries[0].status = Status::Archived; // 这一章唯一的条目被"不要了"，project_chapter 应返回 None
        let store2 = FakeStore(emptied);
        let results2 = generate_book(&ctx(&store2, &uploader, &trash, &state, 2000), "book1").unwrap();
        assert_eq!(results2[0].outcome, ChapterOutcome::Empty, "章空了应该是 Empty 不是 Unchanged");
        assert!(state.get("book1", 0).is_none(), "旧记录应该被清掉，不然会幽灵已导出");
    }

    /// 2026-09-17 真机 bug：跟上面那条"幽灵已导出"回归长得像，但根因不一样——条目没有被撤销/归档，
    /// 只是把这一章唯一条目的去处从 `Notebook`/`Both` 切成纯 `Obsidian`（不再要笔记本）。这种情况下
    /// `project_chapter` 同样返回 `None`（没有条目要笔记本了），但条目本身还活着，设备上的笔记本
    /// 文档也没有被删——历史记录不该被清掉，不然「整理」页的笔记本徽章会凭空消失，用户真机反馈
    /// "推送至原生、再推送至Obsidian，刚才的推送至原生状态就丢了"就是这个根因。
    #[test]
    fn switching_destination_away_from_notebook_keeps_the_record_not_ghost_exported() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        let results = generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "book1").unwrap();
        assert!(matches!(results[0].outcome, ChapterOutcome::Generated { .. }), "先正常生成一次");
        assert!(state.get("book1", 0).is_some(), "生成后应该留下记录");

        let mut switched = book();
        switched.entries[0].destination = Destination::Obsidian; // 条目还活着，只是不再要笔记本了
        let store2 = FakeStore(switched);
        let results2 = generate_book(&ctx(&store2, &uploader, &trash, &state, 2000), "book1").unwrap();
        assert_eq!(results2[0].outcome, ChapterOutcome::Empty, "这次没有条目要笔记本，仍然是 Empty");
        assert!(state.get("book1", 0).is_some(), "但条目没死，历史记录不该被清掉——设备上的文档还在");
    }

    #[test]
    fn unknown_book_errors() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        assert!(generate_book(&ctx(&store, &uploader, &trash, &state, 1000), "no-such-book").is_err());
    }

    #[test]
    fn import_markdown_uploads_to_book_folder_with_deduped_title_and_no_state_written() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FolderAwareUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());

        let (visible_name, uuid) = import_markdown(&ctx(&store, &uploader, &trash, &state, 1000), "book1", "读书笔记", "# 标题\n\n- 要点").unwrap();
        assert_eq!(visible_name, "读书笔记 (deduped)", "标题走跟章节生成同一套去重规则，返回的该是加过后缀的真名");
        assert_eq!(uuid, "claimed-读书笔记 (deduped)-1");
        assert_eq!(uploader.uploads.lock().unwrap()[0].1, BOOK_FOLDER, "落进书本自己的文件夹");
        assert_eq!(*uploader.unique_name_calls.lock().unwrap(), 1);

        // 独立于条目库的一次性导入，不写 NotebookState——不像章节生成那样有指纹可比对。
        assert!(state.get("book1", 0).is_none());
    }

    #[test]
    fn import_markdown_unknown_book_errors_before_touching_uploader() {
        let t = tempfile::tempdir().unwrap();
        let state = NotebookState::new(t.path().to_path_buf());
        let uploader = FakeUploader::default();
        let trash = FakeTrash::default();
        let store = FakeStore(book());
        assert!(import_markdown(&ctx(&store, &uploader, &trash, &state, 1000), "no-such-book", "标题", "正文").is_err());
        assert!(uploader.uploads.lock().unwrap().is_empty(), "书都没找到，不该碰网络");
    }

    // `XochitlUploader` 的生产路径：本地起一个假 xochitl（只认 `GET /documents/...` 设文件夹与 `POST /upload`），
    // 收到上传后按需在临时书库里写 `.metadata` 模拟设备建条目——上传目标文件夹与认领一起测，不碰真设备。

    /// 收 2 个请求（设文件夹 + 上传），返回 `(方法, 路径, 体里有没有 RMDOC 标记)`；收到上传后调 `on_upload`。
    fn fake_xochitl(on_upload: impl FnOnce() + Send + 'static) -> (String, std::thread::JoinHandle<Vec<(String, String, bool)>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let host = format!("127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
        let h = std::thread::spawn(move || {
            let mut seen = vec![];
            let mut on_upload = Some(on_upload);
            for mut req in server.incoming_requests().take(2) {
                let mut body = Vec::new();
                let _ = std::io::Read::read_to_end(req.as_reader(), &mut body);
                seen.push((req.method().to_string(), req.url().to_string(), body.windows(5).any(|w| w == b"RMDOC")));
                let _ = req.respond(tiny_http::Response::from_string("{}"));
                if seen.last().is_some_and(|s| s.0 == "POST") {
                    if let Some(f) = on_upload.take() {
                        f();
                    }
                }
            }
            seen
        });
        (host, h)
    }

    fn write_meta(lib: &std::path::Path, uuid: &str, name: &str, parent: &str) {
        let created = rmsvc_core::clock::now_ms();
        std::fs::write(lib.join(format!("{uuid}.metadata")), format!(r#"{{"type":"DocumentType","visibleName":"{name}","parent":"{parent}","createdTime":"{created}"}}"#)).unwrap();
    }

    /// 回归：按文件夹 **uuid** 设当前文件夹（`GET /documents/<uuid>`）再 `/upload`。此前把 uuid 当文件夹名字去书库里
    /// 找，必然落空、`GET /documents/`（根）——书在文件夹里时笔记本一直生成到根目录。设备稍慢（上传后 300ms 才建好
    /// 条目）时等到它出现再认领，而不是第一次没找到就放弃。
    #[test]
    fn xochitl_uploader_targets_book_folder_and_claims_late_document() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().to_path_buf();
        let lib2 = lib.clone();
        let (host, h) = fake_xochitl(move || {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                write_meta(&lib2, "late", "迟到的文档", BOOK_FOLDER);
            });
        });
        let up = XochitlUploader::new(&host, &lib, 5);
        let folder = Folder::from_parent_str(BOOK_FOLDER).unwrap();
        assert_eq!(up.upload_and_claim(b"RMDOC-bytes", "d1.rmdoc", &folder, "迟到的文档").unwrap(), "late");
        let seen = h.join().unwrap();
        assert_eq!((seen[0].0.as_str(), seen[0].1.as_str()), ("GET", format!("/documents/{BOOK_FOLDER}").as_str()), "按 uuid 设当前文件夹，不落根");
        assert_eq!((seen[1].0.as_str(), seen[1].1.as_str(), seen[1].2), ("POST", "/upload", true));
    }

    #[test]
    fn xochitl_uploader_gives_up_after_wait_with_a_clear_error() {
        let t = tempfile::tempdir().unwrap();
        let (host, h) = fake_xochitl(|| {});
        let up = XochitlUploader::new(&host, t.path(), 5);
        let err = up.upload_and_claim(b"RMDOC", "d.rmdoc", &Folder::Root, "从来没出现过的文档").unwrap_err();
        assert!(err.contains("已上传") && err.contains("秒内没在书库里认出"), "错误信息该说明已上传、已经等过，不是第一次没找到就报的: {err}");
        assert_eq!(h.join().unwrap()[0].1, "/documents/", "根就设根");
    }

    /// 上传前就已经在书库里的同名文档（时间窗内）不算这次生成的：认领要等到真正的新文档出现。
    #[test]
    fn xochitl_uploader_skips_documents_that_existed_before_upload() {
        let t = tempfile::tempdir().unwrap();
        let lib = t.path().to_path_buf();
        write_meta(&lib, "old", "同名", "");
        let lib2 = lib.clone();
        let (host, h) = fake_xochitl(move || {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                write_meta(&lib2, "new", "同名", "");
            });
        });
        let up = XochitlUploader::new(&host, &lib, 5);
        assert_eq!(up.upload_and_claim(b"RMDOC", "d.rmdoc", &Folder::Root, "同名").unwrap(), "new", "上传前就在的同名文档不该被认领");
        h.join().unwrap();
    }

    #[test]
    fn xochitl_uploader_folder_of_document_and_unique_name_use_the_book_folder() {
        let t = tempfile::tempdir().unwrap();
        write_meta(t.path(), "book", "人骨拼图", BOOK_FOLDER);
        write_meta(t.path(), "n1", "第一章", BOOK_FOLDER);
        write_meta(t.path(), "n2", "第一章", "");
        let up = XochitlUploader::new("127.0.0.1:9", t.path(), 5);
        let folder = up.folder_of_document("book").unwrap();
        assert_eq!(folder.as_parent_str(), BOOK_FOLDER);
        assert_eq!(up.unique_name(&folder, "第一章"), "第一章 2", "只在书本文件夹范围内去重");
        assert_eq!(up.unique_name(&Folder::Root, "第二章"), "第二章");
        assert_eq!(up.folder_of_document("no-such"), None, "查不到 → None，调用方落根");
    }
}
