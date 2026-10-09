//! 界面字体的选择：`$XDG_DATA_HOME/shelf/ui-font.json` = `{"version":1,"sans":"…","serif":"…"}`，空串 = 用原生字体。
//! 读它的有两处，都只在 xochitl 启动时读一次，所以改完要整机重启：
//!   - `enhance/ui-font`（xovi 扩展）：xochitl 设应用默认字体时把 reMarkable Sans 换成 `sans`（老界面、没用设计令牌的文字）；
//!   - `shelf/xovi/ui-font-tokens.qmd`：Ark 设计令牌里的 reMarkable Sans / reMarkable Serif Small 换成 `sans` / `serif`
//!     （新版设置页、对话框、标题等）。
//!
//! 只能选界面字体仓库（`fonts/shelf-ui/`）里的家族；阅读字体不在可选之列，界面字体也不进阅读器菜单。
use rmsvc_core::paths::Paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiSelection {
    pub version: u32,
    /// 正文（替换 reMarkable Sans）
    pub sans: String,
    /// 标题（替换 reMarkable Serif Small）
    pub serif: String,
}

pub struct UiFont {
    path: PathBuf,
    /// 本次开机时的选择：xochitl 只在启动时读，和它不一样就是"待重启"。
    boot: UiSelection,
    cur: Mutex<UiSelection>,
}

impl UiFont {
    pub fn load(paths: &Paths) -> UiFont {
        let path = paths.data_dir().join("ui-font.json");
        let sel: UiSelection = rmsvc_core::config::load_or_default(&path);
        UiFont { path, boot: sel.clone(), cur: Mutex::new(sel) }
    }

    pub fn get(&self) -> UiSelection {
        rmsvc_core::sync::lock(&self.cur).clone()
    }

    pub fn restart_needed(&self) -> bool {
        self.get() != self.boot
    }

    /// 改选择。`installed`：界面字体仓库现有的家族名；不在其中且非空的拒绝。
    pub fn set(&self, sans: &str, serif: &str, installed: &[String]) -> Result<UiSelection, String> {
        for f in [sans, serif] {
            if !f.is_empty() && !installed.iter().any(|k| k == f) {
                return Err(format!("没有这个界面字体：{f}（先上传）"));
            }
        }
        self.save(UiSelection { version: 1, sans: sans.into(), serif: serif.into() })
    }

    /// 删了某个界面字体家族：选着它的那一项退回原生。返回是否改了。
    pub fn forget(&self, family: &str) -> Result<bool, String> {
        let mut sel = self.get();
        let before = sel.clone();
        if sel.sans == family {
            sel.sans.clear();
        }
        if sel.serif == family {
            sel.serif.clear();
        }
        if sel == before {
            return Ok(false);
        }
        sel.version = 1;
        self.save(sel).map(|_| true)
    }

    fn save(&self, sel: UiSelection) -> Result<UiSelection, String> {
        rmsvc_core::config::save(&self.path, &sel, None)?;
        *rmsvc_core::sync::lock(&self.cur) = sel.clone();
        Ok(sel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        paths.ensure().unwrap();
        (t, paths)
    }

    #[test]
    fn select_validates_persists_and_tracks_restart() {
        let (_t, paths) = setup();
        let u = UiFont::load(&paths);
        assert_eq!(u.get(), UiSelection::default(), "没有文件 = 原生");
        assert!(!u.restart_needed());
        let installed = vec!["Sarasa UI SC".to_string()];
        assert!(u.set("Nope", "", &installed).is_err(), "没装的拒绝");
        assert!(!u.restart_needed(), "拒绝时不改");
        u.set("Sarasa UI SC", "Sarasa UI SC", &installed).unwrap();
        assert!(u.restart_needed());
        let raw = std::fs::read_to_string(paths.data_dir().join("ui-font.json")).unwrap();
        let j: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!((j["sans"].as_str(), j["serif"].as_str(), j["version"].as_u64()), (Some("Sarasa UI SC"), Some("Sarasa UI SC"), Some(1)));
        // 重新加载（模拟开机）：读回同样的选择，不再待重启
        let u2 = UiFont::load(&paths);
        assert_eq!(u2.get().sans, "Sarasa UI SC");
        assert!(!u2.restart_needed());
        // 删了这个家族：两项都退回原生
        assert!(u2.forget("Sarasa UI SC").unwrap());
        assert_eq!(u2.get(), UiSelection { version: 1, sans: String::new(), serif: String::new() });
        assert!(!u2.forget("Sarasa UI SC").unwrap(), "没选它就不动");
        // 改回原生（空串）总是允许
        u2.set("", "", &[]).unwrap();
    }
}
