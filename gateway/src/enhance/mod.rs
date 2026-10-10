//! 系统增强工具开关（Track 3，2026-09-09）：网关自身固定能力（跟 `manage` 一样不经过服务注册表/反代），
//! 给原来只能在设备原生「设置」App 里改的开关一个网页入口。现接的开关：CJK 荧光笔吸附/
//! 「导入 md 文档」可见性/单击翻页（都在 [`qol`]，同一份 `reading-qol.json`）。
//!
//! 2026-09-30 移除：电池刺客（battop，原 `battop.rs` + `/api/enhance/battop/*`）与手写优化（hw-stroke 扩展的
//! `hwStrokeEnabled` 派生开关）。`reading-qol.json` 里残留的 `hwStroke*` 键不主动清——全量写回、不认识的键原样保留。
//!
//! 开关登记在 [`TOGGLES`] 一张表里（2026-10-10，GW-2）：键名、缺省值、xochitl 里承载它的产物（`.so` 扩展 / qmd 补丁 /
//! 纯网页功能）。读、写、`/api/enhance/status` 的 `toggles`（含"产物实际加载了没有"的判定）都从这张表派生，网页不再
//! 认识任何 `.so`/`.qmd` 文件名。加一个开关 = 表里加一行 + 网页 `manage.js` 的 `TOGGLE_UI` 加一行文案。
//!
//! 以后再加系统增强能力，往这个目录加一个新文件（比照 `qol.rs`）+ 这里挂一个路由，
//! 不需要单独起一个 service（同机文件 I/O，没有独立进程边界的理由）。
mod loaded;
mod qol;

use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;
use serde::Serialize;

/// xochitl 扩展加载状态的扫描器（进程级缓存，见 [`loaded::Scanner`]）。
static LOADED: loaded::Scanner = loaded::Scanner::new();

/// xochitl 主进程映射扫描（同一个进程只扫一次 maps，见 [`loaded::Scanner`]）：「设备健康」的 OTA 判定与清理页复用；
/// maps 的解析与「设备健康」共用 `device::health::parse_maps`。
pub fn xochitl_loaded(paths: &Paths) -> loaded::Loaded {
    LOADED.scan(std::path::Path::new("/proc"), &crate::manage::qrr_dir(paths))
}

/// 一个系统增强开关（存在 `reading-qol.json` 里的布尔键）。
pub struct Toggle {
    /// `reading-qol.json` 里的键名，也是 `PUT /api/enhance/qol` body 里的字段名。
    pub key: &'static str,
    /// 键缺失（或不是布尔）时的取值——必须与读这个键的那一方（C 扩展 / qmd）的缺省一致。
    pub default: bool,
    pub artifact: Artifact,
}

/// 开关在 xochitl 里靠什么生效，决定"实际加载了没有"怎么判（[`load_state`]）。
pub enum Artifact {
    /// `extensions.d/` 下的 xovi 扩展（`.so`），看 xochitl 主进程映射。
    Extension(&'static str),
    /// qt-resource-rebuilder 在 xochitl 启动时读一次的 qmd 补丁，看补丁文件是否早于进程启动。
    Patch(&'static str),
    /// 只影响网页本身，不往 xochitl 里加载东西。
    Web,
}

pub const TOGGLES: &[Toggle] = &[
    // CJK 荧光笔精确吸附（enhance/hl-snap 每次划线时读）。缺省开：跟 hl-snap 的规则一致，键缺失时保持修复开启。
    Toggle { key: "hlSnapCjk", default: true, artifact: Artifact::Extension("hl-snap.so") },
    // 单击翻页（2026-09-24）：阅读器里的 reader-page-turn.qmd 每次打开书时读它（不轮询），切换后下次打开书生效。
    // 缺省关 = xochitl 原生行为。「日漫翻页规则」rtlPageTurn 2026-10-07 删除，旧文件里的键原样保留、不再有人读。
    Toggle { key: "tapPageTurn", default: false, artifact: Artifact::Patch("reader-page-turn.qmd") },
    // 「导入 md 文档」子标签可见性：缺省关——新功能第一次上线，不想让用户点开笔记 tab 就撞见一个半成品。
    Toggle { key: "notesImportMdEnabled", default: false, artifact: Artifact::Web },
];

#[cfg(test)]
pub fn toggle(key: &str) -> Option<&'static Toggle> {
    TOGGLES.iter().find(|t| t.key == key)
}

/// 产物的加载状态。语义逐字移植自 2026-10-10 前网页 `manage.js` 里的徽章判定：
/// 找不到 xochitl 主进程 → `unknown`；扩展：映射了 → `on`，否则 `off`；补丁：已载入 → `on`，文件比进程新（改过还没重启）
/// → `pending`，否则 `off`。纯网页功能没有产物，不判（`null`）。
#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum LoadState {
    On,
    Off,
    Pending,
    Unknown,
}

