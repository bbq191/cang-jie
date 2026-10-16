//! 扩展**实际**有没有加载进 xochitl（2026-09-24 审查补）：网页开关只反映 `reading-qol.json` 里的配置，看不出
//! `.so` 到底进没进进程——历史上两次"开关看着开了、其实没生效"（09-09 langhook 整个从设备上消失；GLIBC
//! 版本不符让 hw-stroke 静默加载失败）。这里直接看 xochitl 主进程的 `/proc/<pid>/maps`：映射了哪个
//! `extensions.d/*.so` 就是真加载了。
//!
//! qmd 补丁（如漫画页边距的 `shelf-comic-margins.qmd`）不是 `.so`，由 qt-resource-rebuilder 在 xochitl **启动时**读
//! 一次：qt-resource-rebuilder.so 在主进程里、qmd 文件在它的 exthome 目录、且文件修改时间早于 xochitl 启动时间，
//! 才算已载入；文件比进程新 = 改过但还没重启 xochitl，报"待重启"。这是按加载机制推断，看不到 qmd 里的
//! LOCATE 是否全部命中。
//!
//! 映射了不等于加载了（2026-10-16 补）：xovi 先映射 `.so` 再调它的 `_xovi_shouldLoad`，扩展认不出固件（如 OTA 后特征码
//! 不中）会自己拒绝加载、什么都不装，maps 里却照样有它。所以写缓存那一次另读 xochitl 启动时的 journal
//!（[`crate::device::health::xochitl_journal`]，同一进程只读一次），自己拒绝加载的从 `extensions` 挪到 `refused`。
//!
//! 主进程判定：`comm == "xochitl"` 且父进程是 1（systemd）。xochitl 渲染 PDF 时会 fork 出同名的
//! worker 子进程，父进程不是 1，排除。
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    /// 找到了 xochitl 主进程。
    pub xochitl: bool,
    /// 主进程里映射了 xovi.so（LD_PRELOAD 生效）。
    pub xovi: bool,
    /// 已映射且没有自己拒绝加载的 `extensions.d/` 下的扩展文件名（去重、排序），如 `hl-snap.so`。
    pub extensions: Vec<String>,
    /// 映射了、但自己在 `_xovi_shouldLoad` 里拒绝加载的扩展（见文件头）。
    pub refused: Vec<String>,
    /// 启动时已载入的 qmd 补丁文件名（排序）。
    pub qmds: Vec<String>,
    /// 存在但比 xochitl 进程新的 qmd（改过、还没重启 xochitl 生效）。
    pub qmds_pending: Vec<String>,
}

/// 读 `pid` 的 stat，是 xochitl 主进程（comm 为 xochitl 且父进程是 1）就返回它的 starttime（开机后的时钟滴答）。
/// 缓存命中时只核对这一个 pid，不再遍历 `/proc`。
fn main_xochitl_ticks(proc_root: &Path, pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    let (comm, ppid, ticks) = rmsvc_core::proc::parse_stat(&stat)?;
    (comm == "xochitl" && ppid == 1).then_some(ticks)
}

/// 任一进程的启动时刻（unix 秒）；读不到 stat 为 None。
pub fn process_start_secs(proc_root: &Path, pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    let (_, _, ticks) = rmsvc_core::proc::parse_stat(&stat)?;
    start_secs(proc_root, ticks)
}

/// 进程启动时刻（unix 秒）：starttime 滴答（arm64/x86 的 USER_HZ 都是 100）+ `/proc/stat` 的 `btime`。
fn start_secs(proc_root: &Path, ticks: u64) -> Option<u64> {
    let btime: u64 = std::fs::read_to_string(proc_root.join("stat")).ok()?.lines().find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())?;
    Some(btime + ticks / 100)
}

