//! OpenAI 兼容 `POST {base_url}/chat/completions` 的传输与应答解析（`mind-serve` 文字问答 / `transcribe-serve`
//! 视觉转写共用）。两边此前各写了一份逐行相同的 `OpenAiCompat::ask/transcribe` 网络部分与 `parse_chat_reply`
//! （只有请求体不同：视觉多一个 `image_url` 段、温度不同），2026-09-20 收进这里。**只抽传输和解析**：
//! 请求体怎么拼（`chat_request`）、`Vision`/`TextModel` 这类业务 trait 仍是各服务自己的关注点。
use crate::truncate_chars as trunc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 一次调用的结果：文本 + token 用量。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatReply {
    pub text: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// 一个已配置好的调用端：后端标识 + baseUrl（去尾 `/`）+ 模型 + key + 带超时的 agent。两个服务此前各有一个字段、
/// 构造逐行相同的 `OpenAiCompat`（2026-09-24 第三轮审计收进来）；各服务的业务 trait（`Vision`/`TextModel`）直接
/// 实现在它身上，只管拼自己的请求体。`Debug` 故意不派生：结构里有 key，别让它有机会被 `{:?}` 打进日志。
pub struct ChatClient {
    backend: String,
    base_url: String,
    model: String,
    key: String,
    agent: ureq::Agent,
    /// 上一次调用结束的墙钟秒数（0 = 还没用过），[`ClientCache`] 据此判断 agent 里的连接是不是已经闲置太久。
    last_done: AtomicU64,
}

impl ChatClient {
    pub fn new(backend: &str, base_url: &str, model: &str, key: &str, timeout: Duration) -> ChatClient {
        ChatClient { backend: backend.into(), base_url: base_url.trim_end_matches('/').into(), model: model.into(), key: key.into(), agent: agent(timeout), last_done: AtomicU64::new(0) }
    }
    /// 写进草稿/回答 `backend` 字段的后端标识。
    pub fn backend(&self) -> &str {
        &self.backend
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    /// 发一次 `chat/completions`（请求体由调用方拼，见 [`post_chat`]）。
    pub fn post(&self, body: &serde_json::Value) -> Result<ChatReply, String> {
        let r = post_chat(&self.agent, &self.base_url, &self.key, body);
        self.last_done.store(rmsvc_core::clock::now_secs(), Ordering::Relaxed);
        r
    }
    /// 到 `now`（墙钟秒）为止闲置了没超过 `max` 秒：没用过的算新鲜；墙钟往回拨了算不新鲜（宁可重建）。
    fn fresh_at(&self, now: u64, max: u64) -> bool {
        let last = self.last_done.load(Ordering::Relaxed);
        last == 0 || (now >= last && now - last <= max)
    }
}

/// 调用端闲置超过这么多秒就不再复用、重建一个（连同新的连接池）。云端负载均衡通常几十秒就回收空闲连接，设备休眠/
/// 换 WiFi 之后旧连接更是早就死了：对方发过 FIN 的 ureq 取用前能看出来，可"悄悄没了"的（休眠期间 NAT 表过期、
/// 换了地址）看不出来，POST 又不会被 ureq 自动重发——要么立刻被 RST、要么干等到超时（缺省 60 秒）才报错。
/// 一轮批量转写、接连几次提问之间的间隔远小于这个值，复用的好处照旧。用墙钟不用 `Instant`：后者不计休眠时间。
const IDLE_REUSE_SECS: u64 = 45;

/// 调用端缓存：配置（后端/baseUrl/模型/key/超时）没变、且上次用完没闲置太久（见 [`IDLE_REUSE_SECS`]）就复用上一次建的
/// [`ChatClient`]（连同 agent 里还活着的 HTTPS 连接）。此前 transcribe-serve 每一轮、mind-serve 每问一次都新建一个，
/// 接连几次调用各做一遍 DNS + TCP + TLS 握手——设备走 WiFi，这几次往返是实打实的射频唤醒（2026-09-30 第五轮审计）。
/// 配置一改（换预置、换 key）指纹就变，下一次调用自然换成新的。
#[derive(Default)]
pub struct ClientCache {
    cur: std::sync::Mutex<Option<(String, std::sync::Arc<ChatClient>)>>,
}

impl ClientCache {
    /// 按当前配置取调用端（缓存命中直接给上一次的）；没 key 返回 `Err(missing_key)`（各服务的提示文案不同，调用方给）。
    pub fn get<C: crate::VendorConfig>(&self, cfg: &C, backend: &str, timeout: Duration, missing_key: &str) -> Result<std::sync::Arc<ChatClient>, String> {
        self.get_at(cfg, backend, timeout, missing_key, rmsvc_core::clock::now_secs())
    }

    fn get_at<C: crate::VendorConfig>(&self, cfg: &C, backend: &str, timeout: Duration, missing_key: &str, now: u64) -> Result<std::sync::Arc<ChatClient>, String> {
        let key = cfg.key().ok_or_else(|| missing_key.to_string())?;
        let fp = format!("{backend}\u{1}{}\u{1}{}\u{1}{key}\u{1}{}", cfg.base_url(), cfg.model(), timeout.as_millis());
        let mut cur = rmsvc_core::sync::lock(&self.cur);
        if let Some((f, c)) = cur.as_ref() {
            if *f == fp && c.fresh_at(now, IDLE_REUSE_SECS) {
                return Ok(c.clone());
            }
        }
        let c = std::sync::Arc::new(ChatClient::new(backend, cfg.base_url(), cfg.model(), &key, timeout));
        *cur = Some((fp, c.clone()));
        Ok(c)
    }
}

/// 建调用用的 agent：连接 10 秒超时 + 整个请求 `timeout`。
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(timeout).build()
}

/// 解析 chat/completions 应答：`choices[0].message.content` 可能是字符串或分段数组；`usage` 缺省 0。
pub fn parse_chat_reply(v: &serde_json::Value) -> Result<ChatReply, String> {
    let msg = v.pointer("/choices/0/message/content").ok_or_else(|| format!("应答无 choices[0].message.content：{}", trunc(&v.to_string(), 300)))?;
    let text = match msg {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join(""),
        _ => return Err("content 形状不认识".into()),
    };
    let u = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(|x| x.as_u64()).unwrap_or(0);
    Ok(ChatReply { text: text.trim().to_string(), prompt_tokens: u("prompt_tokens"), completion_tokens: u("completion_tokens") })
}

/// 发一次调用：`Authorization: Bearer <key>`；非 2xx 带状态码和截断到 300 字的应答体（错误文本可能回显请求内容，
/// 所以截断；key 从不出现在应答里）。`base_url` 末尾的 `/` 由调用方在构造时去掉。
/// **瞬时故障原地重试一次**（见 [`transient_wait`]）：限流 429、网关 5xx、DNS/连不上（设备刚醒、WiFi 还在重连）、
/// 连接被对方掐断。此前一次就报错：转写批量里白白记一次失败（同一条三次就不再自动重试），提问要用户再点一遍。
/// 只在第一次失败得快（[`RETRY_WITHIN`] 以内）时重试——请求已经跑了很久才失败的（超时、慢慢回来的 504）不再搭一整个超时进去。
pub fn post_chat(agent: &ureq::Agent, base_url: &str, key: &str, body: &serde_json::Value) -> Result<ChatReply, String> {
    post_chat_with(agent, base_url, key, body, DEFAULT_RETRY_WAIT)
}

/// 首次失败后多久之内还值得重试一次。
const RETRY_WITHIN: Duration = Duration::from_secs(15);
/// 应答没给 `Retry-After` 时的重试等待。
const DEFAULT_RETRY_WAIT: Duration = Duration::from_secs(2);
/// `Retry-After` 给得再长也只等这么久（这是一次同步请求里的等待）。
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);

