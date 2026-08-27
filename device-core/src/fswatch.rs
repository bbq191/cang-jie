//! 事件驱动 + 防抖 的 xochitl 文档目录监听（替代周期轮询，省电）。
//!
//! 为什么不用定时扫描：reMarkable 靠深度睡眠(suspend-to-RAM)续航，周期性醒来解析是电池刺客。
//! 本模块用 inotify 事件驱动——**空闲时阻塞读**（进程睡死、零唤醒，配合设备 suspend 不掉电），
//! 只有真发生写入后、在防抖静默窗口内才用定时等待，静默结束回调一次。
//!
//! 防抖粒度=按文档：coalesce 一串写入（一次划线/翻页可能落多个 .rm 写事件），
//! 直到某文档"静默超过 debounce"才回调一次，携带受影响的 doc uuid 集合。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};

/// 哨兵 uuid：config 文件（如 reading-qol.json）发生写入时经 channel 送出。
/// 调用方在 dirty 集里见到它即知"开关被改过"，应做**全库重扫**而非增量。
/// 用 NUL 前缀确保永不与真实 doc uuid（十六进制，无 NUL）冲突。
pub const CONFIG_SIGNAL: &str = "\0__config__";

/// xochitl 存储里我们关心的事件：新建/写完关闭/移入移出/删除。
fn watch_mask() -> WatchMask {
    // ISDIR 不是可设的 watch 掩码，只在返回事件的 mask 里体现，故这里不设。
    WatchMask::CREATE
        | WatchMask::CLOSE_WRITE
        | WatchMask::MOVED_TO
        | WatchMask::MOVED_FROM
        | WatchMask::DELETE
}

/// 给 dir 及其所有子目录（文档的页目录 <uuid>/）递归下 watch。inotify 不递归，须手动铺。
fn add_recursive(
    watches: &mut inotify::Watches,
    dir: &Path,
    map: &mut HashMap<WatchDescriptor, PathBuf>,
) {
    if let Ok(wd) = watches.add(dir, watch_mask()) {
        map.insert(wd, dir.to_path_buf());
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                add_recursive(watches, &p, map);
            }
        }
    }
}

/// 从事件全路径抽出文档 uuid：xochitl 下要么是 `<uuid>.metadata` 等顶层文件、
/// 要么是 `<uuid>/<page>.rm` 子目录文件 —— 取相对 root 的第一段、去扩展名即 uuid（uuid 无点）。
fn doc_uuid(root: &Path, full: &Path) -> Option<String> {
    let rel = full.strip_prefix(root).ok()?;
    let first = rel.components().next()?.as_os_str();
    Path::new(first).file_stem()?.to_str().map(|s| s.to_string())
}

