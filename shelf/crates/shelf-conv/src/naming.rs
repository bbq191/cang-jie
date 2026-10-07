//! 母版库的书名规范化补充：书名本身的规范化（`书名 - N卷`，用户 2026-09-20 拍板）用 sheng-ren 的
//! [`bookconv::naming::canonical_book_name`]；这里只有 book-serve 额外要的两个：带扩展名的文件名版本、"名字里有没有卷标记"。
//! 卷标记的识别规则同原 cang-jie `bookconv::naming`（第一个 ` -- ` 之前的段，去掉尾部完结标记后看末尾）。

use regex::Regex;
use std::sync::OnceLock;

const NUM: &str = r"[0-9０-９一二三四五六七八九十百零〇两]+";

fn marker_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // 标记：第N卷/部/册/集/话/回/季/篇 | 卷N | N卷 | 上/中/下(+册/部/卷/篇) | Vol.N
        let marker = format!(
            r"(?:第\s*{NUM}\s*[卷部册集话回季篇]|[卷部册集]\s*{NUM}|{NUM}\s*[卷部册集]|[上中下](?:[册部卷篇])?|[Vv][Oo][Ll]\.?\s*[0-9]+)"
        );
        // 三种连接：括号包裹 | 分隔符/空白 | 无分隔紧贴（仅限带"卷/部/册"字样的标记，避免误伤"世界上"）
        Regex::new(&format!(
            r"^(?P<t>.+?)\s*(?:[（(]\s*(?P<m1>{marker})\s*[)）]|(?:[-–—_·:：]\s*|\s+)(?P<m2>{marker})|(?P<m3>第\s*{NUM}\s*[卷部册集]|[卷部册]\s*{NUM}|[Vv][Oo][Ll]\.?\s*[0-9]+))\s*$"
        ))
        .unwrap()
    })
}

fn tail_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s*[\[（(【]\s*(?:完结版|完结|完|全|终)\s*[\]）)】]\s*$").unwrap())
}

/// 这个名字（不含扩展名）里是否有可识别的卷标记。优化时只在有卷标记的书上把 EPUB 自己的 `dc:title` 改成
/// 规范名（避免把 `abc123.epub` 这种无意义文件名覆盖掉书里本来正确的书名）。
pub fn has_volume_marker(stem: &str) -> bool {
    let first = stem.trim().split(" -- ").next().unwrap_or("").trim();
    let first = tail_tag_re().replace(first, "");
    marker_re().is_match(first.trim())
}

/// 带扩展名的文件名版本：`x -- y.epub` → `x.epub`。扩展名原样保留。
pub fn canonical_file_name(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !ext.is_empty() && ext.len() <= 5 => format!("{}.{ext}", bookconv::naming::canonical_book_name(stem)),
        _ => bookconv::naming::canonical_book_name(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_keeps_extension_and_handles_dots() {
        assert_eq!(canonical_file_name("鏢人 - 卷02 -- 許先哲 -- Anna’s Archive.epub"), "鏢人 - 02卷.epub");
        assert_eq!(canonical_file_name("plain.epub"), "plain.epub");
        assert_eq!(canonical_file_name("no_ext"), "no_ext");
        // 书名里带点（英文缩写）不是扩展名分隔时也不崩：末段超过 5 字符视为书名的一部分。
        assert_eq!(canonical_file_name("Dr. Who Long Title"), "Dr. Who Long Title");
    }

    #[test]
    fn has_volume_marker_only_for_real_markers() {
        assert!(has_volume_marker("鏢人 - 卷02 -- 許先哲"));
        assert!(has_volume_marker("雪人 - 上册"));
        assert!(has_volume_marker("火影忍者 (第3卷) [完]"));
        assert!(!has_volume_marker("abc123"));
        assert!(!has_volume_marker("疯探-空城"));
    }
}
