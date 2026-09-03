//! bind-mount 覆盖休眠屏文件（只读 rootfs 上可挂；真身不改；umount 秒还原）。
//! 需 root（设备上服务本就以 root 跑）。通过 `/proc/mounts` 判断是否已挂，幂等。
use crate::store::{CAROUSEL, SUSPENDED_PNG};
use std::path::Path;
use std::process::Command;

pub fn is_mounted(target: &str) -> bool {
    std::fs::read_to_string("/proc/mounts").map(|m| m.lines().any(|l| l.split_whitespace().nth(1) == Some(target))).unwrap_or(false)
}

fn bind_one(src: &Path, target: &str) -> Result<(), String> {
    if is_mounted(target) {
        return Ok(());
    }
    if !Path::new(target).exists() {
        return Err(format!("{target} 不存在（固件路径变了？）"));
    }
    let st = Command::new("mount").args(["--bind", src.to_str().unwrap_or(""), target]).status().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("mount --bind {} → {} 失败", src.display(), target))
    }
}

fn unbind_one(target: &str) -> Result<(), String> {
    if !is_mounted(target) {
        return Ok(());
    }
    let st = Command::new("umount").arg(target).status().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("umount {target} 失败"))
    }
}

/// current.png → suspended.png；透明卡 → 三张 carousel。返回挂了几条。
pub fn bind(current: &Path, blank: &Path) -> Result<usize, String> {
    if !current.is_file() {
        return Err("还没有激活的壁纸（先上传并激活一张）".into());
    }
    bind_one(current, SUSPENDED_PNG)?;
    for c in CAROUSEL {
        bind_one(blank, c)?;
    }
    Ok(mounted_count())
}

pub fn unbind() -> Result<(), String> {
    for c in CAROUSEL {
        unbind_one(c)?;
    }
    unbind_one(SUSPENDED_PNG)
}

pub fn mounted_count() -> usize {
    [SUSPENDED_PNG].iter().chain(CAROUSEL.iter()).filter(|t| is_mounted(t)).count()
}
