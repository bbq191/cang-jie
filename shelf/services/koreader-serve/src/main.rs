//! koreader-serve —— KOReader（Phase 0 骨架：只有 /health 与自注册；领域路由随后各阶段接入）。
use shelf_core::http::Router;
use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};

const SPEC: ServiceSpec = ServiceSpec {
    name: "koreader-serve",
    label: "KOReader",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8791",
    tab: Some(("KOReader", 20)),
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let router = Router::new();
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[koreader-serve] {e}");
        std::process::exit(1);
    }
}
