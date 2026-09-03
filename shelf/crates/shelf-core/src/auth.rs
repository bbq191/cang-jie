//! 密码哈希 + HTTP Basic 认证。哈希格式 `sha256$<salt-hex>$<digest-hex>`（salt 16 字节，随机）。
//! 只保护对外的网关；loopback 领域服务不认证（只有设备本机能连）。
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

#[cfg(test)]
mod tests {
    use super::*;
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