/// `qrr_dir`（qt-resource-rebuilder 的 exthome）里的 `*.qmd`：按"修改时间是否早于 `start`"分成已载入 / 待重启。
fn qmd_states(qrr_dir: &Path, start: u64) -> (Vec<String>, Vec<String>) {
    let (mut done, mut pending) = (Vec::new(), Vec::new());
    for e in std::fs::read_dir(qrr_dir).into_iter().flatten().flatten() {
        let Some(name) = e.file_name().to_str().map(str::to_string).filter(|n| n.ends_with(".qmd")) else { continue };
        let mtime = e.metadata().ok().and_then(|m| m.modified().ok()).map_or(0, rmsvc_core::clock::secs_of);
        if mtime <= start { done.push(name) } else { pending.push(name) }
    }
    done.sort();
    pending.sort();
    (done, pending)
}

/// 一个 xochitl 主进程的映射结果（按 pid + starttime 认同一个进程）。
struct Mapped {
    pid: u32,
    ticks: u64,
    start: Option<u64>,
    xovi: bool,
    extensions: Vec<String>,
    refused: Vec<String>,
}

/// 进程启动后多久才把映射结果缓存下来：xovi 在 xochitl 启动时逐个加载扩展，刚起的进程映射可能还不全。
const SETTLE_SECS: u64 = 30;

/// 扫描结果缓存。`/api/enhance/status`（管理页每次刷新、笔记页每次刷新都会调）原来每次都遍历整个 `/proc`、
/// 逐个读 comm，再把 xochitl 上百 KB 的 `maps` 整个读一遍；而扩展只在 xochitl 启动时加载，同一个进程的映射结果
/// 不会变。缓存命中时只读一次 `/proc/<pid>/stat` 核对还是同一个进程（pid + starttime），qmd 状态照旧现算
/// （看的是文件修改时间，随时会变）。
pub struct Scanner(std::sync::Mutex<Option<Mapped>>);

impl Scanner {
    pub const fn new() -> Scanner {
        Scanner(std::sync::Mutex::new(None))
    }

    pub fn scan(&self, proc_root: &Path, qrr_dir: &Path) -> Loaded {
        self.scan_with(proc_root, qrr_dir, |pid| crate::device::health::xochitl_journal(proc_root, pid))
    }

    /// `xochitl_log(pid)` 取 xochitl 启动时的 journal，只在要写缓存（进程已起满 [`SETTLE_SECS`]）时调一次。
    fn scan_with(&self, proc_root: &Path, qrr_dir: &Path, xochitl_log: impl FnOnce(u32) -> Option<String>) -> Loaded {
        let mut cache = rmsvc_core::sync::lock(&self.0);
        let hit = cache.as_ref().is_some_and(|m| main_xochitl_ticks(proc_root, m.pid) == Some(m.ticks));
        if !hit {
            *cache = None;
            // 全量找：遍历 `/proc` 的数字目录，每个只读一次 `stat`（comm 就在里面）。
            let Some((pid, ticks)) = rmsvc_core::proc::find_process(proc_root, "xochitl", Some(1)) else { return Loaded::default() };
            // maps 解析跟「设备健康」同一个函数（含路径带空格、行尾 ` (deleted)`）：此前这里另写一份按行尾判 xovi，
            // xovi.so 在 xochitl 运行中被替换后行尾多出 ` (deleted)` 就判成"没加载"，OTA 横幅随之误报。
            let mut maps = crate::device::health::parse_maps(&std::fs::read_to_string(proc_root.join(pid.to_string()).join("maps")).unwrap_or_default());
            let start = start_secs(proc_root, ticks);
            let settled = start.is_some_and(|s| rmsvc_core::clock::now_secs().saturating_sub(s) >= SETTLE_SECS);
            // 刚起的进程不读 journal（结果也不缓存，下次再扫）；起满了读一次、跟映射结果一起缓存。
            if settled && !maps.extensions.is_empty() {
                if let Some(log) = xochitl_log(pid) {
                    crate::device::health::apply_refused(&mut maps, &log);
                }
            }
            let m = Mapped { start, xovi: maps.xovi, pid, ticks, extensions: maps.extensions, refused: maps.refused };
            let loaded = m.loaded(qrr_dir);
            if settled {
                *cache = Some(m);
            }
            return loaded;
        }
        cache.as_ref().map(|m| m.loaded(qrr_dir)).unwrap_or_default()
    }
}

