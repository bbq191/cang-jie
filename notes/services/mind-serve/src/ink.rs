//! 条目库的访问口（只经 ink-serve 的 HTTP，**不直接碰文件**：条目库唯一写者是 ink-serve）。
//! 比 transcribe-serve 的同名模块简单——不用列书/取裁图（没有批量扫描），只要读一本书（拿到问题所在
//! 那条的书名/章节/勾画/转写文本）和写回 `answer` 两个动作。
use notecore::model::{Answer, Book};
use shelf_core::paths::Paths;
use shelf_core::registry;
use std::time::Duration;

pub trait EntryStore: Send + Sync {
    fn book(&self, uuid: &str) -> Result<Book, String>;
    fn post_answer(&self, uuid: &str, id: &str, answer: &Answer) -> Result<(), String>;
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
}

fn enc(s: &str) -> String {
    shelf_core::multipart::percent_encode(s)
}

impl EntryStore for InkHttp {
    fn book(&self, uuid: &str) -> Result<Book, String> {
        let v: serde_json::Value = self.agent.get(&format!("{}/books/{}", self.base()?, enc(uuid))).call().map_err(|e| format!("ink-serve GET /books/{uuid}: {e}")).and_then(|r| serde_json::from_reader(r.into_reader()).map_err(|e| format!("book 应答不是 JSON: {e}")))?;
        serde_json::from_value(v).map_err(|e| format!("book 形状不对: {e}"))
    }
    fn post_answer(&self, uuid: &str, id: &str, answer: &Answer) -> Result<(), String> {
        let body = serde_json::json!({"answer": answer});
        self.agent.post(&format!("{}/books/{}/entries/{}", self.base()?, enc(uuid), enc(id))).set("Content-Type", "application/json").send_string(&body.to_string()).map_err(|e| format!("写回回答 {id}: {e}"))?;
        Ok(())
    }
}
