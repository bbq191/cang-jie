//! 书里没有封面时联网找一张（2026-09-29 以 sheng-ren `library` 的封面查找为参照移植）。母版库「优化」EPUB 前调用
//! （`staging::optimizing`），找到的图只在 OPF 里声明成封面（`bookconv::opfmeta::edit_epub`：不加封面页、正文不变）。
//!
//! 来源依次是（sheng-ren 参照 Koodo Reader 用的书目源，只借鉴"用哪些源"）：
//! 1. **豆瓣**：中文版封面，最贴近手上的书（见 `douban`）。挑前几个对得上的条目里分辨率最高的。设备直连可用（2026-09-29 实测）。
//! 2. 豆瓣没有时找**原作**的封面：Wikidata 找到作品（见 `wikidata`），Open Library 作品本身的封面 → 按英文名、原文名搜 →
//!    Wikimedia Commons 上的作品图片。⚠ 设备在国内网络直连不了这几个网站，通常很快以"连不上"跳过。
//! 3. 都找不到：**生成**一张——书名在上、作者头像（Wikidata 人物照片，设备上多半拿不到）或作者名首字居中、作者名在下（见 `covergen`）。
//!
//! **没查成不生成**：豆瓣那一步遇到网络错误（离线、强制门户返回的不是 JSON、重试完还超时）就整个放弃，这次不补封面，下次优化再试——
//! 生成的封面会写进母版，写进去以后就不会再找真封面了。只有豆瓣明确答复"没有对得上的书"，而原作那一步也没找到时才生成。

mod covergen;
mod douban;
mod matching;
mod net;
mod wikidata;

use net::{enc, Net};
use serde_json::Value;

/// 一次找封面的结果。
#[derive(Debug)]
pub enum Outcome {
    /// 找到了：(图片字节, 说明——哪个条目/作品)。
    Found(Vec<u8>, String),
    /// 没找到，生成了一张 JPEG：(字节, 说明)。
    Generated(Vec<u8>, String),
    /// 没查成或生成不了（网络错误、没有中文字体……），原因。
    Skipped(String),
}

/// 按书名（书里的 `dc:title` 与母版库文件名两路）和作者找封面。
pub fn find_cover(title: &str, file_stem: &str, authors: &[String]) -> Outcome {
    let net = Net::new();
    let titles = matching::title_candidates(title, file_stem);
    if titles.is_empty() {
        return Outcome::Skipped("没有书名，无从找起".into());
    }
    // ① 豆瓣
    let hits = douban::search(&net, &titles, authors);
    if let Some((i, bytes, _)) = douban::best_cover(&net, &hits) {
        return Outcome::Found(bytes, hits[i].label());
    }
    if let Some(e) = net.transient_error() {
        return Outcome::Skipped(format!("豆瓣没查成（{e}），下次优化再试"));
    }
    // ② 原作（Wikidata → Open Library / Commons）。连不上只是少一个来源，不算没查成：设备在国内网络本来就连不上它们。
    let mut work = None;
    for t in &titles {
        if let Ok(Some(w)) = wikidata::find_work(&net, t, authors) {
            work = Some(w);
            break;
        }
    }
    if let Some(w) = &work {
        let name = [&w.en, &w.original, &w.ja].into_iter().find(|n| !n.is_empty()).cloned().unwrap_or_default();
        for url in cover_urls(&net, w) {
            let Ok(bytes) = net.fetch(&url) else { continue };
            if plausible_cover(&bytes).is_some() {
                return Outcome::Found(bytes, format!("原作 {} {name}", w.qid));
            }
        }
    }
    // ③ 生成
    let author = authors.first().map(|a| a.trim().to_string()).unwrap_or_default();
    let portrait = wikidata::author_portrait(&net, authors).ok().flatten();
    let photo = portrait.as_ref().and_then(|(_, url)| net.fetch(url).ok()).filter(|b| image::load_from_memory(b).is_ok());
    let font = match covergen::load_font() {
        Ok(f) => f,
        Err(e) => return Outcome::Skipped(e),
    };
    let shown = titles.first().map_or(title, String::as_str);
    match covergen::render(&font, shown, &author, photo.as_deref(), seed_of(shown)) {
        Ok(bytes) => Outcome::Generated(
            bytes,
            match (&portrait, &photo) {
                (Some((who, _)), Some(_)) => format!("生成（书名 + 作者头像 {who}）"),
                _ => "生成（书名 + 作者名首字）".to_string(),
            },
        ),
        Err(e) => Outcome::Skipped(format!("生成封面失败: {e}")),
    }
}

