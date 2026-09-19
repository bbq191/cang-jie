//! EPUB 漫画 → PDF：真机反复实测坐实 xochitl 的 EPUB 渲染走"文字排版盒模型"，内容区相对物理
//! 页面有一个消不掉的固定内边距（`.content` 元数据的 `margins` 字段，UI 只给 28/56/112 三档
//! 离散预设；直接改文件也没用——xochitl 渲染文档时会用自己的逻辑把这个字段覆盖回去），文字页和
//! 图片页共享同一份限制，CSS 层面（`width` 超 100%、负 `margin`）也测过绕不开——这是 EPUB 渲染
//! 路径本身的硬限制，不是我们代码的 bug。同一批真机实测：PDF 直传（页面物理尺寸精确等于设备
//! 屏幕 954×1696px）左右留白量得 **0.00%**，且 `.content` 里 PDF 文档根本没有 `margins` 这个
//! 字段——PDF 走的是完全独立于 EPUB 文字排版盒模型的直接光栅化路径，从根上不受这个限制。
//!
//! 这个模块是"漫画类 EPUB 优化时改产出 PDF（带书签）"的实现，跟 `comic_split.rs`（EPUB→EPUB
//! 按卷拆分）平行独立、互不影响；复用它的 NCX 标题解析（`ncx_titles_in_range`）和 `imgs_
//! referenced`，图片处理复用 `imgopt::trim_margins`/`downscale_for_epub_comic`（跟 EPUB 漫画线
//! 完全相同的两步，不再额外调用 `pad_to_device_aspect`——PDF 不需要靠补白像素控制留白分布，
//! 直接在页面里摆位置即可，摆位算法见 `convert::pdfwrite::place_image`）。也不碰 `convert::
//! pdfwrite::images_to_pdf`/`convert::cbz`（CBZ→PDF 现状路径），只用新增的 `images_to_pdf_
//! with_toc`/`extract_pages`/`page_count`。

