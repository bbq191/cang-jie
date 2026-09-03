//! 密码哈希 + HTTP Basic 解析 + 内存会话表。哈希格式 `sha256$<salt-hex>$<digest-hex>`（salt 16 字节，随机）。
//! 只保护对外的网关；loopback 领域服务不认证（只有设备本机能连）。
//! 会话：登录页校验密码后发 Cookie 令牌（随机 32 字节 hex），令牌只在内存（重启网关=全部重新登录）。
use base64::Engine;
use sha2::{Digest, Sha256};

fn random_bytes(n: usize) -> Vec<u8> {
    use std::io::Read;
    let mut v = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut v).is_ok() {
            return v;
        }
    }
    // 兜底：时间+pid 混合（只在 /dev/urandom 不可用的怪环境）
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut h = Sha256::new();
    h.update(t.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    h.finalize()[..n.min(32)].to_vec()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 生成人能抄的随机密码（去掉易混字符）。
pub fn random_password(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    random_bytes(len).iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect()
}

pub fn hash_password(password: &str) -> String {
    let salt = random_bytes(16);
    hash_with_salt(password, &salt)
}

fn hash_with_salt(password: &str, salt: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(password.as_bytes());
    format!("sha256${}${}", hex(salt), hex(&h.finalize()))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 3 || parts[0] != "sha256" {
        return false;
    }
    let Ok(salt) = (0..parts[1].len()).step_by(2).map(|i| u8::from_str_radix(&parts[1][i..i + 2], 16)).collect::<Result<Vec<u8>, _>>() else { return false };
    // 常数时间比较（长度相同时逐字节异或）
    let a = hash_with_salt(password, &salt);
    a.len() == stored.len() && a.bytes().zip(stored.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 解析 `Authorization: Basic …` 头 → (user, password)。
pub fn parse_basic(header: &str) -> Option<(String, String)> {
    let b64 = header.strip_prefix("Basic ")?.trim();
    let raw = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let s = String::from_utf8(raw).ok()?;
    let (u, p) = s.split_once(':')?;
    Some((u.to_string(), p.to_string()))
}

/// 解析 `Cookie:` 头里指定名字的值。
pub fn parse_cookie(header: &str, name: &str) -> Option<String> {
    header.split(';').map(|kv| kv.trim()).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k.trim() == name).then(|| v.trim().to_string())
    })
}

/// 内存会话表（Mutex 保护）：签发/校验/吊销，带绝对过期与容量上限（防无限增长）。
pub struct SessionStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
    ttl: std::time::Duration,
    max: usize,
}

impl SessionStore {
    pub fn new(ttl: std::time::Duration, max: usize) -> SessionStore {
        SessionStore { inner: std::sync::Mutex::new(std::collections::HashMap::new()), ttl, max }
    }
    /// 签发新令牌；满了先清过期，仍满则淘汰最早到期的。
    pub fn issue(&self) -> String {
        let token = hex(&random_bytes(32));
        let mut m = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::Instant::now();
        m.retain(|_, exp| *exp > now);
        if m.len() >= self.max {
            if let Some(oldest) = m.iter().min_by_key(|(_, e)| **e).map(|(k, _)| k.clone()) {
                m.remove(&oldest);
            }
        }
        m.insert(token.clone(), now + self.ttl);
        token
    }
    pub fn check(&self, token: &str) -> bool {
        let m = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        m.get(token).map(|exp| *exp > std::time::Instant::now()).unwrap_or(false)
    }
    pub fn revoke(&self, token: &str) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).remove(token);
    }
    /// 吊销除 `keep` 外的全部（改密码后踢掉其它设备）。
    pub fn revoke_others(&self, keep: &str) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).retain(|k, _| k == keep);
    }
    pub fn ttl(&self) -> std::time::Duration {
        self.ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cookie_and_sessions() {
        assert_eq!(parse_cookie("a=1; shelf_session=abc ; b=2", "shelf_session").as_deref(), Some("abc"));
        assert_eq!(parse_cookie("a=1", "shelf_session"), None);
        let s = SessionStore::new(std::time::Duration::from_secs(60), 2);
        let t1 = s.issue();
        assert!(s.check(&t1) && !s.check("nope"));
        let t2 = s.issue();
        let t3 = s.issue(); // 超容量：最早的 t1 被淘汰
        assert!(!s.check(&t1) && s.check(&t2) && s.check(&t3));
        s.revoke_others(&t3);
        assert!(!s.check(&t2) && s.check(&t3));
        s.revoke(&t3);
        assert!(!s.check(&t3));
        let e = SessionStore::new(std::time::Duration::from_secs(0), 8);
        let t = e.issue();
        assert!(!e.check(&t), "ttl=0 立即过期");
    }
    #[test]
    fn hash_roundtrip_and_reject() {
        let h = hash_password("s3cret");
        assert!(h.starts_with("sha256$"));
        assert!(verify_password("s3cret", &h));
        assert!(!verify_password("s3cre", &h));
        assert!(!verify_password("s3cret", "garbage"));
        assert_ne!(hash_password("x"), hash_password("x"), "salt 随机");
    }
    #[test]
    fn basic_header() {
        assert_eq!(parse_basic("Basic c2hlbGY6cGFzczp3b3Jk"), Some(("shelf".into(), "pass:word".into())));
        assert_eq!(parse_basic("Bearer x"), None);
        assert_eq!(random_password(12).len(), 12);
    }
}
