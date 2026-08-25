//! 设备 EPUB 的「页 → 章节名」映射。
//!
//! xochitl 给每本 EPUB 生成 `<uuid>.epubindex`（二进制）记录**每个 spine 文件的起始页**；
//! `.epub` 自身的 nav.xhtml / toc.ncx 记录**spine 文件 → 章节标题**。两者一合 → 页号 → 章名。
//!
//! `.epubindex` 格式（逆向自真机 3.28.x）：头 "rM epub index" 后是若干长度前缀 UTF-16BE 路径条目，
//! 每条路径后跟 3 个大端 u32；**起始页 = 第一个 u32**（两本真机书对齐坐实：赎罪 6 章 + 39 条目的
//! 《Tell Me Your Dreams》嵌套章 a 值均单调正确。旧启发式 `b==0?a:c` 是对赎罪的过拟合，多章书会错）。
//! 页号 0-based（cover=0），与 stardetect 的 page_index（顶层 pages 列表位置）对齐。

use std::collections::HashMap;
use std::io::{Cursor, Read};

use zip::ZipArchive;

fn be32(b: &[u8], p: usize) -> u32 {
    if p + 4 <= b.len() {
        u32::from_be_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
    } else {
        0
    }
}

/// 解码定长 UTF-16BE ASCII 串（非 ASCII/非可打印即返回 None，用于筛真路径）。
fn decode_utf16be_ascii(b: &[u8]) -> Option<String> {
    if b.len() % 2 != 0 {
        return None;
    }
    let mut s = String::with_capacity(b.len() / 2);
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] != 0 || !(0x20..0x7f).contains(&b[i + 1]) {
            return None;
        }
        s.push(b[i + 1] as char);
        i += 2;
    }
    Some(s)
}

/// 解析 .epubindex → [(spine 文件 basename, 起始页)]，按起始页升序、basename 去重（保留首现=Table1）。
pub fn parse_sections(bytes: &[u8]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let n = bytes.len();
    let mut p = 0usize;
    while p + 4 <= n {
        let len = be32(bytes, p) as usize;
        if len >= 8 && len <= 512 && len % 2 == 0 && p + 4 + len + 12 <= n {
            if let Some(s) = decode_utf16be_ascii(&bytes[p + 4..p + 4 + len]) {
                let sl = s.to_lowercase();
                if sl.ends_with(".xhtml") || sl.ends_with(".html") || sl.ends_with(".htm") {
                    let q = p + 4 + len;
                    // 每条路径后跟 3 个大端 u32：第一个(a)=起始页（两本真机书对齐坐实：赎罪
                    // Section01-06=2/39/73/113/152/194、TMYD 嵌套章 a 值单调递增均正确）；
                    // 中间(b)是标志位（0/1），第三个(c)非页号（旧码 b!=0 时误取 c → 多章书页号乱）。
                    let start = be32(bytes, q);
                    let base = s.rsplit('/').next().unwrap_or(&s).to_string();
                    if seen.insert(base.clone()) {
                        out.push((base, start));
                    }
                    p = q + 12;
                    continue;
                }
            }
        }
        p += 1;
    }
    out.sort_by_key(|(_, s)| *s);
    out
}

/// page_index（0-based）→ 所属 spine 文件 basename：起始页 ≤ page 的最后一条。
pub fn page_section<'a>(sections: &'a [(String, u32)], page: usize) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for (base, start) in sections {
        if (*start as usize) <= page {
            best = Some(base);
        } else {
            break; // 已按起始页升序
        }
    }
    best
}

