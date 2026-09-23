//! 唤醒源事件（设备/子系统级）：直接读 `/dev/kmsg` 内核环形缓冲区里的 `PM: active wakeup source: <NAME>`，
//! 落 wakes.tsv 缓存。①按小时级缓存（wakes.tsv 超 50 分钟才刷新）；②`/dev/kmsg` 本身是环形缓冲区，
//! 只有"当前这次开机"的记录——不再有旧实现（fork `journalctl` 查 31 天内核日志）那种跨 boot 查询能力，
//! 这是明知的退化。换来的是彻底消灭"采样进程里唯一的子进程创建"：2026-08-29 真机硬冻结的内核证据是
//! `cgroup_procs_write → percpu_down_write(cgroup_threadgroup_rwsem) → synchronize_rcu` 卡死（见
//! FINDINGS，那次是 systemd 反复拉起 oneshot 服务时的 cgroup 迁移，常驻化已经根治）；2026-09-23 又在
//! 常驻模型下复现一次冻机，冻结前最后一条日志与 wakes.tsv 的刷新时间戳精确重合到秒——不是同一个
//! `cgroup_procs_write` 机制（fork 子进程默认继承父进程 cgroup，不会走那条 syscall），但时间相关性足够
//! 可疑：进程创建（fork/exec/wait）本身仍是这条常驻循环里唯一残留的"非纯内存操作"，干脆去掉，不再赌它
//! 跟内核那条罕见路径有没有关系。
use crate::util::san;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// 缓存保留窗口（与面板最大的 30d 窗口对齐并留 1 天余量）。`/dev/kmsg` 实际只有当前这次开机的记录，
/// 这个窗口现在的意义是"清理 wakes.tsv 里过老的行"，不再代表真能查到这么久——开机越久，这个窗口越接近
/// 真实覆盖范围。
const KEEP_SECS: u64 = 31 * 86400;
/// wakes.tsv 多久没刷新算过期。
const STALE_SECS: u64 = 3000;
/// Linux 通用 ABI（x86/arm/aarch64 等，本设备 aarch64 在内）统一的 `O_NONBLOCK` 值。没有 libc crate
/// （battop 保持零依赖），直接量入常量——非阻塞是为了读完环形缓冲区当前内容后立即返回，不等下一条
/// 未来才会出现的新内核消息（否则会拖死整个采样循环，重蹈 `run_bounded` 当初要防的那类问题）。
const O_NONBLOCK: i32 = 0o4000;
/// 单条 kmsg 记录的读取缓冲；内核消息通常远小于此，超长记录会被内核截断，可接受（丢的是我们不关心
/// 的字段，`active wakeup source: ` 这条消息很短，不会被截）。
const KMSG_BUF: usize = 8192;

/// 解析单条 `/dev/kmsg` 记录（格式 `<pri>,<seq>,<ts_us>,<flags>[,extra];<message>`，`message` 后可能还
/// 跟着结构化字段续行，只看第一行），命中唤醒源就返回 (epoch 秒, 源名)。`boot_epoch` 把记录自带的
/// "开机以来微秒数"换算成墙钟 epoch。
fn parse_kmsg_record(raw: &[u8], boot_epoch: u64) -> Option<(u64, String)> {
    const KEY: &str = "active wakeup source: ";
    let text = String::from_utf8_lossy(raw);
    let first_line = text.lines().next()?;
    let (header, msg) = first_line.split_once(';')?;
    let ts_us: u64 = header.split(',').nth(2)?.parse().ok()?;
    let idx = msg.find(KEY)?;
    let name = msg[idx + KEY.len()..].trim();
    if name.is_empty() {
        return None;
    }
    Some((boot_epoch + ts_us / 1_000_000, name.to_string()))
}

