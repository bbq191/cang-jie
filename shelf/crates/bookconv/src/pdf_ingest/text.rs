//! 带位置的文字抽取（`pdf-extract` 的 OutputDev）与公式区域检测。

/// 一个提取出的字符 + 它在 PDF 用户空间（未缩放的 pt）里的基线位置 + 字号 + 行号（同一视觉行
/// 内的字符共享同一个 `line`，靠 pdf-extract 的 `end_line()` 钩子分行，不是自己按 Y 坐标猜）。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PositionedChar {
    pub ch: char,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub line: usize,
}

pub(super) struct TextCollector {
    pages: Vec<Vec<PositionedChar>>,
    line: usize,
    /// 逐字符插空格用——逻辑照抄 pdf-extract 自带 `PlainTextOutput` 的做法（不是自己发明的）：
    /// `begin_word()` 只是标一个"下一个字符要检查有没有跳空"的标志位，真正判定空格靠比较
    /// `last_end`（上一个字符右边缘的预期位置）和当前字符实际 x 的差。**踩过的坑**：最初
    /// 版本在 `end_word()` 里无条件插一个空格，结果同一个词内部因为字距微调（kerning）被
    /// PDF 内容流拆成好几个 `Tj`/`TJ` 片段时（专业排版的 PDF 非常常见），每个片段边界都会
    /// 被误当成词边界，插出"In tro duction"这种词中间带空格的乱码（2026-09-19 拿真实
    /// pdflatex 样本跑出来才发现，不是凭空想到的）。改成这套"只在有实际跳空缺口时才插空格"
    /// 的判定后，同一份样本恢复成正常的 "1 Introduction"。
    first_char: bool,
    last_end: f64,
    last_y: f64,
}

impl pdf_extract::OutputDev for TextCollector {
    fn begin_page(&mut self, _page_num: u32, _media_box: &pdf_extract::MediaBox, _art_box: Option<(f64, f64, f64, f64)>) -> Result<(), pdf_extract::OutputError> {
        self.pages.push(Vec::new());
        self.line = 0;
        self.first_char = false;
        self.last_end = f64::MAX / 2.0;
        self.last_y = 0.0;
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
    fn output_character(&mut self, trm: &pdf_extract::Transform, width: f64, _spacing: f64, font_size: f64, ch: &str) -> Result<(), pdf_extract::OutputError> {
        let (x, y) = (trm.m31, trm.m32);
        if let Some(page) = self.pages.last_mut() {
            if self.first_char && x > self.last_end + font_size * 0.1 {
                let line = self.line;
                page.push(PositionedChar { ch: ' ', x: self.last_end, y, font_size, line });
            }
            let line = self.line;
            for c in ch.chars() {
                page.push(PositionedChar { ch: c, x, y, font_size, line });
            }
        }
        self.first_char = false;
        self.last_y = y;
        self.last_end = x + width * font_size;
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        self.first_char = true;
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        self.line += 1;
        Ok(())
    }
}

/// 驱动 pdf-extract 跑一遍 `OutputDev`，拿到每页的逐字符位置流。**永远喂原始字节**，不复用
/// 本模块自己已经解析出的 `lopdf::Document`（0.45）——见模块文档，两边 lopdf 版本不同、类型
/// 不兼容，pdf-extract 内部会用它自己锁定的 lopdf 0.42 重新解析一遍。
pub(crate) fn extract_positioned_text(bytes: &[u8]) -> Result<Vec<Vec<PositionedChar>>, String> {
    let doc = pdf_extract::Document::load_mem(bytes).map_err(|e| format!("PDF 结构解析失败: {e}"))?;
    let mut collector = TextCollector { pages: Vec::new(), line: 0, first_char: false, last_end: f64::MAX / 2.0, last_y: 0.0 };
    pdf_extract::output_doc(&doc, &mut collector).map_err(|e| format!("PDF 文字提取失败: {e}"))?;
    Ok(collector.pages)
}

// ============================================================================
// 公式区域探测（纯函数，输入逐字符位置流，输出包围盒——不碰 PDF/渲染，可独立单测）
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// 字符"公式味"判定：**只能靠 Unicode 码位**（pdf-extract 的 `OutputDev` 不暴露字体名，
/// 见模块文档），数学运算符/字母数字符号/希腊字母/常见数学箭头这几个区块命中即算。拿真实
/// pdflatex+amsmath 样本（`tests/fixtures/sample.pdf`）验证过：`∫ ∑ √ ∞ π α β γ ±` 这类
/// 符号确实能提取出正确的 Unicode（Computer Modern 数学字体在这份样本里 ToUnicode 映射
/// 正常），矩阵/分数里的普通字母数字（`a b c d x y`）本身不会被单独命中——这些字符靠下面
/// [`merge_formula_lines`] 的"行合并"机制跟着相邻的强信号符号一起被圈进公式块，不需要
/// 每个字符单独判定。
pub(crate) fn is_formula_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(cp,
        0x2200..=0x22FF   // Mathematical Operators
        | 0x2A00..=0x2AFF // Supplemental Mathematical Operators
        | 0x27C0..=0x27EF // Miscellaneous Mathematical Symbols-A
        | 0x2980..=0x29FF // Miscellaneous Mathematical Symbols-B
        | 0x2190..=0x21FF // Arrows（数学里常见的 → ⇒ 等）
        | 0x1D400..=0x1D7FF // Mathematical Alphanumeric Symbols
        | 0x0370..=0x03FF // Greek and Coptic（公式里的希腊字母变量）
        | 0x2032..=0x2037 // 撇号类（导数记号 ′ ″）
    )
}

