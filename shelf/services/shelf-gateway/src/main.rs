//! shelf-gateway —— 书架对外唯一入口（`0.0.0.0:8778`）。
//! 职责：① 托管单页 UI；② `/api/services` 列注册表；③ `/api/<service>/*` 反向代理到
//! 该服务的 loopback 端口（流式转发 body）。服务缺席 → 404 "未安装"，UI 据 `/api/services` 隐藏 tab。
mod proxy;
mod ui;

use shelf_core::http::{ApiError, Method, Reply, Router};
use shelf_core::paths::Paths;
use shelf_core::registry;
use shelf_core::service::{self, ServiceSpec};

const SPEC: ServiceSpec = ServiceSpec {
    name: "shelf-gateway",
    label: "书架",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "0.0.0.0:8778",
    tab: None,
};

/// `/api/<service>/<rest>` 的服务名映射：URL 段 → 注册名。
fn service_of(segment: &str) -> Option<&'static str> {
    Some(match segment {
        "books" => "book-serve",
        "koreader" => "koreader-serve",
        "fonts" => "font-serve",
        "wallpapers" => "wallpaper-serve",
        "weread" => "weread-serve",
        _ => return None,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    let p1 = paths.clone();
    let p2 = paths.clone();
    let router = Router::new()
        .get("/", |_| Ok(Reply::html(ui::PAGE)))
        .get("/api/services", move |_| Ok(Reply::ok(&serde_json::json!({"services": registry::list(&p1)}))))
        .route(Method::Other, "/api/*", |_| Err(ApiError::bad("unsupported method")))
        .route(Method::Get, "/api/{svc}/*", { let p = p2.clone(); move |r| proxy::forward(&p, service_of(r.param("svc")), r) })
        .route(Method::Post, "/api/{svc}/*", { let p = p2.clone(); move |r| proxy::forward(&p, service_of(r.param("svc")), r) })
        .route(Method::Put, "/api/{svc}/*", { let p = p2.clone(); move |r| proxy::forward(&p, service_of(r.param("svc")), r) })
        .route(Method::Delete, "/api/{svc}/*", { let p = p2.clone(); move |r| proxy::forward(&p, service_of(r.param("svc")), r) })
        .route(Method::Get, "/api/{svc}", { let p = p2.clone(); move |r| proxy::forward(&p, service_of(r.param("svc")), r) })
        .route(Method::Post, "/api/{svc}", { let p = p2.clone(); move |r| proxy::forward(&p, service_of(r.param("svc")), r) });
    if let Err(e) = service::run(&SPEC, &bind, &paths, router) {
        eprintln!("[shelf-gateway] {e}");
        std::process::exit(1);
    }
}
