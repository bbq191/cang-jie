//! 电池刺客（battop）状态探测 + 开关。battop 是独立于 shelf 安装体系之外的诊断采样器
//! （`misc/battery-audit/battop/`，固定装在 `/home/root/battop`，不走 shelf 的 XDG/`bin_dir` 那套），
//! 原生「设置」App 只有一个只读展示页（读 `summary.json`），**从没有过开关**——这里是它第一次有开关。
//!
//! 2026-08-29 出过 cgroup/RCU 死锁事故（`misc/battery-audit/FINDINGS.md`），根因是"反复 service-start
//! 触发的 cgroup 迁移撞上内核 RCU stall"，已经修复为常驻 `Type=simple`（进程内 `loop{sample;sleep}`，
//! 开机只迁一次 cgroup）。单纯 `systemctl start/stop` 不会重现那次事故的触发条件——不是"曾经启动过
//! 就危险"，是"反复重启"才危险，这里的开关只是一次性 start/stop，不循环拉起。
use std::path::Path;
use std::time::UNIX_EPOCH;

const UNIT_FILE: &str = "/usr/lib/systemd/system/battop.service";
const BIN: &str = "/home/root/battop/battop";
const SUMMARY: &str = "/home/root/battop/data/summary.json";

pub struct Status {
    pub installed: bool,
    pub running: bool,
    pub last_sample_at: Option<u64>,
}

pub fn status() -> Status {
    let installed = Path::new(UNIT_FILE).is_file() && Path::new(BIN).is_file();
    let running = installed && crate::manage::run("systemctl", &["is-active", "battop.service"]).map(|s| s.trim() == "active").unwrap_or(false);
    let last_sample_at = std::fs::metadata(SUMMARY).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
    Status { installed, running, last_sample_at }
}

/// `action`：`start`/`stop`。未装（unit 文件不在）时拒绝——网页这次不做"从零装 battop"。
pub fn toggle(action: &str) -> Result<(), String> {
    if !Path::new(UNIT_FILE).is_file() {
        return Err("设备上没有装 battop（这次网页开关只控制已经手动装好的 battop，不提供从网页安装）".into());
    }
    match action {
        "start" | "stop" => crate::manage::run("systemctl", &[action, "battop.service"]).map(|_| ()),
        _ => Err("action 只能 start|stop".into()),
    }
}
