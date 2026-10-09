//! 进程相关小工具：带超时的子进程、`/proc` 读取（开机秒数、按名字找进程）。网关的 systemctl/journalctl 调用、
//! 字体服务的 fc-scan/fc-cache、壁纸服务找 xochitl 主进程此前各写一份；`/proc` 读取不 fork，设备 2 核上比
//! `systemctl show -p MainPID` 省得多。`proc_root` 参数让测试用假目录代替真 `/proc`。
use std::path::Path;
use std::time::{Duration, Instant};

/// 起子进程等它结束，最多等 `timeout`，成功返回 stdout（去首尾空白）。stdout/stderr 各用一条线程排空（否则输出超过
/// 管道缓冲会把子进程写阻塞，被误判成超时）；超时 kill 并返回错误，不 join 读线程（孙进程可能还握着管道，别被它拖住）。
/// 非零退出返回 stderr；stderr 为空时给 `<cmd> 退出码 <n>`，不回一个空错误。
pub fn run_timeout(cmd: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = std::process::Command::new(cmd).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("{cmd}: {e}"))?;
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = r.read_to_end(&mut v);
            v
        })
    };
    let out_t = child.stdout.take().map(|s| drain(Box::new(s)));
    let err_t = child.stderr.take().map(|s| drain(Box::new(s)));
    let deadline = Instant::now() + timeout;
    // 轮询间隔 20ms 起、逐步翻倍到 200ms：短命令（systemctl is-active 几十毫秒）响应不变慢，
    // 慢命令（卸载脚本最长几分钟）不以 50 次/秒的频率白白唤醒。
    let mut poll = Duration::from_millis(20);
    let status = loop {
        match child.try_wait().map_err(|e| format!("{cmd}: {e}"))? {
            Some(st) => break st,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{cmd} 超过 {} 秒未结束，已终止", timeout.as_secs()));
            }
            None => {
                std::thread::sleep(poll.min(deadline.saturating_duration_since(Instant::now())).max(Duration::from_millis(1)));
                poll = (poll * 2).min(Duration::from_millis(200));
            }
        }
    };
    let text = |t: Option<std::thread::JoinHandle<Vec<u8>>>| t.and_then(|h| h.join().ok()).map(|v| String::from_utf8_lossy(&v).trim().to_string()).unwrap_or_default();
    let (out, err) = (text(out_t), text(err_t));
    if status.success() {
        Ok(out)
    } else if err.is_empty() {
        Err(format!("{cmd} 退出码 {}", status.code().map_or("?".to_string(), |c| c.to_string())))
    } else {
        Err(err)
    }
}

/// `/proc/uptime` 第一列：开机以来的秒数（**含休眠**）。读不到 → `None`。
pub fn uptime_secs(proc_root: &Path) -> Option<f64> {
    std::fs::read_to_string(proc_root.join("uptime")).ok()?.split_whitespace().next()?.parse().ok()
}

/// `/proc/<pid>/stat` 的 (comm, ppid, starttime 滴答)。格式 "pid (comm) state ppid …"，comm 可能含空格/括号，
/// 从最后一个 `)` 之后切；starttime 是第 22 列。
pub fn parse_stat(stat: &str) -> Option<(&str, u32, u64)> {
    let (head, rest) = stat.rsplit_once(')')?;
    let comm = head.split_once('(')?.1;
    let mut f = rest.split_whitespace();
    let ppid = f.nth(1)?.parse().ok()?;
    let ticks = f.nth(17)?.parse().ok()?;
    Some((comm, ppid, ticks))
}

/// 遍历 `/proc` 找第一个 comm 等于 `comm`（且父进程是 `ppid`，给了的话）的进程 → (pid, starttime 滴答)。
/// 每个进程只读一次 `stat`。找 xochitl 主进程：`find_process(Path::new("/proc"), "xochitl", Some(1))`。
pub fn find_process(proc_root: &Path, comm: &str, ppid: Option<u32>) -> Option<(u32, u64)> {
    std::fs::read_dir(proc_root).ok()?.flatten().find_map(|e| {
        let pid: u32 = e.file_name().to_str()?.parse().ok()?;
        let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
        let (c, pp, ticks) = parse_stat(&stat)?;
        (c == comm && ppid.is_none_or(|want| want == pp)).then_some((pid, ticks))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_timeout_ok_err_and_timeout() {
        assert_eq!(run_timeout("sh", &["-c", "echo hi"], Duration::from_secs(5)).unwrap(), "hi");
        assert_eq!(run_timeout("sh", &["-c", "echo bad >&2; exit 3"], Duration::from_secs(5)).unwrap_err(), "bad");
        assert_eq!(run_timeout("sh", &["-c", "exit 4"], Duration::from_secs(5)).unwrap_err(), "sh 退出码 4", "stderr 为空也给出原因");
        let t = Instant::now();
        assert!(run_timeout("sleep", &["5"], Duration::from_millis(200)).unwrap_err().contains("已终止"));
        assert!(t.elapsed() < Duration::from_secs(3));
        assert!(run_timeout("/nonexistent-cmd-xyz", &[], Duration::from_secs(1)).is_err());
        // 大输出不会把子进程写阻塞成"超时"
        assert_eq!(run_timeout("sh", &["-c", "head -c 300000 /dev/zero | tr '\\0' a"], Duration::from_secs(5)).unwrap().len(), 300_000);
    }

    #[test]
    fn proc_parsing_with_fake_root() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        std::fs::write(r.join("uptime"), "1234.56 999.0\n").unwrap();
        assert_eq!(uptime_secs(r), Some(1234.56));
        let stat = |pid: u32, comm: &str, ppid: u32, start: u64| {
            std::fs::create_dir_all(r.join(pid.to_string())).unwrap();
            let mut cols = vec!["0".to_string(); 18];
            cols[17] = start.to_string();
            std::fs::write(r.join(pid.to_string()).join("stat"), format!("{pid} ({comm}) S {ppid} {}", cols[..18].join(" "))).unwrap();
        };
        stat(10, "xochitl", 9, 50); // 子进程，不是主进程
        stat(20, "xochitl", 1, 77);
        stat(30, "a (weird) name", 1, 5);
        std::fs::create_dir(r.join("self")).unwrap();
        assert_eq!(find_process(r, "xochitl", Some(1)), Some((20, 77)));
        assert_eq!(find_process(r, "a (weird) name", None), Some((30, 5)), "comm 含括号/空格");
        assert_eq!(find_process(r, "nope", None), None);
        assert_eq!(parse_stat("1 (init) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 42"), Some(("init", 0, 42)));
        assert_eq!(parse_stat("garbage"), None);
    }
}
