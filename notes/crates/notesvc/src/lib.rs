//! 笔记线服务共用件（2026-10-10 收拢，NT-5/NT-6）：
//! - [`InkClient`]：访问 ink-serve 条目库的唯一 HTTP 客户端。此前 transcribe/mind/note 三个服务各有一份 `InkHttp`，
//!   路径拼法、请求体都各写一遍；请求/应答形状现在用 `notecore::api` 的契约类型，收发两端同一份定义。
//!   各服务**仍保留自己的窄 `EntryStore` trait**（只声明自己用到的动作），在服务里给 `InkClient` 实现它——那是离线
//!   单测换内存桩的接缝，不合并。
//! - [`load_or_seed_logged`]：服务配置的启动读取，损坏时打日志 + 另存 `.corrupt` 副本（对齐 `vendorcfg::ConfigCell`）。
use notecore::api::{AnswerPost, BookBrief, BookList, DraftPost};
use notecore::model::Book;
use rmsvc_core::paths::Paths;
use rmsvc_core::registry::{enc, SvcClient};
use std::path::Path;

/// 单张裁图的读取上限：裁图是一条勾画的局部渲染（ink-serve 限 400 万像素，PNG 正常几十到几百 KB）；
/// 给到 RGBA 不压缩的体积还有余量，只防异常应答把内存吃光。
const CROP_MAX_BYTES: u64 = 24 * 1024 * 1024;

pub struct InkClient(SvcClient);

impl InkClient {
    pub fn new(paths: Paths) -> InkClient {
        InkClient(SvcClient::new(paths, "ink-serve", 30))
    }
    fn entry_path(uuid: &str, id: &str, action: &str) -> String {
        format!("/books/{}/entries/{}/{action}", enc(uuid), enc(id))
    }
    pub fn list_books(&self) -> Result<Vec<BookBrief>, String> {
        Ok(self.0.get_typed::<BookList>("/books")?.items)
    }
    /// 整本书（应答里每条条目多一个 `live`，`Entry` 反序列化时忽略）。
    pub fn book(&self, uuid: &str) -> Result<Book, String> {
        self.0.get_typed(&format!("/books/{}", enc(uuid)))
    }
    pub fn crop(&self, uuid: &str, file: &str) -> Result<Vec<u8>, String> {
        self.0.get_bytes(&format!("/books/{}/crops/{}", enc(uuid), enc(file)), CROP_MAX_BYTES).map_err(|e| format!("取裁图 {file}: {e}"))
    }
    pub fn post_draft(&self, uuid: &str, id: &str, d: &DraftPost) -> Result<(), String> {
        self.0.post_json(&Self::entry_path(uuid, id, "draft"), &serde_json::to_value(d).map_err(|e| e.to_string())?)
    }
    pub fn post_answer(&self, uuid: &str, id: &str, a: &AnswerPost) -> Result<(), String> {
        self.0.post_json(&Self::entry_path(uuid, id, "answer"), &serde_json::to_value(a).map_err(|e| e.to_string())?)
    }
}

/// 读服务配置：文件不存在 → 写出缺省（供用户改）；**损坏**（存在但解析不成）→ 打日志、另存一份 `.corrupt` 副本
/// （只留第一份），原文件不动，按缺省运行。此前直接用 `rmsvc_core::config::load_or_seed`，损坏时静默退回缺省，
/// 用户改坏了配置看不出任何迹象（NT-5）。`who` 是日志前缀里的服务名。
pub fn load_or_seed_logged<T: serde::de::DeserializeOwned + Default + serde::Serialize>(who: &str, path: &Path) -> T {
    if rmsvc_core::config::is_corrupt::<T>(path) {
        let bak = rmsvc_core::config::backup_corrupt(path, None);
        eprintln!("[{who}] {} 解析失败，按缺省运行、不覆盖原文件{}", path.display(), if bak.is_some() { "（副本在同目录 .corrupt）" } else { "" });
    }
    rmsvc_core::config::load_or_seed(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct C {
        a: u32,
    }

    #[test]
    fn corrupt_config_falls_back_keeps_original_and_backs_up() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("ink.json");
        assert_eq!(load_or_seed_logged::<C>("t", &p), C::default());
        assert!(p.exists(), "不存在时写出缺省");
        std::fs::write(&p, b"{ broken").unwrap();
        assert_eq!(load_or_seed_logged::<C>("t", &p), C::default());
        assert_eq!(std::fs::read(&p).unwrap(), b"{ broken", "原文件不动");
        assert_eq!(std::fs::read(t.path().join("ink.json.corrupt")).unwrap(), b"{ broken", "另存副本");
        std::fs::write(&p, br#"{"a":7}"#).unwrap();
        assert_eq!(load_or_seed_logged::<C>("t", &p).a, 7);
    }

    #[test]
    fn entry_paths_are_encoded() {
        assert_eq!(InkClient::entry_path("u 1", "e/2", "draft"), "/books/u%201/entries/e%2F2/draft");
    }
}
