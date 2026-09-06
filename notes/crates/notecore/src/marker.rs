//! 行首手写标记（OCR 路）：转写文本开头的 `-`/`•` = 无序、`1.` = 有序、`□`/`口` = 待办。
//! 几何判定（ink-serve）优先；这里兜底：几何没认出来（仍是 Body）时，用转写结果再认一次，并把标记从正文剥掉
//! （笔记本样式自带项目符号/编号，正文里再留一份会重复）。多行文本只看第一行的行首。
use crate::model::Style;

/// 拆出行首标记：返回（认出的样式, 去掉标记后的文本）。认不出 → `(None, 原文)`。
pub fn split_leading_marker(text: &str) -> (Option<Style>, String) {
    let t = text.trim_start();
    let (first, rest) = match t.find('\n') {
        Some(i) => (&t[..i], &t[i..]),
        None => (t, ""),
    };
    let f = first.trim_start();
    let strip = |body: &str| format!("{}{}", body.trim_start(), rest);
    // 待办：空心方框（真方框或手写成的「口」字）
    for m in ["□", "☐", "口"] {
        if let Some(b) = f.strip_prefix(m) {
            if m != "口" || b.starts_with([' ', '\u{3000}']) || b.is_empty() {
                return (Some(Style::Checkbox), strip(b));
            }
        }
    }
    // 无序：短横 / 实心点 / 项目符号 + 空格（避免把负数、连字符吃掉）
    for m in ["- ", "• ", "· ", "* ", "—", "－"] {
        if let Some(b) = f.strip_prefix(m) {
            return (Some(Style::Bullet), strip(b));
        }
    }
    // 有序：`1.` `2、` `3)` `4．`
    let digits: String = f.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let after = &f[digits.len()..];
        for p in [".", "、", ")", "）", "．"] {
            if let Some(b) = after.strip_prefix(p) {
                return (Some(Style::Numbered), strip(b));
            }
        }
    }
    (None, text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_three_markers_and_strips() {
        assert_eq!(split_leading_marker("- 查作者"), (Some(Style::Bullet), "查作者".into()));
        assert_eq!(split_leading_marker("• 查作者\n第二行"), (Some(Style::Bullet), "查作者\n第二行".into()));
        assert_eq!(split_leading_marker("1. 背诵"), (Some(Style::Numbered), "背诵".into()));
        assert_eq!(split_leading_marker("12、要点"), (Some(Style::Numbered), "要点".into()));
        assert_eq!(split_leading_marker("□ 找原文"), (Some(Style::Checkbox), "找原文".into()));
        assert_eq!(split_leading_marker("口 找原文"), (Some(Style::Checkbox), "找原文".into()));
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(split_leading_marker("口渴了"), (None, "口渴了".into()), "「口」后面直接是字＝正常词");
        assert_eq!(split_leading_marker("-3 度"), (None, "-3 度".into()), "负数不是无序");
        assert_eq!(split_leading_marker("2024 年"), (None, "2024 年".into()));
        assert_eq!(split_leading_marker("没听懂"), (None, "没听懂".into()));
        assert_eq!(split_leading_marker(""), (None, String::new()));
    }
}
