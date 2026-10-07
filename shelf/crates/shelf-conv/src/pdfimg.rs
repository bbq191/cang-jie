//! PDF 页图片处理（入库 PDF 裁边 `pdf_ingest::trim`、`pdfwrite` 的页面尺寸）。
//!
//! 从原 cang-jie `bookconv::imgopt` 搬来的 PDF 专用部分：sheng-ren 的 bookconv 只做 EPUB，没有 PDF 页处理，裁边/缩放/编码这几个
//! 内部函数在那边是私有的，所以这里留一份（`jpegopt` 的无损哈夫曼重做直接用 sheng-ren 公开的 `bookconv::jpegopt`）。
//! 纪律同原模块：只缩不放（JPEG 低分辨率页例外，见 [`prepare_comic_page_for_pdf`]）、保宽高比、保原格式、任何失败原样保留。

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::ImageFormat;
use std::io::Cursor;

/// Move 屏最长像素边 / 最短像素边。
pub const MAX_EDGE: u32 = 1696;
pub const MAX_SHORT_EDGE: u32 = 954;
/// 单张图片允许解码的像素数上限（w×h）——设备内存实测定的阈值（900 万像素峰值约 100–110MB），超限的图原样保留。
/// 来历见原 cang-jie `bookconv::imgopt::MAX_DECODE_PIXELS` 的说明（2026-09-19 真机 VmHWM 事故）。
const MAX_DECODE_PIXELS: u64 = 9_000_000;

fn within_decode_budget(w: u32, h: u32) -> bool {
    (w as u64) * (h as u64) <= MAX_DECODE_PIXELS
}
/// 重编码 JPEG 质量（0–100）。85 = 视觉无损级，体积/画质平衡；e-ink 上更看不出差异。
/// 漫画页专用重编码质量——EPUB 线原则④"漫画不允许压画质"：超限时仍必须缩到屏幕框内（否则设备渲染
/// 异常），但不该像普通插图那样再吃一道 85 质量的有损重编码，95 更接近视觉无损。
/// **缩小、放大、补白一律用它**：2026-09-30 用户定，小漫画页预放大后也用 95（此前放大页用 85 控体积——镖人卷02 做 PDF 时
/// q95 21MB→113MB、q85 约 71MB；与 sheng-ren 2026-09-29 的决定一致）。代价是低分辨率漫画的产物明显变大。
const JPEG_QUALITY_COMIC: u8 = 95;
/// 预放大的倍数上限：超过视为缩略图/装饰小图，不值得放大到整页宽。
const MAX_PDF_UPSCALE: f32 = 3.0;

/// 漫画页能处理的格式（JPEG/PNG/GIF/WebP）的 (格式, 宽, 高)，只读文件头。
fn comic_header_dims(bytes: &[u8]) -> Option<(ImageFormat, (u32, u32))> {
    let fmt = image::guess_format(bytes).ok()?;
    if !matches!(fmt, ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP) {
        return None;
    }
    let dims = image::ImageReader::with_format(Cursor::new(bytes), fmt).into_dimensions().ok()?;
    Some((fmt, dims))
}

/// 裁边判定容差：一行/列里像素两两 RGB 通道极差都 ≤ 这个值才算"纯色留白"。留够松（8）容 JPEG 压缩
/// 噪声，但不到能吃掉真实画面渐变的地步。
const TRIM_TOLERANCE: u8 = 8;
/// 单边最多裁掉原图这个比例——防止极端图（比如整页近乎纯色）被误判成"全是留白"裁没内容。
/// 真机《镖人》母版库实测坐实过 0.15 太保守（2026-09-19 用户反馈"优化没把大量留白裁切完"）：
/// 每卷开头的版权页（CIP 页，中文漫画常见排版）实际留白单边能到 22%-29%，旧阈值在 15% 就强行
/// 停手，裁不干净。抽样 43 张真实页量出的最大值约 28.6%，0.35 留出约 6 个百分点余量；两边独立
/// 累加最多到 0.7×边长，仍留 30% 给内容，不会把整页裁没。
const TRIM_MAX_FRACTION: f32 = 0.35;

