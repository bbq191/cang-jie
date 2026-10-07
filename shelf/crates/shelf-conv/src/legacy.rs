//! 旧版优化产物的兼容预处理：母版库里 cang-jie 自带 bookconv（v16 及以前）优化过的书，交给 sheng-ren 优化器之前先过一遍。
//!
//! 2026-10-07 起 book-serve 改用 sheng-ren 的 bookconv（v51 起）。两边写进书里的东西前缀不同：标记
//! `META-INF/com.cangjie.optimized` ↔ `META-INF/eink-optimized`、洗书样式表 `cangjie-wash.css` ↔ `eink-wash.css`、CSS 类
//! `cj-*` ↔ `eink-*`。sheng-ren 的优化器只认自己的前缀：直接喂旧产物，会再写一份 `eink-wash.css`（旧的那份和每章的
//! `<link>` 留着，两份样式表叠加），旧类名对不上新样式表（顶格段、居中、注释字号全失效），v15 及以前追加的重复 `[N]`
//! 也没人去。所以先把旧产物"翻译"成 sheng-ren 的写法（[`preprocess_file`]），再交给优化器：
//!
//! 1. 删旧标记 [`LEGACY_MARKER`]（sheng-ren 产物里只留它自己的标记）；
//! 2. `cangjie-wash.css` 改名 `eink-wash.css`（同目录），OPF manifest 的 href/id 与各章 `<link href>` 跟着改——sheng-ren 清洗层
//!    认出这份就地重写内容，不会再多一份；
//! 3. 旧类名按 [`CLASS_MAP`] 换成 sheng-ren 的同义类（只改 `class` 属性里的词，不碰正文）；
//! 4. 去掉 v15 及以前在注释标号后追加的重复 `[N]`（[`strip_legacy_note_counters`]，只去能确定是重复的那种）。
//!
//! **只改标签属性、不改可见文字**（第 4 条除外：那几个 `[N]` 本来就是旧优化器多加的字）。图片等其余条目原样拷贝压缩数据
//! （`raw_copy`，不解压不解码），整本不进内存——大漫画几百 MB，设备内存小。
//!
//! 不在映射表里的 `cj-` 类不动：`cj-cN`/`cj-wN`（PDF 转 EPUB 的颜色/宽度类）、`cj-imgwrap`（已删的漫画分卷）都定义在书自己的
//! 样式表里，不是优化器的样式表，改名反而失效。`cj-toc-N`、`cj-sec-N`、`cj-uid` 等是 **id** 不是类（目录、`refines` 指着它们），
//! 也不动——改 id 要连同全书引用一起改，拿不准就不处理。

use bookconv::html::{self, Edit};
use std::io::Read;
use std::path::Path;

/// 旧版优化器埋的标记（内容是版本号，`16`、`16-core`……）。
pub const LEGACY_MARKER: &str = "META-INF/com.cangjie.optimized";
/// 旧版洗书样式表的文件名与 manifest id。
const LEGACY_WASH_CSS: &str = "cangjie-wash.css";
const LEGACY_WASH_CSS_ID: &str = "cangjie-wash-css";
/// sheng-ren 清洗层的样式表文件名与 manifest id（`bookconv::wash::is_wash_css_name` 认这个名字，见测试）。
pub const WASH_CSS: &str = "eink-wash.css";
const WASH_CSS_ID: &str = "eink-wash-css";

