//! 反向代理（Facade）：把 `/api/<seg>[/<rest>]` 转给注册表里的服务（剥掉 `<seg>`），状态码/JSON 原样回。
//! **只有请求方向真的流式**（`send(&mut *req.body)` 直接转发原始请求体读取器，上传大文件不额外占内存）；
//! **响应方向整体缓冲进内存**（`into_reader().read_to_end(...)`，2026-09-09 审计发现文档写的"body 流式
//! 透传"跟实现不符，这里改成如实描述）——`rmsvc_core::http::Reply::stream` 现有的流式响应通道是给 SSE
//! 用的，底层走 `tiny_http` 的 `upgrade()` 直接接管裸 socket（不走常规的 Content-Length/chunked 头协商），
//! 拿来复用给任意大小的代理下载响应需要先确认这套机制对非 SSE 场景是否语义正确，评估下来风险和这条
//! 低优先级审计项本身的收益不成比例，这次只改注释，没有改行为。
//!
//! **并发/内存预算闸门**（2026-09-19）：`优化`/`加入xochitl`/`加入KOReader` 这三个操作在这里统一
//! 拦一道——真机测出漫画 optimize/超限分卷投递内存峰值 ≈ 处理的文件体积本身，`book-serve`/
//! `koreader-serve` 的忙锁都是按书名分别加的、点不同的书互不阻塞，同时点几本大部头会线性叠加内存。
//! `gateway` 是这三个操作物理上唯一必经的转发关口（三个服务是独立进程，互不共享内存），闸门放这里
//! 不需要任何跨进程锁，详见 `budget.rs` 文档注释。只有这三条命中路由才会额外读一次 body（几十字节
//! 的小 JSON，`Request::read_small_body` 本来就有 1MB 上限）+ 查一次文件体积，其余请求（含真正的
//! 大文件上传）完全不受影响、维持原有纯流式转发。
use rmsvc_core::http::{ApiError, ApiResult, JsonBody, Method, Reply, Request};
use rmsvc_core::multipart::percent_encode as enc;
use rmsvc_core::paths::Paths;
use rmsvc_core::registry::{self, SvcClient};
use std::io::Read;
use std::time::{Duration, Instant};

/// 等"book-serve 有新事件"的兜底超时：正常靠 [`crate::events::books_wake`] 事件唤醒（忙态结束 book-serve 会发 `books`
/// 事件），这里只防事件丢了/订阅线程重连空窗——所以从原来的 5 秒轮询放宽到 30 秒（整个优化期间唤醒降到 1/6）。
const POLL_FALLBACK: Duration = Duration::from_secs(30);
/// 轮询等一个异步任务（优化/落库）真正跑完的上限——不是永久卡死，服务崩溃/重启导致侦测不到
/// 结果时，超时后如实放弃、让名额自然释放，不为一个查不到结果的任务永久占着并发档位。
const SETTLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GatedOp {
    /// `book-serve` 的"优化"——异步：HTTP 响应几乎立即回"已开始"，真正处理在后台线程跑。
    Optimize,
    /// `book-serve` 的"加入 xochitl"——同上，异步。
    Deliver,
    /// `koreader-serve` 的"加入 KOReader"——同步：`fs::copy`+`fs::rename`，HTTP 响应返回=真正做完。
    KoreaderAdopt,
}

/// 这个请求是不是命中要限流的三个操作之一。`service_name` 是解析过的后端服务名
/// （`book-serve`/`koreader-serve`，不是 URL 段 `books`/`koreader`）。
fn gated_operation(service_name: &str, rest: &str, method: Method) -> Option<GatedOp> {
    if method != Method::Post {
        return None;
    }
    match (service_name, rest) {
        ("book-serve", "staging/optimize") => Some(GatedOp::Optimize),
        ("book-serve", "staging/deliver") => Some(GatedOp::Deliver),
        ("koreader-serve", "books/adopt") => Some(GatedOp::KoreaderAdopt),
        _ => None,
    }
}

