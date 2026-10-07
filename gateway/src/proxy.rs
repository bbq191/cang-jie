//! 反向代理（Facade）：把 `/api/<seg>[/<rest>]` 转给注册表里的服务（剥掉 `<seg>`），状态码/JSON 原样回。
//! **请求方向流式**（`send(&mut *req.body)` 直接转发原始请求体读取器，上传大文件不额外占内存）。
//! **响应方向**：后端给了 `Content-Length` 的 200 应答，若是下载（带 `Content-Disposition`）或体积超过
//! [`STREAM_MIN_BYTES`]（壁纸原图等），走 `Reply::sized_stream` 按定长边读边发，不整个读进网关内存；
//! 其余（JSON 等小应答、没有长度的应答）读完再回——没有长度的流只能走 SSE 那条"读到连接关闭"的通道，
//! 不适合普通下载（见 [`stream_len`]）。
//!
//! **并发/内存预算闸门**（2026-09-19）：`加入xochitl` 在这里统一拦一道——`book-serve` 的忙锁是按书名分别加的、
//! 点不同的书互不阻塞，同时投几本大部头会叠加内存与 xochitl 的导入负担。`gateway` 是这个操作物理上唯一必经的转发关口，
//! 闸门放这里不需要任何跨进程锁，详见 `budget.rs` 文档注释。只有命中的路由才会额外读一次 body（几十字节的小 JSON，
//! `Request::read_small_body` 本来就有 1MB 上限）+ 查一次文件体积，其余请求（含真正的大文件上传）完全
//! 不受影响、维持原有纯流式转发。原来还拦「优化」和勾了同步优化的「抓网文」——书架 2026-10-07 不再优化书（优化全部在
//! 电脑上用 sheng-ren 做），两条连同 book-serve 的接口一起删了；「加入 KOReader」随 2026-09-29 设备卸载 KOReader 撤掉。
use rmsvc_core::http::{ApiError, ApiResult, JsonBody, Method, Reply, Request};
use rmsvc_core::multipart::percent_encode as enc;
use rmsvc_core::paths::Paths;
use rmsvc_core::registry::{self, SvcClient};
use std::io::Read;
use std::time::{Duration, Instant};

/// 等"book-serve 有新事件"的兜底超时：正常靠 [`crate::events::books_wake`] 事件唤醒（忙态结束 book-serve 会发 `books`
/// 事件），这里只防事件丢了/订阅线程重连空窗——所以从原来的 5 秒轮询放宽到 30 秒（整个处理期间唤醒降到 1/6）。
const POLL_FALLBACK: Duration = Duration::from_secs(30);
/// 轮询等一个异步落库真正跑完的上限——不是永久卡死，服务崩溃/重启导致侦测不到
/// 结果时，超时后如实放弃、让名额自然释放，不为一个查不到结果的任务永久占着并发档位。
const SETTLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// 这次落库在闸门里要占的名额：键（书名）+ 档位（按母版库里这本书的体积）。体积由 `size_of` 注入，便于离线测。
fn gate_target(body: &JsonBody, size_of: impl Fn(&str) -> u64) -> Result<(String, crate::budget::Tier), ApiError> {
    let name = body.str("name")?.to_string();
    let tier = crate::budget::tier_of(size_of(&name));
    Ok((name, tier))
}

/// 这个请求是不是要限流的「加入 xochitl」。`service_name` 是解析过的后端服务名（`book-serve`，不是 URL 段 `books`）。
fn is_gated(service_name: &str, rest: &str, method: Method) -> bool {
    method == Method::Post && service_name == "book-serve" && rest == "staging/deliver"
}

/// 不带 `Content-Disposition` 的应答超过这个体积也流式转发（壁纸原图、裁图等图片；JSON 列表远小于它）。
const STREAM_MIN_BYTES: u64 = 256 * 1024;

/// 这条后端应答该不该流式转发；该 → 返回定长。**只流式转发有 `Content-Length` 的 200**：定长流由 tiny_http
/// 按长度发完即结束；没有长度的流只能走 `Reply::stream`（SSE 用的"升级成裸 socket、读到连接关闭"），拿来做
/// 普通下载会让客户端等不到结束（2026-09-24 真机下载卡住就是这一类），所以宁可读完再回。
fn stream_len(status: u16, download: bool, len: Option<u64>) -> Option<u64> {
    let n = len?;
    (status == 200 && (download || n > STREAM_MIN_BYTES)).then_some(n)
}

