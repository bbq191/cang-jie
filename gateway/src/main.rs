//! gateway —— 书架对外唯一入口（部署固定 `0.0.0.0:443`，见 `systemd/gateway.service`
//! 的 `--bind`；这里的 `default_bind` 只是本地手动跑 `gateway serve` 不带参数时的兜底，特意
//! 留非特权端口，本地测试不用 root）。
//! 职责：① 托管单页 UI；② `/api/services` 列注册表；③ `/api/<service>/*` 反向代理到
//! 该服务的 loopback 端口（流式转发 body）。服务缺席 → 404 "未安装"，UI 据 `/api/services` 隐藏 tab。
//! 对外 **HTTPS（私有 CA 签发，首启生成，`/ca.crt` 可下载装信任）+ 登录页密码**（无用户名；首次默认 `shelf`、
//! 登录后必改；CLI 用 Basic）+ **mDNS `shelf.local`** 伪域名（用户 2026-09-03 要求）。策略见 `auth.rs`。
//! 子命令：`serve [--bind]` · `passwd <新密码>` · `reset-password`（回默认并强制改）· `regen-tls`（重签叶证书）。
mod auth;
mod batch;
mod config;
mod device;
mod enhance;
mod events;
mod manage;
mod proxy;
#[cfg(test)]
mod testutil;
mod ui;

use rmsvc_core::http::{bind, ApiError, Method, Reply, Router, ServeOpts};
use rmsvc_core::paths::Paths;
use rmsvc_core::service::{self, ServiceSpec};
use std::sync::Arc;

