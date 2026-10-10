//! mind-serve —— 笔记·脑（loopback 8797）。**按条目单发**（二期重新设计，取代首期"按分区批量跑"）：
//! 网页给某条目勾了「问AI」+ 填了问题，点提问 → 拼 `书名 + 章节 + 勾画原文 + 转写文本 + 问题` → 文字模型 →
//! 写回 `answer`（经 ink-serve 的 HTTP，条目库唯一写者仍是它）。**没有后台线程、没有事件订阅**——跟
//! transcribe-serve 不一样，这里没有"自动扫待处理条目"这回事，纯粹被动等 HTTP 请求，空闲零 CPU
//! （二期耗电评估第 5 点）。出网服务：设备自己的 WiFi 直连。
//! 路由（经网关前缀 `/api/mind`）：`GET /status` · `GET /config` · `PUT /config`（`apiKey` 只写不读）·
//! `POST /books/{uuid}/entries/{id}/ask`（回答这一条——要求条目已勾「问AI」且填了问题，否则 400）。
//! **第二轮整理区反馈（2026-09-08）**：模型预置表横跨四家厂商，`GET /status` 的 `usageByModel` 按模型
//! 分账、带用户自填单价算出的花费估算，见 `config.rs`/`ledger.rs` 模块文档。
mod backend;
mod config;
mod ink;
mod ledger;
mod prompt;
mod worker;

use config::MindConfig;
use notesvc::InkClient;
use ledger::Ledger;
use rmsvc_core::http::{bind, ApiError, Reply, Router, ServeOpts};
use rmsvc_core::paths::Paths;
use rmsvc_core::service::{self, ServiceSpec};
use std::sync::Arc;
use std::time::Duration;
use vendorcfg::{ConfigCell, VendorConfig};
use worker::Ctx;

pub const APP: &str = "notes";

const SPEC: ServiceSpec = ServiceSpec { name: "mind-serve", label: "笔记·脑", version: env!("CARGO_PKG_VERSION"), default_bind: "127.0.0.1:8797", tab: None };

struct State {
    cfg: ConfigCell<MindConfig>,
    ledger: Ledger,
    store: InkClient,
    clients: vendorcfg::ClientCache,
}

impl State {
    fn cfg(&self) -> MindConfig {
        self.cfg.get()
    }
    /// 配置没变就复用上一次的调用端（连同还活着的 HTTPS 连接），见 `vendorcfg::ClientCache`。
    fn model(&self, cfg: &MindConfig) -> Result<Arc<vendorcfg::ChatClient>, String> {
        // 后端标识用 `usage_key()`（预置 id 或 `custom:<model>`）：写进回答 `backend`，跟用量账本同一个键（NT-3）。
        self.clients.get(cfg, &cfg.usage_key(), Duration::from_secs(cfg.timeout_secs), "未配置 API key（网页「管理 → 模型管理」里粘贴，或环境变量 DASHSCOPE_API_KEY）")
    }
}

