//! 服务启动模板（Template Method）：解析参数 → 建目录 → 自注册 → 起服务器。
//! 各领域服务只提供 [`ServiceSpec`] 与路由构造函数，其余流程一致。
use crate::http::{self, Reply, Router, ServeOpts};
use crate::paths::Paths;
use crate::registry::{self, ServiceInfo, UiTab};

pub struct ServiceSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub version: &'static str,
    pub default_bind: &'static str,
    /// 在网关网页上占一个顶层 tab 时的排序（小的在前）；`None`＝不占 tab。标题由网页语言包给，这里不带
    /// （2026-10-10 前是 `Option<(标题, 排序)>`，标题网页从来不读，审计 GW-3）。
    pub tab_order: Option<u32>,
}

/// 命令行：`serve [--bind 127.0.0.1:8790]`。返回 bind。
pub fn parse_bind(args: &[String], default: &str) -> String {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--bind" {
            if let Some(b) = it.next() {
                return b.clone();
            }
        }
    }
    default.to_string()
}

fn port_of(bind: &str) -> u16 {
    bind.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(0)
}

/// 跑服务：自动挂 `GET /health`。永不返回（失败 Err）。
pub fn run(spec: &ServiceSpec, bind: &str, paths: &Paths, router: Router) -> Result<(), String> {
    run_with(spec, bind, paths, router, ServeOpts::default())
}

/// 各服务 `main` 的收尾：跑服务，失败打一行 `[<服务名>] <原因>` 并以退出码 1 结束（交给 systemd `Restart=on-failure`）。
/// 此前 8 个服务 main 各写一遍 `if let Err(e) = service::run(..) { eprintln!(..); exit(1) }`。
pub fn run_or_exit(spec: &ServiceSpec, bind: &str, paths: &Paths, router: Router, opts: ServeOpts) -> ! {
    match run_with(spec, bind, paths, router, opts) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("[{}] {e}", spec.name);
            std::process::exit(1)
        }
    }
}

pub fn run_with(spec: &ServiceSpec, bind: &str, paths: &Paths, router: Router, opts: ServeOpts) -> Result<(), String> {
    paths.ensure().map_err(|e| format!("建目录失败: {e}"))?;
    let info = ServiceInfo {
        name: spec.name.into(),
        port: port_of(bind),
        label: spec.label.into(),
        version: spec.version.into(),
        pid: std::process::id(),
        ui: spec.tab_order.map(|order| UiTab { order }),
    };
    let _reg = registry::register(paths, &info).map_err(|e| format!("注册失败: {e}"))?;
    let name = spec.name;
    let ver = spec.version;
    // 统一挂 /health。分发按"最具体的路由优先"（见 `Router::dispatch`），注册先后不再影响它会不会被 `GET /{name}` 抢走。
    let router = Router::new()
        .get("/health", move |_| Ok(Reply::ok(&serde_json::json!({"ok": true, "service": name, "version": ver}))))
        .merge(router);
    println!("[{}] v{} 监听 {}://{}/{}", spec.name, spec.version, if opts.tls.is_some() { "https" } else { "http" }, bind, if opts.guard.is_some() { "（密码保护）" } else { "" });
    http::serve_with(bind, router, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_parsing() {
        let a: Vec<String> = ["serve", "--bind", "0.0.0.0:1"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_bind(&a, "127.0.0.1:9"), "0.0.0.0:1");
        assert_eq!(parse_bind(&[], "127.0.0.1:9"), "127.0.0.1:9");
        assert_eq!(port_of("127.0.0.1:8790"), 8790);
    }
}
