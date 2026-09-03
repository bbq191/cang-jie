//! HTTP 适配层（唯一碰 tiny_http 的地方）。领域模块只见 [`Request`]/[`Reply`] 两个纯数据类型。
//! 路由 = (方法, 路径模式) → 处理函数；路径模式支持尾部 `/*` 前缀匹配与单段 `{param}`。
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Options,
    Other,
}

impl Method {
    fn from_tiny(m: &tiny_http::Method) -> Method {
        match m {
            tiny_http::Method::Get => Method::Get,
            tiny_http::Method::Post => Method::Post,
            tiny_http::Method::Put => Method::Put,
            tiny_http::Method::Delete => Method::Delete,
            tiny_http::Method::Options => Method::Options,
            _ => Method::Other,
        }
    }
}

/// 进入领域的请求视图：body 是流（大文件不读进内存）。
pub struct Request<'a> {
    pub method: Method,
    pub path: String,
    pub query: HashMap<String, String>,
    pub params: HashMap<String, String>,
    pub content_type: String,
    pub content_length: Option<usize>,
    pub body: &'a mut dyn Read,
}

impl Request<'_> {
    pub fn q(&self, k: &str) -> Option<&str> {
        self.query.get(k).map(|s| s.as_str())
    }
    pub fn param(&self, k: &str) -> &str {
        self.params.get(k).map(|s| s.as_str()).unwrap_or("")
    }
    /// 小 body（JSON 表单）整体读入，上限 1MB。
    pub fn read_small_body(&mut self) -> Result<Vec<u8>, String> {
        let mut v = Vec::new();
        self.body.take(1024 * 1024).read_to_end(&mut v).map_err(|e| e.to_string())?;
        Ok(v)
    }
    pub fn json_body(&mut self) -> Result<serde_json::Value, String> {
        let b = self.read_small_body()?;
        serde_json::from_slice(&b).map_err(|e| format!("JSON 解析失败: {e}"))
    }
}

pub struct Reply {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl Reply {
    pub fn json<T: Serialize>(status: u16, v: &T) -> Reply {
        Reply { status, content_type: "application/json; charset=utf-8".into(), body: serde_json::to_vec(v).unwrap_or_default(), headers: vec![] }
    }
    pub fn ok<T: Serialize>(v: &T) -> Reply {
        Reply::json(200, v)
    }
    pub fn error(status: u16, message: impl Into<String>) -> Reply {
        Reply::json(status, &serde_json::json!({"ok": false, "message": message.into()}))
    }
    pub fn html(body: &str) -> Reply {
        Reply { status: 200, content_type: "text/html; charset=utf-8".into(), body: body.as_bytes().to_vec(), headers: vec![] }
    }
    pub fn bytes(content_type: &str, body: Vec<u8>) -> Reply {
        Reply { status: 200, content_type: content_type.into(), body, headers: vec![] }
    }
    pub fn not_found() -> Reply {
        Reply::error(404, "not found")
    }
}

/// 领域错误 → 回执：`Err(ApiError)` 统一变 JSON。
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}
impl ApiError {
    pub fn bad(m: impl Into<String>) -> ApiError {
        ApiError { status: 400, message: m.into() }
    }
    pub fn internal(m: impl Into<String>) -> ApiError {
        ApiError { status: 500, message: m.into() }
    }
    pub fn not_found(m: impl Into<String>) -> ApiError {
        ApiError { status: 404, message: m.into() }
    }
}
impl From<String> for ApiError {
    fn from(m: String) -> Self {
        ApiError::internal(m)
    }
}
impl From<ApiError> for Reply {
    fn from(e: ApiError) -> Reply {
        Reply::error(e.status, e.message)
    }
}
pub type ApiResult = Result<Reply, ApiError>;

