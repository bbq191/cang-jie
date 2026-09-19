//! 最小 PDF 写入器：一串图片（每图一页）→ PDF。贴合 epub.rs「手搓、零 C 依赖」风格。
//! JPEG 直接作 /DCTDecode 嵌入（不解码、不重编码——漫画页几乎都是 JPEG）；
//! PNG 用现成 `png` crate 解码成原始像素、miniz_oxide zlib 压成 /FlateDecode。
//! 给漫画（CBZ）用——xochitl 原生 PDF 翻页比 EPUB 顺、一页一图最合漫画。

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ColorSpace {
    Gray,
    Rgb,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Filter {
    Dct,
    Flate,
}
impl ColorSpace {
    fn pdf_name(self) -> &'static str {
        match self {
            ColorSpace::Gray => "/DeviceGray",
            ColorSpace::Rgb => "/DeviceRGB",
        }
    }
}
impl Filter {
    fn pdf_name(self) -> &'static str {
        match self {
            Filter::Dct => "/DCTDecode",
            Filter::Flate => "/FlateDecode",
        }
    }
}

/// 一页图片：宽高 + 色彩空间 + 位深 + PDF 过滤器 + 已就绪的流数据（JPEG 原字节 / zlib 压缩像素）。
/// `bits`=每分量位深：常规图 8；「漫画省刷新」1-bit 黑白页 = 1（DeviceGray，1 位/像素，行按字节对齐）。
#[derive(Clone)]
pub struct PdfImage {
    pub width: u32,
    pub height: u32,
    pub color: ColorSpace,
    pub bits: u8,
    pub filter: Filter,
    pub data: Vec<u8>,
}

/// 「漫画省刷新」页：已抖动成双色（0/255）的灰度图 → 1-bit /DeviceGray PDF 图。
/// 行内像素 MSB 优先打包（bit=1 表白、0 表黑），**每行按字节对齐**（PDF 图像扫描行要求），
/// 再 miniz_oxide zlib 压成 /FlateDecode。相比 8-bit 灰度直存，体积 ~1/8 且触发面板更轻的 mono 波形。
pub fn bilevel_image(gray: &image::GrayImage) -> PdfImage {
    let (w, h) = (gray.width(), gray.height());
    let row_bytes = (w as usize).div_ceil(8); // 每行字节数（向上取整到字节）
    let mut packed = vec![0u8; row_bytes * h as usize];
    for y in 0..h {
        let row_off = y as usize * row_bytes;
        for x in 0..w {
            // 抖动产物只有 0/255；≥128 视作白（bit=1）。
            if gray.get_pixel(x, y)[0] >= 128 {
                packed[row_off + (x as usize >> 3)] |= 0x80 >> (x & 7);
            }
        }
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&packed, 6);
    PdfImage { width: w, height: h, color: ColorSpace::Gray, bits: 1, filter: Filter::Flate, data: compressed }
}

/// 按魔数识别 JPEG/PNG，产出可嵌入 PDF 的 PdfImage。
pub fn image_from_bytes(data: &[u8]) -> Result<PdfImage, String> {
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        jpeg_to_image(data)
    } else if data.len() >= 8 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        png_to_image(data)
    } else {
        Err("非 JPEG/PNG 图片".into())
    }
}

