//! shelf-gateway —— 书架对外唯一入口（`0.0.0.0:8778`）。
//! 职责：① 托管单页 UI；② `/api/services` 列注册表；③ `/api/<service>/*` 反向代理到
//! 该服务的 loopback 端口（流式转发 body）。服务缺席 → 404 "未安装"，UI 据 `/api/services` 隐藏 tab。
//! 对外 **HTTPS（自签，首启生成）+ HTTP Basic 密码**（用户 2026-09-03 要求：防止知道地址就乱传文件）。
//! 子命令：`serve [--bind]` · `passwd <新密码>` · `show-password`（初始密码，改过后无）。
mod config;
mod proxy;
mod ui;

use shelf_core::http::{ApiError, Method, Reply, Router};
use shelf_core::paths::Paths;
use shelf_core::registry;
use shelf_core::http::{BasicAuth, ServeOpts};
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
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let mut cfg = config::GatewayConfig::load(&paths);
    match args.first().map(|s| s.as_str()) {
        Some("passwd") => {
            let pw = args.get(1).cloned().unwrap_or_default();
            match cfg.set_password(&paths, &pw) {
                Ok(()) => {
                    println!("[shelf-gateway] 密码已更新（重启网关生效：systemctl restart shelf-gateway）");
                    std::process::exit(0)
                }
                Err(e) => {
                    eprintln!("[shelf-gateway] {e}");
                    std::process::exit(1)
                }
            }
        }
        Some("show-password") => {
            match std::fs::read_to_string(config::initial_password_file(&paths)) {
                Ok(p) => println!("{}", p.trim()),
                Err(_) => println!("（初始密码已改过或尚未生成；改密码：shelf-gateway passwd <新密码>）"),
            }
            std::process::exit(0)
        }
        Some("serve") | None => {}
        Some(x) => {
            eprintln!("未知子命令 {x}（serve|passwd|show-password）");
            std::process::exit(2)
        }
    }
    let bind = service::parse_bind(&args, SPEC.default_bind);
    // 认证与 TLS
    let mut opts = ServeOpts::default();
    if cfg.auth {
        match cfg.ensure_password(&paths) {
            Ok(Some(pw)) => println!("[shelf-gateway] 初始密码已生成：用户 {}  密码 {}  （也在 {}；改密码：shelf-gateway passwd <新密码>）", cfg.user, pw, config::initial_password_file(&paths).display()),
            Ok(None) => {}
            Err(e) => eprintln!("[shelf-gateway] 生成初始密码失败: {e}（继续，但无密码保护！）"),
        }
        let (user, hash) = (cfg.user.clone(), cfg.password_hash.clone());
        if !hash.is_empty() {
            opts.basic_auth = Some(BasicAuth { realm: "shelf".into(), verify: std::sync::Arc::new(move |u, p| u == user && shelf_core::auth::verify_password(p, &hash)) });
        }
    }
    if cfg.https {
        let sans = device_ips();
        match shelf_core::tls::ensure_self_signed(&paths.config_dir().join("tls"), &sans) {
            Ok(pem) => opts.tls = Some(pem),
            Err(e) => eprintln!("[shelf-gateway] TLS 证书失败: {e}（回落 HTTP）"),
        }
    }
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
    if let Err(e) = service::run_with(&SPEC, &bind, &paths, router, opts) {
        eprintln!("[shelf-gateway] {e}");
        std::process::exit(1);
    }
}

/// 设备当前 IPv4 列表（进证书 SAN，减少浏览器地址不匹配告警；解析失败无碍）。
fn device_ips() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(out) = std::process::Command::new("ip").args(["-4", "-o", "addr"]).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(i) = line.find("inet ") {
                let rest = &line[i + 5..];
                let ip = rest.split('/').next().unwrap_or("").trim();
                if !ip.is_empty() && ip != "127.0.0.1" && !v.iter().any(|x| x == ip) {
                    v.push(ip.to_string());
                }
            }
        }
    }
    v
}
