//! 网关登录策略（HTTP 层只见 [`Guard`]）：
//! - 公开：`/login`、`/logout`、`/ca.crt`、`/health`、`/favicon.ico`。
//! - 会话 Cookie `shelf_session`（HttpOnly；HTTPS 时 Secure；SameSite=Strict）——网页登录后凭它。
//! - `Authorization: Basic *:<密码>`（用户名任意）——CLI 用，免登录页。
//! - **首登必改**：`mustChangePassword` 时会话只能访问 `/password`（网页 303 过去、API 回 403）；
//!   Basic 同理只放行 `POST /password`（设备上 `gateway passwd`）。
//! - 未登录：浏览器请求（Accept 含 text/html）303 → `/login?next=…`，其它 401 JSON。密码错延时 500ms。
use crate::config::GatewayConfig;
use rmsvc_core::auth::{parse_basic, parse_cookie, IpFailLimiter, SessionStore};
use rmsvc_core::http::{ApiError, ApiResult, Guard, GuardRequest, Method, Reply, Request};
use rmsvc_core::paths::Paths;
use std::sync::{Arc, Mutex};

pub const COOKIE: &str = "shelf_session";

pub struct AuthState {
    pub cfg: Mutex<GatewayConfig>,
    pub sessions: SessionStore,
    pub paths: Paths,
    pub secure_cookie: bool,
    /// 密码校验失败限速：按来源 IP 各自 [`LOGIN_MAX_FAILS`] 次 / [`LOGIN_FAIL_WINDOW`]，最多记 [`LOGIN_LIMITER_IPS`] 个 IP。
    pub limiter: IpFailLimiter,
    /// Basic 认证"刚验过的密码"缓存，见 [`BasicCache`]。
    basic: BasicCache,
}

/// Basic 认证每个请求都带密码，此前每个请求都跑一遍 60 万轮 PBKDF2（设备上数百毫秒、占满一个核）——`curl` 脚本
/// 连发十几个请求就是好几秒纯 CPU、整机耗电。这里只记**最近一次验证通过**的那个密码的带密钥摘要
/// `SHA-256(本进程随机密钥 ‖ 密码)` 与时间：同一密码在 [`BASIC_CACHE_TTL`] 内再来，常数时间比对摘要即可，不重算 PBKDF2。
///
/// 安全取舍：① 只缓存**验证成功**的结果——错误密码照旧走 PBKDF2 + 失败限速 + 500ms 延时，缓存不给暴力猜测提供任何
/// 加速；② 不存明文，摘要带进程启动时从 `/dev/urandom` 取的 32 字节密钥（不落盘、重启即换），拿不到密钥就没法把摘要
/// 当成离线快速验证的靶子；③ 改密码（[`AuthState::change_password`]）立即清空，旧密码一刻都不再被接受；
/// ④ 只有一格：换个密码来试只会覆盖/错过这一格，不会攒出一张表。能读到网关进程内存的人本来就拿得到会话令牌，
/// 这层缓存不扩大这类攻击面。
struct BasicCache {
    key: [u8; 32],
    last: std::sync::Mutex<Option<([u8; 32], std::time::Instant)>>,
}

/// 同一 Basic 密码免重算 PBKDF2 的时长。只影响 CLI/脚本（网页走会话 Cookie，本来就不每请求验密码）。
pub const BASIC_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

