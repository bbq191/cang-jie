//! 唤醒触发器：跟 `journalctl -f -u xochitl` 看显示状态从 DeepSleep 回 Normal 就轮换下一张。
//! 为什么不用 systemd-sleep 钩子做轮换：真机（2026-09-03）发现充电/USB 连着时按电源键只切显示状态、
//! **内核不 suspend**，钩子根本不跑；而 xochitl 的 `Changing display state from DeepSleep to Normal`
//! 每次唤醒必有，与是否真挂起无关。轮换放唤醒时（不放入睡时）是为了避开与 xochitl 画休眠屏抢时序。
//! 阻塞读 journald 管道：空闲零唤醒，不加周期轮询（xochitl 自己有日志输出时才会被唤醒一下做子串匹配）。
//! journalctl 起不来/反复秒退时指数退避重连（5s→5min），不会在 journald 异常时变成每 5s 一次 fork 的空转。
//! （原"入睡前补 bind"分支随 bind-mount 退役删除，2026-09-06。）
use crate::store::WallpaperStore;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 重连退避的下限/上限，及「跑满这么久算一次健康连接、退避复位」。
const BACKOFF_MIN: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(300);
const HEALTHY_AFTER: Duration = Duration::from_secs(60);

/// 一行 xochitl 日志是否为"唤醒"事件。
pub fn is_wake_line(line: &str) -> bool {
    line.contains("Changing display state from DeepSleep to Normal")
}

/// 指数退避：返回 (本次睡多久, 下次的当前值)。连接健康（`ran >= HEALTHY_AFTER`）则复位到下限。
pub fn next_backoff(cur: Duration, ran: Duration) -> (Duration, Duration) {
    let wait = if ran >= HEALTHY_AFTER { BACKOFF_MIN } else { cur.clamp(BACKOFF_MIN, BACKOFF_MAX) };
    (wait, (wait * 2).min(BACKOFF_MAX))
}

/// 起一次 `journalctl -f` 并阻塞处理到它退出；返回是否成功起了进程（仅用于日志措辞）。
fn follow_once(store: &WallpaperStore, bus: &rmsvc_core::events::EventBus) -> bool {
    let child = Command::new("journalctl").args(["-f", "-n", "0", "-u", "xochitl", "-o", "cat"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[wallpaper-serve] 起 journalctl 失败: {e}");
            return false;
        }
    };
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if !is_wake_line(&line) {
                continue;
            }
            match store.roll() {
                Ok(Some(n)) => {
                    println!("[wallpaper-serve] 唤醒 → 轮换到 {n}");
                    bus.publish("wallpapers", "pool");
                }
                Ok(None) => {}
                Err(e) => eprintln!("[wallpaper-serve] 唤醒轮换失败: {e}"),
            }
        }
    }
    let _ = child.wait();
    true
}

pub fn spawn(store: Arc<WallpaperStore>, bus: Arc<rmsvc_core::events::EventBus>) {
    std::thread::spawn(move || {
        let mut backoff = BACKOFF_MIN;
        loop {
            let started = Instant::now();
            let spawned = follow_once(&store, &bus);
            let (wait, next) = next_backoff(backoff, started.elapsed());
            backoff = next;
            eprintln!("[wallpaper-serve] journalctl {}，{}s 后重连", if spawned { "退出" } else { "未能启动" }, wait.as_secs());
            std::thread::sleep(wait);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_xochitl_display_state_lines() {
        assert!(is_wake_line("06:50:32.606 rm.batterymanager        Changing display state from DeepSleep to Normal (setDisplayState …)"));
        assert!(!is_wake_line("06:50:27.713 rm.batterymanager        Changing display state from Normal to DeepSleep (setDisplayState …)"));
        assert!(!is_wake_line("Woke-up with display sleeping true"));
    }

    #[test]
    fn backoff_doubles_to_cap_and_resets_after_healthy_run() {
        let quick = Duration::from_secs(1);
        let mut cur = BACKOFF_MIN;
        let mut seen = Vec::new();
        for _ in 0..8 {
            let (wait, next) = next_backoff(cur, quick);
            seen.push(wait.as_secs());
            cur = next;
        }
        assert_eq!(seen, vec![5, 10, 20, 40, 80, 160, 300, 300]);
        // 健康连接（跑满 60s）后断开：回到 5s
        assert_eq!(next_backoff(cur, HEALTHY_AFTER).0, BACKOFF_MIN);
        // 异常入参也被钳到区间内
        assert_eq!(next_backoff(Duration::ZERO, quick).0, BACKOFF_MIN);
        assert_eq!(next_backoff(Duration::from_secs(9999), quick).0, BACKOFF_MAX);
    }
}
