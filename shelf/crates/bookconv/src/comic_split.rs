//! EPUB 漫画超限投原生时按 NCX 结构递归拆分——2026-09-18 用户真机反馈驱动（真机测《火影忍者》
//! 281MB 撞 xochitl `/upload` 上限，之前的规则是"大部头漫画默认不投原生"，用户要求"一旦发现超限
//! 就全部拆"，改成按卷/章递归拆成能塞进上传上限的若干份，各自独立投递，不再全有全无）。
//! **只对 EPUB 格式漫画做**（用户明确：CBZ 走 host `comic_gray.py`/`cbz_to_pdf` 那条完全独立的
//! Python 管线，这次不碰）。
//!
//! 拆分依据是书自带的 `toc.ncx`——真机《火影忍者》验证过这类 Calibre 转出的合集漫画会在 NCX 里
//! 老老实实标好每一卷的起始页（`<navLabel>火影忍者（卷八）</navLabel><content src="text/part0001.html.../>`），
//! 不用自己猜结构。递归规则：整书超预算 → 按 NCX 顶层节点切；某一份还超 → 用它自己更深一层的 NCX
//! 节点接着切；切到没有更深节点了还超 → 放弃这一份（不投原生，调用方据此提示"哪几卷没能投上"），
//! 不会无限拆下去，也不会为了硬塞进预算而损内容。

use crate::epub::{assemble, Book, BookMeta, Chapter, Resource};
use crate::wash::{dir_of, is_html, parse_opf, posix_norm, resolve, Entry};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

fn navpoint_event_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)(<navPoint\b)|(</navPoint>)|<navLabel>\s*<text>([^<]*)</text>\s*</navLabel>|<content\s+src="([^"]*)""#).unwrap()
    })
}

/// 线性扫 `toc.ncx`，展平成 `(depth, title, target)` 序列（不建真正的树——跟本 crate 一贯的
/// "扁平+depth"写法一致，如 `wash.rs::dense_ranks`）。NCX 规范保证 `<navLabel>` 和 `<content>`
/// 总是先于自己的子 `<navPoint>` 出现，扫描时按"刚看到 content 就用当前 depth/title 落地一条"
/// 处理即可，不用等子节点扫完。
fn parse_ncx_flat(ncx_text: &str) -> Vec<(usize, String, String)> {
    let mut depth = 0usize;
    let mut cur_title = String::new();
    let mut out = Vec::new();
    for c in navpoint_event_re().captures_iter(ncx_text) {
        if c.get(1).is_some() {
            depth += 1;
        } else if c.get(2).is_some() {
            depth = depth.saturating_sub(1);
        } else if let Some(t) = c.get(3) {
            cur_title = t.as_str().trim().to_string();
        } else if let Some(s) = c.get(4) {
            out.push((depth, std::mem::take(&mut cur_title), s.as_str().to_string()));
        }
    }
    out
}

struct Node {
    title: String,
    start: usize,
    /// 半开区间 [start, end) 的 end——下一个同级节点的 start，或（最后一个同级）继承自容器边界。
    end: usize,
    children: Vec<Node>,
}

/// 把扁平 `(depth,title,spine_idx)` 序列建成真正的树，`end` 边界逐层从容器传下去。
fn build_tree(nodes: &[(usize, String, usize)]) -> Vec<Node> {
    fn go(nodes: &[(usize, String, usize)], i: &mut usize, depth: usize, container_end: usize) -> Vec<Node> {
        let mut out = Vec::new();
        while *i < nodes.len() && nodes[*i].0 == depth {
            let (_, title, start) = nodes[*i].clone();
            *i += 1;
            let end_hint = if *i < nodes.len() && nodes[*i].0 == depth { nodes[*i].2 } else { container_end };
            let children = if *i < nodes.len() && nodes[*i].0 > depth { go(nodes, i, nodes[*i].0, end_hint) } else { Vec::new() };
            out.push(Node { title, start, end: end_hint, children });
        }
        out
    }
    if nodes.is_empty() {
        return Vec::new();
    }
    let min_depth = nodes.iter().map(|n| n.0).min().unwrap();
    let container_end = usize::MAX; // 顶层容器边界由调用方（拿到 spine 总长）再钳一次
    let mut i = 0;
    go(nodes, &mut i, min_depth, container_end)
}

