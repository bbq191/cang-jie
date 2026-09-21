//! 漫画 EPUB 的「文字页留边」：阅读器页边距设成 1（[`crate::imgopt::EPUB_COMIC_MARGINS`]）后，图片页满宽了，
//! 但版权页、前情提要、章节标题页这类**纯文字页**会贴着屏幕边缘。这里给这些页的 `<body>` 加类 [`TEXT_PAGE_CLASS`]，
//! 外链 `cangjie-wash.css` 里配一条 `margin-left/right`，把文字页的左右留白补回默认 56 档的水平。
//!
//! **为什么是 body 类 + margin（2026-09-21 真机诊断书量渲染缓存 `<uuid>.pdf`，页宽 303pt、默认 56 档基线左 17.8/右 18.2）**：
//! - `padding-left/right` 无论写在类、body 还是裸元素上，xochitl **一律不生效**；
//! - `margin-left/right` 用 **pt / em 精确生效**（18pt → 文字左右各多 18pt），`%` 换算怪异（6% 只多出约 1.9pt），不用；
//! - 类选择器（div 类 / body 类）、裸 `p` / `body` / `blockquote` 上的 margin 都生效——取 body 类：不改页面结构、不必包一层 div。
//!
//! 只处理**没有任何图片/SVG 的纯文字页**（"图片 + 一行说明"的页加边距会把图缩小，代价是那行字贴边，接受）。
//! 只在 [`crate::imgopt::EpubComicFrame::MinMargin`] 模式且整本判为漫画时做；缺省模式（开关关）页边距仍是 56，文字页本来就有留白。
//! 幂等：重复优化不会重复加类/规则。
//!
//! **第二件事：含图页去掉 `<body>` 的 class（2026-09-21 真机结构隔离诊断）**。Calibre 转的书每页 `<body class="calibre2">`，
//! 阅读器边距设成 1 后漫画页图片仍被吃掉约 20pt（只有 281.7~291.8pt 宽，理论 302.0）。诊断（`zz-ip3`，边距 1）：
//! 嵌套 div / `.calibre17` 居中 / `.calibre20{width:796px}`（xochitl 直接无视）全都不影响（302.0）；**只要 body 带这个类就被吃**，
//! 而且改类里的规则内容（去掉 `margin:0 5pt`、覆盖成 0/0.01pt/1pt/-5pt）**毫无影响**——所以 CSS 覆盖走不通，只能不带类。
//! body 不带类 + 完整书样式表 → 302.0（0.3/0.7，与合成书一致）。故 [`free_media_pages`] 把**含图片/SVG 的页**的 body class 整个去掉
//! （图片页没有依赖 body 类的排版；图文混排页的说明文字随之贴边，与"不给混排页加边距"的取舍一致）。

use crate::epubzip::Entry;
use crate::wash::parse_opf;
use regex::Regex;
use std::sync::OnceLock;

/// 加在纯文字页 `<body>` 上的类名。
pub const TEXT_PAGE_CLASS: &str = "cj-tp";
/// 追加进 `cangjie-wash.css` 的规则。17.8pt = 默认 56 档的水平留白（56 × 303/954）；末尾分号必须有（xochitl 丢最后一个无分号声明）。
pub const TEXT_PAGE_CSS_RULE: &str = ".cj-tp{margin-left:17.8pt;margin-right:17.8pt;}";

fn has_media(html: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)<(?:img|image|svg|video|object|canvas)\b"#).unwrap()).is_match(html)
}

fn body_tag() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<body\b([^>]*)>"#).unwrap())
}

fn class_attr() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)\bclass\s*=\s*("([^"]*)"|'([^']*)')"#).unwrap())
}

/// 纯文字页：没有图片/SVG 等媒体，且有可见文字（空页不算）。
pub fn is_pure_text_page(html: &str) -> bool {
    let body = crate::comic_detect::strip_noise_tags(html);
    !has_media(&body) && crate::wash::plain_text(&body).chars().any(|c| !c.is_whitespace())
}

/// `<body>` 是否已带 [`TEXT_PAGE_CLASS`]。
pub fn has_text_page_class(html: &str) -> bool {
    let Some(m) = body_tag().captures(html) else { return false };
    class_attr().captures(&m[1]).is_some_and(|c| c.get(2).or(c.get(3)).is_some_and(|v| v.as_str().split_whitespace().any(|t| t == TEXT_PAGE_CLASS)))
}

