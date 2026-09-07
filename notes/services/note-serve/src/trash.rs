//! 旧版本笔记本的软删：**复用书架已有的原生回收站队列**（`book-serve` 的 `POST /trash/add`，真机验证过的
//! `EntitySelection.selectionMoveToTrash()` 路，见 `shelf/services/book-serve/src/trash.rs` +
//! `shelf/xovi/shelf-trash-agent.qmd`）——不是"旧代码"，是跨服务调用同一份现役基础设施，笔记线只挂队列不重造。
//! `add()` 按 uuid+visibleName 入队，`book-serve` 拒绝名字对不上的 uuid（防错删），所以调用方必须传
//! 生成时记录下来的、这份旧文档当时的 visibleName，不能拿当前（可能已变）的书名/章名重算。
use shelf_core::paths::Paths;
use shelf_core::registry;
use std::time::Duration;

pub trait TrashSink: Send + Sync {
    /// 入队；`book-serve` 未运行 / 名字对不上 / uuid 不在库里都算失败，调用方应当"不阻塞本次生成"
    /// 只记日志——旧文档留着不算错，比误删强。
    fn add(&self, uuid: &str, name: &str) -> Result<(), String>;
}

pub struct BookServeTrash {
    paths: Paths,
    agent: ureq::Agent,
}

impl BookServeTrash {
    pub fn new(paths: Paths) -> BookServeTrash {
        BookServeTrash { paths, agent: ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(Duration::from_secs(10)).build() }
    }
}

impl TrashSink for BookServeTrash {
    fn add(&self, uuid: &str, name: &str) -> Result<(), String> {
        let base = registry::find(&self.paths, "book-serve").map(|i| i.base_url()).ok_or("book-serve 未运行")?;
        let body = serde_json::json!({"uuid": uuid, "name": name});
        self.agent.post(&format!("{base}/trash/add")).set("Content-Type", "application/json").send_string(&body.to_string()).map_err(|e| format!("book-serve POST /trash/add: {e}"))?;
        Ok(())
    }
}
