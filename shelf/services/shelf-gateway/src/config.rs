//! 网关配置 `$XDG_CONFIG_HOME/shelf/gateway.json`：HTTPS 开关、Basic 认证用户名与密码哈希。
//! 首启无密码 → 生成随机初始密码：哈希写进配置，明文写 `<config>/gateway-initial-password.txt`（0600）并打印到日志，
//! 用户改密码后该文件被删。改密码：设备上 `shelf-gateway passwd <新密码>`。
use serde::{Deserialize, Serialize};
use shelf_core::auth;
use shelf_core::paths::Paths;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GatewayConfig {
    pub https: bool,
    pub auth: bool,
    pub user: String,
    pub password_hash: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig { https: true, auth: true, user: "shelf".into(), password_hash: String::new() }
    }
}

pub fn initial_password_file(paths: &Paths) -> PathBuf {
    paths.config_dir().join("gateway-initial-password.txt")
}

impl GatewayConfig {
    pub fn path(paths: &Paths) -> PathBuf {
        paths.service_config("gateway")
    }
    pub fn load(paths: &Paths) -> GatewayConfig {
        std::fs::read_to_string(Self::path(paths)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self, paths: &Paths) -> Result<(), String> {
        std::fs::create_dir_all(paths.config_dir()).map_err(|e| e.to_string())?;
        let p = Self::path(paths);
        std::fs::write(&p, serde_json::to_string_pretty(self).unwrap_or_default()).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// 无密码哈希时生成初始密码；返回 Some(明文) 表示本次新生成。
    pub fn ensure_password(&mut self, paths: &Paths) -> Result<Option<String>, String> {
        if !self.password_hash.is_empty() {
            return Ok(None);
        }
        let pw = auth::random_password(10);
        self.password_hash = auth::hash_password(&pw);
        self.save(paths)?;
        let f = initial_password_file(paths);
        std::fs::write(&f, format!("{pw}\n")).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
        }
        Ok(Some(pw))
    }

    pub fn set_password(&mut self, paths: &Paths, pw: &str) -> Result<(), String> {
        if pw.len() < 4 {
            return Err("密码至少 4 位".into());
        }
        self.password_hash = auth::hash_password(pw);
        self.save(paths)?;
        let _ = std::fs::remove_file(initial_password_file(paths));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_password_then_set() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().to_str().unwrap().to_string();
        let paths = Paths::resolve(move |k| if k == "HOME" { Some(h.clone()) } else { None });
        let mut c = GatewayConfig::load(&paths);
        assert!(c.https && c.auth && c.password_hash.is_empty());
        let pw = c.ensure_password(&paths).unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(initial_password_file(&paths)).unwrap().trim(), pw);
        assert!(auth::verify_password(&pw, &GatewayConfig::load(&paths).password_hash));
        assert!(c.ensure_password(&paths).unwrap().is_none(), "已有哈希不再生成");
        assert!(c.set_password(&paths, "abc").is_err());
        c.set_password(&paths, "newpass").unwrap();
        assert!(!initial_password_file(&paths).exists());
        assert!(auth::verify_password("newpass", &GatewayConfig::load(&paths).password_hash));
    }
}
