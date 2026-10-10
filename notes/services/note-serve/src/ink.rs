//! 条目库的只读访问口（本服务只读，不改字段——改字段是 ink-serve/transcribe-serve/mind-serve 的事）。
//! 生产实现是共用的 `notesvc::InkClient`（2026-10-10 三份 `InkHttp` 收成一份）；测试用内存桩。
use notecore::model::Book;
use notesvc::InkClient;

pub trait EntryStore: Send + Sync {
    fn book(&self, uuid: &str) -> Result<Book, String>;
}

impl EntryStore for InkClient {
    fn book(&self, uuid: &str) -> Result<Book, String> {
        InkClient::book(self, uuid)
    }
}
