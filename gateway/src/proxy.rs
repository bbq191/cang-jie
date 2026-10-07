//! 反向代理（Facade）：把 `/api/<seg>[/<rest>]` 转给注册表里的服务（剥掉 `<seg>`），状态码/JSON 原样回。
//! **请求方向流式**（`send(&mut *req.body)` 直接转发原始请求体读取器，上传大文件不额外占内存）。
//! **响应方向**：后端给了 `Content-Length` 的 200 应答，若是下载（带 `Content-Disposition`）或体积超过
//! [`STREAM_MIN_BYTES`]（壁纸原图等），走 `Reply::sized_stream` 按定长边读边发，不整个读进网关内存；
//! 其余（JSON 等小应答、没有长度的应答）读完再回——没有长度的流只能走 SSE 那条"读到连接关闭"的通道，
//! 不适合普通下载（见 [`stream_len`]）。
use rmsvc_core::http::{ApiError, ApiResult, Method, Reply, Request};
use rmsvc_core::multipart::percent_encode as enc;
use rmsvc_core::paths::Paths;
use rmsvc_core::registry;
use std::io::Read;

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
    // 后端会一直等那几个永远不来的字节直到超时。
    let has_body = !matches!(req.method, Method::Get | Method::Delete);
    if let Some(n) = req.content_length.filter(|_| has_body) {
        r = r.set("Content-Length", &n.to_string());
    }
    let resp = if has_body { r.send(&mut *req.body) } else { r.call() };
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

    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn streams_only_sized_downloads_or_big_bodies() {
        assert_eq!(stream_len(200, true, Some(10)), Some(10), "带长度的下载：流式");
        assert_eq!(stream_len(200, false, Some(STREAM_MIN_BYTES + 1)), Some(STREAM_MIN_BYTES + 1), "大图片：流式");
        assert_eq!(stream_len(200, false, Some(1000)), None, "小 JSON：读完再回");
        assert_eq!(stream_len(200, true, None), None, "没有长度的下载不能走读到关闭的流");
        assert_eq!(stream_len(404, true, Some(10)), None, "错误应答读完再回");
    }
}