/// 一行/一列像素是否"纯色"（每个像素与首像素的 RGB 通道极差都 ≤ [`TRIM_TOLERANCE`]）。`px(i)` 取该行/列第 i 个像素。
fn line_is_uniform(len: u32, px: impl Fn(u32) -> [u8; 3]) -> bool {
    if len <= 1 {
        return true;
    }
    let first = px(0);
    (1..len).all(|i| {
        let p = px(i);
        (0..3).all(|c| (p[c] as i16 - first[c] as i16).unsigned_abs() as u8 <= TRIM_TOLERANCE)
    })
}

/// 四边纯色留白的检测：返回 `(left, top, 裁后宽, 裁后高)`；没有可裁的留白 / 图太小 / 会裁成空 → `None`。
/// 只在"确实是留白"时裁——边缘整行/整列像素高度一致（[`TRIM_TOLERANCE`]）才算留白，一遇到不满足就停，
/// 不会裁进真实画面。单边最多裁 [`TRIM_MAX_FRACTION`]，兜底极端误判。
/// `px(x, y)` 取像素 RGB——灰度图直接返回 `[v; 3]`，不必先造一份 3 倍大的 RGB 副本（此前灰度页为探测边缘整图 `to_rgb8()`）。
fn trim_bounds_with(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) -> Option<(u32, u32, u32, u32)> {
    if w < 4 || h < 4 {
        return None;
    }
    let max_v = ((h as f32) * TRIM_MAX_FRACTION) as u32;
    let max_h = ((w as f32) * TRIM_MAX_FRACTION) as u32;
    let row = |y: u32| line_is_uniform(w, |x| px(x, y));
    let col = |x: u32| line_is_uniform(h, |y| px(x, y));
    let mut top = 0u32;
    while top < max_v && top + 1 < h && row(top) {
        top += 1;
    }
    let mut bottom = 0u32;
    while bottom < max_v && bottom + 1 < h && row(h - 1 - bottom) {
        bottom += 1;
    }
    let mut left = 0u32;
    while left < max_h && left + 1 < w && col(left) {
        left += 1;
    }
    let mut right = 0u32;
    while right < max_h && right + 1 < w && col(w - 1 - right) {
        right += 1;
    }
    if top == 0 && bottom == 0 && left == 0 && right == 0 {
        return None; // 没有可裁的留白
    }
    let (new_w, new_h) = (w - left - right, h - top - bottom);
    if new_w == 0 || new_h == 0 {
        return None;
    }
    Some((left, top, new_w, new_h))
}

/// [`trim_bounds_with`] 的 `DynamicImage` 入口：`Luma8`/`Rgb8` 直接读像素，其它类型（调用方已归一，实际不会到）退化成 RGB 副本。
fn trim_bounds(img: &image::DynamicImage) -> Option<(u32, u32, u32, u32)> {
    use image::DynamicImage;
    let (w, h) = (img.width(), img.height());
    match img {
        DynamicImage::ImageLuma8(g) => trim_bounds_with(w, h, |x, y| {
            let v = g.get_pixel(x, y)[0];
            [v, v, v]
        }),
        DynamicImage::ImageRgb8(c) => trim_bounds_with(w, h, |x, y| c.get_pixel(x, y).0),
        other => {
            let c = other.to_rgb8();
            trim_bounds_with(w, h, |x, y| c.get_pixel(x, y).0)
        }
    }
}

/// 解码并归一到 `Luma8`/`Rgb8`（灰度保持灰度），并做四边留白裁边。返回 `(图, 输出格式, 是否裁过或换了格式)`。
/// [`prepare_comic_page_for_pdf`] / [`prepare_comic_page_for_epub`] 共用的前半段（此前 PDF 版整段抄了一份）。
/// 用 `into_luma8`/`into_rgb8`：解码结果本来就是 8 位对应类型（几乎所有漫画页）时**不再拷贝整图**，
/// 且不再让"原始解码图 + 归一副本"同时占内存。
///
/// 静态 GIF/WebP 页也处理（2026-09-29 移植自 sheng-ren）：GIF 转 PNG，WebP 有损的转 JPEG、无损的转 PNG
/// （[`comic_output_format`]）；动图原样保留。带透明通道的图先合成到白底（[`flatten_alpha_on_white`]）——直接丢掉
/// alpha 会把透明区域变成它底下存的颜色，通常是纯黑。像素上限仍是设备实测的 [`MAX_DECODE_PIXELS`]。
fn decode_trim_comic(bytes: &[u8]) -> Option<(image::DynamicImage, ImageFormat, bool)> {
    use image::DynamicImage;
    let (fmt, (w, h)) = comic_header_dims(bytes)?;
    if !within_decode_budget(w, h) {
        return None; // 极端高分辨率原图：不整个解出来，原样保留（见 MAX_DECODE_PIXELS 文档）
    }
    let out_fmt = comic_output_format(fmt, bytes)?;
    let decoded = image::load_from_memory_with_format(bytes, fmt).ok()?;
    let gray = matches!(decoded.color(), image::ColorType::L8 | image::ColorType::L16 | image::ColorType::La8 | image::ColorType::La16);
    let decoded = if decoded.color().has_alpha() { flatten_alpha_on_white(decoded) } else { decoded };
    let img = if gray { DynamicImage::ImageLuma8(decoded.into_luma8()) } else { DynamicImage::ImageRgb8(decoded.into_rgb8()) };
    Some(match trim_bounds(&img) {
        Some((l, t, cw, ch)) => (img.crop_imm(l, t, cw, ch), out_fmt, true),
        None => (img, out_fmt, false),
    })
}

