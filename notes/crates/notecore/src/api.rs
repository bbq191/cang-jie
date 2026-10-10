//! ink-serve HTTP 契约的请求/应答形状（NT-2，2026-10-10）：收发两端共用同一份类型——ink-serve 用它解析请求、
//! 组应答，transcribe-serve / mind-serve 用它发请求、读应答。此前请求体两端各自手拼 `json!`、接收端逐字段
//! `from_value(..).ok()`：字段形状写错照样回 200、悄悄丢掉，`answer` 形状不对甚至会把已有回答清成空。
//! 现在请求体一律 `deny_unknown_fields`，解析不过 ink-serve 回 400，不再静默。
//!
//! 条目状态怎么变不在这里定，全在 `model::Entry` 的方法里（`accept_draft`/`accept_answer`/`apply_marked_text`…）；
//! 这里只管"线上长什么样"。
use crate::model::{Answer, Book, Destination, Entry};
use serde::{Deserialize, Deserializer, Serialize};

/// 网页改字端点 `POST /books/{uuid}/entries/{id}` 的请求体：网页只发这四个字段，各自可缺省（缺省＝不改）。
/// 草稿、回答不走这里，分别是 `…/draft`、`…/answer` 两个命令端点（发送方只有转写/问 AI 服务，字段由它们定）。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryPatch {
    /// 校对文本，行首 `-`/`1.`/`口`/`##`… 标记定样式并剥掉，见 [`Entry::apply_marked_text`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_ai: Option<bool>,
    /// 外层 `None`＝没给这个字段（不改）；`Some(None)`＝给了 `null`（清空）；空白字符串也算清空。
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub question: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<Destination>,
}

/// 区分"字段缺省"和"字段为 null"：字段在场就包一层 `Some`（缺省时 `#[serde(default)]` 给外层 `None`）。
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

impl EntryPatch {
    /// 套到条目上。终态条目（跳过/撤销/删除）一律拒绝，要先在回收站恢复（2026-09-09 审计：此前这条通用端点能把
    /// 终态条目通过改字悄悄拉回活跃态）。空请求体不算改动、不碰 `updated`。
    pub fn apply(&self, e: &mut Entry, now: u64) -> Result<(), String> {
        if e.is_terminal() {
            return Err("这条已跳过/撤销/删除，不能再改，请先在回收站里恢复".to_string());
        }
        if *self == EntryPatch::default() {
            return Ok(());
        }
        if let Some(t) = &self.text {
            e.apply_marked_text(t, now);
        }
        if let Some(v) = self.ask_ai {
            e.ask_ai = v;
        }
        if let Some(q) = &self.question {
            e.question = q.as_deref().filter(|s| !s.trim().is_empty()).map(str::to_string);
        }
        if let Some(d) = self.destination {
            e.destination = d;
        }
        e.updated = now;
        Ok(())
    }
}

/// `POST /books/{uuid}/entries/{id}/draft`（transcribe-serve → ink-serve）：转写**原文**（行首标记不剥，ink-serve
/// 的 [`Entry::accept_draft`] 统一处理）+ 写草稿的后端标识 + 草稿对应的簇指纹。时间由 ink-serve 盖。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftPost {
    pub text: String,
    pub backend: String,
    pub hash: String,
}

/// `POST /books/{uuid}/entries/{id}/answer`（mind-serve → ink-serve）：回答文本 + 后端标识 + 问题存档（`Answer.brief`）。
/// 时间由 ink-serve 盖。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerPost {
    pub text: String,
    pub backend: String,
    pub brief: String,
}

impl AnswerPost {
    pub fn into_answer(self, at: u64) -> Answer {
        Answer { text: self.text, backend: self.backend, at, brief: self.brief }
    }
}

/// `GET /books` 每一项（网页书下拉、transcribe-serve 挑有待转写的书）。`entries` 不计 `Revoked`。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BookBrief {
    pub uuid: String,
    pub title: String,
    pub chapters: usize,
    pub entries: usize,
    /// `needs_transcribe()` 的条目数。
    pub pending: usize,
}

impl BookBrief {
    pub fn of(b: &Book) -> BookBrief {
        BookBrief {
            uuid: b.uuid.clone(),
            title: b.title.clone(),
            chapters: b.chapters.len(),
            entries: b.entries.iter().filter(|e| e.status != crate::model::Status::Revoked).count(),
            pending: b.entries.iter().filter(|e| e.needs_transcribe()).count(),
        }
    }
}

/// `GET /books` 应答外壳。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BookList {
    pub items: Vec<BookBrief>,
}

/// 应答里的一条条目：条目库原样字段 + `live`（= `status.is_live_for_projection()`，网页不再自己抄一份判据）。
/// `live` 只在 HTTP 应答里，**不写进条目库文件**——落盘仍是 `Entry` 本身。读方（`Entry` 反序列化）不认识
/// `live` 会直接忽略，所以 transcribe/mind/note 三个服务照旧按 `Book` 读这份应答。
#[derive(Serialize, Debug)]
pub struct EntryView<'a> {
    #[serde(flatten)]
    pub entry: &'a Entry,
    pub live: bool,
}

