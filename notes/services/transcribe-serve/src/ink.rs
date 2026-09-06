//! 条目库的访问口（只经 ink-serve 的 HTTP，**不直接碰文件**：条目库唯一写者是 ink-serve）。
//! `EntryStore` 抽象出四个动作，生产走注册表找 ink-serve，测试用内存桩。
use notecore::model::{Book, Draft, Style};
use serde::Deserialize;
use shelf_core::paths::Paths;
use shelf_core::registry;
use std::time::Duration;

#[derive(Deserialize, Debug, Clone)]
pub struct BookBrief {
    pub uuid: String,
    #[serde(default)]
    pub pending: usize,
}

pub trait EntryStore: Send + Sync {
    fn list_books(&self) -> Result<Vec<BookBrief>, String>;
    fn book(&self, uuid: &str) -> Result<Book, String>;
    fn crop(&self, uuid: &str, file: &str) -> Result<Vec<u8>, String>;
    /// 写回草稿（与可选的样式修正）。ink-serve 保证不覆盖已校对 `text`。
    fn post_draft(&self, uuid: &str, id: &str, draft: &Draft, style: Option<Style>) -> Result<(), String>;
}

pub struct InkHttp {
    paths: Paths,
    agent: ureq::Agent,
}

impl InkHttp {
    pub fn new(paths: Paths) -> InkHttp {
        InkHttp { paths, agent: ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(Duration::from_secs(30)).build() }
    }
    pub fn base(&self) -> Result<String, String> {
        registry::find(&self.paths, "ink-serve").map(|i| i.base_url()).ok_or_else(|| "ink-serve 未运行".to_string())
    }
    fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        self.agent.get(&format!("{}{path}", self.base()?)).call().map_err(|e| format!("ink-serve GET {path}: {e}")).and_then(|r| serde_json::from_reader(r.into_reader()).map_err(|e| format!("ink-serve {path} 应答不是 JSON: {e}")))
    }
}

fn enc(s: &str) -> String {
    shelf_core::multipart::percent_encode(s)
}

impl EntryStore for InkHttp {
    fn list_books(&self) -> Result<Vec<BookBrief>, String> {
        let v = self.get_json("/books")?;
        serde_json::from_value(v.get("items").cloned().unwrap_or_default()).map_err(|e| format!("books 形状不对: {e}"))
    }
    fn book(&self, uuid: &str) -> Result<Book, String> {
        serde_json::from_value(self.get_json(&format!("/books/{}", enc(uuid)))?).map_err(|e| format!("book 形状不对: {e}"))
    }
    fn crop(&self, uuid: &str, file: &str) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        self.agent.get(&format!("{}/books/{}/crops/{}", self.base()?, enc(uuid), enc(file))).call().map_err(|e| format!("取裁图 {file}: {e}"))?.into_reader().read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
    fn post_draft(&self, uuid: &str, id: &str, draft: &Draft, style: Option<Style>) -> Result<(), String> {
        let mut body = serde_json::json!({"draft": draft});
        if let Some(s) = style {
            body["style"] = serde_json::to_value(s).unwrap_or_default();
        }
        self.agent.post(&format!("{}/books/{}/entries/{}", self.base()?, enc(uuid), enc(id))).set("Content-Type", "application/json").send_string(&body.to_string()).map_err(|e| format!("写回草稿 {id}: {e}"))?;
        Ok(())
    }
}
