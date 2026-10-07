//! 读 EPUB 的几样小事（[`Book`]）：找 OPF、书名/作者/语言、封面图、sheng-ren 的漫画页边距标记，外加写占位 EPUB 的 zip。
//!
//! 2026-10-07 起书架不优化书，只把书原样投进 xochitl，读书只剩这几样；以前借用 sheng-ren `bookconv` 的公开接口，
//! 它连带的图片处理、网页正文抽取、HTTP 客户端等依赖书架一样都用不上，同日改成这里的最小实现（只读 container.xml、
//! OPF 和封面要用的少数几个条目，从不解压整本）。标签扫描用正则：先去掉注释，属性认双引号、单引号、无引号三种写法，
//! 元素名认命名空间前缀（`<opf:item>`）。
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::sync::OnceLock;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// sheng-ren 优化器写进漫画的标记条目（内容是阅读器该设的页边距，xochitl 模式写 `1`）。
/// **必须与 sheng-ren `bookconv::optimize::READER_MARGINS_MARKER` 一致**——两边各写一份字符串，改名要同时改。
pub const READER_MARGINS_MARKER: &str = "META-INF/eink-reader-margins";

/// 单个条目解压后的上限：几 KB 的压缩数据能解出几 GB（zip 炸弹），目录里声明的大小也可以造假，按实际解出的字节数截。
/// 这里读的只有 container.xml、OPF、几页正文和一张封面，256MB 远超真实需要。
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

type FileZip = ZipArchive<std::io::BufReader<std::fs::File>>;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

/// 读一个条目的全部字节，解出超过 `cap` 字节返回 `Ok(None)`（预分配按声明大小，封顶 32MB）。
fn read_capped(r: impl Read, cap: u64, declared: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::with_capacity(declared.min(cap).min(32 << 20) as usize);
    r.take(cap + 1).read_to_end(&mut buf)?;
    Ok((buf.len() as u64 <= cap).then_some(buf))
}

/// 按名读条目；不存在 → `Ok(None)`，超过 [`MAX_ENTRY_BYTES`] 或读失败 → `Err`。
fn read_by_name<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let size = f.size();
    read_capped(f, MAX_ENTRY_BYTES, size)
        .map_err(|e| format!("{name}: {e}"))?
        .map(Some)
        .ok_or_else(|| format!("{name}: 解压后超过单个条目上限 {} MB（损坏或恶意的压缩包？）", MAX_ENTRY_BYTES >> 20))
}

fn read_text<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Option<String> {
    read_by_name(zip, name).ok().flatten().map(|b| String::from_utf8(b).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// 去掉 `<!-- … -->` 注释（注释里的标签不算数）。
fn strip_comments(s: &str) -> Cow<'_, str> {
    static C: OnceLock<Regex> = OnceLock::new();
    re(&C, r"(?s)<!--.*?-->").replace_all(s, "")
}

/// 文本里所有名为 `local`（不分大小写，认 `前缀:local`）的开标签/自闭合标签原文，文档序。
fn start_tags<'a>(text: &'a str, local: &str) -> Vec<&'a str> {
    static TAG: OnceLock<Regex> = OnceLock::new();
    re(&TAG, r"<([A-Za-z_][-\w.]*:)?([A-Za-z_][-\w.]*)\b[^>]*>")
        .captures_iter(text)
        .filter(|c| c[2].eq_ignore_ascii_case(local))
        .map(|c| c.get(0).unwrap().as_str())
        .collect()
}

/// 标签里名为 `name`（完整属性名、不分大小写）的属性原文值。
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    static A: OnceLock<Regex> = OnceLock::new();
    // 跳过开头的 `<元素名`，免得把元素名当属性
    let body = tag.find(|c: char| c.is_whitespace()).map_or("", |i| &tag[i..]);
    // 无引号的值可以含 `/`（`media-type=application/xhtml+xml`），但不以 `/` 结尾（那是自闭合的 `/>`）
    re(&A, r#"([A-Za-z_:][-\w:.]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]*[^\s"'>/]))"#)
        .captures_iter(body)
        .find(|c| c[1].eq_ignore_ascii_case(name))
        .map(|c| c.get(2).or(c.get(3)).or(c.get(4)).unwrap().as_str())
}

