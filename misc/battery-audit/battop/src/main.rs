// battop 采集器(oneshot,由 systemd timer 每 ~10 分钟触发一次)。
//
// 每次运行:载入上次 baseline → 采样 /proc 各进程 CPU 累计 + 电量 →
// 算「距上次的增量」→ 按 comm(进程)与 systemd unit(应用)双聚合 →
// 追加到当日样本文件 → 写新 baseline → 清理 40 天前旧样本文件。
//
// 非实时、不常驻、不持 wakelock、不 WakeSystem —— 本身几乎不耗电。
// 数据目录:$BATTOP_DIR,缺省 /home/root/battop/data。
// 设计见 ../APP-DESIGN.md。纯 std 零依赖。

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CLK_TCK: u64 = 100; // 真机 getconf CLK_TCK = 100(aarch64 Linux 常量)
const RETAIN_DAYS: u64 = 40;
const BAT: &str = "/sys/class/power_supply/max77818_battery";

fn main() {
    let dir = std::env::var("BATTOP_DIR").unwrap_or_else(|_| "/home/root/battop/data".into());
    let dir = PathBuf::from(dir);
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("battop: 建目录失败 {}: {e}", dir.display());
        std::process::exit(1);
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let (cap, status) = read_battery();
    let disc = if status == "Discharging" { 1 } else { 0 };
    let awake = read_uptime();

    // 采样当前所有进程
    let procs = sample_procs();

    // 载入 baseline(上次每 (pid,starttime) 的累计 ticks + 是否有历史)
    let (have_base, base) = load_baseline(&dir);

    // 算增量并双聚合
    let mut by_comm: HashMap<String, u64> = HashMap::new();
    let mut by_unit: HashMap<String, u64> = HashMap::new();
    if have_base {
        for p in &procs {
            let prev = base.get(&(p.pid, p.starttime)).copied();
            // 新进程(不在 baseline)= 距上次全部是它攒的 → 增量即当前累计
            let delta = match prev {
                Some(pv) => p.ticks.saturating_sub(pv),
                None => p.ticks,
            };
            if delta == 0 {
                continue;
            }
            *by_comm.entry(p.comm.clone()).or_insert(0) += delta;
            *by_unit.entry(p.unit.clone()).or_insert(0) += delta;
        }
    }

    // 追加样本行:epoch \t kind \t key \t cpu_ms \t disc
    let file = dir.join(format!("samples-{}.tsv", ymd(now)));
    let mut out = String::new();
    let ms = |ticks: u64| ticks * 1000 / CLK_TCK;
    if have_base {
        for (k, t) in &by_comm {
            out.push_str(&format!("{now}\tproc\t{}\t{}\t{disc}\n", san(k), ms(*t)));
        }
        for (k, t) in &by_unit {
            out.push_str(&format!("{now}\tapp\t{}\t{}\t{disc}\n", san(k), ms(*t)));
        }
    }
    // _sys 行(每次都写,供窗口放电量/在线时长计算):epoch _sys status awake_s cap disc
    out.push_str(&format!("{now}\t_sys\t{}\t{}\t{}\t{disc}\n", san(&status), awake, cap));
    if let Err(e) = append(&file, &out) {
        eprintln!("battop: 写样本失败: {e}");
    }

    // 写新 baseline
    if let Err(e) = write_baseline(&dir, now, cap, &procs) {
        eprintln!("battop: 写 baseline 失败: {e}");
    }

    // 清理 40 天前旧样本
    prune(&dir, now);

    // 简报(observe:被 systemctl 收进 journal)
    eprintln!(
        "battop: t={now} cap={cap} {status} procs={} 进程键={} 应用键={} {}",
        procs.len(),
        by_comm.len(),
        by_unit.len(),
        if have_base { "" } else { "(首次,仅建 baseline)" }
    );
}

struct Proc {
    pid: u64,
    starttime: u64,
    ticks: u64,
    comm: String,
    unit: String,
}

