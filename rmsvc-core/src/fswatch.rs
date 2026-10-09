//! inotify 防抖目录监听（单层目录）。**剥离移植**自旧项目 device-core `fswatch.rs` 的省电思路：
//! 空闲时阻塞等事件（进程睡死、零唤醒，配合设备 suspend），只有真有写入后才在防抖窗口内定时等待，
//! 静默结束回调一次并带上受影响的文件名集合。书架用它给 spool 目录追平（如上传中断留下的半成品）。
//! 三种形态：`watch_debounced` 常驻永不返回；`watch_until` 限时、回调说"完了"就撤；`wait_for` 是 `watch_until`
//! 的"等某个条件成立"版——先挂监听再查一次、到点再查最后一次，调用方不必自己写"查 → 等 → 再查"三段式，
//! 也不会漏掉"查完到挂上监听之间"发生的变化。后两者用于有头有尾的等待（投原生后等 xochitl 建好条目/渲染完），
//! 不给书库目录留常驻监听（§03z"不监听全盘"约束）。
//!
//! 实现：本线程 `poll` inotify 描述符（带超时）再非阻塞读（2026-10-09）。此前另起一条读线程阻塞在
//! `read_events_blocking` 上、经通道把文件名递回来；限时模式结束时读线程醒不来，只能往 `/tmp` 下一个私有
//! "踢醒"目录写文件把它踢醒，建不了踢醒目录时读线程和 inotify 描述符就一直挂到目录下次有动静。现在每个监听
//! 只占调用方这一条线程，返回即释放 inotify，不再碰 `/tmp`。
use inotify::{Inotify, WatchMask};
use std::collections::HashSet;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, Instant};

const MASK: WatchMask = WatchMask::CLOSE_WRITE.union(WatchMask::MOVED_TO).union(WatchMask::CREATE).union(WatchMask::DELETE).union(WatchMask::MOVED_FROM);

/// [`wait_for`] 在 inotify 不可用时退化成轮询的间隔（只为稳健，设备内核一定支持 inotify）。
const FALLBACK_POLL: Duration = Duration::from_secs(1);

/// 监听 `dir` 下文件的 CLOSE_WRITE/MOVED_TO/CREATE/DELETE/MOVED_FROM，防抖后回调（删除也算：网关看注册表目录用）。**永不返回**（init 失败返回）。
pub fn watch_debounced<F>(dir: &Path, debounce: Duration, mut on_settle: F)
where
    F: FnMut(&HashSet<String>),
{
    run(dir, debounce, None, false, |s| {
        on_settle(s);
        false
    });
}

/// 限时监听：同一套事件/防抖，但回调返回 `true` 即结束（本函数返回 `true`）；到 `timeout` 还没结束返回 `false`。
/// 返回时监听随之撤掉（inotify 释放）。inotify 起不来时立即返回 `false`。
pub fn watch_until<F>(dir: &Path, debounce: Duration, timeout: Duration, on_settle: F) -> bool
where
    F: FnMut(&HashSet<String>) -> bool,
{
    run(dir, debounce, Some(Instant::now() + timeout), false, on_settle).unwrap_or(false)
}

