//! 测试专用：一律用 [`sandbox`] 造 `Paths`。
//!
//! 此前各模块测试各写一份 `Paths::resolve(...)`，有的只给 `HOME`、有的只给某个 `XDG_*`：没给的变量会落到真实的缺省
//! 位置（`/home/root/...`、`/tmp/shelf-<uid>`），测试并没有真正隔离——2026-09-20 首版带清理的测试就清过开发机的真实
//! 目录。这里把 `HOME` 与全部 XDG 目录都指进同一个临时目录，谁都不会碰到临时目录以外的东西。
use rmsvc_core::paths::Paths;

/// `HOME` = 临时目录本身；`XDG_*` = 临时目录下同名子目录（`<tmp>/XDG_STATE_HOME` 这样，看名字就知道是谁）。
pub fn sandbox(t: &tempfile::TempDir) -> Paths {
    let h = t.path().to_string_lossy().to_string();
    Paths::resolve(move |k| match k {
        "HOME" => Some(h.clone()),
        "XDG_CONFIG_HOME" | "XDG_DATA_HOME" | "XDG_STATE_HOME" | "XDG_CACHE_HOME" | "XDG_RUNTIME_DIR" => Some(format!("{h}/{k}")),
        _ => None,
    })
}
