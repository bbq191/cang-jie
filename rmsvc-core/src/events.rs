//! 事件总线 + SSE（Server-Sent Events）流：服务在**变更发生处**发事件（上传/落库/删除/轮换/inbox 追平），
//! 网关汇聚后推给浏览器，网页不轮询也能即时刷新（2026-09-06 用户："不喜欢轮询，要事件通知"）。
//! - `EventBus`：进程内广播，订阅者各持一个有界通道（满了丢事件——事件只是"该刷新了"的信号，不携带状态）；
//! - `SseStream`：把通道包成 `Read`，交给 HTTP 层（`http::respond_stream`：接管 socket、一帧一 flush，绕开 tiny_http 的 8 KB chunked 缓冲）；20 s 无事件发一行注释心跳，
//!   浏览器 `EventSource` 断线（WiFi 掉 / 设备休眠醒来）会自动重连；
//! - 事件格式一行 JSON：`{"area":"books","kind":"staging","at":<unix秒>}`，网关转发时补 `"svc"`。
use crate::http::Reply;
use crate::paths::Paths;
use crate::registry;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 心跳间隔（小于常见反向代理/浏览器的空闲超时）。**浏览器流的缺省值**；loopback 上的服务间订阅
/// （网关 → 各服务、transcribe → ink）没有反向代理、读端无超时，用 `?ka=<秒>` 要求更长的心跳，见 [`follow`]。
pub const KEEPALIVE: Duration = Duration::from_secs(20);
/// `?ka=` 允许的范围（秒）：下限防止被要求成忙等，上限保证连接死了最迟这个间隔内能发现（写失败）。
const KEEPALIVE_MIN_SECS: u64 = 5;
const KEEPALIVE_MAX_SECS: u64 = 600;
/// 服务间订阅（[`follow`]）请求的心跳：两端都在 loopback、读端不设超时，心跳只用来"发现对端进程死了没关连接"
/// （对端进程一死内核会关 socket，读端立即 EOF），不需要 20 秒一次；2 分钟一次把空闲唤醒降到 1/6。
pub const FOLLOW_KEEPALIVE_SECS: u64 = 120;
const QUEUE: usize = 64;

/// 解析 `?ka=<秒>`：缺省/非法 → `None`（用缺省 20 秒）；合法值夹到 [5, 600] 秒。
pub fn parse_keepalive_param(v: Option<&str>) -> Option<Duration> {
    let secs: u64 = v?.trim().parse().ok()?;
    Some(Duration::from_secs(secs.clamp(KEEPALIVE_MIN_SECS, KEEPALIVE_MAX_SECS)))
}

/// 请求 `?ka=` 定的心跳（缺省/非法 → [`KEEPALIVE`]）。
fn keepalive_of(req: &crate::http::Request<'_>) -> Duration {
    parse_keepalive_param(req.q("ka")).unwrap_or(KEEPALIVE)
}

