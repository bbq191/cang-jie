//! 联网查书目用的 HTTP：节流、重试、网址编码（移植自 sheng-ren `library::net`，超时与重试按设备收紧）。

use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::io::Read;
use std::time::{Duration, Instant};

pub(crate) const UA: &str = "shelf-book-serve/0.1 (personal e-book library tool)";

/// 最多试几次（429、5xx、超时这类临时错误才重试）。设备上只试 2 次（sheng-ren 在电脑上试 4 次）：优化在后台跑，
/// 连不上的网站（设备直连不了 Wikidata/Open Library）别让一本书卡几分钟。
const TRIES: usize = 2;

/// 带节流和重试的 HTTP：Wikidata 限速严（连续请求会 429），每个请求间隔至少 1.2 秒，429 时按 Retry-After 等。
///
/// - 4xx（除了 429）是"没有"，不重试；
/// - 连不上某个网站（DNS 解析失败、连接失败）就记下这个网站，之后发给它的请求立即失败，不再每次重试等待；
/// - 临时错误（离线、429/5xx 重试完、超时等）记下来（[`Net::transient_error`]）：调用方据此区分"没找到"和"没查成"，
///   没查成的不能当"没有封面"去生成封面并存下来。
pub(crate) struct Net {
    agent: ureq::Agent,
    last: Cell<Option<Instant>>,
    /// 连不上的网站（主机名）。
    down: RefCell<HashSet<String>>,
    transient: RefCell<Option<String>>,
}

impl Net {
    pub(crate) fn new() -> Net {
        Net { agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(12)).user_agent(UA).build(), last: Cell::new(None), down: RefCell::default(), transient: RefCell::new(None) }
    }

    /// 遇到的第一个临时网络错误。
    pub(crate) fn transient_error(&self) -> Option<String> {
        self.transient.borrow().clone()
    }

    fn note_transient(&self, e: &str) {
        self.transient.borrow_mut().get_or_insert_with(|| e.to_string());
    }

    pub(crate) fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        self.fetch_ref(url, None)
    }

    /// `referer`：豆瓣图片服务器不带来源页会拒绝（HTTP 418）。
    pub(crate) fn fetch_ref(&self, url: &str, referer: Option<&str>) -> Result<Vec<u8>, String> {
        let host = host_of(url);
        if self.down.borrow().contains(host) {
            let e = format!("{url}: 连不上 {host}，跳过");
            self.note_transient(&e);
            return Err(e);
        }
        let mut last_err = String::new();
        for attempt in 1..=TRIES {
            if let Some(t) = self.last.get() {
                let gap = Duration::from_millis(1200);
                if t.elapsed() < gap {
                    std::thread::sleep(gap - t.elapsed());
                }
            }
            self.last.set(Some(Instant::now()));
            let mut req = self.agent.get(url);
            if let Some(r) = referer {
                req = req.set("Referer", r);
            }
            let wait = match req.call() {
                Ok(r) => {
                    let mut buf = Vec::new();
                    match r.into_reader().take(20 << 20).read_to_end(&mut buf) {
                        Ok(_) => {
                            return Ok(buf);
                        }
                        Err(e) => {
                            last_err = e.to_string();
                            2
                        }
                    }
                }
                Err(ureq::Error::Status(code, r)) if code == 429 || code >= 500 => {
                    last_err = format!("HTTP {code}");
                    r.header("Retry-After").and_then(|v| v.parse().ok()).unwrap_or(3u64).min(10)
                }
                Err(ureq::Error::Status(code, _)) => return Err(format!("{url}: HTTP {code}")),
                Err(e @ ureq::Error::Transport(_)) => match e.kind() {
                    ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => {
                        self.down.borrow_mut().insert(host.to_string());
                        let e = format!("连不上 {host}（{e}）");
                        self.note_transient(&e);
                        return Err(e);
                    }
                    ureq::ErrorKind::Io | ureq::ErrorKind::BadStatus | ureq::ErrorKind::BadHeader | ureq::ErrorKind::ProxyConnect => {
                        last_err = e.to_string();
                        2
                    }
                    _ => return Err(format!("{url}: {e}")), // 网址不对、重定向太多：重试也没用
                },
            };
            if attempt < TRIES {
                std::thread::sleep(Duration::from_secs(wait));
            }
        }
        let e = format!("{url}: {last_err}");
        self.note_transient(&e);
        Err(e)
    }

    /// 取 JSON。**回来的不是 JSON 也算没查成**（设备 2026-09-29 移植时加）：连着强制门户的 WiFi 时任何网址都返回登录页，
    /// 不能把它当成"豆瓣说没有这本书"——那样会给有封面可找的书生成一张。
    pub(crate) fn json(&self, url: &str) -> Result<Value, String> {
        serde_json::from_slice(&self.fetch(url)?).map_err(|e| {
            let e = format!("{url}: 回来的不是 JSON（{e}）");
            self.note_transient(&e);
            e
        })
    }
}

/// 网址里的主机名（`https://book.douban.com/j/…` → `book.douban.com`）。
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

pub(crate) fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_of_takes_the_authority_part() {
        assert_eq!(super::host_of("https://book.douban.com/j/subject_suggest?q=x"), "book.douban.com");
        assert_eq!(super::host_of("https://www.wikidata.org?x"), "www.wikidata.org");
        assert_eq!(super::host_of("covers.openlibrary.org/b/id/1-L.jpg"), "covers.openlibrary.org");
    }
}