/// 解析 JPEG 的 SOF 段取宽高与分量数；像素原样 DCTDecode 嵌入，不解码。
fn jpeg_to_image(data: &[u8]) -> Result<PdfImage, String> {
    if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err("非 JPEG（缺 SOI）".into());
    }
    let mut i = 2usize;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        // 跳过连续填充 0xFF，定位到 marker 字节
        let mut j = i + 1;
        while j < data.len() && data[j] == 0xFF {
            j += 1;
        }
        if j >= data.len() {
            break;
        }
        let marker = data[j];
        i = j; // data[i] = marker
        // 无长度段：SOI/EOI/RSTn/TEM
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 1;
            continue;
        }
        if i + 3 > data.len() {
            break;
        }
        let seg_len = ((data[i + 1] as usize) << 8) | data[i + 2] as usize;
        // SOF 标记 C0..CF，排除 C4(DHT)/C8(JPG)/CC(DAC)
        let is_sof =
            (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
        if is_sof {
            // 段内容：precision(1) height(2) width(2) components(1)
            if i + 8 > data.len() {
                return Err("JPEG SOF 段截断".into());
            }
            let h = ((data[i + 4] as u32) << 8) | data[i + 5] as u32;
            let w = ((data[i + 6] as u32) << 8) | data[i + 7] as u32;
            let comps = data[i + 8];
            let color = match comps {
                1 => ColorSpace::Gray,
                3 => ColorSpace::Rgb,
                _ => return Err(format!("JPEG 不支持的分量数 {comps}（CMYK 等）")),
            };
            if w == 0 || h == 0 {
                return Err("JPEG 宽高为 0".into());
            }
            return Ok(PdfImage {
                width: w,
                height: h,
                color,
                bits: 8,
                filter: Filter::Dct,
                data: data.to_vec(),
            });
        }
        i += 1 + seg_len; // marker 字节 + 段（长度含 2 个长度字节自身）
    }
    Err("JPEG 未找到 SOF 段".into())
}

/// PNG 用 png crate 解码归一到 8-bit 灰度/RGB（EXPAND 展开调色板/低位深、STRIP_16 降位深；
/// 带 alpha 合成到白底），再 miniz_oxide zlib 压成 /FlateDecode 流。
fn png_to_image(data: &[u8]) -> Result<PdfImage, String> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("PNG 头解码: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG 帧解码: {e}"))?;
    let (w, h) = (info.width, info.height);
    let bytes = &buf[..info.buffer_size()];
    let (color, pixels): (ColorSpace, Vec<u8>) = match info.color_type {
        png::ColorType::Grayscale => (ColorSpace::Gray, bytes.to_vec()),
        png::ColorType::Rgb => (ColorSpace::Rgb, bytes.to_vec()),
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity((w * h) as usize);
            for px in bytes.chunks_exact(2) {
                let (g, a) = (px[0] as u32, px[1] as u32);
                out.push(((g * a + 255 * (255 - a)) / 255) as u8);
            }
            (ColorSpace::Gray, out)
        }
        png::ColorType::Rgba => {
            let mut out = Vec::with_capacity((w * h * 3) as usize);
            for px in bytes.chunks_exact(4) {
                let a = px[3] as u32;
                for c in 0..3 {
                    out.push(((px[c] as u32 * a + 255 * (255 - a)) / 255) as u8);
                }
            }
            (ColorSpace::Rgb, out)
        }
        png::ColorType::Indexed => {
            // EXPAND 应已展开调色板；仍到此说明非常规，明确报错胜过产坏图
            return Err("PNG 调色板未展开（异常）".into());
        }
    };
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&pixels, 6);
    Ok(PdfImage {
        width: w,
        height: h,
        color,
        bits: 8,
        filter: Filter::Flate,
        data: compressed,
    })
}

/// 把若干页图片组装成 PDF 字节。每页 MediaBox = 图片像素尺寸（1px=1pt）。
/// 对象编号：1=Catalog，2=Pages，之后每页 3 个对象（Page/Image/Contents）。
pub fn images_to_pdf(images: &[PdfImage]) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("PDF 至少要有一页".into());
    }
    let n = images.len();
    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(2 + n * 3);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let mut kids = String::new();
    for i in 0..n {
        kids.push_str(&format!("{} 0 R ", 3 + i * 3));
    }
    objects.push(
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.trim_end(), n).into_bytes(),
    );
    for (i, img) in images.iter().enumerate() {
        let page_id = 3 + i * 3;
        let image_id = page_id + 1;
        let contents_id = page_id + 2;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /XObject << /Im0 {im} 0 R >> >> /Contents {con} 0 R >>",
                w = img.width, h = img.height, im = image_id, con = contents_id
            )
            .into_bytes(),
        );
        let mut xobj = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace {cs} /BitsPerComponent {bpc} /Filter {f} /Length {len} >>\nstream\n",
            w = img.width, h = img.height, cs = img.color.pdf_name(), bpc = img.bits, f = img.filter.pdf_name(), len = img.data.len()
        ).into_bytes();
        xobj.extend_from_slice(&img.data);
        xobj.extend_from_slice(b"\nendstream");
        objects.push(xobj);
        let content = format!("q\n{w} 0 0 {h} 0 0 cm\n/Im0 Do\nQ\n", w = img.width, h = img.height);
        objects.push(
            format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content).into_bytes(),
        );
    }
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets: Vec<usize> = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_off = out.len();
    let count = objects.len() + 1; // 含空闲对象 0
    out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n")
            .as_bytes(),
    );
    Ok(out)
}

