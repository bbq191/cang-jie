//! 轮换触发器：xochitl 休眠时读完 `current.png`，就轮换到下一张（下次休眠显示它）。
//!
//! **怎么知道"休眠了"**：xochitl 每次进 DeepSleep 都按 `SleepScreenPath` 重读一遍 `current.png`——2026-09-24 真机用
//! inotify 观察坐实：`OPEN`→`CLOSE_NOWRITE current.png` 恰在它写 `Changing display state from Normal to DeepSleep`
//! 前约 120 ms，空闲 20 分钟里没有别的读。所以只监听壁纸目录的 `IN_CLOSE_NOWRITE`：
//! - 空闲时**零唤醒**，每次休眠只醒一次；不再常驻 `journalctl -f` 子进程（它随全系统每条 journal 醒，
//!   本进程又随 xochitl 每行日志醒——09-24 前的做法，见文末）。
//! - 本服务自己写 `current.png`（原地 truncate 写）产生的是 `IN_CLOSE_WRITE`，不在掩码里，不会自己触发自己；
//!   池图在 `pool/` 子目录，目录监听不递归，网页预览读池图也不触发。
//! - xochitl 已经读完才换图，不和它画休眠屏抢时序（09-03 选在唤醒时轮换也是为了避开这个）。换图若恰好被随后的
//!   内核挂起冻在半路，唤醒后接着写完，下次休眠前早已写好。
//! - 充电/连 USB 时按电源键只切显示状态、内核不挂起（09-03 真机发现，systemd-sleep 钩子因此不可用）——xochitl
//!   照样走 Normal→DeepSleep 画休眠屏、读这个文件，所以同样触发（未单独在充电状态下复核）。
//! - 同一次休眠若被读了不止一次，[`MIN_ROLL_GAP`] 内只轮换一次。
//!
//! 09-24 前：跟 `journalctl -f -u xochitl` 找 `DeepSleep to Normal`（唤醒时轮换）。xochitl 日志走 stdout、
//! journal 里没有可过滤的字段，设备 journalctl 也不支持 `-g`，只能整条跟着。
//!
//! 2026-10-10 起监听本身交给 `rmsvc_core::fswatch::watch_with`（审计 CORE-7）：此前这里手写一份 inotify + 退避重挂，
//! 只因为 fswatch 掩码写死、强制防抖。现在按 [`SPEC`] 只要 `CLOSE_NOWRITE`、不防抖。与旧实现的对照：
//! - 掩码由内核过滤，自己写 `current.png`（`CLOSE_WRITE`）照旧收不到；回调只拿到文件名，按 `current.png` 名字筛。
//!   旧代码还排除了 `ISDIR`（目录名恰好叫 `current.png` 才可能撞上，本服务不会建这样的目录），新接口不区分，接受。
//! - 目录被删/被挪走：fswatch 收到 `IN_IGNORED`/`IN_MOVE_SELF` 后按 1 秒起翻倍、5 分钟封顶的退避等目录回来再挂上。
//!   与旧代码的差别：旧代码重试时自己 `ensure()` 把目录建回来，现在等别人建——上传入池（`write_atomic` 会建父目录）
//!   或服务重启都会建；目录没回来时池是空的，本来也没东西可轮换。目录被**挪走**时旧代码没订 `MOVE_SELF`，会一直
//!   守着挪走后的旧目录，现在会重挂到原路径上。
//! - `watch_with` 只在一开始挂不上监听（目录不在、inotify 起不来）时返回：照旧先 `ensure()` 再按 5 秒到 5 分钟退避重试。
use crate::store::WallpaperStore;
use rmsvc_core::fswatch::{self, WatchMask, WatchSpec};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 两次轮换的最短间隔：同一次休眠里 xochitl 若重复读，只算一次。按**含休眠**的开机时长（`/proc/uptime`，
/// CLOCK_BOOTTIME）计：09-25 前用 `Instant`（CLOCK_MONOTONIC，休眠时不走）——休眠几小时后唤醒、10 秒内又
/// 按电源键休眠，单调时钟只走了几秒，这次读图被当成"同一次休眠的重复读"跳过，下次休眠又显示同一张。
const MIN_ROLL_GAP: Duration = Duration::from_secs(10);
/// 一开始挂不上监听（目录还没建好等）时的重试间隔上下限。
const RETRY_MIN: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);

