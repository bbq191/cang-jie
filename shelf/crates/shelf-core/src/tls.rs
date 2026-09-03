//! 自签证书：首启生成到 `<dir>/{cert,key}.pem`（key 0600），之后复用。SAN 含设备常用地址与主机名，
//! 浏览器仍会提示"不受信任"（自签），确认一次即可；CLI 侧默认不校验证书（局域网 + 密码保护）。
use std::path::Path;

pub struct TlsPem {
    pub cert: Vec<u8>,
    pub key: Vec<u8>,
}

/// 读取或生成。`extra_sans` 追加到 SAN（如设备当前 IP）。
pub fn ensure_self_signed(dir: &Path, extra_sans: &[String]) -> Result<TlsPem, String> {
    let cert_p = dir.join("cert.pem");
    let key_p = dir.join("key.pem");
    if cert_p.is_file() && key_p.is_file() {
        return Ok(TlsPem { cert: std::fs::read(&cert_p).map_err(|e| e.to_string())?, key: std::fs::read(&key_p).map_err(|e| e.to_string())? });
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut sans: Vec<String> = vec!["shelf".into(), "localhost".into(), "remarkable".into(), "10.11.99.1".into()];
    for s in extra_sans {
        if !sans.contains(s) {
            sans.push(s.clone());
        }
    }
    let ck = rcgen::generate_simple_self_signed(sans).map_err(|e| format!("生成证书失败: {e}"))?;
    let cert = ck.cert.pem().into_bytes();
    let key = ck.signing_key.serialize_pem().into_bytes();
    std::fs::write(&cert_p, &cert).map_err(|e| e.to_string())?;
    std::fs::write(&key_p, &key).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(TlsPem { cert, key })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generates_once_then_reuses() {
        let t = tempfile::tempdir().unwrap();
        let a = ensure_self_signed(t.path(), &["10.42.0.224".into()]).unwrap();
        assert!(std::str::from_utf8(&a.cert).unwrap().contains("BEGIN CERTIFICATE"));
        assert!(std::str::from_utf8(&a.key).unwrap().contains("BEGIN PRIVATE KEY"));
        let b = ensure_self_signed(t.path(), &[]).unwrap();
        assert_eq!(a.cert, b.cert);
    }
}
