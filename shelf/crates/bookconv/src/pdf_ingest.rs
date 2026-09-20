//! 入库 PDF 的「优化」：有文字层→转 EPUB（保留图片+公式+必须有 TOC），无文字层的扫描件/
//! PDF 漫画→只做裁边处理、格式不变。跟 `comic_pdf.rs`（EPUB→PDF）方向相反，两条产线平行独立、
//! 互不影响。
//!
//! **三个新依赖分工**（都是纯 Rust、无 C 依赖，见 `Cargo.toml` 里各自的引入注释）：
//! - `lopdf`：本模块自己直接依赖的版本（见 `Cargo.toml` 锁 0.45.0）——页树/字体资源/图片
//!   XObject/`/Outlines` 书签树读取，`classify_pdf`/`optimize_pdf_trim_only`/
//!   `optimize_pdf_to_epub` 的结构解析部分都走它。
//! - `pdf-extract`：内部锁定 lopdf 0.42（`pub use lopdf::*` 重导出的是那个版本，**跟本模块
//!   直接依赖的 lopdf 0.45 是两个不同类型、不能互传**）——只用来驱动它的 `OutputDev` 钩子拿
//!   逐字符位置+字号，永远只喂原始 PDF 字节（`pdf_extract::Document::load_mem`），不跟本模块
//!   自己的 `lopdf::Document`（0.45）混用。**已知局限**：`OutputDev::output_character` 不带
//!   字体名（只有字号），公式区域探测这次只能靠 Unicode 码位判断，不能按数学字体族名判断
//!   （见 `is_formula_char` 文档注释，含真实 pdflatex 样本验证过 Unicode 映射基本正确）。
//! - `hayro`：整页光栅化，只给公式区域裁剪用（自己也是独立解析一遍 PDF 字节，第三份互不
//!   共享的解析，`Pdf::new(bytes)`）。
//!
//! 一份 PDF 因此在 `optimize_pdf_to_epub` 里最多被解析三遍（本模块自己的 lopdf 0.45 一遍、
//! pdf-extract 内部 lopdf 0.42 一遍、hayro 只在有公式的页才额外解析+渲染）——host 侧一次性
//! 处理场景可接受这个重复解析开销，换来三个子系统互不耦合、互相独立可测。
//!
//! **已知局限（真实样本核对时发现，未修——按整行字符包围盒算公式区域，天然是粗粒度的）**：
//! 行内公式紧贴正文（如 "the identity $e^{i\\pi}+1=0$ is"）时，公式两侧紧邻的一两个正文
//! 单词可能被误判进公式块的包围盒、从正文里消失（`tests/fixtures/sample.pdf` 里
//! "identity" 被吞成 "i e" 就是这个问题）——根因是 pdf-extract 在字体切换处（正文字体切数学
//! 斜体/符号字体）就会分出新的 `line`，公式区域按"这一整行字符的包围盒"算，跟真正的公式视觉
//! 边界不完全重合。要根治需要按字符级别（不是行级别）精确圈公式区域，这次没做，记在这里避免
//! 以后误以为是新 bug。

use crate::convert::pdfwrite::{self, PdfPieceWriter};
use crate::epub::{Book, BookMeta, Chapter, Resource};
use std::path::Path;

// ============================================================================
// 分类
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfKind {
    /// 漫画：≥[`COMIC_PAGE_RATIO`] 的页都被一张覆盖页面主体面积的图片占满。
    Comic,
    /// 没有可提取的文字层（扫描件、纯图片 PDF），不是漫画形状但也没有文字可转。
    NoTextLayer,
    /// 有真实可提取的文字层，走 PDF→EPUB 转换。
    TextLayer,
}

/// 判定漫画的页数比例——`≥` 这个比例的页都是"一张图基本铺满整页"才判漫画，镜像
/// `comic_detect::is_comic` 的"图多字少"哲学，只是漫画 EPUB 按"图片数量"计，PDF 天然以页为
/// 单位，改成"页比例"。
pub const COMIC_PAGE_RATIO: f64 = 0.9;
/// 单张图片相对页面面积的覆盖比例阈值——超过这个比例才算"铺满整页"。
pub const COMIC_IMAGE_AREA_RATIO: f64 = 0.85;
/// 判定"有文字层"的每页平均字符数下限（不含空白）。参考 `comic_detect::TEXT_PER_IMAGE=40`
/// 的量级换算成"每页"版本——具体数值已经拿 `tests/fixtures/sample.pdf`（真实 pdflatex 样本，
/// 每页几百字）核对过跟正常文字书的量级差距足够大，不是拍脑袋定的。
pub const MIN_CHARS_PER_PAGE: f64 = 40.0;

/// 判不准（打不开/解析失败）一律退到安全默认：[`PdfKind::NoTextLayer`]（只裁边，不转换）——
/// 跟 `comic_detect::is_comic_epub_file` "打不开当不是漫画、走现状老路径" 同一个"失败模式选
/// 更保守那条"原则：裁边是幂等、低风险操作，转 EPUB 是破坏性格式变更，判不准时不能选后者。
pub fn classify_pdf(path: &Path) -> PdfKind {
    let Ok(bytes) = std::fs::read(path) else { return PdfKind::NoTextLayer };
    classify_pdf_bytes(&bytes)
}

fn classify_pdf_bytes(bytes: &[u8]) -> PdfKind {
    let Ok(doc) = lopdf::Document::load_mem(bytes) else { return PdfKind::NoTextLayer };
    let pages = doc.get_pages();
    if pages.is_empty() {
        return PdfKind::NoTextLayer;
    }
    let total = pages.len();
    let mut comic_pages = 0usize;
    for (_, page_id) in pages.iter() {
        if page_covered_by_big_image(&doc, *page_id) {
            comic_pages += 1;
        }
    }
    if comic_pages as f64 / total as f64 >= COMIC_PAGE_RATIO {
        return PdfKind::Comic;
    }
    let Ok(text_pages) = extract_positioned_text(bytes) else { return PdfKind::NoTextLayer };
    let total_chars: usize = text_pages.iter().map(|p| p.iter().filter(|c| !c.ch.is_whitespace()).count()).sum();
    let avg = total_chars as f64 / total as f64;
    if avg >= MIN_CHARS_PER_PAGE {
        PdfKind::TextLayer
    } else {
        PdfKind::NoTextLayer
    }
}