/// 只听"有人只读地打开又关上了"，每批事件读到就处理（不攒：一次休眠就一次读，攒了只会拖慢轮换）。
const SPEC: WatchSpec = WatchSpec { mask: WatchMask::CLOSE_NOWRITE, debounce: None };

/// 距上次轮换够不够久（`last=None` 表示还没轮换过）。时刻为含休眠的开机秒数（见 [`MIN_ROLL_GAP`]）。
pub fn gap_ok(last: Option<f64>, now: f64) -> bool {
    last.is_none_or(|t| now - t >= MIN_ROLL_GAP.as_secs_f64())
}

/// `/proc/uptime` 第一列（含休眠）；读不到（非 Linux 测试环境等）退回进程内单调时钟——只影响去重判定。
fn boottime_secs() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    rmsvc_core::proc::uptime_secs(std::path::Path::new("/proc")).unwrap_or_else(|| START.get_or_init(Instant::now).elapsed().as_secs_f64())
}

/// 一批事件（文件名集合）到了：里面有 `current.png` 且离上次轮换够久就轮换。`last` 跨监听重挂保留。
fn on_events(names: &HashSet<String>, current_name: &str, last: &mut Option<f64>, store: &WallpaperStore, bus: &rmsvc_core::events::EventBus) {
    if !names.contains(current_name) {
        return; // 含重挂后的那次空集合回调：重挂期间的读看不到，也不该补轮换
    }
    let now = boottime_secs();
    if !gap_ok(*last, now) {
        return;
    }
    *last = Some(now);
    match store.roll() {
        Ok(Some(n)) => {
            println!("[wallpaper-serve] 休眠屏已读 → 轮换到 {n}");
            bus.publish("wallpapers", "pool");
        }
        Ok(None) => {}
        Err(e) => eprintln!("[wallpaper-serve] 轮换失败: {e}"),
    }
}

