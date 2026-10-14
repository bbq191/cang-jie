//! 部署记录与漂移（2026-10-14）：「设备健康 → 概览」的"部署版本"一行与「日志」里的部署历史。
//!
//! 记录由电脑端部署脚本写（`packaging/devlib.sh` 的 `cj_deploy_stamp`，经 `deploy.sh` / `deploy-xovi-ext.sh` /
//! `install-all.sh`）：`$XDG_STATE_HOME/shelf/deploy-log.jsonl`，一次部署一行，只留最近 20 行。每行有提交号、时间、
//! 经 USB 还是 WiFi、装了什么，外加当时装好的文件（`~/.local/bin/*`、xovi 扩展 `.so`、qrr 的 `.qmd`）各自的 sha256。
//! 网关只读：取最后一行当"当前版本"，把它记的文件逐个现算 sha256 比对，对不上的列为漂移——之后有人绕开部署脚本
//! 换过文件、vellum 升级盖回了某个文件、或某次部署装到一半。起因：10-13 只能靠比对 md5 反推 10-10 重构是 10-12 部署的。
//!
//! 只在「设备健康」被打开/刷新时算（跟健康数据同一个 15 秒缓存）；每个文件按 (大小, mtime) 记住上次的哈希，
//! 没变就不重读——第一次约十几个文件、二三十 MB，之后几乎不花时间。
use rmsvc_core::paths::Paths;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

pub const LOG_FILE: &str = "deploy-log.jsonl";
/// 历史最多显示几条（与 devlib.sh 的 `CJ_DEPLOY_KEEP` 缺省值一致）。
const HISTORY: usize = 20;

