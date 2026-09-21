//! summary.json:面板用预聚合。
//! 读全部样本 → 4 窗口(当日/7天/30天/全部)× 应用(友好名)/进程(comm)双分组 + 唤醒源计数 + 每窗口放电
//! → 写 summary.json。窗口计算靠行内 epoch。
//!
//! 流式聚合：逐行喂 [`Summarizer`]，直接累加进 4 个窗口的表，内存只与「不同 key 数」成正比。旧实现先把
//! 40 天全部行读进 `Vec<(epoch,bool,String,u64)>`（几十万条各带一个 String）再对 4 个窗口各扫一遍，
//! 每 ~10 分钟白白分配/拷贝几十 MB。输出格式与旧实现逐字节一致（tie 时按名字升序，旧实现 tie 顺序取决于
//! HashMap 随机迭代序）。
use crate::store::is_sample_file;
use crate::util::json_esc;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

const WINDOWS: [&str; 4] = ["today", "7d", "30d", "all"];

pub struct Summarizer {
    now: u64,
    starts: [u64; 4],
    app: [HashMap<String, u64>; 4],
    proc: [HashMap<String, u64>; 4],
    /// (epoch, cap%, charge_uah)
    sys: Vec<(u64, i64, i64)>,
}

fn add(m: &mut HashMap<String, u64>, key: &str, ms: u64) {
    match m.get_mut(key) {
        Some(v) => *v += ms,
        None => {
            m.insert(key.to_string(), ms);
        }
    }
}

impl Summarizer {
    /// `local_off`：本地时区相对 UTC 的偏移秒（决定「今日 0 点」）。
    pub fn new(now: u64, local_off: i64) -> Summarizer {
        let local = now as i64 + local_off;
        let midnight = (local - local.rem_euclid(86400) - local_off) as u64; // 本地今日 0 点(UTC epoch)
        Summarizer {
            now,
            starts: [midnight, now.saturating_sub(7 * 86400), now.saturating_sub(30 * 86400), 0],
            app: Default::default(),
            proc: Default::default(),
            sys: Vec::new(),
        }
    }

    /// 喂一行样本（畸形行静默忽略）。
    pub fn feed(&mut self, line: &str) {
        let mut f = [""; 7];
        let mut n = 0;
        for part in line.split('\t').take(7) {
            f[n] = part;
            n += 1;
        }
        if n < 3 {
            return;
        }
        let Ok(epoch) = f[0].parse::<u64>() else { return };
        match f[1] {
            kind @ ("proc" | "app") if n >= 4 => {
                let ms: u64 = f[3].parse().unwrap_or(0);
                let is_app = kind == "app";
                let key = if is_app { friendly(f[2]) } else { f[2] };
                for (i, start) in self.starts.iter().enumerate() {
                    if epoch >= *start {
                        add(if is_app { &mut self.app[i] } else { &mut self.proc[i] }, key, ms);
                    }
                }
            }
            "_sys" if n >= 5 => {
                let cap: i64 = f[4].parse().unwrap_or(-1);
                let charge: i64 = if n >= 7 { f[6].parse().unwrap_or(-1) } else { -1 };
                self.sys.push((epoch, cap, charge));
            }
            _ => {}
        }
    }

    /// 汇总成 summary.json 文本。`wakes`：(epoch, 原始唤醒源名)。
    pub fn finish(mut self, wakes: &[(u64, String)]) -> String {
        self.sys.sort_by_key(|t| t.0); // 稳定排序：同 epoch 保持输入序（同旧实现）
        let mut json = format!("{{\"generated\":{},\"windows\":{{", self.now);
        for (wi, (wname, start)) in WINDOWS.iter().zip(self.starts.iter()).enumerate() {
            // 放电:窗口内 _sys 相邻样本,容量% 下降之和(disc)+ 库仑计 µAh 下降之和(精确 mAh,v3)
            let s: Vec<&(u64, i64, i64)> = self.sys.iter().filter(|(e, _, _)| e >= start).collect();
            let mut disc = 0i64;
            let mut drained_uah = 0i64;
            let mut disc_secs = 0u64;
            for w in s.windows(2) {
                let (ca, cb) = (w[0].1, w[1].1);
                if ca >= 0 && cb >= 0 && ca > cb {
                    disc += ca - cb;
                }
                let (qa, qb) = (w[0].2, w[1].2);
                if qa >= 0 && qb >= 0 && qa > qb {
                    drained_uah += qa - qb;
                    disc_secs += w[1].0.saturating_sub(w[0].0);
                }
            }
            let mah = drained_uah / 1000; // 精确放电 mAh
            let ma = if disc_secs > 0 { drained_uah * 3600 / disc_secs as i64 / 1000 } else { 0 }; // 均放电 mA

            // 唤醒源计数(窗口内 journal 事件按友好名聚合)
            let mut wake: HashMap<String, u64> = HashMap::new();
            for (ep, name) in wakes {
                if ep >= start {
                    add(&mut wake, friendly_wake(name), 1);
                }
            }

            if wi > 0 {
                json.push(',');
            }
            json.push_str(&format!("\"{wname}\":{{\"discharge\":{disc},\"mah\":{mah},\"ma\":{ma},\"samples\":{},", s.len()));
            json.push_str(&format!("\"app\":{},", top_json(&self.app[wi])));
            json.push_str(&format!("\"proc\":{},", top_json(&self.proc[wi])));
            json.push_str(&format!("\"wake\":{}}}", top_json(&wake)));
        }
        json.push_str("}}\n");
        json
    }
}