/// 旧类 → sheng-ren 的同义类（逐个对过两边的样式规则与生成处）：
/// - `cj-flush`：清洗层把"标题后首段"改写成的顶格 `<div>`（英文书）——sheng-ren `typeset` 同一写法 `eink-flush`；
/// - `cj-tx`：最小页边距漫画混排页的文字块留边——sheng-ren `comicpad` 的 `eink-tx`；
/// - `cj-tp`：最小页边距漫画纯文字页的 `<body>` 留边——sheng-ren `comicpad` 叫 `eink-textpage`；
/// - `cj-center`/`cj-right`：`align=`/行内 `text-align` 换成的类——`eink-center`/`eink-right`；
/// - `cj-note`：章末每条注释的容器（不跨页）——`eink-note`；`cj-noteicon`：多看图标注释号（限一个字高）——`eink-noteicon`；
/// - `cj-fnote`：已删的 Inline 注释模式写在引用处的 `<span>〔注释〕</span>`——sheng-ren 样式表里同样有 `.eink-fnote`（注释字号），
///   文字原样留着；
/// - `cj-wash`：v9 及以前注入的内联 `<style class="cj-wash">` 排版块——改成 `eink-wash` 后 sheng-ren 清洗层认得出、会清掉。
pub const CLASS_MAP: &[(&str, &str)] = &[
    ("cj-flush", "eink-flush"),
    ("cj-tx", "eink-tx"),
    ("cj-tp", "eink-textpage"),
    ("cj-center", "eink-center"),
    ("cj-right", "eink-right"),
    ("cj-note", "eink-note"),
    ("cj-noteicon", "eink-noteicon"),
    ("cj-fnote", "eink-fnote"),
    ("cj-wash", "eink-wash"),
];

/// 读标记这类小条目的上限（超过算读不出来，不截断）。
const MAX_MARKER_BYTES: u64 = 64;

fn map_class(c: &str) -> Option<&'static str> {
    CLASS_MAP.iter().find(|(old, _)| *old == c).map(|(_, new)| *new)
}

/// 书里旧版优化标记的内容（去首尾空白），即优化它的旧版优化器版本（`16`、`16-core`……）。没有标记、打不开 → `None`。
pub fn legacy_version_file(path: &Path) -> Option<String> {
    let f = std::fs::File::open(path).ok()?;
    let mut ar = zip::ZipArchive::new(std::io::BufReader::new(f)).ok()?;
    let entry = ar.by_name(LEGACY_MARKER).ok()?;
    let bytes = bookconv::util::read_capped(entry, MAX_MARKER_BYTES, 0).ok()??;
    Some(String::from_utf8(bytes).ok()?.trim().to_string())
}

/// [`preprocess_file`] 的统计。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// 改过的 XHTML 文件数。
    pub html_files: usize,
    /// 换掉的类名个数（按出现次数）。
    pub classes_renamed: usize,
    /// 去掉的重复 `[N]` 个数。
    pub counters_stripped: usize,
    /// `cangjie-wash.css` 改了名。
    pub wash_css_renamed: bool,
}

/// `name`（zip 条目路径）的文件名部分是不是 `file`。
fn basename_is(name: &str, file: &str) -> bool {
    name.rsplit('/').next() == Some(file)
}

/// 同目录下换文件名（`a/b/cangjie-wash.css` → `a/b/eink-wash.css`）。
fn with_basename(name: &str, file: &str) -> String {
    match name.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{file}"),
        None => file.to_string(),
    }
}

/// 链接值（可带 `#`/`?`，按原文比较）的文件名部分是旧样式表时换成新名，其余 `None`。
fn rename_css_href(v: &str) -> Option<String> {
    let path_end = v.find(['#', '?']).unwrap_or(v.len());
    let path = &v[..path_end];
    basename_is(path, LEGACY_WASH_CSS).then(|| format!("{}{}", with_basename(path, WASH_CSS), &v[path_end..]))
}

/// `class` 属性值里的旧类换成新类（按空白切词、整词匹配；其余词与空白原样保留）。返回（新值, 换掉的个数）。
fn rename_classes(v: &str) -> (String, usize) {
    let mut out = String::with_capacity(v.len() + 8);
    let mut n = 0;
    let mut rest = v;
    while !rest.is_empty() {
        let ws = rest.len() - rest.trim_start().len();
        out.push_str(&rest[..ws]);
        rest = &rest[ws..];
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..word_end];
        match map_class(word) {
            Some(new) => {
                out.push_str(new);
                n += 1;
            }
            None => out.push_str(word),
        }
        rest = &rest[word_end..];
    }
    (out, n)
}