const SPEC: ServiceSpec = ServiceSpec {
    name: "gateway",
    label: "秘密花园",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "0.0.0.0:8778",
    tab: None,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::from_env();
    let _ = paths.ensure();
    let mut cfg = config::GatewayConfig::load(&paths);
    match args.first().map(|s| s.as_str()) {
        Some("passwd") => {
            let pw = args.get(1).cloned().unwrap_or_default();
            match cfg.set_password(&paths, &pw) {
                Ok(()) => println!("[gateway] 密码已更新（重启网关生效：systemctl restart gateway）"),
                Err(e) => {
                    eprintln!("[gateway] {e}");
                    std::process::exit(1)
                }
            }
            std::process::exit(0)
        }
        Some("reset-password") => {
            if let Err(e) = cfg.reset_password(&paths) {
                eprintln!("[gateway] {e}");
                std::process::exit(1)
            }
            println!("[gateway] 已重置为默认密码 {}，下次登录强制改（重启网关生效）", config::DEFAULT_PASSWORD);
            std::process::exit(0)
        }
        Some("regen-tls") => {
            let dir = paths.config_dir().join("tls");
            for f in ["cert.pem", "key.pem", "cert.meta"] {
                let _ = std::fs::remove_file(dir.join(f));
            }
            println!("[gateway] 叶证书已删除，网关下次启动用同一 CA 重签（已装 CA 的设备不受影响）");
            std::process::exit(0)
        }
        Some("serve") | None => {}
        Some(x) => {
            eprintln!("未知子命令 {x}（serve|passwd|reset-password|regen-tls）");
            std::process::exit(2)
        }
    }
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let mut opts = ServeOpts::default();
    let tls_dir = paths.config_dir().join("tls");
    if cfg.https {
        let mut sans = rmsvc_core::netinfo::ipv4_strings();
        if !cfg.mdns_name.trim().is_empty() {
            sans.push(format!("{}.local", cfg.mdns_name.trim()));
        }
        sans.extend(cfg.extra_sans.iter().cloned());
        match rmsvc_core::tls::ensure_ca_signed(&tls_dir, &sans) {
            Ok(pem) => opts.tls = Some(pem),
            Err(e) => eprintln!("[gateway] TLS 证书失败: {e}（回落 HTTP）"),
        }
    }
    let secure_cookie = opts.tls.is_some();
    // 认证：首启写默认密码 + 必改标志
    let state: Option<auth::Shared> = if cfg.auth {
        match cfg.ensure_password(&paths) {
            Ok(true) => println!("[gateway] 首次启动：默认密码 {}，网页登录后必须改", config::DEFAULT_PASSWORD),
            Ok(false) => {}
            Err(e) => eprintln!("[gateway] 写默认密码失败: {e}（继续，但无密码保护！）"),
        }
        if cfg.password_hash.is_empty() {
            None
        } else {
            let ttl = std::time::Duration::from_secs(u64::from(cfg.session_days.max(1)) * 86400);
            Some(Arc::new(auth::AuthState::new(cfg.clone(), rmsvc_core::auth::SessionStore::new(ttl, 64), paths.clone(), secure_cookie)))
        }
    } else {
        None
    };
    if let Some(st) = &state {
        opts.guard = Some(st.guard());
    }
    if !cfg.mdns_name.trim().is_empty() {
        rmsvc_core::mdns::spawn(vec![cfg.mdns_name.trim().to_string()]);
        println!("[gateway] mDNS 名 {}.local（iOS/macOS/Windows/Linux 可直接访问；安卓走热点 dnsmasq 别名）", cfg.mdns_name.trim());
    }
    let paths = Arc::new(paths);
    let bus = events::spawn(paths.clone()); // 先建总线：batch 的进度事件要发到它
    batch::resume(&paths); // 读回上次没跑完的批量队列继续跑（网关重启/部署新版本不丢）
    device::start(); // 后台只算一次 /usr/bin/xochitl 的 sha256（OTA 横幅的固件判据），不轮询
    let mut router = Router::new()
        .get("/", |_| Ok(Reply::html(ui::page())))
        .get("/ca.crt", { let d = tls_dir.clone(); move |_| Ok(match rmsvc_core::tls::ca_pem(&d) {
            Some(pem) => Reply::bytes("application/x-x509-ca-cert", pem).with_header("Content-Disposition", "attachment; filename=\"shelf-ca.crt\""),
            None => Reply::error(404, "HTTPS 未启用，无 CA"),
        }) })
        .get("/api/services", bind(&paths, |p, _| Ok(manage::services(p))))
        // 语言包：`{lang}` 整段捕获（路由只支持整段 `{param}`，不支持段内混literal后缀），前端仍按
        // `/ui/locales/zh-CN.json` 这种带扩展名的 URL 请求，这里自己剥掉 `.json`。不认识的语言码
        // `ui::locale_json` 会落中文，不会 404/空白。
        .get("/ui/locales/{lang}", |r| {
            let lang = r.param("lang").trim_end_matches(".json");
            Ok(Reply::bytes("application/json", ui::locale_json(lang).as_bytes().to_vec()))
        });
    if let Some(st) = &state {
        router = router
            .get("/login", |r| Ok(Reply::html(&ui::login_page("", r.q("next").unwrap_or("/")))))
            .post("/login", bind(st, |s, r| s.login(r)))
            .post("/logout", bind(st, |s, r| s.logout(r)))
            .get("/password", bind(st, |s, _| Ok(Reply::html(&ui::password_page("", s.must_change())))))
            .post("/password", bind(st, |s, r| s.change_password(r)))
            .get("/api/session", bind(st, |s, _| Ok(s.session_info())));
    } else {
        router = router.get("/api/session", |_| Ok(Reply::ok(&serde_json::json!({"ok": true, "mustChange": false, "auth": false}))));
    }
    // 管理台/引导等网关自身路由。路由器按"最具体优先"分发（字面段多者胜，见 rmsvc_core::http::router），
    // 与注册先后无关——/api/manage 这类不会被 /api/{svc}/* 代理通配抢走；旧注释"必须先注册"已不成立（09-20 起）。
    const PROXIED: &[Method] = &[Method::Get, Method::Post, Method::Put, Method::Delete];
    let router = router
        // 事件流（SSE）：受登录守卫（cookie/Basic）保护
        .get("/api/events", bind(&bus, |b, _| Ok(b.sse_reply())))
        .get("/api/manage", bind(&paths, |p, _| Ok(manage::status(p))))
        .get("/api/foundation", bind(&paths, |p, _| Ok(manage::foundation(p))))
        .post("/api/manage/{seg}/{action}", bind(&paths, manage::action))
        // 系统增强开关（Track 3）：网关自身固定能力。
        .get("/api/enhance/status", bind(&paths, |p, _| Ok(enhance::status(p))))
        .put("/api/enhance/qol", bind(&paths, enhance::set_qol))
        // 设备健康 / OTA 横幅 / 遗留清理（2026-09-25，见 device/mod.rs）：只读采集按需触发，不轮询。
        .get("/api/device/health", bind(&paths, device::health))
        .get("/api/device/ota", bind(&paths, device::ota_status))
        .get("/api/device/wifi", bind(&paths, device::wifi))
        .get("/api/device/cleanup", bind(&paths, device::cleanup_list))
        .post("/api/device/cleanup/delete", bind(&paths, device::cleanup_delete))
        // 服务端批量队列（见 batch.rs）：提交 `{action, names?, all?, folder?}`；`names` 缺省且 `all:true` 表示"所有适用的"。
        // 状态任何会话都能看（关掉浏览器重开、换设备都在）。
        .post("/api/batch", bind(&paths, batch::submit))
        .get("/api/batch/status", |_| Ok(Reply::ok(&batch::status())))
        .post("/api/batch/stop", bind(&paths, batch::stop_route))
        .route(Method::Other, "/api/*", |_| Err(ApiError::bad("unsupported method")))
        .any(PROXIED, "/api/{svc}/*", bind(&paths, proxy::forward))
        .any(PROXIED, "/api/{svc}", bind(&paths, proxy::forward));
    service::run_or_exit(&SPEC, &bind_addr, &paths, router, opts)
}
