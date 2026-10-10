//! 服务配置/状态的 JSON 读写模板：读→反序列化→缺省；可选首启写出缺省；原子保存 + 可选 0600。
//! 收编各服务 config/state 的 load/seed/save 复制（book / font / gateway / wallpaper 四处曾各写一份）。
use crate::fs::{set_mode, write_atomic_mode};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

/// 读并反序列化；文件缺失或损坏 → `T::default()`（不写盘）。
pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// 同 [`load_or_default`]，但文件**不存在**时把缺省写出一份（供用户改）。
/// 只在"不存在"时写：损坏文件（存在但读不出 / 解析失败）保留原样、退回缺省，绝不覆盖用户可能想修的内容，并且
/// **打一行日志 + 另存 `.corrupt` 副本**（[`backup_corrupt`]，只留第一份）——2026-10-10 前这一步只在笔记线的
/// `notesvc::load_or_seed_logged` 里有，书架 / 字体配置改坏了静默退回缺省、看不出任何迹象（审计 NT-5）。
/// 写失败忽略（下次再试），不阻塞启动。
pub fn load_or_seed<T: DeserializeOwned + Default + Serialize>(path: &Path) -> T {
    load_or_seed_checked(path, None).0
}

/// 同 [`load_or_seed`]，另外返回"文件是否损坏"（调用方据此跳过"启动时落盘一次"，免得把坏文件换成缺省）；
/// `backup_mode` 是 `.corrupt` 副本的权限（含 key 的配置给 `Some(0o600)`）。整份只读一次。
pub fn load_or_seed_checked<T: DeserializeOwned + Default + Serialize>(path: &Path, backup_mode: Option<u32>) -> (T, bool) {
    let parsed = match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let c = T::default();
            let _ = save(path, &c, None);
            return (c, false);
        }
        Err(e) => Err(e.to_string()),
        Ok(t) => serde_json::from_str::<T>(&t).map_err(|e| e.to_string()),
    };
    match parsed {
        Ok(c) => (c, false),
        Err(e) => {
            let bak = backup_corrupt(path, backup_mode);
            eprintln!("[config] {} 读不出（{e}），按缺省运行、不覆盖原文件{}", path.display(), if bak.is_some() { "（副本在同目录 .corrupt）" } else { "" });
            (T::default(), true)
        }
    }
}

/// 文件存在但读不出或解析不成 `T`（缺失不算）。给"启动时落盘一次"的调用方判断该不该跳过写盘，
/// 以免把用户可能想修的内容（含 API key）换成缺省。
pub fn is_corrupt<T: DeserializeOwned>(path: &Path) -> bool {
    path.exists() && std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<T>(&t).ok()).is_none()
}

/// 损坏文件留证：把 `path` 复制成同目录 `<文件名>.corrupt`（`mode` 给定时设权限，含 key 的配置给 `Some(0o600)`）。
/// **只留第一份**：副本已存在就不再复制、返回 `None`——留住最早那份坏内容，且调用方可凭返回值"只在头一次打日志"
/// （条目库这类每次刷新都会路过的读路径别刷屏）。新复制了返回副本路径。原文件不动。
pub fn backup_corrupt(path: &Path, mode: Option<u32>) -> Option<std::path::PathBuf> {
    let mut name = path.file_name()?.to_os_string();
    name.push(".corrupt");
    let bak = path.with_file_name(name);
    if bak.exists() || std::fs::copy(path, &bak).is_err() {
        return None;
    }
    if let Some(m) = mode {
        set_mode(&bak, m);
    }
    Some(bak)
}

/// 原子保存（tmp→rename，建齐父目录）。`mode` 给敏感文件设权限（如含密码哈希的 `Some(0o600)`）。
pub fn save<T: Serialize>(path: &Path, value: &T, mode: Option<u32>) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    write_atomic_mode(path, &bytes, mode).map_err(|e| e.to_string())?;
    if let Some(m) = mode {
        set_mode(path, m); // 创建时的权限会被 umask 收窄；这里再定成准确值
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    #[serde(default)]
    struct C {
        a: u32,
        b: String,
    }
    impl Default for C {
        fn default() -> Self {
            C { a: 7, b: "x".into() }
        }
    }
    #[test]
    fn seed_writes_default_once_then_reads_back() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("c.json");
        assert!(!p.exists());
        let c: C = load_or_seed(&p);
        assert_eq!(c, C::default());
        assert!(p.exists(), "首启应写出缺省");
        // 用户改一个字段
        save(&p, &C { a: 9, b: "y".into() }, None).unwrap();
        let c: C = load_or_seed(&p);
        assert_eq!(c, C { a: 9, b: "y".into() });
    }
    #[test]
    fn backup_corrupt_keeps_first_copy_only() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("c.json");
        assert_eq!(backup_corrupt(&p, None), None, "原文件不在");
        std::fs::write(&p, b"{ bad 1").unwrap();
        let bak = backup_corrupt(&p, Some(0o600)).unwrap();
        assert_eq!(bak, t.path().join("c.json.corrupt"));
        assert_eq!(std::fs::metadata(&bak).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::write(&p, b"{ bad 2").unwrap();
        assert_eq!(backup_corrupt(&p, None), None, "已有副本不再复制");
        assert_eq!(std::fs::read(&bak).unwrap(), b"{ bad 1", "留住最早那份");
        assert_eq!(std::fs::read(&p).unwrap(), b"{ bad 2", "原文件不动");
    }

    #[test]
    fn default_on_corrupt_without_overwrite() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("c.json");
        std::fs::write(&p, b"{ not json").unwrap();
        let c: C = load_or_default(&p);
        assert_eq!(c, C::default());
        // 损坏文件保留原样（不被 seed 覆盖）
        let c: C = load_or_seed(&p);
        assert_eq!(c, C::default());
        assert_eq!(std::fs::read(&p).unwrap(), b"{ not json", "损坏内容不覆盖");
        assert_eq!(std::fs::read(t.path().join("c.json.corrupt")).unwrap(), b"{ not json", "损坏时另存副本");
        std::fs::write(&p, br#"{"a":9}"#).unwrap();
        assert_eq!(load_or_seed_checked::<C>(&p, None), (C { a: 9, b: "x".into() }, false), "修好后照常读，副本不影响");
        std::fs::write(&p, b"[").unwrap();
        assert_eq!(load_or_seed_checked::<C>(&p, None), (C::default(), true));
        assert_eq!(std::fs::read(t.path().join("c.json.corrupt")).unwrap(), b"{ not json", "只留第一份");
    }
}
