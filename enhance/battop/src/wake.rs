//! 唤醒源事件(设备/子系统级)：从 journal 内核日志读 `PM: active wakeup source: <NAME>`，落 wakes.tsv 缓存。
//! journalctl 是采样进程里唯一的 fork：读内核日志较贵，故 ①按小时级缓存（wakes.tsv 超 50 分钟才刷新），
//! ②**增量读取**（只问上次缓存最新事件之后的部分，不再每次重读 31 天）。
//! 格式："<epoch>.<us> host kernel: PM: active wakeup source: <NAME>"。
use crate::util::san;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, UNIX_EPOCH};

/// 缓存保留窗口（与面板最大的 30d 窗口对齐并留 1 天余量）。
const KEEP_SECS: u64 = 31 * 86400;
/// wakes.tsv 多久没刷新算过期。
const STALE_SECS: u64 = 3000;

/// 有界执行子进程：读满 stdout 或超 `secs` 秒即 SIGKILL，返回 stdout 字节；spawn 失败/超时返回 None。
/// 纯 std：读线程排空管道（防子进程写满 pipe 阻塞成 D 态），主线程 recv_timeout 计时。常驻模型下
/// 一个卡住的 journalctl 会拖死整个采样循环，故所有外部子进程必须有界。
pub fn run_bounded(mut cmd: Command, secs: u64) -> Option<Vec<u8>> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        let _ = tx.send(buf); // rx 可能已 drop（超时路径）→ 忽略
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(buf) => {
            let _ = child.wait();
            Some(buf)
        }
        Err(_) => {
            let _ = child.kill(); // 超时 → 杀子进程，绝不让循环无限阻塞
            let _ = child.wait();
            None
        }
    }
}

/// 解析 journalctl `-o short-unix` 输出里的唤醒源行 → (epoch 秒, 原始源名)。
pub fn parse_events(text: &str) -> Vec<(u64, String)> {
    const KEY: &str = "active wakeup source: ";
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(idx) = line.find(KEY) else { continue };
        let name = line[idx + KEY.len()..].trim();
        if name.is_empty() {
            continue;
        }
        // 行首是 epoch(可能带 .微秒)
        let epoch: u64 = line
            .split_whitespace()
            .next()
            .and_then(|t| t.split('.').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if epoch == 0 {
            continue;
        }
        out.push((epoch, name.to_string()));
    }
    out
}

/// 读 journal 自 `since` 起的内核唤醒源事件。None = journalctl 起不来/超时（调用方保留旧缓存）。
fn read_wake_events(since: u64) -> Option<Vec<(u64, String)>> {
    // _TRANSPORT=kernel 跨 boot 取内核消息(不用 -k,那只当前 boot);--since 界定,输出有界
    let mut cmd = Command::new("journalctl");
    cmd.args(["-o", "short-unix", "--no-pager", "--since"]).arg(format!("@{since}")).arg("_TRANSPORT=kernel");
    // 有界执行：journald 卡住最多等 20s 就 kill，本轮跳过刷缓存（用旧 wakes.tsv），绝不卡死循环。
    let stdout = run_bounded(cmd, 20)?;
    Some(parse_events(&String::from_utf8_lossy(&stdout)))
}

/// 增量合并：`cache` 里 epoch >= `since` 的条目丢弃（由 `fresh` 整段重读替换，`--since` 是闭区间，同一秒的
/// 事件不会重复也不会漏），再并入 `fresh`，最后裁掉 `cutoff` 之前的。
pub fn merge_events(cache: Vec<(u64, String)>, fresh: Vec<(u64, String)>, since: u64, cutoff: u64) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = cache.into_iter().filter(|(e, _)| *e < since).collect();
    out.extend(fresh);
    out.retain(|(e, _)| *e >= cutoff);
    out
}

/// 增量读回来的是空、而缓存里明明有 `>= since` 的事件：`--since` 是闭区间，健康的 journalctl 至少会
/// 把边界那条事件读回来，所以这多半是 journalctl 出错（run_bounded 不看退出码）或 journal 被清理——
/// 此时合并会把缓存尾部替换成空，宁可保持旧缓存。
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

/// 刷新唤醒缓存（仅在过期时）：增量读 journal → 合并 → 写 wakes.tsv（epoch \t 原始源名）。
/// journalctl 失败/超时**不动旧缓存**（旧实现此处会把空结果写成空文件，丢光已缓存的唤醒历史）。
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
    fn parses_kernel_wake_lines() {
        let text = "\
1760000001.123456 rm kernel: PM: active wakeup source: mwlan
1760000002.000000 rm kernel: unrelated
-- Journal begins at ... --
1760000003.5 rm kernel: PM: active wakeup source:   spi1.0
0.1 rm kernel: PM: active wakeup source: badepoch
1760000004 rm kernel: PM: active wakeup source:
";
        assert_eq!(parse_events(text), vec![(1_760_000_001, "mwlan".to_string()), (1_760_000_003, "spi1.0".to_string())]);
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
        fs::write(t.path().join("wakes.tsv"), "100\ta\nbad line\nxx\tb\n200\tc d\n").unwrap();
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

    #[test]
    fn run_bounded_kills_hung_child_and_returns_output() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo hi"]);
        assert_eq!(run_bounded(c, 5).as_deref(), Some(&b"hi\n"[..]));
        let mut c = Command::new("sh");
        c.args(["-c", "sleep 30"]);
        let t0 = std::time::Instant::now();
        assert!(run_bounded(c, 1).is_none());
        assert!(t0.elapsed() < Duration::from_secs(10), "超时必须杀掉而不是等 30s");
        assert!(run_bounded(Command::new("/nonexistent/bin"), 1).is_none());
    }
}
