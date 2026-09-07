//! note-serve —— 笔记·本（loopback 8798）。它是网页「笔记」tab 的注册方（tab 只挂一个服务，前端组合 ink/transcribe/mind/notes 四段）；
//! 本体职责是把条目库投影成设备笔记本（《书名》文件夹一章一本，xochitl 7 种打字样式）与 md 导出——投影/导出在后续步骤建。
//! 路由（经网关前缀 `/api/notes`）：`GET /status` · `GET /events`。
mod rmdoc;

use shelf_core::events::EventBus;
use shelf_core::http::{bind, Reply, Router};
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;

pub const APP: &str = "notes";

const SPEC: ServiceSpec = ServiceSpec { name: "note-serve", label: "笔记·本", version: env!("CARGO_PKG_VERSION"), default_bind: "127.0.0.1:8798", tab: Some(("笔记", 25)) };

struct State {
    paths: Paths,
    bus: Arc<EventBus>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let st = Arc::new(State { paths: paths.clone(), bus: Arc::new(EventBus::new()) });
    let router = Router::new()
        .get("/events", bind(&st, |s, _| Ok(s.bus.sse_reply())))
        .get("/status", bind(&st, |s, _| Ok(Reply::ok(&serde_json::json!({"ok": true, "vault": s.paths.app_data_dir(APP).join("vault"), "notebooks": "待建：条目库 → 一章一本"})))));
    println!("[note-serve] vault {}", st.paths.app_data_dir(APP).join("vault").display());
    if let Err(e) = service::run(&SPEC, &bind_addr, &paths, router) {
        eprintln!("[note-serve] {e}");
        std::process::exit(1);
    }
}
