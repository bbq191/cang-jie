//! EPUB 里 href 的两层编码：先是 **XML 实体**（属性值里的 `&amp;`/`&#38;` 等，XML 解析器本该解掉、这里不用完整解析器所以手解），
//! 再是 **URL 百分号编码**（`my%20toc.xhtml`）。OPF manifest、`container.xml`、nav 的 `<a href>`、NCX 的 `<content src>`
//! 都要按这个顺序解完，才能跟 zip 条目路径 / `.epubindex` 里的文件名对上。

/// 解 XML 预定义实体与数字字符引用（`&amp; &lt; &gt; &quot; &apos; &#NN; &#xHH;`）；认不出的 `&...` 原样保留。
pub(crate) fn xml_unescape(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('&') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // 实体最长也就 `&#x10FFFF;` 这么几个字符；在近处找不到 `;` 就不是实体
        let decoded = tail[1..].char_indices().take(10).find(|&(_, c)| c == ';').and_then(|(j, _)| {
            let name = &tail[1..1 + j];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let n = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")).map(|h| u32::from_str_radix(h, 16)).or_else(|| name.strip_prefix('#').map(|d| d.parse()))?.ok()?;
                    char::from_u32(n)?
                }
            };
            Some((c, j + 2))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &tail[used..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

/// URL 百分号解码（`%E7%AB%A0` → `章`）；不成对的 `%` 原样保留，解出来不是合法 UTF-8 就退回原串。
pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// href / src 属性的原始值 → 文件路径部分：解 XML 实体、去 `#片段`、再百分号解码。
pub(crate) fn href_path(raw: &str) -> String {
    let unescaped = xml_unescape(raw);
    percent_decode(unescaped.split('#').next().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_xml_entities_and_keeps_unknown_ones() {
        assert_eq!(xml_unescape("a&amp;b.xhtml"), "a&b.xhtml");
        assert_eq!(xml_unescape("&lt;&gt;&quot;&apos;&#38;&#x26;&#X4E2D;"), "<>\"'&&中");
        assert_eq!(xml_unescape("R&D &nbsp; & &;  &#xZZ; &#1114112;"), "R&D &nbsp; & &;  &#xZZ; &#1114112;", "认不出的原样留下");
        assert_eq!(xml_unescape("&amp;amp;"), "&amp;", "只解一层");
        assert!(matches!(xml_unescape("plain.xhtml"), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn href_path_unescapes_before_cutting_fragment_and_percent_decoding() {
        assert_eq!(href_path("a&amp;b.xhtml#p1"), "a&b.xhtml");
        assert_eq!(href_path("my%20toc.xhtml"), "my toc.xhtml");
        assert_eq!(href_path("x&#x23;y.xhtml"), "x", "实体解出来的 # 也是片段分隔符（与 XML 解析器解完再按 URL 读一致）");
        assert_eq!(percent_decode("%E7%AB%A0%zz%"), "章%zz%");
    }
}