use crate::convert::pdfwrite::{self, PdfImage};
use crate::wash::{dir_of, is_html, parse_opf, Entry};
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfReport {
    pub pages: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

/// 读入方式仿 `comic_split::deliver_split_streaming` 阶段一：图片条目留空占位、只读 html/opf/ncx
/// 真实字节判断是不是漫画+抽标题；真正的图片字节按需读、处理完立刻编进 `PdfImage` 就丢原始字节，
/// 峰值内存是"处理到哪张图"而不是"全书图片"，跟现有 EPUB 拆分路径同一套纪律。
///
/// **一图一页**（不像 EPUB 那样允许一个 spine 页塞多张图）——漫画天然就是一页一图，PDF 场景更
/// 贴近这个语义；一个 spine 页如果引用了 N 张图，会展开成 N 个 PDF 页，NCX 标题落在这个 spine
/// 页对应的第一张图上。零图的纯文字页（如后记）在 PDF 场景没有对应物，直接跳过不产出页面——
/// PDF 不是文字排版容器，硬塞文字进去不是这次任务范围。
pub fn optimize_comic_epub_to_pdf_streaming(
    input_path: &Path,
    output_path: &Path,
    mut on_progress: impl FnMut(usize, usize),
) -> Result<PdfReport, String> {
    let bytes_before = std::fs::metadata(input_path).map(|m| m.len() as usize).unwrap_or(0);
    let file = std::fs::File::open(input_path).map_err(|e| format!("打开母版库文件失败: {e}"))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("解 EPUB(非 zip?): {e}"))?;

    let mut entries: Vec<Entry> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| format!("读 EPUB 条目 {i}: {e}"))?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        let data = if crate::imgopt::is_downscalable(&name) {
            Vec::new()
        } else {
            let mut d = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut d).map_err(|e| e.to_string())?;
            d
        };
        entries.push(Entry { name, data });
    }
    drop(zip);

    if !crate::comic_detect::is_comic(&entries) {
        return Err("不是漫画书，漫画→PDF 这条路径不适用".into());
    }
    let opf = parse_opf(&entries).ok_or("解不出 OPF/spine")?;
    let titles_by_spine_idx =
        crate::comic_split::ncx_titles_in_range(&entries, &opf, 0, opf.spine.len());

    // 先摸一遍每个 spine 项引用了几张图，凑出总图数给进度条用（不解码，只读 html 文本里的 <img> 引用）。
    let mut per_page_imgs: Vec<Vec<String>> = Vec::with_capacity(opf.spine.len());
    for p in &opf.spine {
        if !is_html(p) {
            per_page_imgs.push(Vec::new());
            continue;
        }
        let imgs = entries
            .iter()
            .find(|e| &e.name == p)
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .map(|html| crate::comic_split::imgs_referenced(html, dir_of(p)))
            .unwrap_or_default();
        per_page_imgs.push(imgs);
    }
    let total_imgs: usize = per_page_imgs.iter().map(|v| v.len()).sum();
    if total_imgs == 0 {
        return Err("没有找到任何图片，没法生成漫画 PDF".into());
    }

    let file2 = std::fs::File::open(input_path).map_err(|e| format!("重开母版库文件失败: {e}"))?;
    let mut zip2 = zip::ZipArchive::new(std::io::BufReader::new(file2)).map_err(|e| e.to_string())?;

    let mut images: Vec<PdfImage> = Vec::with_capacity(total_imgs);
    let mut titles: Vec<(usize, String)> = Vec::new();
    let mut done = 0usize;
    for (spine_idx, imgs) in per_page_imgs.iter().enumerate() {
        if imgs.is_empty() {
            continue;
        }
        if let Some(title) = titles_by_spine_idx.get(&spine_idx) {
            titles.push((images.len(), title.clone()));
        }
        for img_path in imgs {
            let mut f = zip2
                .by_name(img_path)
                .map_err(|e| format!("读图片 {img_path} 失败: {e}"))?;
            let mut raw = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut raw).map_err(|e| e.to_string())?;
            let trimmed = crate::imgopt::trim_margins(&raw).unwrap_or(raw);
            let sized = crate::imgopt::downscale_for_epub_comic(&trimmed).unwrap_or(trimmed);
            let pdf_img = pdfwrite::image_from_bytes(&sized)
                .map_err(|e| format!("图片 {img_path} 编不进 PDF: {e}"))?;
            images.push(pdf_img);
            done += 1;
            on_progress(done, total_imgs);
        }
    }
    if titles.is_empty() {
        // 一条 NCX 标题都没对上（畸形/没有目录结构）——至少给整本留一条书名书签，别彻底没目录。
        let name = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("漫画");
        titles.push((0, name.to_string()));
    }

    let pdf_bytes = pdfwrite::images_to_pdf_with_toc(&images, &titles)?;
    let bytes_after = pdf_bytes.len();
    let pages = images.len();
    std::fs::write(output_path, &pdf_bytes).map_err(|e| format!("写出 PDF 失败: {e}"))?;
    Ok(PdfReport { pages, bytes_before, bytes_after })
}