/// 统一设备页面尺寸（PDF 1px=1pt 沿用本文件既有惯例）——复用 EPUB 漫画线同一套设备屏幕常量
/// （真机实测 PDF 页面尺寸精确等于这组数字时，左右留白量得 0.00%，是这次新增摆位逻辑的依据）。
pub const PDF_PAGE_W: u32 = crate::imgopt::MAX_SHORT_EDGE;
pub const PDF_PAGE_H: u32 = crate::imgopt::MAX_EDGE;

/// 图片在统一设备页面里怎么摆：优先按宽度撑满、左右各留 1%（常见情况——漫画页比设备"矮"，
/// 撑满宽度后自然在上下留出对称留白，居中摆）；如果这样会导致高度溢出页面（罕见的极端竖长图），
/// 改成按高度撑满、不留任何上下空间，左右留白让步（避免内容溢出屏幕优先于"左右只留1%"这个目标，
/// 两者冲突时选前者）。全程只是计算一个缩放+平移矩阵，不碰图片本身一个像素。
/// 返回 `(绘制宽, 绘制高, x偏移, y偏移)`，直接拼进 PDF Contents 流的 `cm` 矩阵。
pub fn place_image(img_w: u32, img_h: u32, page_w: u32, page_h: u32) -> (f32, f32, f32, f32) {
    let (img_w, img_h, page_w, page_h) = (img_w as f32, img_h as f32, page_w as f32, page_h as f32);
    let target_w = page_w * 0.98;
    let scaled_h = img_h * (target_w / img_w);
    if scaled_h <= page_h {
        (target_w, scaled_h, page_w * 0.01, (page_h - scaled_h) / 2.0)
    } else {
        let scale_h = page_h / img_h;
        let scaled_w = img_w * scale_h;
        (scaled_w, page_h, (page_w - scaled_w) / 2.0, 0.0)
    }
}

/// PDF 书签标题必须用 UTF-16BE（带 `\xFE\xFF` BOM）才能正确显示中文，纯 ASCII 字面量不行。
fn pdf_text_utf16be(s: &str) -> Vec<u8> {
    let mut out = vec![0xFEu8, 0xFF];
    for u in s.encode_utf16() {
        out.push((u >> 8) as u8);
        out.push((u & 0xFF) as u8);
    }
    out
}

