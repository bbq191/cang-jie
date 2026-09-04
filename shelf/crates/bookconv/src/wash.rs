//! 清洗层（对标 host `wash_epub.sh` 的 Calibre 规则，2026-09-03 移植；白皮书 §03i）。作用于**解包后的条目表**，
//! 由 `optimize::optimize_epub_with` 在优化器各遍之前调用，host CLI `epub-optimize` 与设备 book-serve 走同一份。
//!
//! 规则（与 Calibre 参数一一对应）：
//! 1. 伪 DRM 剥离（= `strip_pseudo_drm.py`）：`META-INF/encryption.xml` 只列样式/字体/脚本 → 丢弃这些文件 +
//!    encryption.xml + OPF manifest 项；列了正文/图片/导航 = 真 DRM → **报错停下**。
//! 2. CSS 锁剥离（= `--filter-css font-family,font-size,color,background-color,text-align`）：独立 .css、`<style>`、
//!    `style=""` 三处都剥；补优化器 `strip_font_locks` 只剥内联字体锁的缺口。
//! 3. 边距归零（= `--margin-* 0`）：body/html/@page 的 margin/padding 删掉，并注入 `html,body{margin:0;padding:0}`。
//! 4. 段距归零 + 首行缩进（= `--remove-paragraph-spacing --remove-paragraph-spacing-indent-size 2`）：p/div 的
//!    上下 margin/padding 归零（左右保留：blockquote/列表缩进不伤），`p{text-indent:2em}`；`keep_para_spacing` 时
//!    只注缩进（= `WASH_KEEP_PARA_SPACING=1`）。注入块带 `!important` 兜住类选择器（`.calibre1{margin:1em 0}`）。
//! 5. 空页清理：正文无文字无图（Calibre MOBI 转出的 `mbppagebreak` 独占页）→ 从 spine/manifest/zip 删除，
//!    目录里指向它的条目改指下一篇。
//! 6. 自动目录（= `--use-auto-toc --level1-toc //h:h1 --level2-toc //h:h2`）：缺省**仅在书无目录时**从 h1/h2 生成
//!    `toc.ncx` + `nav.xhtml`（xochitl 两者都认）；`AutoToc::Always` 强制重建（原目录坏掉的书）。
//! 7. 单标签重复 `id=` 折叠（`collapse_dup_id_attrs`）：非法 XHTML 会让 xochitl 整章白屏，这里先修、质量门再拦。
//!
//! 全部规则幂等：注入块带 `class="cj-wash"` 标记，重复过不再叠加。
use crate::htmlproc::collapse_dup_id_attrs;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// zip 条目（目录项已剔除）。
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoToc {
    Off,
    /// 书无目录（nav/ncx 缺失或零条目）时生成。
    IfMissing,
    Always,
}

/// 正文排版语言（决定首行缩进/段落习惯）。`Auto` 由 `wash_entries` 按全书 CJK/拉丁字符占比判定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LangMode {
    Auto,
    /// 中文习惯：首行缩进 2em（两个全角字）、段间无空。
    Cjk,
    /// 拉丁习惯：首行缩进 1.2em、标题后首段不缩进。
    Latin,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WashOpts {
    /// 保留原书段间距（诗集/剧本靠空行分节）。
    pub keep_para_spacing: bool,
    pub auto_toc: AutoToc,
    /// 剥掉的 CSS 属性（小写）。缺省与 host `--filter-css` 一致。
    pub filter_props: Vec<String>,
    /// 正文排版语言（`Auto`=自动探测）。
    pub lang: LangMode,
}

impl Default for WashOpts {
    fn default() -> Self {
        WashOpts { keep_para_spacing: false, auto_toc: AutoToc::IfMissing, filter_props: DEFAULT_FILTER_PROPS.iter().map(|s| s.to_string()).collect(), lang: LangMode::Auto }
    }
}

// background / background-image：书常在 body/分卷页用 CSS 背景图（装饰纹样、分卷插画）。xochitl **无视
// no-repeat / background-size** → 把背景图**平铺**满页盖住正文（真机《飘》body.fen 的 `background:url() no-repeat`
// 被铺成多幅）。剥掉背景图声明即净页（章头 <img> 装饰不受影响，仍保留）。@font-face 的 src:url() 由 filter_css 豁免。
pub const DEFAULT_FILTER_PROPS: &[&str] = &["font-family", "font-size", "font", "color", "background-color", "background-image", "background", "text-align"];
/// 伪 DRM 允许加密的扩展名（= strip_pseudo_drm.py SAFE_EXTS）。
pub const PSEUDO_DRM_SAFE_EXTS: &[&str] = &[".css", ".ttf", ".otf", ".woff", ".woff2", ".js"];
pub const WASH_MARK: &str = "cj-wash";

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct WashReport {
    pub pseudo_drm_stripped: Vec<String>,
    pub css_files: usize,
    pub html_files: usize,
    pub empty_pages_removed: Vec<String>,
    pub toc_generated: usize,
    pub dup_id_tags_collapsed: usize,
}

pub fn is_html(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".xhtml") || l.ends_with(".html") || l.ends_with(".htm")
}

// ───────────────────────── 路径工具（zip 内 posix 路径） ─────────────────────────

