//! `<uuid>.epubindex` 解析：xochitl 导入 EPUB 时生成的二进制索引，记录每个 spine 文件的起始页。
//! 格式事实（逆向，3.28.0.172 真机《人骨拼圖》再核）：文件里有两张表，每条 = `u32 长度 + UTF-16BE 路径 + 3 × u32`；
//! 第一张表的第一个 u32 是**起始页**（0-based）；第二张表的第三个 u32 也是起始页（第一、二个是字符偏移/长度）。
//! 我们只取每个 basename **首次出现**的第一个 u32，按起始页升序。路径全 ASCII，非 ASCII 一律不认（防误配）。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// spine 文件 basename（与目录 href 的 basename 对齐）。
    pub file: String,
    pub start_page: u32,
}

fn be32(b: &[u8], p: usize) -> u32 {
    u32::from_be_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

/// 定长 UTF-16BE 的 ASCII 串；含非可打印 ASCII → None。
fn utf16be_ascii(b: &[u8]) -> Option<String> {
    if b.len() % 2 != 0 {
        return None;
    }
    b.chunks(2).map(|c| (c[0] == 0 && (0x20..0x7f).contains(&c[1])).then_some(c[1] as char)).collect()
}

fn is_html(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".xhtml") || l.ends_with(".html") || l.ends_with(".htm")
}

pub fn parse_epubindex(bytes: &[u8]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let n = bytes.len();
    let mut p = 0usize;
    while p + 4 <= n {
        let len = be32(bytes, p) as usize;
        if (8..=512).contains(&len) && len % 2 == 0 && p + 4 + len + 12 <= n {
            if let Some(path) = utf16be_ascii(&bytes[p + 4..p + 4 + len]).filter(|s| is_html(s)) {
                let q = p + 4 + len;
                let file = path.rsplit('/').next().unwrap_or(&path).to_string();
                if seen.insert(file.clone()) {
                    out.push(Section { file, start_page: be32(bytes, q) });
                }
                p = q + 12;
                continue;
            }
        }
        p += 1;
    }
    out.sort_by_key(|s| s.start_page);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_fixture_two_tables_first_wins() {
        let secs = parse_epubindex(include_bytes!("../../../testdata/renggu/book.epubindex"));
        let get = |f: &str| secs.iter().find(|s| s.file == f).map(|s| s.start_page);
        assert_eq!(get("titlepage.xhtml"), Some(0));
        assert_eq!(get("1.xhtml"), Some(3));
        assert_eq!(get("7.xhtml"), Some(97));
        assert_eq!(get("37.xhtml"), Some(510));
        assert_eq!(secs.len(), 39, "第二张表同名条目不重复计入");
        assert!(secs.windows(2).all(|w| w[0].start_page <= w[1].start_page));
        assert!(parse_epubindex(b"garbage").is_empty());
    }
}