impl Mapped {
    fn loaded(&self, qrr_dir: &Path) -> Loaded {
        let (qmds, qmds_pending) = match (self.extensions.iter().any(|e| e == "qt-resource-rebuilder.so"), self.start) {
            (true, Some(start)) => qmd_states(qrr_dir, start),
            _ => (vec![], vec![]),
        };
        Loaded { xochitl: true, xovi: self.xovi, extensions: self.extensions.clone(), refused: self.refused.clone(), qmds, qmds_pending }
    }
}

/// 不带缓存的一次扫描（测试用）。
#[cfg(test)]
pub fn scan(proc_root: &Path, qrr_dir: &Path) -> Loaded {
    Scanner::new().scan_with(proc_root, qrr_dir, |_| None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_entry(root: &Path, pid: &str, comm: &str, ppid: &str, maps: &str) {
        let d = root.join(pid);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("comm"), format!("{comm}\n")).unwrap();
        // 第 22 列 starttime = 1000 滴答（10 秒）
        std::fs::write(d.join("stat"), format!("{pid} ({comm}) S {ppid} 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 1000 0 0")).unwrap();
        std::fs::write(d.join("maps"), maps).unwrap();
    }

    #[test]
    fn finds_main_process_and_its_mapped_extensions() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        std::fs::create_dir_all(r.join("self")).unwrap();
        proc_entry(r, "839", "xochitl", "613", "7f00-7f01 r-xp 0 00:00 1 /home/root/xovi/extensions.d/bogus.so\n");
        proc_entry(r, "613", "xochitl", "1", "7f00-7f01 r-xp 00000000 b3:04 11 /home/root/xovi/xovi.so\n\
7f02-7f03 r-xp 00000000 b3:04 12 /home/root/xovi/extensions.d/hw-stroke.so\n\
7f04-7f05 r--p 00001000 b3:04 12 /home/root/xovi/extensions.d/hw-stroke.so\n\
7f06-7f07 r-xp 00000000 b3:04 13 /home/root/xovi/extensions.d/hl-snap.so\n\
7f08-7f09 rw-p 00000000 00:00 0 \n");
        proc_entry(r, "700", "sh", "1", "");
        let l = scan(r, &r.join("none"));
        assert!(l.xochitl && l.xovi);
        assert_eq!(l.extensions, ["hl-snap.so", "hw-stroke.so"], "去重排序、不含 worker 子进程的映射");
        assert!(l.qmds.is_empty() && l.qmds_pending.is_empty(), "没有 qt-resource-rebuilder 就不报 qmd");
    }

    #[test]
    fn qmds_split_by_xochitl_start_time() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path().join("proc");
        let qrr = t.path().join("qrr");
        std::fs::create_dir_all(&qrr).unwrap();
        proc_entry(&r, "613", "xochitl", "1", "7f00-7f01 r-xp 0 00:00 1 /home/root/xovi/extensions.d/qt-resource-rebuilder.so\n");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        // 进程启动于 now-60（btime = now-70，starttime = 10 秒）
        std::fs::write(r.join("stat"), format!("cpu 0\nbtime {}\n", now - 70)).unwrap();
        for (n, age) in [("old.qmd", 3600u64), ("new.qmd", 0), ("x.qmd.bak", 3600)] {
            let f = qrr.join(n);
            std::fs::write(&f, "").unwrap();
            let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(now - age);
            std::fs::File::options().write(true).open(&f).unwrap().set_modified(when).unwrap();
        }
        let l = scan(&r, &qrr);
        assert_eq!(l.qmds, ["old.qmd"]);
        assert_eq!(l.qmds_pending, ["new.qmd"], "比进程新的 = 待重启；.bak 不算");
    }

    #[test]
    fn no_xochitl_or_no_xovi() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(scan(t.path(), t.path()), Loaded::default());
        proc_entry(t.path(), "5", "xochitl", "1", "7f00-7f01 r-xp 0 00:00 1 /usr/bin/xochitl\n");
        let l = scan(t.path(), t.path());
        assert!(l.xochitl && !l.xovi && l.extensions.is_empty());
    }

    /// 映射了但自己拒绝加载的扩展进 `refused`、不算加载；只在起满 SETTLE_SECS 后读一次 journal，并跟映射一起缓存。
    #[test]
    fn refused_extension_is_not_loaded_and_journal_read_once() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        std::fs::write(r.join("stat"), format!("cpu 0\nbtime {}\n", rmsvc_core::clock::now_secs() - 3600)).unwrap();
        proc_entry(r, "613", "xochitl", "1", "7f00-7f01 r-xp 0 00:00 1 /home/root/xovi/extensions.d/hl-snap.so\n7f02-7f03 r-xp 0 00:00 2 /home/root/xovi/extensions.d/ui-font.so\n");
        let log = "[hl-snap] _xovi_shouldLoad: 荧光笔扩张函数特征码未唯一命中(未知固件) → 拒绝加载(裸启原生)\n[ui-font] 安装完成\n";
        let sc = Scanner::new();
        let calls = std::cell::Cell::new(0);
        let l = sc.scan_with(r, r, |pid| {
            calls.set(calls.get() + 1);
            assert_eq!(pid, 613);
            Some(log.to_string())
        });
        assert_eq!((l.extensions.clone(), l.refused.clone()), (vec!["ui-font.so".to_string()], vec!["hl-snap.so".to_string()]));
        let l2 = sc.scan_with(r, r, |_| panic!("同一进程命中缓存，不该再读 journal"));
        assert_eq!(l2.refused, ["hl-snap.so"]);
        assert_eq!(calls.get(), 1);
        let hl = crate::enhance::Artifact::Extension("hl-snap.so");
        assert_eq!(crate::enhance::load_state(&hl, &l2), Some(crate::enhance::LoadState::Off), "拒绝加载的扩展徽章不是 On");
    }

    /// 缓存：同一个进程（pid + starttime）第二次不再读 maps；进程换了（starttime 变）就重扫；刚起的进程不缓存。
    #[test]
    fn caches_mapping_per_process_and_rescans_on_restart() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        let now = rmsvc_core::clock::now_secs();
        std::fs::write(r.join("stat"), format!("cpu 0\nbtime {}\n", now - 3600)).unwrap(); // 进程已跑了约 1 小时
        proc_entry(r, "613", "xochitl", "1", "7f00-7f01 r-xp 0 00:00 1 /home/root/xovi/extensions.d/hl-snap.so\n");
        let sc = Scanner::new();
        assert_eq!(sc.scan_with(r, r, |_| None).extensions, ["hl-snap.so"]);
        std::fs::write(r.join("613/maps"), "").unwrap(); // 缓存命中时不会再读 maps
        assert_eq!(sc.scan_with(r, r, |_| None).extensions, ["hl-snap.so"], "同一进程命中缓存");
        // 同 pid 但 starttime 变了（xochitl 重启后恰好复用 pid）：重扫
        std::fs::write(r.join("613/stat"), "613 (xochitl) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 2000 0 0").unwrap();
        assert!(sc.scan_with(r, r, |_| None).extensions.is_empty(), "进程变了要重扫");
        // 进程退出：不再报 xochitl
        std::fs::remove_dir_all(r.join("613")).unwrap();
        assert!(!sc.scan_with(r, r, |_| None).xochitl);
        // 刚启动的进程（不到 SETTLE_SECS）不缓存：之后映射变了能看到
        std::fs::write(r.join("stat"), format!("cpu 0\nbtime {}\n", now - 12)).unwrap();
        proc_entry(r, "700", "xochitl", "1", "");
        assert!(sc.scan_with(r, r, |_| None).extensions.is_empty());
        std::fs::write(r.join("700/maps"), "7f00-7f01 r-xp 0 00:00 1 /home/root/xovi/extensions.d/hw-stroke.so\n").unwrap();
        assert_eq!(sc.scan_with(r, r, |_| None).extensions, ["hw-stroke.so"], "启动初期的结果不该被缓存住");
    }
}