pub type Handler = Arc<dyn Fn(&mut Request<'_>) -> ApiResult + Send + Sync>;

struct Route {
    method: Method,
    pattern: Vec<String>, // 分段；"*" 尾通配；"{x}" 参数
    prefix: bool,
    handler: Handler,
}

#[derive(Default, Clone)]
pub struct Router {
    routes: Vec<Arc<Route>>,
}

impl Router {
    pub fn new() -> Router {
        Router::default()
    }
    pub fn route<F>(mut self, method: Method, pattern: &str, f: F) -> Router
    where
        F: Fn(&mut Request<'_>) -> ApiResult + Send + Sync + 'static,
    {
        let mut segs: Vec<String> = pattern.trim_matches('/').split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
        let prefix = segs.last().map(|s| s == "*").unwrap_or(false);
        if prefix {
            segs.pop();
        }
        self.routes.push(Arc::new(Route { method, pattern: segs, prefix, handler: Arc::new(f) }));
        self
    }
    pub fn get<F>(self, p: &str, f: F) -> Router
    where
        F: Fn(&mut Request<'_>) -> ApiResult + Send + Sync + 'static,
    {
        self.route(Method::Get, p, f)
    }
    pub fn post<F>(self, p: &str, f: F) -> Router
    where
        F: Fn(&mut Request<'_>) -> ApiResult + Send + Sync + 'static,
    {
        self.route(Method::Post, p, f)
    }
    pub fn put<F>(self, p: &str, f: F) -> Router
    where
        F: Fn(&mut Request<'_>) -> ApiResult + Send + Sync + 'static,
    {
        self.route(Method::Put, p, f)
    }
    pub fn delete<F>(self, p: &str, f: F) -> Router
    where
        F: Fn(&mut Request<'_>) -> ApiResult + Send + Sync + 'static,
    {
        self.route(Method::Delete, p, f)
    }

    /// 追加另一组路由（保持各自顺序，self 的在前）。
    pub fn merge(mut self, other: Router) -> Router {
        self.routes.extend(other.routes);
        self
    }

    fn matches(route: &Route, method: Method, path: &str) -> Option<HashMap<String, String>> {
        if route.method != method {
            return None;
        }
        let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        if route.prefix {
            if segs.len() < route.pattern.len() {
                return None;
            }
        } else if segs.len() != route.pattern.len() {
            return None;
        }
        let mut params = HashMap::new();
        for (pat, seg) in route.pattern.iter().zip(segs.iter()) {
            if pat.starts_with('{') && pat.ends_with('}') {
                params.insert(pat[1..pat.len() - 1].to_string(), crate::multipart::percent_decode(seg));
            } else if pat != seg {
                return None;
            }
        }
        if route.prefix {
            params.insert("*".into(), segs[route.pattern.len()..].join("/"));
        }
        Some(params)
    }

    /// 分发（纯函数，可单测）。
    pub fn dispatch(&self, req: &mut Request<'_>) -> Reply {
        let mut path_exists = false;
        for r in &self.routes {
            if let Some(params) = Self::matches(r, req.method, &req.path) {
                req.params = params;
                return match (r.handler)(req) {
                    Ok(rep) => rep,
                    Err(e) => e.into(),
                };
            }
            // 405 判断：同路径其它方法
            let mut probe = Route { method: req.method, pattern: r.pattern.clone(), prefix: r.prefix, handler: r.handler.clone() };
            probe.method = req.method;
            if Self::matches(&probe, req.method, &req.path).is_some() {
                path_exists = true;
            }
        }
        if req.method == Method::Options {
            return Reply { status: 204, content_type: "text/plain".into(), body: vec![], headers: vec![] };
        }
        if path_exists {
            Reply::error(405, "method not allowed")
        } else {
            Reply::not_found()
        }
    }
}

/// 解析查询串（百分号解码）。
pub fn parse_query(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (crate::multipart::percent_decode(k), crate::multipart::percent_decode(v))
        })
        .collect()
}

/// 起阻塞服务器：每请求一线程（上传大文件不阻塞其它请求）。永不返回（bind 失败返回 Err）。
pub fn serve(bind: &str, router: Router) -> Result<(), String> {
    let server = tiny_http::Server::http(bind).map_err(|e| format!("绑定 {bind} 失败: {e}"))?;
    let router = Arc::new(router);
    for mut req in server.incoming_requests() {
        let router = router.clone();
        std::thread::spawn(move || {
            let url = req.url().to_string();
            let (path, query) = url.split_once('?').unwrap_or((&url, ""));
            let method = Method::from_tiny(req.method());
            let mut content_type = String::new();
            let mut content_length = None;
            for h in req.headers() {
                if h.field.equiv("Content-Type") {
                    content_type = h.value.as_str().to_string();
                } else if h.field.equiv("Content-Length") {
                    content_length = h.value.as_str().parse().ok();
                }
            }
            let path = path.to_string();
            let query = parse_query(query);
            let reply = {
                let mut body = req.as_reader();
                let mut r = Request { method, path, query, params: HashMap::new(), content_type, content_length, body: &mut body };
                router.dispatch(&mut r)
            };
            let mut resp = tiny_http::Response::from_data(reply.body).with_status_code(reply.status);
            if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Type"[..], reply.content_type.as_bytes()) {
                resp = resp.with_header(h);
            }
            if let Ok(h) = tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]) {
                resp = resp.with_header(h);
            }
            for (k, v) in reply.headers {
                if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                    resp = resp.with_header(h);
                }
            }
            let _ = req.respond(resp);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(router: &Router, m: Method, path: &str, q: &str) -> (u16, String) {
        let mut empty: &[u8] = b"";
        let mut r = Request { method: m, path: path.into(), query: parse_query(q), params: HashMap::new(), content_type: String::new(), content_length: None, body: &mut empty };
        let rep = router.dispatch(&mut r);
        (rep.status, String::from_utf8_lossy(&rep.body).to_string())
    }

    #[test]
    fn merge_keeps_front_routes_first() {
        let a = Router::new().get("/health", |_| Ok(Reply::ok(&serde_json::json!({"h": 1}))));
        let b = Router::new().get("/{name}", |r| Ok(Reply::ok(&serde_json::json!({"name": r.param("name")}))));
        let r = a.merge(b);
        assert_eq!(call(&r, Method::Get, "/health", "").1, r#"{"h":1}"#);
        assert_eq!(call(&r, Method::Get, "/x.png", "").1, r#"{"name":"x.png"}"#);
    }

    #[test]
    fn routes_params_prefix_and_errors() {
        let router = Router::new()
            .get("/api/fonts", |_| Ok(Reply::ok(&serde_json::json!({"n": 1}))))
            .delete("/api/fonts/{file}", |r| Ok(Reply::ok(&serde_json::json!({"file": r.param("file")}))))
            .get("/api/proxy/*", |r| Ok(Reply::ok(&serde_json::json!({"rest": r.param("*")}))))
            .post("/api/x", |r| Err(ApiError::bad(format!("q={}", r.q("t").unwrap_or("")))));
        assert_eq!(call(&router, Method::Get, "/api/fonts", "").0, 200);
        assert_eq!(call(&router, Method::Delete, "/api/fonts/%E5%AD%97.ttf", "").1, r#"{"file":"字.ttf"}"#);
        assert_eq!(call(&router, Method::Get, "/api/proxy/a/b/c", "").1, r#"{"rest":"a/b/c"}"#);
        assert_eq!(call(&router, Method::Post, "/api/x", "t=1%202"), (400, r#"{"message":"q=1 2","ok":false}"#.into()));
        assert_eq!(call(&router, Method::Post, "/api/fonts", "").0, 405);
        assert_eq!(call(&router, Method::Get, "/nope", "").0, 404);
        assert_eq!(call(&router, Method::Options, "/whatever", "").0, 204);
    }
}