/// 包成 PDF literal string `(...)`，转义 `(`/`)`/`\` 三个特殊字节（UTF-16BE 的某个码元低/高字节
/// 凑巧撞上这三个 ASCII 值也要转义，否则会被误判为字符串提前结束）。
fn pdf_literal_string(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 2);
    out.push(b'(');
    for &b in bytes {
        if b == b'(' || b == b')' || b == b'\\' {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

/// `images_to_pdf` 的平行版本，供 EPUB 漫画→PDF 这条新路径用：①统一页面尺寸（不再跟随每张图
/// 自己的像素尺寸），图片按 `place_image` 算出的位置摆放，不裁不拉伸；②带书签目录（Outlines）
/// ——`titles` 是 `(0-based 页码, 标题)` 列表，允许为空（不带任何目录，此时 Catalog 不含
/// `/Outlines`，效果等同旧版 `images_to_pdf` 只是页面尺寸统一了）。**不修改 `images_to_pdf`
/// 本体**——CBZ→PDF 那条现有路径继续用它，互不影响。
pub fn images_to_pdf_with_toc(images: &[PdfImage], titles: &[(usize, String)]) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("PDF 至少要有一页".into());
    }
    let n = images.len();
    let has_toc = !titles.is_empty();
    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(2 + n * 3 + if has_toc { 1 + titles.len() } else { 0 });

    objects.push(Vec::new()); // obj 1 = Catalog，占位，末尾知道 outline id 再回填
    let mut kids = String::new();
    for i in 0..n {
        kids.push_str(&format!("{} 0 R ", 3 + i * 3));
    }
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.trim_end(), n).into_bytes());

    for (i, img) in images.iter().enumerate() {
        let page_id = 3 + i * 3;
        let image_id = page_id + 1;
        let contents_id = page_id + 2;
        let (dw, dh, x, y) = place_image(img.width, img.height, PDF_PAGE_W, PDF_PAGE_H);
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Im0 {im} 0 R >> >> /Contents {con} 0 R >>",
                pw = PDF_PAGE_W, ph = PDF_PAGE_H, im = image_id, con = contents_id
            )
            .into_bytes(),
        );
        let mut xobj = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace {cs} /BitsPerComponent {bpc} /Filter {f} /Length {len} >>\nstream\n",
            w = img.width, h = img.height, cs = img.color.pdf_name(), bpc = img.bits, f = img.filter.pdf_name(), len = img.data.len()
        ).into_bytes();
        xobj.extend_from_slice(&img.data);
        xobj.extend_from_slice(b"\nendstream");
        objects.push(xobj);
        let content = format!("q\n{dw} 0 0 {dh} {x} {y} cm\n/Im0 Do\nQ\n");
        objects.push(
            format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content).into_bytes(),
        );
    }

    let mut outline_root_id = 0usize;
    if has_toc {
        let base = 3 + n * 3; // 下一个空闲对象号（page 三元组全部占完之后）
        outline_root_id = base;
        let first_id = base + 1;
        let last_id = base + titles.len();
        objects.push(
            format!(
                "<< /Type /Outlines /First {first_id} 0 R /Last {last_id} 0 R /Count {} >>",
                titles.len()
            )
            .into_bytes(),
        );
        for (i, (page_idx, title)) in titles.iter().enumerate() {
            let page_id = 3 + page_idx * 3;
            let mut obj = b"<< /Title ".to_vec();
            obj.extend_from_slice(&pdf_literal_string(&pdf_text_utf16be(title)));
            obj.extend_from_slice(format!(" /Parent {outline_root_id} 0 R").as_bytes());
            if i > 0 {
                obj.extend_from_slice(format!(" /Prev {} 0 R", base + i).as_bytes());
            }
            if i + 1 < titles.len() {
                obj.extend_from_slice(format!(" /Next {} 0 R", base + 2 + i).as_bytes());
            }
            obj.extend_from_slice(format!(" /Dest [{page_id} 0 R /Fit] >>").as_bytes());
            objects.push(obj);
        }
    }

    objects[0] = if has_toc {
        format!("<< /Type /Catalog /Pages 2 0 R /Outlines {outline_root_id} 0 R /PageMode /UseOutlines >>").into_bytes()
    } else {
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()
    };

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets: Vec<usize> = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_off = out.len();
    let count = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n")
            .as_bytes(),
    );
    Ok(out)
}

fn memfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn find_obj_body(pdf: &[u8], id: usize) -> Result<&[u8], String> {
    let marker = format!("\n{id} 0 obj\n");
    let start = memfind(pdf, marker.as_bytes()).ok_or(format!("找不到对象 {id}"))? + marker.len();
    let rel_end = memfind(&pdf[start..], b"\nendobj").ok_or(format!("对象 {id} 缺 endobj"))?;
    Ok(&pdf[start..start + rel_end])
}

fn parse_uint_after(body: &[u8], label: &str) -> Option<u32> {
    let idx = memfind(body, label.as_bytes())? + label.len();
    let mut end = idx;
    while end < body.len() && body[end].is_ascii_digit() {
        end += 1;
    }
    if end == idx {
        return None;
    }
    std::str::from_utf8(&body[idx..end]).ok()?.parse().ok()
}

