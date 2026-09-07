//! note-serve —— 笔记·本（loopback 8798）。它是网页「笔记」tab 的注册方（tab 只挂一个服务，前端组合 ink/transcribe/mind/notes 四段）；
//! 本体职责是把条目库投影成设备笔记本（《书名》文件夹一章一本，xochitl 7 种打字样式）与 md 导出（导出待建）。
//! 生成编排见 `publish.rs`：只读 ink-serve 的条目库（改字段仍是 ink-serve 的事）、按章指纹判断要不要重传，
//! 传完按 `visibleName`+时间窗认领设备新分配的 uuid，旧版本入 `book-serve` 回收站队列（真机验证过的软删路）。
//! 路由（经网关前缀 `/api/notes`）：`GET /status` · `GET /events` ·
//! `GET /books/{uuid}/notebooks`（本地记着的各章生成状态）·
//! `POST /books/{uuid}/generate`（全书重新投影+按需上传）· `POST /books/{uuid}/chapters/{idx}/generate`（单章）。
mod config;
mod ink;
mod notebooks;
mod publish;
mod rmdoc;
mod trash;

use config::NoteConfig;
use ink::{EntryStore, InkHttp};
use notebooks::NotebookState;
use publish::{generate_book, generate_chapter, ChapterResult, Ctx, Uploader, XochitlUploader};
use shelf_core::events::EventBus;
use shelf_core::http::{bind, ApiError, ApiResult, Reply, Router};
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;
use trash::{BookServeTrash, TrashSink};

pub const APP: &str = "notes";

const SPEC: ServiceSpec = ServiceSpec { name: "note-serve", label: "笔记·本", version: env!("CARGO_PKG_VERSION"), default_bind: "127.0.0.1:8798", tab: Some(("笔记", 25)) };

struct State {
    paths: Paths,
    cfg: NoteConfig,
    store: Box<dyn EntryStore>,
    uploader: Box<dyn Uploader>,
    trash: Box<dyn TrashSink>,
    notebooks: NotebookState,
    bus: Arc<EventBus>,
}

impl State {
    fn ctx(&self, now_ms: u64) -> Ctx<'_> {
        Ctx { store: self.store.as_ref(), uploader: self.uploader.as_ref(), trash: self.trash.as_ref(), state: &self.notebooks, cfg: &self.cfg, now_ms }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn results_reply(results: &[ChapterResult]) -> ApiResult {
    Ok(Reply::ok(&serde_json::json!({"chapters": results})))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let cfg: NoteConfig = shelf_core::config::load_or_seed(&paths.app_config_dir(APP).join("note.json"));
    let notebooks = NotebookState::new(paths.app_state_dir(APP).join("notebooks"));
    if let Err(e) = notebooks.ensure() {
        eprintln!("[note-serve] 建目录失败: {e}");
        std::process::exit(1);
    }
    let uploader = XochitlUploader::new(&cfg.xochitl_host, &paths.xochitl_dir(), cfg.upload_timeout_secs);
    let st = Arc::new(State {
        store: Box::new(InkHttp::new(paths.clone())),
        uploader: Box::new(uploader),
        trash: Box::new(BookServeTrash::new(paths.clone())),
        notebooks,
        cfg,
        paths: paths.clone(),
        bus: Arc::new(EventBus::new()),
    });
    let router = Router::new()
        .get("/events", bind(&st, |s, _| Ok(s.bus.sse_reply())))
        .get("/status", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"ok": true, "vault": s.paths.app_data_dir(APP).join("vault"), "folderPattern": s.cfg.folder_name_pattern, "xochitlHost": s.cfg.xochitl_host, "export": "待建"})))))
        .get("/books", bind(&st, |s, _| {
            let items = s.store.list_books().map_err(ApiError::bad)?;
            let out: Vec<serde_json::Value> = items.iter().map(|b| serde_json::json!({"uuid": b.uuid, "notebooks": s.notebooks.list(&b.uuid)})).collect();
            Ok(Reply::ok(&serde_json::json!({"items": out})))
        }))
        .get("/books/{uuid}/notebooks", bind(&st, |s, r| {
            let items: std::collections::BTreeMap<usize, notebooks::ChapterRecord> = s.notebooks.list(r.param("uuid"));
            Ok(Reply::ok(&serde_json::json!({"chapters": items})))
        }))
        .post("/books/{uuid}/generate", bind(&st, |s, r| {
            let uuid = r.param("uuid").to_string();
            let results = generate_book(&s.ctx(now_ms()), &uuid).map_err(ApiError::bad)?;
            s.bus.publish("notes", "notebooks");
            results_reply(&results)
        }))
        .post("/books/{uuid}/chapters/{idx}/generate", bind(&st, |s, r| {
            let uuid = r.param("uuid").to_string();
            let idx: usize = r.param("idx").parse().map_err(|_| ApiError::bad("章序号不对"))?;
            let book = s.store.book(&uuid).map_err(ApiError::bad)?;
            let result = generate_chapter(&s.ctx(now_ms()), &book, idx);
            s.bus.publish("notes", "notebooks");
            results_reply(std::slice::from_ref(&result))
        }));
    println!("[note-serve] 状态 {}；文件夹样式 {}；xochitl {}", st.notebooks.dir().display(), st.cfg.folder_name_pattern, st.cfg.xochitl_host);
    if let Err(e) = service::run(&SPEC, &bind_addr, &paths, router) {
        eprintln!("[note-serve] {e}");
        std::process::exit(1);
    }
}