/// 漫画页产物的编码格式：JPEG、PNG 保持原格式；GIF 转 PNG（调色板图，无损）；WebP 看编码方式——有损的转 JPEG，无损的转 PNG。
/// 动图（多帧 GIF、动画 WebP）返回 `None`：只取第一帧会丢内容，原样保留。
fn comic_output_format(fmt: ImageFormat, bytes: &[u8]) -> Option<ImageFormat> {
    use image::AnimationDecoder;
    match fmt {
        ImageFormat::Jpeg | ImageFormat::Png => Some(fmt),
        ImageFormat::Gif => {
            let frames = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?.into_frames().take(2).count();
            (frames == 1).then_some(ImageFormat::Png)
        }
        ImageFormat::WebP => {
            if image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).ok()?.has_animation() {
                return None;
            }
            Some(if webp_is_lossless(bytes)? { ImageFormat::Png } else { ImageFormat::Jpeg })
        }
        _ => None,
    }
}

/// WebP 的图像数据是不是无损编码（`VP8L` 块）；有损是 `VP8 ` 块。按 RIFF 块顺序找第一个图像块（扩展格式 `VP8X` 在它前面）。
/// 认不出 → `None`。
fn webp_is_lossless(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let mut i = 12usize;
    while i + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        match &bytes[i..i + 4] {
            b"VP8L" => return Some(true),
            b"VP8 " => return Some(false),
            _ => {}
        }
        i = i.checked_add(8)?.checked_add(size)?.checked_add(size & 1)?;
    }
    None
}

/// 带透明通道的图合成到白底：灰度+alpha → `Luma8`，其余 → `Rgb8`（按 8 位合成，16 位先降到 8 位）。
/// 每个分量 `c·a/255 + 255·(1 − a/255)`，四舍五入。
fn flatten_alpha_on_white(img: image::DynamicImage) -> image::DynamicImage {
    use image::DynamicImage;
    let blend = |c: u8, a: u8| -> u8 { ((c as u32 * a as u32 + 255 * (255 - a as u32) + 127) / 255) as u8 };
    match img.color() {
        image::ColorType::La8 | image::ColorType::La16 => {
            let la = img.into_luma_alpha8();
            let (w, h) = la.dimensions();
            DynamicImage::ImageLuma8(image::GrayImage::from_fn(w, h, |x, y| {
                let p = la.get_pixel(x, y).0;
                image::Luma([blend(p[0], p[1])])
            }))
        }
        _ => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
                let p = rgba.get_pixel(x, y).0;
                image::Rgb([blend(p[0], p[3]), blend(p[1], p[3]), blend(p[2], p[3])])
            }))
        }
    }
}