/// 读目录下全部样本 → 聚合 → 原子写 summary.json。
pub fn write_summary(dir: &Path, now: u64, local_off: i64, wakes: &[(u64, String)]) -> std::io::Result<()> {
    let mut sm = Summarizer::new(now, local_off);
    let mut line = String::new();
    for e in fs::read_dir(dir)?.flatten() {
        if !is_sample_file(&e.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(f) = fs::File::open(e.path()) else { continue };
        let mut r = BufReader::new(f);
        loop {
            line.clear();
            match r.read_line(&mut line) {
                Ok(0) | Err(_) => break, // EOF / 非 UTF-8 坏文件：跳过其余（旧实现整文件丢弃）
                Ok(_) => sm.feed(line.trim_end_matches(['\n', '\r'])),
            }
        }
    }
    let json = sm.finish(wakes);
    let tmp = dir.join("summary.json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, dir.join("summary.json"))
}

/// 取前 15 名(按值降序，同值按名升序保证输出稳定)→ JSON 数组;pct=占窗口总量百分比。
fn top_json(m: &HashMap<String, u64>) -> String {
    let total: u64 = m.values().sum();
    let mut v: Vec<(&String, &u64)> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    v.truncate(15);
    let mut s = String::from("[");
    for (i, (k, ms)) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let pct = (**ms * 100).checked_div(total).unwrap_or(0);
        s.push_str(&format!("{{\"name\":\"{}\",\"ms\":{},\"pct\":{}}}", json_esc(k), ms, pct));
    }
    s.push(']');
    s
}

/// systemd unit / comm → 友好应用名(应用视图用;设计 §5)。
fn friendly(unit: &str) -> &str {
    match unit {
        "xochitl" => "reMarkable 核心",
        "memfaultd" | "crashuploader" => "Memfault 遥测",
        "remarkable-counter-metrics" | "slumber-metrics" | "nm-metrics" | "battery-status-metrics" => "reMarkable 度量",
        "mdm-agent" => "reMarkable MDM",
        "rm-sync" | "update-engine" | "swupdate" => "reMarkable 同步/更新",
        "NetworkManager" | "wpa_supplicant" | "systemd-networkd" | "systemd-resolved" => "网络栈",
        "marker-manager" | "tee-supplicant" => "硬件",
        "wr-serve" | "cj-stars" | "wr-renew" | "battop" | "cangjie-wallpaper" => "cang-jie",
        "kernel" => "内核",
        _ if unit.ends_with("-metrics") => "reMarkable 度量",
        _ => unit,
    }
}

/// 唤醒源名 → 友好名。
fn friendly_wake(name: &str) -> &str {
    match name {
        "mwlan" => "WiFi",
        "xochitl.batterymanager" => "电池管理",
        "sleep.resume" => "唤醒锁",
        "udev.charger" => "充电器",
        "gpio-hall-sensors" => "合盖磁吸",
        "rtc0" | "rtc" => "定时器(RTC)",
        _ if name.starts_with("spi") => "触控笔(SPI)",
        _ if name.ends_with("pwrkey") => "电源键",
        // I2C 设备名形如 "0-0048" / "1-0021"
        _ if name.len() >= 3 && name.as_bytes()[0].is_ascii_digit() && name[1..].starts_with('-') => "传感器(I2C)",
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ymd;

    /// 确定性造 45 天样本（含畸形行、无 charge 的旧格式 _sys 行）。与旧实现在同一数据集上产出的
    /// summary.json 逐字节对拍过（GOLDEN 即旧实现输出）。
    fn gen_days(now: u64) -> HashMap<String, String> {
        let mut day: HashMap<String, String> = HashMap::new();
        let mut ep = now - 45 * 86400;
        let mut i: u64 = 0;
        while ep <= now {
            let mut s = String::new();
            let disc = (i % 5).min(1);
            let keys = ["xochitl", "wr-serve", "memfaultd", "battop", "kernel", "NetworkManager", "foo-metrics"];
            for (k, key) in keys.iter().enumerate() {
                let ms = 100 + (i * 37 + k as u64 * 1013) % 5000 * 10 + k as u64;
                s.push_str(&format!("{ep}\tproc\t{key}\t{ms}\t{disc}\n"));
                s.push_str(&format!("{ep}\tapp\t{key}\t{}\t{disc}\n", ms + 3));
            }
            let cap = 100 - ((i / 3) % 100) as i64;
            let charge = 3_000_000 - (i as i64 % 3000) * 700;
            if i % 7 < 1 {
                s.push_str(&format!("{ep}\t_sys\tDischarging\t{}\t{cap}\t{disc}\n", i * 600));
            } else {
                s.push_str(&format!("{ep}\t_sys\tDischarging\t{}\t{cap}\t{disc}\t{charge}\t-120000\n", i * 600));
            }
            if i % 11 < 1 {
                s.push_str("garbage line\nxx\tproc\n");
            }
            day.entry(ymd(ep)).or_default().push_str(&s);
            ep += 3600 * 3 + 600;
            i += 1;
        }
        day
    }

    fn gen_wakes(now: u64) -> Vec<(u64, String)> {
        let names = ["mwlan", "rtc0", "spi1.0", "0-0048", "foo", "gpio-hall-sensors", "pwrkey"];
        let mut wakes = Vec::new();
        for (k, n) in names.iter().enumerate() {
            for m in 0..=(k as u64) {
                wakes.push((now - 100 - m, n.to_string()));
            }
            wakes.push((now - 8 * 86400, n.to_string()));
            wakes.push((now - 40 * 86400, n.to_string()));
        }
        wakes
    }

    const GOLDEN: &str = include_str!("../tests/golden-summary.json");

    #[test]
    fn matches_legacy_output_byte_for_byte() {
        let now = 1_760_000_000;
        let t = crate::util::testutil::tmp();
        for (k, v) in gen_days(now) {
            fs::write(t.path().join(format!("samples-{k}.tsv")), v).unwrap();
        }
        fs::write(t.path().join("baseline.tsv"), "not a sample").unwrap();
        write_summary(t.path(), now, 8 * 3600, &gen_wakes(now)).unwrap();
        assert_eq!(fs::read_to_string(t.path().join("summary.json")).unwrap(), GOLDEN);
        assert!(!t.path().join("summary.json.tmp").exists());
    }

    #[test]
    fn today_window_uses_local_midnight() {
        // 本地 +08:00：UTC 2025-10-09 17:00 = 本地 10-10 01:00 → 今日 0 点 = UTC 10-09 16:00
        let now = 1_760_029_200 + 1000; // 2025-10-09 17:16:40 UTC
        let mut sm = Summarizer::new(now, 8 * 3600);
        let midnight = now - (now + 8 * 3600) % 86400;
        sm.feed(&format!("{}\tproc\tin\t10\t1", midnight));
        sm.feed(&format!("{}\tproc\tout\t99\t1", midnight - 1));
        let j = sm.finish(&[]);
        let today = &j[j.find("\"today\"").unwrap()..j.find("\"7d\"").unwrap()];
        assert!(today.contains("\"in\"") && !today.contains("\"out\""), "{today}");
    }

    #[test]
    fn empty_input_is_valid_shape() {
        let j = Summarizer::new(1000, 0).finish(&[]);
        assert!(j.starts_with("{\"generated\":1000,\"windows\":{\"today\":{\"discharge\":0,\"mah\":0,\"ma\":0,\"samples\":0,\"app\":[],\"proc\":[],\"wake\":[]}"));
        assert!(j.ends_with("}}\n"));
    }

    #[test]
    fn top15_ties_are_name_ordered_and_pct_safe() {
        let mut m = HashMap::new();
        for i in 0..20 {
            m.insert(format!("k{i:02}"), 5u64);
        }
        let j = top_json(&m);
        assert!(j.starts_with("[{\"name\":\"k00\""));
        assert_eq!(j.matches("\"name\"").count(), 15);
        assert_eq!(top_json(&HashMap::new()), "[]");
        let mut z = HashMap::new();
        z.insert("a".to_string(), 0u64);
        assert!(top_json(&z).contains("\"pct\":0"), "总量 0 不除零");
    }

    #[test]
    fn friendly_names() {
        assert_eq!(friendly("foo-metrics"), "reMarkable 度量");
        assert_eq!(friendly("weird"), "weird");
        assert_eq!(friendly_wake("0-0048"), "传感器(I2C)");
        assert_eq!(friendly_wake("1-x"), "传感器(I2C)");
        assert_eq!(friendly_wake("ab"), "ab");
        assert_eq!(friendly_wake("é-1"), "é-1", "非 ASCII 首字节不切片也不 panic");
    }
}