/// 去标签取纯文本（nav/ncx 标题里可能夹 <span> 等）。
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 从 nav.xhtml 的 `<a href=..>标题</a>` 抽 basename→标题。
fn parse_nav_xhtml(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let re = regex::Regex::new(r#"(?is)<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap();
    for cap in re.captures_iter(text) {
        let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title = strip_tags(cap.get(2).map(|m| m.as_str()).unwrap_or(""));
        let base = href.split('#').next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
        if !base.is_empty() && !title.is_empty() {
            map.entry(base).or_insert(title);
        }
    }
    map
}

/// 从 toc.ncx 的 navPoint（<text>标题</text> … <content src=".."/>）抽 basename→标题。
fn parse_ncx(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let re = regex::Regex::new(r#"(?is)<navpoint\b.*?<text>(.*?)</text>.*?<content\s+src="([^"]+)""#).unwrap();
    for cap in re.captures_iter(text) {
        let title = strip_tags(cap.get(1).map(|m| m.as_str()).unwrap_or(""));
        let href = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let base = href.split('#').next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
        if !base.is_empty() && !title.is_empty() {
            map.entry(base).or_insert(title);
        }
    }
    map
}

/// 定位并读出 .epub（zip）里的 (nav.xhtml 文本, toc.ncx 文本)。
fn read_toc_texts(epub: &[u8]) -> (Option<String>, Option<String>) {
    let mut ar = match ZipArchive::new(Cursor::new(epub)) {
        Ok(a) => a,
        Err(_) => return (None, None),
    };
    let mut nav_name = None;
    let mut ncx_name = None;
    for i in 0..ar.len() {
        if let Ok(f) = ar.by_index(i) {
            let n = f.name().to_string();
            let nl = n.to_lowercase();
            if nl.ends_with("nav.xhtml") || (nl.contains("nav") && nl.ends_with(".xhtml")) {
                nav_name.get_or_insert(n);
            } else if nl.ends_with(".ncx") {
                ncx_name.get_or_insert(n);
            }
        }
    }
    let read = |ar: &mut ZipArchive<Cursor<&[u8]>>, name: &str| -> Option<String> {
        let mut f = ar.by_name(name).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let nav = nav_name.and_then(|n| read(&mut ar, &n));
    let ncx = ncx_name.and_then(|n| read(&mut ar, &n));
    (nav, ncx)
}

/// 大小写不敏感找 ASCII pattern 的首个字节位置（pattern 全 ASCII → 返回值是有效 char 边界）。
fn find_ci(s: &str, pat: &[u8]) -> Option<usize> {
    s.as_bytes().windows(pat.len()).position(|w| w.eq_ignore_ascii_case(pat))
}

/// 取某页(0-based)所属 spine 文件的**正文纯文本**（去标签）。供生词本查词取原句上下文用。
/// `page → basename`（`page_section`）→ 在 zip 里按 basename 匹配 spine 条目 → 读出 → 截到 `<body>`
/// 之后（去掉 head/title/style 污染）→ `strip_tags`。找不到章/文件/解析失败/空文本 → None。
///
/// 注：`strip_tags` 会把所有空白折叠成单空格 → 章节文本丢换行，但中英句末标点（。！？.!?）保留，
/// `cardvocab::sentence_of` 靠标点扩句仍成立。
pub fn page_fulltext(epub: &[u8], sections: &[(String, u32)], page: usize) -> Option<String> {
    let base = page_section(sections, page)?;
    let mut ar = ZipArchive::new(Cursor::new(epub)).ok()?;
    // zip 条目名可能带路径前缀，按 basename 匹配。
    let mut target: Option<String> = None;
    for i in 0..ar.len() {
        if let Ok(f) = ar.by_index(i) {
            let n = f.name();
            if n.rsplit('/').next().unwrap_or(n) == base {
                target = Some(n.to_string());
                break;
            }
        }
    }
    let name = target?;
    let mut f = ar.by_name(&name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    let off = find_ci(&s, b"<body").unwrap_or(0);
    let text = strip_tags(&s[off..]);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// 读 .epub（zip）→ spine 文件 basename → 章节标题（优先 nav.xhtml，退回 toc.ncx）。
pub fn chapter_map(epub: &[u8]) -> HashMap<String, String> {
    let (nav, ncx) = read_toc_texts(epub);
    if let Some(t) = nav {
        let m = parse_nav_xhtml(&t);
        if !m.is_empty() {
            return m;
        }
    }
    if let Some(t) = ncx {
        return parse_ncx(&t);
    }
    HashMap::new()
}

/// 层级解析 nav.xhtml：basename → (标题, 父章节标题 option)。
/// 子 `<ol>` 内的条目父 = 外层 `<li>` 的 `<a>` 标题（章/部）；顶层条目父 = None。
/// 用一个正则按出现顺序抓 `<ol>` / `</ol>` / `<a href=..>标题</a>` 三类事件，
/// 靠 ol 嵌套深度维护父栈（parent stack）。
fn parse_nav_xhtml_hier(text: &str) -> HashMap<String, (String, Option<String>)> {
    let mut map: HashMap<String, (String, Option<String>)> = HashMap::new();
    let re = regex::Regex::new(r#"(?is)(<ol\b)|(</ol\s*>)|<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap();
    let mut ol_depth: i32 = 0;
    let mut stack: Vec<String> = Vec::new(); // 每层嵌套 <ol> 所属的父标题
    let mut last_anchor: Option<String> = None;
    for cap in re.captures_iter(text) {
        if cap.get(1).is_some() {
            // <ol> 开：根 ol(depth→1) 不入栈；嵌套 ol(depth≥2) 把刚见的锚点作为父入栈
            ol_depth += 1;
            if ol_depth >= 2 {
                stack.push(last_anchor.clone().unwrap_or_default());
            }
        } else if cap.get(2).is_some() {
            // </ol> 闭
            if ol_depth >= 2 {
                stack.pop();
            }
            if ol_depth > 0 {
                ol_depth -= 1;
            }
        } else {
            let href = cap.get(3).map(|m| m.as_str()).unwrap_or("");
            let title = strip_tags(cap.get(4).map(|m| m.as_str()).unwrap_or(""));
            let base = href.split('#').next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            let parent = stack.last().filter(|s| !s.is_empty()).cloned();
            if !base.is_empty() && !title.is_empty() {
                map.entry(base).or_insert((title.clone(), parent));
            }
            last_anchor = Some(title);
        }
    }
    map
}

/// 读 .epub → basename → (标题, 父章节 option)。优先 nav.xhtml **层级**解析，
/// 退回 toc.ncx **扁平**（父=None——ncx 嵌套暂不解析，我们自己组装的书都有 nav.xhtml）。
pub fn chapter_map_hier(epub: &[u8]) -> HashMap<String, (String, Option<String>)> {
    let (nav, ncx) = read_toc_texts(epub);
    if let Some(t) = &nav {
        let m = parse_nav_xhtml_hier(t);
        if !m.is_empty() {
            return m;
        }
    }
    if let Some(t) = &ncx {
        return parse_ncx(t).into_iter().map(|(k, v)| (k, (v, None))).collect();
    }
    HashMap::new()
}

/// 页号（0-based）→ 章节标签：有父章则「父 - 自身」（章 - 节），否则只「自身」（章）。
pub fn page_chapter_label(
    sections: &[(String, u32)],
    hier: &HashMap<String, (String, Option<String>)>,
    page: usize,
) -> Option<String> {
    let base = page_section(sections, page)?;
    let (title, parent) = hier.get(base)?;
    Some(match parent {
        Some(p) => format!("{p} - {title}"),
        None => title.clone(),
    })
}

/// 页号（0-based）→ 章节标题。sections 来自 parse_sections，titles 来自 chapter_map。抽不到返回 None。
pub fn page_chapter(sections: &[(String, u32)], titles: &HashMap<String, String>, page: usize) -> Option<String> {
    let base = page_section(sections, page)?;
    titles.get(base).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sections_from_real_fixture() {
        let bytes = include_bytes!("../tests/fixtures/redemption.epubindex");
        let secs = parse_sections(bytes);
        // 真机《赎罪》.epubindex：Section01 起 page2, 02 起 39, 03 起 73, 04 起 113, 05 起 152, 06 起 194
        let get = |name: &str| secs.iter().find(|(b, _)| b == name).map(|(_, s)| *s);
        assert_eq!(get("Section01.xhtml"), Some(2));
        assert_eq!(get("Section02.xhtml"), Some(39));
        assert_eq!(get("Section03.xhtml"), Some(73));
        assert_eq!(get("Section06.xhtml"), Some(194));
        // 页 → section
        assert_eq!(page_section(&secs, 40), Some("Section02.xhtml"));
        assert_eq!(page_section(&secs, 73), Some("Section03.xhtml"));
        assert_eq!(page_section(&secs, 200), Some("Section06.xhtml"));
    }

    #[test]
    fn nav_and_ncx_parse() {
        let nav = r#"<nav><ol>
          <li><a href="xhtml/Section01.xhtml">洋娃娃</a></li>
          <li><a href="xhtml/Section03.xhtml">狗熊兄妹</a></li>
        </ol></nav>"#;
        let m = parse_nav_xhtml(nav);
        assert_eq!(m.get("Section01.xhtml").map(|s| s.as_str()), Some("洋娃娃"));
        assert_eq!(m.get("Section03.xhtml").map(|s| s.as_str()), Some("狗熊兄妹"));

        let ncx = r#"<navMap><navPoint><navLabel><text>临时家长会</text></navLabel>
          <content src="xhtml/Section02.xhtml"/></navPoint></navMap>"#;
        let m2 = parse_ncx(ncx);
        assert_eq!(m2.get("Section02.xhtml").map(|s| s.as_str()), Some("临时家长会"));
    }

    #[test]
    fn page_chapter_combined() {
        let bytes = include_bytes!("../tests/fixtures/redemption.epubindex");
        let secs = parse_sections(bytes);
        let mut titles = HashMap::new();
        titles.insert("Section03.xhtml".to_string(), "狗熊兄妹".to_string());
        assert_eq!(page_chapter(&secs, &titles, 80).as_deref(), Some("狗熊兄妹")); // page80 ∈ [73,113)
    }

    // 真机《Tell Me Your Dreams》nav 骨架：Book One/Two/Three 各带子 <ol> 的 Chapter，
    // 前后夹若干顶层条目（Title Page/Dedication/Author's Note...）。
    const NAV_NESTED: &str = r#"<nav epub:type="toc"><ol>
      <li><a href="chap_0002.xhtml">Dedication</a></li>
      <li><a href="chap_0003.xhtml">Book One</a>
        <ol>
          <li><a href="chap_0004.xhtml">Chapter One</a></li>
          <li><a href="chap_0005.xhtml">Chapter Two</a></li>
        </ol>
      </li>
      <li><a href="chap_0014.xhtml">Book Two</a>
        <ol>
          <li><a href="chap_0015.xhtml">Chapter Eleven</a></li>
        </ol>
      </li>
      <li><a href="chap_0035.xhtml">Author’s Note</a></li>
    </ol></nav>"#;

    #[test]
    fn nav_hier_parent_child() {
        let m = parse_nav_xhtml_hier(NAV_NESTED);
        // 子节：父 = 外层 <li> 的章
        assert_eq!(m.get("chap_0004.xhtml").cloned(), Some(("Chapter One".into(), Some("Book One".into()))));
        assert_eq!(m.get("chap_0005.xhtml").cloned(), Some(("Chapter Two".into(), Some("Book One".into()))));
        assert_eq!(m.get("chap_0015.xhtml").cloned(), Some(("Chapter Eleven".into(), Some("Book Two".into()))));
        // 章自身（父项文件）+ 顶层条目：父 = None
        assert_eq!(m.get("chap_0003.xhtml").cloned(), Some(("Book One".into(), None)));
        assert_eq!(m.get("chap_0002.xhtml").cloned(), Some(("Dedication".into(), None)));
        assert_eq!(m.get("chap_0035.xhtml").cloned(), Some(("Author’s Note".into(), None)));
    }

    #[test]
    fn page_label_nested_and_flat() {
        // 嵌套：spine 起始页人造表，验证 page→「章 - 节」
        let secs = vec![
            ("chap_0003.xhtml".to_string(), 5u32),  // Book One 首页
            ("chap_0004.xhtml".to_string(), 6),     // Chapter One
            ("chap_0005.xhtml".to_string(), 20),    // Chapter Two
            ("chap_0035.xhtml".to_string(), 200),   // Author's Note（顶层）
        ];
        let hier = parse_nav_xhtml_hier(NAV_NESTED);
        assert_eq!(page_chapter_label(&secs, &hier, 5).as_deref(), Some("Book One")); // 章自身页
        assert_eq!(page_chapter_label(&secs, &hier, 10).as_deref(), Some("Book One - Chapter One")); // ∈[6,20)
        assert_eq!(page_chapter_label(&secs, &hier, 25).as_deref(), Some("Book One - Chapter Two")); // ≥20
        assert_eq!(page_chapter_label(&secs, &hier, 210).as_deref(), Some("Author’s Note")); // 顶层无父

        // 扁平书（《赎罪》全平级章）：page_chapter_label 退化成只章名，无 " - "
        let flat = parse_nav_xhtml(r#"<ol><li><a href="xhtml/Section03.xhtml">狗熊兄妹</a></li></ol>"#);
        let flat_hier: HashMap<String, (String, Option<String>)> =
            flat.into_iter().map(|(k, v)| (k, (v, None))).collect();
        let rsecs = parse_sections(include_bytes!("../tests/fixtures/redemption.epubindex"));
        assert_eq!(page_chapter_label(&rsecs, &flat_hier, 80).as_deref(), Some("狗熊兄妹"));
    }

    #[test]
    fn page_fulltext_extracts_body_only() {
        use std::io::Write;
        // 造最小 EPUB zip：一个 spine xhtml（含 head/title 污染 + body 正文，路径带前缀）。
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opt =
                zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zw.start_file("OEBPS/chap1.xhtml", opt).unwrap();
            zw.write_all(
                "<html><head><title>不该出现的标题</title></head>\
                 <body><p>骆督察一直很讨厌医院的气味。</p><p>窗外下着雨。</p></body></html>"
                    .as_bytes(),
            )
            .unwrap();
            zw.finish().unwrap();
        }
        let secs = vec![("chap1.xhtml".to_string(), 0u32)];
        let text = page_fulltext(&buf, &secs, 0).unwrap();
        assert!(text.contains("骆督察一直很讨厌医院的气味。"));
        assert!(text.contains("窗外下着雨。"));
        assert!(!text.contains("不该出现的标题"), "head/title 应被 <body> 截断排除");
        assert!(page_fulltext(&[1, 2, 3], &secs, 0).is_none(), "非 zip → None");
    }
}