/// 给 `<body>` 加上 [`TEXT_PAGE_CLASS`]（保留原有 class）。没有 `<body>` 或已带 → `None`。
pub fn add_text_page_class(html: &str) -> Option<String> {
    let m = body_tag().captures(html)?;
    if has_text_page_class(html) {
        return None;
    }
    let whole = m.get(0)?;
    let attrs = &m[1];
    let new_attrs = match class_attr().captures(attrs) {
        Some(c) => {
            let v = c.get(2).or(c.get(3)).map(|v| v.as_str()).unwrap_or("");
            let joined = if v.trim().is_empty() { TEXT_PAGE_CLASS.to_string() } else { format!("{} {TEXT_PAGE_CLASS}", v.trim()) };
            let all = c.get(0)?;
            format!("{}class=\"{joined}\"{}", &attrs[..all.start()], &attrs[all.end()..])
        }
        None => format!("{attrs} class=\"{TEXT_PAGE_CLASS}\""),
    };
    Some(format!("{}<body{new_attrs}>{}", &html[..whole.start()], &html[whole.end()..]))
}

/// 去掉 `<body>` 的整个 class 属性（保留其它属性）。没有 `<body>` 或没有 class → `None`。
pub fn strip_body_class(html: &str) -> Option<String> {
    let m = body_tag().captures(html)?;
    let attrs = &m[1];
    let c = class_attr().captures(attrs)?;
    let all = c.get(0)?;
    let whole = m.get(0)?;
    // 连同 class 属性前面的空白一起删，避免留下 `<body  id=..>` 这种难看的双空格。
    let head = attrs[..all.start()].trim_end();
    let new_attrs = format!("{head}{}", &attrs[all.end()..]);
    Some(format!("{}<body{new_attrs}>{}", &html[..whole.start()], &html[whole.end()..]))
}