pub fn load_state(a: &Artifact, l: &loaded::Loaded) -> Option<LoadState> {
    let has = |list: &[String], f: &str| list.iter().any(|x| x == f);
    Some(match a {
        Artifact::Web => return None,
        _ if !l.xochitl => LoadState::Unknown,
        Artifact::Extension(so) if has(&l.extensions, so) => LoadState::On,
        Artifact::Patch(qmd) if has(&l.qmds, qmd) => LoadState::On,
        Artifact::Patch(qmd) if has(&l.qmds_pending, qmd) => LoadState::Pending,
        _ => LoadState::Off,
    })
}

/// `toggles` 里的一项：网页据 `kind` 选徽章说明文字（扩展 / 补丁 / 网页功能），据 `loaded` 选徽章。
#[derive(Serialize, Debug, PartialEq)]
pub struct ToggleState {
    key: &'static str,
    on: bool,
    /// `extension` / `patch` / `web`。
    kind: &'static str,
    loaded: Option<LoadState>,
}

/// `GET /api/enhance/status` 应答（`PUT /api/enhance/qol` 改完也回这一份）。`hlSnapCjk`/`notesImportMdEnabled`/`tapPageTurn`
/// 三个顶层布尔是 2026-10-10 加 `toggles` 之前的形状，笔记页（`notesImportMdEnabled`）还在读，保留。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    hl_snap_cjk: bool,
    notes_import_md_enabled: bool,
    tap_page_turn: bool,
    toggles: Vec<ToggleState>,
    loaded: loaded::Loaded,
}

fn toggle_states(q: &qol::Qol, l: &loaded::Loaded) -> Vec<ToggleState> {
    TOGGLES
        .iter()
        .map(|t| {
            let kind = match t.artifact {
                Artifact::Extension(_) => "extension",
                Artifact::Patch(_) => "patch",
                Artifact::Web => "web",
            };
            ToggleState { key: t.key, on: q.on(t), kind, loaded: load_state(&t.artifact, l) }
        })
        .collect()
}

pub fn status(paths: &Paths) -> Reply {
    let q = qol::Qol::load(paths);
    let loaded = xochitl_loaded(paths);
    let toggles = toggle_states(&q, &loaded);
    let on = |key: &str| toggles.iter().any(|t| t.key == key && t.on);
    Reply::ok(&Status { hl_snap_cjk: on("hlSnapCjk"), notes_import_md_enabled: on("notesImportMdEnabled"), tap_page_turn: on("tapPageTurn"), toggles, loaded })
}

