//! 服务注册/发现（Registry）。每个领域服务启动时写 `<services_dir>/<name>.json`，退出时删除；
//! 网关/CLI 读目录即知"活着的服务"，缺哪个就隐藏该 tab / 回 404——拔插零配置。
//! 运行时目录重启即清，加上按 `/proc/<pid>` 清理陈旧条目，崩溃残留也不会误报。
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 网关 UI 的一个 tab。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UiTab {
    pub title: String,
    pub order: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServiceInfo {
    pub name: String,
    pub port: u16,
    pub label: String,
    pub version: String,
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiTab>,
}

impl ServiceInfo {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// 注册守卫：Drop 时删注册文件。
pub struct Registration {
    file: PathBuf,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

fn file_of(paths: &Paths, name: &str) -> PathBuf {
    paths.services_dir().join(format!("{name}.json"))
}

/// 写注册文件（原子：先写 .tmp 再 rename）。同名且 pid 仍活着的条目**拒绝覆盖**（防误起第二实例顶掉
/// 正在服务的那份——真机踩过：调试起了个 `--bind 127.0.0.1:1` 把 wallpaper-serve 注册顶没了）。
pub fn register(paths: &Paths, info: &ServiceInfo) -> std::io::Result<Registration> {
    std::fs::create_dir_all(paths.services_dir())?;
    let file = file_of(paths, &info.name);
    if let Some(existing) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<ServiceInfo>(&t).ok()) {
        if existing.pid != info.pid && pid_alive(existing.pid) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} 已由 pid {} 注册（端口 {}）；先停掉它再起第二份", info.name, existing.pid, existing.port),
            ));
        }
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(info)?)?;
    std::fs::rename(&tmp, &file)?;
    Ok(Registration { file })
}

fn pid_alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    } else {
        true
    }
}

/// 列出活着的服务（按 ui.order 排序，无 tab 的在后）；陈旧条目（pid 已死）顺手删除。
pub fn list(paths: &Paths) -> Vec<ServiceInfo> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(paths.services_dir()) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let Ok(info) = serde_json::from_str::<ServiceInfo>(&text) else { continue };
        if !pid_alive(info.pid) {
            let _ = std::fs::remove_file(&p);
            continue;
        }
        out.push(info);
    }
    out.sort_by_key(|s| (s.ui.as_ref().map(|t| t.order).unwrap_or(u32::MAX), s.name.clone()));
    out
}

pub fn find(paths: &Paths, name: &str) -> Option<ServiceInfo> {
    list(paths).into_iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(t: &tempfile::TempDir) -> Paths {
        let h = t.path().to_str().unwrap().to_string();
        Paths::resolve(move |k| if k == "HOME" || k == "XDG_RUNTIME_DIR" { Some(h.clone()) } else { None })
    }
    fn info(name: &str, order: u32, pid: u32) -> ServiceInfo {
        ServiceInfo {
            name: name.into(),
            port: 8790,
            label: name.into(),
            version: "0".into(),
            pid,
            ui: Some(UiTab { title: name.into(), order }),
        }
    }

    #[test]
    fn register_list_and_drop() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        let me = std::process::id();
        let _a = register(&p, &info("b-svc", 2, me)).unwrap();
        let g = register(&p, &info("a-svc", 1, me)).unwrap();
        let names: Vec<String> = list(&p).into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["a-svc", "b-svc"]);
        drop(g);
        assert_eq!(list(&p).len(), 1);
        assert!(find(&p, "b-svc").is_some());
        assert!(find(&p, "a-svc").is_none());
    }

    #[test]
    fn second_live_instance_cannot_clobber() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        let me = std::process::id();
        let _g = register(&p, &info("svc", 1, me)).unwrap();
        // 用 pid 1（init，必活）模拟"另一个活着的实例"
        let err = match register(&p, &info("svc", 1, 1)) {
            Err(e) => e,
            Ok(_) => panic!("同名活实例不该注册成功"),
        };
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(find(&p, "svc").unwrap().pid, me);
    }

    #[test]
    fn stale_pid_entries_are_purged() {
        let t = tempfile::tempdir().unwrap();
        let p = paths(&t);
        // pid 4294967295 不可能存活
        let g = register(&p, &info("dead", 1, u32::MAX)).unwrap();
        std::mem::forget(g); // 模拟崩溃：不删文件
        assert!(list(&p).is_empty());
        assert!(!p.services_dir().join("dead.json").exists());
    }
}