/// `/api/{svc}/*` → 按 URL 段查目录表找服务名再转发（段不在表里 404）。
pub fn forward(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let Some(name) = crate::manage::service_of(req.param("svc")) else { return Err(ApiError::not_found("未知服务")) };
    let Some(info) = registry::find(paths, name) else {
        return Err(ApiError { status: 404, message: format!("{name} 未安装或未运行") });
    };
    // 剥掉服务段：`/api/fonts/x` → 后端 `/x`，`/api/fonts` → 后端 `/`。后端直连（SSH 调试）与经网关同一套路由。
    let rest = req.param("*").to_string();
    let gated = gated_operation(name, &rest, req.method);

    // 命中三个限流操作才读 body 拿书名、过闸门；其余请求原样走下面已有的流式转发，不碰这段。
    let mut body_override: Option<Vec<u8>> = None;
    let mut slot: Option<crate::budget::Slot<'static>> = None;
    let mut book_name = String::new();
    if gated.is_some() {
        // 读一次 body 拿书名、过闸门后还要原样转发给后端，所以先读成字节再解析（`req.json()` 会把流读空）。
        let buf = req.read_small_body().map_err(ApiError::bad)?;
        let parsed: serde_json::Value = serde_json::from_slice(&buf).map_err(|e| ApiError::bad(format!("请求不是 JSON: {e}")))?;
        book_name = JsonBody(parsed).str("name")?.to_string();
        let bytes = paths.staging_dir().join(&book_name).metadata().map(|m| m.len()).unwrap_or(0);
        let tier = crate::budget::tier_of(bytes);
        slot = Some(crate::budget::global().admit(tier, &book_name).map_err(|e| ApiError { status: e.status(), message: e.message() })?);
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
    if let Some(n) = req.content_length {
        r = r.set("Content-Length", &n.to_string());
    }
    let resp = if matches!(req.method, Method::Get | Method::Delete) {
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
    // 带 Content-Disposition 的是下载（母版库原件可达上百 MB）：边读边发，不整个读进网关内存；
    // 其余（JSON 等小应答）照旧读完再回。
    // 后端给了 Content-Length 就原样带上，网关也按定长转发（没有长度就走 SSE 式的读到关闭）。
    let mut reply = if status == 200 && disposition.is_some() {
        let len = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
        let reader: Box<dyn std::io::Read + Send> = Box::new(resp.into_reader());
        match len {
            Some(n) => Reply::sized_stream(&ctype, reader, n),
            None => Reply::stream(&ctype, reader),
        }
        .with_status(status)
    } else {
        let mut body = Vec::new();
        resp.into_reader().read_to_end(&mut body).map_err(|e| ApiError::internal(e.to_string()))?;
        Reply { status, content_type: ctype, body, headers: vec![], stream: None }
    };
    if let Some(v) = disposition {
        reply = reply.with_header("Content-Disposition", &v);
    }

    // 名额释放时机：`KoreaderAdopt` 是同步操作，走到这里真正的复制已经做完，`slot` 出函数作用域
    // 自然 Drop 释放，不用特殊处理。`Optimize`/`Deliver` 是异步的，HTTP 响应此刻只代表"已经开始"，
    // 真正的内存开销在后台线程里继续——把 `slot` 转移进一个监控线程，轮询该服务自己的 `/staging`
    // 列表直到这本书不再 busy（或条目已经不在了，比如漫画→PDF 改名），`slot` 才出那个线程的作用域
    // 释放；轮询/线程本身跟这次 HTTP 响应完全解耦，不影响这次请求的返回时间。
    if let Some(kind) = gated {
        if matches!(kind, GatedOp::Optimize | GatedOp::Deliver) {
            if let Some(slot) = slot.take() {
                let client = SvcClient::new(paths.clone(), "book-serve", 10);
                std::thread::spawn(move || {
                    poll_until_settled(&client, &book_name);
                    drop(slot);
                });
            }
        }
    }

    Ok(reply)
}

/// 查 book-serve 的 `/staging`（直连后端服务，不经网关自己这层转发，避免自己调自己）直到
/// [`crate::budget::is_settled`] 判定这本书已经不再忙，或等到 [`SETTLE_POLL_TIMEOUT`] 放弃。
/// **事件驱动**：每次查完就阻塞等 [`crate::events::books_wake`]（book-serve 有事件才醒），至多 [`POLL_FALLBACK`] 兜底一次；
/// 先取代数再查，查询期间到达的事件不会漏。
/// 查询失败要**连续** [`MAX_POLL_FAILURES`] 次才放弃（每次隔 [`FAILURE_RETRY`]）：此前一次失败就放名额，
/// 而大书优化时 book-serve 正忙、10 秒查询超时恰恰最容易撞上，于是第二本大书被放进来、内存照样叠加——
/// 闸门在最该起作用的时候失效（2026-09-24 审查）。连续失败才说明服务真的挂了/重启了（任务随之没了），
/// 这时再放名额，不让侦测本身不可靠把并发档位永久卡住。
pub(crate) fn poll_until_settled(client: &SvcClient, name: &str) {
    let wake = crate::events::books_wake();
    wait_settled(|| client.get_json("/staging"), name, Instant::now() + SETTLE_POLL_TIMEOUT, || wake.generation(), |seen, d| { wake.wait_change(seen, d); });
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
    fn gated_operation_matches_exactly_three_routes() {
        assert_eq!(gated_operation("book-serve", "staging/optimize", Method::Post), Some(GatedOp::Optimize));
        assert_eq!(gated_operation("book-serve", "staging/deliver", Method::Post), Some(GatedOp::Deliver));
        assert_eq!(gated_operation("koreader-serve", "books/adopt", Method::Post), Some(GatedOp::KoreaderAdopt));
    }

    #[test]
    fn gated_operation_ignores_everything_else() {
        assert_eq!(gated_operation("book-serve", "staging", Method::Post), None, "落库入库本身走多文件上传，不该被拦下来读 body");
        assert_eq!(gated_operation("book-serve", "staging", Method::Get), None, "列表查询不限流");
        assert_eq!(gated_operation("book-serve", "staging/optimize", Method::Get), None, "方法不对不该命中");
        assert_eq!(gated_operation("koreader-serve", "books", Method::Get), None);
        assert_eq!(gated_operation("font-serve", "staging/optimize", Method::Post), None, "服务名对不上不该误命中");
    }

    /// 回归：一次查询失败（大书优化时 book-serve 忙、查询超时）不能提前放名额，要等它真的不忙。
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

    /// 连续失败到上限 → 放弃（服务真挂了，任务已随之消失）。
    #[test]
    fn persistent_failure_gives_up_after_limit() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        wait_settled(|| { calls.set(calls.get() + 1); Err("down".into()) }, "x", Instant::now() + Duration::from_secs(3600), || 0, |_, _| {});
        assert_eq!(calls.get(), MAX_POLL_FAILURES);
    }
}
