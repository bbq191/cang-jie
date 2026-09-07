//! 原生书库「建文件夹」队列：外部进程不能直接建文件夹（跟不能直改 `.metadata` 一样，唯一合法路是
//! xochitl 自己的代码路 `Library.createCollection(parentId, name)`）——由注入 MainView 的
//! `shelf/xovi/shelf-mkdir-agent.qmd` 定时拉 `GET /mkdir/pending` 执行。本模块只管队列：`add()` 入队去重，
//! `pending()` 顺手把已经真实存在的文件夹名剔除（QML 端无需 ack，跟 `trash.rs` 的剔除方式对称）。
//! 队列文件 `$XDG_STATE_HOME/shelf/books/mkdir-pending.json`。
//! 首个用途：`note-serve` 生成《书名》一章一本时，目标文件夹不存在就调 `POST /mkdir/add`（2026-09-07）。
use serde::{Deserialize, Serialize};
use shelf_core::fs::write_atomic;
use shelf_core::xochitl::find_folder_by_name;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pending {
    pub name: String,
    pub at: u64,
}

pub struct MkdirQueue {
    file: PathBuf,
    lib_dir: PathBuf,
    lock: Mutex<()>,
}

impl MkdirQueue {
    pub fn new(state_books_dir: &Path, lib_dir: &Path) -> MkdirQueue {
        MkdirQueue { file: state_books_dir.join("mkdir-pending.json"), lib_dir: lib_dir.to_path_buf(), lock: Mutex::new(()) }
    }

    fn load(&self) -> Vec<Pending> {
        std::fs::read(&self.file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save(&self, items: &[Pending]) -> Result<(), String> {
        if let Some(p) = self.file.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        write_atomic(&self.file, &serde_json::to_vec(items).map_err(|e| e.to_string())?).map_err(|e| format!("写建夹队列失败: {e}"))
    }

    /// 入队一个文件夹名；已经真实存在或已在队列里都不重复加。名字不能为空/带路径分隔符（防误传路径）。
    pub fn add(&self, name: &str) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("文件夹名不能为空".into());
        }
        if name.contains('/') || name.contains('\\') {
            return Err("文件夹名不能带路径分隔符".into());
        }
        if find_folder_by_name(&self.lib_dir, name).is_some() {
            return Ok(0); // 已经存在，不用建
        }
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut items = self.load();
        if !items.iter().any(|p| p.name == name) {
            items.push(Pending { name: name.to_string(), at: shelf_core::clock::now_secs() });
            self.save(&items)?;
        }
        Ok(items.len())
    }

    /// 待办文件夹名（QML 代理拉取）：顺手清掉已经真实建出来的（QML 端无需 ack）。
    /// 返回 (待办文件夹名列表, 本次清掉几条)。
    pub fn pending(&self) -> Result<(Vec<String>, usize), String> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let items = self.load();
        let keep: Vec<Pending> = items.iter().filter(|p| find_folder_by_name(&self.lib_dir, &p.name).is_none()).cloned().collect();
        let pruned = items.len() - keep.len();
        if pruned > 0 {
            self.save(&keep)?;
        }
        Ok((keep.iter().map(|p| p.name.clone()).collect(), pruned))
    }

    pub fn list(&self) -> Vec<Pending> {
        self.load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(t: &tempfile::TempDir) -> PathBuf {
        let d = t.path().join("xochitl");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn add_rejects_bad_names_and_existing_folders_then_pending_prunes_created() {
        let t = tempfile::tempdir().unwrap();
        let lib_dir = lib(&t);
        let q = MkdirQueue::new(&t.path().join("state"), &lib_dir);

        assert!(q.add("").unwrap_err().contains("不能为空"));
        assert!(q.add("a/b").unwrap_err().contains("路径分隔符"));

        assert_eq!(q.add("《人骨拼圖》").unwrap(), 1);
        assert_eq!(q.add("《人骨拼圖》").unwrap(), 1, "重复入队不翻倍");
        assert_eq!(q.add(" 《人骨拼圖》 ").unwrap(), 1, "trim 后同名，不重复入队"); // add() trims name first

        assert_eq!(q.add("《消失的爱人》").unwrap(), 2);

        let (names, pruned) = q.pending().unwrap();
        assert_eq!((names.len(), pruned), (2, 0));

        // QML 代理把《人骨拼圖》真的建出来了
        std::fs::write(lib_dir.join("f1.metadata"), r#"{"type":"CollectionType","visibleName":"《人骨拼圖》","parent":""}"#).unwrap();
        let (names, pruned) = q.pending().unwrap();
        assert_eq!((names, pruned), (vec!["《消失的爱人》".to_string()], 1));
        assert_eq!(q.list().len(), 1);

        // 已存在的文件夹再入队直接判"不用建"，不落队列
        assert_eq!(q.add("《人骨拼圖》").unwrap(), 0);
        assert_eq!(q.list().len(), 1, "没有新增");
    }
}
