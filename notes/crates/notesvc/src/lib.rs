//! 笔记线服务共用件（2026-10-10 收拢，NT-5/NT-6）：
//! - [`InkClient`]：访问 ink-serve 条目库的唯一 HTTP 客户端。此前 transcribe/mind/note 三个服务各有一份 `InkHttp`，
//!   路径拼法、请求体都各写一遍；请求/应答形状现在用 `notecore::api` 的契约类型，收发两端同一份定义。
//!   各服务**仍保留自己的窄 `EntryStore` trait**（只声明自己用到的动作），在服务里给 `InkClient` 实现它——那是离线
//!   单测换内存桩的接缝，不合并。
//!
//! 曾经还有 `load_or_seed_logged`（配置损坏时打日志 + 另存 `.corrupt` 副本），2026-10-10 下沉进基座 `rmsvc_core::config::load_or_seed`。
use notecore::api::{AnswerPost, BookBrief, BookList, DraftPost};
use notecore::model::Book;
use rmsvc_core::paths::Paths;
use rmsvc_core::registry::{enc, SvcClient, SvcError};

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
    /// 读操作返回结构化的 [`SvcError`]（带 ink-serve 的状态码；没运行/连不上时 `status: None`），HTTP 处理函数直接 `?`
    /// 就能把 ink-serve 的 404 原样透传、连不上回 503（`From<SvcError> for ApiError`）。此前是字符串错误，note-serve
    /// 一律回 400、mind-serve 一律回 404，连"ink-serve 未运行"也是 400（2026-10-10 审计 CORE-1）。只要文字的调用方
    /// （转写的一轮报告）取 `.message`。
    pub fn list_books(&self) -> Result<Vec<BookBrief>, SvcError> {
        Ok(self.0.try_get_typed::<BookList>("/books")?.items)
    }
    /// 整本书（应答里每条条目多一个 `live`，`Entry` 反序列化时忽略）。错误同 [`Self::list_books`]。
    pub fn book(&self, uuid: &str) -> Result<Book, SvcError> {
        self.0.try_get_typed(&format!("/books/{}", enc(uuid)))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(t: &tempfile::TempDir) -> Paths {
        let h = t.path().to_str().unwrap().to_string();
        Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None })
    }

    /// 本地起一个假 ink-serve（应答固定的状态码与体），登记进临时运行时目录的服务注册表。
    fn fake_ink(paths: &Paths, status: u16, body: &'static str) -> rmsvc_core::registry::Registration {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let _ = req.respond(tiny_http::Response::from_string(body).with_status_code(status));
            }
        });
        let info = rmsvc_core::registry::ServiceInfo { name: "ink-serve".into(), port, label: String::new(), version: String::new(), pid: std::process::id(), ui: None };
        rmsvc_core::registry::register(paths, &info).unwrap()
    }

    #[test]
    fn read_errors_carry_ink_status_or_none_when_not_running() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        let ink = InkClient::new(p.clone());
        let e = ink.book("b1").unwrap_err();
        assert_eq!(e.status, None, "没运行 → 无状态码（处理函数回 503）: {e:?}");
        let _reg = fake_ink(&p, 404, r#"{"ok":false,"message":"没有这本书的条目"}"#);
        let e = ink.book("b1").unwrap_err();
        assert_eq!((e.status, e.message.as_str()), (Some(404), "没有这本书的条目"), "对方的 404 与原因原样带回");
        assert_eq!(rmsvc_core::http::ApiError::from(e).status, 404);
    }

    #[test]
    fn reads_typed_book_list() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        let _reg = fake_ink(&p, 200, r#"{"items":[{"uuid":"b1","title":"书","chapters":3,"entries":5,"pending":2}]}"#);
        let books = InkClient::new(p).list_books().unwrap();
        assert_eq!((books.len(), books[0].uuid.as_str(), books[0].pending), (1, "b1", 2));
    }

    #[test]
    fn entry_paths_are_encoded() {
        assert_eq!(InkClient::entry_path("u 1", "e/2", "draft"), "/books/u%201/entries/e%2F2/draft");
    }
}