/// **漫画 PDF 页的单趟处理**（现唯一调用方：入库 PDF 裁边 `pdf_ingest::trim`；最初为已移除的 EPUB 漫画→PDF 写）：解码一次 → 裁边 → 按 PDF 里实际绘制的整数像素尺寸
/// （[`crate::pdfwrite::place_image`]）重采样一次 → 编码一次。**恰好没有可裁的留白、
/// 也不需要缩小时返回 `None`，调用方直接嵌原图字节（零损失）。**
///
/// 取代此前的 `trim_margins` → `downscale_for_epub_comic` 两道串联，它们各自 decode+encode 一遍，
/// 叠加以下三处画质损失（2026-09-20 用户反馈"EPUB 漫画优化成 PDF 会降画质"，拿真机同款乱马/镖人
/// 样本离线量化：1091px 宽网点漫画相对"一次理想重采样"只有 26-31dB）：
///
/// 1. **重采样两遍**：先 Lanczos 缩到 954 框，PDF 里又按 98% 页宽（934.92 非整数）+ 非整数偏移摆放，
///    阅读器等于再缩+亚像素平移一遍；这里直接一次缩到 `place_image` 的整数绘制尺寸，阅读器 1:1 贴。
/// 2. **JPEG 有损代际两代**（裁边一代、缩放一代，各 q95）：合成一趟只剩一代。
/// 3. **灰度图被 `to_rgb8()` 转成 RGB 再编码**：这里保持灰度（单分量 JPEG / 灰度 PNG），不引入
///    多余的色度通道噪声，体积也更小。
///
/// **JPEG 低分辨率源图会由我们预放大**（2026-09-20 真机 A/B 坐实）：镖人卷02 源图仅 566×800，PDF 里
/// 按 934 宽摆放要放大 1.65 倍。让 xochitl 放大 vs 我们先 Lanczos 放大到整数绘制宽、设备 1:1 显示，
/// 用户对照后判定**后者明显更清晰**（xochitl 的 PDF 放大滤镜偏糊）。代价是体积：q95 会 21MB→113MB
/// （2026-09-20 起放大页曾用 q85 压到约 71MB，2026-09-30 用户定放大页也用 q95）。边界：放大倍数超过
/// [`MAX_PDF_UPSCALE`]（缩略图/装饰小图，放大只是白涨体积）不放大；PNG 不放大（无损放大体积暴涨）。
pub fn prepare_comic_page_for_pdf(bytes: &[u8], page_w: u32, page_h: u32) -> Option<Vec<u8>> {
    let (img, fmt, trimmed) = decode_trim_comic(bytes)?;
    let (cw, ch) = (img.width(), img.height());
    let (dw, dh, _, _) = crate::pdfwrite::place_image(cw, ch, page_w, page_h);
    // 高度撑满分支的绘制宽可能是奇数——取偶保证左右边距整数（页宽偶数时）。
    let dw = ((dw.round() as u32) & !1).max(2);
    let dh = (dh.round() as u32).max(1);
    let shrink = dw < cw && dh < ch;
    let upscale = fmt == ImageFormat::Jpeg && dw > cw && dh > ch && (dw as f32 / cw as f32) <= MAX_PDF_UPSCALE;
    if !trimmed && !shrink && !upscale {
        return None; // 既没裁又不缩放：原图字节零损失直接嵌
    }
    let img = if shrink || upscale { resize_lanczos3(&img, dw, dh) } else { img };
    encode_keep_gray(fmt, &img, JPEG_QUALITY_COMIC)
}

/// Lanczos3 重采样，SIMD 实现（`fast_image_resize`，x86 SSE4/AVX2、aarch64 NEON 运行期自动选）。
///
/// 替换 `DynamicImage::resize_exact(.., Lanczos3)` 的原因：2026-09-20 分阶段计时（乱马/镖人，
/// release、每页 ~1000×1500）显示**缩放占整页处理时间的 74–79%**（145–218ms/页），编码 15–22%，
/// 解码/裁边探测可忽略；`fast_image_resize` 同一算法（Lanczos3 卷积）快约 **20 倍**（7–11ms/页）。
/// **不是逐位一致**：与 `image` 库实现的像素差均值 0.1–0.2 灰阶、最大 ~30（仅高对比边缘），二者对
/// 浮点参照（PIL）都是 53–55dB——远低于随后 JPEG q95 编码本身的误差（约 45dB），没有可见差别。
/// 仅处理 `Luma8`/`Rgb8`（调用方已归一到这两种）；其它类型或库报错时退回 `image` 自带实现。
fn resize_lanczos3(img: &image::DynamicImage, dw: u32, dh: u32) -> image::DynamicImage {
    use fast_image_resize::images::{Image, ImageRef};
    use fast_image_resize::{FilterType as FirFilter, PixelType, ResizeAlg, ResizeOptions, Resizer};
    use image::DynamicImage;
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FirFilter::Lanczos3));
    let fast = || -> Option<DynamicImage> {
        let mut resizer = Resizer::new();
        match img {
            DynamicImage::ImageLuma8(g) => {
                let src = ImageRef::new(g.width(), g.height(), g.as_raw(), PixelType::U8).ok()?;
                let mut dst = Image::new(dw, dh, PixelType::U8);
                resizer.resize(&src, &mut dst, &opts).ok()?;
                image::GrayImage::from_raw(dw, dh, dst.into_vec()).map(DynamicImage::ImageLuma8)
            }
            DynamicImage::ImageRgb8(c) => {
                let src = ImageRef::new(c.width(), c.height(), c.as_raw(), PixelType::U8x3).ok()?;
                let mut dst = Image::new(dw, dh, PixelType::U8x3);
                resizer.resize(&src, &mut dst, &opts).ok()?;
                image::RgbImage::from_raw(dw, dh, dst.into_vec()).map(DynamicImage::ImageRgb8)
            }
            _ => None,
        }
    };
    fast().unwrap_or_else(|| img.resize_exact(dw, dh, FilterType::Lanczos3))
}