fn sample_procs() -> Vec<Proc> {
    let mut v = Vec::new();
    let entries = match fs::read_dir("/proc") {
        Ok(e) => e,
        Err(_) => return v,
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        let pid: u64 = match name.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // comm 在首个 '(' 与末个 ')' 之间(可含空格/括号)
        let lp = match stat.find('(') {
            Some(i) => i,
            None => continue,
        };
        let rp = match stat.rfind(')') {
            Some(i) => i,
            None => continue,
        };
        if rp < lp {
            continue;
        }
        let comm = stat[lp + 1..rp].to_string();
        // ')' 之后是 field3(state) 起的空格分隔字段
        let rest: Vec<&str> = stat[rp + 1..].split_whitespace().collect();
        // field14 utime=idx11, field15 stime=idx12, field22 starttime=idx19
        if rest.len() < 20 {
            continue;
        }
        let utime: u64 = rest[11].parse().unwrap_or(0);
        let stime: u64 = rest[12].parse().unwrap_or(0);
        let starttime: u64 = rest[19].parse().unwrap_or(0);
        let unit = read_unit(pid);
        v.push(Proc { pid, starttime, ticks: utime + stime, comm, unit });
    }
    v
}

// 从 cgroup v2 取归属 unit;找不到 .service/.scope 则归 kernel/other
fn read_unit(pid: u64) -> String {
    let cg = match fs::read_to_string(format!("/proc/{pid}/cgroup")) {
        Ok(s) => s,
        Err(_) => return "kernel".into(),
    };
    // 形如 "0::/system.slice/xochitl.service"
    let path = cg.trim().rsplit("::").next().unwrap_or("").trim();
    if path.is_empty() || path == "/" {
        return "kernel".into();
    }
    for seg in path.split('/').rev() {
        if seg.ends_with(".service") || seg.ends_with(".scope") {
            // 去掉 @实例后缀与 .service,取干净名
            let base = seg.trim_end_matches(".service").trim_end_matches(".scope");
            let base = base.split('@').next().unwrap_or(base);
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    // 无 service:取叶子(如 user.slice)或 other
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("other").to_string()
}

fn read_battery() -> (i64, String) {
    let cap = fs::read_to_string(format!("{BAT}/capacity"))
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(-1);
    let status = fs::read_to_string(format!("{BAT}/status"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Unknown".into());
    (cap, status)
}

fn read_uptime() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split('.').next().and_then(|x| x.trim().parse().ok()))
        .unwrap_or(0)
}

fn load_baseline(dir: &Path) -> (bool, HashMap<(u64, u64), u64>) {
    let mut m = HashMap::new();
    let content = match fs::read_to_string(dir.join("baseline.tsv")) {
        Ok(c) => c,
        Err(_) => return (false, m),
    };
    for line in content.lines() {
        if line.starts_with('_') {
            continue; // meta
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 {
            continue;
        }
        if let (Ok(pid), Ok(st), Ok(t)) = (f[0].parse(), f[1].parse(), f[2].parse()) {
            m.insert((pid, st), t);
        }
    }
    (true, m)
}

fn write_baseline(dir: &Path, now: u64, cap: i64, procs: &[Proc]) -> std::io::Result<()> {
    let tmp = dir.join("baseline.tsv.tmp");
    let mut s = format!("_ts\t{now}\t_cap\t{cap}\n");
    for p in procs {
        // pid \t starttime \t ticks \t comm \t unit
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            p.pid, p.starttime, p.ticks, san(&p.comm), san(&p.unit)
        ));
    }
    fs::write(&tmp, s)?;
    fs::rename(&tmp, dir.join("baseline.tsv"))
}

fn append(path: &Path, data: &str) -> std::io::Result<()> {
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(data.as_bytes())
}

fn prune(dir: &Path, now: u64) {
    let cutoff = now.saturating_sub(RETAIN_DAYS * 86400);
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if !n.starts_with("samples-") || !n.ends_with(".tsv") {
            continue;
        }
        if let Ok(md) = e.metadata() {
            if let Ok(m) = md.modified() {
                if let Ok(d) = m.duration_since(UNIX_EPOCH) {
                    if d.as_secs() < cutoff {
                        let _ = fs::remove_file(e.path());
                    }
                }
            }
        }
    }
}

// TSV 安全:去掉 tab/换行
fn san(s: &str) -> String {
    s.chars().map(|c| if c == '\t' || c == '\n' { ' ' } else { c }).collect()
}

// epoch(秒,UTC)→ YYYYMMDD(仅用于文件按天滚动;窗口计算靠行内 epoch)
fn ymd(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    format!("{:04}{:02}{:02}", y, m, d)
}