/// XML 字符引用还原：`&amp; &lt; &gt; &quot; &apos;`、`&#NNN;`、`&#xHH;`；认不出的原样保留。
fn xml_unescape(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    static E: OnceLock<Regex> = OnceLock::new();
    re(&E, r"&(#[xX][0-9A-Fa-f]{1,6}|#[0-9]{1,7}|amp|lt|gt|quot|apos|nbsp);").replace_all(s, |c: &regex::Captures| {
        let ent = &c[1];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .map(|h| u32::from_str_radix(h, 16))
                .unwrap_or_else(|| ent[1..].parse::<u32>())
                .ok()
                .and_then(char::from_u32),
        };
        ch.map_or_else(|| c[0].to_string(), String::from)
    })
}

/// XML 文本/属性转义（`& < > "`），并丢掉 XML 1.0 不允许的字符——转义救不了它们，留着整份文档就不是合法 XML
/// （真机踩过：PDF 标题按错误编码解出一串 `\0`，写进 OPF 后 xochitl 整本只渲染 1 页）。
pub(crate) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// 元素内容 → 纯文本：去标签（含 CDATA 包装）、字符引用还原、空白折叠成单个空格。写回 XML 时调用方要再 [`xml_escape`]。
fn plain_text(fragment: &str) -> String {
    static TAGS: OnceLock<Regex> = OnceLock::new();
    let s = fragment.replace("<![CDATA[", "").replace("]]>", "");
    let s = re(&TAGS, r"<[^>]*>").replace_all(&strip_comments(&s), "").into_owned();
    xml_unescape(&s).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 图片扩展名（小写，不带点）→ media-type；认不出的当 JPEG。
pub(crate) fn image_media_type_of_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/jpeg",
    }
}

fn is_image_ext(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".gif", ".webp"].iter().any(|e| l.ends_with(e))
}

/// 图片路径的扩展名（小写，只看文件名部分）；没有扩展名当 `jpg`。
fn image_ext_of(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    match base.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() => e.to_ascii_lowercase(),
        _ => "jpg".into(),
    }
}

/// zip 内 posix 路径：去掉 `.`、空段，`..` 回退一级。
fn posix_norm(p: &str) -> String {
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

fn dir_of(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

/// 百分号解码（`%20` 等）；解出的不是 UTF-8 就原样返回。
fn percent_decode(s: &str) -> Cow<'_, str> {
    if !s.contains('%') {
        return Cow::Borrowed(s);
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).map_or(Cow::Borrowed(s), Cow::Owned)
}

/// 链接原文（相对 `base_dir`，可能带 `#锚点`、字符引用、百分号编码）→ zip 路径。
fn resolve(base_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let p = percent_decode(&xml_unescape(href)).into_owned();
    if base_dir.is_empty() {
        posix_norm(&p)
    } else {
        posix_norm(&format!("{base_dir}/{p}"))
    }
}

/// 打开着的一本 EPUB：zip 中央目录只解一次，书名、封面、页边距标记都从这里取（此前投一本书要把 zip 打开两三次，
/// 图多的漫画中央目录不小）。只读 container.xml、OPF 和需要的少数几个条目，从不解压整本。
pub struct Book {
    zip: FileZip,
    /// `(OPF 所在目录, OPF 文本)`；缺 container.xml / OPF 时是 `Err(原因)`——页边距标记照样能读，书名为空，造不了占位。
    opf: Result<(String, String), String>,
}

impl Book {
    /// 打开并读出 OPF。不是 zip → `Err`；缺 container.xml / OPF 不算错（见 [`Book::opf`]）。
    pub fn open(epub: &Path) -> Result<Book, String> {
        let file = std::fs::File::open(epub).map_err(|e| format!("打开 {} 失败: {e}", epub.display()))?;
        let mut zip = ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("解 EPUB 失败: {e}"))?;
        let opf = Self::read_opf(&mut zip);
        Ok(Book { zip, opf })
    }

    fn read_opf(zip: &mut FileZip) -> Result<(String, String), String> {
        let container = read_text(zip, "META-INF/container.xml").ok_or("缺 META-INF/container.xml")?;
        let container = strip_comments(&container);
        let opf_path = start_tags(&container, "rootfile").into_iter().find_map(|t| attr(t, "full-path")).ok_or("container.xml 里没有 full-path")?;
        let opf_path = resolve("", opf_path);
        let opf = read_text(zip, &opf_path).ok_or("读不到 OPF")?;
        Ok((dir_of(&opf_path).to_string(), opf))
    }

    /// OPF 文本；缺 container.xml / OPF → `Err(原因)`。
    pub fn opf(&self) -> Result<&str, String> {
        self.opf.as_ref().map(|(_, t)| t.as_str()).map_err(String::clone)
    }

    /// OPF 里第一个非空的 Dublin Core 元素（`title`/`creator`/`language`…）的纯文本，见 [`dc_text`]。
    pub fn dc(&self, local: &str) -> Option<String> {
        dc_text(self.opf().ok()?, local)
    }

    /// `dc:title`：xochitl 进库后的显示名取自它。
    pub fn title(&self) -> Option<String> {
        self.dc("title")
    }

    /// 封面图（扩展名, 字节）；找不到 → `None`。见 [`cover_path`]。
    pub fn cover(&mut self) -> Option<(String, Vec<u8>)> {
        let (opf_dir, opf) = self.opf.as_ref().ok()?;
        let zip = &mut self.zip;
        let path = cover_path(opf, opf_dir, |p| read_text(zip, p))?;
        Some((image_ext_of(&path), read_by_name(&mut self.zip, &path).ok()??))
    }

    /// sheng-ren 写在漫画里的页边距（[`READER_MARGINS_MARKER`] 条目的内容）。没有、读不出、不是数字 → `None`。
    pub fn reader_margins(&mut self) -> Option<u32> {
        let entry = self.zip.by_name(READER_MARGINS_MARKER).ok()?;
        let bytes = read_capped(entry, 16, 0).ok()??;
        std::str::from_utf8(&bytes).ok()?.trim().parse().ok()
    }
}

