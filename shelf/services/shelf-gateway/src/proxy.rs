//! 反向代理（Facade）：把 `/api/<seg>[/<rest>]` 转给注册表里的服务（剥掉 `<seg>`），body 流式透传、状态码/JSON 原样回。
use shelf_core::http::{ApiError, ApiResult, Method, Reply, Request};
use shelf_core::paths::Paths;
use shelf_core::registry;
use std::io::Read;

pub fn forward(paths: &Paths, service: Option<&'static str>, req: &mut Request<'_>) -> ApiResult {
    let Some(name) = service else { return Err(ApiError::not_found("未知服务")) };
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
    if let Some(n) = req.content_length {
        r = r.set("Content-Length", &n.to_string());
    }
    let resp = if matches!(req.method, Method::Get | Method::Delete) { r.call() } else { r.send(&mut *req.body) };
    let (status, resp) = match resp {
        Ok(r) => (r.status(), r),
        Err(ureq::Error::Status(c, r)) => (c, r),
        Err(e) => return Err(ApiError { status: 502, message: format!("{name} 无响应: {e}") }),
    };
    let ctype = resp.header("Content-Type").unwrap_or("application/octet-stream").to_string();
    let mut body = Vec::new();
    resp.into_reader().read_to_end(&mut body).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Reply { status, content_type: ctype, body, headers: vec![] })
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_query() {
        assert_eq!(super::enc("a b/中"), "a%20b%2F%E4%B8%AD");
    }
}
