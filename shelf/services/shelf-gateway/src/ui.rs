//! 单页 UI（手机/电脑浏览器打开 `https://<设备IP>:8778/`）。固定 tab「传书」（母版库总入口）+「管理」，中间的服务 tab
//! 按 `/api/services` 注册表动态生成（xochitl 字体 = font-serve、KOReader = koreader-serve、壁纸 = wallpaper-serve）。
//! 上传逐文件一请求（每本独立成败、独立进度条），所有上传口共用一个 `uploader` + 服务端同形回执（`asset::receipt`）；
//! 格式白名单由 [`page`] 从 `shelf_core::formats` 注入（`__EXTS__`），网页 accept / 选中即拦与服务端上传门同源。
//! 页面源码在 `services/shelf-gateway/ui/`（index.html 骨架 + style.css + app.js + auth.css），编译期 `include_str!` 进二进制：
//! 网页仍是单文件零外链，但 JS/CSS 是真文件——编辑器/`node --check`（CI）直接检查，改样式不用在 Rust 原始字符串里找。
use std::sync::OnceLock;

const INDEX_HTML: &str = include_str!("../ui/index.html");
const STYLE_CSS: &str = include_str!("../ui/style.css");
const APP_JS: &str = include_str!("../ui/app.js");
const AUTH_CSS: &str = include_str!("../ui/auth.css");

/// 渲染主页：骨架 + 样式 + 脚本拼成单文件，再把格式白名单注入（进程内只算一次）。
pub fn page() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    PAGE.get_or_init(|| {
        use shelf_core::formats::{BOOK_EXTS, DICT_EXTS, FONT_EXTS, HOST_CONVERTIBLE_EXTS, IMAGE_EXTS, KOREADER_ONLY_EXTS, NATIVE_EXTS};
        let exts = serde_json::json!({"book": BOOK_EXTS, "native": NATIVE_EXTS, "convertible": HOST_CONVERTIBLE_EXTS, "koOnly": KOREADER_ONLY_EXTS, "font": FONT_EXTS, "dict": DICT_EXTS, "image": IMAGE_EXTS});
        INDEX_HTML.replace("__STYLE__", STYLE_CSS).replace("__SCRIPT__", APP_JS).replace("__EXTS__", &exts.to_string())
    })
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 登录页：只要密码，无用户名。`error` 空=无提示。
pub fn login_page(error: &str, next: &str) -> String {
    format!(r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>书架 · 登录</title><style>{AUTH_CSS}</style></head><body>
<form method="post" action="/login" autocomplete="on"><h1>书架</h1>
<label for="pw">密码</label><input id="pw" name="password" type="password" autofocus required autocomplete="current-password">
<input type="hidden" name="next" value="{next}"><div class="err">{err}</div><button type="submit">登录</button>
<p class="small">首次使用密码为 <code>shelf</code>，登录后必须改。<br>浏览器提示"不安全"是自签证书所致：<a href="/ca.crt">下载 CA 证书</a> 装进手机/电脑信任库一次即不再提示。</p></form></body></html>"#, next = esc(next), err = esc(error))
}

/// 改密码页：`forced`=首登必改（不给"返回"）。
pub fn password_page(error: &str, forced: bool) -> String {
    let hint = if forced { "首次登录：请先设置新密码（至少 6 位，不能是默认密码）。" } else { "至少 6 位。改完其它已登录设备需重新登录。" };
    let back = if forced { "" } else { r#"<p class="small"><a href="/">返回书架</a></p>"# };
    format!(r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>书架 · 改密码</title><style>{AUTH_CSS}</style></head><body>
<form method="post" action="/password"><h1>设置密码</h1><p class="small" style="margin-top:0">{hint}</p>
<label for="cur">当前密码</label><input id="cur" name="current" type="password" required autocomplete="current-password">
<label for="new">新密码</label><input id="new" name="new" type="password" required minlength="6" autocomplete="new-password">
<label for="cf">再输一次</label><input id="cf" name="confirm" type="password" required minlength="6" autocomplete="new-password">
<div class="err">{err}</div><button type="submit">保存</button>{back}</form></body></html>"#, err = esc(error))
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_injects_format_whitelists_once() {
        let p = super::page();
        assert!(!p.contains("__EXTS__"), "占位应被替换");
        assert!(p.contains(r#""book":["epub","pdf""#) && p.contains(r#""font":["ttf""#) && p.contains(r#""dict":["ifo""#) && p.contains(r#""image":["jpg""#));
        assert!(p.contains(r#""native":["epub","pdf"]"#) && p.contains(r#""convertible":["azw3""#) && p.contains(r#""koOnly":["cbz""#), "三档格式说明注入");
        assert!(std::ptr::eq(p, super::page()), "OnceLock 只渲染一次");
        assert!(!p.contains("__STYLE__") && !p.contains("__SCRIPT__") && p.contains("<style>") && p.contains("</script></body></html>"), "骨架三段拼接完整");
        assert!(super::APP_JS.contains("__EXTS__") && !super::APP_JS.contains("__STYLE__"), "白名单占位在 app.js");
    }
}