/// OPF 里第一个非空的 Dublin Core 元素（`title`/`creator`/`language`…，认任意前缀）的纯文本。
fn dc_text(opf: &str, local: &str) -> Option<String> {
    static DC: OnceLock<Regex> = OnceLock::new();
    let opf = strip_comments(opf);
    re(&DC, r"(?s)<([A-Za-z_][-\w.]*):([A-Za-z_][-\w.]*)\b[^>]*?(?:/>|>(.*?)</[A-Za-z_][-\w.]*:[A-Za-z_][-\w.]*\s*>)")
        .captures_iter(&opf)
        .filter(|c| c[2].eq_ignore_ascii_case(local))
        .filter_map(|c| c.get(3).map(|m| plain_text(m.as_str())))
        .find(|s| !s.is_empty())
}

struct Item<'a> {
    id: &'a str,
    href: &'a str,
    media_type: &'a str,
    properties: &'a str,
}

impl Item<'_> {
    fn is_image(&self) -> bool {
        self.media_type.starts_with("image/") || is_image_ext(self.href.split('#').next().unwrap_or(""))
    }
}

fn manifest_items(opf: &str) -> Vec<Item<'_>> {
    start_tags(opf, "item")
        .into_iter()
        .filter_map(|t| Some(Item { id: attr(t, "id")?, href: attr(t, "href")?, media_type: attr(t, "media-type").unwrap_or(""), properties: attr(t, "properties").unwrap_or("") }))
        .collect()
}