impl BasicCache {
    fn new() -> BasicCache {
        use std::io::Read;
        let mut key = [0u8; 32];
        let ok = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut key)).is_ok();
        if !ok {
            // 拿不到随机源（只在极怪的环境）：退回时间+pid 派生，照样不落盘。
            use sha2::Digest;
            let seed = format!("{:?}{}", std::time::SystemTime::now(), std::process::id());
            key.copy_from_slice(&sha2::Sha256::digest(seed.as_bytes()));
        }
        BasicCache { key, last: std::sync::Mutex::new(None) }
    }

    fn digest(&self, pw: &str) -> [u8; 32] {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(self.key);
        h.update(pw.as_bytes());
        h.finalize().into()
    }

    fn hit(&self, pw: &str) -> bool {
        let d = self.digest(pw);
        let last = rmsvc_core::sync::lock(&self.last);
        // 常数时间比对（逐字节异或累加），不因前缀相同与否泄露时序。
        last.as_ref().is_some_and(|(cached, at)| at.elapsed() < BASIC_CACHE_TTL && cached.iter().zip(d.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0)
    }

    fn remember(&self, pw: &str) {
        *rmsvc_core::sync::lock(&self.last) = Some((self.digest(pw), std::time::Instant::now()));
    }

    fn clear(&self) {
        *rmsvc_core::sync::lock(&self.last) = None;
    }
}

/// 60 秒内连续输错 5 次就锁 60 秒内的后续尝试：正常人手误几次远够用（登录页每次错还有 500ms 延时），
/// 而暴力猜密码被压到 ≈ 5 次/分钟；锁定期内根本不做 60 万轮 PBKDF2，也就不会被并发猜测烧 CPU。
pub const LOGIN_MAX_FAILS: usize = 5;
pub const LOGIN_FAIL_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
/// 限速表最多记多少个来源 IP（每项只是几个时间戳），超了淘汰最旧的（见 [`IpFailLimiter`]）。
/// 按 IP 分桶后，局域网里别人输错只锁他自己的地址，不再连带锁住主人（2026-09-24 前是全局计数）。
/// USB 网段 10.11.99.0/24 与 127.0.0.1 **不豁免**：设备的 lo 别名就是 10.11.99.1，豁免会放过一整类请求。
pub const LOGIN_LIMITER_IPS: usize = 256;

