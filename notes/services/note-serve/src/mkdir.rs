//! 《书名》文件夹缺失时的建夹请求：**复用书架已有的建夹代理队列**（`book-serve` 的 `POST /mkdir/add`，
//! 真正的建夹由 xochitl 自己的 `Library.createCollection` 执行，见 `shelf/services/book-serve/src/mkdir.rs` +
//! `shelf/xovi/shelf-mkdir-agent.qmd`）——跨服务调用同一份现役基础设施，跟 `trash.rs` 是同一个套路。
//! 这是 fire-and-forget：请求入队后不等待、不阻塞本次上传——这次大概率还是落库根，等 MainView 的
//! 8s 轮询代理把文件夹建出来后，这本书**下一次**生成别的章节时就能正确落进去了。
use shelf_core::paths::Paths;
use shelf_core::registry;
use std::time::Duration;

pub trait MkdirSink: Send + Sync {
    fn request(&self, name: &str) -> Result<(), String>;
}

pub struct BookServeMkdir {
    paths: Paths,
    agent: ureq::Agent,
}

impl BookServeMkdir {
    pub fn new(paths: Paths) -> BookServeMkdir {
        BookServeMkdir { paths, agent: ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(Duration::from_secs(10)).build() }
    }
}

impl MkdirSink for BookServeMkdir {
    fn request(&self, name: &str) -> Result<(), String> {
        let base = registry::find(&self.paths, "book-serve").map(|i| i.base_url()).ok_or("book-serve 未运行")?;
        let body = serde_json::json!({"name": name});
        self.agent.post(&format!("{base}/mkdir/add")).set("Content-Type", "application/json").send_string(&body.to_string()).map_err(|e| format!("book-serve POST /mkdir/add: {e}"))?;
        Ok(())
    }
}