/// 封面图的 zip 路径：OPF 声明的封面（`<meta name="cover" content="id">` 指向的图片项，其次 `properties="cover-image"`；
/// 指向 txt 之类的坏声明不算）；没有就取前 12 个 spine 页（跳过导航页）里第一张图。
fn cover_path(opf: &str, opf_dir: &str, mut read: impl FnMut(&str) -> Option<String>) -> Option<String> {
    let opf = strip_comments(opf);
    let items = manifest_items(&opf);
    let meta_ids: Vec<&str> = start_tags(&opf, "meta").into_iter().filter(|t| attr(t, "name") == Some("cover")).filter_map(|t| attr(t, "content")).collect();
    let declared = meta_ids
        .iter()
        .find_map(|id| items.iter().find(|i| i.id == *id && i.is_image()))
        .or_else(|| items.iter().find(|i| i.is_image() && i.properties.split_whitespace().any(|p| p == "cover-image")));
    if let Some(it) = declared {
        return Some(resolve(opf_dir, it.href));
    }
    let by_id: HashMap<&str, &Item> = items.iter().map(|i| (i.id, i)).collect();
    let pages = start_tags(&opf, "itemref")
        .into_iter()
        .filter_map(|t| by_id.get(attr(t, "idref")?).copied())
        .filter(|it| !it.properties.split_whitespace().any(|p| p == "nav"))
        .take(12);
    for it in pages {
        let page = resolve(opf_dir, it.href);
        let Some(text) = read(&page) else { continue };
        let text = strip_comments(&text);
        for tag in start_tags(&text, "img").into_iter().chain(start_tags(&text, "image")) {
            let Some(v) = ["src", "xlink:href", "href"].iter().find_map(|a| attr(tag, a)) else { continue };
            if v.is_empty() || v.starts_with('#') || v.contains("://") || v.starts_with("data:") {
                continue;
            }
            let path = resolve(dir_of(&page), v);
            if is_image_ext(&path) {
                return Some(path);
            }
        }
    }
    None
}

/// 写 EPUB：先写 `mimetype`（第一个条目、STORED，OCF 规范要求），之后图片 STORED、其余 deflate。
pub(crate) struct EpubWriter<W: Write + Seek> {
    zw: ZipWriter<W>,
}

impl<W: Write + Seek> EpubWriter<W> {
    pub(crate) fn new(w: W) -> Result<Self, String> {
        let mut me = EpubWriter { zw: ZipWriter::new(w) };
        me.put_with("mimetype", CompressionMethod::Stored, b"application/epub+zip")?;
        Ok(me)
    }

    pub(crate) fn put(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        let m = if is_image_ext(name) { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        self.put_with(name, m, data)
    }

    fn put_with(&mut self, name: &str, m: CompressionMethod, data: &[u8]) -> Result<(), String> {
        self.zw.start_file(name, SimpleFileOptions::default().compression_method(m)).map_err(|e| e.to_string())?;
        self.zw.write_all(data).map_err(|e| e.to_string())
    }

    pub(crate) fn finish(self) -> Result<W, String> {
        let mut w = self.zw.finish().map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(w)
    }
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
        let opf = r#"<package><!-- <opf:item id="x" href="old.xhtml"/> --><opf:item id="y" href="new.xhtml"/></package>"#;
        let items = manifest_items(&strip_comments(opf)).into_iter().map(|i| i.href.to_string()).collect::<Vec<_>>();
        assert_eq!(items, ["new.xhtml"], "注释里的不算、带前缀");
    }

    #[test]
    fn dc_text_decodes_and_skips_empty() {
        let opf = r#"<metadata><dc:title/><dc:title>  </dc:title><dc:title>Tom &amp; <i>Jerry</i> &#20013;</dc:title><dc11:creator><![CDATA[A & B]]></dc11:creator></metadata>"#;
        assert_eq!(dc_text(opf, "title").as_deref(), Some("Tom & Jerry 中"));
        assert_eq!(dc_text(opf, "creator").as_deref(), Some("A & B"));
        assert_eq!(dc_text(opf, "language"), None);
    }

    #[test]
    fn escape_and_unescape() {
        assert_eq!(xml_unescape("a&amp;b&lt;&#x4E2D;&#25991;&bogus;&"), "a&b<中文&bogus;&");
        assert_eq!(xml_escape("A & <B> \"q\"\u{0}\u{1}"), "A &amp; &lt;B&gt; &quot;q&quot;");
    }

    #[test]
    fn resolve_paths() {
        assert_eq!(resolve("OEBPS/Text", "../images/a%20b.jpg#x"), "OEBPS/images/a b.jpg");
        assert_eq!(resolve("", "./content.opf"), "content.opf");
        assert_eq!(resolve("OEBPS", "a&amp;b.xhtml"), "OEBPS/a&b.xhtml");
    }
}