/// 含图片/SVG 的页去掉 body class（见模块文档"第二件事"）。返回处理的页数。幂等。
pub fn free_media_pages(entries: &mut [(String, Vec<u8>, bool)]) -> usize {
    let mut n = 0usize;
    for (_, data, ish) in entries.iter_mut() {
        if !*ish {
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else { continue };
        if !has_media(&crate::comic_detect::strip_noise_tags(text)) {
            continue;
        }
        if let Some(new) = strip_body_class(text) {
            *data = new.into_bytes();
            n += 1;
        }
    }
    n
}

/// 优化第一阶段的入口：给整本漫画的纯文字页加类，并把规则追加进 `cangjie-wash.css`。
/// 找不到外链 wash css（没跑清洗层）就**整体跳过**——只加类不配规则毫无意义。返回加了类的页数。
pub fn pad_text_pages(entries: &mut [(String, Vec<u8>, bool)]) -> usize {
    let Some(css_idx) = entries.iter().position(|(n, _, _)| crate::wash::is_wash_css_name(n)) else { return 0 };
    let mut padded = 0usize;
    let mut any_text_page = false;
    for (_, data, ish) in entries.iter_mut() {
        if !*ish {
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else { continue };
        if !is_pure_text_page(text) {
            continue;
        }
        any_text_page = true;
        if let Some(new) = add_text_page_class(text) {
            *data = new.into_bytes();
            padded += 1;
        }
    }
    if any_text_page {
        let css = &mut entries[css_idx].1;
        if let Ok(text) = std::str::from_utf8(css) {
            if !text.contains(TEXT_PAGE_CSS_RULE) {
                let mut s = text.trim_end().to_string();
                s.push('\n');
                s.push_str(TEXT_PAGE_CSS_RULE);
                s.push('\n');
                *css = s.into_bytes();
            }
        }
    }
    padded
}

/// 沿 spine 检查：所有纯文字页都已带类（没有文字页 → 真）。`comic_detect::is_min_margin_comic_file` 用它把
/// "上一版优化的漫画（文字页没留边）"挡在登记之外——否则页边距设成 1 后文字页会贴边。
pub fn all_text_pages_padded(entries: &[Entry]) -> bool {
    let Some(opf) = parse_opf(entries) else { return false };
    opf.spine.iter().filter_map(|p| entries.iter().find(|e| &e.name == p)).all(|e| match std::str::from_utf8(&e.data) {
        Ok(html) if crate::epubzip::is_html(&e.name) => !is_pure_text_page(html) || has_text_page_class(html),
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_text_page_detection() {
        assert!(is_pure_text_page("<html><head><title>t</title></head><body><p>版权信息</p></body></html>"));
        assert!(!is_pure_text_page("<html><body><p>说明</p><img src=\"a.jpg\"/></body></html>"), "图+字不是纯文字页");
        assert!(!is_pure_text_page("<html><body><svg><image href=\"a.jpg\"/></svg></body></html>"), "SVG 封面页不是");
        assert!(!is_pure_text_page("<html><head><title>只有标题</title></head><body>  \n </body></html>"), "head 里的 title 不算可见文字；空页不算");
    }

    #[test]
    fn add_class_keeps_existing_and_is_idempotent() {
        let a = add_text_page_class("<html><body><p>x</p></body></html>").unwrap();
        assert!(a.contains(r#"<body class="cj-tp">"#), "{a}");
        assert!(add_text_page_class(&a).is_none(), "已带则不重复加");
        let b = add_text_page_class(r#"<html><body id="b" class="calibre x">t</body></html>"#).unwrap();
        assert!(b.contains(r#"<body id="b" class="calibre x cj-tp">"#), "保留原 class 与其它属性: {b}");
        let c = add_text_page_class("<html><BODY CLASS='k'>t</BODY></html>").unwrap();
        assert!(has_text_page_class(&c) && c.contains("k cj-tp"), "大小写/单引号: {c}");
        assert!(add_text_page_class("<p>没有 body</p>").is_none());
        assert!(!has_text_page_class(r#"<body class="cj-tpx">"#), "类名要整词匹配");
    }

    #[test]
    fn strip_body_class_only_on_media_pages() {
        assert_eq!(strip_body_class(r#"<html><body id="b" class="calibre2">x</body></html>"#).unwrap(), r#"<html><body id="b">x</body></html>"#);
        assert_eq!(strip_body_class(r#"<html><body class='calibre2'>x</body></html>"#).unwrap(), "<html><body>x</body></html>");
        assert!(strip_body_class("<html><body>x</body></html>").is_none(), "没有 class 不动");
        let mut es = vec![
            ("a.xhtml".to_string(), r#"<html><head><title>t</title></head><body class="calibre2"><div><img src="1.jpg"/></div></body></html>"#.as_bytes().to_vec(), true),
            ("b.xhtml".to_string(), r#"<html><body class="calibre2"><p>版权页</p></body></html>"#.as_bytes().to_vec(), true),
            ("c.xhtml".to_string(), r#"<html><body class="calibre2"><svg><image href="c.jpg"/></svg></body></html>"#.as_bytes().to_vec(), true),
        ];
        assert_eq!(free_media_pages(&mut es), 2, "图片页、SVG 页去类");
        assert!(!std::str::from_utf8(&es[0].1).unwrap().contains("class=\"calibre2\""));
        assert!(std::str::from_utf8(&es[1].1).unwrap().contains("calibre2"), "纯文字页不动（留给 cj-tp）");
        assert_eq!(free_media_pages(&mut es), 0, "幂等");
    }

    fn entry(name: &str, s: &str, html: bool) -> (String, Vec<u8>, bool) {
        (name.to_string(), s.as_bytes().to_vec(), html)
    }

    #[test]
    fn pad_only_pure_text_pages_and_add_css_once() {
        let mut es = vec![
            entry("OEBPS/cangjie-wash.css", "p{text-indent:0.01em;}\n", false),
            entry("OEBPS/copyright.xhtml", "<html><body><p>版权页</p></body></html>", true),
            entry("OEBPS/p1.xhtml", "<html><body><img src=\"1.jpg\"/></body></html>", true),
            entry("OEBPS/mixed.xhtml", "<html><body><img src=\"2.jpg\"/><p>阿塔……</p></body></html>", true),
            entry("OEBPS/blank.xhtml", "<html><body></body></html>", true),
        ];
        assert_eq!(pad_text_pages(&mut es), 1);
        assert!(std::str::from_utf8(&es[1].1).unwrap().contains("cj-tp"));
        for i in [2, 3, 4] {
            assert!(!std::str::from_utf8(&es[i].1).unwrap().contains("cj-tp"), "图片页/混排页/空页不加: {}", es[i].0);
        }
        let css = std::str::from_utf8(&es[0].1).unwrap().to_string();
        assert_eq!(css.matches(TEXT_PAGE_CSS_RULE).count(), 1, "{css}");
        assert!(css.trim_end().ends_with(';') || css.trim_end().ends_with('}'));
        // 幂等
        assert_eq!(pad_text_pages(&mut es), 0);
        assert_eq!(std::str::from_utf8(&es[0].1).unwrap().matches(TEXT_PAGE_CSS_RULE).count(), 1);
    }

    #[test]
    fn no_wash_css_means_no_change_and_no_text_pages_means_no_css_rule() {
        let mut es = vec![entry("c.xhtml", "<html><body><p>字</p></body></html>", true)];
        assert_eq!(pad_text_pages(&mut es), 0, "没有 wash css：只加类不配规则没意义，整体跳过");
        assert!(!std::str::from_utf8(&es[0].1).unwrap().contains("cj-tp"));
        let mut es = vec![entry("cangjie-wash.css", "p{x:1;}", false), entry("p.xhtml", "<html><body><img src=\"a\"/></body></html>", true)];
        assert_eq!(pad_text_pages(&mut es), 0);
        assert!(!std::str::from_utf8(&es[0].1).unwrap().contains(".cj-tp"), "没有文字页就不写规则");
    }
}