fn router(st: &Arc<State>) -> Router {
    Router::new()
        .get("/status", bind(st, |s, _| {
            let (cfg, usage) = (s.cfg(), s.ledger.snapshot());
            Ok(Reply::ok(&serde_json::json!({"config": cfg.public(), "usageByModel": vendorcfg::usage::usage_profile(&cfg, &usage), "usage": usage})))
        }))
        .get("/config", bind(st, |s, _| Ok(Reply::ok(&s.cfg().public()))))
        .put("/config", bind(st, |s, r| {
            let j = r.json()?;
            let next = s.cfg.update(|c| c.apply(&j))?;
            Ok(Reply::ok(&next.public()))
        }))
        .post("/books/{uuid}/entries/{id}/ask", bind(st, |s, r| {
            let (uuid, id) = (r.param("uuid").to_string(), r.param("id").to_string());
            // 先取书：ink-serve 的 404 原样透传、没运行回 503（此前一律当 404，2026-10-10 审计 CORE-1）。
            let book = s.store.book(&uuid)?;
            let e = book.entries.iter().find(|e| e.id == id).ok_or_else(|| ApiError::not_found("没有这条目"))?;
            let cfg = s.cfg();
            // 没配 key 是用户该去设置的事 → 400。
            let model = s.model(&cfg).map_err(ApiError::bad)?;
            let now = rmsvc_core::clock::now_secs();
            let ctx = Ctx { store: &s.store, model: model.as_ref(), cfg: &cfg, ledger: &s.ledger, now };
            let a = worker::ask_entry(&ctx, &uuid, &book.title, e)?;
            // 点「提问」弹出这次调用的消耗（token）——不是账本累计，是这一次调用的实际数字。
            Ok(Reply::ok(&serde_json::json!({"ok": true, "answer": a.text, "promptTokens": a.prompt_tokens, "completionTokens": a.completion_tokens})))
        }))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    // `.migrate()`：老配置文件搬进新形状，不迁移会让真机已存的 key 在升级后凭空消失，见 config.rs 文档；
    // 读→迁移→0600→落盘一次这套启动流程收在 `ConfigCell::load`。
    let cfg = ConfigCell::load(&paths.app_config_dir(APP).join("mind.json"), MindConfig::migrate);
    let st = Arc::new(State { cfg, ledger: Ledger::open(&paths.app_state_dir(APP).join("mind.json")), store: InkClient::new(paths.clone()), clients: Default::default() });
    let router = router(&st);
    let c = st.cfg();
    println!("[mind-serve] 配置 {}；模型 {}（{} @ {}）；key {:?}", st.cfg.path().display(), c.usage_key(), c.model(), c.base_url(), c.key_source());
    service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::{Method, TestRequest};

    fn paths(t: &tempfile::TempDir) -> Paths {
        let h = t.path().to_str().unwrap().to_string();
        Paths::resolve(move |k| (k == "HOME" || k.starts_with("XDG_")).then(|| h.clone()))
    }

    fn state(p: &Paths) -> Arc<State> {
        let cfg = ConfigCell::load(&p.app_config_dir(APP).join("mind.json"), MindConfig::migrate);
        Arc::new(State { cfg, ledger: Ledger::open(&p.app_state_dir(APP).join("mind.json")), store: InkClient::new(p.clone()), clients: Default::default() })
    }

    /// CORE-1：ink-serve 没运行 → 503；ink-serve 回 404 → 404 原样透传（此前一律 404，连"没运行"也是）。
    #[test]
    fn ask_passes_ink_status_through() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        std::fs::create_dir_all(p.app_config_dir(APP)).unwrap();
        let r = router(&state(&p));
        let ask = || TestRequest::new(Method::Post, "/books/b/entries/e/ask").dispatch(&r);
        assert_eq!(ask().status, 503, "ink-serve 没运行");

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let _ = req.respond(tiny_http::Response::from_string(r#"{"ok":false,"message":"没有这本书的条目"}"#).with_status_code(404));
            }
        });
        let info = rmsvc_core::registry::ServiceInfo { name: "ink-serve".into(), port, label: String::new(), version: String::new(), pid: std::process::id(), ui: None };
        let _reg = rmsvc_core::registry::register(&p, &info).unwrap();
        let reply = ask();
        assert_eq!(reply.status, 404);
        assert!(String::from_utf8_lossy(&reply.body).contains("没有这本书的条目"));
    }

    /// 配置字段不合法 → 400（存盘失败的 500 由 `vendorcfg::ConfigCell` 的测试覆盖）。
    #[test]
    fn put_config_rejects_unknown_preset_with_400() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        std::fs::create_dir_all(p.app_config_dir(APP)).unwrap();
        let r = router(&state(&p));
        assert_eq!(TestRequest::new(Method::Put, "/config").json(&serde_json::json!({"preset": "no-such"})).dispatch(&r).status, 400);
        assert_eq!(TestRequest::new(Method::Put, "/config").json(&serde_json::json!({"timeoutSecs": 30})).dispatch(&r).status, 200);
    }
}
