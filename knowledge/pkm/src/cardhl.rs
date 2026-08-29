//! 荧光笔高亮 → 卡片色槽（类型性映射）。
//!
//! 数据源：EPUB 页 `.rm` 的 `SceneGlyphItem`（GlyphRange），自带高亮**文字** + 颜色。
//! 判色（2026-08-25 真机《13 67》干净样本坐实，见白皮书）：6 支荧光笔映射到 6 个**固定色槽**，
//! 槽序固定 = Yellow/Blue/Pink/Orange/Green/Gray（0..6）。颜色分两种编码：
//! - `PenColor::Yellow/Pink/Green/GreyOverlap` → 直接判（色 1/3/5/6），其 `color_rgba` 是占位黑，忽略。
//! - `PenColor::Unknown(9)`(HIGHLIGHT) → 靠 `color_rgba` 区分：浅蓝(190,234,254)=Blue、橙(255,195,140)=Orange。
//! 其它颜色（黑/红/普通蓝笔等）不属于这 6 色 → 不摄取。
//!
//! 类型性（非位置性）：颜色语义跨模板固定，画什么色进哪个槽与书的 Tag 无关；Tag 只改**槽提示词**。
//! 一段高亮进唯一一个槽（多对一收敛，不重复）；同色多段 = 该槽多行。

use remarkable_lines::shared::pen_color::PenColor;
use remarkable_lines::v6::block::Block;
use remarkable_lines::RemarkableFile;

/// 6 个固定色槽，序号即 `slot()`（与用户定义的颜色序一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlColor {
    Yellow, // 0
    Blue,   // 1
    Pink,   // 2
    Orange, // 3
    Green,  // 4
    Gray,   // 5
}

pub const SLOT_COUNT: usize = 6;

impl HlColor {
    pub fn slot(self) -> usize {
        match self {
            HlColor::Yellow => 0,
            HlColor::Blue => 1,
            HlColor::Pink => 2,
            HlColor::Orange => 3,
            HlColor::Green => 4,
            HlColor::Gray => 5,
        }
    }

    /// 从 GlyphRange 的 `(color, color_rgba)` 判色槽；非 6 色返回 None（不摄取）。
    pub fn from_glyph(color: &PenColor, rgba: Option<(u8, u8, u8, u8)>) -> Option<HlColor> {
        match color {
            PenColor::Yellow => Some(HlColor::Yellow),
            PenColor::Pink => Some(HlColor::Pink),
            PenColor::Green => Some(HlColor::Green),
            PenColor::GreyOverlap => Some(HlColor::Gray),
            // HIGHLIGHT(9) 走 rgba 细分
            PenColor::Unknown(9) => match rgba {
                Some(c) if near(c, 190, 234, 254) => Some(HlColor::Blue),
                Some(c) if near(c, 255, 195, 140) => Some(HlColor::Orange),
                _ => None,
            },
            _ => None,
        }
    }
}

/// RGB 曼哈顿距离阈值内视为同色（6 色两两相距 ≥200，阈值 60 足以分开且容忍轻微抖动）。
fn near(c: (u8, u8, u8, u8), r: u8, g: u8, b: u8) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs();
    d(c.0, r) + d(c.1, g) + d(c.2, b) < 60
}

/// 一条摘取的高亮。
#[derive(Debug, Clone)]
pub struct Highlight {
    pub slot: usize,
    pub text: String,
    pub y: f64,     // 页面纵坐标（同槽多段按此+start 排序，还原阅读顺序）
    pub start: u32, // 字符起点（同 y 时的次级排序键 + 去重的字符范围起点）
    pub len: u32,   // 字符长度（去重判子集用；xochitl 划线常生成重复/子集段）
}

/// 清洗高亮文字：去 EPUB 软连字/换行残符（如 `ά`、软连字符）、trim。保守起见只去已知噪声。
fn clean_text(s: &str) -> String {
    // ά(U+03AC) 是 EPUB 换行/连字渲染残符（不该出现在正文），软连字符/BOM 同样是噪声；破折号等正常标点保留。
    s.chars()
        .filter(|&c| c != 'ά' && c != '\u{00ad}' && c != '\u{feff}')
        .collect::<String>()
        .trim()
        .to_string()
}