/// `/api/{svc}/*` → 按 URL 段查目录表找服务名再转发（段不在表里 404）。
pub fn forward(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let Some(name) = crate::manage::service_of(req.param("svc")) else { return Err(ApiError::not_found("未知服务")) };
    let Some(info) = registry::find(paths, name) else {
        return Err(ApiError { status: 404, message: format!("{name} 未安装或未运行") });
    };
    // 剥掉服务段：`/api/fonts/x` → 后端 `/x`，`/api/fonts` → 后端 `/`。后端直连（SSH 调试）与经网关同一套路由。
    let rest = req.param("*").to_string();
    let gated = is_gated(name, &rest, req.method);

    // 命中限流操作才读 body 拿书名、过闸门；其余请求原样走下面已有的流式转发，不碰这段。
    let mut body_override: Option<Vec<u8>> = None;
    let mut slot: Option<crate::budget::Slot<'static>> = None;
    let mut book_name = String::new();
    if gated {
        // 读一次 body 拿书名、过闸门后还要原样转发给后端，所以先读成字节再解析（`req.json()` 会把流读空）。
        let buf = req.read_small_body().map_err(ApiError::bad)?;
        let parsed: serde_json::Value = serde_json::from_slice(&buf).map_err(|e| ApiError::bad(format!("请求不是 JSON: {e}")))?;
        let staging = paths.staging_dir();
        let (key, tier) = gate_target(&JsonBody(parsed), |n| staging.join(n).metadata().map(|m| m.len()).unwrap_or(0))?;
        slot = Some(crate::budget::global().admit(tier, &key).map_err(|e| ApiError { status: e.status(), message: e.message() })?);
        book_name = key;
        body_override = Some(buf);
    }

    let mut url = format!("{}/{}", info.base_url(), rest);
    if !req.query.is_empty() {
        let q: Vec<String> = req.query.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect();
        url.push('?');
        url.push_str(&q.join("&"));
    }
    let method = match req.method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Delete => "DELETE",
        _ => return Err(ApiError::bad("unsupported method")),
    };
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(900)).build();
    let mut r = agent.request(method, &url);
    if !req.content_type.is_empty() {
        r = r.set("Content-Type", &req.content_type);
    }
    // 只在真的转发请求体时带长度：GET/DELETE 走 `call()` 不发 body，若照抄客户端的 Content-Length（带 body 的 DELETE），
    // 后端会一直等那几个永远不来的字节直到超时；闸门那条路重发的是读出来的字节，长度以它为准。
    let has_body = !matches!(req.method, Method::Get | Method::Delete);
    if let Some(n) = body_override.as_ref().map(Vec::len).or(req.content_length).filter(|_| has_body) {
        r = r.set("Content-Length", &n.to_string());
    }
    let resp = if !has_body {
        r.call()
    } else if let Some(buf) = body_override {
        r.send(&mut std::io::Cursor::new(buf))
    } else {
        r.send(&mut *req.body)
    };
    let (status, resp) = match resp {
        Ok(r) => (r.status(), r),
        Err(ureq::Error::Status(c, r)) => (c, r),
        Err(e) => return Err(ApiError { status: 502, message: format!("{name} 无响应: {e}") }),
    };
    let ctype = resp.header("Content-Type").unwrap_or("application/octet-stream").to_string();
    // 只转发这一个头：后端服务想让浏览器"下载保存"而不是原地展示/跳转时设它（如 md/zip 导出、CA 证书下载，
    // 见 gateway::main 的证书下载同款用法）；别的头一律不转发，不给后端服务借这条通道夹带别的东西。
    let disposition = resp.header("Content-Disposition").map(str::to_string);
    // 下载（母版库原件可达上百 MB）与大应答：按定长边读边发，不整个读进网关内存；其余照旧读完再回。
    let len = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
    let mut reply = if let Some(n) = stream_len(status, disposition.is_some(), len) {
        Reply::sized_stream(&ctype, Box::new(resp.into_reader()), n).with_status(status)
    } else {
        let mut body = Vec::new();
        resp.into_reader().read_to_end(&mut body).map_err(|e| ApiError::internal(e.to_string()))?;
        Reply { status, content_type: ctype, body, headers: vec![], stream: None }
    };
    if let Some(v) = disposition {
        reply = reply.with_header("Content-Disposition", &v);
    }

    // 名额释放时机：落库是异步的，HTTP 响应此刻只代表"已经开始"，真正的上传在后台线程里继续——把 `slot` 转移进一个
    // 监控线程，轮询该服务自己的 `/staging` 列表直到这本书不再 busy（或条目已经不在了），`slot` 才出那个线程的作用域
    // 释放；轮询/线程本身跟这次 HTTP 响应完全解耦，不影响这次请求的返回时间。
    if let Some(slot) = slot.take() {
        let client = SvcClient::new(paths.clone(), "book-serve", 10);
        std::thread::spawn(move || {
            poll_until_settled(&client, &book_name);
            drop(slot);
        });
    }

    Ok(reply)
}