/// 超预算时按卷拆分——**只处理"自己产出的漫画 PDF"**（没有书签目录，视为普通用户上传的原生 PDF，
/// 返回 `Ok(None)`，调用方按现状"整本拒绝"处理，不是新引入的失败模式）。
///
/// 先按书签（对应原书 NCX 顶层"卷"边界）分组；某一卷自己还超预算（罕见，如单卷本身就很大）时
/// 在这一卷内部按页贪心再切一层，不再往更深递归——跟 `comic_split::plan_pieces_sized` 2026-09-19
/// 简化"只切一层"是同一个理由：真实漫画每卷体积通常远低于上传预算，为这种边界情况维护一整套
/// 递归复杂度不值得。
///
/// ⚠️ 已知内存权衡：解析的是"已经写到磁盘的完整 PDF"，`extract_pages` 会把所有页的图片流字节
/// 拷贝进内存（峰值 ≈ 1-2 倍这份超限 PDF 自身体积），不是像 `comic_split::deliver_split_streaming`
/// 那样逐份读逐份丢。这条路径只在漫画整本优化后仍超 90MB 上传上限时才触发，真机验证阶段必须用
/// 大部头样本测 `VmHWM`，如果峰值确实随书变大顶到风险区，需要回来改成流式重新解析（读 xref 定位
/// 每份要用到的对象、只把这一份的图片字节读进内存），这次先用直读换实现简单，不是最终定论。
pub fn split_comic_pdf_if_oversized(
    pdf_path: &Path,
    budget: u64,
) -> Result<Option<Vec<(String, Vec<u8>)>>, String> {
    let pdf_bytes = std::fs::read(pdf_path).map_err(|e| format!("读 PDF 失败: {e}"))?;
    let whole_file_size = pdf_bytes.len() as u64; // 跟调用方 `deliver()` 判超限用的同一个量（整份文件体积）
    let Ok(n) = pdfwrite::page_count(&pdf_bytes) else { return Ok(None) };
    let Ok((all_images, all_titles)) = pdfwrite::extract_pages(&pdf_bytes, 0, n) else {
        return Ok(None);
    };
    drop(pdf_bytes); // 已经拷进 all_images/all_titles，原始整份文件字节不用再留着。
    if all_titles.is_empty() {
        return Ok(None); // 没有书签目录——不是我们自己产出的漫画 PDF，不拆。
    }
    if whole_file_size <= budget {
        return Ok(None); // 整本已经在预算内，调用方按"不用拆，直接投原生"处理
    }

    // 按页分组用图片字节数做比例（PDF 对象结构/xref 的固定开销相对图片体积可以忽略——真实漫画
    // 单张图几十到几百 KB，几千个对象的 xref 表也就几十 KB），页数级预算判断已经用真实整份文件
    // 体积（上面那句），这里只是分组比例尺，不需要跟整份文件体积一样精确。
    let sizes: Vec<u64> = all_images.iter().map(|im| im.data.len() as u64).collect();

    let mut boundaries: Vec<usize> = all_titles.iter().map(|(idx, _)| *idx).collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    if boundaries.first() != Some(&0) {
        boundaries.insert(0, 0);
    }
    let title_at: std::collections::HashMap<usize, String> = all_titles.into_iter().collect();
    let stem = pdf_path.file_stem().and_then(|s| s.to_str()).unwrap_or("漫画");
    let mut pieces: Vec<(String, Vec<u8>)> = Vec::new();

    for (i, &start) in boundaries.iter().enumerate() {
        let end = boundaries.get(i + 1).copied().unwrap_or(n);
        let vol_title = title_at.get(&start).cloned().unwrap_or_else(|| format!("第 {}-{} 页", start + 1, end));
        let vol_size: u64 = sizes[start..end].iter().sum();
        if vol_size <= budget {
            let images: Vec<PdfImage> = all_images[start..end].to_vec();
            let out = pdfwrite::images_to_pdf_with_toc(&images, &[(0, vol_title.clone())])?;
            pieces.push((format!("{stem} - {vol_title}.pdf"), out));
            continue;
        }
        // 这一卷本身还超预算：按页贪心再切一层。
        let (mut s, mut acc) = (start, 0u64);
        for i in start..end {
            if acc > 0 && acc + sizes[i] > budget {
                push_sub_piece(&mut pieces, stem, &vol_title, &all_images, s, i);
                s = i;
                acc = 0;
            }
            acc += sizes[i];
        }
        push_sub_piece(&mut pieces, stem, &vol_title, &all_images, s, end);
    }
    Ok(Some(pieces))
}

