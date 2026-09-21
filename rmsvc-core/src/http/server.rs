//! 服务器：tiny_http 适配（每请求一线程、并发名额、守卫、SSE 裸 socket 流式回执、TLS）。
use super::router::{parse_query, Router};
use super::{header_of, Method, Reply, Request};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

/// 同时在处理的请求数缺省上限（含一直挂着的 SSE 流，每个请求占一条线程）。
/// 取值依据：设备 2 核、约 2GB 内存；合法并发上限 ≈ 浏览器每源 6 条连接 × 几个标签页 + 网关到各服务的
/// 8 条 loopback SSE 订阅 ≈ 30 上下；每条线程栈虚拟 2MB、常驻只有几十 KB，64 条最坏也就几 MB 常驻，
/// 既给合法用法留一倍以上余量，又让局域网内恶意/失控的大量连接（每个请求一条线程、登录还要做 60 万轮
/// PBKDF2）吃不掉整台设备。超限的请求直接 503 + `Retry-After`，不再 spawn 线程。
pub const DEFAULT_MAX_CONCURRENT: usize = 64;

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
}
impl GuardRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        header_of(&self.headers, name)
    }
}

const KEPT_HEADERS: &[&str] = &["Cookie", "Authorization", "Accept", "Host", "X-Forwarded-Proto", "User-Agent"];

fn reply_to_tiny(reply: Reply) -> tiny_http::Response<Box<dyn Read + Send>> {
    let mut headers = Vec::new();
    if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Type"[..], reply.content_type.as_bytes()) {
        headers.push(h);
    }
    for (k, v) in reply.headers {
        if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            headers.push(h);
        }
    }
    let len = reply.body.len();
    tiny_http::Response::new(tiny_http::StatusCode(reply.status), headers, Box::new(std::io::Cursor::new(reply.body)) as Box<dyn Read + Send>, Some(len), None)
}

/// 流式回执（SSE）：**不能**走 `respond`——tiny_http 的 chunked 编码器（chunked_transfer::Encoder）攒满 8 KB 才发、
/// 外面还套一层 1 KB BufWriter，小帧永远滞留（真机 curl 30 s 零字节）。改用 `Request::upgrade` 拿到裸 socket：
/// 先发一个只有头的 200（Content-Type: text/event-stream，无长度、`Connection: upgrade`——浏览器/curl 对 200 忽略它，
/// 按"读到连接关闭"处理），然后从 reader 读一帧写一帧、每帧 flush；客户端断开 → 写失败 → 退出，socket 随之关闭。
fn respond_stream(req: tiny_http::Request, status: u16, content_type: &str, extra: Vec<(String, String)>, mut reader: Box<dyn Read + Send>) {
    let mut resp = tiny_http::Response::empty(tiny_http::StatusCode(status));
    if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()) {
        resp = resp.with_header(h);
    }
    for (k, v) in extra {
        if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            resp = resp.with_header(h);
        }
    }
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
    let server = match opts.tls {
        Some(pem) => tiny_http::Server::https(bind, tiny_http::SslConfig { certificate: pem.cert, private_key: pem.key }).map_err(|e| format!("绑定 {bind}（TLS）失败: {e}"))?,
        None => tiny_http::Server::http(bind).map_err(|e| format!("绑定 {bind} 失败: {e}"))?,
    };
    let router = Arc::new(router);
    let guard = opts.guard.map(Arc::new);
    let inflight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for mut req in server.incoming_requests() {
        let Some(permit) = Permit::try_acquire(&inflight, opts.max_concurrent) else {
            let _ = req.respond(reply_to_tiny(Reply::error(503, "服务繁忙（并发请求过多），请稍后重试").with_header("Retry-After", "2")));
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
            let path = path.to_string();
            if let Some(g) = &guard {
                if let Some(reply) = (g.check)(&GuardRequest { method, path: path.clone(), headers: headers.clone() }) {
                    let _ = req.respond(reply_to_tiny(reply));
                    return;
                }
            }
            let query = parse_query(query);
            let mut reply = {
                let mut body = req.as_reader();
                let mut r = Request { method, path, query, params: HashMap::new(), content_type, content_length, headers, body: &mut body };
                // 处理函数 panic（release 是 panic=unwind）：线程本来会带着 panic 消亡、tiny_http 回一个空 500，网页拿不到 JSON；
                // 这里兜住回标准的 JSON 500（panic 信息已由默认 hook 打到 stderr/journal），并发名额由 `_permit` 的 Drop 照常归还。
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| router.dispatch(&mut r))).unwrap_or_else(|_| Reply::error(500, "服务内部错误（已记录到日志）"))
            };
            if let Some(reader) = reply.stream.take() {
                respond_stream(req, reply.status, &reply.content_type, reply.headers, reader);
                return;
            }
            let _ = req.respond(reply_to_tiny(reply));
        });
    }
    Ok(())
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

    /// 起真服务：处理函数 panic 得到 JSON 500，且并发名额归还、后续请求照常服务。
    #[test]
    fn handler_panic_becomes_json_500_and_server_keeps_serving() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let router = Router::new()
            .get("/boom", |_| -> ApiResult { panic!("测试用 panic") })
            .get("/ok", |_| Ok(Reply::ok(&serde_json::json!({"ok": true}))));
        let opts = ServeOpts { max_concurrent: Some(2), ..ServeOpts::default() };
        let addr = format!("127.0.0.1:{port}");
        std::thread::spawn(move || {
            let _ = serve_with(&addr, router, opts);
        });
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
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
}