// `send` 闭包原样返回 ureq 的 `Result`（`ureq::Error` 体积大是 ureq 自己的类型，这里不另装箱）。
#[allow(clippy::result_large_err)]
fn post_chat_with(agent: &ureq::Agent, base_url: &str, key: &str, body: &serde_json::Value, default_wait: Duration) -> Result<ChatReply, String> {
    let url = format!("{base_url}/chat/completions");
    let payload = body.to_string();
    let send = || agent.post(&url).set("Authorization", &format!("Bearer {key}")).set("Content-Type", "application/json").send_string(&payload);
    let started = std::time::Instant::now();
    let resp = match send() {
        Err(e) => match transient_wait(&e, default_wait).filter(|_| started.elapsed() < RETRY_WITHIN) {
            Some(wait) => {
                eprintln!("[vendorcfg] {}，{} 秒后重试一次", describe(&url, e), wait.as_secs());
                std::thread::sleep(wait);
                send()
            }
            None => Err(e),
        },
        ok => ok,
    };
    let resp = resp.map_err(|e| describe(&url, e))?;
    let v: serde_json::Value = serde_json::from_reader(resp.into_reader()).map_err(|e| format!("应答不是 JSON：{e}"))?;
    parse_chat_reply(&v)
}

/// 错误 → 给人看的一句话（状态码错误带截断的应答体）。
fn describe(url: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, r) => format!("HTTP {c}：{}", trunc(&r.into_string().unwrap_or_default(), 300)),
        e => format!("连不上 {url}：{e}"),
    }
}

