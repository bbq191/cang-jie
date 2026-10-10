//! 服务器：tiny_http 适配（每请求一线程、并发名额、守卫、SSE 裸 socket 流式回执、TLS）。
use super::router::{parse_query, Router};
use super::{header_of, Body, Method, Reply, Request, REMOTE_IP_HEADER};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

/// 同时在处理的请求数缺省上限（含一直挂着的 SSE 流，每个请求占一条线程）。
/// 取值依据：设备 2 核、约 2GB 内存；合法并发上限 ≈ 浏览器每源 6 条连接 × 几个标签页 + 网关到各服务的
/// 8 条 loopback SSE 订阅 ≈ 30 上下；每条线程栈虚拟 2MB、常驻只有几十 KB，64 条最坏也就几 MB 常驻，
/// 既给合法用法留一倍以上余量，又让局域网内恶意/失控的大量连接（每个请求一条线程、登录还要做 60 万轮
/// PBKDF2）吃不掉整台设备。超限的请求直接 503 + `Retry-After`，不再 spawn 线程。
pub const DEFAULT_MAX_CONCURRENT: usize = 64;

/// 连接读空闲超时：服务端等着读时，这么久没收到一个字节就断开这条连接。上游 tiny_http 0.12 不设任何
/// 超时——慢客户端、只发半个请求头/半个 TLS 握手的 slowloris、手机休眠后留下的半开 keep-alive 连接，
/// 都会永久占住一条连接线程（网关直面局域网，天天用会慢慢攒）。这是"空闲"超时不是"总时长"超时：
/// 大文件上传只要一直有字节在流就不受影响；处理函数自己慢（长轮询、排队）不算——那时服务端没在读。
/// 靠 `vendor/tiny_http` 的补丁对每条 accept 出来的连接设 `read_timeout`（2026-09-24 第三轮审计）。
pub const READ_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// 服务选项：TLS（PEM）与请求守卫（登录/密码策略由服务自己定义，HTTP 层只负责"先问守卫再分发"）。
pub struct ServeOpts {
    pub tls: Option<crate::tls::TlsPem>,
    pub guard: Option<Guard>,
    /// 并发请求上限；`None`=不限。缺省 [`DEFAULT_MAX_CONCURRENT`]。
    pub max_concurrent: Option<usize>,
}

impl Default for ServeOpts {
    fn default() -> Self {
        ServeOpts { tls: None, guard: None, max_concurrent: Some(DEFAULT_MAX_CONCURRENT) }
    }
}

/// 并发名额：`try_acquire` 成功后随请求线程存活，Drop 归还（线程 panic 展开时同样归还）。
struct Permit(Arc<std::sync::atomic::AtomicUsize>);

impl Permit {
    fn try_acquire(counter: &Arc<std::sync::atomic::AtomicUsize>, max: Option<usize>) -> Option<Permit> {
        use std::sync::atomic::Ordering;
        let prev = counter.fetch_add(1, Ordering::AcqRel);
        if max.is_some_and(|m| prev >= m) {
            counter.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Permit(counter.clone()))
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// 守卫检查函数：`None`=放行，`Some(reply)`=拦下并直接回这个应答。
pub type GuardFn = Arc<dyn Fn(&GuardRequest) -> Option<Reply> + Send + Sync>;

/// 守卫：看到请求（方法/路径/头）后返回 `None`=放行，`Some(reply)`=拦下并直接回这个应答。
pub struct Guard {
    pub check: GuardFn,
}

pub struct GuardRequest {
    pub method: Method,
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// TCP 对端 IP（tiny_http `remote_addr()`；HTTPS 下取自 rustls 底下的 TcpStream，同样可靠）。
    /// 登录失败限速按它分桶；取不到时为 `None`（实际只在极端情况下发生，调用方自行归一个桶）。
    pub remote: Option<std::net::IpAddr>,
}
impl GuardRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        header_of(&self.headers, name)
    }
}

const KEPT_HEADERS: &[&str] = &["Cookie", "Authorization", "Accept", "Host", "X-Forwarded-Proto", "User-Agent"];