/// 解出 `/Title (...)` 这个 literal string 的原始字节（保留转义前的 `\(`/`\)`/`\\`）。
fn parse_pdf_literal(body: &[u8], after: &str) -> Option<Vec<u8>> {
    let idx = memfind(body, after.as_bytes())? + after.len();
    if body.get(idx) != Some(&b'(') {
        return None;
    }
    let mut i = idx + 1;
    let mut out = Vec::new();
    while i < body.len() {
        match body[i] {
            b'\\' if i + 1 < body.len() => {
                out.push(body[i + 1]);
                i += 2;
            }
            b')' => return Some(out),
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    None
}

fn utf16be_to_string(mut bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        bytes = &bytes[2..];
    }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| ((c[0] as u16) << 8) | c[1] as u16).collect();
    String::from_utf16_lossy(&units)
}

/// 廉价识别"这份 PDF 是不是我们自己 `images_to_pdf_with_toc` 产出的"——只读文件开头一小段
/// （Catalog 永远是第一个写的对象，在文件最前面），不用把整份 PDF 读进内存解析对象结构。给
/// `book-serve::Staging::list()` 这种高频调用场景判断"漫画 PDF 是否已优化"用。
pub fn looks_like_own_comic_pdf(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(f) = std::fs::File::open(path) else { return false };
    let mut head = Vec::with_capacity(4096);
    if f.take(4096).read_to_end(&mut head).is_err() {
        return false;
    }
    memfind(&head, b"/Type /Catalog").is_some() && memfind(&head, b"/Outlines").is_some()
}

/// 总页数（`/Type /Pages` 对象的 `/Count`）——只认自己生成的固定对象编号结构，喂陌生 PDF 大概率 `Err`。
pub fn page_count(pdf: &[u8]) -> Result<usize, String> {
    let pages_body = find_obj_body(pdf, 2)?;
    parse_uint_after(pages_body, "/Count ").ok_or_else(|| "Pages 对象缺 /Count".to_string()).map(|n| n as usize)
}

