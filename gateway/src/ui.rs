//! 单页 UI（手机/电脑浏览器打开 `https://<设备IP>/`，2026-09-10 起绑标准 443 端口，不用带端口号）。固定 tab「传书」（母版库总入口）+「管理」，中间的服务 tab
//! 按 `/api/services` 注册表动态生成（笔记 = note-serve；xochitl 字体 = font-serve、壁纸 = wallpaper-serve 收在「其他」里）。
//! 上传逐文件一请求（每本独立成败、独立进度条），所有上传口共用一个 `uploader` + 服务端同形回执（`asset::receipt`）；
//! 格式白名单由 [`page`] 从 `rmsvc_core::formats` 注入（`__EXTS__`），网页 accept / 选中即拦与服务端上传门同源。
//! 页面源码在 `gateway/ui/`（index.html 骨架 + style.css + 几个 `.js`（拼接顺序见 [`APP_JS`]）+ auth.css），编译期 `include_str!` 进二进制：
//! 网页仍是单文件零外链，但 JS/CSS 是真文件——编辑器/`node --check`（CI）直接检查，改样式不用在 Rust 原始字符串里找。
use rmsvc_core::http::html_escape as esc;
use std::sync::OnceLock;

const INDEX_HTML: &str = include_str!("../ui/index.html");
const STYLE_CSS: &str = include_str!("../ui/style.css");
/// 网页脚本按职责拆成几个源文件（2026-10-10），编译期按下面的顺序首尾相接拼回**同一个** `<script>`：页面仍是单文件、
/// 零额外请求，运行时与拆分前只差顶层定义的先后顺序。全部是经典脚本、共用一个全局作用域（不是 ES 模块），所以：
/// ① 顶层 `const` 只能引用排在它前面的文件里的名字（函数声明会提升，不受此限）；② 除最后的 `app.js` 外，各文件只定义、
/// 加载时不碰 DOM/location——`core.js` 连调用时也不碰 DOM，node 测试整文件加载它（`ui/test/load.mjs`）。
/// - `core.js`：纯逻辑——转义/格式化、i18n 的 `T()`、格式白名单 `EXT`（`__EXTS__` 占位在这里）、`j()` 等取数封装、
///   `coalesce` 这类可单测的小件。
/// - `dom.js`：DOM 小工具——`el`/`btn`/toast/对话框/上传器/二级标签。
/// - `transfer.js`「传书」、`notes.js`「笔记」、`assets.js`「其他」（字体/壁纸）、`manage.js`「管理」（含模型卡片）、
///   `health.js`「设备健康」与页头横幅。
/// - `app.js`：启动代码（唯一有加载期副作用的文件）：建 tab、开 SSE。CI 的 `node --check gateway/ui/app.js` 只查它，
///   其余文件的语法由 `ui/test/scripts.test.mjs` 编译整段拼接产物兜住。
const APP_JS: &str = concat!(
    include_str!("../ui/core.js"),
    include_str!("../ui/dom.js"),
    include_str!("../ui/transfer.js"),
    include_str!("../ui/notes.js"),
    include_str!("../ui/assets.js"),
    include_str!("../ui/manage.js"),
    include_str!("../ui/health.js"),
    include_str!("../ui/app.js"),
);
const AUTH_CSS: &str = include_str!("../ui/auth.css");

/// i18n 语言包（2026-09-09 起；09-10 起覆盖主界面全部正文，只有登录/改密码页仍是 [`login_page`]/[`password_page`]
/// 里的中文——未登录态读不到网页的语言选择）。继续走 `include_str!` 编译进二进制，不破坏"单文件
/// 零外链"部署（不用改 build/deploy/install 脚本，语言包新增/改词只是改这两个 JSON 再重新编译）。
const LOCALE_ZH_CN: &str = include_str!("../ui/locales/zh-CN.json");
const LOCALE_EN_US: &str = include_str!("../ui/locales/en-US.json");

/// 按语言码取语言包 JSON；不认识的语言码一律落中文（不是空白页面）。
pub fn locale_json(lang: &str) -> &'static str {
    match lang {
        "en-US" | "en" => LOCALE_EN_US,
        _ => LOCALE_ZH_CN,
    }
}