/// 一行是否判定为"公式行"：至少一个字符命中 [`is_formula_char`]。
pub(super) fn line_is_formula(chars: &[&PositionedChar]) -> bool {
    chars.iter().any(|c| is_formula_char(c.ch))
}

pub(super) fn bbox_of(chars: &[&PositionedChar]) -> Option<BBox> {
    if chars.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in chars {
        x0 = x0.min(c.x);
        y0 = y0.min(c.y);
        x1 = x1.max(c.x + c.font_size.max(1.0));
        y1 = y1.max(c.y + c.font_size.max(1.0));
    }
    Some(BBox { x0, y0, x1, y1 })
}

/// 每行的平均字号，用来估算"相邻行"的判定阈值（行高的若干倍）。
pub(super) fn avg_font_size(chars: &[&PositionedChar]) -> f64 {
    if chars.is_empty() {
        return 10.0;
    }
    chars.iter().map(|c| c.font_size).sum::<f64>() / chars.len() as f64
}

/// 探测这一页里的公式区域：按 `line` 分组→挑出公式行→纵向相邻（垂直间距小于约 1.5 倍平均
/// 字号）且水平范围有重叠的公式行合并成一个块（覆盖分数/矩阵/多行 `align` 这类跨行公式）。
pub(crate) fn detect_formula_regions(chars: &[PositionedChar]) -> Vec<BBox> {
    if chars.is_empty() {
        return Vec::new();
    }
    let max_line = chars.iter().map(|c| c.line).max().unwrap_or(0);
    let mut formula_lines: Vec<(usize, BBox, f64)> = Vec::new(); // (line_no, bbox, avg_font_size)
    for line_no in 0..=max_line {
        let line_chars: Vec<&PositionedChar> = chars.iter().filter(|c| c.line == line_no).collect();
        if line_chars.is_empty() || !line_is_formula(&line_chars) {
            continue;
        }
        if let Some(bbox) = bbox_of(&line_chars) {
            formula_lines.push((line_no, bbox, avg_font_size(&line_chars)));
        }
    }
    if formula_lines.is_empty() {
        return Vec::new();
    }
    // 按行号顺序合并相邻公式行（行号本身就是文档顺序，不需要再按 y 排序）。
    let mut blocks: Vec<BBox> = Vec::new();
    let mut cur = formula_lines[0].1;
    let mut cur_font = formula_lines[0].2;
    let mut prev_line = formula_lines[0].0;
    for (line_no, bbox, font) in formula_lines.into_iter().skip(1) {
        let gap_lines = line_no.saturating_sub(prev_line);
        let x_overlap = bbox.x0 <= cur.x1 + cur_font * 4.0 && bbox.x1 >= cur.x0 - cur_font * 4.0;
        if gap_lines <= 2 && x_overlap {
            cur.x0 = cur.x0.min(bbox.x0);
            cur.y0 = cur.y0.min(bbox.y0);
            cur.x1 = cur.x1.max(bbox.x1);
            cur.y1 = cur.y1.max(bbox.y1);
            cur_font = cur_font.max(font);
        } else {
            blocks.push(cur);
            cur = bbox;
            cur_font = font;
        }
        prev_line = line_no;
    }
    blocks.push(cur);
    // 每个块留一点边距，避免刚好裁掉括号/上下限的边缘。
    for b in blocks.iter_mut() {
        // 留白系数刻意调小（不是 0——分数/矩阵的括号/上下限边缘还是需要一点余量）：真实样本
        // 核对时发现行内公式紧贴正文（如"identity $e^{i\pi}$ is"）时，留白太大会啃掉公式
        // 两侧紧邻的正文字符（2026-09-19 用 sample.pdf 跑出来才发现，不是理论推演）——公式块
        // 边界目前只按整行字符包围盒算，天然比较粗，留白只能保守给一点，不能靠它兜底精确边界。
        let pad = cur_font.max(4.0) * 0.08;
        b.x0 -= pad;
        b.y0 -= pad;
        b.x1 += pad;
        b.y1 += pad;
    }
    blocks
}

// ============================================================================
// 字号识别标题（没有 /Outlines 书签目录时的 TOC 兜底）
// ============================================================================
