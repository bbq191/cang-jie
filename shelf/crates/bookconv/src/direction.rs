//! 翻页方向：读 OPF `<spine page-progression-direction="rtl|ltr">`（2026-09-25）。
//!
//! 背景：xochitl 阅读器里的 `reader-page-turn.qmd` 按这个属性决定"日漫从右往左翻页"（book-serve
//! `GET /reading-direction/{uuid}`，见系统增强线白皮书 §03i）。**只读、保留原书自带的方向**：2026-09-25 起母版库曾能按书
//! 手动指定、「优化」时写进 OPF，2026-09-30 用户定撤掉（写入函数一并删除）。
//!
//! 不自动判：漫画识别（`comic_detect`）只能看出"是漫画"，看不出"是日漫"——国漫、美漫是从左往右，
//! 自动设 rtl 会把它们翻反（规范白皮书 §4.6）。
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// 书的翻页方向（EPUB 3 `page-progression-direction` 的两个显式值；`default` 与缺省按 `Ltr` 以外的"未写"处理）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageDirection {
    /// 从左往右（普通书、国漫、美漫）。
    Ltr,
    /// 从右往左（日漫）。
    Rtl,
}

impl PageDirection {
    /// 写进 OPF 的属性值。
    pub fn as_str(self) -> &'static str {
        match self {
            PageDirection::Ltr => "ltr",
            PageDirection::Rtl => "rtl",
        }
    }
    /// `"rtl"`/`"ltr"` → 方向；其它（含 `"auto"`、`"default"`、空）→ `None`。
    pub fn parse(s: &str) -> Option<PageDirection> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rtl" => Some(PageDirection::Rtl),
            "ltr" => Some(PageDirection::Ltr),
            _ => None,
        }
    }
}

/// `<spine …>` 开标签（允许命名空间前缀如 `<opf:spine`，允许自闭合）。
fn spine_tag_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)<(?:[A-Za-z_][\w.-]*:)?spine\b[^>]*>"#).unwrap())
}

/// 开标签里的 `page-progression-direction="…"`（单双引号都认）。
fn attr_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)(\s)page-progression-direction\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap())
}

/// 读 OPF 文本里写明的方向：`rtl`/`ltr` → `Some`；没写、写 `default` 或别的值 → `None`。
pub fn spine_direction(opf: &str) -> Option<PageDirection> {
    let tag = spine_tag_re().find(opf)?.as_str();
    let c = attr_re().captures(tag)?;
    PageDirection::parse(c.get(2).or_else(|| c.get(3)).map_or("", |m| m.as_str()))
}

/// 读 EPUB 文件里写明的方向（只读 container.xml 与 OPF 两个条目）。读不了 / 没写 → `None`。
pub fn spine_direction_file(epub: &Path) -> Option<PageDirection> {
    let (_, _, opf) = crate::placeholder::open_opf(epub).ok()?;
    spine_direction(&opf)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_explicit_values_only() {
        assert_eq!(spine_direction(r#"<package><spine toc="ncx" page-progression-direction="rtl"/></package>"#), Some(PageDirection::Rtl));
        assert_eq!(spine_direction(r#"<spine page-progression-direction='ltr'>"#), Some(PageDirection::Ltr));
        assert_eq!(spine_direction(r#"<opf:spine page-progression-direction="rtl">"#), Some(PageDirection::Rtl));
        assert_eq!(spine_direction(r#"<spine page-progression-direction="default">"#), None);
        assert_eq!(spine_direction(r#"<spine toc="ncx">"#), None);
        assert_eq!(spine_direction("<package/>"), None);
        // 只看 spine 开标签，不被正文别处的同名字样骗到
        assert_eq!(spine_direction(r#"<meta content='page-progression-direction="rtl"'/><spine toc="ncx">"#), None);
    }


}