/// 渲染主页：骨架 + 样式 + 脚本拼成单文件，再把格式白名单注入（进程内只算一次）。
pub fn page() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    PAGE.get_or_init(|| {
        use rmsvc_core::formats::{BOOK_EXTS, FONT_EXTS, IMAGE_EXTS};
        // "convertible" 档（azw3/mobi/azw/prc/fb2/txt）2026-09-17 随 EPUB 线架构调整退役；"仅
        // KOReader" 档（cbz/cbr/djvu/html/htm/rtf/doc/docx/chm/xps）2026-09-18 用户明确要求一并
        // 退役——母版库只收 EPUB/PDF，`BOOK_EXTS == NATIVE_EXTS`，不再需要 `koOnly` 字段区分两档；`native` 字段同理 2026-10-07 不再注入。
        // `dict`（KOReader 词典上传口的格式）随 2026-09-29 设备卸载 KOReader、网页撤掉词典上传一并不再注入。
        let exts = serde_json::json!({"book": BOOK_EXTS, "font": FONT_EXTS, "image": IMAGE_EXTS});
        INDEX_HTML.replace("__STYLE__", STYLE_CSS).replace("__SCRIPT__", APP_JS).replace("__EXTS__", &exts.to_string())
    })
}

/// 登录页：只要密码，无用户名。`error` 空=无提示。
pub fn login_page(error: &str, next: &str) -> String {
    format!(r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>秘密花园 · 登录</title><style>{AUTH_CSS}</style></head><body>
<form method="post" action="/login" autocomplete="on"><h1>秘密花园</h1>
<label for="pw">密码</label><input id="pw" name="password" type="password" autofocus required autocomplete="current-password">
<input type="hidden" name="next" value="{next}"><div class="err">{err}</div><button type="submit">登录</button>
<p class="small">首次使用密码为 <code>shelf</code>，登录后必须改。<br>浏览器提示"不安全"是自签证书所致：<a href="/ca.crt">下载 CA 证书</a> 装进手机/电脑信任库一次即不再提示。</p></form></body></html>"#, next = esc(next), err = esc(error))
}

/// 改密码页：`forced`=首登必改（不给"返回"）。
pub fn password_page(error: &str, forced: bool) -> String {
    // 2026-09-09 审计修：密码最小长度原来在这里硬编码了两处 `minlength="6"` + 提示文案里的"6"，
    // 跟 `config::MIN_PASSWORD_LEN`（服务端真正校验用的那个）各写各的——真改了那个常量，这里
    // 三处不会跟着变，会出现"服务端要求 N 位，网页却只拦到 6 位就放行提交"的体验错配（不是安全
    // 漏洞，服务端仍是最终裁决者，纯粹 UX 一致性问题）。改成从常量插值，单一事实源。
    let n = crate::config::MIN_PASSWORD_LEN;
    let hint = if forced { format!("首次登录：请先设置新密码（至少 {n} 位，不能是默认密码）。") } else { format!("至少 {n} 位。改完其它已登录设备需重新登录。") };
    let back = if forced { "" } else { r#"<p class="small"><a href="/">返回秘密花园</a></p>"# };
    format!(r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>秘密花园 · 改密码</title><style>{AUTH_CSS}</style></head><body>
<form method="post" action="/password"><h1>设置密码</h1><p class="small" style="margin-top:0">{hint}</p>
<label for="cur">当前密码</label><input id="cur" name="current" type="password" required autocomplete="current-password">
<label for="new">新密码</label><input id="new" name="new" type="password" required minlength="{n}" autocomplete="new-password">
<label for="cf">再输一次</label><input id="cf" name="confirm" type="password" required minlength="{n}" autocomplete="new-password">
<div class="err">{err}</div><button type="submit">保存</button>{back}</form></body></html>"#, err = esc(error))
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_injects_format_whitelists_once() {
        let p = super::page();
        assert!(!p.contains("__EXTS__"), "占位应被替换");
        assert!(p.contains(r#""book":["epub","pdf"]"#) && p.contains(r#""font":["ttf""#) && p.contains(r#""image":["jpg""#));
        assert!(!p.contains(r#""dict""#), "KOReader 词典上传口已撤，格式表不再注入");
        assert!(!p.contains(r#""native""#), "native 与 book 相同，2026-10-07 起不再注入");
        assert!(!p.contains(r#""convertible""#), "convertible 档已随 EPUB 线架构调整退役");
        assert!(!p.contains(r#""koOnly""#), "仅 KOReader 档已随 2026-09-18 格式收窄退役");
        assert!(std::ptr::eq(p, super::page()), "OnceLock 只渲染一次");
        assert!(!p.contains("__STYLE__") && !p.contains("__SCRIPT__") && p.contains("<style>") && p.contains("</script></body></html>"), "骨架三段拼接完整");
        assert!(super::APP_JS.contains("__EXTS__") && !super::APP_JS.contains("__STYLE__"), "白名单占位在拼接后的脚本里");
    }

    /// 拆分后的脚本拼回去要包含拆分前 app.js 的全部顶层定义，且每个只定义一次（漏拼一个文件、或两个文件重复定义同名
    /// 函数，浏览器里要么 ReferenceError、要么后者静默盖掉前者）。名单取自 2026-10-10 拆分前的 app.js。
    #[test]
    fn concatenated_script_defines_every_top_level_name_once() {
        let js = super::APP_JS;
        let consts = [
            "$", "esc", "fmtB", "stagingLowSpace", "badge", "xoviBadge", "wait", "coalesce", "refreshSec", "guardClick", "btn", "el", "renderBusy", "toastHost", "showToast", "toast", "toastLink",
            "modal", "confirmDialog", "promptDialog", "LS", "onUsb", "T", "currentLang", "EXT", "BOOK_EXT", "up", "authRedirect", "jsend", "sendT", "postJ", "bindToggle", "upHtml", "delBtn", "fontLabel",
            "cjkBadge", "GUIDE", "stgNameOptions", "stgIsTodo", "TABS", "STYLE_NAMES", "STATUS_NAMES", "OBSIDIAN_ICON", "DEST_ICON", "DEST_ORDER", "statusName", "PROVIDER_NAMES", "fmtUptime", "fmtSec",
            "fmtTime", "banner", "modAct",
        ];
        for n in consts {
            let at_line_start = js.lines().filter(|l| l.starts_with(&format!("const {n}=")) || l.starts_with(&format!("const {n},"))).count();
            assert_eq!(at_line_start, 1, "顶层 const {n} 应恰好定义一次");
        }
        let funcs = [
            "j", "uploader", "subtabs", "fillList", "stgBadges", "stgRow", "renderTransfer", "renderFonts", "assetTab", "renderOther", "renderNotes", "mountModelPanel", "mountHealth", "mountCleanup",
            "showOtaBanner", "showWifiBanner", "showAgentFailBanner", "renderManage",
        ];
        for f in funcs {
            assert_eq!(js.matches(&format!("function {f}(")).count(), 1, "function {f} 应恰好定义一次");
        }
        assert!(js.trim_end().ends_with("})();"), "启动代码（app.js 的自执行函数）拼在最后");
    }
    #[test]
    fn locale_files_have_identical_key_sets() {
        // 语言包 key 漂移是这套 i18n 最容易悄悄坏掉的地方：某个语言加了新 key、另一个忘了加，
        // 前端查表查不到会静默显示 undefined（比显示错误语言更容易被漏看）——用一条离线测试钉住
        // "两份文件 key 集合完全一致"，比指望人工记得同步两份 JSON 可靠。
        let zh: serde_json::Value = serde_json::from_str(super::LOCALE_ZH_CN).unwrap();
        let en: serde_json::Value = serde_json::from_str(super::LOCALE_EN_US).unwrap();
        let keys = |v: &serde_json::Value| -> std::collections::BTreeSet<String> { v.as_object().unwrap().keys().cloned().collect() };
        assert_eq!(keys(&zh), keys(&en), "两份语言包的 key 集合必须完全一致");
        assert!(!keys(&zh).is_empty());
    }

    /// 网页里**动态拼接**的 i18n 键（`T('ota.reason.'+r)` 这类）：取值来自后端，后端新增一个取值、语言包忘了加，页面就会
    /// 显示键名本身，而 node 的语言包测试只查得到字面量键（FE-3，2026-10-10）。这里遍历后端会产生的取值集合，断言两份
    /// 语言包都有对应键。取值集合尽量直接取 Rust 常量本身；网关里没有源头的（由别的服务/脚本产生），在下面就地列出并注明出处。
    #[test]
    fn dynamic_i18n_keys_cover_every_backend_value() {
        use crate::device::ota::{Reason, Recovery};
        /// WiFi 横幅只对这两种状态出文案（`health.js` 的 `showWifiBanner`）。取值由 `packaging/wifi-watch/wifi-watch.sh`
        /// 的 `probe()`（约 101-105 行：`ok`/`none`/`portal`）写进状态文件，网关原样转发，`ok` 与网关兜底的 `unknown` 不出横幅。
        const WIFI_BANNER_STATES: &[&str] = &["portal", "none"];
        /// 代理放弃记录的 `kind`：由 book-serve 产生——`shelf/services/book-serve/src/trash.rs:96` 记 `trash`、
        /// `shelf/services/book-serve/src/mkdir.rs:151` 记 `mkdir`（`agent_failures.rs` 的 `Failure.kind` 是 String）。
        /// 网关里没有源头，先在这里列一份；book-serve 把它收成枚举后应改为直接引用。
        const AGENT_FAIL_KINDS: &[&str] = &["trash", "mkdir"];
        // 按线上格式（serde）把枚举取成字符串。
        fn ser<T: serde::Serialize>(v: &T) -> String {
            serde_json::to_value(v).unwrap().as_str().unwrap().to_string()
        }
        let mut want: Vec<String> = Vec::new();
        want.extend(Reason::ALL.iter().map(|r| format!("ota.reason.{}", ser(r))));
        want.extend(Recovery::SHOWN.iter().map(|r| format!("ota.recovery.{}", ser(r))));
        want.extend(crate::batch::Action::ALL.iter().map(|a| format!("stg.batch.{}", a.key())));
        want.extend(WIFI_BANNER_STATES.iter().map(|s| format!("wifi.banner.{s}")));
        want.extend(AGENT_FAIL_KINDS.iter().map(|k| format!("agentfail.{k}")));
        want.extend(crate::manage::MODULES.iter().map(|m| format!("manage.modules.label.{}", m.seg)));
        for (name, text) in [("zh-CN", super::LOCALE_ZH_CN), ("en-US", super::LOCALE_EN_US)] {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let missing: Vec<&String> = want.iter().filter(|k| v.get(k.as_str()).is_none()).collect();
            assert!(missing.is_empty(), "{name} 语言包缺这些动态键：{missing:?}");
        }
        // 网关自己发的事件：网页 core.js 的 EV 表要有这些取值（网页所有事件分支都只认 EV 表）。
        use crate::events::{AREA_BOOKS, AREA_MANAGE, KIND_BATCH, KIND_SERVICES, TAB_BOOKS, TAB_MANAGE, TAB_NOTES, TAB_OTHER};
        let ev = &super::APP_JS[super::APP_JS.find("const EV={").expect("core.js 应定义 EV 表")..];
        let ev = &ev[..ev.find("};").unwrap()];
        for v in [AREA_BOOKS, AREA_MANAGE, KIND_BATCH, KIND_SERVICES] {
            assert!(ev.contains(&format!(":'{v}'")), "core.js 的 EV 表缺 {v}");
        }
        // 事件的 `tab`（FE-6）：网页按它找顶层 tab，EV.tab 要有网关会发的每一个取值（含 MODULES 里声明的）。
        let tabs = &ev[ev.find("tab:{").expect("core.js 的 EV 表应有 tab")..];
        let tabs = &tabs[..tabs.find('}').unwrap()];
        for v in [TAB_BOOKS, TAB_NOTES, TAB_OTHER, TAB_MANAGE].into_iter().chain(crate::manage::MODULES.iter().map(|m| m.tab)) {
            assert!(tabs.contains(&format!(":'{v}'")), "core.js 的 EV.tab 缺 {v}");
        }
        // 系统增强开关：网页 manage.js 的 TOGGLE_UI 要给 TOGGLES 里每个键配文案，否则那个开关在网页上不出现。
        for t in crate::enhance::TOGGLES {
            assert!(super::APP_JS.contains(&format!("\n  {}:{{panel:", t.key)), "manage.js 的 TOGGLE_UI 缺 {}", t.key);
        }
    }

    #[test]
    fn locale_json_falls_back_to_chinese_for_unknown_lang() {
        assert_eq!(super::locale_json("fr-FR"), super::LOCALE_ZH_CN);
        assert_eq!(super::locale_json("en-US"), super::LOCALE_EN_US);
        assert_eq!(super::locale_json("zh-CN"), super::LOCALE_ZH_CN);
    }

    #[test]
    fn password_page_minlength_matches_config_constant() {
        // 2026-09-09 审计修：改密码页的 minlength/提示文案该跟 config::MIN_PASSWORD_LEN 联动，
        // 不是各写各的硬编码——常量改了，这里必须跟着变，否则回归会漏掉这条一致性。
        let n = crate::config::MIN_PASSWORD_LEN;
        let p = super::password_page("", false);
        assert!(p.contains(&format!("minlength=\"{n}\"")), "页面里的 minlength 应该等于当前常量: {p}");
        assert!(p.contains(&format!("至少 {n} 位")), "提示文案也该带上当前常量: {p}");
    }
}
