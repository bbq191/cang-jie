//! 裁图：把一片手写从 xochitl 现成的页缩略图里裁出来（喂视觉模型转写）。
//! 页坐标 → 缩略图像素是等比映射：缩略图按页宽高等比缩小（真机 384×512 对 3:4 页），x 原点可在页中线。
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

/// 从缩略图 PNG 字节裁出一片，返回 PNG 字节。
pub fn crop_png(thumb_png: &[u8], geom: &PageGeom, bbox: (f32, f32, f32, f32)) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory_with_format(thumb_png, ImageFormat::Png).map_err(|e| format!("缩略图解码失败: {e}"))?;
    let (w, h) = img.dimensions();
    let (x, y, cw, ch) = geom.to_pixels(bbox, w, h);
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