/// 生成封面的样式种子：书名的 FNV-1a 哈希（同一本书每次一样）。
fn seed_of(title: &str) -> u64 {
    title.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// 下载下来的是不是像样的封面：能解码、够大、竖版比例。
pub(crate) fn plausible_cover(bytes: &[u8]) -> Option<&'static str> {
    let (ext, _) = bookconv::convert::common::image_ext_mime(bytes)?;
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()?;
    (h >= 300 && w >= 180 && (0.45..=1.05).contains(&(w as f32 / h as f32))).then_some(ext)
}

/// 原作的候选封面图网址：Open Library 作品本身的封面 → 按英文名（加作者）、原文名、日文名搜 → Wikimedia Commons 上的作品图片。
fn cover_urls(net: &Net, w: &wikidata::Work) -> Vec<String> {
    let mut ids: Vec<i64> = Vec::new();
    let mut add = |id: i64| {
        if !ids.contains(&id) {
            ids.push(id);
        }
    };
    if !w.ol.is_empty() {
        if let Ok(v) = net.json(&format!("https://openlibrary.org/works/{}.json", enc(&w.ol))) {
            v["covers"].as_array().into_iter().flatten().filter_map(Value::as_i64).filter(|&c| c > 0).take(3).for_each(&mut add);
        }
    }
    let mut queries: Vec<String> = Vec::new();
    if !w.en.is_empty() {
        if !w.author_en.is_empty() {
            queries.push(format!("title={}&author={}", enc(&w.en), enc(&w.author_en)));
        }
        queries.push(format!("title={}", enc(&w.en)));
    }
    for t in [&w.original, &w.ja] {
        if !t.is_empty() && t != &w.en {
            queries.push(format!("title={}", enc(t)));
        }
    }
    for q in queries {
        if let Ok(v) = net.json(&format!("https://openlibrary.org/search.json?limit=5&fields=cover_i&{q}")) {
            v["docs"].as_array().into_iter().flatten().filter_map(|d| d["cover_i"].as_i64()).take(3).for_each(&mut add);
        }
    }
    let mut urls: Vec<String> = ids.into_iter().take(8).map(|id| format!("https://covers.openlibrary.org/b/id/{id}-L.jpg?default=false")).collect();
    if !w.image.is_empty() {
        urls.push(format!("{}?width=1000", w.image.replace("http://", "https://")));
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plausible_cover_wants_a_portrait_image_of_reasonable_size() {
        let jpg = |w: u32, h: u32| {
            let mut b = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut b, 80).encode_image(&image::DynamicImage::ImageRgb8(image::RgbImage::new(w, h))).unwrap();
            b
        };
        assert_eq!(plausible_cover(&jpg(400, 600)), Some("jpg"));
        assert_eq!(plausible_cover(&jpg(100, 150)), None, "太小");
        assert_eq!(plausible_cover(&jpg(800, 400)), None, "横图不像封面");
        assert_eq!(plausible_cover(b"<html>login</html>"), None);
    }

    #[test]
    fn seed_is_stable_per_title() {
        assert_eq!(seed_of("白夜行"), seed_of("白夜行"));
        assert_ne!(seed_of("白夜行"), seed_of("幻夜"));
    }

    /// 联网冒烟：真去豆瓣找《白夜行》的封面（要联网，默认不跑：`cargo test -p book-serve -- --ignored cover_fetch`）。
    #[test]
    #[ignore]
    fn live_douban_finds_byakuyako() {
        match find_cover("白夜行", "东野圭吾《白夜行》", &["東野圭吾".to_string()]) {
            Outcome::Found(bytes, how) => assert!(plausible_cover(&bytes).is_some() && how.contains("豆瓣"), "{how}"),
            other => panic!("{other:?}"),
        }
    }

    /// 生成封面（本机有中文字体才跑得出来；没有就跳过）。
    #[test]
    fn generated_cover_is_a_plausible_portrait_jpeg() {
        let Ok(font) = covergen::load_font() else { return };
        let b = covergen::render(&font, "不存在的书", "某作者", None, seed_of("不存在的书")).unwrap();
        assert_eq!(plausible_cover(&b), Some("jpg"));
    }
}
