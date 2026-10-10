//! 运行期配置的"内存副本 + 落盘"小盒子（`transcribe-serve`/`mind-serve` 的 `PUT /config` 共用）：
//! 两边此前各自维护 `Mutex<Config>` + `cfg_path`，启动时"读 → 迁移 → 0600 → 落盘一次"、`PUT` 时
//! "锁 → 克隆 → apply → 存盘 → 换入"逐行相同。这里只收这段流程；配置结构本身、`apply()` 的字段规则仍是各服务自己的。
use rmsvc_core::http::ApiError;
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct ConfigCell<C> {
    path: PathBuf,
    cur: Mutex<C>,
}

impl<C: Clone + Default + Serialize + DeserializeOwned> ConfigCell<C> {
    /// 启动加载：读文件（不存在则写出缺省，损坏则保留原文件退回缺省）→ `migrate`（老配置文件搬进新形状，
    /// 不迁移会让真机已存的 key 在升级后凭空消失）→ 收紧成 0600（含 key）→ 落盘一次让文件形状跟运行时一致。
    /// **文件损坏时跳过这次落盘**，并另存一份 `.corrupt` 副本（只留第一份，最早的坏内容不被后来的覆盖）：
    /// 此前这里无条件 `save`，把用户的 key 换成了缺省（2026-09-24 审查）。之后用户在网页上主动保存配置，才会写出新文件。
    pub fn load(path: &Path, migrate: impl FnOnce(C) -> C) -> ConfigCell<C> {
        // 损坏时的留证（0600 的 `.corrupt` 副本）与日志由基座做，这里只据 `corrupt` 决定跳过落盘
        let (cfg, corrupt) = rmsvc_core::config::load_or_seed_checked::<C>(path, Some(0o600));
        let cfg = migrate(cfg);
        rmsvc_core::fs::set_mode(path, 0o600);
        if !corrupt {
            let _ = rmsvc_core::config::save(path, &cfg, Some(0o600));
        }
        ConfigCell { path: path.to_path_buf(), cur: Mutex::new(cfg) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 当前配置的克隆。
    pub fn get(&self) -> C {
        rmsvc_core::sync::lock(&self.cur).clone()
    }

    /// 在**副本**上跑 `f`（失败则原配置不变）→ 0600 存盘 → 换入，返回新配置。整个过程持锁，并发 PUT 串行。
    /// 错误直接是 `PUT /config` 的回执：`f` 拒绝（字段不合法，如未知预置）→ 400；存盘失败（盘满、目录不可写）→ 500。
    /// 此前两种都是字符串、处理函数一律回 400，存盘失败看着像"填错了"（2026-10-10 审计 CORE-1）。
    pub fn update(&self, f: impl FnOnce(&mut C) -> Result<(), String>) -> Result<C, ApiError> {
        let mut cur = rmsvc_core::sync::lock(&self.cur);
        let mut next = cur.clone();
        f(&mut next).map_err(ApiError::bad)?;
        rmsvc_core::config::save(&self.path, &next, Some(0o600)).map_err(ApiError::internal)?;
        *cur = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    struct C {
        a: u32,
        legacy: String,
    }

    #[test]
    fn load_migrates_persists_0600_and_update_is_transactional() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("c.json");
        std::fs::write(&p, r#"{"a":1,"legacy":"old"}"#).unwrap();
        let cell = ConfigCell::<C>::load(&p, |mut c| {
            if !c.legacy.is_empty() {
                c.a += 100;
                c.legacy.clear();
            }
            c
        });
        assert_eq!(cell.get(), C { a: 101, legacy: String::new() });
        assert!(std::fs::read_to_string(&p).unwrap().contains("101"), "迁移后落盘一次");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // apply 失败：原配置与磁盘都不变
        assert_eq!(cell.update(|c| { c.a = 5; Err("拒绝".into()) }).unwrap_err().status, 400, "字段不合法是请求的错");
        assert_eq!(cell.get().a, 101);
        assert!(std::fs::read_to_string(&p).unwrap().contains("101"));
        // 成功：换入并落盘
        let n = cell.update(|c| { c.a = 7; Ok(()) }).unwrap();
        assert_eq!(n.a, 7);
        assert_eq!(cell.get().a, 7);
        assert!(std::fs::read_to_string(&p).unwrap().contains("\"a\": 7"));
    }

    /// 存盘失败（目录不可写）是服务端故障 → 500，原配置不变。
    #[cfg(unix)]
    #[test]
    fn update_save_failure_is_500_and_keeps_old_config() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("cfg");
        std::fs::create_dir(&dir).unwrap();
        let cell = ConfigCell::<C>::load(&dir.join("c.json"), |c| c);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        // root 跑测试时权限挡不住写，这条就测不出来，跳过
        if std::fs::write(dir.join("probe"), b"").is_ok() {
            return;
        }
        let r = cell.update(|c| { c.a = 9; Ok(()) });
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(r.unwrap_err().status, 500);
        assert_eq!(cell.get().a, 0);
    }

    /// 回归：配置文件损坏时启动不能把它（含 key）换成缺省；另存 .corrupt 副本。
    #[test]
    fn load_keeps_corrupt_file_untouched() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("c.json");
        let bad = r#"{"a":1,"legacy":"sk-secret""#;
        std::fs::write(&p, bad).unwrap();
        let cell = ConfigCell::<C>::load(&p, |c| c);
        assert_eq!(cell.get(), C::default());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), bad, "原文件不动");
        assert_eq!(std::fs::read_to_string(p.with_extension("json.corrupt")).unwrap(), bad);
    }
}
