//! inotify 防抖目录监听（单层目录）。**剥离移植**自旧项目 device-core `fswatch.rs` 的省电思路：
//! 空闲时阻塞读事件（进程睡死、零唤醒，配合设备 suspend），只有真有写入后才在防抖窗口内定时等待，
//! 静默结束回调一次并带上受影响的文件名集合。书架用它给 spool 目录追平（如上传中断留下的半成品）。
use inotify::{Inotify, WatchMask};
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// 监听 `dir` 下文件的 CLOSE_WRITE/MOVED_TO/CREATE，防抖后回调。**永不返回**（init 失败返回）。
pub fn watch_debounced<F>(dir: &Path, debounce: Duration, mut on_settle: F)
where
    F: FnMut(&HashSet<String>),
{
    let mut inotify = match Inotify::init() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[fswatch] inotify init 失败: {e}");
            return;
        }
    };
    if let Err(e) = inotify.watches().add(dir, WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO | WatchMask::CREATE) {
        eprintln!("[fswatch] watch {} 失败: {e}", dir.display());
        return;
    }
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            let events = match inotify.read_events_blocking(&mut buf) {
                Ok(ev) => ev,
                Err(e) => {
                    eprintln!("[fswatch] 读事件失败: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            for ev in events {
                if let Some(name) = ev.name.and_then(|n| n.to_str().map(|s| s.to_string())) {
                    if tx.send(name).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let mut dirty: HashSet<String> = HashSet::new();
    let mut last = Instant::now();
    loop {
        if dirty.is_empty() {
            match rx.recv() {
                Ok(n) => {
                    dirty.insert(n);
                    last = Instant::now();
                }
                Err(_) => return,
            }
            continue;
        }
        let elapsed = last.elapsed();
        if elapsed >= debounce {
            on_settle(&dirty);
            dirty.clear();
            continue;
        }
        match rx.recv_timeout(debounce - elapsed) {
            Ok(n) => {
                dirty.insert(n);
                last = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
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
}
