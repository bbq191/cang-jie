//! 服务启动模板（Template Method）：解析参数 → 建目录 → 自注册 → 起服务器。
//! 各领域服务只提供 [`ServiceSpec`] 与路由构造函数，其余流程一致。
use crate::http::{self, Reply, Router};
use crate::paths::Paths;
use crate::registry::{self, ServiceInfo, UiTab};

pub struct ServiceSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub version: &'static str,
    pub default_bind: &'static str,
    pub tab: Option<(&'static str, u32)>,
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
    paths.ensure().map_err(|e| format!("建目录失败: {e}"))?;
    let info = ServiceInfo {
        name: spec.name.into(),
        port: port_of(bind),
        label: spec.label.into(),
        version: spec.version.into(),
        pid: std::process::id(),
        ui: spec.tab.map(|(t, o)| UiTab { title: t.into(), order: o }),
    };
    let _reg = registry::register(paths, &info).map_err(|e| format!("注册失败: {e}"))?;
    let name = spec.name;
    let ver = spec.version;
    let router = router.get("/health", move |_| Ok(Reply::ok(&serde_json::json!({"ok": true, "service": name, "version": ver}))));
    println!("[{}] v{} 监听 http://{}/", spec.name, spec.version, bind);
    http::serve(bind, router)
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