pub fn spawn(store: Arc<WallpaperStore>, bus: Arc<rmsvc_core::events::EventBus>) {
    std::thread::spawn(move || {
        let cur = store.current_path().to_path_buf();
        let (Some(dir), Some(current_name)) = (cur.parent(), cur.file_name().and_then(|n| n.to_str())) else {
            eprintln!("[wallpaper-serve] {} 不像个文件路径，不监听休眠", cur.display());
            return;
        };
        let mut last: Option<f64> = None;
        let mut wait = RETRY_MIN;
        loop {
            let _ = store.ensure(); // 目录不在就建上，免得监听一直挂不上
            // 挂上之后不再返回（目录失效由 fswatch 自己等它回来重挂）；返回＝没挂上。
            fswatch::watch_with(dir, &SPEC, |names| on_events(names, current_name, &mut last, &store, &bus));
            eprintln!("[wallpaper-serve] 监听 {} 失败，{}s 后重试", dir.display(), wait.as_secs());
            std::thread::sleep(wait);
            wait = (wait * 2).min(RETRY_MAX);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_batches_naming_current_png_roll() {
        let t = tempfile::tempdir().unwrap();
        let store = WallpaperStore::new(&rmsvc_core::paths::Paths::sandbox(t.path()));
        store.ensure().unwrap();
        for (n, v) in [("a.png", 10), ("b.png", 200)] {
            std::fs::write(store.pool().join(n), png(v)).unwrap();
        }
        store.activate("a.png").unwrap();
        let bus = rmsvc_core::events::EventBus::new();
        let mut last = None;
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        // 重挂后的空集合、别的文件被读：都不轮换
        on_events(&names(&[]), "current.png", &mut last, &store, &bus);
        on_events(&names(&["pool"]), "current.png", &mut last, &store, &bus);
        assert_eq!((store.state().current.as_deref(), last), (Some("a.png"), None));
        on_events(&names(&["pool", "current.png"]), "current.png", &mut last, &store, &bus);
        assert_eq!(store.state().current.as_deref(), Some("b.png"));
        assert!(last.is_some());
        // 同一次休眠重复读：间隔内不再轮换
        on_events(&names(&["current.png"]), "current.png", &mut last, &store, &bus);
        assert_eq!(store.state().current.as_deref(), Some("b.png"));
    }

    #[test]
    fn repeated_reads_within_gap_roll_once() {
        let t0 = 1_000.0;
        assert!(gap_ok(None, t0));
        assert!(!gap_ok(Some(t0), t0 + 3.0));
        assert!(gap_ok(Some(t0), t0 + MIN_ROLL_GAP.as_secs_f64()));
    }

    #[test]
    fn boottime_is_positive_and_monotone() {
        let a = boottime_secs();
        let b = boottime_secs();
        assert!(a > 0.0 && b >= a);
    }

    fn png(v: u8) -> Vec<u8> {
        let img = image::GrayImage::from_pixel(8, 8, image::Luma([v]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn wait_current(store: &WallpaperStore, want: &str) -> bool {
        for _ in 0..100 {
            if store.state().current.as_deref() == Some(want) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// 真 inotify：壁纸目录被整个删掉（监听随之失效）→ 上传把目录建回来后监听重新挂上，休眠读图照旧轮换。
    /// 此前（09-30 起）是 watch_once 返回错误、由本模块的重试循环重建目录和监听；2026-10-10 起交给 fswatch 重挂。
    #[test]
    fn rolls_again_after_dir_removed_and_recreated() {
        let t = tempfile::tempdir().unwrap();
        let paths = rmsvc_core::paths::Paths::sandbox(t.path());
        let store = Arc::new(WallpaperStore::new(&paths));
        store.ensure().unwrap();
        let dir = store.current_path().parent().unwrap().to_path_buf();
        spawn(store.clone(), Arc::new(rmsvc_core::events::EventBus::new()));
        std::thread::sleep(Duration::from_millis(300)); // 等监听建好
        std::fs::remove_dir_all(&dir).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // 上传入池会建回目录（write_atomic 建父目录）；状态文件不在壁纸目录下，删目录不影响它
        for (n, v) in [("a.png", 10), ("b.png", 200)] {
            rmsvc_core::fs::write_atomic(&store.pool().join(n), &png(v)).unwrap();
        }
        store.activate("a.png").unwrap();
        // fswatch 重挂退避 1 秒起：反复模拟休眠读图，直到轮换发生（最多 5 秒）
        let t0 = Instant::now();
        while store.state().current.as_deref() != Some("b.png") {
            assert!(t0.elapsed() < Duration::from_secs(5), "目录回来后应重新监听并轮换");
            let _ = std::fs::read(store.current_path()).unwrap();
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// 一开始目录就不在：spawn 先建目录再挂监听，照常轮换。
    #[test]
    fn creates_missing_dir_then_watches() {
        let t = tempfile::tempdir().unwrap();
        let paths = rmsvc_core::paths::Paths::sandbox(t.path());
        let store = Arc::new(WallpaperStore::new(&paths));
        let dir = store.current_path().parent().unwrap().to_path_buf();
        assert!(!dir.exists());
        spawn(store.clone(), Arc::new(rmsvc_core::events::EventBus::new()));
        std::thread::sleep(Duration::from_millis(300));
        assert!(store.pool().is_dir(), "spawn 应先建好目录");
        for (n, v) in [("a.png", 10), ("b.png", 200)] {
            std::fs::write(store.pool().join(n), png(v)).unwrap();
        }
        store.activate("a.png").unwrap();
        let _ = std::fs::read(store.current_path()).unwrap();
        assert!(wait_current(&store, "b.png"));
    }

    /// 真 inotify：别人读完 current.png → 轮换；本服务写 current.png、读池图 → 不触发；同一次休眠重复读只轮换一次。
    #[test]
    fn real_inotify_rolls_on_read_but_not_on_own_write() {
        let t = tempfile::tempdir().unwrap();
        let paths = rmsvc_core::paths::Paths::sandbox(t.path());
        let store = Arc::new(WallpaperStore::new(&paths));
        store.ensure().unwrap();
        std::fs::write(store.pool().join("a.png"), png(10)).unwrap();
        std::fs::write(store.pool().join("b.png"), png(200)).unwrap();
        store.activate("a.png").unwrap();
        spawn(store.clone(), Arc::new(rmsvc_core::events::EventBus::new()));
        std::thread::sleep(Duration::from_millis(300)); // 等监听建好

        // 本服务自己写 current.png + 读池图：不该触发轮换
        store.activate("a.png").unwrap();
        let _ = store.read("b.png").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(store.state().current.as_deref(), Some("a.png"));

        // 模拟 xochitl 休眠时读 current.png → 轮换到 b
        let _ = std::fs::read(store.current_path()).unwrap();
        assert!(wait_current(&store, "b.png"));
        // 紧接着又读一次（同一次休眠）：间隔内不再轮换
        let _ = std::fs::read(store.current_path()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(store.state().current.as_deref(), Some("b.png"));
    }
}
