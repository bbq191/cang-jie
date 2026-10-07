//! PDF 页图片处理（入库 PDF 裁边 `pdf_ingest::trim`、`pdfwrite` 的页面尺寸）。
//!
//! 解码、裁边、缩放、编码都用 sheng-ren 公开的 `bookconv::imgopt`（`decode_page` → `trim_page` → `Page8::resize_lanczos3` →
//! `Page8::encode`，JPEG 编码后带无损的哈夫曼表重做）；这里只留 PDF 专用的部分：按 PDF 页里的绘制尺寸
//! （[`crate::pdfwrite::place_image`]）一次缩放到位、低分辨率 JPEG 预放大。
//! 纪律同原模块：只缩不放（JPEG 低分辨率页例外，见 [`prepare_comic_page_for_pdf`]）、保宽高比、保原格式、任何失败原样保留。
//!
//! 和 sheng-ren 漫画页（`prepare_comic_page_for_epub`）不同的两个选项：
//! - **裁边用 `TrimMode::AnyUniform`**：任何颜色的纯色边都裁（沿用此前 cang-jie 的行为）。扫描件 PDF 常带扫描仪的黑框/灰框，
//!   裁掉才有用；sheng-ren 只裁白边，是因为它裁完会补白边、黑底出血页会被改样——这里裁完不补边，页面留白是 PDF 页本身的白底，
//!   代价同样是黑底出血页的黑边会被裁掉（画面本身不动）。
//! - **不按 EXIF 摆正**（`apply_exif: false`）：PDF 阅读器画内嵌 JPEG 时不看 EXIF，原 PDF 里显示的就是原始像素方向。

use bookconv::imgopt::{decode_page, trim_page, Page8, PageDecode, TrimMode, JPEG_QUALITY_COMIC, MAX_DECODE_PIXELS};
use image::ImageFormat;

/// Move 屏最长像素边 / 最短像素边。
pub const MAX_EDGE: u32 = 1696;
pub const MAX_SHORT_EDGE: u32 = 954;
/// 预放大的倍数上限：超过视为缩略图/装饰小图，不值得放大到整页宽。
const MAX_PDF_UPSCALE: f32 = 3.0;

/// 解码 + 裁边。返回 `(图, 输出格式, 是否裁过)`。单张解码上限用 sheng-ren 的 `MAX_DECODE_PIXELS`（900 万像素，
/// 设备内存实测定的阈值，2026-09-19 真机 VmHWM 事故），超限的图原样保留。静态 GIF 转 PNG、WebP 有损转 JPEG/无损转 PNG，
/// 动图原样保留；带透明通道的先合成到白底。
fn decode_trim(bytes: &[u8]) -> Option<(Page8, ImageFormat, bool)> {
    let (img, fmt) = decode_page(bytes, PageDecode { grayscale: false, max_px: MAX_DECODE_PIXELS, apply_exif: false })?;
    let orig = img.dimensions();
    let (img, _, _) = trim_page(img, TrimMode::AnyUniform);
    let trimmed = img.dimensions() != orig;
    Some((img, fmt, trimmed))
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
    let (img, fmt, trimmed) = decode_trim(bytes)?;
    let (cw, ch) = img.dimensions();
    let (dw, dh, _, _) = crate::pdfwrite::place_image(cw, ch, page_w, page_h);
    // 高度撑满分支的绘制宽可能是奇数——取偶保证左右边距整数（页宽偶数时）。
    let dw = ((dw.round() as u32) & !1).max(2);
    let dh = (dh.round() as u32).max(1);
    let shrink = dw < cw && dh < ch;
    let upscale = fmt == ImageFormat::Jpeg && dw > cw && dh > ch && (dw as f32 / cw as f32) <= MAX_PDF_UPSCALE;
    if !trimmed && !shrink && !upscale {
        return None; // 既没裁又不缩放：原图字节零损失直接嵌
    }
    let img = if shrink || upscale { img.resize_lanczos3(dw, dh) } else { img };
    // 漫画质量 95（2026-09-30 用户定，缩小、放大一律 95）；灰度保持单分量
    img.encode(fmt, JPEG_QUALITY_COMIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage};
    use std::io::Cursor;

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

    /// 走生产的解码+裁边（[`decode_trim`]），返回裁后尺寸；没有可裁的留白 → `None`。
    fn trim_dims(bytes: &[u8]) -> Option<(u32, u32)> {
        let (img, _, trimmed) = decode_trim(bytes)?;
        trimmed.then(|| img.dimensions())
    }

    #[test]
    fn trim_crops_uniform_white_border_only() {
        let framed = framed_jpeg(200, 300, 10);
        let (w, h) = trim_dims(&framed).expect("四边留白应触发裁边");
        assert_eq!((w, h), (180, 280), "应精确裁掉 10px 留白: got {w}x{h}");
    }

    #[test]
    fn trim_also_crops_dark_and_colored_borders() {
        // PDF 页用 `TrimMode::AnyUniform`：扫描件的黑框、彩色纯色边也裁（sheng-ren 的漫画页只裁白边）。
        for c in [[0u8, 0, 0], [200, 30, 30]] {
            let img = DynamicImage::ImageRgb8(RgbImage::from_fn(200, 300, |x, y| {
                if !(10..190).contains(&x) || !(10..290).contains(&y) { image::Rgb(c) } else { image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]) }
            }));
            let mut buf = Vec::new();
            JpegEncoder::new_with_quality(&mut buf, 100).encode_image(&img).unwrap();
            assert_eq!(trim_dims(&buf), Some((180, 280)), "{c:?} 边");
        }
    }

    #[test]
    fn trim_none_when_no_uniform_border() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(200, 300, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])));
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 95).encode_image(&img).unwrap();
        assert!(trim_dims(&buf).is_none(), "画面一直到边缘、没有留白，不该裁");
    }




    #[test]
    fn oversized_image_skipped() {
        let huge = jpeg_of(5001, 5000);
        assert!(decode_trim(&huge).is_none(), "超限图应跳过裁边");
        assert!(prepare_comic_page_for_pdf(&huge, 954, 1696).is_none());
    }
}