/// 查 book-serve 的 `/staging`（直连后端服务，不经网关自己这层转发，避免自己调自己）直到
/// [`crate::budget::is_settled`] 判定这本书已经不再忙，或等到 [`SETTLE_POLL_TIMEOUT`] 放弃。
/// **事件驱动**：每次查完就阻塞等 [`crate::events::books_wake`]（book-serve 有事件才醒），至多 [`POLL_FALLBACK`] 兜底一次；
/// 先取代数再查，查询期间到达的事件不会漏。
/// 查询失败要**连续** [`MAX_POLL_FAILURES`] 次才放弃（每次隔 [`FAILURE_RETRY`]）：此前一次失败就放名额，
/// 而 book-serve 忙着处理大书时 10 秒查询超时恰恰最容易撞上，于是第二本大书被放进来、内存照样叠加——
/// 闸门在最该起作用的时候失效（2026-09-24 审查）。连续失败才说明服务真的挂了/重启了（任务随之没了），
/// 这时再放名额，不让侦测本身不可靠把并发档位永久卡住。
pub(crate) fn poll_until_settled(client: &SvcClient, name: &str) {
    let wake = crate::events::books_wake();
    wait_settled(|| client.get_json("/staging"), name, Instant::now() + SETTLE_POLL_TIMEOUT, || wake.generation(), |seen, d| throttled_wait(wake, seen, d, MIN_REQUERY));
}

/// 两次 `GET /staging` 之间的最短间隔。book-serve 每有母版库事件就发一条 `staging`（同一个 kind 分不出"进度"还是"忙完"；
/// 以前大书优化期间约每秒一条），每条都唤醒等待方、每条都整表查一次的话，就是持续的 loopback 请求 + book-serve 整表序列化。代价：名额最多晚这么久才归还（下一本排队的书晚几秒开始），与整本处理时长相比可忽略。
const MIN_REQUERY: Duration = Duration::from_secs(5);

/// 等 `wake` 的代数离开 `seen`（至多 `max`），但从调用起**至少**过 `min.min(max)` 才返回——事件再密也不会让调用方
/// 比这更频繁地去查。
fn throttled_wait(wake: &crate::events::Wake, seen: u64, max: Duration, min: Duration) {
    let start = Instant::now();
    wake.wait_change(seen, max);
    if let Some(rest) = min.min(max).checked_sub(start.elapsed()) {
        std::thread::sleep(rest);
    }
}

/// 连续几次查询失败才认定"服务已不可达、任务没了"。
const MAX_POLL_FAILURES: u32 = 6;
/// 查询失败后至多隔多久重试（服务卡住时事件多半不来，不能只等事件）。
const FAILURE_RETRY: Duration = Duration::from_secs(5);