/// `PUT /api/enhance/qol`：接 [`TOGGLES`] 里任一键的布尔值（`{hlSnapCjk}`/`{notesImportMdEnabled}`/`{tapPageTurn}`），
/// body 里出现哪个就改哪个（`qol::patch` 本身是通用的 key-patch）。「漫画页边距」开关 `comicMinMargin` 2026-10-07 删除
/// （带 sheng-ren 页边距标记的漫画一律登记，见 book-serve `comic_margins.rs`），「日漫翻页规则」`rtlPageTurn` 同日删除；
/// 旧文件里的这两个键照常原样保留、不再有人读。
pub fn set_qol(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let body = req.json()?;
    let mut changes = serde_json::Map::new();
    // 与 reading-qol.json 键同名的直通布尔开关。
    for t in TOGGLES {
        if let Some(v) = body.0.get(t.key).and_then(|v| v.as_bool()) {
            changes.insert(t.key.into(), serde_json::Value::Bool(v));
        }
    }
    if changes.is_empty() {
        let keys: Vec<&str> = TOGGLES.iter().map(|t| t.key).collect();
        return Err(ApiError::bad(format!("body 需要 {} 其中一个布尔字段", keys.join("/"))));
    }
    qol::patch(paths, changes).map_err(ApiError::internal)?;
    Ok(status(paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmsvc_core::http::{Method, TestRequest};

    fn put(paths: &Paths, body: &[u8]) -> ApiResult {
        TestRequest::new(Method::Put, "/api/enhance/qol").content_type("application/json").body(body.to_vec()).with(|r| set_qol(paths, r))
    }

    #[test]
    fn set_qol_applies_only_present_boolean_keys_and_rejects_empty() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let rep = put(&paths, br#"{"notesImportMdEnabled":true,"hlSnapCjk":false,"junk":1}"#).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&rep.body).unwrap();
        assert_eq!((v["notesImportMdEnabled"].as_bool(), v["hlSnapCjk"].as_bool()), (Some(true), Some(false)));
        assert_eq!(v["tapPageTurn"], false, "没传的键保持缺省");
        // 只传 tapPageTurn：上一次写的键不被冲掉
        put(&paths, br#"{"tapPageTurn":true}"#).unwrap();
        assert!(qol::Qol::load(&paths).on_key("notesImportMdEnabled"));
        // 已删的 comicMinMargin（2026-10-07）不再被认：单独传它 → 拒绝
        assert!(put(&paths, br#"{"comicMinMargin":true}"#).is_err());
        // 已移除的 hwStrokeEnabled（2026-09-30）不再被认：单独传它 → 拒绝，也不写任何键
        assert!(put(&paths, br#"{"hwStrokeEnabled":true}"#).is_err());
        assert!(!std::fs::read_to_string(paths.home().join(".local/share/cangjie-ime/reading-qol.json")).unwrap().contains("hwStroke"));
        // 没有任何可识别的布尔字段 → 拒绝
        assert!(put(&paths, br#"{"hlSnapCjk":"yes"}"#).is_err());
    }

    /// 单击翻页开关：缺省关；单独写不冲掉别的键。已删的 rtlPageTurn 单独传 → 拒绝，状态里也不再有它。
    #[test]
    fn tap_page_turn_default_off_and_rtl_switch_gone() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        assert!(!qol::Qol::load(&paths).on_key("tapPageTurn"));
        put(&paths, br#"{"notesImportMdEnabled":true}"#).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&put(&paths, br#"{"tapPageTurn":true}"#).unwrap().body).unwrap();
        assert_eq!(v["tapPageTurn"].as_bool(), Some(true));
        assert!(v.get("rtlPageTurn").is_none());
        assert!(put(&paths, br#"{"rtlPageTurn":true}"#).is_err());
        let q = qol::Qol::load(&paths);
        assert!(q.on_key("tapPageTurn") && q.on_key("notesImportMdEnabled"));
    }

    /// 线上格式快照（2026-10-10，GW-1）：`GET /api/enhance/status`（「管理」页开关、笔记页「导入 md」可见性读它）。
    /// 开发机/CI 上没有 xochitl 进程，`loaded` 是全空的那一份。
    #[test]
    fn wire_snapshot_status() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let v: serde_json::Value = serde_json::from_slice(&status(&paths).body).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "hlSnapCjk": true, "notesImportMdEnabled": false, "tapPageTurn": false,
                "toggles": [
                    {"key": "hlSnapCjk", "on": true, "kind": "extension", "loaded": "unknown"},
                    {"key": "tapPageTurn", "on": false, "kind": "patch", "loaded": "unknown"},
                    {"key": "notesImportMdEnabled", "on": false, "kind": "web", "loaded": null},
                ],
                "loaded": {"xochitl": false, "xovi": false, "extensions": [], "qmds": [], "qmdsPending": []},
            })
        );
    }

    /// 加载状态判定（逐字移植自网页原来的徽章逻辑）：没有 xochitl → unknown；扩展看映射；补丁看已载入 / 待重启；网页功能不判。
    #[test]
    fn load_state_matches_former_web_badges() {
        let l = |xochitl: bool, ext: &[&str], qmds: &[&str], pending: &[&str]| loaded::Loaded {
            xochitl,
            xovi: true,
            extensions: ext.iter().map(|s| s.to_string()).collect(),
            qmds: qmds.iter().map(|s| s.to_string()).collect(),
            qmds_pending: pending.iter().map(|s| s.to_string()).collect(),
        };
        let so = Artifact::Extension("hl-snap.so");
        let qmd = Artifact::Patch("reader-page-turn.qmd");
        assert_eq!(load_state(&so, &l(false, &["hl-snap.so"], &[], &[])), Some(LoadState::Unknown), "没有 xochitl 主进程：判断不了");
        assert_eq!(load_state(&so, &l(true, &["hl-snap.so"], &[], &[])), Some(LoadState::On));
        assert_eq!(load_state(&so, &l(true, &["other.so"], &["hl-snap.so"], &[])), Some(LoadState::Off), "扩展只看映射，不看 qmd 列表");
        assert_eq!(load_state(&qmd, &l(false, &[], &["reader-page-turn.qmd"], &[])), Some(LoadState::Unknown));
        assert_eq!(load_state(&qmd, &l(true, &[], &["reader-page-turn.qmd"], &[])), Some(LoadState::On));
        assert_eq!(load_state(&qmd, &l(true, &[], &[], &["reader-page-turn.qmd"])), Some(LoadState::Pending));
        assert_eq!(load_state(&qmd, &l(true, &["reader-page-turn.qmd"], &[], &[])), Some(LoadState::Off));
        assert_eq!(load_state(&Artifact::Web, &l(true, &[], &[], &[])), None);
        assert_eq!(load_state(&Artifact::Web, &l(false, &[], &[], &[])), None);
    }

    /// 表里键名不重复，缺省值与读这些键的那一方一致（hl-snap 缺省开，其余缺省关）。
    #[test]
    fn toggles_table_is_consistent() {
        let keys: std::collections::BTreeSet<&str> = TOGGLES.iter().map(|t| t.key).collect();
        assert_eq!(keys.len(), TOGGLES.len());
        assert!(toggle("hlSnapCjk").unwrap().default);
        assert!(!toggle("tapPageTurn").unwrap().default && !toggle("notesImportMdEnabled").unwrap().default);
        assert!(toggle("rtlPageTurn").is_none() && toggle("comicMinMargin").is_none() && toggle("hwStrokeEnabled").is_none());
    }
}