/// 读当前 `/dev/kmsg` 环形缓冲区里 epoch >= `since` 的唤醒源事件。None = 打开/读取失败（调用方保留旧
/// 缓存）。非阻塞读到 `WouldBlock`（缓冲区当前内容已读完）即停，不等待未来的新消息。
fn read_wake_events(since: u64) -> Option<Vec<(u64, String)>> {
    let boot_epoch = crate::util::now_secs().saturating_sub(crate::procs::read_uptime());
    let mut f = OpenOptions::new().read(true).custom_flags(O_NONBLOCK).open("/dev/kmsg").ok()?;
    // 显式定位到缓冲区最早的记录（内核对 /dev/kmsg 的 SEEK_SET 有专门语义），不依赖新 fd 的默认位置；
    // 失败（比如内核不支持这个 seek）就照旧从当前位置读，尽力而为。
    let _ = f.seek(SeekFrom::Start(0));
    let mut out = Vec::new();
    let mut buf = [0u8; KMSG_BUF];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(ev) = parse_kmsg_record(&buf[..n], boot_epoch) {
                    if ev.0 >= since {
                        out.push(ev);
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
    Some(out)
}

/// 增量合并：`cache` 里 epoch >= `since` 的条目丢弃（由 `fresh` 整段重读替换，`--since` 是闭区间，同一秒的
/// 事件不会重复也不会漏），再并入 `fresh`，最后裁掉 `cutoff` 之前的。
pub fn merge_events(cache: Vec<(u64, String)>, fresh: Vec<(u64, String)>, since: u64, cutoff: u64) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = cache.into_iter().filter(|(e, _)| *e < since).collect();
    out.extend(fresh);
    out.retain(|(e, _)| *e >= cutoff);
    out
}

/// 增量读回来的是空、而缓存里明明有 `>= since` 的事件：健康情况下至少会把边界那条事件读回来，所以这
/// 多半是 `/dev/kmsg` 打不开/读取出错（`read_wake_events` 已经在这些情况下提前返回 None，这个判据主要
/// 兜底"打开成功但环形缓冲区被其它读者/日志轮转抢先翻过去了"这种边缘情况）。
fn looks_broken(cache: &[(u64, String)], fresh: &[(u64, String)], since: u64) -> bool {
    fresh.is_empty() && cache.iter().any(|(e, _)| *e >= since)
}

/// wakes.tsv 是否需要刷新（不存在 / 超 STALE_SECS）。
fn is_stale(dir: &Path, now: u64) -> bool {
    fs::metadata(dir.join("wakes.tsv"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| now.saturating_sub(d.as_secs()) > STALE_SECS)
        .unwrap_or(true)
}

/// 刷新唤醒缓存（仅在过期时）：增量读 `/dev/kmsg` → 合并 → 写 wakes.tsv（epoch \t 原始源名）。
/// 读取失败/打不开**不动旧缓存**（旧实现此处会把空结果写成空文件，丢光已缓存的唤醒历史）。
pub fn refresh_if_stale(dir: &Path, now: u64) {
    if !is_stale(dir, now) {
        return;
    }
    let cache = load_cache(dir);
    let cutoff = now.saturating_sub(KEEP_SECS);
    // 增量起点 = 缓存里最新事件（钳到 now：时钟曾跳到未来时不至于一直问不到新事件）；无缓存 = 全窗口。
    let since = cache.iter().map(|(e, _)| *e).max().map(|m| m.min(now)).unwrap_or(cutoff).max(cutoff);
    let Some(fresh) = read_wake_events(since) else { return };
    if looks_broken(&cache, &fresh, since) {
        return;
    }
    write_cache(dir, &merge_events(cache, fresh, since, cutoff));
}

fn write_cache(dir: &Path, ev: &[(u64, String)]) {
    let mut s = String::new();
    for (ep, name) in ev {
        s.push_str(&format!("{ep}\t{}\n", san(name)));
    }
    let tmp = dir.join("wakes.tsv.tmp");
    if fs::write(&tmp, s).is_ok() {
        let _ = fs::rename(&tmp, dir.join("wakes.tsv"));
    }
}

/// 读唤醒缓存。
pub fn load_cache(dir: &Path) -> Vec<(u64, String)> {
    let Ok(content) = fs::read_to_string(dir.join("wakes.tsv")) else { return Vec::new() };
    content
        .lines()
        .filter_map(|line| {
            let (e, n) = line.split_once('\t')?;
            Some((e.parse::<u64>().ok()?, n.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kmsg_record_wake_source() {
        let boot_epoch = 1_760_000_000 - 100;
        let raw = b"6,1234,100000000,-;PM: active wakeup source: mwlan";
        assert_eq!(parse_kmsg_record(raw, boot_epoch), Some((1_760_000_000, "mwlan".to_string())));
    }

    #[test]
    fn parse_kmsg_record_handles_continuation_lines() {
        // dictionary 续行（SUBSYSTEM=/DEVICE=）只看第一行，不影响解析。
        let raw = b"6,1234,100000000,-;PM: active wakeup source: spi1.0\n SUBSYSTEM=platform\n DEVICE=+platform:spi1.0";
        assert_eq!(parse_kmsg_record(raw, 0), Some((100, "spi1.0".to_string())));
    }

    #[test]
    fn parse_kmsg_record_trims_whitespace_in_name() {
        let raw = b"6,1,0,-;PM: active wakeup source:   spi1.0  ";
        assert_eq!(parse_kmsg_record(raw, 0), Some((0, "spi1.0".to_string())));
    }

    #[test]
    fn parse_kmsg_record_ignores_unrelated_message() {
        assert_eq!(parse_kmsg_record(b"6,1,0,-;unrelated kernel message", 0), None);
    }

    #[test]
    fn parse_kmsg_record_rejects_empty_name() {
        assert_eq!(parse_kmsg_record(b"6,1,0,-;PM: active wakeup source: ", 0), None);
    }

    #[test]
    fn parse_kmsg_record_bad_header_is_none() {
        assert_eq!(parse_kmsg_record(b"garbage no semicolon", 0), None);
        assert_eq!(parse_kmsg_record(b"6,1;PM: active wakeup source: x", 0), None); // 缺 ts_us 字段
        assert_eq!(parse_kmsg_record(b"6,1,notanumber,-;PM: active wakeup source: x", 0), None);
    }

    fn ev(v: &[(u64, &str)]) -> Vec<(u64, String)> {
        v.iter().map(|(e, n)| (*e, n.to_string())).collect()
    }

    #[test]
    fn merge_replaces_tail_and_trims_old() {
        let cache = ev(&[(50, "old"), (100, "a"), (200, "b"), (200, "b2")]);
        // since=200：缓存里 >=200 的被 fresh（含同秒全部事件）替换；<cutoff(80) 的被裁
        let fresh = ev(&[(200, "b"), (200, "b2"), (250, "c")]);
        let m = merge_events(cache, fresh, 200, 80);
        assert_eq!(m, ev(&[(100, "a"), (200, "b"), (200, "b2"), (250, "c")]));
    }

    #[test]
    fn merge_is_idempotent_when_nothing_new() {
        let cache = ev(&[(100, "a"), (200, "b")]);
        let m = merge_events(cache.clone(), ev(&[(200, "b")]), 200, 0);
        assert_eq!(m, cache);
    }

    #[test]
    fn empty_readback_with_cached_tail_is_treated_as_journal_failure() {
        let cache = ev(&[(100, "a"), (200, "b")]);
        assert!(looks_broken(&cache, &[], 200), "边界事件 b 应被读回，空 = 出错");
        assert!(!looks_broken(&cache, &ev(&[(200, "b")]), 200));
        assert!(!looks_broken(&[], &[], 50), "首次且窗口内确实没事件：正常");
        assert!(!looks_broken(&cache, &[], 300), "since 已越过缓存最新事件：空 = 没有新事件");
    }

    #[test]
    fn cache_roundtrip_skips_garbage() {
        let t = crate::util::testutil::tmp();
        std::fs::write(t.path().join("wakes.tsv"), "100\ta\nbad line\nxx\tb\n200\tc d\n").unwrap();
        assert_eq!(load_cache(t.path()), ev(&[(100, "a"), (200, "c d")]));
        write_cache(t.path(), &ev(&[(1, "x\ty")]));
        assert_eq!(load_cache(t.path()), ev(&[(1, "x y")]));
    }

    #[test]
    fn stale_when_missing_or_old() {
        let t = crate::util::testutil::tmp();
        assert!(is_stale(t.path(), 1_000_000_000));
        write_cache(t.path(), &[]);
        let now = crate::util::now_secs();
        assert!(!is_stale(t.path(), now));
        assert!(is_stale(t.path(), now + STALE_SECS + 5));
    }
}
