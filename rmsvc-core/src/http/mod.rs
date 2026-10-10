//! HTTP 适配层（唯一碰 tiny_http 的地方）。领域模块只见 [`Request`]/[`Reply`] 两个纯数据类型。
//! 路由 = (方法, 路径模式) → 处理函数；路径模式支持尾部 `/*` 前缀匹配与单段 `{param}`。
mod encoding;
mod router;
mod server;
mod test_request;
pub use encoding::{percent_decode, percent_decode_path, percent_encode};
pub use router::{encode_query, parse_query, Router};
pub use test_request::TestRequest;
pub use server::{serve, serve_with, Guard, GuardFn, GuardRequest, ServeOpts, DEFAULT_MAX_CONCURRENT};

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

    /// HTTP 方法名（`GET`/`POST`/…）；`Other` → `None`（反向代理转发时据此拒掉不认识的方法）。
    pub fn as_str(self) -> Option<&'static str> {
        Some(match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Other => return None,
        })
    }
}

/// HTML 文本/属性转义（`& < > " '`）：网关登录页、fontconfig XML 这类拼字符串的地方共用。
pub fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// 进入领域的请求视图：body 是流（大文件不读进内存）。
pub struct Request<'a> {
    pub method: Method,
    pub path: String,
    pub query: HashMap<String, String>,
    pub params: HashMap<String, String>,
    pub content_type: String,
    pub content_length: Option<usize>,
    /// 少量请求头（Cookie/Authorization/Accept/Host/X-Forwarded-Proto），登录/守卫用。
    pub headers: Vec<(String, String)>,
    pub body: &'a mut dyn Read,
}

/// 服务器写入的内部头：TCP 对端 IP。**只由服务器写**——客户端发来的同名头不在保留白名单里、进不来，
/// 所以处理函数读到的一定是真实对端地址（不是 `X-Forwarded-For` 这类可伪造的值）。
pub const REMOTE_IP_HEADER: &str = "X-Rmsvc-Remote-Ip";

/// [`Request::small_body`] 的上限（1MB）：JSON 表单这类小请求体（当年还有 KOReader 配置补丁，koreader-serve 2026-09-30 已删）。
pub const SMALL_BODY_MAX: u64 = 1024 * 1024;

/// 按名取头（不区分大小写）——[`Request`] 与 [`GuardRequest`] 共用。
fn header_of<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// JSON 请求体的取值门面：缺字段 / 类型不对统一变 400 `ApiError`，各服务不再各写一遍
/// `j.get("name").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad("缺 name"))`。
pub struct JsonBody(pub serde_json::Value);

impl JsonBody {
    /// 必填字符串字段。
    pub fn str(&self, key: &str) -> Result<&str, ApiError> {
        self.0.get(key).and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad(format!("缺 {key}")))
    }
    /// 可选字符串字段（去首尾空白；缺 / 空白 → `default`）。
    pub fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.0.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(default)
    }
    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.0.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }
    /// 必填布尔字段。
    pub fn bool(&self, key: &str) -> Result<bool, ApiError> {
        self.0.get(key).and_then(|v| v.as_bool()).ok_or_else(|| ApiError::bad(format!("缺 {key}")))
    }
    /// 可选字符串字段（原样，不去空白；缺或不是字符串 → `None`）。"没给"与"给了空串"要区分时用它。
    pub fn opt_str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.as_str())
    }
    pub fn opt_bool(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(|v| v.as_bool())
    }
    pub fn opt_u64(&self, key: &str) -> Option<u64> {
        self.0.get(key).and_then(|v| v.as_u64())
    }
    /// 可选数字字段（整数也认）；缺或不是数字 → `None`。
    pub fn opt_f64(&self, key: &str) -> Option<f64> {
        self.0.get(key).and_then(|v| v.as_f64())
    }
    /// 可选嵌套对象字段（缺或不是对象 → `None`），包成 [`JsonBody`] 接着用同一套取法。会拷一份——请求体都是小 JSON。
    pub fn opt_obj(&self, key: &str) -> Option<JsonBody> {
        self.0.get(key).filter(|v| v.is_object()).cloned().map(JsonBody)
    }
    /// 字符串数组字段（缺 → 空；非字符串元素跳过）。
    pub fn str_list(&self, key: &str) -> Vec<String> {
        self.opt_str_list(key).unwrap_or_default()
    }
    /// 可选字符串数组字段：缺或不是数组 → `None`；是数组 → `Some`（非字符串元素跳过，空数组得 `Some(空)`）。
    /// "没给"与"给了空数组"要区分时用它（如批量投递：没给 names 才看 all）。
    pub fn opt_str_list(&self, key: &str) -> Option<Vec<String>> {
        self.0.get(key).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
    }
}

