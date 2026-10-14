//! 「今日回顾」笔记本（2026-10-14）：把到期的已校对条目（ink-serve `GET /review/due`，最多 [`LIMIT`] 条）排成一页设备笔记本，
//! 放在书库根下，名字固定「今日回顾」。每次生成都换一份新文档、旧的那份交给 book-serve 回收站队列（与章节笔记本同一条软删路）。
//! 回顾结果只在网页上记（设备笔记本是只读投影，在设备上改了不会回传）。
//!
//! 什么时候生成：网页「笔记 → 回顾」点「生成今日回顾笔记本」；或者打开了「自动生成」后，收到 ink-serve 的 `entries` 事件
//! （合上一本书、摄取完成时会发）且离上次生成已过 [`AUTO_GAP`]。不另起定时器，不额外唤醒设备。
//! 状态文件 `$XDG_STATE_HOME/notes/review.json`：`{auto, generatedAt(毫秒), docUuid, visibleName, count}`。
use crate::publish::{upload_page, Ctx};
use notecore::api::DueList;
use rmsvc_core::xochitl::Folder;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 一页放几条。
pub const LIMIT: usize = 5;
/// 笔记本名字。
pub const NAME: &str = "今日回顾";
/// 自动生成的最小间隔：离上次生成不到这么久就不再生成（"一天一次"，不按日历日算——设备上的进程拿不到本地时区）。
pub const AUTO_GAP_MS: u64 = 20 * 3600 * 1000;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewRecord {
    pub auto: bool,
    pub generated_at: u64,
    pub doc_uuid: String,
    pub visible_name: String,
    pub count: usize,
}

pub struct ReviewState {
    file: PathBuf,
}

impl ReviewState {
    pub fn new(file: PathBuf) -> ReviewState {
        ReviewState { file }
    }
    pub fn get(&self) -> ReviewRecord {
        rmsvc_core::config::load_or_default(&self.file)
    }
    pub fn set(&self, r: &ReviewRecord) -> Result<(), String> {
        rmsvc_core::config::save(&self.file, r, None)
    }
    /// 自动生成到点了没有：开着自动、且离上次生成已过 [`AUTO_GAP_MS`]。
    pub fn auto_due(&self, now_ms: u64) -> bool {
        let r = self.get();
        r.auto && now_ms.saturating_sub(r.generated_at) >= AUTO_GAP_MS
    }
}

/// 生成结果：`count` 为 0 表示没有到期的条目，什么都没传（旧笔记本也不动）。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Generated {
    pub count: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_uuid: Option<String>,
}

/// 按 `due` 生成一份「今日回顾」：上传新的 → 旧的（若有且不同）交回收站 → 记状态。
pub fn generate(c: &Ctx, st: &ReviewState, due: &DueList) -> Result<Generated, String> {
    if due.items.is_empty() {
        return Ok(Generated { count: 0, total: due.total, doc_uuid: None });
    }
    let mut rec = st.get();
    let folder = Folder::Root;
    // 第一次才去重（书库根下恰好已有用户自己的「今日回顾」）；之后沿用定下的名字——旧的那份还占着这个名字。
    let visible_name = if rec.visible_name.is_empty() { c.uploader.unique_name(&folder, NAME) } else { rec.visible_name.clone() };
    let paragraphs = notecore::project::project_review(&due.items);
    let new_uuid = upload_page(c, &paragraphs, &folder, &visible_name)?;
    if !rec.doc_uuid.is_empty() && rec.doc_uuid != new_uuid {
        if let Err(e) = c.trash.add(&rec.doc_uuid, &rec.visible_name) {
            eprintln!("[note-serve] 旧的「{}」（{}）入回收站队列失败，先留着: {e}", rec.visible_name, rec.doc_uuid);
        }
    }
    rec.generated_at = c.now_ms;
    rec.doc_uuid = new_uuid.clone();
    rec.visible_name = visible_name;
    rec.count = due.items.len();
    st.set(&rec)?;
    Ok(Generated { count: due.items.len(), total: due.total, doc_uuid: Some(new_uuid) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebooks::NotebookState;
    use crate::publish::Uploader;
    use crate::trash::TrashSink;
    use notecore::api::DueItem;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Up(Mutex<Vec<(String, String)>>);
    impl Uploader for Up {
        fn upload_and_claim(&self, _b: &[u8], _f: &str, folder: &Folder, name: &str) -> Result<String, String> {
            let mut v = self.0.lock().unwrap();
            v.push((folder.as_parent_str().to_string(), name.to_string()));
            Ok(format!("doc-{}", v.len()))
        }
        fn unique_name(&self, _f: &Folder, base: &str) -> String {
            format!("{base} 2")
        }
    }
    #[derive(Default)]
    struct Tr(Mutex<Vec<(String, String)>>);
    impl TrashSink for Tr {
        fn add(&self, u: &str, n: &str) -> Result<(), String> {
            self.0.lock().unwrap().push((u.into(), n.into()));
            Ok(())
        }
    }

    fn due(n: usize) -> DueList {
        let e: notecore::model::Entry = serde_json::from_value(serde_json::json!({"id": "e", "page": "p", "page_index": 0, "created": 0, "updated": 0, "status": "reviewed", "text": "定稿"})).unwrap();
        DueList { items: (0..n).map(|_| DueItem { book: "b".into(), book_title: "书".into(), entry: e.clone() }).collect(), total: n + 3 }
    }

    #[test]
    fn generates_at_root_replaces_old_and_skips_when_nothing_due() {
        let t = tempfile::tempdir().unwrap();
        let st = ReviewState::new(t.path().join("review.json"));
        let (up, tr, nb) = (Up::default(), Tr::default(), NotebookState::new(t.path().join("nb")));
        let c = Ctx { uploader: &up, trash: &tr, state: &nb, now_ms: 1000 };
        assert_eq!(generate(&c, &st, &due(0)).unwrap(), Generated { count: 0, total: 3, doc_uuid: None });
        assert!(up.0.lock().unwrap().is_empty(), "没到期的不上传");
        let g = generate(&c, &st, &due(2)).unwrap();
        assert_eq!((g.count, g.total, g.doc_uuid.as_deref()), (2, 5, Some("doc-1")));
        assert_eq!(up.0.lock().unwrap()[0], (String::new(), "今日回顾 2".to_string()), "书库根；第一次去重");
        assert!(tr.0.lock().unwrap().is_empty());
        let c2 = Ctx { now_ms: 2000, ..c };
        generate(&c2, &st, &due(1)).unwrap();
        assert_eq!(up.0.lock().unwrap()[1].1, "今日回顾 2", "之后沿用名字，不再去重");
        assert_eq!(*tr.0.lock().unwrap(), [("doc-1".to_string(), "今日回顾 2".to_string())], "旧的交回收站");
        let r = st.get();
        assert_eq!((r.generated_at, r.doc_uuid.as_str(), r.count), (2000, "doc-2", 1));
    }

    #[test]
    fn auto_due_needs_switch_and_gap() {
        let t = tempfile::tempdir().unwrap();
        let st = ReviewState::new(t.path().join("review.json"));
        assert!(!st.auto_due(AUTO_GAP_MS * 2), "缺省不自动");
        st.set(&ReviewRecord { auto: true, generated_at: 1000, ..Default::default() }).unwrap();
        assert!(!st.auto_due(1000 + AUTO_GAP_MS - 1));
        assert!(st.auto_due(1000 + AUTO_GAP_MS));
    }
}