pub fn posix_norm(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

pub fn dir_of(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}

pub fn resolve(base_dir: &str, rel: &str) -> String {
    if base_dir.is_empty() {
        posix_norm(rel)
    } else {
        posix_norm(&format!("{base_dir}/{rel}"))
    }
}

/// `target` 相对 `base_dir` 的路径（都是 zip 内绝对路径）。
pub fn relative_to(base_dir: &str, target: &str) -> String {
    let b: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    let t: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    let common = b.iter().zip(t.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = vec!["..".into(); b.len() - common];
    out.extend(t[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (b.get(i + 1), b.get(i + 2)) {
                if let Ok(v) = u8::from_str_radix(&format!("{}{}", *h as char, *l as char), 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn find_opf(entries: &[Entry]) -> Option<usize> {
    // container.xml 指向优先，否则第一个 .opf
    if let Some(c) = entries.iter().find(|e| e.name == "META-INF/container.xml") {
        let t = String::from_utf8_lossy(&c.data);
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r#"full-path="([^"]+)""#).unwrap());
        if let Some(m) = re.captures(&t) {
            let p = posix_norm(&m[1]);
            if let Some(i) = entries.iter().position(|e| e.name == p) {
                return Some(i);
            }
        }
    }
    entries.iter().position(|e| e.name.to_ascii_lowercase().ends_with(".opf"))
}

// ───────────────────────── 1. 伪 DRM ─────────────────────────

/// encryption.xml 里的加密目标（zip 内路径）。
pub fn encrypted_targets(entries: &[Entry]) -> Option<Vec<String>> {
    let enc = entries.iter().find(|e| e.name == "META-INF/encryption.xml")?;
    let t = String::from_utf8_lossy(&enc.data);
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"CipherReference\s+URI="([^"]+)""#).unwrap());
    Some(re.captures_iter(&t).map(|c| posix_norm(&percent_decode(&c[1]))).filter(|u| !u.starts_with('#')).collect())
}

/// 真 DRM 判据：加密了非样式/字体/脚本的文件。返回违规项。
pub fn real_drm_items(targets: &[String]) -> Vec<String> {
    targets.iter().filter(|t| { let l = t.to_ascii_lowercase(); !PSEUDO_DRM_SAFE_EXTS.iter().any(|e| l.ends_with(e)) }).cloned().collect()
}

fn strip_pseudo_drm(entries: &mut Vec<Entry>, rep: &mut WashReport) -> Result<(), String> {
    let Some(targets) = encrypted_targets(entries) else { return Ok(()) };
    let bad = real_drm_items(&targets);
    if !bad.is_empty() {
        return Err(format!("加密 EPUB（真 DRM，加密了 {} 等 {} 项），xochitl/KOReader 都读不了", bad.iter().take(3).cloned().collect::<Vec<_>>().join("、"), bad.len()));
    }
    let drop: HashSet<String> = targets.iter().cloned().chain(std::iter::once("META-INF/encryption.xml".to_string())).collect();
    if let Some(oi) = find_opf(entries) {
        let opf_dir = dir_of(&entries[oi].name).to_string();
        let mut text = String::from_utf8_lossy(&entries[oi].data).into_owned();
        for t in &targets {
            let rel = relative_to(&opf_dir, t);
            let re = Regex::new(&format!(r#"<item\b[^>]*\bhref="{}"[^>]*/>\s*"#, regex::escape(&rel))).unwrap();
            text = re.replace_all(&text, "").into_owned();
        }
        entries[oi].data = text.into_bytes();
    }
    entries.retain(|e| !drop.contains(&e.name));
    rep.pseudo_drm_stripped = targets;
    Ok(())
}

// ───────────────────────── 2–4. CSS 声明处理 ─────────────────────────

fn decl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // 属性名 : 值（值里的 HTML 实体 `&#39;` 含分号，按实体整体吃）
    RE.get_or_init(|| Regex::new(r#"(?i)([-a-zA-Z]+)\s*:\s*((?:&#?\w+;|[^;])*);?"#).unwrap())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Spacing {
    /// 不动 margin/padding。
    Keep,
    /// 上下归零、左右保留（p/div）。
    Vertical,
    /// 全部删除（body/html/@page）。
    All,
}

/// 对一段声明文本：剥 `filter` 里的属性；按 `spacing` 处理 margin/padding。
fn filter_decls(decls: &str, filter: &[String], spacing: Spacing) -> String {
    let mut out: Vec<String> = Vec::new();
    for c in decl_re().captures_iter(decls) {
        let prop = c[1].to_ascii_lowercase();
        let val = c[2].trim();
        if filter.iter().any(|f| *f == prop) {
            continue;
        }
        let is_box = prop == "margin" || prop == "padding";
        let is_box_side = prop.starts_with("margin-") || prop.starts_with("padding-");
        match spacing {
            Spacing::Keep => {}
            Spacing::All if is_box || is_box_side => continue,
            Spacing::Vertical if prop.ends_with("-top") || prop.ends_with("-bottom") => {
                if is_box_side {
                    continue;
                }
            }
            Spacing::Vertical if is_box => {
                // 简写：保留左右
                let parts: Vec<&str> = val.split_whitespace().filter(|p| !p.starts_with('!')).collect();
                let important = if val.contains("!important") { " !important" } else { "" };
                let (r, l) = match parts.len() {
                    1 => (parts[0], parts[0]),
                    2 | 3 => (parts[1], parts[1]),
                    4 => (parts[1], parts[3]),
                    _ => continue,
                };
                out.push(if r == l { format!("{prop}:0 {r}{important}") } else { format!("{prop}:0 {r} 0 {l}{important}") });
                continue;
            }
            _ => {}
        }
        out.push(format!("{prop}:{val}"));
    }
    out.join(";")
}

fn selector_spacing(selector: &str) -> Spacing {
    static ELEM_P: OnceLock<Regex> = OnceLock::new();
    static ELEM_BODY: OnceLock<Regex> = OnceLock::new();
    let p = ELEM_P.get_or_init(|| Regex::new(r#"(?i)(^|[\s,>+~])(p|div)(?:[\s,.#:\[]|$)"#).unwrap());
    let b = ELEM_BODY.get_or_init(|| Regex::new(r#"(?i)(^|[\s,>+~])(body|html)(?:[\s,.#:\[]|$)|^@page\b"#).unwrap());
    let s = selector.trim();
    if b.is_match(s) {
        Spacing::All
    } else if p.is_match(s) {
        Spacing::Vertical
    } else {
        Spacing::Keep
    }
}

/// 整段 CSS（文件或 <style> 内容）：逐规则剥锁 + 边距处理。`@media{}` 嵌套靠"从内向外"匹配最内层规则。
pub fn filter_css(css: &str, opts: &WashOpts) -> String {
    static RULE: OnceLock<Regex> = OnceLock::new();
    let rule = RULE.get_or_init(|| Regex::new(r#"(?s)([^{}]+)\{([^{}]*)\}"#).unwrap());
    rule.replace_all(css, |c: &regex::Captures| {
        let sel = &c[1];
        let trimmed = sel.trim_start();
        if trimmed.starts_with("@font-face") || trimmed.starts_with("@import") {
            return c[0].to_string();
        }
        let spacing = match selector_spacing(sel) {
            Spacing::Vertical if opts.keep_para_spacing => Spacing::Keep,
            s => s,
        };
        format!("{}{{{}}}", sel, filter_decls(&c[2], &opts.filter_props, spacing))
    }).into_owned()
}

fn style_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<([a-z][a-z0-9]*)\b([^>]*?)\sstyle="([^"]*)"([^>]*)>"#).unwrap())
}

/// (x)html：`style=""`（按标签名定边距策略）+ `<style>` 块剥锁；注入清洗样式块；折叠重复 id。
pub fn wash_html(html: &str, opts: &WashOpts) -> (String, usize) {
    let before_dup = count_dup_id_tags(html);
    let s = collapse_dup_id_attrs(html);
    let s = style_attr_re().replace_all(&s, |c: &regex::Captures| {
        let tag = c[1].to_ascii_lowercase();
        let spacing = match tag.as_str() {
            "body" | "html" => Spacing::All,
            "p" | "div" if !opts.keep_para_spacing => Spacing::Vertical,
            _ => Spacing::Keep,
        };
        let cleaned = filter_decls(&c[3], &opts.filter_props, spacing);
        if cleaned.is_empty() {
            format!("<{}{}{}>", &c[1], &c[2], &c[4])
        } else {
            format!("<{}{} style=\"{}\"{}>", &c[1], &c[2], cleaned, &c[4])
        }
    }).into_owned();
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    let block = BLOCK.get_or_init(|| Regex::new(r#"(?is)(<style\b[^>]*>)(.*?)(</style>)"#).unwrap());
    let s = block.replace_all(&s, |c: &regex::Captures| {
        if c[1].contains(WASH_MARK) {
            c[0].to_string()
        } else {
            format!("{}{}{}", &c[1], filter_css(&c[2], opts), &c[3])
        }
    }).into_owned();
    (inject_style(&s, opts), before_dup)
}

/// 清洗样式块（注入 `</head>` 前；无 head 则 `<body` 前）。按 `opts.lang` 注中/英排版习惯。
/// 注意：`opts.lang` 应已被 `wash_entries` 从 `Auto` 解析为 `Cjk`/`Latin`（此处把 `Auto` 兜底当 `Cjk`）。
pub fn wash_css(opts: &WashOpts) -> String {
    let mut css = String::from("html,body{margin:0!important;padding:0!important}@page{margin:0}");
    if !opts.keep_para_spacing {
        css.push_str("p,div{margin-top:0!important;margin-bottom:0!important;padding-top:0!important;padding-bottom:0!important}");
    }
    match opts.lang {
        LangMode::Latin => {
            // 拉丁：首行缩进小一些（2em 对拉丁偏大）、标题/分隔后的首段不缩进（若 xochitl 认相邻选择器则生效，不认亦无害）
            css.push_str("p{text-indent:1.2em!important}");
            css.push_str("h1+p,h2+p,h3+p,h4+p,h5+p,h6+p,hr+p,blockquote+p{text-indent:0!important}");
        }
        // Cjk 与 Auto 兜底：中文习惯 2em
        _ => css.push_str("p{text-indent:2em!important}"),
    }
    css
}

fn inject_style(html: &str, opts: &WashOpts) -> String {
    if html.contains(&format!("class=\"{WASH_MARK}\"")) {
        return html.to_string();
    }
    let block = format!("<style class=\"{WASH_MARK}\" type=\"text/css\">{}</style>", wash_css(opts));
    if let Some(i) = html.find("</head>") {
        format!("{}{}{}", &html[..i], block, &html[i..])
    } else if let Some(i) = html.find("<body") {
        format!("{}<head>{}</head>{}", &html[..i], block, &html[i..])
    } else {
        html.to_string()
    }
}

pub fn count_dup_id_tags(html: &str) -> usize {
    static TAG: OnceLock<Regex> = OnceLock::new();
    static ID: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r#"(?s)<[a-zA-Z][^>]*>"#).unwrap());
    let id = ID.get_or_init(|| Regex::new(r#"\bid=""#).unwrap());
    tag.find_iter(html).filter(|m| id.find_iter(m.as_str()).count() > 1).count()
}

// ───────────────────────── OPF 视图 ─────────────────────────

struct Opf {
    index: usize,
    dir: String,
    /// manifest id → zip 路径
    items: HashMap<String, String>,
    /// spine 顺序的 zip 路径
    spine: Vec<String>,
    nav_doc: Option<String>,
    ncx: Option<String>,
}

fn parse_opf(entries: &[Entry]) -> Option<Opf> {
    let index = find_opf(entries)?;
    let dir = dir_of(&entries[index].name).to_string();
    let text = String::from_utf8_lossy(&entries[index].data);
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static ATTR: OnceLock<Regex> = OnceLock::new();
    static REF: OnceLock<Regex> = OnceLock::new();
    let item = ITEM.get_or_init(|| Regex::new(r#"(?s)<item\b([^>]*)/?>"#).unwrap());
    let attr = ATTR.get_or_init(|| Regex::new(r#"([a-zA-Z:-]+)\s*=\s*"([^"]*)""#).unwrap());
    let iref = REF.get_or_init(|| Regex::new(r#"<itemref\b[^>]*\bidref="([^"]+)""#).unwrap());
    let mut items = HashMap::new();
    let mut nav_doc = None;
    let mut ncx = None;
    for c in item.captures_iter(&text) {
        let attrs: HashMap<String, String> = attr.captures_iter(&c[1]).map(|a| (a[1].to_ascii_lowercase(), a[2].to_string())).collect();
        let (Some(id), Some(href)) = (attrs.get("id"), attrs.get("href")) else { continue };
        let path = resolve(&dir, &percent_decode(href));
        if attrs.get("properties").map(|p| p.split_whitespace().any(|x| x == "nav")).unwrap_or(false) {
            nav_doc = Some(path.clone());
        }
        if attrs.get("media-type").map(|m| m.contains("dtbncx")).unwrap_or(false) {
            ncx = Some(path.clone());
        }
        items.insert(id.clone(), path);
    }
    let spine: Vec<String> = iref.captures_iter(&text).filter_map(|c| items.get(&c[1]).cloned()).collect();
    Some(Opf { index, dir, items, spine, nav_doc, ncx })
}

// ───────────────────────── 5. 空页清理 ─────────────────────────

fn is_empty_page(html: &str) -> bool {
    static BODY: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    let body = BODY.get_or_init(|| Regex::new(r#"(?is)<body\b[^>]*>(.*?)</body>"#).unwrap());
    let tag = TAG.get_or_init(|| Regex::new(r#"(?s)<[^>]*>"#).unwrap());
    let inner = body.captures(html).map(|c| c[1].to_string()).unwrap_or_default();
    let low = inner.to_ascii_lowercase();
    if low.contains("<img") || low.contains("<svg") || low.contains("<image") || low.contains("<video") || low.contains("<audio") {
        return false;
    }
    let text = tag.replace_all(&inner, "");
    let text = text.replace("&nbsp;", " ").replace("&#160;", " ").replace('\u{a0}', " ");
    text.trim().is_empty()
}

fn remove_empty_pages(entries: &mut Vec<Entry>, rep: &mut WashReport) {
    let Some(opf) = parse_opf(entries) else { return };
    let mut removed: Vec<String> = Vec::new();
    for p in &opf.spine {
        if Some(p) == opf.nav_doc.as_ref() {
            continue;
        }
        if let Some(e) = entries.iter().find(|e| &e.name == p) {
            if is_html(&e.name) && is_empty_page(&String::from_utf8_lossy(&e.data)) {
                removed.push(p.clone());
            }
        }
    }
    if removed.is_empty() || removed.len() >= opf.spine.len() {
        return; // 全空不动（别把书删没）
    }
    let removed_set: HashSet<&String> = removed.iter().collect();
    // 替换目标：spine 里下一篇未删的，没有则上一篇
    let replacement = |p: &String| -> Option<String> {
        let i = opf.spine.iter().position(|x| x == p)?;
        opf.spine[i + 1..].iter().chain(opf.spine[..i].iter().rev()).find(|x| !removed_set.contains(x)).cloned()
    };
    let repl: HashMap<String, String> = removed.iter().filter_map(|p| replacement(p).map(|r| (p.clone(), r))).collect();
    // OPF：删 itemref + item
    let ids: Vec<String> = opf.items.iter().filter(|(_, v)| removed_set.contains(v)).map(|(k, _)| k.clone()).collect();
    let mut text = String::from_utf8_lossy(&entries[opf.index].data).into_owned();
    for id in &ids {
        let re = Regex::new(&format!(r#"<itemref\b[^>]*\bidref="{}"[^>]*/?>(?:\s*</itemref>)?\s*"#, regex::escape(id))).unwrap();
        text = re.replace_all(&text, "").into_owned();
        let re = Regex::new(&format!(r#"<item\b[^>]*\bid="{}"[^>]*/?>(?:\s*</item>)?\s*"#, regex::escape(id))).unwrap();
        text = re.replace_all(&text, "").into_owned();
    }
    entries[opf.index].data = text.into_bytes();
    // 目录（ncx/nav）里指向被删页的引用 → 改指替换页
    let toc_files: Vec<String> = entries.iter().filter(|e| is_toc_file(&e.name)).map(|e| e.name.clone()).collect();
    for tf in toc_files {
        let tdir = dir_of(&tf).to_string();
        let Some(e) = entries.iter_mut().find(|e| e.name == tf) else { continue };
        let text = String::from_utf8_lossy(&e.data).into_owned();
        let new = href_re().replace_all(&text, |c: &regex::Captures| {
            let target = resolve(&tdir, &percent_decode(&c[2]));
            match repl.get(&target) {
                Some(r) => format!("{}=\"{}\"", &c[1], relative_to(&tdir, r)),
                None => c[0].to_string(),
            }
        }).into_owned();
        e.data = new.into_bytes();
    }
    entries.retain(|e| !removed_set.contains(&e.name));
    rep.empty_pages_removed = removed;
}

pub fn is_toc_file(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    let base = l.rsplit('/').next().unwrap_or(&l);
    l.ends_with(".ncx") || (base.starts_with("nav") && (base.ends_with(".xhtml") || base.ends_with(".html")))
}

pub fn href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r##"(src|href)="([^"#]+)(#[^"]*)?""##).unwrap())
}

// ───────────────────────── 6. 自动目录 ─────────────────────────

/// 目录条目数（ncx `src` + nav `href`，排除 toc 文件自指与非 html 目标）。
pub fn toc_entry_count(entries: &[Entry]) -> usize {
    let mut n = 0;
    for e in entries.iter().filter(|e| is_toc_file(&e.name)) {
        let t = String::from_utf8_lossy(&e.data);
        n += href_re().captures_iter(&t).filter(|c| { let l = c[2].to_ascii_lowercase(); !l.starts_with("http") && (l.ends_with(".xhtml") || l.ends_with(".html") || l.ends_with(".htm")) }).count();
    }
    n
}

fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // h1–h6 都收（书只用 h3 当章标题也不漏目录）；多级由 build_ncx/build_nav 按 dense-rank 嵌套。
    RE.get_or_init(|| Regex::new(r#"(?is)<h([1-6])\b([^>]*)>(.*?)</h[1-6]>"#).unwrap())
}

/// 把出现过的 h 级别稠密化为连续深度 1..N（如书用 {h1,h3} → 各条 rank 1/2），供嵌套用。
fn dense_ranks(items: &[(u8, String, String, String)]) -> Vec<u8> {
    let mut levels: Vec<u8> = items.iter().map(|i| i.0).collect();
    levels.sort_unstable();
    levels.dedup();
    items.iter().map(|i| (levels.iter().position(|&l| l == i.0).unwrap_or(0) as u8) + 1).collect()
}

fn plain_text(html: &str) -> String {
    static TAG: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r#"(?s)<[^>]*>"#).unwrap());
    let t = tag.replace_all(html, "");
    t.replace("&nbsp;", " ").replace("&#160;", " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 从 spine 各章 h1/h2 生成目录；标题无 id 则补 `id="cj-toc-N"`。返回条目 (level, title, zip路径, frag)。
fn collect_headings(entries: &mut [Entry], spine: &[String], nav_doc: Option<&String>) -> Vec<(u8, String, String, String)> {
    let mut out = Vec::new();
    let mut counter = 0usize;
    static ID: OnceLock<Regex> = OnceLock::new();
    let id_re = ID.get_or_init(|| Regex::new(r#"\bid="([^"]*)""#).unwrap());
    for p in spine {
        if Some(p) == nav_doc {
            continue;
        }
        let Some(e) = entries.iter_mut().find(|e| &e.name == p) else { continue };
        let html = String::from_utf8_lossy(&e.data).into_owned();
        let mut changed = false;
        let new = heading_re().replace_all(&html, |c: &regex::Captures| {
            let level: u8 = c[1].parse().unwrap_or(1);
            let title = plain_text(&c[3]);
            if title.is_empty() {
                return c[0].to_string();
            }
            let attrs = c[2].to_string();
            let (attrs, frag) = match id_re.captures(&attrs) {
                Some(m) => (attrs.clone(), m[1].to_string()),
                None => {
                    counter += 1;
                    changed = true;
                    let f = format!("cj-toc-{counter}");
                    (format!("{attrs} id=\"{f}\""), f)
                }
            };
            out.push((level, title, p.clone(), frag));
            format!("<h{}{}>{}</h{}>", &c[1], attrs, &c[3], &c[1])
        }).into_owned();
        if changed {
            e.data = new.into_bytes();
        }
    }
    out
}

fn build_ncx(items: &[(u8, String, String, String)], ncx_dir: &str, title: &str) -> String {
    let ranks = dense_ranks(items);
    let depth_max = ranks.iter().copied().max().unwrap_or(1);
    let mut s = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="cj-wash"/><meta name="dtb:depth" content="{depth_max}"/></head><docTitle><text>{}</text></docTitle><navMap>"#, xml_escape(title));
    let mut depth = 0u8; // 当前打开的 navPoint 层数
    for (i, (_, t, path, frag)) in items.iter().enumerate() {
        let d = ranks[i].min(depth + 1); // 钳制：不跳跃深入 >1 层，保证良构
        if d <= depth {
            for _ in 0..(depth - d + 1) {
                s.push_str("</navPoint>");
            }
        }
        let href = format!("{}#{}", relative_to(ncx_dir, path), frag);
        s.push_str(&format!(r#"<navPoint id="np{}" playOrder="{}"><navLabel><text>{}</text></navLabel><content src="{}"/>"#, i + 1, i + 1, xml_escape(t), xml_escape(&href)));
        depth = d;
    }
    for _ in 0..depth {
        s.push_str("</navPoint>");
    }
    s.push_str("</navMap></ncx>");
    s
}

fn build_nav(items: &[(u8, String, String, String)], nav_dir: &str) -> String {
    let ranks = dense_ranks(items);
    let mut s = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>目录</title></head><body><nav epub:type="toc" id="toc"><h1>目录</h1>"#);
    let mut depth = 0u8; // 当前打开的 <ol> 层数
    for (i, (_, t, path, frag)) in items.iter().enumerate() {
        let d = ranks[i].min(depth + 1);
        if d > depth {
            for _ in depth..d {
                s.push_str("<ol>");
            }
        } else {
            for _ in d..depth {
                s.push_str("</li></ol>");
            }
            s.push_str("</li>");
        }
        depth = d;
        let href = format!("{}#{}", relative_to(nav_dir, path), frag);
        s.push_str(&format!(r#"<li><a href="{}">{}</a>"#, xml_escape(&href), xml_escape(t)));
    }
    for _ in 0..depth {
        s.push_str("</li></ol>");
    }
    s.push_str("</nav></body></html>");
    s
}

fn auto_toc(entries: &mut Vec<Entry>, mode: AutoToc, rep: &mut WashReport) {
    if mode == AutoToc::Off || (mode == AutoToc::IfMissing && toc_entry_count(entries) > 0) {
        return;
    }
    let Some(opf) = parse_opf(entries) else { return };
    let headings = collect_headings(entries, &opf.spine, opf.nav_doc.as_ref());
    if headings.is_empty() {
        return;
    }
    let title = {
        let t = String::from_utf8_lossy(&entries[opf.index].data);
        static T: OnceLock<Regex> = OnceLock::new();
        T.get_or_init(|| Regex::new(r#"(?s)<dc:title[^>]*>(.*?)</dc:title>"#).unwrap()).captures(&t).map(|c| plain_text(&c[1])).unwrap_or_else(|| "目录".into())
    };
    let ncx_path = opf.ncx.clone().unwrap_or_else(|| resolve(&opf.dir, "toc.ncx"));
    let nav_path = opf.nav_doc.clone().unwrap_or_else(|| resolve(&opf.dir, "nav.xhtml"));
    let ncx = build_ncx(&headings, dir_of(&ncx_path), &title).into_bytes();
    let nav = build_nav(&headings, dir_of(&nav_path)).into_bytes();
    let mut text = String::from_utf8_lossy(&entries[opf.index].data).into_owned();
    if opf.ncx.is_none() {
        text = text.replacen("</manifest>", &format!(r#"<item id="cj-ncx" href="{}" media-type="application/x-dtbncx+xml"/></manifest>"#, relative_to(&opf.dir, &ncx_path)), 1);
        static SPINE: OnceLock<Regex> = OnceLock::new();
        let sp = SPINE.get_or_init(|| Regex::new(r#"<spine\b([^>]*)>"#).unwrap());
        text = sp.replace(&text, |c: &regex::Captures| {
            let attrs = c[1].to_string();
            if attrs.contains("toc=") {
                format!("<spine{attrs}>")
            } else {
                format!("<spine{attrs} toc=\"cj-ncx\">")
            }
        }).into_owned();
    }
    if opf.nav_doc.is_none() {
        text = text.replacen("</manifest>", &format!(r#"<item id="cj-nav" href="{}" media-type="application/xhtml+xml" properties="nav"/></manifest>"#, relative_to(&opf.dir, &nav_path)), 1);
    }
    entries[opf.index].data = text.into_bytes();
    for (path, data) in [(ncx_path, ncx), (nav_path, nav)] {
        match entries.iter_mut().find(|e| e.name == path) {
            Some(e) => e.data = data,
            None => entries.push(Entry { name: path, data }),
        }
    }
    rep.toc_generated = headings.len();
}

// ───────────────────────── 入口 ─────────────────────────

/// 全书 CJK vs 拉丁字符占比 → 主语言（Han 字数 ≥ 拉丁字母数 = Cjk）。扫全部 html 正文，早停够量即定。
fn detect_dominant_script(entries: &[Entry]) -> LangMode {
    let (mut han, mut latin) = (0u64, 0u64);
    for e in entries.iter().filter(|e| is_html(&e.name) && !is_toc_file(&e.name)) {
        let Ok(t) = std::str::from_utf8(&e.data) else { continue };
        for ch in plain_text(t).chars() {
            if matches!(ch, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}') {
                han += 1;
            } else if ch.is_ascii_alphabetic() {
                latin += 1;
            }
        }
        if han + latin > 20_000 {
            break; // 够量即判，不必扫全书
        }
    }
    if han >= latin {
        LangMode::Cjk
    } else {
        LangMode::Latin
    }
}

/// 对条目表就地清洗。真 DRM 返回 Err（调用方应整体失败、原样不动）。
pub fn wash_entries(entries: &mut Vec<Entry>, opts: &WashOpts) -> Result<WashReport, String> {
    let mut rep = WashReport::default();
    strip_pseudo_drm(entries, &mut rep)?;
    remove_empty_pages(entries, &mut rep);
    // Auto → 探测主语言，解析成具体 Cjk/Latin 再逐文件注排版（探测在剥空页之后、注样式之前）。
    let opts = if opts.lang == LangMode::Auto {
        let mut o = opts.clone();
        o.lang = detect_dominant_script(entries);
        o
    } else {
        opts.clone()
    };
    let opts = &opts;
    for e in entries.iter_mut() {
        let l = e.name.to_ascii_lowercase();
        if l.ends_with(".css") {
            if let Ok(t) = std::str::from_utf8(&e.data) {
                e.data = filter_css(t, opts).into_bytes();
                rep.css_files += 1;
            }
        } else if is_html(&e.name) && !is_toc_file(&e.name) {
            if let Ok(t) = std::str::from_utf8(&e.data) {
                let (out, dups) = wash_html(t, opts);
                rep.dup_id_tags_collapsed += dups;
                e.data = out.into_bytes();
                rep.html_files += 1;
            }
        }
    }
    auto_toc(entries, opts.auto_toc, &mut rep);
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, data: &str) -> Entry {
        Entry { name: name.into(), data: data.as_bytes().to_vec() }
    }
    fn s(entries: &[Entry], name: &str) -> String {
        String::from_utf8(entries.iter().find(|e| e.name == name).unwrap().data.clone()).unwrap()
    }
    const OPF: &str = r#"<?xml version="1.0"?><package version="2.0"><metadata><dc:title>测试书</dc:title></metadata><manifest><item id="css" href="style.css" media-type="text/css"/><item id="dk" href="dkagent.css" media-type="text/css"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="pb" href="pb.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="pb"/><itemref idref="c2"/></spine></package>"#;

    #[test]
    fn paths() {
        assert_eq!(posix_norm("OEBPS/../a/./b"), "a/b");
        assert_eq!(resolve("OEBPS/text", "../style.css"), "OEBPS/style.css");
        assert_eq!(relative_to("OEBPS", "OEBPS/text/c1.xhtml"), "text/c1.xhtml");
        assert_eq!(relative_to("OEBPS/text", "OEBPS/style.css"), "../style.css");
        assert_eq!(relative_to("", "a.xhtml"), "a.xhtml");
        assert_eq!(percent_decode("%E5%AD%97.xhtml"), "字.xhtml");
    }

    #[test]
    fn decl_filter_and_spacing() {
        let f: Vec<String> = DEFAULT_FILTER_PROPS.iter().map(|s| s.to_string()).collect();
        assert_eq!(filter_decls("font-family:'A';color:#333;text-indent:1em;font:12px x", &f, Spacing::Keep), "text-indent:1em");
        assert_eq!(filter_decls("margin:1em 2em;padding-top:3px;padding-left:4px", &f, Spacing::Vertical), "margin:0 2em;padding-left:4px");
        assert_eq!(filter_decls("margin:1em 2em 3em 4em", &f, Spacing::Vertical), "margin:0 2em 0 4em");
        assert_eq!(filter_decls("margin:5pt", &f, Spacing::Vertical), "margin:0 5pt");
        assert_eq!(filter_decls("margin:5pt;padding:2px;line-height:1.5", &f, Spacing::All), "line-height:1.5");
        assert_eq!(filter_decls("font-family: &#39;A&#39;; text-indent:2em", &f, Spacing::Keep), "text-indent:2em", "实体分号不截断");
        assert_eq!(selector_spacing("p.calibre1"), Spacing::Vertical);
        assert_eq!(selector_spacing("div > p"), Spacing::Vertical);
        assert_eq!(selector_spacing("body"), Spacing::All);
        assert_eq!(selector_spacing("@page"), Spacing::All);
        assert_eq!(selector_spacing(".calibre1"), Spacing::Keep);
        assert_eq!(selector_spacing("span, pre"), Spacing::Keep);
    }

    #[test]
    fn css_file_filter_keeps_font_face_and_media() {
        let o = WashOpts::default();
        let css = "@font-face{font-family:X;src:url(x.ttf)} body{margin:5pt;color:#333} p{margin:1em 0;text-align:justify;text-indent:0} @media print{ p{margin-top:2em} } .c{margin:1em}";
        let out = filter_css(css, &o);
        assert!(out.contains("@font-face{font-family:X;src:url(x.ttf)}"), "{out}");
        assert!(out.contains("body{}"), "{out}");
        assert!(out.contains(" p{margin:0 0;text-indent:0}"), "{out}");
        assert!(out.contains("p{}"), "media 内规则也处理: {out}");
        assert!(out.contains(".c{margin:1em}"), "类选择器不动: {out}");
        let k = WashOpts { keep_para_spacing: true, ..Default::default() };
        assert!(filter_css("p{margin:1em 0}", &k).contains("p{margin:1em 0}"));
    }

    #[test]
    fn strips_background_image_keeps_font_src() {
        let o = WashOpts::default();
        // 分卷页背景图（xochitl 平铺盖正文）应剥；@font-face 的 src:url 保留。
        let css = "@font-face{font-family:F;src:url(f.ttf)} body.fen{background:url(bg.png) no-repeat bottom center;background-size:100% auto;margin:0} .x{background-image:url(y.png);color:#333;text-indent:2em}";
        let out = filter_css(css, &o);
        assert!(!out.contains("bg.png") && !out.contains("y.png"), "背景图应剥: {out}");
        assert!(out.contains("f.ttf"), "@font-face src 保留: {out}");
        assert!(out.contains("text-indent:2em"), "非背景声明保留: {out}");
    }

    #[test]
    fn html_wash_injects_once_and_collapses_dup_ids() {
        let o = WashOpts::default();
        let html = r#"<html><head><link rel="stylesheet" href="s.css"/></head><body style="margin:5pt"><p id="a" id="b" style="font-size:12px;margin-top:1em;margin-left:2em">x</p><div style="color:gray">y</div><style>p{color:#333;margin:1em}</style></body></html>"#;
        let (out, dups) = wash_html(html, &o);
        assert_eq!(dups, 1);
        assert!(out.contains(r#"<p id="a" style="margin-left:2em">x</p>"#), "{out}");
        assert!(out.contains("<div>y</div>"), "空 style 整个删: {out}");
        assert!(out.contains("<body>"), "{out}");
        assert!(out.contains("<style>p{margin:0 1em}</style>"), "1em 四边→上下归零左右保留: {out}");
        assert!(out.contains(&format!(r#"<style class="{WASH_MARK}""#)), "{out}");
        assert!(out.contains("text-indent:2em!important"), "{out}");
        let (again, _) = wash_html(&out, &o);
        assert_eq!(again.matches(WASH_MARK).count(), 1, "幂等");
        // 无 head
        let (nh, _) = wash_html("<html><body><p>z</p></body></html>", &o);
        assert!(nh.starts_with("<html><head><style"), "{nh}");
    }

    #[test]
    fn pseudo_drm_stripped_and_real_drm_rejected() {
        let mut v = vec![
            e("mimetype", "application/epub+zip"),
            e("META-INF/encryption.xml", r#"<encryption><EncryptedData><CipherData><CipherReference URI="dkagent.css"/></CipherData></EncryptedData></encryption>"#),
            e("content.opf", OPF),
            e("dkagent.css", "secret"),
            e("style.css", "p{}"),
            e("c1.xhtml", "<html><body><p>a</p></body></html>"),
            e("pb.xhtml", "<html><body><p>b</p></body></html>"),
            e("c2.xhtml", "<html><body><p>c</p></body></html>"),
        ];
        let rep = wash_entries(&mut v, &WashOpts::default()).unwrap();
        assert_eq!(rep.pseudo_drm_stripped, vec!["dkagent.css"]);
        assert!(!v.iter().any(|x| x.name == "dkagent.css" || x.name == "META-INF/encryption.xml"));
        assert!(!s(&v, "content.opf").contains("dkagent.css"), "manifest 项删除");
        let mut real = vec![e("META-INF/encryption.xml", r#"<CipherReference URI="OEBPS/c1.xhtml"/><CipherReference URI="a.ttf"/>"#), e("OEBPS/c1.xhtml", "")];
        let err = wash_entries(&mut real, &WashOpts::default()).unwrap_err();
        assert!(err.contains("真 DRM") && err.contains("OEBPS/c1.xhtml"), "{err}");
    }

    #[test]
    fn empty_page_removed_and_toc_retargeted() {
        let mut v = vec![
            e("content.opf", OPF),
            e("style.css", ""),
            e("dkagent.css", ""),
            e("toc.ncx", r#"<ncx><navMap><navPoint><content src="c1.xhtml"/></navPoint><navPoint><content src="pb.xhtml#x"/></navPoint></navMap></ncx>"#),
            e("c1.xhtml", "<html><body><p>a</p></body></html>"),
            e("pb.xhtml", r#"<html><body><div class="mbppagebreak"></div>&nbsp;</body></html>"#),
            e("c2.xhtml", "<html><body><p>c</p></body></html>"),
        ];
        let rep = wash_entries(&mut v, &WashOpts { auto_toc: AutoToc::Off, ..Default::default() }).unwrap();
        assert_eq!(rep.empty_pages_removed, vec!["pb.xhtml"]);
        assert!(!v.iter().any(|x| x.name == "pb.xhtml"));
        let opf = s(&v, "content.opf");
        assert!(!opf.contains(r#"idref="pb""#) && !opf.contains(r#"id="pb""#), "{opf}");
        assert!(s(&v, "toc.ncx").contains(r#"src="c2.xhtml""#), "指向空页的目录改指下一篇: {}", s(&v, "toc.ncx"));
        // 有图的页不算空
        assert!(!is_empty_page(r#"<html><body><img src="a.png"/></body></html>"#));
    }

    #[test]
    fn auto_toc_generated_only_when_missing() {
        let mk = || vec![
            e("OEBPS/content.opf", r#"<package version="3.0"><metadata><dc:title>书</dc:title></metadata><manifest><item id="c1" href="text/c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/c2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#),
            e("OEBPS/text/c1.xhtml", "<html><body><h1>第一章</h1><p>a</p><h2 id=\"s1\">一节</h2></body></html>"),
            e("OEBPS/text/c2.xhtml", "<html><body><h1>第<i>二</i>章</h1><p>b</p></body></html>"),
        ];
        let mut v = mk();
        let rep = wash_entries(&mut v, &WashOpts::default()).unwrap();
        assert_eq!(rep.toc_generated, 3);
        let ncx = s(&v, "OEBPS/toc.ncx");
        assert!(ncx.contains(r#"src="text/c1.xhtml#cj-toc-1""#) && ncx.contains(r#"src="text/c1.xhtml#s1""#) && ncx.contains("第二章"), "{ncx}");
        let nav = s(&v, "OEBPS/nav.xhtml");
        assert!(nav.contains(r#"<li><a href="text/c1.xhtml#cj-toc-1">第一章</a><ol><li><a href="text/c1.xhtml#s1">一节</a></li></ol></li>"#), "{nav}");
        let opf = s(&v, "OEBPS/content.opf");
        assert!(opf.contains(r#"toc="cj-ncx""#) && opf.contains(r#"properties="nav""#), "{opf}");
        assert!(s(&v, "OEBPS/text/c1.xhtml").contains(r#"<h1 id="cj-toc-1">"#));
        assert_eq!(toc_entry_count(&v), 6, "ncx 3 + nav 3");
        // 已有目录 → IfMissing 不动
        let mut w = mk();
        w.push(e("OEBPS/toc.ncx", r#"<ncx><navMap><navPoint><content src="text/c1.xhtml"/></navPoint></navMap></ncx>"#));
        let rep = wash_entries(&mut w, &WashOpts::default()).unwrap();
        assert_eq!(rep.toc_generated, 0);
        assert!(!w.iter().any(|x| x.name == "OEBPS/nav.xhtml"));
    }

    #[test]
    fn lang_aware_indent() {
        let cjk = wash_css(&WashOpts { lang: LangMode::Cjk, ..Default::default() });
        assert!(cjk.contains("text-indent:2em!important") && !cjk.contains("1.2em"));
        let lat = wash_css(&WashOpts { lang: LangMode::Latin, ..Default::default() });
        assert!(lat.contains("text-indent:1.2em!important") && lat.contains("h1+p") && lat.contains("text-indent:0!important"));
    }

    #[test]
    fn detect_script_and_apply_to_wash() {
        let cjk = vec![e("c.xhtml", "<html><body><p>这是一本中文书籍需要两字缩进的测试内容足够多的汉字</p></body></html>")];
        assert_eq!(detect_dominant_script(&cjk), LangMode::Cjk);
        let en = vec![e("c.xhtml", "<html><body><p>This is an English book with plenty of latin letters here indeed</p></body></html>")];
        assert_eq!(detect_dominant_script(&en), LangMode::Latin);
        // 端到端：英文书 wash 后注入拉丁缩进
        let mut v = vec![
            e("content.opf", r#"<package version="3.0"><metadata><dc:title>B</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#),
            e("c1.xhtml", "<html><head></head><body><h2>Chapter One</h2><p>English prose flowing across the page with many words indeed here</p></body></html>"),
        ];
        wash_entries(&mut v, &WashOpts::default()).unwrap();
        assert!(s(&v, "c1.xhtml").contains("text-indent:1.2em!important"), "英文书应注拉丁缩进");
    }

    #[test]
    fn auto_toc_from_h3_and_deep_nesting() {
        // 只用 h3 当章标题：旧 h1/h2 正则会漏，现在应生成目录
        let mut v = vec![
            e("content.opf", r#"<package version="3.0"><metadata><dc:title>书</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#),
            e("c1.xhtml", "<html><body><h3>章一</h3><p>a</p><h3>章二</h3></body></html>"),
        ];
        assert_eq!(wash_entries(&mut v, &WashOpts::default()).unwrap().toc_generated, 2, "h3 也进目录");
        // 多级嵌套 h1>h2>h3>h1
        let items = vec![(1u8, "A".into(), "c.xhtml".into(), "a".into()), (2, "B".into(), "c.xhtml".into(), "b".into()), (3, "C".into(), "c.xhtml".into(), "c".into()), (1, "D".into(), "c.xhtml".into(), "d".into())];
        let nav = build_nav(&items, "");
        assert!(nav.contains(r#"<li><a href="c.xhtml#a">A</a><ol><li><a href="c.xhtml#b">B</a><ol><li><a href="c.xhtml#c">C</a></li></ol></li></ol></li><li><a href="c.xhtml#d">D</a></li></ol>"#), "{nav}");
        let ncx = build_ncx(&items, "", "T");
        assert!(ncx.contains(r#"<navPoint id="np1" playOrder="1"><navLabel><text>A</text></navLabel><content src="c.xhtml#a"/><navPoint id="np2""#), "{ncx}");
        assert!(ncx.contains(r#"</navPoint></navPoint></navPoint><navPoint id="np4""#), "C 收 3 层再开 D: {ncx}");
    }
}
