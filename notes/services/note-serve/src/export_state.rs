//! 记"每本书每章上次导出 md 时的内容指纹"：一书一文件 `$XDG_STATE_HOME/notes/exports/<uuid>.json`。
//! 跟 `notebooks.rs` 的 `NotebookState` 同一个结构、同一份理由（不是条目库，丢了最坏后果只是"重新判
//! 一次要不要重写文件"）——三期落盘导出一直是"每次全量重写"，没有跳过逻辑；整理区第三轮反馈（用户
//! 追问"生成完成后是不是应该移出列表"）要求"整理"页能分辨"这一章已经跟当前内容同步了"，落设备笔记本
//! 那边本来就有这份记账（`ChapterRecord.fingerprint`），导出这边一直没有——现在补上，两条投影路径
//! 用同一套"指纹没变就跳过+记账"纪律。
use serde::{Deserialize, Serialize};
use shelf_core::fs::write_atomic;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportRecord {
    pub fingerprint: String,
    pub exported_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct BookRecord {
    chapters: BTreeMap<String, ExportRecord>,
}

pub struct ExportState {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl ExportState {
    pub fn new(dir: PathBuf) -> ExportState {
        ExportState { dir, lock: Mutex::new(()) }
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }
    fn path(&self, book_uuid: &str) -> PathBuf {
        self.dir.join(format!("{book_uuid}.json"))
    }
    fn load(&self, book_uuid: &str) -> BookRecord {
        std::fs::read(self.path(book_uuid)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn get(&self, book_uuid: &str, chapter_idx: usize) -> Option<ExportRecord> {
        self.load(book_uuid).chapters.get(&chapter_idx.to_string()).cloned()
    }

    pub fn set(&self, book_uuid: &str, chapter_idx: usize, rec: ExportRecord) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut b = self.load(book_uuid);
        b.chapters.insert(chapter_idx.to_string(), rec);
        write_atomic(&self.path(book_uuid), &serde_json::to_vec_pretty(&b).map_err(|e| e.to_string())?).map_err(|e| format!("写导出状态失败: {e}"))
    }

    /// 清掉某章的记录——章内容变成"没有可导出的条目"（`fingerprint_chapter` 回 `None`）时用，
    /// 不然旧记录会一直显示"已同步"，跟当前"这章根本没导出文件"的事实对不上。
    pub fn clear(&self, book_uuid: &str, chapter_idx: usize) -> Result<(), String> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut b = self.load(book_uuid);
        if b.chapters.remove(&chapter_idx.to_string()).is_some() {
            write_atomic(&self.path(book_uuid), &serde_json::to_vec_pretty(&b).map_err(|e| e.to_string())?).map_err(|e| format!("写导出状态失败: {e}"))?;
        }
        Ok(())
    }

    pub fn list(&self, book_uuid: &str) -> BTreeMap<usize, ExportRecord> {
        self.load(book_uuid).chapters.into_iter().filter_map(|(k, v)| k.parse().ok().map(|i| (i, v))).collect()
    }
    #[cfg(test)]
    fn dir(&self) -> &std::path::Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_clear_roundtrip_persists_across_instances() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("exports");
        let st = ExportState::new(dir.clone());
        st.ensure().unwrap();
        assert!(st.get("b1", 0).is_none());
        let rec = ExportRecord { fingerprint: "fp1".into(), exported_at: 100 };
        st.set("b1", 0, rec.clone()).unwrap();
        assert_eq!(st.get("b1", 0), Some(rec.clone()));
        assert!(st.get("b1", 1).is_none(), "另一章互不影响");

        let st2 = ExportState::new(dir);
        assert_eq!(st2.get("b1", 0), Some(rec), "重启后（新实例）读到同一份");
        assert_eq!(st2.dir().join("b1.json").exists(), true);

        let rec2 = ExportRecord { fingerprint: "fp2".into(), exported_at: 200 };
        st2.set("b1", 0, rec2.clone()).unwrap();
        assert_eq!(st2.get("b1", 0), Some(rec2), "覆盖旧记录");

        st2.set("b1", 1, ExportRecord { fingerprint: "fp3".into(), exported_at: 300 }).unwrap();
        assert_eq!(st2.list("b1").len(), 2);

        st2.clear("b1", 0).unwrap();
        assert!(st2.get("b1", 0).is_none(), "清掉之后就是没有过记录");
        assert_eq!(st2.list("b1").len(), 1, "另一章不受影响");
        st2.clear("b1", 0).unwrap(); // 再清一次（本来就没有）不报错
    }
}
