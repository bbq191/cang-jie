//! 条目库的只读访问口（本服务只读，不改字段——改字段是 ink-serve/transcribe-serve/mind-serve 的事）。
//! 生产走注册表找 ink-serve；测试用内存桩。同款套路见 `transcribe-serve::ink`。
//!
//! 传输层委托 `rmsvc_core::registry::SvcClient`（2026-09-09 消重复，见该模块文档）。
use notecore::model::Book;
use rmsvc_core::paths::Paths;
use rmsvc_core::registry::{enc, SvcClient};

#[derive(serde::Deserialize, Debug, Clone)]
pub struct BookBrief {
    pub uuid: String,
    // ink-serve 的 `/books` 本来就吐这个字段（`main.rs` 的 `list_active` 投影），2026-09-16 之前
    // 这里没声明只是没人用；当时给 host `shelf notes pull` 认书用（该 CLI 2026-09-18 已砍）。
    pub title: String,
}

/// ink-serve `GET /books` 的应答外壳（`{"items": [...]}`）。
#[derive(serde::Deserialize)]
struct Items {
    #[serde(default)]
    items: Vec<BookBrief>,
}

pub trait EntryStore: Send + Sync {
    fn list_books(&self) -> Result<Vec<BookBrief>, String>;
    fn book(&self, uuid: &str) -> Result<Book, String>;
}

pub struct InkHttp(SvcClient);

impl InkHttp {
    pub fn new(paths: Paths) -> InkHttp {
        InkHttp(SvcClient::new(paths, "ink-serve", 30))
    }
}

impl EntryStore for InkHttp {
    fn list_books(&self) -> Result<Vec<BookBrief>, String> {
        Ok(self.0.get_typed::<Items>("/books")?.items)
    }
    fn book(&self, uuid: &str) -> Result<Book, String> {
        self.0.get_typed(&format!("/books/{}", enc(uuid)))
    }
}