/// 一章 XHTML：去重复 `[N]` → 改类名、改指向旧样式表的 `<link href>`。返回（新文本, 换掉的类名数, 去掉的 `[N]` 数）；没改动时文本原样。
pub fn convert_html(text: &str) -> (String, usize, usize) {
    let stripped = strip_legacy_note_counters(text);
    let counters = count_counters(text) - count_counters(&stripped);
    let mut classes = 0usize;
    let out = html::edit_attrs(&stripped, &["class", "href"], |t, a| {
        if a.is("class") {
            let (v, n) = rename_classes(a.value);
            if n == 0 {
                return Edit::Keep;
            }
            classes += n;
            Edit::Set(v)
        } else if t.is("link") {
            rename_css_href(a.value).map_or(Edit::Keep, Edit::Set)
        } else {
            Edit::Keep
        }
    })
    .into_owned();
    (out, classes, counters)
}

/// OPF：manifest 里旧样式表那一项的 href、id 换成新名。
pub fn convert_opf(text: &str) -> String {
    html::edit_attrs(text, &["href", "id"], |t, a| {
        if !t.is("item") {
            Edit::Keep
        } else if a.is("href") {
            rename_css_href(a.value).map_or(Edit::Keep, Edit::Set)
        } else if a.value == LEGACY_WASH_CSS_ID {
            Edit::Set(WASH_CSS_ID.to_string())
        } else {
            Edit::Keep
        }
    })
    .into_owned()
}

/// 洗书样式表里的类选择器（`.cj-flush{…}`、`p.cj-tx{…}`）换成新类名。sheng-ren 清洗层随后整份重写这个文件，这里换名只是让
/// 中间产物前后一致。
fn convert_wash_css(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\.(cj-[A-Za-z0-9_-]+)").unwrap());
    re.replace_all(text, |c: &regex::Captures| match map_class(&c[1]) {
        Some(new) => format!(".{new}"),
        None => c[0].to_string(),
    })
    .into_owned()
}

fn counter_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r##" <a href="#([^"]+)">\[\d+\]</a>"##).unwrap())
}

fn count_counters(text: &str) -> usize {
    counter_re().find_iter(text).count()
}

/// 去掉 v15 及以前在注释标号后面追加的 `[N]`：形如 `<a href="#f">原标号</a> <a href="#f">[3]</a>`——前面紧挨着一个指向同一锚点
/// 的链接时才去（原标号还在、仍可点，去掉后就是原书的样子）。另两种旧写法（图标整个换成 `[N]`、`<sup>` 后跟 `[N]`）原标号已被
/// 替换或不再是链接，去掉会丢标号，不动。母版库的优化是原地覆盖，旧版产物已经没有原件可回，只能这样还原能确定的部分。
/// （原 cang-jie `bookconv::optimize::html_pass::strip_legacy_note_counters`，原样搬来。）
pub fn strip_legacy_note_counters(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for c in counter_re().captures_iter(text) {
        let m = c.get(0).unwrap();
        let before = &text[..m.start()];
        let same_target = before.ends_with("</a>")
            && before.rfind("<a ").is_some_and(|i| html::attr_value(&before[i..before[i..].find('>').map_or(before.len(), |j| i + j + 1)], "href") == Some(&format!("#{}", &c[1])));
        if same_target {
            out.push_str(&text[last..m.start()]);
            last = m.end();
        }
    }
    out.push_str(&text[last..]);
    out
}

