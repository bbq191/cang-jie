//! 记"每本书每章上次生成了哪份设备文档"：一书一文件 `$XDG_STATE_HOME/notes/notebooks/<uuid>.json`。
//! 这是 note-serve 自己的簿记，不是条目库（条目库的唯一写者仍是 ink-serve）——丢了这份文件最坏后果只是
//! "重新判一次要不要重生成"，不丢数据（重生成一份新文档、旧文档如果还在库里最多多滞留一份，人工能看见）。
use serde::{Deserialize, Serialize};
use shelf_core::fs::write_atomic;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChapterRecord {
    pub doc_uuid: String,
    /// 生成时写进 `.metadata` 的 visibleName——旧版本要排回收站时，`book-serve` 按这个名字核对 uuid。
    pub visible_name: String,
    pub fingerprint: String,
    pub generated_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct BookRecord {
    chapters: BTreeMap<String, ChapterRecord>,
}

pub struct NotebookState {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl NotebookState {
    pub fn new(dir: PathBuf) -> NotebookState {
        NotebookState { dir, lock: Mutex::new(()) }
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    fn path(&self, book_uuid: &str) -> PathBuf {
        self.dir.join(format!("{book_uuid}.json"))
    }
    fn load(&self, book_uuid: &str) -> BookRecord {
        std::fs::read(self.path(book_uuid)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn get(&self, book_uuid: &str, chapter_idx: usize) -> Option<ChapterRecord> {
        self.load(book_uuid).chapters.get(&chapter_idx.to_string()).cloned()
    }

    pub fn set(&self, book_uuid: &str, chapter_idx: usize, rec: ChapterRecord) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut b = self.load(book_uuid);
        b.chapters.insert(chapter_idx.to_string(), rec);
        write_atomic(&self.path(book_uuid), &serde_json::to_vec_pretty(&b).map_err(|e| e.to_string())?).map_err(|e| format!("写笔记本状态失败: {e}"))
    }

    /// 这本书目前记着的全部章节记录（章序号 → 记录），网页状态展示用。
    pub fn list(&self, book_uuid: &str) -> BTreeMap<usize, ChapterRecord> {
        self.load(book_uuid).chapters.into_iter().filter_map(|(k, v)| k.parse().ok().map(|i| (i, v))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_get_roundtrips_persists_across_instances() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("notebooks");
        let st = NotebookState::new(dir.clone());
        st.ensure().unwrap();
        assert!(st.get("b1", 0).is_none());
        let rec = ChapterRecord { doc_uuid: "d1".into(), visible_name: "第1章 起点".into(), fingerprint: "fp1".into(), generated_at: 100 };
        st.set("b1", 0, rec.clone()).unwrap();
        assert_eq!(st.get("b1", 0), Some(rec.clone()));
        assert!(st.get("b1", 1).is_none(), "另一章互不影响");

        // 换一个新实例（模拟重启）应该读到同一份
        let st2 = NotebookState::new(dir);
        assert_eq!(st2.get("b1", 0), Some(rec));

        let rec2 = ChapterRecord { doc_uuid: "d2".into(), visible_name: "第1章 起点".into(), fingerprint: "fp2".into(), generated_at: 200 };
        st2.set("b1", 0, rec2.clone()).unwrap();
        assert_eq!(st2.get("b1", 0), Some(rec2.clone()), "覆盖旧记录");

        let rec3 = ChapterRecord { doc_uuid: "d3".into(), visible_name: "第2章".into(), fingerprint: "fp3".into(), generated_at: 300 };
        st2.set("b1", 1, rec3.clone()).unwrap();
        let all = st2.list("b1");
        assert_eq!(all.len(), 2);
        assert_eq!(all[&0], rec2);
        assert_eq!(all[&1], rec3);
        assert!(st2.list("no-such-book").is_empty());
    }
}
