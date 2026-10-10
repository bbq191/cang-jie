//! 条目库的访问口（只经 ink-serve 的 HTTP，**不直接碰文件**：条目库唯一写者是 ink-serve）。
//! `EntryStore` 只声明本服务用到的四个动作，生产实现是共用的 `notesvc::InkClient`（2026-10-10 三份 `InkHttp`
//! 收成一份），测试用内存桩。
use notecore::api::{BookBrief, DraftPost};
use notecore::model::Book;
use notesvc::InkClient;

pub trait EntryStore: Send + Sync {
    fn list_books(&self) -> Result<Vec<BookBrief>, String>;
    fn book(&self, uuid: &str) -> Result<Book, String>;
    fn crop(&self, uuid: &str, file: &str) -> Result<Vec<u8>, String>;
    /// 写回草稿（转写**原文**；剥行首标记、样式采不采纳由 ink-serve 的 `Entry::accept_draft` 定，已校对 `text` 永不被覆盖）。
    fn post_draft(&self, uuid: &str, id: &str, draft: &DraftPost) -> Result<(), String>;
}

impl EntryStore for InkClient {
    // 转写只把错误写进一轮报告的 `note`（给人看），不需要状态码。
    fn list_books(&self) -> Result<Vec<BookBrief>, String> {
        InkClient::list_books(self).map_err(|e| e.message)
    }
    fn book(&self, uuid: &str) -> Result<Book, String> {
        InkClient::book(self, uuid).map_err(|e| e.message)
    }
    fn crop(&self, uuid: &str, file: &str) -> Result<Vec<u8>, String> {
        InkClient::crop(self, uuid, file)
    }
    fn post_draft(&self, uuid: &str, id: &str, draft: &DraftPost) -> Result<(), String> {
        InkClient::post_draft(self, uuid, id, draft)
    }
}