/// **只认自己 `images_to_pdf_with_toc` 生成的 PDF**（对象编号规则、MediaBox 全部已知固定），
/// 抽出 `[start,end)` 页范围的图片数据 + 落在这个范围内的书签（页码改写成范围内的本地下标）。
/// 不是通用 PDF 解析器，喂陌生第三方 PDF 大概率直接 `Err`（找不到期望的对象），这是有意为之——
/// 分卷投递只该处理"我们自己产出的漫画 PDF"，别的 PDF 走现状"整本超限拒绝"的老路径。
pub fn extract_pages(pdf: &[u8], start: usize, end: usize) -> Result<(Vec<PdfImage>, Vec<(usize, String)>), String> {
    let pages_body = find_obj_body(pdf, 2)?;
    let n = parse_uint_after(pages_body, "/Count ").ok_or("Pages 对象缺 /Count")? as usize;
    if start >= end || end > n {
        return Err(format!("页码范围 [{start},{end}) 超出总页数 {n}"));
    }

    let mut images = Vec::with_capacity(end - start);
    for page_idx in start..end {
        let image_id = 3 + page_idx * 3 + 1;
        let body = find_obj_body(pdf, image_id)?;
        let w = parse_uint_after(body, "/Width ").ok_or("图片对象缺 /Width")?;
        let h = parse_uint_after(body, "/Height ").ok_or("图片对象缺 /Height")?;
        let color = if memfind(body, b"/DeviceGray").is_some() { ColorSpace::Gray } else { ColorSpace::Rgb };
        let bits = parse_uint_after(body, "/BitsPerComponent ").ok_or("图片对象缺 /BitsPerComponent")? as u8;
        let filter = if memfind(body, b"/DCTDecode").is_some() { Filter::Dct } else { Filter::Flate };
        let len = parse_uint_after(body, "/Length ").ok_or("图片对象缺 /Length")? as usize;
        let stream_marker = b"stream\n";
        let stream_at = memfind(body, stream_marker).ok_or("图片对象缺 stream")? + stream_marker.len();
        if stream_at + len > body.len() {
            return Err(format!("图片对象 {image_id} 流数据被截断"));
        }
        images.push(PdfImage { width: w, height: h, color, bits, filter, data: body[stream_at..stream_at + len].to_vec() });
    }

    let mut titles = Vec::new();
    let catalog = find_obj_body(pdf, 1)?;
    if let Some(outline_root) = parse_uint_after(catalog, "/Outlines ") {
        let root_body = find_obj_body(pdf, outline_root as usize)?;
        let first = parse_uint_after(root_body, "/First ").ok_or("Outlines 缺 /First")? as usize;
        let last = parse_uint_after(root_body, "/Last ").ok_or("Outlines 缺 /Last")? as usize;
        for item_id in first..=last {
            let item_body = find_obj_body(pdf, item_id)?;
            let dest_page_id = parse_uint_after(item_body, "/Dest [").ok_or("书签缺 /Dest")? as usize;
            let page_idx = (dest_page_id - 3) / 3;
            if page_idx >= start && page_idx < end {
                let title_bytes = parse_pdf_literal(item_body, "/Title ").ok_or("书签缺 /Title")?;
                titles.push((page_idx - start, utf16be_to_string(&title_bytes)));
            }
        }
    }
    Ok((images, titles))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1x1 红色 PNG（真实字节，含 IHDR/IDAT/IEND）
    const RED_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn png_decodes_to_rgb() {
        let img = image_from_bytes(RED_PNG).unwrap();
        assert_eq!((img.width, img.height), (1, 1));
        assert_eq!(img.color, ColorSpace::Rgb);
        assert_eq!(img.filter, Filter::Flate);
    }

    #[test]
    fn jpeg_sof_parses_dimensions() {
        // 构造最小 JPEG 骨架：SOI + SOF0(3 分量 2x3) + EOI
        let jpeg: &[u8] = &[
            0xFF, 0xD8, // SOI
            0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x03, 0x00, 0x02, 0x03, 0x01, 0x11, 0x00, 0x02,
            0x11, 0x01, 0x03, 0x11, 0x01, // SOF0: h=3 w=2 comps=3
            0xFF, 0xD9, // EOI
        ];
        let img = jpeg_to_image(jpeg).unwrap();
        assert_eq!((img.width, img.height), (2, 3));
        assert_eq!(img.color, ColorSpace::Rgb);
        assert_eq!(img.filter, Filter::Dct);
        assert_eq!(img.data, jpeg); // 原字节直嵌
    }

    #[test]
    fn pdf_structure_wellformed() {
        let img = image_from_bytes(RED_PNG).unwrap();
        let pdf = images_to_pdf(&[img]).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.7"), "缺 PDF 头");
        assert!(pdf.ends_with(b"%%EOF\n"), "缺 EOF");
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Type /Catalog"));
        assert!(s.contains("/Type /Pages"));
        assert!(s.contains("/Count 1"));
        assert!(s.contains("/Subtype /Image"));
        assert!(s.contains("/MediaBox [0 0 1 1]"));
        assert!(s.contains("startxref"));
        // xref 条目数 = 对象数(1 catalog +1 pages +3 每页) + 空闲 0 = 6
        assert!(s.contains("xref\n0 6\n"), "xref 计数错: {s}");
    }

    #[test]
    fn empty_pages_error() {
        assert!(images_to_pdf(&[]).is_err());
    }

    #[test]
    fn place_image_wide_page_centers_top_bottom_with_one_percent_side_margin() {
        // 常见漫画页：比设备页面"矮"（宽/高比 0.7 > 设备 0.5625），按宽度撑满，上下自动留白对称。
        let (dw, dh, x, y) = place_image(700, 1000, 954, 1696);
        assert!((x - 954.0 * 0.01).abs() < 0.01, "左边距应为页宽 1%: x={x}");
        assert!((dw - 954.0 * 0.98).abs() < 0.01, "绘制宽应为页宽 98%: dw={dw}");
        let bottom_gap = 1696.0 - dh - y;
        assert!((y - bottom_gap).abs() < 0.01, "上下留白应对称: top={y} bottom={bottom_gap}");
    }

    #[test]
    fn place_image_extreme_tall_image_falls_back_to_height_fit_without_overflow() {
        // 极端竖长图：按 98% 宽度撑满会导致高度溢出页面，改按高度撑满避免溢出。
        let (dw, dh, x, y) = place_image(100, 1000, 954, 1696);
        assert_eq!(y, 0.0, "改按高度撑满时不留上下边距");
        assert!((dh - 1696.0).abs() < 0.01, "绘制高应等于页高: dh={dh}");
        assert!(dw < 954.0 * 0.98, "改按高度撑满后绘制宽应小于 98% 页宽（左右留白让步）: dw={dw}");
        assert!(x > 0.0, "水平应居中留白");
    }

    #[test]
    fn images_to_pdf_with_toc_structure_and_chinese_title() {
        let img1 = image_from_bytes(RED_PNG).unwrap();
        let img2 = image_from_bytes(RED_PNG).unwrap();
        let pdf = images_to_pdf_with_toc(&[img1, img2], &[(0, "镖人 卷一".to_string())]).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.7"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Type /Outlines"), "缺 Outlines 根对象");
        assert!(s.contains("/PageMode /UseOutlines"));
        assert!(s.contains(&format!("/MediaBox [0 0 {PDF_PAGE_W} {PDF_PAGE_H}]")), "页面尺寸应统一为设备尺寸");
        // 中文标题必须是 UTF-16BE + BOM 编码，不能原样出现 UTF-8 字节
        assert!(!s.contains("镖人"), "中文标题不该以 UTF-8 明文出现在 PDF 里");
        let title_utf16 = pdf_text_utf16be("镖人 卷一");
        let title_literal = pdf_literal_string(&title_utf16);
        assert!(pdf.windows(title_literal.len()).any(|w| w == title_literal.as_slice()), "找不到编码后的书签标题字节");
    }

    #[test]
    fn images_to_pdf_with_toc_empty_titles_has_no_outlines() {
        let img = image_from_bytes(RED_PNG).unwrap();
        let pdf = images_to_pdf_with_toc(&[img], &[]).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(!s.contains("/Outlines"), "空标题列表不该产出 Outlines 对象");
    }

    #[test]
    fn extract_pages_round_trips_images_and_titles() {
        let images: Vec<PdfImage> = (0..4).map(|_| image_from_bytes(RED_PNG).unwrap()).collect();
        let titles = vec![(0, "第一卷".to_string()), (2, "第二卷".to_string())];
        let pdf = images_to_pdf_with_toc(&images, &titles).unwrap();

        let (extracted, extracted_titles) = extract_pages(&pdf, 0, 4).unwrap();
        assert_eq!(extracted.len(), 4);
        for (orig, got) in images.iter().zip(extracted.iter()) {
            assert_eq!(orig.width, got.width);
            assert_eq!(orig.height, got.height);
            assert_eq!(orig.color, got.color);
            assert_eq!(orig.filter, got.filter);
            assert_eq!(orig.data, got.data, "抽取的图片流字节应跟生成时完全一致");
        }
        assert_eq!(extracted_titles, titles);

        // 切一段范围 [2,4)，页码应重新映射成范围内的本地下标（原 2 -> 0）
        let (sub_images, sub_titles) = extract_pages(&pdf, 2, 4).unwrap();
        assert_eq!(sub_images.len(), 2);
        assert_eq!(sub_titles, vec![(0, "第二卷".to_string())]);
    }

    #[test]
    fn looks_like_own_comic_pdf_detects_outline_not_plain_pdf() {
        let img = image_from_bytes(RED_PNG).unwrap();
        let with_toc = images_to_pdf_with_toc(&[img], &[(0, "卷一".to_string())]).unwrap();
        let img2 = image_from_bytes(RED_PNG).unwrap();
        let plain = images_to_pdf(&[img2]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.pdf");
        let p2 = dir.path().join("b.pdf");
        std::fs::write(&p1, &with_toc).unwrap();
        std::fs::write(&p2, &plain).unwrap();
        assert!(looks_like_own_comic_pdf(&p1));
        assert!(!looks_like_own_comic_pdf(&p2), "无 Outlines 的普通 PDF 不该被误判为我们自己的产物");
    }

    #[test]
    fn extract_pages_rejects_out_of_range() {
        let img = image_from_bytes(RED_PNG).unwrap();
        let pdf = images_to_pdf_with_toc(&[img], &[]).unwrap();
        assert!(extract_pages(&pdf, 0, 5).is_err());
    }
}
