//! 条目库的只读访问口（本服务只读，不改字段——改字段是 ink-serve/transcribe-serve/mind-serve 的事）。
//! 生产实现是共用的 `notesvc::InkClient`（2026-10-10 三份 `InkHttp` 收成一份）；测试用内存桩。
use notecore::model::Book;
use notesvc::InkClient;
use rmsvc_core::registry::SvcError;

pub trait EntryStore: Send + Sync {
    /// 错误带 ink-serve 的状态码（没运行/连不上为 `None`），处理函数 `?` 即可：404 透传、连不上 503。
    fn book(&self, uuid: &str) -> Result<Book, SvcError>;
}

impl EntryStore for InkClient {
    fn book(&self, uuid: &str) -> Result<Book, SvcError> {
        InkClient::book(self, uuid)
    }
}
