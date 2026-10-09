//! `reading-qol.json` 读写（cangjie-ime 设置页共享配置，`~/.local/share/cangjie-ime/reading-qol.json`）。
//!
//! **全量写回铁律**（旧系统增强白皮书 §08「全量防覆盖」，那份已移出仓库；现行说明见 enhance 白皮书 §02）：`langhook`（C hook）+ 原生「设置」App 的
//! QML 子页都要求写这个文件时带上全部已知键——只写自己关心的那个会把别的开关（★全局待办/单词笔记/
//! 手写识别配置…）冲掉。这里不照抄 QML 那种手写全部字段的方式：[`patch`] 把整份文件当成不透明的
//! `serde_json::Map` 读进来，只覆盖调用方明确要改的键，其余原样写回——不知道、不关心的键天然不会丢，
//! 也不怕将来别处新增字段时这边漏改。
use rmsvc_core::paths::Paths;
use serde_json::{Map, Value};
use std::path::PathBuf;

fn path(paths: &Paths) -> PathBuf {
    paths.home().join(".local/share/cangjie-ime/reading-qol.json")
}

/// 缺失、损坏或顶层不是对象 → 空表。
fn load(paths: &Paths) -> Map<String, Value> {
    rmsvc_core::config::load_or_default(&path(paths))
}

/// 只 patch 传入的键，其余原样透传写回。
pub fn patch(paths: &Paths, changes: Map<String, Value>) -> Result<(), String> {
    // 进程内串行化读-改-写：网页连点几个开关会并发进来多个 PUT，各自 load→改→写会互相覆盖对方刚改的键。
    // （跨进程——QML/C hook 也写这个文件——靠"全量写回"约定，锁不到，见头注。）
    static PATCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = rmsvc_core::sync::lock(&PATCH_LOCK);
    let mut map = load(paths);
    map.extend(changes);
    rmsvc_core::config::save(&path(paths), &map, None) // 缩进 JSON、原子写，父目录自己建
}

/// `reading-qol.json` 的一次快照：`/api/enhance/status` 一次请求要读好几个开关，读一次文件、各开关从同一份
/// 快照取（此前每个开关各自读一遍整份文件）。
pub struct Qol(Map<String, Value>);

impl Qol {
    pub fn load(paths: &Paths) -> Qol {
        Qol(load(paths))
    }

    fn flag(&self, key: &str, default: bool) -> bool {
        self.0.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    /// CJK 荧光笔精确吸附开关（`hlSnapCjk`，langhook C hook 消费）。缺省视为开——跟 QML 侧 `c.hlSnapCjk !== false`
    /// 同一条缺省规则（`xovi-extensions/reading-qol/settings-reading-enhance.qmd`）。
    pub fn hl_snap_cjk(&self) -> bool {
        self.flag("hlSnapCjk", true)
    }

    /// 「导入 md 文档」开关（`notesImportMdEnabled`），控制笔记 tab「导入」子标签是否显示。跟
    /// `hl_snap_cjk` 缺省开不同，这个缺省关——新功能第一次上线，不想让用户点开笔记 tab 就撞见一个
    /// 半成品，得手动去「管理→实验室」打开才看得到。
    pub fn notes_import_md_enabled(&self) -> bool {
        self.flag("notesImportMdEnabled", false)
    }

    /// 「单击翻页」开关（`tapPageTurn`，2026-09-24）：xochitl 阅读器里的 `reader-page-turn.qmd` 每次打开书时读它（不轮询），
    /// 切换后下次打开书生效。缺省关 = xochitl 原生行为。「日漫翻页规则」`rtlPageTurn` 2026-10-07 删除（书架只管入库，
    /// 翻页方向交给书本身）；旧文件里的这个键原样保留、不再有人读。
    pub fn tap_page_turn(&self) -> bool {
        self.flag("tapPageTurn", false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_paths() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let paths = crate::testutil::sandbox(&t);
        (t, paths)
    }

    #[test]
    fn hl_snap_cjk_defaults_true_when_missing() {
        let (_t, paths) = tmp_paths();
        assert!(Qol::load(&paths).hl_snap_cjk(), "文件不存在时缺省视为开，跟 QML 侧一致");
    }

    #[test]
    fn patch_preserves_unknown_keys() {
        let (_t, paths) = tmp_paths();
        // 模拟 QML 已经写过一份、带上跟这个面板无关的开关
        let mut seed = Map::new();
        seed.insert("starTodoEnabled".into(), Value::Bool(true));
        seed.insert("cardhwEnabled".into(), Value::Bool(true));
        seed.insert("hlSnapCjk".into(), Value::Bool(true));
        patch(&paths, seed).unwrap();

        let mut change = Map::new();
        change.insert("hlSnapCjk".into(), Value::Bool(false));
        patch(&paths, change).unwrap();

        assert!(!Qol::load(&paths).hl_snap_cjk(), "patch 的键要生效");
        let full = load(&paths);
        assert_eq!(full["starTodoEnabled"], Value::Bool(true), "没碰过的键不能被冲掉");
        assert_eq!(full["cardhwEnabled"], Value::Bool(true), "没碰过的键不能被冲掉");
    }

    /// 2026-09-30 移除手写优化后，旧设备上 `reading-qol.json` 里还留着 `hwStroke*` 键：网页改别的开关时照样原样写回。
    #[test]
    fn patch_keeps_removed_hw_stroke_keys() {
        let (_t, paths) = tmp_paths();
        let mut seed = Map::new();
        seed.insert("hwStrokeNibMinRatio".into(), serde_json::json!(0.6));
        patch(&paths, seed).unwrap();
        let mut change = Map::new();
        change.insert("tapPageTurn".into(), Value::Bool(true));
        patch(&paths, change).unwrap();
        assert_eq!(load(&paths)["hwStrokeNibMinRatio"], serde_json::json!(0.6));
    }

    #[test]
    fn notes_import_md_enabled_defaults_false() {
        let (_t, paths) = tmp_paths();
        assert!(!Qol::load(&paths).notes_import_md_enabled(), "新功能第一次上线，缺省关，不是缺省开");
    }
}
