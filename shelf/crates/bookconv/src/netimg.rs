//! 远程图抓取（优化器内联远程 `<img>` 与稍后读共用）。原在 weread-device readlater.rs，
//! 2026-09-03 随内容层抽入 bookconv：优化器不再反向依赖稍后读。
use crate::convert::common;
use crate::imgopt;

/// 抓图 UA（稍后读与优化器共用同一标识）。
pub const UA: &str = "Mozilla/5.0 (compatible; cangjie-readlater/1.0)";

/// 最大抓图体积；超过的整张不要（此前 `take(20MB)` 静默截断，魔数照样认得出，残图被写进书里）。
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// 抓远程图并按 EPUB 插图框降采样（[`imgopt::downscale_for_epub`]，与书里本地插图同一规则）。`src` 支持协议相对
/// `//host/path`；非 http(s) 返回 None。返回 (字节, 扩展名, mime)；非图（魔数不认）或超过 [`MAX_IMAGE_BYTES`] → None。
///
/// 两个调用方（优化器内联远程图、抓网文）产出的都是 EPUB：此前用整页漫画的屏幕框（横图 1696 宽），优化器内联的远程图
/// 不再经过 `transform_image_bytes`，横幅远程图宽度能到 1696（违反"插图宽绝不超 842"）；抓网文的图则先缩到 1696、
/// 优化时再缩到 842，重采样两次、JPEG 损失两代。
pub fn fetch_image(ag: &ureq::Agent, src: &str, referer: &str) -> Option<(Vec<u8>, &'static str, &'static str)> {
    // 协议相对 URL（`//host/path`，Wikipedia 等常用）补 https:；其余非 http(s)（data:/未解析相对）跳过。
    let abs = if let Some(rest) = src.strip_prefix("//") {
        format!("https://{rest}")
    } else if src.starts_with("http://") || src.starts_with("https://") {
        src.to_string()
    } else {
        return None;
    };
    let mut req = ag.get(&abs).set("User-Agent", UA);
    if !referer.is_empty() {
        req = req.set("Referer", referer);
    }
    let resp = req.call().ok()?;
    let mut bytes: Vec<u8> = Vec::new();
    use std::io::Read;
    resp.into_reader().take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    let (ext, mime) = common::image_ext_mime(&bytes)?; // 魔数识别 JPEG/PNG/GIF；非图→None
    if let Some(smaller) = imgopt::downscale_for_epub(&bytes) {
        return Some((smaller, ext, mime));
    }
    Some((bytes, ext, mime))
}

/// 统一超时的 HTTP agent（`timeout_secs`=0 表示不限）。bookconv 不引用旧 crate，
/// 此构造与 device-core::http_agent 语义一致、独立实现。
pub fn http_agent(timeout_secs: u64) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new();
    if timeout_secs > 0 {
        b = b.timeout(std::time::Duration::from_secs(timeout_secs));
    }
    b.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// 本机起一个只回一次响应的 HTTP 服务，返回它的 URL。
    fn serve_once(body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut req = [0u8; 4096];
                let _ = s.read(&mut req);
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}/a.jpg")
    }

    fn jpeg_of(w: u32, h: u32) -> Vec<u8> {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])));
        let mut buf = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90).encode_image(&img).unwrap();
        buf
    }

    /// 横幅远程图按 EPUB 插图框缩（宽 ≤ 842），不再按整页屏幕框留到 1696 宽。
    #[test]
    fn remote_banner_fits_epub_frame() {
        let url = serve_once(jpeg_of(2000, 500));
        let (bytes, ext, _) = fetch_image(&http_agent(10), &url, "").expect("应抓到");
        assert_eq!(ext, "jpg");
        let img = image::load_from_memory(&bytes).unwrap();
        assert!(img.width() <= imgopt::EPUB_READABLE_W, "宽 {} 超出插图框", img.width());
    }

    /// 超过上限的图整张不要（此前截断到 20MB、魔数照样认得出，残图进书）。
    #[test]
    fn oversized_remote_image_is_rejected_not_truncated() {
        let mut body = jpeg_of(8, 8);
        body.resize(MAX_IMAGE_BYTES as usize + 1, 0);
        let url = serve_once(body);
        assert!(fetch_image(&http_agent(10), &url, "").is_none());
    }
}