/// 回执的 `Content-Type` + 自定义头 → tiny_http 头（不合法的头名/值跳过）。三种回执体共用这一处（此前逐行重复三遍）。
fn tiny_headers(content_type: &str, extra: Vec<(String, String)>) -> Vec<tiny_http::Header> {
    std::iter::once(("Content-Type".to_string(), content_type.to_string()))
        .chain(extra)
        .filter_map(|(k, v)| tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).ok())
        .collect()
}

/// 发一个回执，按 [`Body`] 的形态分派：
/// - `Bytes`：按长度一次发完；
/// - `Sized`：交给 tiny_http 按定长边读边发，发完这条响应就结束。不能走 SSE 那条路：那是"升级成裸 socket、读到连接
///   关闭为止"，reader 读完后连接并不会被关掉，客户端一直等（2026-09-24 真机：母版库原件下载头发出后永远收不完）；
/// - `EventStream`：见 [`respond_event_stream`]。
fn respond(req: tiny_http::Request, reply: Reply) {
    let Reply { status, content_type, headers, body } = reply;
    let code = tiny_http::StatusCode(status);
    let headers = tiny_headers(&content_type, headers);
    match body {
        Body::Bytes(b) => {
            let len = b.len();
            let _ = req.respond(tiny_http::Response::new(code, headers, std::io::Cursor::new(b), Some(len), None));
        }
        Body::Sized { reader, len } => {
            // 阈值调到最大：tiny_http 缺省超过 32 KB 就改 chunked、丢掉 Content-Length，浏览器便显示不了下载进度。
            // 长度超过 usize（32 位平台上的 4GB+）不会出现在这台设备上；真出现就退回 chunked，不截断。
            let len = usize::try_from(len).ok();
            let _ = req.respond(tiny_http::Response::new(code, headers, reader, len, None).with_chunked_threshold(usize::MAX));
        }
        Body::EventStream(reader) => respond_event_stream(req, code, headers, reader),
    }
}

/// 事件流回执（SSE）：**不能**走 `respond`——tiny_http 的 chunked 编码器（chunked_transfer::Encoder）攒满 8 KB 才发、
/// 外面还套一层 1 KB BufWriter，小帧永远滞留（真机 curl 30 s 零字节）。改用 `Request::upgrade` 拿到裸 socket：
/// 先发一个只有头的 200（Content-Type: text/event-stream，无长度、`Connection: upgrade`——浏览器/curl 对 200 忽略它，
/// 按"读到连接关闭"处理），然后从 reader 读一帧写一帧、每帧 flush；客户端断开 → 写失败 → 退出，socket 随之关闭。
fn respond_event_stream(req: tiny_http::Request, code: tiny_http::StatusCode, headers: Vec<tiny_http::Header>, mut reader: Box<dyn Read + Send>) {
    let resp = headers.into_iter().fold(tiny_http::Response::empty(code), |r, h| r.with_header(h));
    let mut sock = req.upgrade("sse", resp);
    let mut buf = [0u8; 4096];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if sock.write_all(&buf[..n]).is_err() || sock.flush().is_err() {
            break;
        }
    }
}

/// 起阻塞服务器：每请求一线程（上传大文件不阻塞其它请求）。永不返回（bind 失败返回 Err）。
pub fn serve(bind: &str, router: Router) -> Result<(), String> {
    serve_with(bind, router, ServeOpts::default())
}

pub fn serve_with(bind: &str, router: Router, opts: ServeOpts) -> Result<(), String> {
    let listener = std::net::TcpListener::bind(bind).map_err(|e| format!("绑定 {bind} 失败: {e}"))?;
    serve_listener(listener, router, opts)
}