/// 一次部署。字段由 `packaging/lib.sh` 的 `stamp_meta` 定；缺的字段取缺省值（旧行、手写的行照样能读）。
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Stamp {
    pub at: u64,
    /// 哪个脚本记的：`install-all` / `shelf`（deploy.sh）/ `hl-snap` / `ui-font`。
    pub label: String,
    /// deploy.sh 的 `--only`（空 = 全装）。
    pub only: String,
    /// install-all 里失败的步骤（空格分隔；空 = 没有）。
    pub failed: String,
    pub commit: String,
    /// 部署时工作区有未提交的改动：装上去的不完全是 `commit` 那一版。
    pub dirty: bool,
    /// `usb` / `wifi`（按部署目标地址判断）。
    pub via: String,
    /// 从哪台电脑部署的。
    pub host: String,
    /// 路径 → sha256。只用来核对漂移，不发给网页。
    #[serde(skip_serializing)]
    pub files: BTreeMap<String, String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DriftState {
    /// 文件还在，内容跟记录不一样。
    Changed,
    /// 记录里有、现在没有了。
    Missing,
    /// 扩展 `.so`：新版在待换入区里，等整机重启才换进去（不算异常，单独标出来）。
    Pending,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Drift {
    pub path: String,
    pub state: DriftState,
}

/// `GET /api/device/health` 里的 `deploy`。
#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Deploy {
    /// 最近一次部署；这台设备还没用带部署记录的脚本部署过时为 null。
    pub current: Option<Stamp>,
    /// 最近几次，新的在前（含 `current`）。
    pub history: Vec<Stamp>,
    /// `current` 记了几个文件。
    pub files: usize,
    pub drift: Vec<Drift>,
}

/// 读整份记录（旧的在前）。读不到为空；坏行（写到一半、手改坏了）跳过。
pub fn read_log(file: &Path) -> Vec<Stamp> {
    let Ok(s) = std::fs::read_to_string(file) else { return Vec::new() };
    s.lines().filter_map(|l| serde_json::from_str::<Stamp>(l).ok()).collect()
}

/// 比对 `st` 记的文件与现状。`home` 之外的路径不碰（记录文件只有 root 能写，这里再多一道：网关不去读任意路径）。
/// `hash` 读不到返回 None（文件不在、没权限）。
pub fn drift(st: &Stamp, home: &Path, pending_dir: &Path, hash: &dyn Fn(&Path) -> Option<String>) -> Vec<Drift> {
    let mut out = Vec::new();
    for (p, want) in &st.files {
        let path = Path::new(p);
        if !path.starts_with(home) {
            continue;
        }
        let have = hash(path);
        if have.as_deref() == Some(want.as_str()) {
            continue;
        }
        let pending = p.contains("/extensions.d/") && path.file_name().is_some_and(|n| hash(&pending_dir.join(n)).as_deref() == Some(want.as_str()));
        let state = if pending {
            DriftState::Pending
        } else if have.is_none() {
            DriftState::Missing
        } else {
            DriftState::Changed
        };
        out.push(Drift { path: p.clone(), state });
    }
    out
}

/// 带 (大小, mtime) 缓存的 sha256：文件没变就不重读。
fn cached_sha256(p: &Path) -> Option<String> {
    type Cache = Mutex<HashMap<PathBuf, (u64, SystemTime, String)>>;
    static C: OnceLock<Cache> = OnceLock::new();
    let md = std::fs::metadata(p).ok().filter(|m| m.is_file())?;
    let key = (md.len(), md.modified().ok()?);
    let c = C.get_or_init(Default::default);
    if let Some((l, m, h)) = c.lock().unwrap_or_else(|e| e.into_inner()).get(p) {
        if (*l, *m) == key {
            return Some(h.clone());
        }
    }
    let h = super::ota::sha256_file(p).ok()?;
    c.lock().unwrap_or_else(|e| e.into_inner()).insert(p.to_path_buf(), (key.0, key.1, h.clone()));
    Some(h)
}

pub fn collect(paths: &Paths) -> Deploy {
    let mut log = read_log(&paths.state_dir().join(LOG_FILE));
    let Some(cur) = log.last().cloned() else { return Deploy::default() };
    let drift = drift(&cur, paths.home(), &super::health::so_pending_dir(paths), &cached_sha256);
    log.reverse();
    log.truncate(HISTORY);
    Deploy { files: cur.files.len(), current: Some(cur), history: log, drift }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(s: &str) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn reads_log_skipping_bad_lines_and_reports_drift() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let home = paths.home().to_path_buf();
        assert_eq!(collect(&paths), Deploy::default(), "没有记录：current 为 null");
        let bin = home.join(".local/bin");
        let ext = home.join("xovi/extensions.d");
        let pend = super::super::health::so_pending_dir(&paths);
        for d in [&bin, &ext, &pend] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(bin.join("gateway"), "g").unwrap();
        std::fs::write(bin.join("book-serve"), "b2").unwrap(); // 部署后被换过
        std::fs::write(ext.join("ui-font.so"), "old").unwrap();
        std::fs::write(pend.join("ui-font.so"), "new").unwrap(); // 新版待换入
        let f = |n: &Path| n.display().to_string();
        let line = serde_json::json!({
            "at": 2, "label": "install-all", "commit": "abc1234", "dirty": true, "via": "usb", "host": "pc", "extra": 1,
            "files": {
                f(&bin.join("gateway")): sha("g"), f(&bin.join("book-serve")): sha("b1"),
                f(&bin.join("gone")): sha("x"), f(&ext.join("ui-font.so")): sha("new"),
                "/etc/passwd": sha("nope"),
            }
        });
        let log = format!("{}\n{{坏行\n{}\n", serde_json::json!({"at": 1, "label": "shelf", "commit": "0000000"}), line);
        std::fs::create_dir_all(paths.state_dir()).unwrap();
        std::fs::write(paths.state_dir().join(LOG_FILE), log).unwrap();
        let d = collect(&paths);
        let cur = d.current.as_ref().unwrap();
        assert_eq!((cur.commit.as_str(), cur.dirty, cur.via.as_str(), d.files), ("abc1234", true, "usb", 5));
        assert_eq!(d.history.iter().map(|s| s.at).collect::<Vec<_>>(), [2, 1], "新的在前，坏行跳过");
        let mut got: Vec<(String, DriftState)> = d.drift.iter().map(|x| (x.path.rsplit('/').next().unwrap().to_string(), x.state)).collect();
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(got, [("book-serve".into(), DriftState::Changed), ("gone".into(), DriftState::Missing), ("ui-font.so".into(), DriftState::Pending)], "home 之外的路径不碰");
        // 文件表不发给网页
        let v = serde_json::to_value(&d).unwrap();
        assert!(v["current"].get("files").is_none() && v["history"][0].get("files").is_none());
        assert_eq!(v["drift"][0]["state"], "changed");
    }

    #[test]
    fn cached_hash_follows_content_changes() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("f");
        std::fs::write(&p, "a").unwrap();
        assert_eq!(cached_sha256(&p), Some(sha("a")));
        std::fs::write(&p, "bb").unwrap(); // 大小变了：重算
        assert_eq!(cached_sha256(&p), Some(sha("bb")));
        assert_eq!(cached_sha256(&t.path().join("none")), None);
        assert_eq!(cached_sha256(t.path()), None, "目录不算");
    }
}
