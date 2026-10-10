//! 条目库的访问口（只经 ink-serve 的 HTTP，**不直接碰文件**：条目库唯一写者是 ink-serve）。
//! 生产实现是共用的 `notesvc::InkClient`（2026-10-10 三份 `InkHttp` 收成一份）。`worker` 只需要"写回回答"这一个动作，
//! `EntryStore` 就只声明它（测试换内存桩的接缝）；取书在 main.rs 里直接用 `InkClient::book`。
use notecore::api::AnswerPost;
use notesvc::InkClient;

pub trait EntryStore: Send + Sync {
    fn post_answer(&self, uuid: &str, id: &str, answer: &AnswerPost) -> Result<(), String>;
}

impl EntryStore for InkClient {
    fn post_answer(&self, uuid: &str, id: &str, answer: &AnswerPost) -> Result<(), String> {
        InkClient::post_answer(self, uuid, id, answer)
    }
}
