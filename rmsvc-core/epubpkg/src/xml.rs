//! 不用完整 XML 解析器的标签扫描（正则）：先去掉注释，属性认双引号、单引号、无引号三种写法，元素名认命名空间前缀
//! （`<opf:item>`），名字不分大小写。只服务 container.xml / OPF / 正文里找几个标签这类小事，不做校验。
use crate::escape::xml_unescape;
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

pub(crate) fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

/// 去掉 `<!-- … -->` 注释（注释里的标签不算数）。
pub fn strip_comments(s: &str) -> Cow<'_, str> {
    static C: OnceLock<Regex> = OnceLock::new();
    re(&C, r"(?s)<!--.*?-->").replace_all(s, "")
}

/// 文本里所有名为 `local`（不分大小写，认 `前缀:local`）的开标签/自闭合标签原文，文档序。不去注释（调用方先 [`strip_comments`]）。
pub fn start_tags<'a>(text: &'a str, local: &str) -> Vec<&'a str> {
    static TAG: OnceLock<Regex> = OnceLock::new();
    re(&TAG, r"<([A-Za-z_][-\w.]*:)?([A-Za-z_][-\w.]*)\b[^>]*>")
        .captures_iter(text)
        .filter(|c| c[2].eq_ignore_ascii_case(local))
        .map(|c| c.get(0).unwrap().as_str())
        .collect()
}

/// 标签里名为 `name`（完整属性名、不分大小写）的属性原文值（未解实体）。
pub fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    static A: OnceLock<Regex> = OnceLock::new();
    // 跳过开头的 `<元素名`，免得把元素名当属性
    let body = tag.find(|c: char| c.is_whitespace()).map_or("", |i| &tag[i..]);
    // 无引号的值可以含 `/`（`media-type=application/xhtml+xml`），但不以 `/` 结尾（那是自闭合的 `/>`）
    re(&A, r#"([A-Za-z_:][-\w:.]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]*[^\s"'>/]))"#)
        .captures_iter(body)
        .find(|c| c[1].eq_ignore_ascii_case(name))
        .map(|c| c.get(2).or(c.get(3)).or(c.get(4)).unwrap().as_str())
}

/// 元素内容 → 纯文本：去标签（含 CDATA 包装）、字符引用还原（另把 OPF 里常见但 XML 没定义的 `&nbsp;` 当空格）、
/// 空白折叠成单个空格。写回 XML 时调用方要自己再转义。
pub fn plain_text(fragment: &str) -> String {
    static TAGS: OnceLock<Regex> = OnceLock::new();
    let s = fragment.replace("<![CDATA[", "").replace("]]>", "");
    let s = re(&TAGS, r"<[^>]*>").replace_all(&strip_comments(&s), "").replace("&nbsp;", " ");
    xml_unescape(&s).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_quotes_prefixes_and_comments() {
        let t = r#"<opf:item id='a' href="x&amp;y.xhtml" media-type=application/xhtml+xml/>"#;
        assert_eq!(attr(t, "id"), Some("a"));
        assert_eq!(attr(t, "HREF"), Some("x&amp;y.xhtml"));
        assert_eq!(attr(t, "media-type"), Some("application/xhtml+xml"));
        assert_eq!(attr(t, "properties"), None);
        let text = r#"<a><!-- <opf:item id="x"/> --><opf:item id="y"/><itemref idref="z"/></a>"#;
        assert_eq!(start_tags(&strip_comments(text), "item"), [r#"<opf:item id="y"/>"#], "注释里的不算、itemref 不是 item");
    }

    #[test]
    fn plain_text_strips_tags_cdata_and_entities() {
        assert_eq!(plain_text("  Tom &amp; <i>Jerry</i>&nbsp;&#20013; "), "Tom & Jerry 中");
        assert_eq!(plain_text("<![CDATA[A & B]]>"), "A & B");
        assert_eq!(plain_text("a<!-- x -->b"), "ab");
    }
}
