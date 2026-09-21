//! 漫画「页边距」待办：哪些原生书库里的文档，应在用户**首次打开**时由阅读器自己设成 [`bookconv::imgopt::EPUB_COMIC_MARGINS`]。
//!
//! 为什么不直接改文档的 `.content`：外部写文件会被运行中 xochitl 的内存状态盖回去（真机 5 次实验只成功 1 次，白皮书 §20）。
//! 唯一可靠的是走 xochitl 自己的代码路径——注入 DocumentView 的 `shelf/xovi/shelf-comic-margins.qmd` 在书打开时
//! `GET /margins/<uuid>`，命中就调用阅读器的 `EpubProperties.setMargins(m)`（与界面点"页边距"同一条路径），成功后
//! `POST /margins/applied` 销账。**每本书只设一次**：之后用户在界面上自己改回去，我们不再干预。
//!
//! 只登记**用新版管线处理过的纯图漫画**（`Staging::comic_margin_eligible`）：补白比例是按"边距 0"算的，旧管线产物
//! （补白到屏幕比例）在边距 0 下会贴左、右侧空一大块，反而更糟。队列文件 `$XDG_STATE_HOME/shelf/books/comic-margins.json`，
//! 持久化/去重/剔除委托通用的 [`PendingQueue`]（同 `trash.rs`/`mkdir.rs`）。
use crate::pending_queue::PendingQueue;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pending {
    pub uuid: String,
    pub margins: u32,
    pub at: u64,
}

pub struct ComicMargins {
    q: PendingQueue<Pending>,
    lib_dir: PathBuf,
}

impl ComicMargins {
    pub fn new(state_books_dir: &Path, lib_dir: &Path) -> ComicMargins {
        ComicMargins { q: PendingQueue::new(state_books_dir.join("comic-margins.json")), lib_dir: lib_dir.to_path_buf() }
    }

    fn exists(&self, uuid: &str) -> bool {
        self.lib_dir.join(format!("{uuid}.metadata")).is_file()
    }

    /// 登记（同 uuid 去重）。uuid 必须形状对且真在书库里。
    pub fn add(&self, uuid: &str, margins: u32) -> Result<usize, String> {
        if !rmsvc_core::xochitl::is_uuid_shape(uuid) {
            return Err("uuid 形状不对".into());
        }
        if !self.exists(uuid) {
            return Err("书库里没有这份文档".into());
        }
        self.q.add(|p| p.uuid == uuid, || Pending { uuid: uuid.to_string(), margins, at: rmsvc_core::clock::now_secs() })
    }

    /// 这本书该设的边距；没登记（或书已不在）→ None。QML 代理每次开书调一次。
    pub fn get(&self, uuid: &str) -> Option<u32> {
        if !rmsvc_core::xochitl::is_uuid_shape(uuid) {
            return None;
        }
        self.q.list().into_iter().find(|p| p.uuid == uuid).map(|p| p.margins)
    }

    /// 销账（已设好）。返回剩余待办数。
    pub fn applied(&self, uuid: &str) -> Result<usize, String> {
        let (kept, _) = self.q.prune(|p| p.uuid != uuid)?;
        Ok(kept.len())
    }

    /// 启动时清掉书已不在库里的待办。
    pub fn prune_missing(&self) -> usize {
        self.q.prune(|p| self.exists(&p.uuid)).map(|(_, n)| n).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const U1: &str = "11111111-1111-1111-1111-111111111111";
    const U2: &str = "22222222-2222-2222-2222-222222222222";

    fn setup(t: &tempfile::TempDir) -> ComicMargins {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(format!("{U1}.metadata")), "{}").unwrap();
        ComicMargins::new(t.path(), &lib)
    }

    #[test]
    fn add_get_applied_roundtrip_and_dedupes() {
        let t = tempfile::tempdir().unwrap();
        let m = setup(&t);
        assert_eq!(m.get(U1), None);
        m.add(U1, 0).unwrap();
        m.add(U1, 0).unwrap(); // 幂等
        assert_eq!(m.get(U1), Some(0));
        assert_eq!(m.applied(U1), Ok(0));
        assert_eq!(m.get(U1), None, "销账后不再返回：用户自己改回去的边距不被干预");
    }

    #[test]
    fn add_rejects_bad_or_missing_uuid_and_survives_restart() {
        let t = tempfile::tempdir().unwrap();
        let m = setup(&t);
        assert!(m.add("../../etc/passwd", 0).is_err());
        assert!(m.add(U2, 0).is_err(), "书库里没有");
        assert_eq!(m.get("../x"), None);
        m.add(U1, 0).unwrap();
        let again = ComicMargins::new(t.path(), &t.path().join("xochitl"));
        assert_eq!(again.get(U1), Some(0), "落盘，重启不丢");
    }

    #[test]
    fn prune_missing_drops_entries_whose_book_is_gone() {
        let t = tempfile::tempdir().unwrap();
        let m = setup(&t);
        m.add(U1, 0).unwrap();
        std::fs::remove_file(t.path().join("xochitl").join(format!("{U1}.metadata"))).unwrap();
        assert_eq!(m.prune_missing(), 1);
        assert_eq!(m.get(U1), None);
    }
}