/// 一页 (x)html 里引用的图片，解析成 zip 内绝对路径（相对该页自身目录解析，去重按出现顺序）。
fn imgs_referenced(html: &str, page_dir: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"(?i)<img\b[^>]*\bsrc="([^"]+)""#).unwrap());
    let mut seen = std::collections::HashSet::new();
    re.captures_iter(html)
        .map(|c| posix_norm(&resolve(page_dir, &c[1])))
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// `[start,end)` 这段 spine（含它引用的图片）的字节总量估算——page 自身 + 各自引用的图（按路径去重，
/// 防止极端情况下多页共用同一张图被重复计入）。不追求字节级精确，够判断"塞不塞得进预算"就行。
fn range_bytes(entries: &[Entry], spine: &[String], start: usize, end: usize) -> u64 {
    let mut total = 0u64;
    let mut counted_imgs = std::collections::HashSet::new();
    for p in &spine[start..end] {
        let Some(e) = entries.iter().find(|e| &e.name == p) else { continue };
        total += e.data.len() as u64;
        if !is_html(p) {
            continue;
        }
        let Ok(html) = std::str::from_utf8(&e.data) else { continue };
        for img in imgs_referenced(html, dir_of(p)) {
            if counted_imgs.insert(img.clone()) {
                if let Some(ie) = entries.iter().find(|e| e.name == img) {
                    total += ie.data.len() as u64;
                }
            }
        }
    }
    total
}

/// 拆分方案的一条：能塞进预算的一份（`fits=true`，调用方据此打包投递）或拆到叶子仍超限被放弃的
/// 一份（`fits=false`，调用方据此提示用户"哪一卷没能投原生"，原书完整字节仍在母版库/KOReader）。
pub struct PlannedPiece {
    pub title: String,
    pub start: usize,
    pub end: usize,
    pub fits: bool,
}

fn plan_node(node: &Node, entries: &[Entry], spine: &[String], budget: u64, out: &mut Vec<PlannedPiece>) {
    let size = range_bytes(entries, spine, node.start, node.end);
    if size <= budget || node.children.is_empty() {
        out.push(PlannedPiece { title: node.title.clone(), start: node.start, end: node.end, fits: size <= budget });
        return;
    }
    for c in &node.children {
        plan_node(c, entries, spine, budget, out);
    }
}

/// 整本书是否超预算、要不要拆的入口。返回 `None`＝整本已经在预算内，调用方按"不用拆，原样整本投"
/// 处理（跟改动前行为完全一致）。拆不出方案（没有 `toc.ncx`/NCX 目标对不上 spine）时报错，调用方
/// 退回"超限直接拒绝"这条改动前就有的老路径，不是新引入的失败模式。
pub fn plan_splits(entries: &[Entry], budget: u64) -> Result<Option<Vec<PlannedPiece>>, String> {
    let Some(opf) = parse_opf(entries) else { return Err("解不出 OPF/spine，没法按结构拆".into()) };
    let total = range_bytes(entries, &opf.spine, 0, opf.spine.len());
    if total <= budget {
        return Ok(None);
    }
    let Some(ncx_path) = &opf.ncx else { return Err("没有 toc.ncx，没法按结构拆".into()) };
    let Some(ncx_entry) = entries.iter().find(|e| &e.name == ncx_path) else { return Err("toc.ncx 缺失".into()) };
    let ncx_text = String::from_utf8_lossy(&ncx_entry.data);
    let ncx_dir = dir_of(ncx_path);
    let flat = parse_ncx_flat(&ncx_text);
    let resolved: Vec<(usize, String, usize)> = flat
        .into_iter()
        .filter_map(|(depth, title, target)| {
            let no_frag = target.split('#').next().unwrap_or(&target);
            let abs = posix_norm(&resolve(ncx_dir, no_frag));
            opf.spine.iter().position(|p| p == &abs).map(|idx| (depth, title, idx))
        })
        .collect();
    if resolved.is_empty() {
        return Err("toc.ncx 目标都对不上 spine，没法按结构拆".into());
    }
    let mut roots = build_tree(&resolved);
    // 顶层容器边界钳到真实 spine 长度（build_tree 建树时顶层用 usize::MAX 占位，容器边界只有
    // 拿到 spine 总长后才知道）；只有"最后一个顶层节点"会被这个钳制实际影响到。
    if let Some(last) = roots.last_mut() {
        if last.end == usize::MAX {
            last.end = opf.spine.len();
        }
    }
    let mut out = Vec::new();
    for r in &roots {
        plan_node(r, entries, &opf.spine, budget, &mut out);
    }
    Ok(Some(out))
}

/// 按规划出的一段 spine range 组一份独立 EPUB：range 内每页各自的图片重新收进
/// `images/NNNN.{ext}`（去重、路径全新分配，不依赖原书目录结构），页面本身简化成
/// "一张图占一页"的最小 body（原页面的 CSS/装饰 wrapper 对纯图片漫画页没有实质意义，不带过去，
/// 避免连带原书内联 style/字体锁这类已经被 `optimize_epub_with` 处理过的东西节外生枝）。
/// 只接 `entries`（不接 `spine`）——`Opf`/`parse_opf` 是 `pub(crate)`，跨 crate（book-serve）调
/// 不到，`spine` 在这里重新解一遍（跟 `plan_splits` 内部解的那次逻辑相同、成本可忽略）。
pub fn build_piece(entries: &[Entry], start: usize, end: usize, title: &str, book_id_suffix: &str) -> Result<Vec<u8>, String> {
    let opf = parse_opf(entries).ok_or("解不出 OPF/spine")?;
    let spine = &opf.spine;
    let mut chapters = Vec::new();
    let mut resources = Vec::new();
    let mut remap: HashMap<String, String> = HashMap::new();
    for p in &spine[start..end] {
        let Some(e) = entries.iter().find(|e| &e.name == p) else { continue };
        if !is_html(p) {
            continue;
        }
        let Ok(html) = std::str::from_utf8(&e.data) else { continue };
        let mut body = String::new();
        for img in imgs_referenced(html, dir_of(p)) {
            let new_path = match remap.get(&img) {
                Some(np) => np.clone(),
                None => {
                    let Some(ie) = entries.iter().find(|e| e.name == img) else { continue };
                    let ext = img.rsplit('.').next().unwrap_or("jpg").to_ascii_lowercase();
                    let media = if ext == "png" { "image/png" } else { "image/jpeg" };
                    let np = format!("images/{:04}.{ext}", resources.len() + 1);
                    resources.push(Resource { path: np.clone(), media_type: media.into(), bytes: ie.data.clone() });
                    remap.insert(img.clone(), np.clone());
                    np
                }
            };
            body.push_str(&format!(r#"<div style="text-align:center"><img src="{new_path}"/></div>"#));
        }
        if !body.is_empty() {
            chapters.push(Chapter { title: String::new(), html_body: body, level: 1 });
        }
    }
    if chapters.is_empty() {
        return Err(format!("《{title}》这一段没有找到可用页面，跳过"));
    }
    let mut book = Book {
        meta: BookMeta {
            book_id: format!("cangjie-comic-split-{book_id_suffix}"),
            title: title.to_string(),
            author: String::new(),
            language: "zh".into(),
            publisher: String::new(),
            cover: None,
            cover_ext: "jpg".into(),
            cover_media_type: "image/jpeg".into(),
        },
        chapters,
        resources,
    };
    assemble(&mut book)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, data: &[u8]) -> Entry {
        Entry { name: name.into(), data: data.to_vec() }
    }
    fn html_e(name: &str, img_rel: &str) -> Entry {
        e(name, format!(r#"<html><body><img src="{img_rel}"/></body></html>"#).as_bytes())
    }

    /// 造一本 N 卷合集漫画：每卷若干页，NCX 标好每卷起始页，跟真机《火影忍者》结构一致
    /// （每页一个 `text/pNNNN.html` + 一张 `images/NNNN.jpg`）。
    fn make_multivol(pages_per_vol: &[usize], page_bytes: usize) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut manifest = String::new();
        let mut spine = String::new();
        let mut navpoints = String::new();
        let mut page_no = 0usize;
        for (vi, &n) in pages_per_vol.iter().enumerate() {
            let vol_start_page = page_no;
            for _ in 0..n {
                entries.push(html_e(&format!("text/p{page_no:04}.html"), &format!("../images/{page_no:04}.jpg")));
                entries.push(e(&format!("images/{page_no:04}.jpg"), &vec![7u8; page_bytes]));
                manifest += &format!(
                    r#"<item id="h{page_no}" href="text/p{page_no:04}.html" media-type="application/xhtml+xml"/><item id="i{page_no}" href="images/{page_no:04}.jpg" media-type="image/jpeg"/>"#
                );
                spine += &format!(r#"<itemref idref="h{page_no}"/>"#);
                page_no += 1;
            }
            navpoints += &format!(
                r#"<navPoint id="nv{vi}"><navLabel><text>卷{vi}</text></navLabel><content src="text/p{vol_start_page:04}.html"/></navPoint>"#
            );
        }
        entries.push(e(
            "content.opf",
            format!(r#"<package version="3.0"><metadata><dc:title>t</dc:title></metadata><manifest>{manifest}<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="ncx">{spine}</spine></package>"#).as_bytes(),
        ));
        entries.push(e("toc.ncx", format!(r#"<ncx><navMap>{navpoints}</navMap></ncx>"#).as_bytes()));
        entries.push(e(
            "META-INF/container.xml",
            br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
        ));
        entries
    }

    #[test]
    fn whole_book_within_budget_returns_none() {
        let entries = make_multivol(&[2, 2, 2], 100);
        assert!(plan_splits(&entries, 10_000).unwrap().is_none());
    }

    #[test]
    fn splits_by_top_level_ncx_when_oversized() {
        // 每卷 3 页 * 1000 字节 ≈ 3000+，整书 3 卷 ≈ 9000+，预算给 4000 只够单卷
        let entries = make_multivol(&[3, 3, 3], 1000);
        let pieces = plan_splits(&entries, 4000).unwrap().expect("应该要拆");
        assert_eq!(pieces.len(), 3, "三卷应该各自成一份");
        assert!(pieces.iter().all(|p| p.fits), "每卷单独都在预算内");
        assert_eq!(pieces[0].title, "卷0");
        assert_eq!(pieces[1].start, 3, "第二卷从第 3 页开始");
        assert_eq!(pieces[2].end, 9, "第三卷（也是最后一卷）end 钳到 spine 总长");
    }

    #[test]
    fn leaf_still_oversized_marked_not_fits_without_infinite_recursion() {
        // 只有顶层 NCX（没有更深一层），某一卷本身就超预算——放弃这一份，不无限递归。
        let entries = make_multivol(&[1, 5, 1], 2000);
        let pieces = plan_splits(&entries, 3000).unwrap().expect("应该要拆");
        assert_eq!(pieces.len(), 3);
        assert!(pieces[0].fits && pieces[2].fits, "两个小卷都在预算内");
        assert!(!pieces[1].fits, "中间这卷 5 页仍超预算，标记为放弃、不是死循环");
    }

    /// 手动验证用：拿真机真实下载的《火影忍者》测——不进 CI（本地文件路径依赖），
    /// `cargo test --lib comic_split::tests::against_real_naruto_book -- --ignored --nocapture` 手跑。
    #[test]
    #[ignore]
    fn against_real_naruto_book() {
        let path = "/tmp/naruto/naruto.epub";
        let bytes = std::fs::read(path).expect("先手动 scp 真机文件到这个路径");
        let entries = crate::check::read_entries(&bytes).unwrap();
        let budget = 150 * 1024 * 1024; // 跟 book-serve 缺省 native_upload_limit_mb 一致
        let pieces = plan_splits(&entries, budget).unwrap().expect("281MB 应该超预算触发拆分");
        println!("拆出 {} 份:", pieces.len());
        let opf = parse_opf(&entries).unwrap();
        for p in &pieces {
            let sz = range_bytes(&entries, &opf.spine, p.start, p.end);
            println!("  {} [{},{}) {} MB fits={}", p.title, p.start, p.end, sz / 1024 / 1024, p.fits);
        }
        assert!(pieces.iter().all(|p| p.fits), "真机这本书每卷体积应该都在 150MB 预算内");
        // 实际组一份出来，确认真的是合法 EPUB——挑真有内容的一卷（卷八），不是只有一页的封面份。
        let vol8 = pieces.iter().find(|p| p.title.contains("卷八")).unwrap();
        let out = build_piece(&entries, vol8.start, vol8.end, &vol8.title, "naruto-vol8").unwrap();
        std::fs::write("/tmp/naruto-vol8-test.epub", &out).unwrap();
        println!("组出{}（{} 页）{} 字节，写到 /tmp/naruto-vol8-test.epub 供人工核验", vol8.title, vol8.end - vol8.start, out.len());
    }

    #[test]
    fn build_piece_produces_valid_epub_with_remapped_images() {
        let entries = make_multivol(&[2], 50);
        let opf = parse_opf(&entries).unwrap();
        let bytes = build_piece(&entries, 0, 2, "卷0", "t0").unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(zip.by_name("OEBPS/images/0001.jpg").is_ok(), "第一页图片应该被收进新资源目录");
        assert!(zip.by_name("OEBPS/images/0002.jpg").is_ok());
        use std::io::Read;
        let mut c1 = String::new();
        zip.by_name("OEBPS/chap_0001.xhtml").unwrap().read_to_string(&mut c1).unwrap();
        assert!(c1.contains(r#"src="images/0001.jpg""#), "{c1}");
    }
}
