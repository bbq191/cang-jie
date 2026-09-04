//! 小文件工具：原子写（同目录 `.tmp` → rename，同分区 rename 原子）+ 可选 unix 权限。
//! 收编 registry / 字体 fonts.json / 各 config-save 里各自重复的"写 tmp 再 rename"实现，
//! 也把壁纸 state、book config 从"原地 write（非原子）"统一到原子写。
use std::path::{Path, PathBuf};

/// 原子写：先写 `<path>.tmp` 再 rename 覆盖。写前建齐父目录。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// 设 unix 权限（如 0o600 给含密码哈希的配置）；非 unix 平台 no-op。失败静默（非致命）。
pub fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_write_creates_parent_and_leaves_no_tmp() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("sub/dir/f.json");
        write_atomic(&p, b"hello").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        // 覆盖写
        write_atomic(&p, b"world").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"world");
        // 不留 .tmp
        let mut tmp = p.as_os_str().to_owned();
        tmp.push(".tmp");
        assert!(!PathBuf::from(tmp).exists(), "tmp 应已 rename 掉");
    }
}
