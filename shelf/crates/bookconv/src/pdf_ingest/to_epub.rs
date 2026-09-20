//! 有文字层 PDF → EPUB（正文、标题、图片、公式区域截图）。
use super::*;

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
pub(super) fn decode_pdf_image_to_bytes(doc: &lopdf::Document, img: &lopdf::xobject::PdfImage) -> Result<Vec<u8>, String> {
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

pub(super) fn encode_raw_pixels_png(pixels: &[u8], w: u32, h: u32, gray: bool) -> Result<Vec<u8>, String> {
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
    let book = Book {
        meta: BookMeta { book_id: format!("{PDF_BOOK_ID_PREFIX}{title}"), title, author: String::new(), language: "zh".to_string(), publisher: String::new(), cover: None, cover_ext: String::new(), cover_media_type: String::new() },
        chapters,
        resources,
    };
    let report = PdfToEpubReport { pages: page_count, chapters: book.chapters.len(), images: total_images, formula_blocks: total_formula_blocks };
    Ok((book, report))
}

/// 把章节正文里**原有**的标题段落升级成 `<h2>`，不额外添加文字（此前 `<h2>标题</h2>` + 正文里原有的
/// 标题段落，同一个标题在章内出现两次，属于改动书籍内容）。匹配 `<p>标题</p>`（整段就是标题）或
/// `<p>标题 …`（标题与后文同段，拆成 `<h2>` + 剩余 `<p>`）；章内找不到就**不加**——宁可标题只留在
/// 目录里也不往正文里塞原书没有的字。
pub(super) fn promote_heading(title: &str, body: String) -> String {
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

pub(super) fn point_in_bbox(x: f64, y: f64, b: &BBox) -> bool {
    x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1
}

pub(super) fn pdf_doc_title(doc: &lopdf::Document) -> Option<String> {
    let info_ref = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
    let info = doc.get_dictionary(info_ref).ok()?;
    let title = info.get(b"Title").ok()?;
    let bytes = title.as_str().ok()?;
    Some(String::from_utf8_lossy(bytes).trim().to_string()).filter(|s| !s.is_empty())
}

/// 把整页位图裁到某个公式包围盒对应的像素区域，编码成 PNG。PDF 用户空间→像素空间的换算用
/// `page.render_dimensions()`/MediaBox 尺寸算缩放比（不精确到子像素，公式裁剪本来就不需要）。
pub(super) fn crop_pixmap_to_png(pixmap: &hayro::vello_cpu::Pixmap, region: &BBox, page: &hayro::hayro_syntax::page::Page) -> Option<Vec<u8>> {
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