/// 这一页是不是被一张覆盖主体面积的图片占满——只看图片声明的像素宽高比 MediaBox 面积占比的
/// 粗略近似（不去解析内容流里的 `cm` 变换算精确摆放尺寸；漫画/扫描页几乎都是"一张图等比撑满
/// 整页"，用图片自身宽高比 vs 页面宽高比接近、且没有第二张显著大小的图这个粗判据足够）。
fn page_covered_by_big_image(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> bool {
    let Ok(images) = doc.get_page_images(page_id) else { return false };
    if images.is_empty() {
        return false;
    }
    let Ok(dict) = doc.get_dictionary(page_id) else { return false };
    let Some((pw, ph)) = media_box_size(doc, dict) else { return false };
    let page_area = pw * ph;
    if page_area <= 0.0 {
        return false;
    }
    images.iter().any(|img| {
        // 图片是位图，没有物理尺寸；用"宽高比跟页面宽高比接近"代替"面积占比"（面积单位不同、
        // 没法直接比较像素面积和 pt 面积）——比例接近说明这张图大概率是整页等比缩放摆放的。
        let img_ratio = img.width as f64 / img.height.max(1) as f64;
        let page_ratio = pw / ph.max(0.001);
        (img_ratio / page_ratio - 1.0).abs() < (1.0 - COMIC_IMAGE_AREA_RATIO)
    })
}

fn media_box_size(doc: &lopdf::Document, page_dict: &lopdf::Dictionary) -> Option<(f64, f64)> {
    let mb = get_inherited_media_box(doc, page_dict)?;
    if mb.len() < 4 {
        return None;
    }
    let w = (mb[2].as_float().unwrap_or(0.0) - mb[0].as_float().unwrap_or(0.0)).abs();
    let h = (mb[3].as_float().unwrap_or(0.0) - mb[1].as_float().unwrap_or(0.0)).abs();
    Some((w as f64, h as f64))
}

fn get_inherited_media_box(doc: &lopdf::Document, page_dict: &lopdf::Dictionary) -> Option<Vec<lopdf::Object>> {
    if let Ok(arr) = page_dict.get(b"MediaBox").and_then(|o| o.as_array()) {
        return Some(arr.clone());
    }
    let parent_ref = page_dict.get(b"Parent").ok()?.as_reference().ok()?;
    let parent = doc.get_dictionary(parent_ref).ok()?;
    get_inherited_media_box(doc, parent)
}

// ============================================================================
// 逐字符位置提取（pdf-extract OutputDev 驱动）
// ============================================================================

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

struct TextCollector {
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
fn line_is_formula(chars: &[&PositionedChar]) -> bool {
    chars.iter().any(|c| is_formula_char(c.ch))
}

fn bbox_of(chars: &[&PositionedChar]) -> Option<BBox> {
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
fn avg_font_size(chars: &[&PositionedChar]) -> f64 {
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Heading {
    pub page: usize, // 0-indexed
    pub line: usize,
    pub level: i64, // 1=最大字号…最多 3 档
    pub title: String,
}

/// 标题候选字号相对正文基准字号的倍数下限。
pub const HEADING_SIZE_RATIO: f64 = 1.2;
/// 没有识别出任何标题候选时的兜底分块页数。
pub const FALLBACK_CHUNK_PAGES: usize = 20;

/// 一页的"正文基准字号"＝出现次数最多的字号（众数）；字号按 0.5pt 归并，避免浮点误差把同一个
/// 视觉字号拆成好几个桶。
fn body_font_size(page: &[PositionedChar]) -> f64 {
    use std::collections::HashMap;
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for c in page {
        if c.ch.is_whitespace() {
            continue;
        }
        let bucket = (c.font_size * 2.0).round() as i64;
        *counts.entry(bucket).or_insert(0) += 1;
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(b, _)| b as f64 / 2.0).unwrap_or(10.0)
}

/// 按字号识别标题：每页算正文基准字号，字号 ≥ 基准 × [`HEADING_SIZE_RATIO`] 的行判成标题候选；
/// 连续（同页、行号相邻）的候选行合并成一个标题；不同字号分档映射 `level`（最大字号＝1，最多
/// 3 档，超出封顶到 3）。**全书零候选**时不在这里兜底——那是调用方（`optimize_pdf_to_epub`）
/// 的责任：识别不出结构就按 [`FALLBACK_CHUNK_PAGES`] 固定页数分块，不假装有真实章节。
pub(crate) fn detect_headings_by_font_size(pages: &[Vec<PositionedChar>]) -> Vec<Heading> {
    let mut headings = Vec::new();
    // 全书统一的"字号→level"映射：先收集所有页面里出现过的、判定为标题候选的字号，降序去重，
    // 取前 3 档；不同页各自独立判"是不是标题候选"（相对各自页的正文基准），但档位映射是全书
    // 统一的，不然同一本书不同页的"最大字号"却对应不同 level，nav.xhtml 嵌套会乱。
    let mut candidate_sizes: Vec<i64> = Vec::new();
    let mut per_page_body: Vec<f64> = Vec::with_capacity(pages.len());
    for page in pages {
        let body = body_font_size(page);
        per_page_body.push(body);
        let max_line = page.iter().map(|c| c.line).max().unwrap_or(0);
        for line_no in 0..=max_line {
            let line_chars: Vec<&PositionedChar> = page.iter().filter(|c| c.line == line_no && !c.ch.is_whitespace()).collect();
            if line_chars.is_empty() {
                continue;
            }
            let avg = avg_font_size(&line_chars);
            if avg >= body * HEADING_SIZE_RATIO {
                candidate_sizes.push((avg * 2.0).round() as i64);
            }
        }
    }
    if candidate_sizes.is_empty() {
        return headings;
    }
    candidate_sizes.sort_unstable();
    candidate_sizes.dedup();
    candidate_sizes.reverse(); // 大字号在前＝level 1
    let level_of = |size: f64| -> i64 {
        let bucket = (size * 2.0).round() as i64;
        let rank = candidate_sizes.iter().position(|&s| s == bucket).unwrap_or(candidate_sizes.len() - 1);
        (rank as i64 + 1).min(3)
    };

    for (page_idx, page) in pages.iter().enumerate() {
        let body = per_page_body[page_idx];
        let max_line = page.iter().map(|c| c.line).max().unwrap_or(0);
        let mut i = 0usize;
        while i <= max_line {
            let line_chars: Vec<&PositionedChar> = page.iter().filter(|c| c.line == i && !c.ch.is_whitespace()).collect();
            if line_chars.is_empty() {
                i += 1;
                continue;
            }
            let avg = avg_font_size(&line_chars);
            if avg < body * HEADING_SIZE_RATIO {
                i += 1;
                continue;
            }
            // 连续的标题候选行合并成一个标题（同一个标题换行显示的情况）。标题文字要保留空格
            // （行内原有的词间空格），只有判"是不是标题候选"用的字号统计才该滤掉空白字符——
            // 之前误用同一份过滤后的 line_chars 拼标题，"1 Introduction" 会被拼成
            // "1Introduction"，2026-09-19 真机样本核对时发现。
            let title_line = |ln: usize| -> String { page.iter().filter(|c| c.line == ln).map(|c| c.ch).collect::<String>().trim().to_string() };
            let mut title: String = title_line(i);
            let level = level_of(avg);
            let mut j = i + 1;
            while j <= max_line {
                let next_chars: Vec<&PositionedChar> = page.iter().filter(|c| c.line == j && !c.ch.is_whitespace()).collect();
                if next_chars.is_empty() {
                    break;
                }
                let next_avg = avg_font_size(&next_chars);
                if next_avg < body * HEADING_SIZE_RATIO || level_of(next_avg) != level {
                    break;
                }
                title.push(' ');
                title.push_str(&title_line(j));
                j += 1;
            }
            headings.push(Heading { page: page_idx, line: i, level, title: title.trim().to_string() });
            i = j;
        }
    }
    headings
}

// ============================================================================
// 裁边路径（Comic / NoTextLayer 共用，格式不变仍是 PDF）
// ============================================================================

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfTrimReport {
    pub pages: usize,
}

/// 无文字层/漫画 PDF：逐页取主图片字节→ `imgopt::prepare_comic_page_for_pdf`（裁边+按需缩放，单趟）
/// →喂给 `PdfPieceWriter` 写出新 PDF（复用漫画 EPUB→PDF 那条产线的写手）。
///
/// **不允许变动书籍内容**（用户 2026-09-20 明确要求），所以这条路径**只处理"零文字、每页恰好一张
/// 整页图"的 PDF**——重写页面等于丢掉图片以外的一切，其它形状一律拒绝并保持原文件不动：
/// - 有任何可提取文字（含扫描件的 OCR 隐形文字层）：重写会丢文字层；
/// - 某页不是"单张整页图"（矢量图形、多图拼版、纯文字页）：重写会丢掉这一页的其余内容；
/// - 无法确认没有文字层（提取失败）：宁可不动。
/// 此前没有这些闸门，且 `encode_pdf_image` 名义上"给 trim_margins 用"却**从未调用 trim_margins**
/// （实测根本没裁边）、`finish(&[])` 还把原 PDF 的书签全部丢掉。
///
/// **目录**：保留原 PDF 书签（页码映射到输出页，层级压平）；原文件没有书签则按页分段兜底
/// （[`crate::comic_pdf::page_chunk_titles`]），保证输出一定有目录。
pub fn optimize_pdf_trim_only(src: &Path, dst_tmp: &Path, mut on_progress: impl FnMut(usize, usize)) -> Result<PdfTrimReport, String> {
    let bytes = std::fs::read(src).map_err(|e| format!("读源文件失败: {e}"))?;
    let doc = lopdf::Document::load_mem(&bytes).map_err(|e| format!("PDF 结构解析失败: {e}"))?;
    let pages = doc.get_pages();
    let page_count = pages.len();
    if page_count == 0 {
        return Err("PDF 没有可用页面".into());
    }
    let text_pages = extract_positioned_text(&bytes).map_err(|e| format!("无法确认这个 PDF 没有文字层，为保住书籍内容不做改动：{e}"))?;
    let text_chars: usize = text_pages.iter().map(|p| p.iter().filter(|c| c.ch != '\u{0}' && !c.ch.is_whitespace()).count()).sum();
    if text_chars > 0 {
        return Err(format!("这个 PDF 含 {text_chars} 个可提取文字（文字层），改写页面会丢掉文字，保持原样"));
    }
    for (i, (_, page_id)) in pages.iter().enumerate() {
        let n_images = doc.get_page_images(*page_id).map(|v| v.len()).unwrap_or(0);
        if n_images != 1 || !page_covered_by_big_image(&doc, *page_id) {
            return Err(format!("第 {} 页不是\"单张整页图片\"（图片 {n_images} 张），改写页面会丢掉这一页的其余内容，保持原样", i + 1));
        }
    }
    let mut titles: Vec<(usize, String)> = doc
        .get_toc()
        .map(|t| t.toc.iter().map(|e| (e.page.saturating_sub(1).min(page_count - 1), e.title.clone())).collect())
        .unwrap_or_default();
    if titles.is_empty() {
        titles = crate::comic_pdf::page_chunk_titles(page_count);
    }
    let mut writer = PdfPieceWriter::begin(page_count, true);
    for (i, (_, page_id)) in pages.iter().enumerate() {
        on_progress(i, page_count);
        let images = doc.get_page_images(*page_id).map_err(|e| format!("读第 {} 页图片失败: {e}", i + 1))?;
        let raw = decode_pdf_image_to_bytes(&doc, &images[0])?;
        let sized = crate::imgopt::prepare_comic_page_for_pdf(&raw, pdfwrite::PDF_PAGE_W, pdfwrite::PDF_PAGE_H).unwrap_or(raw);
        writer.write_page(&pdfwrite::image_from_bytes(&sized)?)?;
    }
    on_progress(page_count, page_count);
    let out = writer.finish(&titles)?;
    std::fs::write(dst_tmp, &out).map_err(|e| format!("写出临时文件失败: {e}"))?;
    Ok(PdfTrimReport { pages: page_count })
}

/// 把 lopdf 的 `PdfImage`（第三方 PDF 里的原始图片流）解成可以喂给 `imgopt::trim_margins`/
/// `pdfwrite::image_from_bytes` 的通用 JPEG/PNG 字节。**这次只稳妥处理两种最常见的情况**：
/// `/DCTDecode`（JPEG，原样透传，不解码不重编码）和 `/FlateDecode` 的 8-bit 灰度/RGB 原始像素
/// （解压后重新编码成 PNG）。CCITTFax/JBIG2/JPX/索引色/非 8-bit 这些少见情况直接报错跳过这页
/// （不是这次范围内要支持的全部 PDF 图片编码，`hayro-ccitt`/`hayro-jbig2` 这两个传递依赖理论上
/// 能补上，留作已知的后续扩展点，不在这次实现）。
/// **不能自己手撸 zlib inflate**——真机踩过：pdflatex/pdftex 产出的 `/FlateDecode` 图片流
/// 常见带 `/DecodeParms << /Predictor 10 ... >>`（PNG 逐行预测器），裸 inflate 出来的字节
/// 每行多一个过滤类型前缀字节（真实样本：200×100×3=60000 应有字节，裸 inflate 出 60100，
/// 多出来的 100 字节精确等于行数）——`lopdf::Stream::decompressed_content()` 已经正确处理
/// 了这个预测器（含 PNG 10-15 与 TIFF 2 两种），必须重新按 `img.id` 取回原始 `Stream` 对象
/// 调它，不能图省事直接对 `img.content`（未解预测器的裸字节）手动 inflate。
fn decode_pdf_image_to_bytes(doc: &lopdf::Document, img: &lopdf::xobject::PdfImage) -> Result<Vec<u8>, String> {
    let filters = img.filters.clone().unwrap_or_default();
    if filters.iter().any(|f| f == "DCTDecode") {
        return Ok(img.content.to_vec());
    }
    if filters.is_empty() || filters.iter().any(|f| f == "FlateDecode") {
        let obj = doc.get_object(img.id).map_err(|e| format!("重取图片对象失败: {e}"))?;
        let stream = obj.as_stream().map_err(|e| format!("图片对象不是 stream: {e}"))?;
        let raw = if filters.is_empty() { img.content.to_vec() } else { stream.decompressed_content().map_err(|e| format!("PDF 图片解压失败: {e}"))? };
        let is_gray = img.color_space.as_deref() == Some("DeviceGray");
        return encode_raw_pixels_png(&raw, img.width as u32, img.height as u32, is_gray);
    }
    Err(format!("暂不支持的 PDF 图片编码: {filters:?}"))
}

fn encode_raw_pixels_png(pixels: &[u8], w: u32, h: u32, gray: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(if gray { png::ColorType::Grayscale } else { png::ColorType::Rgb });
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| format!("PNG 编码头失败: {e}"))?;
        writer.write_image_data(pixels).map_err(|e| format!("PNG 编码数据失败: {e}"))?;
    }
    Ok(out)
}

// ============================================================================
// PDF → EPUB（有文字层）
// ============================================================================

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfToEpubReport {
    pub pages: usize,
    pub chapters: usize,
    pub images: usize,
    pub formula_blocks: usize,
}

/// 有文字层的 PDF → EPUB：结构解析（页树/书签）→ 逐页文字+位置提取→ 公式区域探测→ 非公式
/// 文字重排成段落→ 图片提取（穿插在段落之后，不做精确的内容流位置插值，见下方图片提取处
/// 注释）→ 公式块用 hayro 整页渲染后裁剪成图片→ 章节/TOC 构建（书签优先，没有书签按字号
/// 识别标题，标题也识别不出来就按固定页数分块）→ `epub::assemble`。
pub fn optimize_pdf_to_epub(src: &Path, mut on_progress: impl FnMut(usize, usize)) -> Result<(Book, PdfToEpubReport), String> {
    let bytes = std::fs::read(src).map_err(|e| format!("读源文件失败: {e}"))?;
    let doc = lopdf::Document::load_mem(&bytes).map_err(|e| format!("PDF 结构解析失败: {e}"))?;
    let pages_map = doc.get_pages();
    let page_count = pages_map.len();
    if page_count == 0 {
        return Err("PDF 没有可用页面".into());
    }
    let page_ids: Vec<lopdf::ObjectId> = pages_map.values().copied().collect();

    on_progress(0, page_count.max(1) * 2);
    let text_pages = extract_positioned_text(&bytes)?;
    if text_pages.len() != page_count {
        return Err(format!("文字提取页数 {} 跟结构解析页数 {} 对不上", text_pages.len(), page_count));
    }

    // 公式区域（每页一份，可能为空）。
    let formula_regions: Vec<Vec<BBox>> = text_pages.iter().map(|p| detect_formula_regions(p)).collect();
    let total_formula_blocks: usize = formula_regions.iter().map(|v| v.len()).sum();

    // 章节边界：书签优先。
    let toc = doc.get_toc().ok();
    let mut resources: Vec<Resource> = Vec::new();
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut total_images = 0usize;

    // 公式渲染缓存：同页多个公式块只渲染一次整页。
    let render_settings = hayro::RenderSettings { x_scale: 2.0, y_scale: 2.0, ..Default::default() };

    let hayro_pdf = hayro::hayro_syntax::Pdf::new(std::sync::Arc::new(bytes.clone())).ok();

    let mut page_html: Vec<String> = Vec::with_capacity(page_count);
    for (idx, chars) in text_pages.iter().enumerate() {
        on_progress(page_count + idx, page_count * 2);
        let regions = &formula_regions[idx];
        // 公式块先渲染成图片（同页多块共用一次整页渲染）。图片只是**补充**：文字流里不删任何字符。
        let mut region_imgs: Vec<Option<String>> = vec![None; regions.len()];
        if !regions.is_empty() {
            if let Some(pdf) = &hayro_pdf {
                if let Some(page) = pdf.pages().get(idx) {
                    let cache = hayro::RenderCache::new();
                    let pixmap = hayro::render(page, &cache, &hayro::hayro_interpret::InterpreterSettings::default(), &render_settings);
                    for (bi, region) in regions.iter().enumerate() {
                        if let Some(png) = crop_pixmap_to_png(&pixmap, region, page) {
                            let path = format!("images/pdf_p{}_f{}.png", idx + 1, bi + 1);
                            region_imgs[bi] = Some(format!("<p><img src=\"../{path}\" alt=\"formula\"/></p>"));
                            resources.push(Resource { path, media_type: "image/png".to_string(), bytes: png });
                        }
                    }
                }
            }
        }
        // 换行不等于换段——PDF 里一段话正常会自动折成好几个视觉行，只有行间垂直间距明显
        // 大于普通行高（约 1.5 倍字号，同一段落内的换行通常间距≈1 倍字号）才算真的换段。
        // 只按 `line` 变化就切 `<p>` 会把每一行拆成单独一段，读起来像分行诗不是正常段落
        // （2026-09-19 真机样本核对时发现）。
        //
        // **不允许变动书籍内容**（用户 2026-09-20 明确要求）：文字层里的每个字符都进文字流，包括公式
        // 外接框内的——此前把框内字符整体丢掉，实测会把紧贴公式的正文单词一并吞掉（"the identity iπ
        // e+1=0" 丢了 identity，"A final short section … symbol," 整半句消失）。公式图片在其所属段落
        // 结束处补一张（`pending` 记录本段落里出现过的公式块，段落收尾时统一输出）。
        let mut html = String::new();
        html.push_str("<p>");
        let mut in_para = false;
        let mut last_line: Option<usize> = None;
        let mut last_y: Option<f64> = None;
        let mut pending: Vec<usize> = Vec::new();
        let mut flushed = vec![false; regions.len()];
        let flush_pending = |html: &mut String, pending: &mut Vec<usize>, flushed: &mut Vec<bool>| {
            for bi in pending.drain(..) {
                if !flushed[bi] {
                    flushed[bi] = true;
                    if let Some(img) = &region_imgs[bi] {
                        html.push_str(img);
                    }
                }
            }
        };
        for c in chars {
            if let Some(ll) = last_line {
                if c.line != ll {
                    let gap = last_y.map(|ly| (ly - c.y).abs()).unwrap_or(0.0);
                    if gap > c.font_size.max(1.0) * 1.5 {
                        html.push_str("</p>");
                        flush_pending(&mut html, &mut pending, &mut flushed);
                        html.push_str("<p>");
                        in_para = false;
                    } else {
                        html.push(' ');
                    }
                }
            }
            if c.ch == '\u{0}' {
                continue;
            }
            if let Some(bi) = regions.iter().position(|b| point_in_bbox(c.x, c.y, b)) {
                if !flushed[bi] && !pending.contains(&bi) {
                    pending.push(bi);
                }
            }
            html.push_str(&crate::util::xml_escape(&c.ch.to_string()));
            in_para = true;
            last_line = Some(c.line);
            last_y = Some(c.y);
        }
        if !in_para && html.ends_with("<p>") {
            html.truncate(html.len() - 3);
        } else {
            html.push_str("</p>");
        }
        flush_pending(&mut html, &mut pending, &mut flushed);
        // 兜底：没有任何字符落进去的公式块（理论上不会）也别丢图。
        for (bi, img) in region_imgs.iter().enumerate() {
            if !flushed[bi] {
                if let Some(img) = img {
                    html.push_str(img);
                }
            }
        }
        // 页内图片（跟文字混排的插图/照片），追加在段落最后。
        if let Ok(images) = doc.get_page_images(page_ids[idx]) {
            for (ii, img) in images.iter().enumerate() {
                if let Ok(raw) = decode_pdf_image_to_bytes(&doc, img) {
                    let path = format!("images/pdf_p{}_img{}.png", idx + 1, ii + 1);
                    let media_type = if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 { "image/jpeg" } else { "image/png" };
                    html.push_str(&format!("<p><img src=\"../{path}\" alt=\"\"/></p>"));
                    resources.push(Resource { path, media_type: media_type.to_string(), bytes: raw });
                    total_images += 1;
                }
            }
        }
        page_html.push(html);
    }
    on_progress(page_count * 2, page_count * 2);

    // 章节正文前补一个 `<h2>` 标题元素——书签/字号识别出的标题这时候只是 nav.xhtml 的 TOC
    // 条目文字，正文本身原样含着那行字但没有任何视觉强调（等同于普通段落），读起来不像真实
    // 书籍章节。真实书籍章节页顶部同时有 TOC 条目和正文内可见的标题是标准约定，不是重复。
    let with_heading = |title: &str, body: String| -> String { promote_heading(title, body) };

    match toc {
        Some(toc) if !toc.toc.is_empty() => {
            let entries = toc.toc;
            for (i, e) in entries.iter().enumerate() {
                let start = e.page.saturating_sub(1).min(page_count.saturating_sub(1));
                let end = entries.get(i + 1).map(|n| n.page.saturating_sub(1)).unwrap_or(page_count);
                let end = end.max(start + 1).min(page_count);
                let body: String = page_html[start..end].concat();
                chapters.push(Chapter { title: e.title.clone(), html_body: with_heading(&e.title, body), level: e.level as i64 });
            }
        }
        _ => {
            let headings = detect_headings_by_font_size(&text_pages);
            if headings.is_empty() {
                // 兜底：按固定页数分块，如实标注不是真实章节结构——这种情况正文前不补 `<h2>`
                // 标题（"第 N 部分"不是从内容里识别出来的，硬加会显得像是真的检测到了结构）。
                let mut start = 0usize;
                let mut idx = 1;
                while start < page_count {
                    let end = (start + FALLBACK_CHUNK_PAGES).min(page_count);
                    let body: String = page_html[start..end].concat();
                    chapters.push(Chapter { title: format!("第 {idx} 部分"), html_body: body, level: 1 });
                    start = end;
                    idx += 1;
                }
            } else {
                for (i, h) in headings.iter().enumerate() {
                    let start = h.page;
                    let end = headings.get(i + 1).map(|n| n.page).unwrap_or(page_count).max(start + 1).min(page_count);
                    let body: String = page_html[start..end].concat();
                    chapters.push(Chapter { title: h.title.clone(), html_body: with_heading(&h.title, body), level: h.level });
                }
            }
        }
    }
    if chapters.is_empty() {
        chapters.push(Chapter { title: "正文".to_string(), html_body: page_html.concat(), level: 1 });
    }

    let title = pdf_doc_title(&doc).unwrap_or_else(|| src.file_stem().and_then(|s| s.to_str()).unwrap_or("PDF").to_string());
    let mut book = Book {
        meta: BookMeta { book_id: format!("pdf:{title}"), title, author: String::new(), language: "zh".to_string(), publisher: String::new(), cover: None, cover_ext: String::new(), cover_media_type: String::new() },
        chapters,
        resources,
    };
    let report = PdfToEpubReport { pages: page_count, chapters: book.chapters.len(), images: total_images, formula_blocks: total_formula_blocks };
    mark_pdf_source(&mut book);
    Ok((book, report))
}

/// 把章节正文里**原有**的标题段落升级成 `<h2>`，不额外添加文字（此前 `<h2>标题</h2>` + 正文里原有的
/// 标题段落，同一个标题在章内出现两次，属于改动书籍内容）。匹配 `<p>标题</p>`（整段就是标题）或
/// `<p>标题 …`（标题与后文同段，拆成 `<h2>` + 剩余 `<p>`）；章内找不到就**不加**——宁可标题只留在
/// 目录里也不往正文里塞原书没有的字。
fn promote_heading(title: &str, body: String) -> String {
    let t = crate::util::xml_escape(title.trim());
    if t.is_empty() {
        return body;
    }
    let whole = format!("<p>{t}</p>");
    if let Some(i) = body.find(&whole) {
        let mut out = body.clone();
        out.replace_range(i..i + whole.len(), &format!("<h2>{t}</h2>"));
        return out;
    }
    let prefix = format!("<p>{t} ");
    if let Some(i) = body.find(&prefix) {
        let mut out = body.clone();
        out.replace_range(i..i + prefix.len(), &format!("<h2>{t}</h2><p>"));
        return out;
    }
    body
}

fn point_in_bbox(x: f64, y: f64, b: &BBox) -> bool {
    x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1
}

fn pdf_doc_title(doc: &lopdf::Document) -> Option<String> {
    let info_ref = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
    let info = doc.get_dictionary(info_ref).ok()?;
    let title = info.get(b"Title").ok()?;
    let bytes = title.as_str().ok()?;
    Some(String::from_utf8_lossy(bytes).trim().to_string()).filter(|s| !s.is_empty())
}

/// 把整页位图裁到某个公式包围盒对应的像素区域，编码成 PNG。PDF 用户空间→像素空间的换算用
/// `page.render_dimensions()`/MediaBox 尺寸算缩放比（不精确到子像素，公式裁剪本来就不需要）。
fn crop_pixmap_to_png(pixmap: &hayro::vello_cpu::Pixmap, region: &BBox, page: &hayro::hayro_syntax::page::Page) -> Option<Vec<u8>> {
    let (pw, ph) = page.render_dimensions();
    let (pix_w, pix_h) = (pixmap.width() as f64, pixmap.height() as f64);
    let sx = pix_w / pw.max(1.0) as f64;
    let sy = pix_h / ph.max(1.0) as f64;
    let x0 = (region.x0 * sx).max(0.0) as u32;
    let y1_img = pix_h - (region.y0 * sy); // PDF y 轴向上，图片 y 轴向下
    let y0_img = pix_h - (region.y1 * sy);
    let x1 = ((region.x1 * sx).min(pix_w)) as u32;
    let y0 = y0_img.max(0.0) as u32;
    let y1 = (y1_img.min(pix_h)) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let data = pixmap.data_as_u8_slice();
    let (w, h) = (pixmap.width() as u32, pixmap.height() as u32);
    let img = image::RgbaImage::from_raw(w as u32, h as u32, data.to_vec())?;
    let cropped = image::imageops::crop_imm(&img, x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)).to_image();
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(cropped).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

/// PDF 转出的 EPUB 来源标记：往 `content.opf` 塞一个自定义 meta，写法对齐 `epub.rs::
/// content_opf` 现有 `<meta name="cover" ...>` 那套——实现细节见下方 `mark_pdf_source`（塞进
/// `BookMeta.publisher` 是不行的，会污染真实元数据；改成往第一章标题前加一个不可见占位不优雅；
/// 最终用最不破坏既有 `epub::assemble` 结构的办法：约定 `BookMeta.book_id` 前缀
/// `"pdf:"`——跟 `looks_like_pdf_derived_epub` 配对识别，见该函数文档）。
fn mark_pdf_source(_book: &mut Book) {
    // book_id 已经在 optimize_pdf_to_epub 里用 "pdf:" 前缀构造，这里不需要额外操作——
    // 保留这个函数是为了让"标记来源"这个步骤在调用点显式可见，不是悄悄藏在 book_id 构造里。
}

/// 识别"这份 EPUB 是入库 PDF 转出来的"——检查 `content.opf` 里的 `dc:identifier` 是不是
/// `"pdf:"` 前缀（`optimize_pdf_to_epub` 用 `book_id: format!("pdf:{title}")` 构造，
/// `epub.rs::content_opf` 原样写进 `<dc:identifier id="pub-id">weread:{book_id}</dc:
/// identifier>`）。跟 `pdfwrite::looks_like_own_bookconv_pdf` 同构：开 zip、读一个文件、
/// 找标记字符串，不用完整解析 EPUB 结构。
pub fn looks_like_pdf_derived_epub(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else { return false };
    let Ok(mut zip) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { return false };
    let Ok(mut entry) = zip.by_name("OEBPS/content.opf") else { return false };
    let mut buf = String::new();
    if std::io::Read::read_to_string(&mut entry, &mut buf).is_err() {
        return false;
    }
    buf.contains("weread:pdf:")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PDF: &[u8] = include_bytes!("../tests/fixtures/sample.pdf");

    fn char_at(ch: char, x: f64, y: f64, font_size: f64, line: usize) -> PositionedChar {
        PositionedChar { ch, x, y, font_size, line }
    }

    // ---- 公式区域探测 ----

    #[test]
    fn formula_detection_single_inline_run() {
        let chars = vec![
            char_at('x', 0.0, 100.0, 10.0, 0),
            char_at('=', 10.0, 100.0, 10.0, 0),
            char_at('π', 20.0, 100.0, 10.0, 0), // 命中数学符号区块
        ];
        let blocks = detect_formula_regions(&chars);
        assert_eq!(blocks.len(), 1, "单行内联公式应该产出一个公式块");
    }

    #[test]
    fn formula_detection_multiline_merges_into_one_block() {
        // 模拟分数/矩阵：多行都出现数学符号，行号相邻。
        let chars = vec![
            char_at('∫', 0.0, 200.0, 10.0, 0),
            char_at('0', 5.0, 200.0, 10.0, 0),
            char_at('√', 0.0, 190.0, 10.0, 1),
            char_at('π', 5.0, 190.0, 10.0, 1),
        ];
        let blocks = detect_formula_regions(&chars);
        assert_eq!(blocks.len(), 1, "垂直相邻的公式行应该合并成一个块，不是两个");
    }

    #[test]
    fn formula_detection_plain_prose_has_zero_blocks() {
        let chars = vec![
            char_at('h', 0.0, 100.0, 10.0, 0),
            char_at('e', 5.0, 100.0, 10.0, 0),
            char_at('l', 10.0, 100.0, 10.0, 0),
            char_at('l', 15.0, 100.0, 10.0, 0),
            char_at('o', 20.0, 100.0, 10.0, 0),
        ];
        assert!(detect_formula_regions(&chars).is_empty(), "纯正文字符不该产出任何公式块");
    }

    #[test]
    fn formula_detection_empty_input() {
        assert!(detect_formula_regions(&[]).is_empty());
    }

    #[test]
    fn formula_detection_far_lines_not_merged() {
        let chars = vec![
            char_at('π', 0.0, 500.0, 10.0, 0),
            char_at('α', 0.0, 100.0, 10.0, 10), // 行号差很远，不该合并
        ];
        let blocks = detect_formula_regions(&chars);
        assert_eq!(blocks.len(), 2, "相隔很远的两处公式不该被误合并成一个块");
    }

    // ---- 字号识别标题 ----

    #[test]
    fn heading_detection_picks_larger_font_lines() {
        let mut page = Vec::new();
        // 正文：字号 10，多次出现占多数。
        for i in 0..20 {
            page.push(char_at('a', i as f64, 100.0, 10.0, 1));
        }
        // 标题行：字号 16（1.6 倍正文），字号更大。
        for i in 0..5 {
            page.push(char_at('T', i as f64, 200.0, 16.0, 0));
        }
        let headings = detect_headings_by_font_size(&[page]);
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].level, 1);
    }

    #[test]
    fn heading_detection_zero_candidates_when_uniform_size() {
        let page: Vec<PositionedChar> = (0..20).map(|i| char_at('a', i as f64, 100.0, 10.0, 0)).collect();
        assert!(detect_headings_by_font_size(&[page]).is_empty(), "字号完全统一时不该识别出标题");
    }

    #[test]
    fn heading_detection_ranks_levels_by_size() {
        let mut p1 = Vec::new();
        for i in 0..10 {
            p1.push(char_at('a', i as f64, 100.0, 10.0, 1));
        }
        for i in 0..3 {
            p1.push(char_at('T', i as f64, 200.0, 20.0, 0)); // 最大字号 → level 1
        }
        let mut p2 = Vec::new();
        for i in 0..10 {
            p2.push(char_at('a', i as f64, 100.0, 10.0, 1));
        }
        for i in 0..3 {
            p2.push(char_at('S', i as f64, 200.0, 14.0, 0)); // 次大字号 → level 2
        }
        let headings = detect_headings_by_font_size(&[p1, p2]);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].level, 2);
    }

    // ---- 分类（拿真实 pdflatex 样本核对，不是拍脑袋） ----

    #[test]
    fn classify_real_pdflatex_sample_is_text_layer() {
        assert_eq!(classify_pdf_bytes(SAMPLE_PDF), PdfKind::TextLayer, "真实 pdflatex 文字样本应该判定为有文字层");
    }

    #[test]
    fn classify_one_image_per_page_pdf_is_comic() {
        const RED_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
            0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
            0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
            0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
            0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        // `images_to_pdf` 的 MediaBox 直接等于图片像素尺寸（1px=1pt，见其文档注释），宽高比
        // 天然跟页面一致——不需要专门构造特定尺寸的图片，这条判据本来就该命中。
        let img = pdfwrite::image_from_bytes(RED_PNG).unwrap();
        let comic_pdf = pdfwrite::images_to_pdf(&[img.clone(), img.clone(), img]).unwrap();
        assert_eq!(classify_pdf_bytes(&comic_pdf), PdfKind::Comic);
    }

    #[test]
    fn classify_unparseable_bytes_defaults_to_no_text_layer() {
        assert_eq!(classify_pdf_bytes(b"not a pdf"), PdfKind::NoTextLayer, "解析失败要退到保守默认（只裁边），不能默认转换");
    }

    // ---- 端到端：真实样本转 EPUB ----

    #[test]
    fn optimize_pdf_to_epub_real_sample_produces_chapters_and_resources() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("sample.pdf");
        std::fs::write(&src, SAMPLE_PDF).unwrap();
        let (book, report) = optimize_pdf_to_epub(&src, |_, _| {}).unwrap();
        assert_eq!(report.pages, 3, "样本是 3 页");
        assert!(report.chapters >= 2, "没有书签，应该靠字号识别出至少 2 个标题层级的章节，实际 {}", report.chapters);
        assert!(report.formula_blocks >= 1, "样本含多处公式，应该探测到至少一个公式块");
        assert!(!book.resources.is_empty(), "样本含嵌入图片+公式块渲染图，resources 不该是空的");
        let bytes = crate::epub::assemble(&mut { book }).unwrap();
        assert!(!bytes.is_empty());
    }

    /// 章节 HTML 的可见文字（去标签、去空白、还原 `&amp;` 等）。
    fn visible(html: &str) -> String {
        let no_tags = regex::Regex::new(r"(?s)<[^>]*>").unwrap().replace_all(html, "");
        let t = no_tags.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
        t.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// **不允许变动书籍内容**的核心不变量：PDF 文字层提取出的每个字符，按顺序原样出现在 EPUB 文字流里，
    /// 不多不少（此前公式外接框内字符被整体丢弃，吞掉了 identity / 整半句正文；标题还被重复输出一遍）。
    #[test]
    fn epub_text_flow_equals_pdf_text_layer_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("sample.pdf");
        std::fs::write(&src, SAMPLE_PDF).unwrap();
        let expected: String = extract_positioned_text(SAMPLE_PDF)
            .unwrap()
            .iter()
            .flatten()
            .filter(|c| c.ch != '\u{0}')
            .map(|c| c.ch)
            .filter(|c| !c.is_whitespace())
            .collect();
        let (book, _) = optimize_pdf_to_epub(&src, |_, _| {}).unwrap();
        let got: String = book.chapters.iter().map(|c| visible(&c.html_body)).collect();
        assert_eq!(got, expected, "EPUB 文字流必须与 PDF 文字层逐字一致");
        // 具体回归点：紧贴公式的正文单词与整半句不能丢；标题只出现一次。
        assert!(got.contains("identity") && got.contains("quadraticformula"), "行内公式旁的单词被吞了");
        assert!(got.contains("Afinalshortsection") && got.contains("onemoreinlinesymbol"), "整半句正文被吞了");
        assert_eq!(got.matches("1Introduction").count(), 1, "章节标题不该重复输出");
    }

    #[test]
    fn promote_heading_upgrades_existing_paragraph_without_adding_text() {
        assert_eq!(promote_heading("1 Intro", "<p>1 Intro</p><p>body</p>".into()), "<h2>1 Intro</h2><p>body</p>");
        assert_eq!(promote_heading("1 Intro", "<p>1 Intro Some text</p>".into()), "<h2>1 Intro</h2><p>Some text</p>");
        // 章内找不到标题段落：不往正文里塞原书没有的字。
        assert_eq!(promote_heading("Missing", "<p>body</p>".into()), "<p>body</p>");
    }

    #[test]
    fn looks_like_pdf_derived_epub_detects_marker() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("sample.pdf");
        std::fs::write(&src, SAMPLE_PDF).unwrap();
        let (mut book, _) = optimize_pdf_to_epub(&src, |_, _| {}).unwrap();
        let bytes = crate::epub::assemble(&mut book).unwrap();
        let out = dir.path().join("out.epub");
        std::fs::write(&out, &bytes).unwrap();
        assert!(looks_like_pdf_derived_epub(&out));
    }

    #[test]
    fn looks_like_pdf_derived_epub_false_for_unrelated_zip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("not_epub.zip");
        let file = std::fs::File::create(&p).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file::<_, ()>("hello.txt", Default::default()).unwrap();
        std::io::Write::write_all(&mut zip, b"hi").unwrap();
        zip.finish().unwrap();
        assert!(!looks_like_pdf_derived_epub(&p));
    }

    // ---- 裁边路径 ----

    #[test]
    fn optimize_pdf_trim_only_comic_shaped_fixture_roundtrips() {
        // sample.pdf 是文字样本，不代表"裁边"路径的真实输入形状（裁边只服务一页一图的扫描件/
        // 漫画 PDF）——这里用既有的 `images_to_pdf`（漫画 EPUB→PDF 那条产线复用的写手，已经
        // 有自己的测试覆盖）现造一份"每页一张图"的合成 PDF，形状上才贴近这条路径真正会遇到的
        // 输入。
        const RED_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
            0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
            0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
            0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
            0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let img = pdfwrite::image_from_bytes(RED_PNG).unwrap();
        let comic_pdf = pdfwrite::images_to_pdf(&[img.clone(), img.clone(), img]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("comic.pdf");
        std::fs::write(&src, &comic_pdf).unwrap();
        let dst = dir.path().join("out.pdf");
        let report = optimize_pdf_trim_only(&src, &dst, |_, _| {}).unwrap();
        assert_eq!(report.pages, 3);
        assert!(pdfwrite::looks_like_own_bookconv_pdf(&dst), "裁边输出应该能被识别成自产 PDF");
        // 要有目录：源 PDF 没有书签 → 按页分段兜底，不是空的。
        let titles = pdfwrite::PdfFileReader::open(&dst).unwrap().outline_titles().unwrap();
        assert_eq!(titles, vec![(0, "第 1–3 页".to_string())]);
    }

    fn red_png_pdf(pages: usize, titles: &[(usize, String)]) -> Vec<u8> {
        // 与设备页面(954×1696)同宽高比的整页图，才满足"单张整页图"判据。
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(95, 169, image::Rgb([200, 30, 30])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let img = pdfwrite::image_from_bytes(&png).unwrap();
        let imgs: Vec<_> = (0..pages).map(|_| img.clone()).collect();
        pdfwrite::images_to_pdf_with_toc(&imgs, titles).unwrap()
    }

    #[test]
    fn trim_only_preserves_original_bookmarks() {
        // 原书签必须保留（不许变动书籍内容 + 要有目录），页码原样。
        let titles = vec![(0, "第一卷".to_string()), (2, "第二卷".to_string())];
        let pdf = red_png_pdf(4, &titles);
        let dir = tempfile::tempdir().unwrap();
        let (src, dst) = (dir.path().join("a.pdf"), dir.path().join("o.pdf"));
        std::fs::write(&src, &pdf).unwrap();
        optimize_pdf_trim_only(&src, &dst, |_, _| {}).unwrap();
        assert_eq!(pdfwrite::PdfFileReader::open(&dst).unwrap().outline_titles().unwrap(), titles);
    }

    #[test]
    fn trim_only_refuses_pdf_with_text_layer_and_leaves_it_untouched() {
        // 文字样本含大量文字：重写页面会丢文字层——必须拒绝，且不产出任何文件。
        let dir = tempfile::tempdir().unwrap();
        let (src, dst) = (dir.path().join("t.pdf"), dir.path().join("o.pdf"));
        std::fs::write(&src, SAMPLE_PDF).unwrap();
        let err = optimize_pdf_trim_only(&src, &dst, |_, _| {}).unwrap_err();
        assert!(err.contains("文字"), "应说明是因为文字层: {err}");
        assert!(!dst.exists());
    }
}