/// 取不到对端地址时（实际只在极端情况下发生）归到这一个桶里，照样限速。
fn client_ip(ip: Option<std::net::IpAddr>) -> std::net::IpAddr {
    ip.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

impl AuthState {
    pub fn new(cfg: GatewayConfig, sessions: SessionStore, paths: Paths, secure_cookie: bool) -> AuthState {
        AuthState { cfg: Mutex::new(cfg), sessions, paths, secure_cookie, limiter: IpFailLimiter::new(LOGIN_MAX_FAILS, LOGIN_FAIL_WINDOW, LOGIN_LIMITER_IPS), basic: BasicCache::new() }
    }

    /// 被限速锁定时给的 429 应答（`json` 决定 JSON 还是登录页 HTML）。
    fn locked_reply(&self, wait: std::time::Duration, json: bool, html: impl FnOnce(&str) -> String) -> Reply {
        let secs = wait.as_secs().max(1);
        let msg = format!("密码错误次数过多，请 {secs} 秒后再试");
        let r = if json { Reply::error(429, &msg) } else { Reply::html(&html(&msg)).with_status(429) };
        r.with_header("Retry-After", &secs.to_string())
    }
}

pub type Shared = Arc<AuthState>;

fn wants_html(accept: Option<&str>) -> bool {
    accept.map(|a| a.contains("text/html")).unwrap_or(false)
}

fn is_public(method: Method, path: &str) -> bool {
    matches!((method, path), (Method::Get, "/login") | (Method::Post, "/login") | (Method::Post, "/logout") | (Method::Get, "/ca.crt") | (Method::Get, "/health") | (Method::Get, "/favicon.ico"))
}

/// 通过 Cookie 或 Basic 认出的身份。
enum Who {
    Session,
    Basic,
    Nobody,
}

impl AuthState {
    fn identify(&self, r: &GuardRequest) -> Who {
        if let Some(tok) = r.header("Cookie").and_then(|c| parse_cookie(c, COOKIE)) {
            if self.sessions.check(&tok) {
                return Who::Session;
            }
        }
        if let Some((_, pw)) = r.header("Authorization").and_then(parse_basic) {
            let ip = client_ip(r.remote);
            if self.limiter.locked_for(ip).is_some() {
                return Who::Nobody; // 锁定期不做校验（见 LOGIN_MAX_FAILS）
            }
            if self.verify_basic(&pw) {
                self.limiter.reset(ip);
                return Who::Basic;
            }
            self.limiter.record_failure(ip);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Who::Nobody
    }

    /// 校验密码。**锁内只拷出哈希、PBKDF2（60 万轮，设备上数百毫秒）在锁外算**：`cfg` 锁是每个带会话的请求
    /// （`must_change()`）都要拿的，持锁算哈希会让登录尝试/Basic 校验期间所有其它请求排队等它。
    fn verify(&self, pw: &str) -> bool {
        let hash = self.cfg.lock().map(|c| c.password_hash.clone()).unwrap_or_default();
        GatewayConfig::verify_hash(&hash, pw)
    }

    /// Basic 头里的密码：先查 [`BasicCache`]，没命中才跑 PBKDF2，通过后记下。登录表单/改密的"当前密码"不走缓存。
    fn verify_basic(&self, pw: &str) -> bool {
        if self.basic.hit(pw) {
            return true;
        }
        let ok = self.verify(pw);
        if ok {
            self.basic.remember(pw);
        }
        ok
    }

    pub fn must_change(&self) -> bool {
        self.cfg.lock().map(|c| c.must_change_password).unwrap_or(false)
    }

    pub fn guard(self: &Arc<Self>) -> Guard {
        let st = self.clone();
        Guard {
            check: Arc::new(move |r: &GuardRequest| {
                if is_public(r.method, &r.path) {
                    return None;
                }
                let html = wants_html(r.header("Accept"));
                match st.identify(r) {
                    Who::Nobody => Some(if html {
                        Reply::redirect(&format!("/login?next={}", rmsvc_core::multipart::percent_encode(&r.path)))
                    } else {
                        Reply::error(401, "需要登录（网页 /login；CLI 用 Basic 密码）").with_header("WWW-Authenticate", "Basic realm=\"shelf\", charset=\"UTF-8\"")
                    }),
                    Who::Session | Who::Basic if st.must_change() && !matches!((r.method, r.path.as_str()), (_, "/password") | (Method::Get, "/api/session")) => Some(if html {
                        Reply::redirect("/password")
                    } else {
                        Reply::error(403, "首次登录必须先改密码：网页打开 /password，或在设备上运行 `gateway passwd <新密码>`")
                    }),
                    _ => None,
                }
            }),
        }
    }

    fn cookie_header(&self, token: &str, max_age: u64) -> String {
        format!("{COOKIE}={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict{}", if self.secure_cookie { "; Secure" } else { "" })
    }

    /// `POST /login`（表单或 JSON `{password}`）。
    pub fn login(&self, req: &mut Request<'_>) -> ApiResult {
        let (pw, next, json) = read_password_body(req)?;
        let ip = client_ip(req.remote_ip());
        if let Some(wait) = self.limiter.locked_for(ip) {
            return Ok(self.locked_reply(wait, json, |m| crate::ui::login_page(m, &next)));
        }
        let ok = self.verify(&pw);
        if ok {
            self.limiter.reset(ip);
        } else {
            self.limiter.record_failure(ip);
            std::thread::sleep(std::time::Duration::from_millis(500));
            return Ok(if json { Reply::error(401, "密码错误") } else { Reply::html(&crate::ui::login_page("密码错误", &next)).with_status(401) });
        }
        let tok = self.sessions.issue();
        let dest = if self.must_change() { "/password".to_string() } else if is_local_path(&next) { next } else { "/".into() };
        let cookie = self.cookie_header(&tok, self.sessions.ttl().as_secs());
        Ok(if json { Reply::ok(&serde_json::json!({"ok": true, "mustChange": self.must_change(), "next": dest})) } else { Reply::redirect(&dest) }.with_header("Set-Cookie", &cookie))
    }

    /// `POST /logout`。
    pub fn logout(&self, req: &mut Request<'_>) -> ApiResult {
        if let Some(tok) = req.header("Cookie").and_then(|c| parse_cookie(c, COOKIE)) {
            self.sessions.revoke(&tok);
        }
        Ok(Reply::redirect("/login").with_header("Set-Cookie", &self.cookie_header("", 0)))
    }

    /// `POST /password`：`current` + `new` + `confirm`（表单）或 JSON `{current,new}`。
    pub fn change_password(&self, req: &mut Request<'_>) -> ApiResult {
        let json = req.content_type.starts_with("application/json");
        let (current, new, confirm) = if json {
            let v = req.json_body().map_err(ApiError::bad)?;
            let g = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            (g("current"), g("new"), g("new"))
        } else {
            let f = req.form_body().map_err(ApiError::bad)?;
            let g = |k: &str| f.get(k).cloned().unwrap_or_default();
            (g("current"), g("new"), g("confirm"))
        };
        let ip = client_ip(req.remote_ip());
        let forced = self.must_change();
        let fail = |msg: &str, status: u16| -> ApiResult { Ok(if json { Reply::error(status, msg) } else { Reply::html(&crate::ui::password_page(msg, forced)).with_status(status) }) };
        if let Some(wait) = self.limiter.locked_for(ip) {
            return Ok(self.locked_reply(wait, json, |m| crate::ui::password_page(m, forced)));
        }
        // Basic 已证明持有当前密码；会话则必须再输一次当前密码。两次 PBKDF2 都在 cfg 锁外算（见 [`Self::verify`]）。
        let via_basic = req.header("Authorization").and_then(parse_basic).is_some_and(|(_, p)| self.verify_basic(&p));
        if !via_basic && !self.verify(&current) {
            self.limiter.record_failure(ip);
            std::thread::sleep(std::time::Duration::from_millis(500));
            return fail("当前密码错误", 401);
        }
        // 注意：持 cfg 锁期间不能再调 must_change()/verify()（std Mutex 不可重入，曾卡死测试）。
        let mut cfg = self.cfg.lock().map_err(|_| ApiError::internal("锁"))?;
        if new != confirm {
            return fail("两次输入的新密码不一致", 400);
        }
        if let Err(e) = GatewayConfig::validate_new(&new) {
            return fail(&e, 400);
        }
        cfg.set_password(&self.paths, &new).map_err(ApiError::internal)?;
        drop(cfg);
        self.basic.clear(); // 旧密码的 Basic 缓存立即作废
        // 改密后踢掉其它设备的会话，本会话保留。
        let keep = req.header("Cookie").and_then(|c| parse_cookie(c, COOKIE)).unwrap_or_default();
        self.sessions.revoke_others(&keep);
        Ok(if json { Reply::ok(&serde_json::json!({"ok": true, "message": "密码已更新"})) } else { Reply::redirect("/") })
    }

    /// `GET /api/session`。
    pub fn session_info(&self) -> Reply {
        Reply::ok(&serde_json::json!({"ok": true, "mustChange": self.must_change()}))
    }
}

fn read_password_body(req: &mut Request<'_>) -> Result<(String, String, bool), ApiError> {
    if req.content_type.starts_with("application/json") {
        let v = req.json_body().map_err(ApiError::bad)?;
        Ok((v.get("password").and_then(|x| x.as_str()).unwrap_or("").to_string(), String::new(), true))
    } else {
        let f = req.form_body().map_err(ApiError::bad)?;
        Ok((f.get("password").cloned().unwrap_or_default(), f.get("next").cloned().unwrap_or_default(), false))
    }
}

/// 登录后跳转的 `next` 只许是本站路径：以单个 `/` 开头，不能是 `//host`，也不能含 `\`（浏览器把
/// `/\evil.com` 当成 `//evil.com` 跳外站，2026-09-24 审查）或控制字符（防响应头注入）。
fn is_local_path(next: &str) -> bool {
    next.starts_with('/') && !next.starts_with("//") && !next.contains('\\') && !next.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn state(must_change: bool) -> Shared {
        let t = tempfile::tempdir().unwrap();
        let paths = crate::testutil::sandbox(&t);
        let mut cfg = GatewayConfig::default();
        cfg.ensure_password(&paths).unwrap();
        if !must_change {
            cfg.set_password(&paths, "secret1").unwrap();
        }
        std::mem::forget(t);
        Arc::new(AuthState::new(cfg, SessionStore::new(std::time::Duration::from_secs(3600), 16), paths, true))
    }
    const IP_A: &str = "192.168.1.10";
    const IP_B: &str = "192.168.1.11";
    fn gr(method: Method, path: &str, headers: &[(&str, &str)]) -> GuardRequest {
        gr_from(IP_A, method, path, headers)
    }
    fn gr_from(ip: &str, method: Method, path: &str, headers: &[(&str, &str)]) -> GuardRequest {
        GuardRequest { method, path: path.into(), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), remote: Some(ip.parse().unwrap()) }
    }
    fn req<'a>(method: Method, path: &str, ct: &str, headers: &[(&str, &str)], body: &'a mut &[u8]) -> Request<'a> {
        req_from(IP_A, method, path, ct, headers, body)
    }
    /// 模拟服务器：对端 IP 经内部头 `REMOTE_IP_HEADER` 传给处理函数。
    fn req_from<'a>(ip: &str, method: Method, path: &str, ct: &str, headers: &[(&str, &str)], body: &'a mut &[u8]) -> Request<'a> {
        let mut hs: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        hs.push((rmsvc_core::http::REMOTE_IP_HEADER.to_string(), ip.to_string()));
        Request { method, path: path.into(), query: HashMap::new(), params: HashMap::new(), content_type: ct.into(), content_length: None, headers: hs, body }
    }

    #[test]
    fn guard_public_redirect_and_basic() {
        let st = state(false);
        let g = st.guard();
        assert!((g.check)(&gr(Method::Get, "/login", &[])).is_none());
        assert!((g.check)(&gr(Method::Get, "/ca.crt", &[])).is_none());
        let r = (g.check)(&gr(Method::Get, "/", &[("Accept", "text/html,*/*")])).unwrap();
        assert_eq!(r.status, 303);
        assert!(r.headers.iter().any(|(k, v)| k == "Location" && v == "/login?next=%2F"));
        let r = (g.check)(&gr(Method::Get, "/api/services", &[])).unwrap();
        assert_eq!(r.status, 401);
        // Basic：用户名任意
        let b = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "whoever:secret1");
        assert!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &format!("Basic {b}"))])).is_none());
        let bad = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "x:wrong");
        assert_eq!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &format!("Basic {bad}"))])).unwrap().status, 401);
    }

    #[test]
    fn login_sets_cookie_and_must_change_flow() {
        let st = state(true);
        let g = st.guard();
        // 表单登录默认密码 → 303 /password + Set-Cookie
        let mut body: &[u8] = b"password=shelf&next=%2Fx";
        let mut r = req(Method::Post, "/login", "application/x-www-form-urlencoded", &[], &mut body);
        let rep = st.login(&mut r).unwrap();
        assert_eq!(rep.status, 303);
        let loc = rep.headers.iter().find(|(k, _)| k == "Location").unwrap().1.clone();
        assert_eq!(loc, "/password", "首登必改");
        let cookie = rep.headers.iter().find(|(k, _)| k == "Set-Cookie").unwrap().1.clone();
        assert!(cookie.contains("HttpOnly") && cookie.contains("Secure"));
        let tok = cookie.split(';').next().unwrap().to_string();
        // 带会话访问首页 → 被赶去 /password；API → 403
        let r = (g.check)(&gr(Method::Get, "/", &[("Cookie", &tok), ("Accept", "text/html")])).unwrap();
        assert_eq!(r.status, 303);
        assert_eq!((g.check)(&gr(Method::Post, "/api/books", &[("Cookie", &tok)])).unwrap().status, 403);
        assert!((g.check)(&gr(Method::Post, "/password", &[("Cookie", &tok)])).is_none());
        // 改密：不一致 / 太短 / 成功
        let mut b: &[u8] = b"current=shelf&new=abcdef1&confirm=zzz";
        assert_eq!(st.change_password(&mut req(Method::Post, "/password", "application/x-www-form-urlencoded", &[("Cookie", &tok)], &mut b)).unwrap().status, 400);
        let mut b: &[u8] = b"current=shelf&new=abcdef1&confirm=abcdef1";
        let rep = st.change_password(&mut req(Method::Post, "/password", "application/x-www-form-urlencoded", &[("Cookie", &tok)], &mut b)).unwrap();
        assert_eq!(rep.status, 303);
        assert!(!st.must_change());
        assert!((g.check)(&gr(Method::Get, "/", &[("Cookie", &tok)])).is_none(), "改完密码本会话仍有效");
        assert!(st.cfg.lock().unwrap().verify("abcdef1"));
        // 错密码登录 JSON → 401
        let mut b: &[u8] = br#"{"password":"nope"}"#;
        assert_eq!(st.login(&mut req(Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 401);
        // 登出
        let rep = st.logout(&mut req(Method::Post, "/logout", "", &[("Cookie", &tok)], &mut (&b""[..]))).unwrap();
        assert_eq!(rep.status, 303);
        assert_eq!((g.check)(&gr(Method::Get, "/api/services", &[("Cookie", &tok)])).unwrap().status, 401);
    }

    #[test]
    fn verify_accepts_only_the_current_password() {
        let st = state(false);
        assert!(st.verify("secret1") && !st.verify("nope") && !st.verify(""));
        st.cfg.lock().unwrap().password_hash.clear();
        assert!(!st.verify("secret1"), "没有密码哈希时一律不通过");
    }

    #[test]
    fn repeated_wrong_passwords_lock_out_without_verifying() {
        let st = state(false);
        for _ in 0..LOGIN_MAX_FAILS {
            let mut b: &[u8] = br#"{"password":"nope"}"#;
            assert_eq!(st.login(&mut req(Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 401);
        }
        // 已锁定：即使给对密码也直接 429（不做校验），带 Retry-After
        let mut b: &[u8] = br#"{"password":"secret1"}"#;
        let rep = st.login(&mut req(Method::Post, "/login", "application/json", &[], &mut b)).unwrap();
        assert_eq!(rep.status, 429);
        assert!(rep.headers.iter().any(|(k, _)| k == "Retry-After"));
        // Basic 同样被挡
        let g = st.guard();
        let ok = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "x:secret1");
        assert_eq!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &format!("Basic {ok}"))])).unwrap().status, 401);
        // 解锁（模拟窗口过去）后成功登录会清零
        st.limiter.reset(IP_A.parse().unwrap());
        let mut b: &[u8] = br#"{"password":"secret1"}"#;
        assert_eq!(st.login(&mut req(Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 200);
    }

    #[test]
    fn lockout_is_per_source_ip() {
        let st = state(false);
        let g = st.guard();
        for _ in 0..LOGIN_MAX_FAILS {
            let mut b: &[u8] = br#"{"password":"nope"}"#;
            assert_eq!(st.login(&mut req_from(IP_A, Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 401);
        }
        let mut b: &[u8] = br#"{"password":"secret1"}"#;
        assert_eq!(st.login(&mut req_from(IP_A, Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 429, "A 已锁定");
        // B 不受 A 连累：表单登录、JSON 登录、Basic 都照常
        let mut b: &[u8] = br#"{"password":"secret1"}"#;
        assert_eq!(st.login(&mut req_from(IP_B, Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 200, "B 仍可登录");
        let ok = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "x:secret1");
        let basic = format!("Basic {ok}");
        assert!((g.check)(&gr_from(IP_B, Method::Get, "/api/services", &[("Authorization", &basic)])).is_none(), "B 的 Basic 照常");
        assert_eq!((g.check)(&gr_from(IP_A, Method::Get, "/api/services", &[("Authorization", &basic)])).unwrap().status, 401, "A 的 Basic 仍被挡");
        // B 登录成功只清 B 自己，A 仍锁着
        let mut b: &[u8] = br#"{"password":"secret1"}"#;
        assert_eq!(st.login(&mut req_from(IP_A, Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 429);
        // USB 网段 / 回环不豁免：同样会被锁
        for ip in ["10.11.99.1", "127.0.0.1"] {
            for _ in 0..LOGIN_MAX_FAILS {
                let mut b: &[u8] = br#"{"password":"nope"}"#;
                assert_eq!(st.login(&mut req_from(ip, Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 401);
            }
            let mut b: &[u8] = br#"{"password":"secret1"}"#;
            assert_eq!(st.login(&mut req_from(ip, Method::Post, "/login", "application/json", &[], &mut b)).unwrap().status, 429, "{ip} 不豁免");
        }
    }

    #[test]
    fn existing_session_unaffected_by_lockout() {
        let st = state(false);
        let g = st.guard();
        let mut b: &[u8] = br#"{"password":"secret1"}"#;
        let rep = st.login(&mut req_from(IP_A, Method::Post, "/login", "application/json", &[], &mut b)).unwrap();
        let tok = rep.headers.iter().find(|(k, _)| k == "Set-Cookie").unwrap().1.split(';').next().unwrap().to_string();
        for _ in 0..LOGIN_MAX_FAILS {
            let mut b: &[u8] = br#"{"password":"nope"}"#;
            st.login(&mut req_from(IP_A, Method::Post, "/login", "application/json", &[], &mut b)).unwrap();
        }
        assert!(st.limiter.locked_for(IP_A.parse().unwrap()).is_some());
        assert!((g.check)(&gr_from(IP_A, Method::Get, "/api/services", &[("Cookie", &tok)])).is_none(), "已登录会话不受锁定影响");
    }

    #[test]
    fn basic_can_change_password_without_current_field() {
        let st = state(true);
        let b = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "cli:shelf");
        let mut body: &[u8] = br#"{"new":"longer1"}"#;
        let rep = st.change_password(&mut req(Method::Post, "/password", "application/json", &[("Authorization", &format!("Basic {b}"))], &mut body)).unwrap();
        assert_eq!(rep.status, 200, "{}", String::from_utf8_lossy(&rep.body));
        assert!(!st.must_change());
    }

    /// Basic 缓存：同一密码第二次起不再跑 PBKDF2（把哈希清空后仍放行＝没有重算）；错密码不受缓存影响；
    /// 改密码后旧密码立即失效。
    #[test]
    fn basic_auth_caches_only_the_verified_password_and_forgets_it_on_change() {
        let st = state(false);
        let g = st.guard();
        let basic = |pw: &str| format!("Basic {}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, format!("cli:{pw}")));
        let ok = basic("secret1");
        assert!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &ok)])).is_none());
        let real_hash = std::mem::take(&mut st.cfg.lock().unwrap().password_hash);
        assert!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &ok)])).is_none(), "命中缓存：不再重算 PBKDF2（哈希已清空也放行）");
        assert_eq!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &basic("secret2"))])).unwrap().status, 401, "别的密码不命中缓存");
        st.cfg.lock().unwrap().password_hash = real_hash;
        // 用 Basic 改密：旧密码的缓存随即作废
        let mut body: &[u8] = br#"{"new":"longer1"}"#;
        assert_eq!(st.change_password(&mut req(Method::Post, "/password", "application/json", &[("Authorization", &ok)], &mut body)).unwrap().status, 200);
        assert_eq!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &ok)])).unwrap().status, 401, "改密后旧密码不能再靠缓存通过");
        assert!((g.check)(&gr(Method::Get, "/api/services", &[("Authorization", &basic("longer1"))])).is_none());
    }

    #[test]
    fn login_next_only_allows_local_paths() {
        assert!(is_local_path("/"));
        assert!(is_local_path("/books?x=1"));
        for bad in ["", "books", "//evil.com", "/\\evil.com", "/\\/evil.com", "https://evil.com", "/x\r\nSet-Cookie: a=b"] {
            assert!(!is_local_path(bad), "{bad:?}");
        }
    }
}