impl Request<'_> {
    /// 按名取头（不区分大小写）。
    pub fn header(&self, name: &str) -> Option<&str> {
        header_of(&self.headers, name)
    }
    /// TCP 对端 IP（见 [`REMOTE_IP_HEADER`]）；测试里手工构造、没填这个头时为 `None`。
    pub fn remote_ip(&self) -> Option<std::net::IpAddr> {
        self.header(REMOTE_IP_HEADER).and_then(|v| v.parse().ok())
    }
    /// 查询参数是否为真（`1` / `true`）。
    pub fn q_flag(&self, k: &str) -> bool {
        matches!(self.q(k), Some("1") | Some("true"))
    }
    pub fn q(&self, k: &str) -> Option<&str> {
        self.query.get(k).map(|s| s.as_str())
    }
    /// 必填查询参数（缺或空白 → 400 "缺 <k>"）。
    pub fn q_required(&self, k: &str) -> Result<&str, ApiError> {
        self.q(k).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| ApiError::bad(format!("缺 {k}")))
    }
    /// 解析查询参数（`?limit=50`）：缺 → `None`；有但解析不了 → `None`（与缺省同处理，调用方 `unwrap_or(默认)`）。
    pub fn q_parse<T: std::str::FromStr>(&self, k: &str) -> Option<T> {
        self.q(k).and_then(|v| v.trim().parse().ok())
    }
    pub fn param(&self, k: &str) -> &str {
        self.params.get(k).map(|s| s.as_str()).unwrap_or("")
    }
    /// 小 body（JSON 表单 / 配置补丁）整体读入，上限 [`SMALL_BODY_MAX`]。**超限报错**而不是截断：此前
    /// `take(1MB)` 静默截断，超长的 KOReader 配置补丁会被切成半截再交给合并脚本、JSON 报一句莫名的解析错
    /// （2026-09-24 审计）。多读 1 字节即可判定超限，不必读完整个超长 body。
    /// 超过 [`SMALL_BODY_MAX`] → 413，读失败（客户端半路断开/读超时）→ 400（2026-10-10 删掉了返回 `String` 的
    /// `read_small_body`/`json_body`/`form_body`，那几个拿不到 413）。
    pub fn small_body(&mut self) -> Result<Vec<u8>, ApiError> {
        let too_big = || ApiError::too_large(format!("请求体超过 {} KB 上限", SMALL_BODY_MAX / 1024));
        if self.content_length.is_some_and(|n| n as u64 > SMALL_BODY_MAX) {
            return Err(too_big());
        }
        let mut v = Vec::new();
        self.body.take(SMALL_BODY_MAX + 1).read_to_end(&mut v).map_err(|e| ApiError::bad(e.to_string()))?;
        if v.len() as u64 > SMALL_BODY_MAX {
            return Err(too_big());
        }
        Ok(v)
    }
    /// JSON body → `Value`；超限 413，读失败 / 不是合法 JSON → 400。
    pub fn json_value(&mut self) -> Result<serde_json::Value, ApiError> {
        let b = self.small_body()?;
        serde_json::from_slice(&b).map_err(|e| ApiError::bad(format!("JSON 解析失败: {e}")))
    }
    /// `application/x-www-form-urlencoded` 表单 → map；超限 413，读失败 400。
    pub fn form(&mut self) -> Result<HashMap<String, String>, ApiError> {
        let b = self.small_body()?;
        Ok(parse_query(&String::from_utf8_lossy(&b).replace('+', " ")))
    }
    /// JSON body → [`JsonBody`]（解析失败 400；超过 [`SMALL_BODY_MAX`] 413——2026-10-10 前超限也报 400）。
    pub fn json(&mut self) -> Result<JsonBody, ApiError> {
        self.json_value().map(JsonBody)
    }
    /// multipart 请求的 boundary；非 multipart → 400。
    pub fn multipart_boundary(&self) -> Result<String, ApiError> {
        crate::multipart::boundary_of(&self.content_type).ok_or_else(|| ApiError::bad("需要 multipart/form-data"))
    }
}

pub struct Reply {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
    /// 流式响应体（SSE 等）：Some 时忽略 `body`，chunked 边读边发，直到 reader 返回 0 或客户端断开。
    pub stream: Option<Box<dyn Read + Send>>,
}