/// 监听 xochitl_dir，事件按 debounce 防抖后回调 on_settle(受影响 doc uuid 集合)。**永不返回**。
///
/// 空闲阻塞、settle 窗口定时——低功耗。传 log_only=true 时只打日志不回调（真机首轮观察 xochitl
/// 落 .rm 的真实事件序列用，遵"先只读打日志确认真实布局再接线"纪律）。
///
/// `config_file`：可选。给一个配置文件绝对路径（如 reading-qol.json），额外 inotify 监听其**所在目录**，
/// 该文件被写入时向回调送 [`CONFIG_SIGNAL`]（与 doc uuid 一样进 dirty 集，同样低功耗、零轮询）。
/// 用途：设置页改开关只写 config、不动文档树，靠它唤醒 daemon 并触发全库重扫。None=不监听配置。
pub fn watch_debounced<F>(
    xochitl_dir: &str,
    config_file: Option<&str>,
    debounce: Duration,
    log_only: bool,
    mut on_settle: F,
) where
    F: FnMut(&HashSet<String>),
{
    let mut inotify = match Inotify::init() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[fswatch] inotify init 失败: {e}");
            return;
        }
    };
    let root = PathBuf::from(xochitl_dir);
    let mut wd_map: HashMap<WatchDescriptor, PathBuf> = HashMap::new();
    add_recursive(&mut inotify.watches(), &root, &mut wd_map);

    // 可选：监听 config 文件所在目录（非递归即可，config 直接躺在目录里）。记住其 wd + 文件名，
    // 读线程据此把该文件的写事件翻译成 CONFIG_SIGNAL（其余同目录文件的写入忽略）。
    let mut config_wds: HashSet<WatchDescriptor> = HashSet::new();
    let mut config_basename: Option<String> = None;
    if let Some(cf) = config_file {
        let cf_path = Path::new(cf);
        if let (Some(parent), Some(base)) = (cf_path.parent(), cf_path.file_name().and_then(|b| b.to_str())) {
            match inotify.watches().add(parent, watch_mask()) {
                Ok(wd) => {
                    config_wds.insert(wd);
                    config_basename = Some(base.to_string());
                    println!("[fswatch] 附加监听配置 {}（写入即触发全库重扫）", cf);
                }
                Err(e) => eprintln!("[fswatch] 配置目录 {} watch 失败: {e}（开关改动需重启 daemon 生效）", parent.display()),
            }
        }
    }
    println!(
        "[fswatch] 监听 {} （{} 个 watch，debounce {:?}{}）",
        xochitl_dir,
        wd_map.len(),
        debounce,
        if log_only { "，log-only 观察模式" } else { "" }
    );

    // 读线程：阻塞读事件 → 解析 doc uuid（或 config→CONFIG_SIGNAL）送 channel；见新目录补 watch。
    let (tx, rx) = mpsc::channel::<String>();
    let root_r = root.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            // 先把本轮事件抽成 owned（结束借用后才能再 watches().add）。
            let events = match inotify.read_events_blocking(&mut buf) {
                Ok(ev) => ev,
                Err(e) => {
                    eprintln!("[fswatch] read: {e}");
                    return;
                }
            };
            let mut collected: Vec<(WatchDescriptor, EventMask, Option<String>)> = Vec::new();
            for ev in events {
                collected.push((
                    ev.wd.clone(),
                    ev.mask,
                    ev.name.and_then(|n| n.to_str()).map(|s| s.to_string()),
                ));
            }
            for (wd, mask, name) in collected {
                // config 目录事件：只认目标文件名的写入 → CONFIG_SIGNAL（不参与 doc uuid 解析）。
                if config_wds.contains(&wd) {
                    if name.as_deref() == config_basename.as_deref() {
                        let _ = tx.send(CONFIG_SIGNAL.to_string());
                    }
                    continue;
                }
                let dir = match wd_map.get(&wd) {
                    Some(d) => d.clone(),
                    None => continue,
                };
                let full = match &name {
                    Some(n) => dir.join(n),
                    None => dir.clone(),
                };
                // 新建文档子目录 → 补 watch（否则新书的页 .rm 事件收不到）。
                if mask.contains(EventMask::ISDIR)
                    && (mask.contains(EventMask::CREATE) || mask.contains(EventMask::MOVED_TO))
                {
                    add_recursive(&mut inotify.watches(), &full, &mut wd_map);
                }
                if let Some(u) = doc_uuid(&root_r, &full) {
                    let _ = tx.send(u);
                }
            }
        }
    });

    // 防抖循环（本线程）：dirty 空 → 阻塞 recv（睡死）；dirty 非空 → 定时 recv_timeout 等静默。
    let mut dirty: HashSet<String> = HashSet::new();
    loop {
        if dirty.is_empty() {
            match rx.recv() {
                Ok(u) => {
                    dirty.insert(u);
                }
                Err(_) => return, // 读线程死了
            }
        } else {
            match rx.recv_timeout(debounce) {
                Ok(u) => {
                    dirty.insert(u); // 还在写，继续 coalesce
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if log_only {
                        println!("[fswatch] settle（log-only）: {:?}", dirty);
                    } else {
                        on_settle(&dirty);
                    }
                    dirty.clear();
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}