/// 从一页 `.rm` 提取所有可归槽的荧光笔高亮（非 6 色的忽略）。按 (y,start) 排序。
pub fn extract_highlights(rm_bytes: &[u8]) -> Result<Vec<Highlight>, String> {
    let rf = RemarkableFile::read(rm_bytes).map_err(|e| format!("解析 .rm: {e:?}"))?;
    let blocks = match rf {
        RemarkableFile::V6 { blocks, .. } => blocks,
        _ => return Err("非 v6 .rm".into()),
    };
    let mut out = Vec::new();
    for b in blocks {
        if let Block::SceneGlyphItem(sib) = b {
            if let Some(g) = sib.item.value {
                if let Some(hc) = HlColor::from_glyph(&g.color, g.color_rgba) {
                    let text = clean_text(&g.text);
                    if text.is_empty() {
                        continue;
                    }
                    let y = g.rectangles.first().map(|r| r.y).unwrap_or(0.0);
                    out.push(Highlight { slot: hc.slot(), text, y, start: g.start, len: g.length });
                }
            }
        }
    }
    out.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap().then(a.start.cmp(&b.start)));
    Ok(dedup_within_slot(out))
}

/// 同槽内去重：丢掉字符范围 `[start, start+len)` 被同槽另一段**包含**的段（xochitl 划线常生成
/// 完全重复段 '款'×2 或子集 '行'⊂'行吗？'）。等长重复保留先出现的一条。跨槽不去重（不同色=不同用途，
/// 即便同一句文字也各自保留）。保持入参已排好的 (y,start) 顺序。
fn dedup_within_slot(hs: Vec<Highlight>) -> Vec<Highlight> {
    let n = hs.len();
    let mut keep = vec![true; n];
    for i in 0..n {
        for j in 0..n {
            if i == j || !keep[j] || hs[i].slot != hs[j].slot {
                continue;
            }
            let (is, ie) = (hs[i].start, hs[i].start + hs[i].len);
            let (js, je) = (hs[j].start, hs[j].start + hs[j].len);
            // j 落在 i 内，且 i 更长（真子集）或等长但 i 先出现 → 丢 j。
            let j_in_i = js >= is && je <= ie;
            let i_bigger = (ie - is) > (je - js) || ((ie - is) == (je - js) && i < j);
            if j_in_i && i_bigger {
                keep[j] = false;
            }
        }
    }
    hs.into_iter().zip(keep).filter(|(_, k)| *k).map(|(h, _)| h).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_six_colors() {
        // 具体 PenColor 四色（rgba 占位黑，忽略）
        assert_eq!(HlColor::from_glyph(&PenColor::Yellow, Some((0, 0, 0, 255))), Some(HlColor::Yellow));
        assert_eq!(HlColor::from_glyph(&PenColor::Pink, Some((0, 0, 0, 255))), Some(HlColor::Pink));
        assert_eq!(HlColor::from_glyph(&PenColor::Green, Some((0, 0, 0, 255))), Some(HlColor::Green));
        assert_eq!(HlColor::from_glyph(&PenColor::GreyOverlap, None), Some(HlColor::Gray));
        // HIGHLIGHT(9) 靠 rgba 分蓝/橙
        assert_eq!(HlColor::from_glyph(&PenColor::Unknown(9), Some((190, 234, 254, 255))), Some(HlColor::Blue));
        assert_eq!(HlColor::from_glyph(&PenColor::Unknown(9), Some((255, 195, 140, 255))), Some(HlColor::Orange));
        // 轻微抖动仍判对
        assert_eq!(HlColor::from_glyph(&PenColor::Unknown(9), Some((188, 236, 252, 255))), Some(HlColor::Blue));
        // 非 6 色不摄取
        assert_eq!(HlColor::from_glyph(&PenColor::Black, Some((0, 0, 0, 255))), None);
        assert_eq!(HlColor::from_glyph(&PenColor::Red, None), None);
        assert_eq!(HlColor::from_glyph(&PenColor::Unknown(9), None), None); // HIGHLIGHT 无 rgba 无法判
    }

    #[test]
    fn slot_order() {
        assert_eq!(HlColor::Yellow.slot(), 0);
        assert_eq!(HlColor::Gray.slot(), 5);
    }

    #[test]
    fn dedup_subset_and_exact() {
        let mk = |slot, text: &str, start, len, y| Highlight { slot, text: text.into(), start, len, y };
        let hs = vec![
            mk(3, "款", 420, 1, 100.0),
            mk(3, "款", 420, 1, 100.0),     // 完全重复 → 去
            mk(3, "行", 232, 1, 200.0),     // 子集 → 去
            mk(3, "行吗？", 232, 3, 200.0), // 包含 '行' → 留
            mk(1, "款", 420, 1, 300.0),     // 不同槽同文字 → 留
        ];
        let out = dedup_within_slot(hs);
        let s3: Vec<String> = out.iter().filter(|h| h.slot == 3).map(|h| h.text.clone()).collect();
        assert_eq!(s3, vec!["款".to_string(), "行吗？".to_string()], "槽3应只剩'款'+'行吗？'");
        assert_eq!(out.iter().filter(|h| h.slot == 1).count(), 1, "跨槽同文字保留");
    }
}