/// 预处理一本旧版优化产物：`input` 里有 [`LEGACY_MARKER`] 时把翻译后的书写到 `output`，返回统计；没有旧标记 → `Ok(None)`，
/// 不碰 `output`（调用方直接拿原书去优化）。出错时删掉这次建出的 `output`。调用方照旧用临时文件、成功才改名。
pub fn preprocess_file(input: &Path, output: &Path) -> Result<Option<Report>, String> {
    let file = std::fs::File::open(input).map_err(|e| format!("打开 {} 失败: {e}", input.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("解 EPUB(非 zip?): {e}"))?;
    if zip.index_for_name(LEGACY_MARKER).is_none() {
        return Ok(None);
    }
    let names: Vec<String> = zip.file_names().map(str::to_string).collect();
    if names.iter().any(|n| basename_is(n, LEGACY_WASH_CSS)) && names.iter().any(|n| basename_is(n, WASH_CSS)) {
        return Err(format!("书里同时有 {LEGACY_WASH_CSS} 和 {WASH_CSS}，不知道该留哪份，没有改动"));
    }
    let r = write_converted(&mut zip, output);
    if r.is_err() {
        let _ = std::fs::remove_file(output);
    }
    r.map(Some)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Html,
    Opf,
    WashCss,
    Other,
}

fn write_converted(zip: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>, output: &Path) -> Result<Report, String> {
    let mut rep = Report::default();
    let mut w = bookconv::epubzip::EpubWriter::create(output)?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| format!("读 EPUB 条目 {i}: {e}"))?;
        let name = f.name().to_string();
        if f.is_dir() || name == "mimetype" || name == LEGACY_MARKER {
            continue; // mimetype 建 `EpubWriter` 时已写；旧标记删掉
        }
        let kind = if bookconv::epubzip::is_html(&name) {
            Kind::Html
        } else if name.to_ascii_lowercase().ends_with(".opf") {
            Kind::Opf
        } else if basename_is(&name, LEGACY_WASH_CSS) {
            Kind::WashCss
        } else {
            Kind::Other
        };
        if kind == Kind::Other {
            w.raw_copy(f)?; // 图片、字体、NCX 等：压缩数据原样拷贝，不解压不解码
            continue;
        }
        let size = f.size();
        let bytes = bookconv::epubzip::read_all(&mut f, size, &name)?;
        drop(f);
        let Ok(text) = std::str::from_utf8(&bytes) else {
            // 不是 UTF-8 的文本条目：认不出就不改，原样写回（样式表照样换名，免得 OPF/`<link>` 指空）
            let out_name = if kind == Kind::WashCss { with_basename(&name, WASH_CSS) } else { name };
            w.put(&out_name, &bytes)?;
            continue;
        };
        let (out_name, out_text) = match kind {
            Kind::Html => {
                let (t, classes, counters) = convert_html(text);
                if t != text {
                    rep.html_files += 1;
                }
                rep.classes_renamed += classes;
                rep.counters_stripped += counters;
                (name, t)
            }
            Kind::Opf => (name, convert_opf(text)),
            Kind::WashCss => {
                rep.wash_css_renamed = true;
                (with_basename(&name, WASH_CSS), convert_wash_css(text))
            }
            Kind::Other => unreachable!("上面已经原样拷贝"),
        };
        w.put(&out_name, out_text.as_bytes())?;
    }
    w.finish()?;
    Ok(rep)
}

// ───────────── 旧版"最小页边距"漫画的识别（登记"首次打开设页边距"用） ─────────────

/// 旧版"页边距最小化"页框从 v15 起才有；v16 只改了文字处理与图片编码，页框没变。
const LEGACY_MIN_MARGIN_SINCE: u32 = 15;
/// 旧版最小页边距页框：2026-09-30 起 952×1457，之前 954×1458。
const LEGACY_FRAMES: [(u32, u32); 2] = [(952, 1457), (954, 1458)];
/// 旧版纯文字页、混排页文字块的留边类。
const LEGACY_TEXT_PAGE_CLASS: &str = "cj-tp";
const LEGACY_TEXT_BLOCK_CLASS: &str = "cj-tx";

