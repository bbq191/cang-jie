//! OPF 视图：定位 OPF、解出 manifest/spine/nav/ncx，以及 OPF 里的标识符/书名。
use super::*;

pub(super) fn find_opf(entries: &[Entry]) -> Option<usize> {
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

// ───────────────────────── OPF 视图 ─────────────────────────

pub(crate) struct Opf {
    pub(crate) index: usize,
    pub(crate) dir: String,
    /// manifest id → zip 路径
    #[allow(dead_code)]
    pub(crate) items: HashMap<String, String>,
    /// spine 顺序的 zip 路径
    pub(crate) spine: Vec<String>,
    pub(crate) nav_doc: Option<String>,
    #[allow(dead_code)]
    pub(crate) ncx: Option<String>,
}

pub(crate) fn parse_opf(entries: &[Entry]) -> Option<Opf> {
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

/// OPF 的 `unique-identifier` 实际取值（`<package unique-identifier="X">` 指向的那个
/// `<dc:identifier id="X">` 元素的文本内容）。EPUB2 规范要求 `toc.ncx` 的 `dtb:uid` 跟这个值
/// 完全一致——真机《疯探》坐实：这本"番茄小说 EPUB Generator"产物的 `toc.ncx` navMap 结构完全
/// 正确（94 条 navPoint 全部可达），但 `dtb:uid` 是生成器随手写的另一个 uuid，跟 OPF 的
/// `dc:identifier` 对不上；reMarkable 原生目录面板遇到这种不匹配**直接不显示目录入口**（不是
/// 显示空列表），换一本 `dtb:uid` 匹配的书（《雪人》）目录入口就在。见 `fix_ncx_uid`。
pub(super) fn opf_unique_identifier(entries: &[Entry]) -> Option<String> {
    let i = find_opf(entries)?;
    let text = String::from_utf8_lossy(&entries[i].data);
    static PKG: OnceLock<Regex> = OnceLock::new();
    let pkg_re = PKG.get_or_init(|| Regex::new(r#"<package\b[^>]*\bunique-identifier="([^"]+)""#).unwrap());
    let uid_attr = &pkg_re.captures(&text)?[1];
    static ID: OnceLock<Regex> = OnceLock::new();
    let id_re = ID.get_or_init(|| Regex::new(r#"(?s)<dc:identifier\b[^>]*\bid="([^"]+)"[^>]*>([^<]*)</dc:identifier>"#).unwrap());
    id_re.captures_iter(&text).find(|c| &c[1] == uid_attr).map(|c| c[2].trim().to_string())
}

/// OPF `<dc:title>` 的纯文本内容，取不到时兜底"目录"。
pub(super) fn opf_book_title(entries: &[Entry], opf_index: usize) -> String {
    let t = String::from_utf8_lossy(&entries[opf_index].data);
    static T: OnceLock<Regex> = OnceLock::new();
    T.get_or_init(|| Regex::new(r#"(?s)<dc:title[^>]*>(.*?)</dc:title>"#).unwrap()).captures(&t).map(|c| plain_text(&c[1])).unwrap_or_else(|| "目录".into())
}
