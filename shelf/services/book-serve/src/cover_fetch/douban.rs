//! 豆瓣：按书名找条目（中文版，最贴近用户手上的书），取封面（移植自 sheng-ren `library::douban`，只要封面这一半）。
//!
//! 豆瓣没有公开 API：找条目用网页的搜索建议接口。请求要少（全程节流）；图片服务器要带来源页（不带回 HTTP 418）。
//! 页面改版了取不到就当没找到，交给下一个源。设备直连豆瓣 2026-09-29 实测可用。

use super::matching::{norm_author, norm_s, similarity};
use super::net::{enc, Net};

const REFERER: &str = "https://book.douban.com/";

/// 搜索建议里对得上的条目。
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) year: String,
    /// 封面大图（`/l/`）网址。
    pub(crate) pic: String,
}

impl Hit {
    pub(crate) fn label(&self) -> String {
        format!("豆瓣 {} {} {}", self.title, self.author, self.year).trim().to_string()
    }
}

/// 按书名找：书名（简体化后）相同、或只差卷次后缀（≤2 字），且作者对得上的条目（没有作者信息时书名要完全相同）。
/// 依次试各个书名，第一个有结果的为准，保持豆瓣给的顺序。
pub(crate) fn search(net: &Net, titles: &[String], authors: &[String]) -> Vec<Hit> {
    let want_authors: Vec<String> = authors.iter().map(|a| norm_author(a)).filter(|a| !a.is_empty()).collect();
    for t in titles {
        let nt = norm_s(t);
        if nt.is_empty() {
            continue;
        }
        let Ok(v) = net.json(&format!("https://book.douban.com/j/subject_suggest?q={}", enc(t))) else { continue };
        let mut hits = Vec::new();
        for x in v.as_array().into_iter().flatten() {
            let g = |k: &str| x[k].as_str().unwrap_or("").to_string();
            let (title, author, pic, id) = (g("title"), g("author_name"), g("pic"), g("id"));
            if id.is_empty() || x["type"].as_str().is_some_and(|ty| ty != "b") {
                continue;
            }
            let dt = norm_s(&title);
            let title_ok = dt == nt || ((nt.starts_with(&dt) || dt.starts_with(&nt)) && nt.chars().count().abs_diff(dt.chars().count()) <= 2);
            let author_ok = if want_authors.is_empty() {
                dt == nt
            } else {
                author.split(['/', '、', ',', '，']).any(|a| want_authors.iter().any(|w| similarity(w, &norm_author(a)) >= 0.6))
            };
            if title_ok && author_ok {
                let pic = pic.replace("/view/subject/s/", "/view/subject/l/").replace("/view/subject/m/", "/view/subject/l/");
                hits.push(Hit { title, author, year: g("year"), pic });
            }
        }
        if !hits.is_empty() {
            return hits;
        }
    }
    Vec::new()
}

/// 前几个条目里挑分辨率最高、像封面的封面图（老条目只有 200 多像素宽的小图）。返回（条目下标, 图片, 扩展名）。
pub(crate) fn best_cover(net: &Net, hits: &[Hit]) -> Option<(usize, Vec<u8>, &'static str)> {
    let mut best: Option<(u64, usize, Vec<u8>, &'static str)> = None;
    for (i, h) in hits.iter().enumerate().take(4).filter(|(_, h)| !h.pic.is_empty()) {
        let Ok(bytes) = net.fetch_ref(&h.pic, Some(REFERER)) else { continue };
        let Some(ext) = super::plausible_cover(&bytes) else { continue };
        let area = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok()).map_or(0, |(w, h)| w as u64 * h as u64);
        if best.as_ref().is_none_or(|b| area > b.0) {
            best = Some((area, i, bytes, ext));
        }
        if area >= 600 * 900 {
            break;
        }
    }
    best.map(|(_, i, b, e)| (i, b, e))
}