fn push_sub_piece(
    pieces: &mut Vec<(String, Vec<u8>)>,
    stem: &str,
    vol_title: &str,
    all_images: &[PdfImage],
    start: usize,
    end: usize,
) {
    let sub_title = format!("{vol_title}（第 {}-{} 页）", start + 1, end);
    let images: Vec<PdfImage> = all_images[start..end].to_vec();
    match pdfwrite::images_to_pdf_with_toc(&images, &[(0, sub_title.clone())]) {
        Ok(out) => pieces.push((format!("{stem} - {sub_title}.pdf"), out)),
        Err(_) => {} // 单份组包失败极罕见（图片列表非空即不会失败）——按"这份跳过"而不是让整体拆分失败。
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn one_px_jpeg() -> Vec<u8> {
        // 复用 pdfwrite 测试里用过的最小 JPEG 骨架构造思路：只需 SOI+SOF0+EOI，宽高任意。
        vec![
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x10, 0x03, 0x01, 0x11,
            0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xD9,
        ]
    }

    fn build_test_epub(chapters: &[(&str, &[&str])]) -> Vec<u8> {
        // chapters: (spine 文件名, 引用的图片文件名列表)；每个 chapter 一个最小 xhtml。
        let mut buf = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buf);
            let mut z = zip::ZipWriter::new(cursor);
            let opt = zip::write::SimpleFileOptions::default();
            z.start_file("mimetype", opt).unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            z.start_file("META-INF/container.xml", opt).unwrap();
            z.write_all(br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#).unwrap();

            let mut manifest_items = String::new();
            let mut spine_items = String::new();
            let mut nav_points = String::new();
            let jpeg = one_px_jpeg();
            let mut img_names: Vec<String> = Vec::new();
            for (i, (chap, imgs)) in chapters.iter().enumerate() {
                let body: String = imgs
                    .iter()
                    .map(|img| format!(r#"<img src="{img}"/>"#))
                    .collect();
                z.start_file(format!("OEBPS/{chap}"), opt).unwrap();
                z.write_all(format!("<html><body>{body}</body></html>").as_bytes()).unwrap();
                manifest_items.push_str(&format!(r#"<item id="c{i}" href="{chap}" media-type="application/xhtml+xml"/>"#));
                spine_items.push_str(&format!(r#"<itemref idref="c{i}"/>"#));
                nav_points.push_str(&format!(
                    r#"<navPoint><navLabel><text>第{i}章</text></navLabel><content src="{chap}"/></navPoint>"#
                ));
                for img in imgs.iter() {
                    if !img_names.contains(&img.to_string()) {
                        img_names.push(img.to_string());
                    }
                }
            }
            for img in &img_names {
                z.start_file(format!("OEBPS/{img}"), opt).unwrap();
                z.write_all(&jpeg).unwrap();
                manifest_items.push_str(&format!(r#"<item id="{img}" href="{img}" media-type="image/jpeg"/>"#));
            }
            manifest_items.push_str(r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#);
            z.start_file("OEBPS/content.opf", opt).unwrap();
            z.write_all(format!(
                r#"<?xml version="1.0"?><package><metadata></metadata><manifest>{manifest_items}</manifest><spine toc="ncx">{spine_items}</spine></package>"#
            ).as_bytes()).unwrap();
            z.start_file("OEBPS/toc.ncx", opt).unwrap();
            z.write_all(format!(r#"<?xml version="1.0"?><ncx><navMap>{nav_points}</navMap></ncx>"#).as_bytes()).unwrap();
            z.finish().unwrap();
        }
        buf
    }

    #[test]
    fn optimize_comic_epub_to_pdf_produces_one_page_per_image_with_toc() {
        // is_comic 要求 >=20 张图——两章分别 10/11 张图，凑够 21 张触发漫画判定。
        let c1_imgs: Vec<String> = (1..=10).map(|i| format!("i{i}.jpg")).collect();
        let c2_imgs: Vec<String> = (11..=21).map(|i| format!("i{i}.jpg")).collect();
        let c1_refs: Vec<&str> = c1_imgs.iter().map(|s| s.as_str()).collect();
        let c2_refs: Vec<&str> = c2_imgs.iter().map(|s| s.as_str()).collect();
        let epub = build_test_epub(&[("c1.xhtml", &c1_refs), ("c2.xhtml", &c2_refs)]);
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.epub");
        let output = dir.path().join("test.pdf");
        std::fs::write(&input, &epub).unwrap();

        let mut calls = Vec::new();
        let rep = optimize_comic_epub_to_pdf_streaming(&input, &output, |d, t| calls.push((d, t))).unwrap();
        assert_eq!(rep.pages, 21, "21 张图应展开成 21 个 PDF 页");
        assert_eq!(calls.last(), Some(&(21, 21)));

        let pdf_bytes = std::fs::read(&output).unwrap();
        assert_eq!(pdfwrite::page_count(&pdf_bytes).unwrap(), 21);
        let (_, titles) = pdfwrite::extract_pages(&pdf_bytes, 0, 21).unwrap();
        assert_eq!(titles, vec![(0, "第0章".to_string()), (10, "第1章".to_string())], "标题应落在各章第一张图对应的页码上");
    }

    #[test]
    fn optimize_comic_epub_to_pdf_rejects_non_comic() {
        // 图太少、判不成漫画（is_comic 需要 >=20 张图）。
        let epub = build_test_epub(&[("c1.xhtml", &["i1.jpg"])]);
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.epub");
        let output = dir.path().join("test.pdf");
        std::fs::write(&input, &epub).unwrap();
        assert!(optimize_comic_epub_to_pdf_streaming(&input, &output, |_, _| {}).is_err());
    }

    #[test]
    fn split_comic_pdf_returns_none_when_within_budget() {
        let images: Vec<PdfImage> = (0..5).map(|_| pdfwrite::image_from_bytes(&one_px_jpeg()).unwrap()).collect();
        let pdf = pdfwrite::images_to_pdf_with_toc(&images, &[(0, "卷一".into())]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.pdf");
        std::fs::write(&path, &pdf).unwrap();
        assert!(split_comic_pdf_if_oversized(&path, 10_000_000).unwrap().is_none());
    }

    #[test]
    fn split_comic_pdf_returns_none_for_pdf_without_outline() {
        let images: Vec<PdfImage> = (0..5).map(|_| pdfwrite::image_from_bytes(&one_px_jpeg()).unwrap()).collect();
        let pdf = pdfwrite::images_to_pdf_with_toc(&images, &[]).unwrap(); // 无书签 = 不是我们自己的漫画 PDF
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.pdf");
        std::fs::write(&path, &pdf).unwrap();
        assert!(split_comic_pdf_if_oversized(&path, 1).unwrap().is_none());
    }

    #[test]
    fn split_comic_pdf_splits_by_volume_boundary_when_oversized() {
        // 每张图的 JPEG 骨架体积一样，两卷各 3 张图，budget 卡在刚好装不下 6 张但装得下 3 张。
        let jpeg = one_px_jpeg();
        let per_img = jpeg.len() as u64;
        let images: Vec<PdfImage> = (0..6).map(|_| pdfwrite::image_from_bytes(&jpeg).unwrap()).collect();
        let titles = vec![(0, "卷一".to_string()), (3, "卷二".to_string())];
        let pdf = pdfwrite::images_to_pdf_with_toc(&images, &titles).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.pdf");
        std::fs::write(&path, &pdf).unwrap();

        let budget = per_img * 4; // 装得下一卷（3张），装不下两卷（6张）
        let pieces = split_comic_pdf_if_oversized(&path, budget).unwrap().unwrap();
        assert_eq!(pieces.len(), 2, "应该按卷边界切成两份");
        for (name, bytes) in &pieces {
            assert!(name.contains("卷"));
            assert_eq!(pdfwrite::page_count(bytes).unwrap(), 3);
        }
    }
}
