//! transcribe-serve —— 笔记·转写（loopback 8796）。订阅 ink-serve 事件：条目库有新手写 → 取裁图 → 视觉模型 → 草稿写回
//! （经 ink-serve 的 HTTP，条目库唯一写者仍是它；已校对 `text` 永不被覆盖）。唯一出网的笔记服务：设备自己的 WiFi 直连。
//! 路由（经网关前缀 `/api/transcribe`）：`GET /status` · `GET /config` · `PUT /config`（`apiKey` 只写不读）·
//! `POST /books/{uuid}/entries/{id}`（强制转写一条，网页「重新转写」）· `GET /events`。
//! （`POST /run`、`POST /retry` 网页早已不调，2026-10-10 删。）
mod backend;
mod config;
mod ink;
mod ledger;
mod prompt;
mod worker;

use config::TranscribeConfig;
use notesvc::InkClient;
use ledger::Ledger;
use rmsvc_core::events::{follow, Event, EventBus};
use rmsvc_core::http::{bind, ApiError, Reply, Router, ServeOpts};
use rmsvc_core::paths::Paths;
use rmsvc_core::service::{self, ServiceSpec};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use vendorcfg::{ConfigCell, VendorConfig};
use worker::{Ctx, Failures, Target};

pub const APP: &str = "notes";

const SPEC: ServiceSpec = ServiceSpec { name: "transcribe-serve", label: "笔记·转写", version: env!("CARGO_PKG_VERSION"), default_bind: "127.0.0.1:8796", tab: None };
/// 事件到跑之间的防抖：合上书 ink-serve 会连发几条。
const DEBOUNCE: Duration = Duration::from_secs(3);

/// 本服务发的事件（网关按 `area` 路由到笔记页；线上值不能改，前端按这两个字面量认）。
const EVENT_AREA: &str = "notes";
/// 转写配置/一轮结果变了（网页据此刷新转写状态）。
const EVENT_TRANSCRIBE: &str = "transcribe";
/// 订阅的 ink-serve 事件：条目库变了（与 ink-serve 的 `EVENT_ENTRIES` 同值，两边一起改）。
const INK_EVENT_ENTRIES: (&str, &str) = ("notes", "entries");

struct State {
    paths: Paths,
    cfg: ConfigCell<TranscribeConfig>,
    ledger: Ledger,
    failures: Failures,
    bus: Arc<EventBus>,
    store: InkClient,
    /// 同一时刻只跑一轮（自动与手动互斥）。
    run_lock: Mutex<()>,
    trigger: SyncSender<()>,
    clients: vendorcfg::ClientCache,
}

impl State {
    fn cfg(&self) -> TranscribeConfig {
        self.cfg.get()
    }
    /// 配置没变就复用上一轮的调用端（连同还活着的 HTTPS 连接），见 `vendorcfg::ClientCache`。
    fn vision(&self, cfg: &TranscribeConfig) -> Result<Arc<vendorcfg::ChatClient>, String> {
        // 后端标识用 `usage_key()`（预置 id 或 `custom:<model>`）：写进草稿 `backend`，跟用量账本同一个键（NT-3）。
        self.clients.get(cfg, &cfg.usage_key(), Duration::from_secs(cfg.timeout_secs), "未配置 API key（网页「管理 → 模型管理」里粘贴，或环境变量 DASHSCOPE_API_KEY）")
    }
    /// 跑一轮（阻塞拿锁）。没 key → 直接报告不出网。
    fn run(&self, only: Option<Target<'_>>) -> ledger::RunReport {
        let _g = rmsvc_core::sync::lock(&self.run_lock);
        // 一轮里逐条记账 + 一轮报告合成一次写盘（取舍见 `vendorcfg::Ledger::hold`）。
        let _hold = self.ledger.hold();
        let cfg = self.cfg();
        let now = rmsvc_core::clock::now_secs();
        let report = match self.vision(&cfg) {
            Ok(v) => worker::run_once(&Ctx { store: &self.store, vision: v.as_ref(), cfg: &cfg, ledger: &self.ledger, failures: &self.failures, now }, only),
            Err(e) => ledger::RunReport { at: now, note: e, ..Default::default() },
        };
        let changed = ledger::record_if_new(&self.ledger, &report);
        if report.done > 0 || report.failed > 0 {
            println!("[transcribe-serve] 一轮：扫 {} 成 {} 败 {} 跳 {} 余 {} {}", report.scanned, report.done, report.failed, report.skipped, report.left, report.note);
        }
        if changed {
            self.bus.publish(EVENT_AREA, EVENT_TRANSCRIBE);
        }
        report
    }
    fn kick(&self) {
        match self.trigger.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => {}
            Err(TrySendError::Disconnected(())) => eprintln!("[transcribe-serve] 工作线程没了"),
        }
    }
}