/// 同 [`serve_with`]，在已经绑好的监听上起服务。测试用它绑 0 端口后直接交过来：此前测试先绑 0 端口取号、放掉、
/// 再按号重绑，放掉到重绑之间并行的别的测试可能抢走这个端口，偶发失败。
pub(crate) fn serve_listener(listener: std::net::TcpListener, router: Router, opts: ServeOpts) -> Result<(), String> {
    let bind = listener.local_addr().map(|a| a.to_string()).unwrap_or_default();
    let ssl = opts.tls.map(|pem| tiny_http::SslConfig { certificate: pem.cert, private_key: pem.key });
    let server = tiny_http::Server::from_listener_with_read_timeout(listener, ssl, Some(READ_IDLE_TIMEOUT)).map_err(|e| format!("在 {bind} 起服务失败: {e}"))?;
    let router = Arc::new(router);
    let guard = opts.guard.map(Arc::new);
    let inflight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for mut req in server.incoming_requests() {
        let Some(permit) = Permit::try_acquire(&inflight, opts.max_concurrent) else {
            respond(req, Reply::error(503, "服务繁忙（并发请求过多），请稍后重试").with_header("Retry-After", "2"));
            continue;
        };
        let router = router.clone();
        let guard = guard.clone();
        std::thread::spawn(move || {
            let _permit = permit;
            let url = req.url().to_string();
            let (path, query) = url.split_once('?').unwrap_or((&url, ""));
            let method = Method::from_tiny(req.method());
            let mut content_type = String::new();
            let mut content_length = None;
            let mut headers: Vec<(String, String)> = Vec::new();
            for h in req.headers() {
                if h.field.equiv("Content-Type") {
                    content_type = h.value.as_str().to_string();
                } else if h.field.equiv("Content-Length") {
                    content_length = h.value.as_str().parse().ok();
                } else if let Some(k) = KEPT_HEADERS.iter().find(|k| h.field.equiv(k)) {
                    headers.push((k.to_string(), h.value.as_str().to_string()));
                }
            }
            // 对端 IP 以内部头传给处理函数（[`Request`] 是各服务直接构造的公开结构体，加字段会波及所有调用方）。
            // 客户端自己发来的同名头在上面的 KEPT_HEADERS 白名单里就被丢掉了，这里写入的只可能是真实对端地址。
            let remote = req.remote_addr().map(|a| a.ip().to_canonical());
            if let Some(ip) = remote {
                headers.push((REMOTE_IP_HEADER.to_string(), ip.to_string()));
            }
            let path = path.to_string();
            if let Some(g) = &guard {
                // 守卫也可能 panic（读配置/会话表）：同处理函数一样兜成 JSON 500，而不是让线程带着 panic 消亡、回空 500。
                let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (g.check)(&GuardRequest { method, path: path.clone(), headers: headers.clone(), remote })));
                if let Some(reply) = checked.unwrap_or_else(|_| Some(Reply::error(500, "服务内部错误（已记录到日志）"))) {
                    respond(req, reply);
                    return;
                }
            }
            let query = parse_query(query);
            let reply = {
                let mut body = req.as_reader();
                let mut r = Request { method, path, query, params: HashMap::new(), content_type, content_length, headers, body: &mut body };
                // 处理函数 panic（release 是 panic=unwind）：线程本来会带着 panic 消亡、tiny_http 回一个空 500，网页拿不到 JSON；
                // 这里兜住回标准的 JSON 500（panic 信息已由默认 hook 打到 stderr/journal），并发名额由 `_permit` 的 Drop 照常归还。
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| router.dispatch(&mut r))).unwrap_or_else(|_| Reply::error(500, "服务内部错误（已记录到日志）"))
            };
            respond(req, reply);
        });
    }
    // 走到这里＝tiny_http 的 accept 线程已退出（遇到非暂时性 accept 错误），服务再也收不到连接。此前返回 `Ok(())`，
    // 各服务 main 于是以退出码 0 正常结束——单元是 `Restart=on-failure`，systemd 不会拉起，服务就此静默消失。
    // 报错让进程非零退出，交给 systemd 重启（2026-09-25 第四轮审计）。
    Err(format!("{bind} 停止接受连接（accept 线程退出），退出交给 systemd 重启"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ApiResult;

    #[test]
    fn permit_caps_concurrency_and_releases_on_drop() {
        let c = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let a = Permit::try_acquire(&c, Some(2)).unwrap();
        let b = Permit::try_acquire(&c, Some(2)).unwrap();
        assert!(Permit::try_acquire(&c, Some(2)).is_none(), "满额后拒绝，且失败的尝试不占名额");
        assert_eq!(c.load(std::sync::atomic::Ordering::SeqCst), 2);
        drop(a);
        let c3 = Permit::try_acquire(&c, Some(2)).expect("释放后可再取");
        drop((b, c3));
        assert_eq!(c.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(Permit::try_acquire(&c, None).is_some(), "None=不限");
    }

    /// 绑 0 端口、把监听直接交给 `start`（不放掉再重绑，并行测试抢不走），返回端口。监听已经在 listen，
    /// 服务线程还没开始 accept 时连进来的连接在内核队列里等着，不用轮询等它就绪。
    fn free_port_and_wait(start: impl FnOnce(std::net::TcpListener) + Send + 'static) -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || start(l));
        port
    }

    /// 对端 IP 路由：守卫看到 `GuardRequest.remote`，处理函数经 `Request::remote_ip()` 读到；客户端伪造的同名头无效。
    fn ip_router_and_guard() -> (Router, Guard) {
        let router = Router::new().get("/ip", |r| Ok(Reply::ok(&serde_json::json!({"ip": r.remote_ip().map(|i| i.to_string())}))));
        let guard = Guard {
            check: Arc::new(|g: &GuardRequest| if g.remote == Some(std::net::Ipv4Addr::LOCALHOST.into()) { None } else { Some(Reply::error(403, "守卫没拿到对端 IP")) }),
        };
        (router, guard)
    }

    #[test]
    fn remote_ip_reaches_guard_and_handler_and_cannot_be_spoofed() {
        let (router, guard) = ip_router_and_guard();
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, router, ServeOpts { guard: Some(guard), ..ServeOpts::default() });
        });
        let body = ureq::get(&format!("http://127.0.0.1:{port}/ip")).set(REMOTE_IP_HEADER, "1.2.3.4").call().unwrap().into_string().unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ip"], "127.0.0.1");
    }

    /// HTTPS（tiny_http + rustls）路径下同样拿得到对端 IP；顺带证明带名称约束的 CA 签出的叶能完成真实握手。
    #[test]
    fn remote_ip_works_over_tls() {
        let dir = tempfile::tempdir().unwrap();
        let pem = crate::tls::ensure_ca_signed(dir.path(), &[]).unwrap();
        let ca = crate::tls::ca_pem(dir.path()).unwrap();
        let (router, guard) = ip_router_and_guard();
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, router, ServeOpts { tls: Some(pem), guard: Some(guard), ..ServeOpts::default() });
        });
        let mut roots = rustls::RootCertStore::empty();
        let ca_der = x509_parser::pem::parse_x509_pem(&ca).unwrap().1.contents;
        roots.add(rustls_pki_types::CertificateDer::from(ca_der)).unwrap();
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth();
        let conn = rustls::ClientConnection::new(Arc::new(cfg), rustls_pki_types::ServerName::try_from("shelf.local").unwrap()).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, std::net::TcpStream::connect(("127.0.0.1", port)).unwrap());
        tls.write_all(b"GET /ip HTTP/1.1\r\nHost: shelf.local\r\nX-Rmsvc-Remote-Ip: 1.2.3.4\r\nConnection: close\r\n\r\n").unwrap();
        let mut out = Vec::new();
        let _ = tls.read_to_end(&mut out); // 对端直接断开 TCP 时 rustls 报 UnexpectedEof，内容已读到
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.contains(r#""ip":"127.0.0.1""#), "{text}");
    }

    /// 回归（RUSTSEC-2024-0336）：TLS 握手中途收到 close_notify 告警，连接线程不能在 rustls `complete_io` 里空转
    /// （rustls 0.20 整个系列都会死循环：网关 443 上任何局域网主机发 7 个字节，就能让一条连接线程吃满一个核）。
    /// 要量"有没有线程在空转"只能看进程 CPU 时间，而同一测试进程里别的测试（PBKDF2 之类）也在吃 CPU，
    /// 所以把探针放进独立子进程只跑它一个（`--exact … --ignored`），子进程自己量自己。
    /// 2026-10-10 实测：rustls 0.20 时子进程 1.5 秒窗口里吃满 ~1.5 秒 CPU（红），0.23 时近 0（绿）。
    #[test]
    fn close_notify_during_handshake_does_not_spin() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "http::server::tests::close_notify_probe", "--ignored", "--test-threads=1", "--nocapture"])
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && text.contains("1 passed"), "{text}");
    }

    /// [`close_notify_during_handshake_does_not_spin`] 的子进程探针：起 HTTPS 服务，发一条明文 close_notify 告警后
    /// 不关自己这端，看接下来 1.5 秒本进程吃了多少 CPU。
    #[test]
    #[ignore = "由 close_notify_during_handshake_does_not_spin 在独立子进程里跑（要单独量本进程 CPU）"]
    fn close_notify_probe() {
        fn cpu_secs() -> f64 {
            let mut u: libc::rusage = unsafe { std::mem::zeroed() };
            // SAFETY: getrusage 只往给定的结构体里写。
            unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
            let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
            tv(u.ru_utime) + tv(u.ru_stime)
        }
        let dir = tempfile::tempdir().unwrap();
        let pem = crate::tls::ensure_ca_signed(dir.path(), &[]).unwrap();
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, Router::new(), ServeOpts { tls: Some(pem), ..ServeOpts::default() });
        });
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        // TLS 明文告警记录：类型 21（alert）、版本 3.3、长度 2、级别 1（warning）、描述 0（close_notify）
        s.write_all(&[21, 3, 3, 0, 2, 1, 0]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let c0 = cpu_secs();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let used = cpu_secs() - c0;
        assert!(used < 0.5, "握手中收到 close_notify 后有线程在空转：1.5 秒里吃了 {used:.2} 秒 CPU");
        drop(s);
    }

    /// 只发半个请求头就不动的连接：到读空闲超时就被断开（不再永久占线程）；监听本身不受影响，
    /// 空闲超过超时时长之后新连接照常服务（曾试过把 SO_RCVTIMEO 设在监听 socket 上，accept 也跟着
    /// 超时、上游 accept 循环遇错即退出，整个服务停摆——这条测试的后半段就是防它）。
    #[test]
    fn half_sent_request_is_dropped_after_idle_timeout_and_server_keeps_accepting() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let server = tiny_http::Server::from_listener_with_read_timeout(l, None, Some(std::time::Duration::from_millis(1500))).unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let _ = req.respond(tiny_http::Response::from_string("ok"));
            }
        });
        let mut slow = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        slow.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n").unwrap(); // 故意不发结束空行
        slow.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let t0 = std::time::Instant::now();
        let mut out = Vec::new();
        let _ = slow.read_to_end(&mut out);
        // Linux 上读超时报 EAGAIN（WouldBlock），上游只对 TimedOut 回 408，其余读错误直接关连接——两种都算断开。
        let text = String::from_utf8_lossy(&out);
        assert!(text.is_empty() || text.starts_with("HTTP/1.1 408"), "{text:?}");
        let took = t0.elapsed();
        assert!(took >= std::time::Duration::from_millis(1200) && took < std::time::Duration::from_secs(5), "{took:?}");
        std::thread::sleep(std::time::Duration::from_millis(2000)); // 监听空闲超过超时时长
        let body = ureq::get(&format!("http://127.0.0.1:{port}/")).call().unwrap().into_string().unwrap();
        assert_eq!(body, "ok");
    }

    /// 起真服务：处理函数 panic 得到 JSON 500，且并发名额归还、后续请求照常服务。
    #[test]
    fn handler_panic_becomes_json_500_and_server_keeps_serving() {
        let router = Router::new()
            .get("/boom", |_| -> ApiResult { panic!("测试用 panic") })
            .get("/ok", |_| Ok(Reply::ok(&serde_json::json!({"ok": true}))));
        let opts = ServeOpts { max_concurrent: Some(2), ..ServeOpts::default() };
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, router, opts);
        });
        for _ in 0..3 {
            // max_concurrent=2（串行客户端最多「上一请求收尾 + 当前」两条）：若 panic 后名额没归还，累计泄漏到第三轮会变 503
            match ureq::get(&format!("http://127.0.0.1:{port}/boom")).call() {
                Err(ureq::Error::Status(500, r)) => {
                    let v: serde_json::Value = serde_json::from_str(&r.into_string().unwrap()).unwrap();
                    assert_eq!(v["ok"], false);
                    assert!(v["message"].as_str().unwrap().contains("内部错误"));
                }
                other => panic!("期望 500，得到 {other:?}"),
            }
            assert_eq!(ureq::get(&format!("http://127.0.0.1:{port}/ok")).call().unwrap().status(), 200);
        }
    }

    /// 守卫 panic 同样得到 JSON 500，服务照常。
    #[test]
    fn guard_panic_becomes_json_500() {
        let router = Router::new().get("/ok", |_| Ok(Reply::ok(&serde_json::json!({"ok": true}))));
        let guard = Guard { check: Arc::new(|g: &GuardRequest| if g.path == "/boom" { panic!("测试用守卫 panic") } else { None }) };
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, router, ServeOpts { guard: Some(guard), ..ServeOpts::default() });
        });
        match ureq::get(&format!("http://127.0.0.1:{port}/boom")).call() {
            Err(ureq::Error::Status(500, r)) => assert!(r.into_string().unwrap().contains("内部错误")),
            other => panic!("期望 JSON 500，得到 {other:?}"),
        }
        assert_eq!(ureq::get(&format!("http://127.0.0.1:{port}/ok")).call().unwrap().status(), 200);
    }

    /// 事件流（SSE）：不带长度，小帧不被缓冲、立刻到达；reader 不结束连接就一直开着（与定长下载是两条路）。
    /// 处理函数手工加的 `Content-Length` 头也不会把它变成"定长下载"——发法只看 [`Body`] 变体（2026-10-10 前看这个头）。
    #[test]
    fn event_stream_delivers_small_frames_immediately_without_length() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let rx = std::sync::Mutex::new(Some(rx));
        /// 从通道读帧的 reader：通道关闭 = 流结束。
        struct Frames(std::sync::mpsc::Receiver<Vec<u8>>);
        impl Read for Frames {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let Ok(f) = self.0.recv() else { return Ok(0) };
                buf[..f.len()].copy_from_slice(&f);
                Ok(f.len())
            }
        }
        let router = Router::new().get("/events", move |_| {
            let rx = rx.lock().unwrap().take().unwrap();
            Ok(Reply::event_stream("text/event-stream", Box::new(Frames(rx))).with_header("Content-Length", "3"))
        });
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, router, ServeOpts::default());
        });
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        s.write_all(b"GET /events HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        tx.send(b"data: 1\n\n".to_vec()).unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 1024];
        while !String::from_utf8_lossy(&got).contains("data: 1") {
            let n = s.read(&mut buf).expect("小帧应在 5 秒内到达，不能滞留在缓冲里");
            assert!(n > 0, "流还没结束，连接不该关：{}", String::from_utf8_lossy(&got));
            got.extend_from_slice(&buf[..n]);
        }
        let head = String::from_utf8_lossy(&got).to_ascii_lowercase();
        assert!(head.starts_with("http/1.1 200") && head.contains("text/event-stream"), "{head}");
        tx.send(b"data: 2\n\n".to_vec()).unwrap();
        let n = s.read(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).contains("data: 2"), "第二帧照样到达（没有按头里的 3 字节截断）");
        drop(tx);
        let mut rest = Vec::new();
        let _ = s.read_to_end(&mut rest);
    }

    /// 回归：已知长度的流（文件下载）要能完整收完、响应正常结束——此前走 SSE 的"读到连接关闭"路径，
    /// reader 读完连接却不关，客户端永远收不完（2026-09-24 真机）。
    #[test]
    fn sized_stream_completes_with_content_length() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let want = data.clone();
        let router = Router::new().get("/f", move |_| {
            let d = data.clone();
            let n = d.len() as u64;
            Ok(Reply::sized_stream("application/octet-stream", Box::new(std::io::Cursor::new(d)), n).with_header("Content-Disposition", "attachment; filename=\"f.bin\""))
        });
        let port = free_port_and_wait(move |l| {
            let _ = serve_listener(l, router, ServeOpts::default());
        });
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(10)).build();
        let resp = agent.get(&format!("http://127.0.0.1:{port}/f")).call().unwrap();
        assert_eq!(resp.header("Content-Length"), Some("300000"));
        assert!(resp.header("Content-Disposition").is_some_and(|v| v.contains("f.bin")));
        let mut got = Vec::new();
        std::io::Read::read_to_end(&mut resp.into_reader(), &mut got).expect("10 秒内应读完，不能卡住");
        assert_eq!(got, want);
    }
}