/// 旧版（v15、v16）按"最小页边距"页框优化、文字页也留了边的漫画：还没用 sheng-ren 重新优化、就直接投到原生书库时，
/// 照旧登记"首次打开设页边距 1"（新产物看 sheng-ren 写的 `META-INF/eink-reader-margins`）。判据同原 cang-jie
/// `comic_detect::is_min_margin_comic_file` + `is_min_margin_framed_file`：整本是漫画、纯文字页带 `cj-tp`、混排页的文字块带
/// `cj-tx`，且抽查的整页图过半是最小页边距页框的尺寸。只读文字条目和图片文件头，不解码。
pub fn is_legacy_min_margin_comic_file(path: &Path) -> bool {
    legacy_version_file(path).and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v >= LEGACY_MIN_MARGIN_SINCE) && legacy_text_padded_comic(path) && legacy_min_margin_framed(path)
}

fn legacy_text_padded_comic(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else { return false };
    let Ok(mut zip) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { return false };
    let Ok(sk) = bookconv::epubzip::read_skeleton(&mut zip) else { return false };
    let entries = sk.entries;
    if !bookconv::comic_detect::is_comic(&entries) {
        return false;
    }
    let Some(opf) = bookconv::wash::parse_opf(&entries) else { return false };
    // 同名取第一条（反向收集，先出现的覆盖后出现的）
    let by_name: std::collections::HashMap<&str, &bookconv::epubzip::Entry> = entries.iter().rev().map(|e| (e.name.as_str(), e)).collect();
    opf.spine.iter().filter_map(|p| by_name.get(p.as_str())).all(|e| match std::str::from_utf8(&e.data) {
        Ok(text) if bookconv::epubzip::is_html(&e.name) => {
            let body = bookconv::comic_detect::strip_noise_tags(text);
            let visible = html::plain_text(&body).chars().any(|c| !c.is_whitespace());
            let media = has_media(&body);
            let pure_text_ok = !(visible && !media) || body_has_class(text, LEGACY_TEXT_PAGE_CLASS);
            let mixed_ok = !(visible && media) || all_text_blocks_have_class(text, LEGACY_TEXT_BLOCK_CLASS);
            pure_text_ok && mixed_ok
        }
        _ => true,
    })
}

fn has_media(html_text: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"(?i)<(?:img|image|svg|video|object|canvas)\b"#).unwrap()).is_match(html_text)
}

fn class_has(tag: &str, class: &str) -> bool {
    html::attr_value(tag, "class").is_some_and(|v| v.split_whitespace().any(|c| c == class))
}

fn body_has_class(text: &str, class: &str) -> bool {
    html::tags(text).find(|t| t.is_start() && t.is("body")).is_some_and(|t| class_has(&text[t.start..t.end], class))
}

/// 每个文字块（`<p>`、`<h1-6>`、清洗层生成的 `<div class="cj-flush">`）都带 `class`。
fn all_text_blocks_have_class(text: &str, class: &str) -> bool {
    html::tags(text).filter(|t| t.is_start()).all(|t| {
        let tag = &text[t.start..t.end];
        let is_block = t.is("p") || t.heading_level().is_some() || (t.is("div") && class_has(tag, "cj-flush"));
        !is_block || class_has(tag, class)
    })
}

/// 抽前 24 张"整页大小"的图（短边 ≥ 318，装饰小图不计），过半是旧版最小页边距页框尺寸。只读每张图开头至多 256KB 取宽高。
fn legacy_min_margin_framed(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else { return false };
    let Ok(mut zip) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { return false };
    let (mut sampled, mut matched) = (0usize, 0usize);
    for i in 0..zip.len() {
        if sampled >= 24 {
            break;
        }
        let Ok(f) = zip.by_index(i) else { continue };
        let lower = f.name().to_ascii_lowercase();
        if !(lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".png")) {
            continue;
        }
        let mut head = Vec::new();
        if f.take(256 * 1024).read_to_end(&mut head).is_err() {
            continue;
        }
        let Some((_, (w, h))) = bookconv::imgopt::header_dims(&head) else { continue };
        if w.min(h) < 954 / 3 {
            continue;
        }
        sampled += 1;
        if LEGACY_FRAMES.contains(&(w, h)) {
            matched += 1;
        }
    }
    sampled > 0 && matched * 2 > sampled
}

#[cfg(test)]
mod tests;