/// 这次失败值不值得重试、重试前等多久；`None` = 不重试（4xx 鉴权/参数错、超时、其它）。
fn transient_wait(e: &ureq::Error, default_wait: Duration) -> Option<Duration> {
    match e {
        ureq::Error::Status(429 | 500 | 502 | 503 | 504, r) => {
            let after = r.header("Retry-After").and_then(|v| v.trim().parse::<u64>().ok()).map(Duration::from_secs);
            Some(after.unwrap_or(default_wait).min(MAX_RETRY_WAIT))
        }
        ureq::Error::Status(..) => None,
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => Some(default_wait),
            // 连接被掐断（复用的连接其实早死了、对方重启）可以重来；读写超时不重试（已经等过一整个超时了）。
            ureq::ErrorKind::Io => {
                let io = std::error::Error::source(t).and_then(|s| s.downcast_ref::<std::io::Error>())?;
                use std::io::ErrorKind::*;
                matches!(io.kind(), ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof).then_some(default_wait)
            }
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_segmented_content_and_usage() {
        let r = parse_chat_reply(&serde_json::json!({"choices":[{"message":{"content":" 答案 \n"}}],"usage":{"prompt_tokens":50,"completion_tokens":8}})).unwrap();
        assert_eq!(r, ChatReply { text: "答案".into(), prompt_tokens: 50, completion_tokens: 8 });
        let r = parse_chat_reply(&serde_json::json!({"choices":[{"message":{"content":[{"type":"text","text":"甲"},{"type":"text","text":"乙"}]}}]})).unwrap();
        assert_eq!((r.text.as_str(), r.prompt_tokens), ("甲乙", 0), "usage 缺省 0");
        assert!(parse_chat_reply(&serde_json::json!({"error":"x"})).unwrap_err().contains("choices"));
        assert!(parse_chat_reply(&serde_json::json!({"choices":[{"message":{"content":5}}]})).is_err());
    }

    #[test]
    fn chat_client_trims_base_url_and_sends_bearer_key() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let h = std::thread::spawn(move || {
            let req = server.recv().unwrap();
            let seen = (req.url().to_string(), req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.to_string()));
            let _ = req.respond(tiny_http::Response::from_string(r#"{"choices":[{"message":{"content":"好"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#));
            seen
        });
        let c = ChatClient::new("qwen", &format!("http://127.0.0.1:{port}/v1/"), "m", "sk-1", Duration::from_secs(5));
        assert_eq!((c.backend(), c.model()), ("qwen", "m"));
        let r = c.post(&serde_json::json!({})).unwrap();
        assert_eq!((r.text.as_str(), r.prompt_tokens), ("好", 3));
        let (url, auth) = h.join().unwrap();
        assert_eq!(url, "/v1/chat/completions", "baseUrl 末尾的 / 去掉，不出现 //");
        assert_eq!(auth.as_deref(), Some("Bearer sk-1"));
    }

    #[test]
    fn client_cache_reuses_until_config_changes() {
        use std::collections::BTreeMap;
        #[derive(serde::Serialize)]
        struct Cfg {
            keys: BTreeMap<String, String>,
            prices: BTreeMap<String, crate::Price>,
        }
        const PRESETS: &[crate::Preset] = &[crate::Preset { id: "m", label: "m", model: "m", base_url: crate::DASHSCOPE, provider: "dashscope" }];
        impl crate::VendorConfig for Cfg {
            fn presets() -> &'static [crate::Preset] {
                PRESETS
            }
            fn preset(&self) -> &str {
                "m"
            }
            fn custom_model(&self) -> &str {
                ""
            }
            fn custom_base_url(&self) -> &str {
                ""
            }
            fn keys(&self) -> &BTreeMap<String, String> {
                &self.keys
            }
            fn prices(&self) -> &BTreeMap<String, crate::Price> {
                &self.prices
            }
        }
        let with_key = |k: &str| Cfg { keys: [("dashscope".to_string(), k.to_string())].into(), prices: BTreeMap::new() };
        let cache = ClientCache::default();
        let t = Duration::from_secs(5);
        let a = cache.get(&with_key("sk-1"), "qwen", t, "没 key").unwrap();
        let b = cache.get(&with_key("sk-1"), "qwen", t, "没 key").unwrap();
        assert!(std::sync::Arc::ptr_eq(&a, &b), "配置没变复用同一个");
        // 闲置太久（含休眠）不复用：用过一次之后按墙钟算
        a.last_done.store(1_000, Ordering::Relaxed);
        let fresh = cache.get_at(&with_key("sk-1"), "qwen", t, "没 key", 1_000 + IDLE_REUSE_SECS).unwrap();
        assert!(std::sync::Arc::ptr_eq(&a, &fresh), "刚用过不久照样复用");
        let stale = cache.get_at(&with_key("sk-1"), "qwen", t, "没 key", 1_001 + IDLE_REUSE_SECS).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&a, &stale), "闲置超时重建（旧连接多半已经死了）");
        stale.last_done.store(5_000, Ordering::Relaxed);
        assert!(!std::sync::Arc::ptr_eq(&stale, &cache.get_at(&with_key("sk-1"), "qwen", t, "没 key", 4_000).unwrap()), "墙钟往回拨也重建");
        let a = cache.get(&with_key("sk-1"), "qwen", t, "没 key").unwrap();
        let c = cache.get(&with_key("sk-2"), "qwen", t, "没 key").unwrap();
        assert!(!std::sync::Arc::ptr_eq(&a, &c), "换 key 重建");
        assert!(!std::sync::Arc::ptr_eq(&c, &cache.get(&with_key("sk-2"), "qwen", Duration::from_secs(9), "没 key").unwrap()), "换超时重建");
        if std::env::var(crate::KEY_ENV).is_err() {
            assert_eq!(cache.get(&Cfg { keys: BTreeMap::new(), prices: BTreeMap::new() }, "qwen", t, "没 key").err().as_deref(), Some("没 key"));
        }
    }

    #[test]
    fn post_chat_maps_status_and_transport_errors() {
        // 状态码错误：带状态码与应答体（截断）
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let _ = req.respond(tiny_http::Response::from_string("bad key").with_status_code(401));
            }
        });
        let a = agent(Duration::from_secs(5));
        let e = post_chat_with(&a, &format!("http://127.0.0.1:{port}"), "k", &serde_json::json!({}), Duration::ZERO).unwrap_err();
        assert!(e.contains("HTTP 401") && e.contains("bad key"), "{e}");
        // 连不上（重试一次仍连不上）
        let e = post_chat_with(&a, "http://127.0.0.1:1", "k", &serde_json::json!({}), Duration::ZERO).unwrap_err();
        assert!(e.contains("连不上"), "{e}");
    }

    /// 起一个按脚本逐个应答的假服务：`(状态码, 可选 Retry-After)`；返回端口与"收到了几个请求"的计数。
    fn scripted(script: Vec<(u16, Option<&'static str>)>) -> (u16, std::sync::Arc<AtomicU64>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let hits = std::sync::Arc::new(AtomicU64::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            for (req, (code, after)) in server.incoming_requests().zip(script) {
                h.fetch_add(1, Ordering::Relaxed);
                let body = if code == 200 { r#"{"choices":[{"message":{"content":"好"}}]}"# } else { "busy" };
                let mut resp = tiny_http::Response::from_string(body).with_status_code(code);
                if let Some(v) = after {
                    resp = resp.with_header(tiny_http::Header::from_bytes("Retry-After", v).unwrap());
                }
                let _ = req.respond(resp);
            }
        });
        (port, hits)
    }

    /// 瞬时故障（限流/网关 5xx）原地重试一次就成功；鉴权错不重试；连着两次瞬时故障只重试一次就报错。
    #[test]
    fn transient_failures_are_retried_once() {
        let a = agent(Duration::from_secs(5));
        let call = |port: u16| post_chat_with(&a, &format!("http://127.0.0.1:{port}"), "k", &serde_json::json!({}), Duration::ZERO);
        let (port, hits) = scripted(vec![(503, None), (200, None)]);
        assert_eq!(call(port).unwrap().text, "好");
        assert_eq!(hits.load(Ordering::Relaxed), 2);
        let (port, hits) = scripted(vec![(429, Some("0")), (200, None)]);
        assert_eq!(call(port).unwrap().text, "好", "429 按 Retry-After 等完重试");
        assert_eq!(hits.load(Ordering::Relaxed), 2);
        let (port, hits) = scripted(vec![(401, None), (200, None)]);
        assert!(call(port).unwrap_err().contains("HTTP 401"));
        assert_eq!(hits.load(Ordering::Relaxed), 1, "鉴权错重试也没用");
        let (port, hits) = scripted(vec![(502, None), (502, None), (200, None)]);
        assert!(call(port).unwrap_err().contains("HTTP 502"));
        assert_eq!(hits.load(Ordering::Relaxed), 2, "只重试一次");
    }
}
