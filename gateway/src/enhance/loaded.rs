//! 扩展**实际**有没有加载进 xochitl（2026-09-24 审查补）：网页开关只反映 `reading-qol.json` 里的配置，看不出
//! `.so` 到底进没进进程——历史上两次"开关看着开了、其实没生效"（09-09 langhook 整个从设备上消失；GLIBC
//! 版本不符让 hw-stroke 静默加载失败）。这里直接看 xochitl 主进程的 `/proc/<pid>/maps`：映射了哪个
//! `extensions.d/*.so` 就是真加载了。
//!
//! 主进程判定：`comm == "xochitl"` 且父进程是 1（systemd）。xochitl 渲染 PDF 时会 fork 出同名的
//! worker 子进程，父进程不是 1，排除。
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    /// 找到了 xochitl 主进程。
    pub xochitl: bool,
    /// 主进程里映射了 xovi.so（LD_PRELOAD 生效）。
    pub xovi: bool,
    /// 已映射的 `extensions.d/` 下的扩展文件名（去重、排序），如 `hl-snap.so`。
    pub extensions: Vec<String>,
}

fn xochitl_main_pid(proc_root: &Path) -> Option<String> {
    std::fs::read_dir(proc_root).ok()?.flatten().find_map(|e| {
        let pid = e.file_name().to_str()?.to_string();
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
        if comm.trim_end() != "xochitl" {
            return None;
        }
        // stat: "pid (comm) state ppid ..."——comm 可能含空格，从最后一个 ')' 之后切。
        let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
        let ppid = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?;
        (ppid == "1").then_some(pid)
    })
}

pub fn scan(proc_root: &Path) -> Loaded {
    let Some(pid) = xochitl_main_pid(proc_root) else { return Loaded::default() };
    let maps = std::fs::read_to_string(proc_root.join(&pid).join("maps")).unwrap_or_default();
    let mut exts: Vec<String> = maps
        .lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .filter_map(|p| p.split_once("/extensions.d/").map(|(_, f)| f.to_string()))
        .collect();
    exts.sort();
    exts.dedup();
    Loaded { xochitl: true, xovi: maps.lines().any(|l| l.ends_with("/xovi.so")), extensions: exts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_entry(root: &Path, pid: &str, comm: &str, ppid: &str, maps: &str) {
        let d = root.join(pid);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("comm"), format!("{comm}\n")).unwrap();
        std::fs::write(d.join("stat"), format!("{pid} ({comm}) S {ppid} 1 1 0")).unwrap();
        std::fs::write(d.join("maps"), maps).unwrap();
    }

    #[test]
    fn finds_main_process_and_its_mapped_extensions() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        std::fs::create_dir_all(r.join("self")).unwrap();
        proc_entry(r, "839", "xochitl", "613", "7f00-7f01 r-xp 0 00:00 1 /home/root/xovi/extensions.d/bogus.so\n");
        proc_entry(r, "613", "xochitl", "1", "7f00-7f01 r-xp 00000000 b3:04 11 /home/root/xovi/xovi.so\n\
7f02-7f03 r-xp 00000000 b3:04 12 /home/root/xovi/extensions.d/hw-stroke.so\n\
7f04-7f05 r--p 00001000 b3:04 12 /home/root/xovi/extensions.d/hw-stroke.so\n\
7f06-7f07 r-xp 00000000 b3:04 13 /home/root/xovi/extensions.d/hl-snap.so\n\
7f08-7f09 rw-p 00000000 00:00 0 \n");
        proc_entry(r, "700", "sh", "1", "");
        let l = scan(r);
        assert!(l.xochitl && l.xovi);
        assert_eq!(l.extensions, ["hl-snap.so", "hw-stroke.so"], "去重排序、不含 worker 子进程的映射");
    }

    #[test]
    fn no_xochitl_or_no_xovi() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(scan(t.path()), Loaded::default());
        proc_entry(t.path(), "5", "xochitl", "1", "7f00-7f01 r-xp 0 00:00 1 /usr/bin/xochitl\n");
        let l = scan(t.path());
        assert!(l.xochitl && !l.xovi && l.extensions.is_empty());
    }
}