/// 按 `fmt` 编码，JPEG **保持灰度图为单分量**。`image` 0.25 的 `JpegEncoder::encode_image(&DynamicImage)` 对
/// `ImageLuma8` 也会转成 3 分量 RGB 输出（2026-09-20 实测 SOF 分量数=3、回读 `Rgb8`），必须走
/// `ImageEncoder::write_image(.., ExtendedColorType::L8)` 才是真灰度 JPEG。仅接受 `Luma8`/`Rgb8`
/// （调用方已归一到这两种），JPEG 遇其它类型返回 `None`；PNG 走 `image` 自带无损编码；其余格式 `None`。
fn encode_keep_gray(fmt: ImageFormat, img: &image::DynamicImage, jpeg_quality: u8) -> Option<Vec<u8>> {
    use image::{DynamicImage, ExtendedColorType, ImageEncoder};
    let mut out = Vec::new();
    match fmt {
        ImageFormat::Jpeg => {
            let enc = JpegEncoder::new_with_quality(&mut out, jpeg_quality);
            match img {
                DynamicImage::ImageLuma8(g) => enc.write_image(g.as_raw(), g.width(), g.height(), ExtendedColorType::L8).ok()?,
                DynamicImage::ImageRgb8(c) => enc.write_image(c.as_raw(), c.width(), c.height(), ExtendedColorType::Rgb8).ok()?,
                _ => return None,
            }
            // 哈夫曼表按这张图重做（无损：解码逐像素相同，见 `jpegopt`），同样画质小 7%–16%
            return Some(bookconv::jpegopt::optimize_verified(out));
        }
        ImageFormat::Png => img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).ok()?,
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView, RgbImage};

    fn jpeg_of(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }));
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 90).encode_image(&img).unwrap();
        buf
    }

    fn gray_jpeg_of(w: u32, h: u32, border: u32) -> Vec<u8> {
        // 灰度渐变内容 + 四周 `border` 像素纯白留白。
        let img = image::GrayImage::from_fn(w, h, |x, y| {
            if x < border || y < border || x >= w - border || y >= h - border {
                image::Luma([255])
            } else {
                image::Luma([((x * 7 + y * 3) % 200) as u8])
            }
        });
        let mut buf = Vec::new();
        // 必须 write_image(L8)：encode_image(&DynamicImage) 会把灰度悄悄转成 3 分量 RGB。
        image::ImageEncoder::write_image(
            JpegEncoder::new_with_quality(&mut buf, 95),
            img.as_raw(),
            w,
            h,
            image::ExtendedColorType::L8,
        )
        .unwrap();
        buf
    }
    #[test]
    fn prepare_pdf_page_returns_none_when_no_work_needed() {
        // PNG 不放大：700×1000 无白边、比绘制宽小 → 原字节零损失直接嵌。
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(700, 1000, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])));
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        assert!(prepare_comic_page_for_pdf(&png, 954, 1696).is_none());
        // 放大倍数超上限（缩略图）的 JPEG 同样不放大。
        assert!(prepare_comic_page_for_pdf(&jpeg_of(200, 300), 954, 1696).is_none());
    }

    #[test]
    fn prepare_pdf_page_upscales_low_res_jpeg_to_exact_draw_width() {
        // 镖人同款：566×800 → 按 934 宽摆放；预放大到整数绘制宽，阅读器 1:1。
        let out = prepare_comic_page_for_pdf(&jpeg_of(566, 800), 954, 1696).expect("低分辨率 JPEG 必须预放大");
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!(img.width(), 934);
        let (dw, dh, x, _) = crate::pdfwrite::place_image(img.width(), img.height(), 954, 1696);
        assert_eq!((dw, dh), (img.width() as f32, img.height() as f32));
        assert_eq!(x.fract(), 0.0);
    }

    #[test]
    fn prepare_pdf_page_shrinks_once_to_exact_integer_draw_size() {
        // 1091×1592 灰度页（乱马同款尺寸）：一次缩到 934 宽，与 place_image 的绘制尺寸精确吻合 → 阅读器 1:1。
        let src = gray_jpeg_of(1091, 1592, 0);
        let out = prepare_comic_page_for_pdf(&src, 954, 1696).expect("超过绘制宽必须缩");
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!(img.width(), 934, "缩后宽必须等于 place_image 的整数绘制宽");
        let (dw, dh, x, y) = crate::pdfwrite::place_image(img.width(), img.height(), 954, 1696);
        assert_eq!((dw, dh), (img.width() as f32, img.height() as f32), "阅读器里应 1:1 无二次缩放");
        assert_eq!(x.fract(), 0.0);
        assert_eq!(y.fract(), 0.0);
    }

    #[test]
    fn resize_lanczos3_simd_matches_image_crate_closely() {
        // 合成带高对比边缘+渐变的 RGB 与灰度图，SIMD 结果与 image 库实现的像素差必须很小（均值 <0.5 灰阶）。
        let rgb = DynamicImage::ImageRgb8(RgbImage::from_fn(1091, 1592, |x, y| {
            let edge = if (x / 37 + y / 41) % 2 == 0 { 20 } else { 235 };
            image::Rgb([edge, ((x + y) % 256) as u8, (x % 256) as u8])
        }));
        let gray = DynamicImage::ImageLuma8(image::GrayImage::from_fn(700, 1000, |x, y| image::Luma([((x * 3 + y * 5) % 256) as u8])));
        for (img, (dw, dh)) in [(rgb, (934u32, 1363u32)), (gray, (934, 1334))] {
            let fast = resize_lanczos3(&img, dw, dh);
            let slow = img.resize_exact(dw, dh, FilterType::Lanczos3);
            assert_eq!(fast.color(), slow.color());
            assert_eq!(fast.dimensions(), (dw, dh));
            let (a, b) = (fast.as_bytes(), slow.as_bytes());
            let mean = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as f64).sum::<f64>() / a.len() as f64;
            assert!(mean < 0.5, "SIMD 与 image 库 Lanczos3 差距过大: 均值 {mean}");
        }
    }

    #[test]
    fn prepare_pdf_page_keeps_grayscale_grayscale() {
        let src = gray_jpeg_of(1091, 1592, 0);
        assert_eq!(image::load_from_memory(&src).unwrap().color(), image::ColorType::L8, "夹具本身必须是真灰度");
        let out = prepare_comic_page_for_pdf(&src, 954, 1696).unwrap();
        assert_eq!(image::load_from_memory(&out).unwrap().color(), image::ColorType::L8, "灰度页不该被转成 RGB");
    }

    #[test]
    fn prepare_pdf_page_trims_border_then_upscales_once() {
        // 700×1000 带 40px 白边：先裁成 ~620×920，再一次放大到 934 宽（不是先裁编一代、再放大编一代）。
        let src = gray_jpeg_of(700, 1000, 40);
        let out = prepare_comic_page_for_pdf(&src, 954, 1696).expect("有白边必须裁");
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!(img.width(), 934);
        assert_eq!(img.color(), image::ColorType::L8);
    }

    #[test]
    fn prepare_pdf_page_trim_and_shrink_in_one_pass() {
        let src = gray_jpeg_of(1400, 2000, 60);
        let out = prepare_comic_page_for_pdf(&src, 954, 1696).unwrap();
        let (w, h) = image::load_from_memory(&out).unwrap().dimensions();
        assert_eq!(w, 934, "裁边后仍 >934 宽 → 缩到绘制宽: {w}x{h}");
    }

    /// 造一张带纯白边框的图：中心是彩色渐变，四边留白。
    fn framed_jpeg(w: u32, h: u32, margin: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            if x < margin || y < margin || x >= w - margin || y >= h - margin {
                image::Rgb([255, 255, 255])
            } else {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
            }
        }));
        let mut buf = Vec::new();
        // 高质量无损级编码，避免 JPEG 压缩噪声把"纯色"判花（真实场景裁边容差本身留够松，这里只是
        // 让测试信号干净，不代表生产输入总是这么干净）。
        JpegEncoder::new_with_quality(&mut buf, 100).encode_image(&img).unwrap();
        buf
    }

    /// 走生产的解码+裁边探测（[`decode_trim_comic`]），返回裁后尺寸；没有可裁的留白 → `None`。
    fn trim_dims(bytes: &[u8]) -> Option<(u32, u32)> {
        let (img, _, trimmed) = decode_trim_comic(bytes)?;
        trimmed.then(|| img.dimensions())
    }

    #[test]
    fn trim_crops_uniform_white_border_only() {
        let framed = framed_jpeg(200, 300, 10);
        let (w, h) = trim_dims(&framed).expect("四边留白应触发裁边");
        assert_eq!((w, h), (180, 280), "应精确裁掉 10px 留白: got {w}x{h}");
    }

    #[test]
    fn trim_none_when_no_uniform_border() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(200, 300, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])));
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 95).encode_image(&img).unwrap();
        assert!(trim_dims(&buf).is_none(), "画面一直到边缘、没有留白，不该裁");
    }

    #[test]
    fn trim_capped_by_max_fraction_for_near_solid_image() {
        // 几乎整张纯色(只有中心一小块不同)——裁边不能把整张图裁没，单边应被 TRIM_MAX_FRACTION 卡住。
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(200, 200, |x, y| {
            if (90..110).contains(&x) && (90..110).contains(&y) { image::Rgb([0, 0, 0]) } else { image::Rgb([255, 255, 255]) }
        }));
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 100).encode_image(&img).unwrap();
        let (w, h) = trim_dims(&buf).expect("大片留白应触发裁边");
        let cap = (200.0 * TRIM_MAX_FRACTION) as u32;
        assert!(w >= 200 - 2 * cap && h >= 200 - 2 * cap, "单边最多裁 TRIM_MAX_FRACTION，不能把画面裁没: got {w}x{h}");
    }

    #[test]
    fn trim_handles_margin_beyond_old_cap() {
        // 真机回归（2026-09-19，《镖人》母版库反馈"优化没把大量留白裁切完"）：中文漫画常见的
        // 版权页（CIP 页）实测单边留白能到 22%-29%（抽样见会话记录），旧的 15% 上限在这里会
        // 强行停手、裁不干净。造一张留白比例超过旧上限、但仍在新上限内的图，确认新阈值下能
        // 裁到位（不是卡在旧的 15% 就停）。
        let (w, h, margin_frac) = (400u32, 600u32, 0.25f32);
        let margin = (w as f32 * margin_frac) as u32;
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            if x < margin || y < margin || x >= w - margin || y >= h - margin {
                image::Rgb([255, 255, 255])
            } else {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
            }
        }));
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 100).encode_image(&img).unwrap();
        let (got_w, got_h) = trim_dims(&buf).expect("留白应触发裁边");
        let old_cap = (w as f32 * 0.15) as u32;
        assert!(got_w < w - 2 * old_cap, "25% 留白不该被旧的 15% 上限卡住: got {got_w}");
        assert_eq!((got_w, got_h), (w - 2 * margin, h - 2 * margin), "留白在新上限内应该精确裁掉: got {got_w}x{got_h}");
    }

    #[test]
    fn within_decode_budget_boundary() {
        assert!(within_decode_budget(3000, 3000), "900 万像素，等于上限，应允许");
        assert!(!within_decode_budget(3001, 3000), "超一点点也该拒绝");
    }

    #[test]
    fn oversized_image_skipped() {
        let huge = jpeg_of(5001, 5000);
        assert!(decode_trim_comic(&huge).is_none(), "超限图应跳过裁边");
        assert!(prepare_comic_page_for_pdf(&huge, 954, 1696).is_none());
    }
}