impl Reply {
    pub fn json<T: Serialize>(status: u16, v: &T) -> Reply {
        Reply { status, content_type: "application/json; charset=utf-8".into(), body: serde_json::to_vec(v).unwrap_or_default(), headers: vec![], stream: None }
    }
    pub fn ok<T: Serialize>(v: &T) -> Reply {
        Reply::json(200, v)
    }
    pub fn error(status: u16, message: impl Into<String>) -> Reply {
        Reply::json(status, &serde_json::json!({"ok": false, "message": message.into()}))
    }
    pub fn html(body: &str) -> Reply {
        Reply { status: 200, content_type: "text/html; charset=utf-8".into(), body: body.as_bytes().to_vec(), headers: vec![], stream: None }
    }
    pub fn bytes(content_type: &str, body: Vec<u8>) -> Reply {
        Reply { status: 200, content_type: content_type.into(), body, headers: vec![], stream: None }
    }
    /// 流式响应（SSE）：不知长度，读到 reader 结束或客户端断开为止。
    pub fn stream(content_type: &str, reader: Box<dyn Read + Send>) -> Reply {
        Reply { status: 200, content_type: content_type.into(), body: Vec::new(), headers: vec![], stream: Some(reader) }
    }
    /// 已知长度的流（文件下载）：带 `Content-Length`，服务器按定长响应边读边发，发完即结束（不走 SSE 那条路）。
    pub fn sized_stream(content_type: &str, reader: Box<dyn Read + Send>, len: u64) -> Reply {
        Reply::stream(content_type, reader).with_header("Content-Length", &len.to_string())
    }
    pub fn not_found() -> Reply {
        Reply::error(404, "not found")
    }
    /// 303 跳转（表单提交后用 303 避免重复提交）。
    pub fn redirect(location: &str) -> Reply {
        Reply { status: 303, content_type: "text/plain; charset=utf-8".into(), body: Vec::new(), headers: vec![("Location".into(), location.into())], stream: None }
    }
    pub fn with_header(mut self, k: &str, v: &str) -> Reply {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn with_status(mut self, status: u16) -> Reply {
        self.status = status;
        self
    }
}

/// 领域错误 → 回执：`Err(ApiError)` 统一变 JSON。
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}
impl ApiError {
    /// 任意状态码（下面几个具名构造器覆盖不到的，比如代理把上游的状态码原样透传）。
    pub fn new(status: u16, m: impl Into<String>) -> ApiError {
        ApiError { status, message: m.into() }
    }
    /// 409：与现有状态冲突（同名已存在、正在处理中）。
    pub fn conflict(m: impl Into<String>) -> ApiError {
        ApiError::new(409, m)
    }
    /// 413：请求体超过上限。
    pub fn too_large(m: impl Into<String>) -> ApiError {
        ApiError::new(413, m)
    }
    /// 502：调用的下游服务 / xochitl 回了错误或不像样的应答。
    pub fn bad_gateway(m: impl Into<String>) -> ApiError {
        ApiError::new(502, m)
    }
    /// 503：下游服务没在运行 / 暂时不可用（连不上）。
    pub fn unavailable(m: impl Into<String>) -> ApiError {
        ApiError::new(503, m)
    }
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

/// 把共享状态绑进处理函数：`router.get("/x", bind(&st, |st, r| …))`。
/// 替代各服务 main 里 `let (s1, s2, s3, …) = (st.clone(), …)` 的手工克隆串。
pub fn bind<S, F>(state: &Arc<S>, f: F) -> impl Fn(&mut Request<'_>) -> ApiResult + Send + Sync + 'static
where
    S: Send + Sync + 'static,
    F: Fn(&S, &mut Request<'_>) -> ApiResult + Send + Sync + 'static,
{
    let st = state.clone();
    move |r| f(&st, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_names_escape_and_more_json_accessors() {
        assert_eq!(Method::Delete.as_str(), Some("DELETE"));
        assert_eq!(Method::Other.as_str(), None);
        assert_eq!(html_escape(r#"<a href="x">'&'</a>"#), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
        let b = JsonBody(serde_json::json!({"s": " x ", "b": true, "n": 7, "l": ["a", 1, "b"]}));
        assert_eq!(b.opt_str("s"), Some(" x "));
        assert_eq!(b.opt_str("nope"), None);
        assert_eq!((b.opt_bool("b"), b.opt_u64("n"), b.opt_u64("s")), (Some(true), Some(7), None));
        assert!(b.bool("b").unwrap() && b.bool("n").is_err());
        assert_eq!(b.str_list("l"), ["a", "b"]);
        assert!(b.str_list("nope").is_empty());
        assert_eq!((b.opt_f64("n"), b.opt_f64("s")), (Some(7.0), None));
        let nested = JsonBody(serde_json::json!({"p": {"x": 0.5}, "s": "x"}));
        assert_eq!(nested.opt_obj("p").and_then(|p| p.opt_f64("x")), Some(0.5));
        assert!(nested.opt_obj("s").is_none() && nested.opt_obj("nope").is_none(), "不是对象 / 没给 → None");
        assert_eq!(b.opt_str_list("l"), Some(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(b.opt_str_list("nope"), None, "没给 → None");
        assert_eq!(b.opt_str_list("s"), None, "不是数组 → None");
        assert_eq!(JsonBody(serde_json::json!({"l": []})).opt_str_list("l"), Some(vec![]), "空数组也算给了");
        TestRequest::new(Method::Get, "/").query_string("limit=50&bad=x&blank=%20").with(|r| {
            assert_eq!(r.q_parse::<usize>("limit"), Some(50));
            assert_eq!(r.q_parse::<usize>("bad"), None);
            assert_eq!(r.q_required("limit").unwrap(), "50");
            assert_eq!(r.q_required("blank").unwrap_err().message, "缺 blank");
        });
    }

    #[test]
    fn json_body_accessors() {
        let b = JsonBody(serde_json::json!({"name": "x", "folder": "  ", "keep": false}));
        assert_eq!(b.str("name").unwrap(), "x");
        assert_eq!(b.str("nope").unwrap_err().message, "缺 nope");
        assert_eq!(b.str_or("folder", "lib"), "lib", "空白当缺省");
        assert!(!b.bool_or("keep", true) && b.bool_or("other", true));
    }

    fn req_with<'a>(body: &'a mut &[u8], content_length: Option<usize>) -> Request<'a> {
        Request { method: Method::Post, path: "/".into(), query: HashMap::new(), params: HashMap::new(), content_type: String::new(), content_length, headers: vec![], body }
    }

    /// 回归：超过上限的小 body 报错，而不是静默截断成半截内容交给调用方。
    #[test]
    fn small_body_rejects_oversize_instead_of_truncating() {
        let max = SMALL_BODY_MAX as usize;
        let exact = vec![b'a'; max];
        let mut r: &[u8] = &exact;
        assert_eq!(req_with(&mut r, None).small_body().unwrap().len(), max, "恰好上限照收");
        let over = vec![b'a'; max + 1];
        let mut r: &[u8] = &over;
        assert!(req_with(&mut r, None).small_body().unwrap_err().message.contains("上限"), "没有 Content-Length（chunked）也按实际字节判");
        let mut r: &[u8] = b"{}";
        assert!(req_with(&mut r, Some(max + 1)).small_body().is_err(), "声明长度超限直接拒，不读 body");
        let mut r: &[u8] = b"{\"a\":1}";
        assert_eq!(req_with(&mut r, Some(7)).json().unwrap().0["a"], 1, "正常小 body 不受影响");
    }

    /// 取 body 辅助：超限 413、坏 JSON 400。
    #[test]
    fn api_body_helpers_give_413_for_oversize_and_400_for_bad_json() {
        let max = SMALL_BODY_MAX as usize;
        let over = vec![b'a'; max + 1];
        let mut r: &[u8] = &over;
        assert_eq!(req_with(&mut r, None).small_body().unwrap_err().status, 413);
        let mut r: &[u8] = b"{}";
        assert_eq!(req_with(&mut r, Some(max + 1)).json().err().map(|e| e.status), Some(413), "json() 超限也是 413");
        let mut r: &[u8] = b"{}";
        assert_eq!(req_with(&mut r, Some(max + 1)).form().unwrap_err().status, 413);
        let mut r: &[u8] = b"{half";
        let e = req_with(&mut r, None).json_value().unwrap_err();
        assert!(e.status == 400 && e.message.starts_with("JSON 解析失败"), "{e:?}");
        let mut r: &[u8] = b"a=1+2&b=x";
        assert_eq!(req_with(&mut r, None).form().unwrap()["a"], "1 2");
        assert_eq!((ApiError::new(418, "x").status, ApiError::conflict("x").status, ApiError::too_large("x").status), (418, 409, 413));
        assert_eq!((ApiError::bad_gateway("x").status, ApiError::unavailable("x").status), (502, 503));
    }
}