/// 等 `dir` 里出现某个结果：`check` 返回 `Some` 即结束并把它交回；最多等 `timeout`，到点仍没有 → `None`。
/// 次序：**先挂监听，再查一次**（挂上之后的变化都会排进 inotify 队列，不会漏），之后每次目录变化静默 `debounce`
/// 再查；到点再查最后一次。inotify 起不来时退化成每秒查一次，语义不变。
pub fn wait_for<T>(dir: &Path, debounce: Duration, timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    let mut found = None;
    let watched = run(dir, debounce, Some(deadline), true, |_| {
        found = check();
        found.is_some()
    });
    if found.is_some() {
        return found;
    }
    if watched.is_none() {
        loop {
            if let Some(v) = check() {
                return Some(v);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            std::thread::sleep(FALLBACK_POLL.min(deadline - now));
        }
    }
    check()
}

/// 一个目录上的 inotify 监听（非阻塞描述符，由调用方 `poll` 着等）。
struct Watcher {
    inotify: Inotify,
    buf: [u8; 4096],
}

impl Watcher {
    fn open(dir: &Path) -> Option<Watcher> {
        let inotify = match Inotify::init() {
            Ok(i) => i,
            Err(e) => {
                eprintln!("[fswatch] inotify init 失败: {e}");
                return None;
            }
        };
        if let Err(e) = inotify.watches().add(dir, MASK) {
            eprintln!("[fswatch] watch {} 失败: {e}", dir.display());
            return None;
        }
        Some(Watcher { inotify, buf: [0; 4096] })
    }

    /// 最多等 `timeout`（`None`＝无限等）；把这期间到的带文件名的事件并进 `dirty`，返回是否并进了新名字。
    fn wait(&mut self, timeout: Option<Duration>, dirty: &mut HashSet<String>) -> bool {
        if !crate::sys::poll_readable(&[self.inotify.as_raw_fd()], timeout)[0] {
            return false;
        }
        let mut got = false;
        loop {
            match self.inotify.read_events(&mut self.buf) {
                Ok(events) => {
                    for ev in events {
                        if let Some(name) = ev.name.and_then(|n| n.to_str()) {
                            dirty.insert(name.to_string());
                            got = true;
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return got,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    // 不该发生；歇一下，别让持续报错的描述符把循环变成忙等。
                    eprintln!("[fswatch] 读事件失败: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                    return got;
                }
            }
        }
    }
}

/// 共同的防抖循环。返回 `None`＝监听没挂上；`Some(true)`＝回调说完了；`Some(false)`＝到点。
/// `check_first`：挂上监听后先以空集合调一次回调（[`wait_for`] 用）。
fn run<F>(dir: &Path, debounce: Duration, deadline: Option<Instant>, check_first: bool, mut on_settle: F) -> Option<bool>
where
    F: FnMut(&HashSet<String>) -> bool,
{
    let mut w = Watcher::open(dir)?;
    let mut dirty: HashSet<String> = HashSet::new();
    if check_first && on_settle(&dirty) {
        return Some(true);
    }
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        if deadline.is_some_and(|d| now >= d) {
            return Some(false);
        }
        let remain = deadline.map(|d| d - now);
        let wait = if dirty.is_empty() {
            remain
        } else {
            let elapsed = last.elapsed();
            if elapsed >= debounce {
                if on_settle(&dirty) {
                    return Some(true);
                }
                dirty.clear();
                continue;
            }
            Some(remain.map_or(debounce - elapsed, |r| r.min(debounce - elapsed)))
        };
        if w.wait(wait, &mut dirty) {
            last = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn coalesces_burst_into_one_callback() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().to_path_buf();
        let seen: Arc<Mutex<Vec<HashSet<String>>>> = Arc::new(Mutex::new(vec![]));
        let seen2 = seen.clone();
        let d2 = dir.clone();
        std::thread::spawn(move || {
            watch_debounced(&d2, Duration::from_millis(200), move |s| seen2.lock().unwrap().push(s.clone()));
        });
        std::thread::sleep(Duration::from_millis(150));
        for n in ["a.epub", "b.epub", "a.epub"] {
            std::fs::write(dir.join(n), b"x").unwrap();
            std::thread::sleep(Duration::from_millis(30));
        }
        std::thread::sleep(Duration::from_millis(600));
        let got = seen.lock().unwrap();
        assert_eq!(got.len(), 1, "应合并成一次回调: {:?}", *got);
        assert_eq!(got[0], ["a.epub", "b.epub"].into_iter().map(String::from).collect::<HashSet<_>>());
    }

    #[test]
    fn watch_until_returns_true_when_callback_done_and_false_on_timeout() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().to_path_buf();
        let d2 = dir.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            std::fs::write(d2.join("x.content"), b"{}").unwrap();
        });
        let t0 = Instant::now();
        let ok = watch_until(&dir, Duration::from_millis(50), Duration::from_secs(5), |s| s.contains("x.content"));
        assert!(ok && t0.elapsed() < Duration::from_secs(2), "看到文件即结束");
        let t0 = Instant::now();
        let ok = watch_until(&dir, Duration::from_millis(50), Duration::from_millis(300), |_| true);
        assert!(!ok && t0.elapsed() >= Duration::from_millis(300) && t0.elapsed() < Duration::from_secs(2), "没动静到点返回 false");
    }

    #[test]
    fn wait_for_checks_first_then_on_change_then_at_deadline() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().to_path_buf();
        // 条件一开始就成立：不等任何事件
        let t0 = Instant::now();
        assert_eq!(wait_for(&dir, Duration::from_millis(50), Duration::from_secs(5), || Some(1)), Some(1));
        assert!(t0.elapsed() < Duration::from_secs(1));
        // 之后才出现：目录变化唤醒，远早于超时
        let d2 = dir.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            std::fs::write(d2.join("x.metadata"), b"{}").unwrap();
        });
        let t0 = Instant::now();
        let got = wait_for(&dir, Duration::from_millis(50), Duration::from_secs(5), || dir.join("x.metadata").exists().then_some("x"));
        assert_eq!(got, Some("x"));
        assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
        // 没动静：到点 None，且到点还会再查最后一次（这里第二次查询才成立）
        let mut n = 0;
        let got = wait_for(&dir, Duration::from_millis(50), Duration::from_millis(200), || {
            n += 1;
            (n >= 2).then_some(n)
        });
        assert_eq!(got, Some(2), "首查失败、期间无事件、到点那次成立");
    }

    #[test]
    fn wait_for_falls_back_to_polling_when_watch_cannot_be_set() {
        let t = tempfile::tempdir().unwrap();
        let missing = t.path().join("no-such-dir");
        let mut n = 0;
        let t0 = Instant::now();
        let got = wait_for(&missing, Duration::from_millis(50), Duration::from_secs(5), || {
            n += 1;
            (n >= 2).then_some(n)
        });
        assert_eq!(got, Some(2));
        assert!(t0.elapsed() >= Duration::from_millis(900) && t0.elapsed() < Duration::from_secs(3), "按 1 秒间隔轮询: {:?}", t0.elapsed());
        assert!(!watch_until(&missing, Duration::from_millis(50), Duration::from_secs(5), |_| true), "watch_until 挂不上监听照旧立即 false");
    }
}