/// [`poll_until_settled`] 的循环本体，查询/代数/等待都由调用方注入，便于离线测试。
fn wait_settled(
    mut query: impl FnMut() -> Result<serde_json::Value, String>,
    name: &str,
    deadline: Instant,
    generation: impl Fn() -> u64,
    wait: impl Fn(u64, Duration),
) {
    let mut failures = 0u32;
    loop {
        let seen = generation();
        let failed = match query() {
            Ok(json) if crate::budget::is_settled(&json, name) => return,
            Ok(_) => false, // 还在忙，继续轮询
            Err(_) => true,
        };
        failures = if failed { failures + 1 } else { 0 };
        if failures >= MAX_POLL_FAILURES || Instant::now() >= deadline {
            return;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if failed {
            wait(generation(), FAILURE_RETRY.min(left)); // 等下一次事件，至多 FAILURE_RETRY
        } else {
            wait(seen, POLL_FALLBACK.min(left));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_deliver_is_gated_and_tier_follows_book_size() {
        assert!(is_gated("book-serve", "staging/deliver", Method::Post));
        for gone in ["staging/optimize", "staging/fetch-article"] {
            assert!(!is_gated("book-serve", gone, Method::Post), "{gone} 已随书架不再优化删除");
        }
        assert!(!is_gated("koreader-serve", "books/adopt", Method::Post), "加入 KOReader 随 2026-09-29 卸载撤掉");
        let big = |_: &str| crate::budget::LARGE_THRESHOLD_BYTES + 1;
        assert_eq!(gate_target(&JsonBody(serde_json::json!({"name": "x.epub"})), big).unwrap(), ("x.epub".to_string(), crate::budget::Tier::Large));
        assert!(gate_target(&JsonBody(serde_json::json!({})), big).is_err(), "缺 name 直接 400");
    }

    /// 回归：客户端发了带 `Content-Length` 的 DELETE，网关用 `call()` 不转发 body，也就不能转发这个长度——否则后端会
    /// 干等那几个字节。假后端只读请求头，记下有没有 `content-length`，然后立刻回 200。
    #[test]
    fn delete_forwards_no_content_length() {
        use std::io::Write;
        let t = tempfile::tempdir().unwrap();
        let paths = crate::testutil::sandbox(&t);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let backend = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = sock.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let body = b"{\"ok\":true}";
            write!(sock, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            sock.write_all(body).unwrap();
            String::from_utf8_lossy(&buf).to_ascii_lowercase()
        });
        let info = registry::ServiceInfo { name: "font-serve".into(), port, label: String::new(), version: String::new(), pid: std::process::id(), ui: None };
        let _reg = registry::register(&paths, &info).unwrap();
        let mut rd: &[u8] = b"12345";
        let params = [("svc", "fonts"), ("*", "x.ttf")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let mut req = Request { method: Method::Delete, path: "/api/fonts/x.ttf".into(), query: Default::default(), params, content_type: String::new(), content_length: Some(5), headers: vec![], body: &mut rd };
        let reply = forward(&paths, &mut req).unwrap();
        let head = backend.join().unwrap();
        assert_eq!(reply.status, 200);
        assert!(head.starts_with("delete /x.ttf"), "{head}");
        assert!(!head.contains("content-length"), "DELETE 不转发 body，也不能带长度：{head}");
    }

    #[test]
    fn gate_ignores_everything_else() {
        assert!(!is_gated("book-serve", "staging", Method::Post), "入库本身走多文件上传，不该被拦下来读 body");
        assert!(!is_gated("book-serve", "staging", Method::Get), "列表查询不限流");
        assert!(!is_gated("book-serve", "staging/deliver", Method::Get), "方法不对不该命中");
        assert!(!is_gated("font-serve", "staging/deliver", Method::Post), "服务名对不上不该误命中");
    }

    #[test]
    fn streams_only_sized_downloads_or_big_bodies() {
        assert_eq!(stream_len(200, true, Some(10)), Some(10), "带长度的下载：流式");
        assert_eq!(stream_len(200, false, Some(STREAM_MIN_BYTES + 1)), Some(STREAM_MIN_BYTES + 1), "大图片：流式");
        assert_eq!(stream_len(200, false, Some(1000)), None, "小 JSON：读完再回");
        assert_eq!(stream_len(200, true, None), None, "没有长度的下载不能走读到关闭的流");
        assert_eq!(stream_len(404, true, Some(10)), None, "错误应答读完再回");
    }

    /// 回归：一次查询失败（book-serve 忙、查询超时）不能提前放名额，要等它真的不忙。
    #[test]
    fn transient_query_failure_does_not_release_slot_early() {
        use std::cell::Cell;
        let busy = serde_json::json!({"items": [{"name": "big.epub", "busy": true}]});
        let idle = serde_json::json!({"items": [{"name": "big.epub", "busy": false}]});
        let script: Vec<Result<serde_json::Value, String>> = vec![Ok(busy.clone()), Err("timeout".into()), Err("timeout".into()), Ok(busy), Err("timeout".into()), Ok(idle)];
        let calls = Cell::new(0usize);
        wait_settled(|| { let i = calls.get(); calls.set(i + 1); script[i].clone() }, "big.epub", Instant::now() + Duration::from_secs(3600), || 0, |_, _| {});
        assert_eq!(calls.get(), 6, "应一直等到真正不忙（第 6 次查询）才返回");
    }

    /// 事件密集（比如每秒一条）也不会让等待方查得比 `min` 更勤；没有事件时照旧等到 `max`（`max` 比 `min` 短时以 `max` 为准）。
    #[test]
    fn throttled_wait_never_returns_before_min_interval() {
        let w = std::sync::Arc::new(crate::events::Wake::default());
        let seen = w.generation();
        w.bump(); // 事件早就到了：wait_change 立即返回，但仍要等满 min
        let t = Instant::now();
        throttled_wait(&w, seen, Duration::from_secs(5), Duration::from_millis(150));
        let el = t.elapsed();
        assert!(el >= Duration::from_millis(150) && el < Duration::from_secs(3), "{el:?}");
        let seen = w.generation();
        let t = Instant::now();
        throttled_wait(&w, seen, Duration::from_millis(60), Duration::from_secs(5));
        let el = t.elapsed();
        assert!(el >= Duration::from_millis(60) && el < Duration::from_secs(3), "无事件：max 更短时以 max 为准，{el:?}");
    }

    /// 连续失败到上限 → 放弃（服务真挂了，任务已随之消失）。
    #[test]
    fn persistent_failure_gives_up_after_limit() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        wait_settled(|| { calls.set(calls.get() + 1); Err("down".into()) }, "x", Instant::now() + Duration::from_secs(3600), || 0, |_, _| {});
        assert_eq!(calls.get(), MAX_POLL_FAILURES);
    }
}
