//! 裁图：把一片手写从 xochitl 现成的页缩略图里裁出来（喂视觉模型转写）。
//! 页坐标 → 缩略图像素是等比映射：缩略图按页宽高等比缩小（真机 384×512 对 3:4 页），x 原点可在页中线。
//! ⚠️ `page_w`/`page_h` 必须是 EPUB 排版引擎的虚拟画布尺寸（真机实测 960×1280），**不是物理屏 1404×1872**——
//! 2026-09-07 用错 1404×1872 时裁图整体裁偏（真机四条草稿全抄了裁图之外的印刷体/空白，见白皮书 §03g），
//! `cluster`/`pair`（notecore::geom）不受影响：那两步只在 `.rm` 原始坐标系内比相对距离，不需要知道画布真实尺寸。
//! 缩略图只有 384 px 宽，精度不够时再换高分辨率自渲染（策略可替换：`Cropper` 只暴露"给我这片的 PNG"）。
use image::{DynamicImage, GenericImageView, ImageFormat};
use std::io::Cursor;

/// 页坐标系描述（来自配置，真机样本标定）。
#[derive(Debug, Clone, Copy)]
pub struct PageGeom {
    pub page_w: f32,
    pub page_h: f32,
    pub x_origin_center: bool,
    pub margin: f32,
}

impl PageGeom {
    /// 页坐标包围盒 → 缩略图像素矩形 (x, y, w, h)，带留白、夹在图内。
    pub fn to_pixels(&self, bbox: (f32, f32, f32, f32), img_w: u32, img_h: u32) -> (u32, u32, u32, u32) {
        let sx = img_w as f32 / self.page_w;
        let sy = img_h as f32 / self.page_h;
        let off = if self.x_origin_center { self.page_w / 2.0 } else { 0.0 };
        let x0 = ((bbox.0 - self.margin + off) * sx).floor().clamp(0.0, img_w as f32 - 1.0);
        let y0 = ((bbox.1 - self.margin) * sy).floor().clamp(0.0, img_h as f32 - 1.0);
        let x1 = ((bbox.2 + self.margin + off) * sx).ceil().clamp(x0 + 1.0, img_w as f32);
        let y1 = ((bbox.3 + self.margin) * sy).ceil().clamp(y0 + 1.0, img_h as f32);
        (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
    }
}

/// 裁剪区域小于这个像素数就当"裁不到"：真机坐实（2026-09-07，§03g）有的手写落在一页里滚动
/// 到视口以外的地方，缩略图根本没画到，`to_pixels` 会被强制夹到图边缘、裁出一条几乎没内容的窄条——
/// 视觉模型 API 直接拒收（"height:1 ... must be larger than 10"）。宁可让上游拿到清楚的错误提前跳过，
/// 也不要真送一张废图去烧一次 API 调用。
const MIN_CROP_PX: u32 = 8;

/// 从缩略图 PNG 字节裁出一片，返回 PNG 字节。
pub fn crop_png(thumb_png: &[u8], geom: &PageGeom, bbox: (f32, f32, f32, f32)) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory_with_format(thumb_png, ImageFormat::Png).map_err(|e| format!("缩略图解码失败: {e}"))?;
    let (w, h) = img.dimensions();
    let (x, y, cw, ch) = geom.to_pixels(bbox, w, h);
    if cw < MIN_CROP_PX || ch < MIN_CROP_PX {
        return Err(format!("裁剪区域只有 {cw}x{ch} px，可能落在缩略图视口以外（页面滚动出去了）"));
    }
    let sub: DynamicImage = img.crop_imm(x, y, cw, ch);
    let mut out = Vec::new();
    sub.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| format!("裁图编码失败: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_page_bbox_to_thumbnail_pixels() {
        let g = PageGeom { page_w: 1404.0, page_h: 1872.0, x_origin_center: true, margin: 0.0 };
        // 页中线右侧 351 单位、纵向 468..936 → 缩略图 384×512：x=(351+702)/1404*384=288, y=468/1872*512=128, h=128
        assert_eq!(g.to_pixels((351.0, 468.0, 702.0, 936.0), 384, 512), (288, 128, 96, 128));
        let g2 = PageGeom { x_origin_center: false, margin: 10.0, ..g };
        assert_eq!(g2.to_pixels((-100.0, -100.0, 5000.0, 5000.0), 384, 512), (0, 0, 384, 512), "越界夹回图内");
    }

    #[test]
    fn real_page_geometry_lands_on_the_real_highlight() {
        // 2026-09-07 真机回归：拿高亮 1（"她只想睡覺。"）在 .rm 里的真实 rect，
        // 检验 960×1280 画布配置算出来的缩略图像素框，真落在真机缩略图里那条橙色高亮所在的行/列
        // （行 264–281、列 51–138，用 numpy 扫像素颜色反测得——过程见白皮书 §03g）。
        // 1404×1872 算出来的框是行 179–192、列 96–156，完全落空，这条测试就是防它再回来。
        let g = PageGeom { page_w: 960.0, page_h: 1280.0, x_origin_center: true, margin: 0.0 };
        let rect = (-351.649_85, 656.453, -351.649_85 + 220.058, 656.453 + 45.6); // (x0,y0,x1,y1)
        let (x0, y0, cw, ch) = g.to_pixels(rect, 384, 512);
        let (x1, y1) = (x0 + cw, y0 + ch);
        assert!((45..=60).contains(&x0), "左边界应落在真机测得的 51 附近，got {x0}");
        assert!((130..=145).contains(&x1), "右边界应落在真机测得的 138 附近，got {x1}");
        assert!((260..=270).contains(&y0), "上边界应落在真机测得的 264 附近，got {y0}");
        assert!((278..=290).contains(&y1), "下边界应落在真机测得的 281 附近，got {y1}");
    }

    #[test]
    fn crop_png_rejects_offscreen_bbox() {
        // 真机第四条批注（长句+下划线）y=1507..1620，超出 960×1280 画布——夹到图边缘后宽/高只剩 1px，
        // 真拿去喂视觉模型会被 API 拒收（"height:1 ... must be larger than 10"，2026-09-07 真机 400）。
        let mut img = image::RgbaImage::new(384, 512);
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let g = PageGeom { page_w: 960.0, page_h: 1280.0, x_origin_center: true, margin: 24.0 };
        let err = crop_png(&png, &g, (-89.27, 1507.02, 398.32, 1620.08)).unwrap_err();
        assert!(err.contains("视口以外"), "应给出清楚的错误而不是裁一张废图: {err}");
    }

    #[test]
    fn crops_real_png() {
        let mut img = image::RgbaImage::new(384, 512);
        for y in 128..256 {
            for x in 288..384 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let g = PageGeom { page_w: 1404.0, page_h: 1872.0, x_origin_center: true, margin: 0.0 };
        let out = crop_png(&png, &g, (351.0, 468.0, 702.0, 936.0)).unwrap();
        let c = image::load_from_memory_with_format(&out, ImageFormat::Png).unwrap();
        assert_eq!(c.dimensions(), (96, 128));
        assert_eq!(c.get_pixel(0, 0), image::Rgba([0, 0, 0, 255]), "裁到的是黑块");
    }
}
