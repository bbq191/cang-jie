//! 唤醒触发器：跟 `journalctl -f -u xochitl` 看显示状态从 DeepSleep 回 Normal 就轮换下一张。
//! 为什么不用 systemd-sleep 钩子做轮换：真机（2026-09-03）发现充电/USB 连着时按电源键只切显示状态、
//! **内核不 suspend**，钩子根本不跑；而 xochitl 的 `Changing display state from DeepSleep to Normal`
//! 每次唤醒必有，与是否真挂起无关。轮换放唤醒时（不放入睡时）是为了避开与 xochitl 画休眠屏抢时序。
//! 阻塞读 journald 管道：空闲零唤醒，不加周期轮询。
use crate::store::WallpaperStore;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

/// 一行 xochitl 日志是否为"唤醒"事件。
pub fn is_wake_line(line: &str) -> bool {
    line.contains("Changing display state from DeepSleep to Normal")
}

/// 是否为"入睡"事件（补 bind 用；不轮换）。
pub fn is_sleep_line(line: &str) -> bool {
    line.contains("Changing display state from Normal to DeepSleep")
}

pub fn spawn(store: Arc<WallpaperStore>) {
    std::thread::spawn(move || loop {
        let child = Command::new("journalctl").args(["-f", "-n", "0", "-u", "xochitl", "-o", "cat"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[wallpaper-serve] 起 journalctl 失败: {e}（30s 后重试）");
                std::thread::sleep(Duration::from_secs(30));
                continue;
            }
        };
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if is_wake_line(&line) {
                    match store.roll() {
                        Ok(Some(n)) => println!("[wallpaper-serve] 唤醒 → 轮换到 {n}"),
                        Ok(None) => {}
                        Err(e) => eprintln!("[wallpaper-serve] 唤醒轮换失败: {e}"),
                    }
                } else if is_sleep_line(&line) {
                    // 入睡前只补 bind（幂等），不动图。
                    if let Err(e) = crate::mount::bind(store.current_path(), store.blank_path()) {
                        eprintln!("[wallpaper-serve] 入睡补 bind 失败: {e}");
                    }
                }
            }
        }
        let _ = child.wait();
        eprintln!("[wallpaper-serve] journalctl 退出，5s 后重连");
        std::thread::sleep(Duration::from_secs(5));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_xochitl_display_state_lines() {
        assert!(is_wake_line("06:50:32.606 rm.batterymanager        Changing display state from DeepSleep to Normal (setDisplayState …)"));
        assert!(is_sleep_line("06:50:27.713 rm.batterymanager        Changing display state from Normal to DeepSleep (setDisplayState …)"));
        assert!(!is_wake_line("Woke-up with display sleeping true"));
    }
}