/// 订阅 ink-serve `/events`：条目变了就踢一下工作线程（`rmsvc_core::events::follow`：注册表 inotify 唤醒、断线退避、
/// loopback 长心跳，与网关汇聚同一套实现）。
fn watch_ink(st: Arc<State>) {
    follow(&st.paths.clone(), "ink-serve", |json| {
        if Event::parse(json).is_some_and(|e| e.is(INK_EVENT_ENTRIES.0, INK_EVENT_ENTRIES.1)) && st.cfg().auto {
            st.kick();
        }
    });
}

/// 工作线程：收到踢 → 防抖 → 跑一轮。一轮里 panic 兜住只丢这一轮：不兜的话工作线程就此退出，
/// 之后自动转写再也不跑，而 HTTP 照常应答、看不出异常（`kick` 只会在下一次踢时打一句"工作线程没了"）。
fn work_loop(st: Arc<State>, rx: Receiver<()>) {
    while rx.recv().is_ok() {
        std::thread::sleep(DEBOUNCE);
        while rx.try_recv().is_ok() {}
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| st.run(None))).is_err() {
            eprintln!("[transcribe-serve] 一轮转写 panic（已兜住，下次再踢照常跑）");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind_addr = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    // `.migrate()`：老配置文件（重做模型预置表之前，2026-09-08 上午之前落盘的，单一 model/baseUrl/apiKey
    // 三件套）搬进新形状——不迁移的话真机已经保存的 key 会在升级后凭空消失，见 config.rs 模块文档；
    // 读→迁移→0600→落盘一次这套启动流程收在 `ConfigCell::load`。
    let cfg = ConfigCell::load(&paths.app_config_dir(APP).join("transcribe.json"), TranscribeConfig::migrate);
    let (tx, rx) = sync_channel::<()>(2);
    let st = Arc::new(State {
        paths: paths.clone(),
        cfg,
        ledger: Ledger::open(&paths.app_state_dir(APP).join("transcribe.json")),
        failures: Failures::default(),
        bus: Arc::new(EventBus::new()),
        store: InkClient::new(paths.clone()),
        run_lock: Mutex::new(()),
        trigger: tx,
        clients: Default::default(),
    });
    {
        let s = st.clone();
        std::thread::spawn(move || work_loop(s, rx));
        let s = st.clone();
        std::thread::spawn(move || watch_ink(s));
        // 追平：启动后踢一次（ink-serve 可能已积了待转写；没 key 则一轮空跑只记 note）
        if st.cfg().auto {
            st.kick();
        }
    }
    let router = Router::new()
        .get("/events", bind(&st, |s, r| Ok(s.bus.sse_reply_for(r))))
        .get("/status", bind(&st, |s, _| {
            let (ink, pending) = match s.store.list_books() {
                Ok(b) => (true, b.iter().map(|x| x.pending).sum::<usize>()),
                Err(_) => (false, 0),
            };
            let (cfg, usage) = (s.cfg(), s.ledger.snapshot());
            Ok(Reply::ok(&serde_json::json!({"config": cfg.public(), "usageByModel": vendorcfg::usage::usage_profile(&cfg, &usage), "usage": usage, "failures": s.failures.list(), "inkReachable": ink, "pending": pending})))
        }))
        .get("/config", bind(&st, |s, _| Ok(Reply::ok(&s.cfg().public()))))
        .put("/config", bind(&st, |s, r| {
            let j = r.json()?;
            let next = s.cfg.update(|c| c.apply(&j))?;
            let has_key = next.key().is_some();
            s.bus.publish(EVENT_AREA, EVENT_TRANSCRIBE);
            if has_key && next.auto {
                s.kick();
            }
            Ok(Reply::ok(&next.public()))
        }))
        .post("/books/{uuid}/entries/{id}", bind(&st, |s, r| {
            let (uuid, id) = (r.param("uuid").to_string(), r.param("id").to_string());
            let rep = s.run(Some(Target { uuid: &uuid, id: &id }));
            if rep.done == 1 {
                // 点「重转」弹出这次调用的消耗（token）——不是账本累计，是这一次调用的实际数字。
                Ok(Reply::ok(&serde_json::json!({"ok": true, "promptTokens": rep.prompt_tokens, "completionTokens": rep.completion_tokens})))
            } else {
                Err(ApiError::bad(if rep.note.is_empty() { "没有这条目或它没有手写".to_string() } else { rep.note }))
            }
        }));
    let c = st.cfg();
    println!("[transcribe-serve] 配置 {}；模型 {}（{} @ {}）；key {:?}", st.cfg.path().display(), c.usage_key(), c.model(), c.base_url(), c.key_source());
    service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default())
}