impl<'a> EntryView<'a> {
    pub fn of(entry: &'a Entry) -> EntryView<'a> {
        EntryView { entry, live: entry.status.is_live_for_projection() }
    }
}

/// `GET /books/{uuid}` 应答：字段与 [`Book`] 一一对应（`entries` 换成带 `live` 的 [`EntryView`]）。`Book` 加字段时
/// 这里要跟着加——测试 `book_view_has_exactly_book_fields_plus_live` 会先报错。
#[derive(Serialize, Debug)]
pub struct BookView<'a> {
    pub uuid: &'a str,
    pub title: &'a str,
    pub author: &'a str,
    pub chapters: &'a [String],
    pub entries: Vec<EntryView<'a>>,
    pub page_mtimes: &'a std::collections::BTreeMap<String, u64>,
}

impl<'a> BookView<'a> {
    pub fn of(b: &'a Book) -> BookView<'a> {
        BookView { uuid: &b.uuid, title: &b.title, author: &b.author, chapters: &b.chapters, entries: b.entries.iter().map(EntryView::of).collect(), page_mtimes: &b.page_mtimes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;

    fn entry(status: &str) -> Entry {
        serde_json::from_str(&format!(r#"{{"id":"e","page":"p","page_index":0,"created":0,"updated":0,"status":"{status}","question":"旧问题"}}"#)).unwrap()
    }

    #[test]
    fn patch_rejects_unknown_or_misshaped_fields() {
        assert!(serde_json::from_str::<EntryPatch>(r#"{"answer":{"text":"x"}}"#).is_err(), "回答不走改字端点");
        assert!(serde_json::from_str::<EntryPatch>(r#"{"draft":{"text":"x"}}"#).is_err());
        assert!(serde_json::from_str::<EntryPatch>(r#"{"askAi":"yes"}"#).is_err(), "类型不对不能悄悄当没给");
        assert!(serde_json::from_str::<EntryPatch>(r#"{"destination":"nowhere"}"#).is_err());
        assert!(serde_json::from_str::<AnswerPost>(r#"{"text":"x","backend":"b"}"#).is_err(), "回答缺字段");
        assert!(serde_json::from_str::<DraftPost>(r#"{"text":"x","backend":"b","hash":"h","style":"bullet"}"#).is_err());
    }

    #[test]
    fn patch_question_absent_null_and_blank() {
        let mut e = entry("reviewed");
        serde_json::from_str::<EntryPatch>(r#"{"askAi":true}"#).unwrap().apply(&mut e, 1).unwrap();
        assert_eq!((e.ask_ai, e.question.as_deref(), e.updated), (true, Some("旧问题"), 1), "没给 question 不动");
        serde_json::from_str::<EntryPatch>(r#"{"question":"  "}"#).unwrap().apply(&mut e, 2).unwrap();
        assert_eq!(e.question, None, "空白＝清空");
        serde_json::from_str::<EntryPatch>(r#"{"question":"新的"}"#).unwrap().apply(&mut e, 3).unwrap();
        serde_json::from_str::<EntryPatch>(r#"{"question":null}"#).unwrap().apply(&mut e, 4).unwrap();
        assert_eq!(e.question, None, "null＝清空");
        serde_json::from_str::<EntryPatch>("{}").unwrap().apply(&mut e, 9).unwrap();
        assert_eq!(e.updated, 4, "空请求体不算改动");
    }

    #[test]
    fn patch_text_and_destination_and_terminal_refused() {
        let mut e = entry("draft");
        let p: EntryPatch = serde_json::from_str(r#"{"text":"- 查作者","destination":"obsidian"}"#).unwrap();
        p.apply(&mut e, 5).unwrap();
        assert_eq!((e.text.as_deref(), e.status, e.destination), (Some("查作者"), Status::Reviewed, Destination::Obsidian));
        let mut t = entry("archived");
        assert!(p.apply(&mut t, 6).is_err());
        assert_eq!(t.text, None);
    }

    #[test]
    fn senders_serialize_what_receiver_accepts() {
        let p = EntryPatch { ask_ai: Some(true), question: Some(Some("q".into())), ..Default::default() };
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(j, r#"{"askAi":true,"question":"q"}"#);
        assert_eq!(serde_json::from_str::<EntryPatch>(&j).unwrap(), p);
        let d = DraftPost { text: "- x".into(), backend: "b".into(), hash: "h".into() };
        assert_eq!(serde_json::from_str::<DraftPost>(&serde_json::to_string(&d).unwrap()).unwrap(), d);
        let a = AnswerPost { text: "t".into(), backend: "b".into(), brief: "q".into() };
        assert_eq!(serde_json::from_str::<AnswerPost>(&serde_json::to_string(&a).unwrap()).unwrap(), a);
    }

    /// `BookView` 与 `Book` 字段一一对应，只在每条条目上多一个 `live`；读方按 `Book` 读回来与原书相等。
    #[test]
    fn book_view_has_exactly_book_fields_plus_live() {
        let mut b: Book = serde_json::from_str(r#"{"uuid":"u","title":"t","author":"a","chapters":["一"],"page_mtimes":{"p":1}}"#).unwrap();
        b.entries = vec![entry("mined"), entry("pending"), entry("archived")];
        let view = serde_json::to_value(BookView::of(&b)).unwrap();
        let raw = serde_json::to_value(&b).unwrap();
        let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&view), keys(&raw), "书级字段一一对应");
        for (v, r) in view["entries"].as_array().unwrap().iter().zip(raw["entries"].as_array().unwrap()) {
            let mut v = v.clone();
            let live = v.as_object_mut().unwrap().remove("live").unwrap();
            assert_eq!(&v, r, "条目字段原样");
            assert_eq!(live, r["status"] == "pending");
        }
        assert_eq!(serde_json::from_value::<Book>(view).unwrap(), b, "读方忽略 live");
    }
}