#[derive(Default)]
pub struct EventBus {
    subs: Mutex<Vec<SyncSender<String>>>,
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus::default()
    }

    /// 订阅（缺省心跳 [`KEEPALIVE`]）：返回 SSE 可读流（掉线后由 HTTP 层 drop，总线在下次 publish 时清掉死订阅者）。
    pub fn subscribe(&self) -> SseStream {
        self.subscribe_with(KEEPALIVE)
    }

    /// 同 [`subscribe`]，显式指定心跳间隔。
    pub fn subscribe_with(&self, keepalive: Duration) -> SseStream {
        let (tx, rx) = sync_channel::<String>(QUEUE);
        crate::sync::lock(&self.subs).push(tx);
        SseStream { rx, pending: Vec::new(), keepalive }
    }

    /// 发一条事件（`area`=UI 区域：books/fonts/wallpapers/manage/notes 等；`kind`=细分）。
    pub fn publish(&self, area: &str, kind: &str) {
        let at = crate::clock::now_secs();
        self.publish_raw(&serde_json::json!({"area": area, "kind": kind, "at": at}).to_string());
    }

    /// 同 [`Self::publish`]，再并进 `extra` 里的字段（对象以外的 `extra` 忽略；`area`/`kind`/`at` 不被覆盖）。
    /// 渲染自检这类要带 uuid/状态的事件用，不必自己拼 `{"area","kind","at"}` 再 `publish_raw`。
    pub fn publish_with(&self, area: &str, kind: &str, extra: serde_json::Value) {
        let mut v = serde_json::json!({"area": area, "kind": kind, "at": crate::clock::now_secs()});
        if let (Some(dst), serde_json::Value::Object(src)) = (v.as_object_mut(), extra) {
            for (k, val) in src {
                dst.entry(k).or_insert(val);
            }
        }
        self.publish_raw(&v.to_string());
    }

    /// 转发已成型的 JSON 行（网关汇聚用）。
    pub fn publish_raw(&self, json_line: &str) {
        let frame = format!("data: {json_line}\n\n");
        let mut subs = crate::sync::lock(&self.subs);
        subs.retain(|tx| match tx.try_send(frame.clone()) {
            Ok(()) | Err(TrySendError::Full(_)) => true,
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    #[cfg(test)]
    pub fn subscribers(&self) -> usize {
        crate::sync::lock(&self.subs).len()
    }

    /// `GET /events` 的回执：`text/event-stream` 流式响应，心跳读 `req` 的 `?ka=`（缺省/非法 → 20 秒，合法值夹到 [5, 600] 秒）。
    /// 2026-10-10 前还有个不带参数的 `sse_reply()`，心跳由 `Router::dispatch` 每个请求塞进线程局部再隐式读出（http ↔ events
    /// 互相引用，审计 CORE-5）；各服务的 `/events` 路由都已改传请求，线程局部与分发钩子一并删掉。
    pub fn sse_reply_for(&self, req: &crate::http::Request<'_>) -> Reply {
        Reply::stream("text/event-stream; charset=utf-8", Box::new(self.subscribe_with(keepalive_of(req)))).with_header("Cache-Control", "no-cache").with_header("X-Accel-Buffering", "no")
    }
}

/// 把事件通道包成阻塞 `Read`：无事件时等 `keepalive` 后吐一行注释心跳。
pub struct SseStream {
    rx: Receiver<String>,
    pending: Vec<u8>,
    keepalive: Duration,
}

impl SseStream {
    pub fn keepalive(&self) -> Duration {
        self.keepalive
    }
}

impl Read for SseStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            match self.rx.recv_timeout(self.keepalive) {
                Ok(frame) => self.pending = frame.into_bytes(),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => self.pending = b": ping\n\n".to_vec(),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = self.pending.len().min(buf.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

/// 代数计数 + 条件变量的"有变化就醒"唤醒器：变化方 [`Wake::bump`]，等待方记下 [`Wake::generation`] 再 [`Wake::wait_change`]，
/// 记下之后发生的变化不会漏（代数已经不同，立即返回）；多次变化合并成一次醒来。注册表目录监听（[`registry_wake`]）和
/// 网关的"book-serve 有新事件"共用这一份（此前两边各写一份，逐行相同）。
#[derive(Default)]
pub struct Wake {
    generation: Mutex<u64>,
    cv: Condvar,
}

impl Wake {
    pub fn generation(&self) -> u64 {
        *crate::sync::lock(&self.generation)
    }
    pub fn bump(&self) {
        *crate::sync::lock(&self.generation) += 1;
        self.cv.notify_all();
    }
    /// 等到代数不再是 `seen` 或 `timeout` 到，返回当前代数。
    pub fn wait_change(&self, seen: u64, timeout: Duration) -> u64 {
        let g = crate::sync::lock(&self.generation);
        let (g, _) = self.cv.wait_timeout_while(g, timeout, |g| *g == seen).unwrap_or_else(|e| e.into_inner());
        *g
    }
}

/// 注册表目录变化唤醒器：每个 `services_dir` 一条 inotify 监听线程（进程内共享，防抖 300ms），变化时 [`Wake::bump`]。
/// [`follow`] 靠它"等服务出现/重启"而不是每 3 秒轮询注册表（此前网关 8 条订阅线程各自 3 秒一次读注册表目录，
/// 服务没装/没起时也不停）；网关用它发"服务注册/注销"事件、批量队列恢复时等 book-serve 起来，不另起监听线程。
/// inotify 初始化失败时监听线程直接返回，等待者只剩各自的超时兜底。
pub fn registry_wake(paths: &Paths) -> Arc<Wake> {
    static M: OnceLock<Mutex<HashMap<PathBuf, Arc<Wake>>>> = OnceLock::new();
    let dir = paths.services_dir();
    let mut m = crate::sync::lock(M.get_or_init(|| Mutex::new(HashMap::new())));
    if let Some(w) = m.get(&dir) {
        return w.clone();
    }
    let w = Arc::new(Wake::default());
    let _ = std::fs::create_dir_all(&dir);
    let (w2, d2) = (w.clone(), dir.clone());
    // 监听线程阻塞在 inotify 上（空闲零唤醒）。
    let _ = std::thread::Builder::new().name("reg-watch".into()).spawn(move || {
        crate::fswatch::watch_debounced(&d2, Duration::from_millis(300), |_| w2.bump());
    });
    m.insert(dir, w.clone());
    w
}

/// [`follow`] 的各种等待时长（缺省值见 `Default`；测试用毫秒级）。
#[derive(Clone, Debug)]
pub struct FollowTiming {
    /// 连接失败/流断开后的首次重试等待，之后翻倍到 `retry_max`。
    pub retry_min: Duration,
    pub retry_max: Duration,
    /// 对方服务没有 `/events`（404）时的等待：它不会凭空长出事件流，只有注册表变化（升级重启）才值得再试，
    /// 超时只是兜底。此前网关对 mind-serve 每 3 秒白打一个 404。
    pub no_events_wait: Duration,
    /// 服务没注册时的兜底轮询。正常靠注册表 inotify 唤醒（测试 `follow_waits_for_registration…` 证明是 inotify 而非超时在
    /// 起作用），只有 inotify 初始化失败才靠它；所以取 5 分钟——此前 60 秒，网关里每个"没装"的服务一条线程、每分钟白醒一次。
    pub not_registered_wait: Duration,
    /// 一条流撑过这么久才算"健康"，重置退避。
    pub healthy_after: Duration,
}

impl Default for FollowTiming {
    fn default() -> Self {
        FollowTiming {
            retry_min: Duration::from_secs(3),
            retry_max: Duration::from_secs(60),
            no_events_wait: Duration::from_secs(10 * 60),
            not_registered_wait: Duration::from_secs(300),
            healthy_after: Duration::from_secs(10),
        }
    }
}

/// 订阅另一个服务的 `GET /events`，把每条事件的 JSON 行交给 `on_json`（**阻塞，永不返回**，放线程里）。
/// 收编此前网关 `events::subscribe_loop` 与 `transcribe-serve::watch_ink` 各写一份的"查注册表 → 连流 → 逐行解析 →
/// 断线重连"循环，并修掉它们的耗电问题：
/// - 服务没起：等注册表目录的 inotify 事件（[`registry_wake`]），不再 3 秒轮询；
/// - 服务没有 `/events`（404）：长等待（见 [`FollowTiming::no_events_wait`]）；
/// - 连接失败/断流：指数退避（3s → 60s），流撑过 10 秒才重置；
/// - 请求带 `?ka=120`：loopback 上心跳 2 分钟一次（浏览器流仍是 20 秒）。
pub fn follow(paths: &Paths, svc: &str, mut on_json: impl FnMut(&str)) {
    follow_with(paths, svc, &FollowTiming::default(), &AtomicBool::new(false), &mut on_json);
}

/// [`follow`] 的可控版本：`stop` 置位后在下一次唤醒（事件/超时）时返回——生产不用，给测试关停线程。
pub fn follow_with(paths: &Paths, svc: &str, timing: &FollowTiming, stop: &AtomicBool, on_json: &mut dyn FnMut(&str)) {
    let wake = registry_wake(paths);
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).build(); // 读不设超时：靠对端关连接/心跳发现断线
    let mut backoff = timing.retry_min;
    while !stop.load(Ordering::Relaxed) {
        // 先取代数再查注册表：查完到开始等待之间发生的变化不会被漏掉（代数已变，wait_change 立即返回）。
        let seen = wake.generation();
        let Some(info) = registry::find(paths, svc) else {
            wake.wait_change(seen, timing.not_registered_wait);
            continue;
        };
        match agent.get(&format!("{}/events?ka={}", info.base_url(), FOLLOW_KEEPALIVE_SECS)).call() {
            Ok(resp) => {
                let started = Instant::now();
                for line in BufReader::new(resp.into_reader()).lines().map_while(Result::ok) {
                    if let Some(json) = parse_sse_line(&line) {
                        on_json(json);
                    }
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                }
                if started.elapsed() >= timing.healthy_after {
                    backoff = timing.retry_min;
                }
                wake.wait_change(seen, backoff); // 对端重启会重新注册 → 立即被唤醒重连
                backoff = (backoff * 2).min(timing.retry_max);
            }
            Err(ureq::Error::Status(404, _)) => {
                wake.wait_change(seen, timing.no_events_wait);
            }
            Err(_) => {
                wake.wait_change(seen, backoff);
                backoff = (backoff * 2).min(timing.retry_max);
            }
        }
    }
}

/// 一条事件的公共部分（`{"area","kind",...}`）；[`follow`] 的订阅方据此判断"是不是我关心的那类变化"。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Event {
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub kind: String,
}

impl Event {
    /// 解析一行事件 JSON；不是 JSON 对象 → `None`。
    pub fn parse(json: &str) -> Option<Event> {
        serde_json::from_str(json).ok()
    }
    pub fn is(&self, area: &str, kind: &str) -> bool {
        self.area == area && self.kind == kind
    }
}

/// 解析一行 SSE 文本：`data: {...}` → JSON 行；心跳/空行 → None。
pub fn parse_sse_line(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    line.strip_prefix("data:").map(str::trim).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_returns_on_bump_and_times_out_otherwise() {
        let w = Arc::new(Wake::default());
        let seen = w.generation();
        // 没人 bump：超时返回，代数不变
        let t = Instant::now();
        assert_eq!(w.wait_change(seen, Duration::from_millis(60)), seen);
        assert!(t.elapsed() >= Duration::from_millis(60));
        // 另一线程 bump：远早于超时就返回
        let w2 = w.clone();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            w2.bump();
        });
        let t = Instant::now();
        assert_eq!(w.wait_change(seen, Duration::from_secs(10)), seen + 1);
        assert!(t.elapsed() < Duration::from_secs(5), "bump 应立即唤醒等待者");
        h.join().unwrap();
        // 已经变过的代数：wait 立即返回（不会漏掉 wait 之前发生的事件）
        assert_eq!(w.wait_change(seen, Duration::from_secs(10)), seen + 1);
    }

    #[test]
    fn publish_reaches_subscribers_and_drops_dead_ones() {
        let bus = EventBus::new();
        let mut a = bus.subscribe();
        let b = bus.subscribe();
        assert_eq!(bus.subscribers(), 2);
        bus.publish("books", "staging");
        let mut buf = [0u8; 2048];
        let n = a.read(&mut buf).unwrap();
        let s = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(s.starts_with("data: {") && s.ends_with("}\n\n") && s.contains(r#""area":"books""#) && s.contains(r#""kind":"staging""#), "{s}");
        drop(b);
        bus.publish("fonts", "fonts");
        assert_eq!(bus.subscribers(), 1, "死订阅者在下次 publish 时清掉");
        assert_eq!(parse_sse_line("data: {\"a\":1}\n"), Some("{\"a\":1}"));
        assert_eq!(parse_sse_line(": ping\n"), None);
        assert_eq!(parse_sse_line(""), None);
    }

    #[test]
    fn publish_with_merges_extra_and_event_parses() {
        let bus = EventBus::new();
        let mut s = bus.subscribe();
        bus.publish_with("books", "render", serde_json::json!({"uuid": "u1", "area": "evil"}));
        let mut buf = [0u8; 2048];
        let n = s.read(&mut buf).unwrap();
        let line = parse_sse_line(std::str::from_utf8(&buf[..n]).unwrap().trim_end()).unwrap().to_string();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!((v["area"].as_str(), v["kind"].as_str(), v["uuid"].as_str()), (Some("books"), Some("render"), Some("u1")), "extra 不覆盖公共字段");
        let e = Event::parse(&line).unwrap();
        assert!(e.is("books", "render") && !e.is("notes", "render"));
        assert_eq!(Event::parse("not json"), None);
    }

    #[test]
    fn keepalive_param_parses_and_clamps() {
        assert_eq!(parse_keepalive_param(None), None);
        assert_eq!(parse_keepalive_param(Some("abc")), None, "非法值回缺省");
        assert_eq!(parse_keepalive_param(Some("120")), Some(Duration::from_secs(120)));
        assert_eq!(parse_keepalive_param(Some("1")), Some(Duration::from_secs(5)), "夹下限，防忙等");
        assert_eq!(parse_keepalive_param(Some("99999")), Some(Duration::from_secs(600)), "夹上限");
    }

    /// 心跳只看传进来的请求（在路由外直接调用也对）。
    #[test]
    fn sse_reply_for_reads_ka_from_given_request() {
        use crate::http::{Method, TestRequest};
        let ka = |q: Option<&str>| {
            let t = TestRequest::new(Method::Get, "/events");
            let mut t = if let Some(v) = q { t.query("ka", v) } else { t };
            t.with(|r| keepalive_of(r))
        };
        assert_eq!((ka(None), ka(Some("120")), ka(Some("1")), ka(Some("x"))), (KEEPALIVE, Duration::from_secs(120), Duration::from_secs(5), KEEPALIVE));
        let bus = EventBus::new();
        let rep = TestRequest::new(Method::Get, "/events").query("ka", "120").with(|r| bus.sse_reply_for(r));
        assert_eq!(rep.content_type, "text/event-stream; charset=utf-8");
        assert!(rep.stream.is_some() && rep.headers.iter().any(|(k, v)| k == "Cache-Control" && v == "no-cache"));
        assert_eq!(bus.subscribers(), 1);
    }

    /// 起一个假服务（`/events` 可选）并注册进临时注册表，返回其命中计数。
    fn fake_service(p: &Paths, name: &str, with_events: bool, bus: Option<Arc<EventBus>>) -> (registry::Registration, Arc<std::sync::atomic::AtomicUsize>) {
        use crate::http::{ApiError, Router};
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h2 = hits.clone();
        let mut router = Router::new();
        if with_events {
            let bus = bus.unwrap();
            router = router.get("/events", move |r| Ok(bus.sse_reply_for(r)));
        } else {
            router = router.get("/{x}", move |_| {
                h2.fetch_add(1, Ordering::SeqCst);
                Err(ApiError::not_found("no"))
            });
        }
        let addr = format!("127.0.0.1:{port}");
        std::thread::spawn(move || {
            let _ = crate::http::serve(&addr, router);
        });
        // 等端口起来
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let info = registry::ServiceInfo { name: name.into(), port, label: name.into(), version: "0".into(), pid: std::process::id(), ui: None };
        (registry::register(p, &info).unwrap(), hits)
    }

    fn tmp_paths(t: &tempfile::TempDir) -> Paths {
        let h = t.path().to_str().unwrap().to_string();
        Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None })
    }

    #[test]
    fn follow_waits_for_registration_then_streams_events_and_reconnect_wakes_on_registry() {
        let t = tempfile::tempdir().unwrap();
        let p = tmp_paths(&t);
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let timing = FollowTiming { retry_min: Duration::from_millis(50), retry_max: Duration::from_millis(200), no_events_wait: Duration::from_secs(30), not_registered_wait: Duration::from_secs(30), healthy_after: Duration::from_secs(5) };
        let (p2, stop2) = (p.clone(), stop.clone());
        let h = std::thread::spawn(move || follow_with(&p2, "fake", &timing, &stop2, &mut |j| tx.send(j.to_string()).unwrap()));
        std::thread::sleep(Duration::from_millis(300)); // follow 已在等注册；不注册就不该有任何动静
        let bus = Arc::new(EventBus::new());
        let (_reg, _) = fake_service(&p, "fake", true, Some(bus.clone()));
        // 注册后靠 inotify 唤醒（not_registered_wait 是 30 秒，超时兜底赶不上），应在 3 秒内连上
        let mut got = None;
        for _ in 0..30 {
            bus.publish("books", "staging");
            if let Ok(j) = rx.recv_timeout(Duration::from_millis(100)) {
                got = Some(j);
                break;
            }
        }
        let j = got.expect("注册后应被 inotify 唤醒并连上事件流");
        assert!(j.contains(r#""area":"books""#), "{j}");
        stop.store(true, Ordering::Relaxed);
        bus.publish("books", "staging"); // 让读循环醒来看到 stop
        h.join().unwrap();
    }

    #[test]
    fn follow_backs_off_long_when_service_has_no_events_endpoint() {
        let t = tempfile::tempdir().unwrap();
        let p = tmp_paths(&t);
        let (_reg, hits) = fake_service(&p, "quiet", false, None);
        let stop = Arc::new(AtomicBool::new(false));
        let timing = FollowTiming { retry_min: Duration::from_millis(50), retry_max: Duration::from_millis(100), no_events_wait: Duration::from_secs(30), not_registered_wait: Duration::from_secs(30), healthy_after: Duration::from_secs(5) };
        let (p2, stop2) = (p.clone(), stop.clone());
        std::thread::spawn(move || follow_with(&p2, "quiet", &timing, &stop2, &mut |_| {}));
        std::thread::sleep(Duration::from_millis(1500));
        stop.store(true, Ordering::Relaxed);
        // 3 秒轮询时 1.5 秒内也许只有 1 次；这里用 50ms 级退避来证明"404 不走短退避"：若走短退避会命中几十次
        assert_eq!(hits.load(Ordering::SeqCst), 1, "404 后应长等待，而不是按短退避反复重试");
    }

    #[test]
    fn stream_reads_in_chunks_and_ends_when_bus_dropped() {
        let bus = EventBus::new();
        let mut s = bus.subscribe();
        bus.publish_raw("{\"x\":1}");
        let mut small = [0u8; 4];
        assert_eq!(s.read(&mut small).unwrap(), 4);
        assert_eq!(&small, b"data");
        let mut rest = [0u8; 64];
        let n = s.read(&mut rest).unwrap();
        assert_eq!(std::str::from_utf8(&rest[..n]).unwrap(), ": {\"x\":1}\n\n");
        drop(bus);
        assert_eq!(s.read(&mut rest).unwrap(), 0, "总线没了 → EOF");
    }
}
