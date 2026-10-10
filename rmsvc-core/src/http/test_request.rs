//! 构造 [`Request`] 的 Builder（测试用，也给下游服务的测试用，所以不是 `cfg(test)`）。
//! [`Request`] 只能用 8 个字段的结构体字面量造，此前十几处测试各手写一遍，加一个字段就要改遍全仓库
//! （这也是"服务器内部头 `REMOTE_IP_HEADER`"而不是新字段的原因之一，2026-10-10 审计 CORE-9）。这里攒齐字段，
//! 最后借出一个 `&mut Request` 给处理函数 / 路由用：
//!
//! ```
//! use rmsvc_core::http::{Method, Router, TestRequest, Reply};
//! let router = Router::new().get("/x", |r| Ok(Reply::ok(&r.q("a"))));
//! let reply = TestRequest::new(Method::Get, "/x").query("a", "1").dispatch(&router);
//! assert_eq!(reply.body.as_bytes(), br#""1""#);
//! ```
use super::{Method, Reply, Request, Router, REMOTE_IP_HEADER};
use std::collections::HashMap;

pub struct TestRequest {
    method: Method,
    path: String,
    query: HashMap<String, String>,
    params: HashMap<String, String>,
    content_type: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    content_length: Option<usize>,
}

impl TestRequest {
    pub fn new(method: Method, path: &str) -> TestRequest {
        TestRequest { method, path: path.to_string(), query: HashMap::new(), params: HashMap::new(), content_type: String::new(), headers: vec![], body: vec![], content_length: Some(0) }
    }

    /// 查询参数（已解码的值，同 `parse_query` 的结果）。
    pub fn query(mut self, k: &str, v: &str) -> TestRequest {
        self.query.insert(k.to_string(), v.to_string());
        self
    }

    /// 整串查询（`a=1&b=%20`，同服务器那样用 `parse_query` 解码），并进已有的参数。
    pub fn query_string(mut self, q: &str) -> TestRequest {
        self.query.extend(super::parse_query(q));
        self
    }

    /// 路径参数（`{name}`）：直接调处理函数、不经路由时用；经 [`Self::dispatch`] 时路由会按路径重填。
    pub fn param(mut self, k: &str, v: &str) -> TestRequest {
        self.params.insert(k.to_string(), v.to_string());
        self
    }

    pub fn header(mut self, k: &str, v: &str) -> TestRequest {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }

    /// TCP 对端 IP（服务器写的内部头 [`REMOTE_IP_HEADER`]）。
    pub fn remote_ip(self, ip: std::net::IpAddr) -> TestRequest {
        self.header(REMOTE_IP_HEADER, &ip.to_string())
    }

    pub fn content_type(mut self, ct: &str) -> TestRequest {
        self.content_type = ct.to_string();
        self
    }

    /// 请求体；`Content-Length` 跟着设成它的长度（要模拟 chunked / 谎报长度用 [`Self::content_length`]）。
    pub fn body(mut self, b: impl Into<Vec<u8>>) -> TestRequest {
        self.body = b.into();
        self.content_length = Some(self.body.len());
        self
    }

    /// JSON 请求体（同时设 `Content-Type: application/json`）。
    pub fn json(self, v: &serde_json::Value) -> TestRequest {
        self.content_type("application/json").body(v.to_string())
    }

    pub fn content_length(mut self, n: Option<usize>) -> TestRequest {
        self.content_length = n;
        self
    }

    /// 借出一个 [`Request`] 给 `f`（处理函数、取值辅助……）。
    pub fn with<R>(&mut self, f: impl FnOnce(&mut Request<'_>) -> R) -> R {
        let mut body: &[u8] = &self.body;
        let mut req = Request {
            method: self.method,
            path: self.path.clone(),
            query: self.query.clone(),
            params: self.params.clone(),
            content_type: self.content_type.clone(),
            content_length: self.content_length,
            headers: self.headers.clone(),
            body: &mut body,
        };
        f(&mut req)
    }

    /// 交给路由分发，返回回执。
    pub fn dispatch(mut self, router: &Router) -> Reply {
        self.with(|r| router.dispatch(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_request_with_all_fields() {
        let mut t = TestRequest::new(Method::Post, "/p")
            .query_string("k=x&z=%E4%B8%AD")
            .query("k", "v")
            .param("id", "7")
            .header("Cookie", "a=b")
            .remote_ip(std::net::Ipv4Addr::LOCALHOST.into())
            .json(&serde_json::json!({"x": 1}));
        t.with(|r| {
            assert_eq!((r.method, r.path.as_str(), r.q("k"), r.param("id"), r.header("cookie")), (Method::Post, "/p", Some("v"), "7", Some("a=b")));
            assert_eq!(r.remote_ip(), Some(std::net::Ipv4Addr::LOCALHOST.into()));
            assert_eq!(r.q("z"), Some("中"));
            assert_eq!(r.content_type, "application/json");
            assert_eq!(r.content_length, Some(7));
            assert_eq!(r.json().unwrap().0["x"], 1);
        });
        // 可以借出多次，每次 body 从头读
        assert_eq!(t.with(|r| r.small_body().unwrap()), br#"{"x":1}"#);
        let router = Router::new().get("/items/{id}", |r| Ok(Reply::ok(&r.param("id"))));
        assert_eq!(TestRequest::new(Method::Get, "/items/42").dispatch(&router).body.as_bytes(), br#""42""#);
        assert_eq!(TestRequest::new(Method::Get, "/none").dispatch(&router).status, 404);
    }
}
