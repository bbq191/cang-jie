//! ★ 全局待办 —— 手绘五角星检测（移植自 pkm-semantic/proto，逐算法对拍 Python）。
//!
//! 判据（真机对账定案，见 pkm-semantic/README）：**专用笔色 + 星形(自相交)** 双条件。
//! 自相交是核心不变量：pentagram 恒 5、真机松散星 10~20、圆/方框/对勾/正常字母 = 0。
//! 满页黑色手写里草书汉字也会自相交 → 必须颜色门控。
//!
//! 几何一律 f64（Point 是 f32，上采 f64 与 rmscene/Python 逐字节一致）。

use remarkable_lines::shared::pen_color::PenColor;
use remarkable_lines::v6::block::Block;
use remarkable_lines::RemarkableFile;

pub type Pt = (f64, f64);

#[derive(Debug, Clone)]
pub struct Stroke {
    pub points: Vec<Pt>,
    pub color: String, // PenColor 名，如 "RED"（与 Python _enum_name 对齐）
}

/// PenColor → 名字（对齐 Python rm_strokes._enum_name / rmscene .name）。
fn color_name(c: &PenColor) -> String {
    match c {
        PenColor::Black => "BLACK",
        PenColor::Grey => "GRAY",
        PenColor::White => "WHITE",
        PenColor::Yellow => "YELLOW",
        PenColor::Green => "GREEN",
        PenColor::Pink => "PINK",
        PenColor::Blue => "BLUE",
        PenColor::Red => "RED",
        PenColor::GreyOverlap => "GRAY_OVERLAP",
        PenColor::Unknown(v) => return format!("UNKNOWN_{v}"),
    }
    .to_string()
}

/// 只读一页 .rm → 笔划（≥2 点）。对齐 Python read_strokes。
pub fn read_strokes(rm_bytes: &[u8]) -> Result<Vec<Stroke>, String> {
    let rf = RemarkableFile::read(rm_bytes).map_err(|e| format!("解析 .rm: {e:?}"))?;
    let blocks = match rf {
        RemarkableFile::V6 { blocks, .. } => blocks,
        _ => return Err("非 v6 .rm".into()),
    };
    let mut out = Vec::new();
    for b in blocks {
        if let Block::SceneLineItem(sib) = b {
            if let Some(line) = sib.item.value {
                if line.points.len() < 2 {
                    continue;
                }
                let points = line
                    .points
                    .iter()
                    .map(|p| (p.x as f64, p.y as f64))
                    .collect();
                out.push(Stroke {
                    points,
                    color: color_name(&line.color),
                });
            }
        }
    }
    Ok(out)
}

// ---------- 几何（对拍 Python geometry.py）----------

fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn bbox(pts: &[Pt]) -> (f64, f64, f64, f64) {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x0, y0, x1, y1)
}

pub fn size_diag(pts: &[Pt]) -> f64 {
    let (x0, y0, x1, y1) = bbox(pts);
    (x1 - x0).hypot(y1 - y0)
}

pub fn aspect(pts: &[Pt]) -> f64 {
    let (x0, y0, x1, y1) = bbox(pts);
    let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
    let (lo, hi) = if w < h { (w, h) } else { (h, w) };
    if lo > 1e-6 {
        hi / lo
    } else {
        1e9
    }
}

/// 弧长等距重采样为 n 点。对拍 Python geometry.resample。
pub fn resample(pts: &[Pt], n: usize) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut d = vec![0.0f64; pts.len()];
    for i in 1..pts.len() {
        d[i] = d[i - 1] + dist(pts[i], pts[i - 1]);
    }
    let total = *d.last().unwrap();
    if total <= 1e-9 {
        return vec![pts[0]; n];
    }
    let step = total / (n as f64 - 1.0);
    let mut out = Vec::with_capacity(n);
    let mut j = 0usize;
    for i in 0..n {
        let target = i as f64 * step;
        while j < d.len() - 1 && d[j + 1] < target {
            j += 1;
        }
        if j >= pts.len() - 1 {
            out.push(*pts.last().unwrap());
            continue;
        }
        let seg = d[j + 1] - d[j];
        let t = if seg <= 1e-9 {
            0.0
        } else {
            (target - d[j]) / seg
        };
        out.push((
            pts[j].0 + t * (pts[j + 1].0 - pts[j].0),
            pts[j].1 + t * (pts[j + 1].1 - pts[j].1),
        ));
    }
    out
}

fn ccw(p: Pt, q: Pt, r: Pt) -> f64 {
    (r.1 - p.1) * (q.0 - p.0) - (q.1 - p.1) * (r.0 - p.0)
}

fn seg_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    ((ccw(c, d, a) > 0.0) != (ccw(c, d, b) > 0.0))
        && ((ccw(a, b, c) > 0.0) != (ccw(a, b, d) > 0.0))
}

/// 路径自相交次数（重采样到 n 段后数非相邻线段交点）。对拍 Python geometry.self_intersections。
pub fn self_intersections(pts: &[Pt], n: usize) -> u32 {
    let rs = resample(pts, n);
    let m = rs.len().saturating_sub(1);
    let mut c = 0u32;
    for i in 0..m {
        for j in (i + 2)..m {
            if i == 0 && j == m - 1 {
                continue; // 首尾线段相邻
            }
            if seg_cross(rs[i], rs[i + 1], rs[j], rs[j + 1]) {
                c += 1;
            }
        }
    }
    c
}

// ---------- 检测 ----------

#[derive(Debug, Clone)]
pub struct StarConfig {
    pub si_resample: usize,
    pub aspect_max: f64,
    pub size_min: f64,
    pub size_max: f64,
    pub self_int_min: u32,
    /// 颜色门控：空=不门控（黑对黑每页约 8 假阳，仅调试）；生产必设专用笔色，如 ["RED"]。
    pub todo_colors: Option<Vec<String>>,
}

impl Default for StarConfig {
    fn default() -> Self {
        StarConfig {
            si_resample: 48,
            aspect_max: 1.9,
            size_min: 40.0,
            size_max: 400.0,
            self_int_min: 5,
            todo_colors: None,
        }
    }
}

/// 单个标记（合并后的点序）是否为星。对拍 Python is_star + _geometry_star。
pub fn is_star(pts: &[Pt], cfg: &StarConfig, color: &str) -> bool {
    if let Some(cols) = &cfg.todo_colors {
        if !cols.iter().any(|c| c == color) {
            return false;
        }
    }
    let sz = size_diag(pts);
    sz >= cfg.size_min
        && sz <= cfg.size_max
        && aspect(pts) <= cfg.aspect_max
        && self_intersections(pts, cfg.si_resample) >= cfg.self_int_min
}

/// 空间相邻笔划聚成"一个标记"（真手绘星多笔叠加）。并查集 bbox 膨胀 gap 相交则同组。
/// 保留 first-seen 顺序，组内按原始下标序（对拍 Python _cluster）。
pub fn cluster(strokes: &[Stroke], gap: f64) -> Vec<Vec<usize>> {
    let n = strokes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let boxes: Vec<_> = strokes.iter().map(|s| bbox(&s.points)).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            let (ax0, ay0, ax1, ay1) = boxes[i];
            let (bx0, by0, bx1, by1) = boxes[j];
            if ax0 - gap <= bx1 && bx0 - gap <= ax1 && ay0 - gap <= by1 && by0 - gap <= ay1 {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    // first-seen 顺序聚组
    let mut order: Vec<usize> = Vec::new();
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_insert_with(|| {
            order.push(r);
            Vec::new()
        });
        groups.get_mut(&r).unwrap().push(i);
    }
    order.into_iter().map(|r| groups.remove(&r).unwrap()).collect()
}

// ---------- 整库扫描（bin/daemon 共用）----------

#[derive(Debug, Clone)]
pub struct PageHit {
    pub page_index: i64, // .content 里的页序（-1=未在页序中找到）
    pub page_uuid: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct DocStars {
    pub uuid: String,
    pub title: String,
    pub hits: Vec<PageHit>,
}

fn read_json(p: &std::path::Path) -> serde_json::Value {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// .content → 页顺序（page-uuid 列表）。兼容 cPages.pages / 顶层 pages（dict 或字符串）。
fn page_order(content: &serde_json::Value) -> Vec<String> {
    let pages = content
        .get("cPages")
        .and_then(|c| c.get("pages"))
        .or_else(|| content.get("pages"));
    match pages.and_then(|p| p.as_array()) {
        Some(arr) => arr
            .iter()
            .map(|p| match p {
                serde_json::Value::Object(_) => {
                    p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
                }
                _ => p.as_str().unwrap_or("").to_string(),
            })
            .collect(),
        None => vec![],
    }
}

/// 扫一个文档（xochitl 镜像里的 <uuid>）→ 星命中。跳过文件夹/回收站。
pub fn scan_document_dir(
    uuid: &str,
    src: &std::path::Path,
    cfg: &StarConfig,
    cluster_gap: f64,
) -> Option<DocStars> {
    let meta = read_json(&src.join(format!("{uuid}.metadata")));
    if meta.get("type").and_then(|v| v.as_str()) == Some("CollectionType") {
        return None;
    }
    if meta.get("parent").and_then(|v| v.as_str()) == Some("trash") {
        return None;
    }
    let title = meta
        .get("visibleName")
        .and_then(|v| v.as_str())
        .unwrap_or(&uuid[..uuid.len().min(8)])
        .to_string();
    let order = page_order(&read_json(&src.join(format!("{uuid}.content"))));

    let dir = src.join(uuid);
    let mut rm_files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("rm"))
        .collect();
    rm_files.sort();

    let mut hits = Vec::new();
    for rm in rm_files {
        let page_uuid = rm.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let idx = order.iter().position(|p| p == &page_uuid).map(|i| i as i64).unwrap_or(-1);
        let bytes = match std::fs::read(&rm) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let n = scan_page(&bytes, cfg, cluster_gap);
        if n > 0 {
            hits.push(PageHit { page_index: idx, page_uuid, count: n });
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by_key(|h| (h.page_index < 0, h.page_index));
    Some(DocStars { uuid: uuid.to_string(), title, hits })
}

/// 被标记的页总数（"有待办"的页数，跨全部文档）。待办的本质是"这页有记号"，不是精确计数。
pub fn flagged_page_count(docs: &[DocStars]) -> usize {
    docs.iter().map(|d| d.hits.len()).sum()
}

/// 结果稳定指纹：只随"哪本书哪页被标记"变化（**不含星数**——聚类计数在密集页不可靠，
/// 计数抖动不应触发重出文档；页面标记 ≥1 星即稳）。用于变更检测省 churn。
pub fn results_fingerprint(docs: &[DocStars]) -> String {
    let mut s = String::new();
    for d in docs {
        s.push_str(&d.uuid);
        s.push('|');
        for h in &d.hits {
            s.push_str(&format!("{},", h.page_index));
        }
        s.push('\n');
    }
    format!("{:x}", md5::compute(s.as_bytes()))
}

fn page_label(page_index: i64) -> String {
    if page_index >= 0 {
        format!("第 {} 页", page_index + 1)
    } else {
        "未知页".into()
    }
}

/// Markdown 清单（bin/调试用）。以"页面标记"为准，不显示不可靠的精确计数。
pub fn render_todo_markdown(docs: &[DocStars]) -> String {
    let pages = flagged_page_count(docs);
    let mut md = format!("# ★ 全局待办\n\n> {pages} 页有待办 · {} 本文档 · 只读扫描生成\n\n", docs.len());
    for d in docs {
        md.push_str(&format!("## {}\n", d.title));
        for h in &d.hits {
            md.push_str(&format!("- ★ {} · 有待办\n", page_label(h.page_index)));
        }
        md.push('\n');
    }
    md
}

/// 待办文档正文 HTML（组成 EPUB 一章，注入书库供设备端阅读）。
/// e-ink 纪律：纯黑正文、无花哨。原生不支持跨文件超链接 → 只列"书名 · 第N页"，靠全局搜索软跳转。
/// 以"页面标记"为准（星是记号不是数量；密集页聚类计数不可靠，不显示误导数字）。
pub fn render_todo_html(docs: &[DocStars]) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }
    let pages = flagged_page_count(docs);
    let mut h = String::new();
    h.push_str(&format!(
        "<h1>★ 全局待办</h1>\n<p>{pages} 页有待办 · {} 本文档。原生不支持跨文件跳转——用顶部搜索输入书名即可软跳转。</p>\n",
        docs.len()
    ));
    if docs.is_empty() {
        h.push_str("<p>暂无待办。用专用笔色画一个五角星即可标记。</p>\n");
    }
    for d in docs {
        h.push_str(&format!("<h2>{}</h2>\n<ul>\n", esc(&d.title)));
        for hit in &d.hits {
            h.push_str(&format!("<li>★ {} · 有待办</li>\n", page_label(hit.page_index)));
        }
        h.push_str("</ul>\n");
    }
    h
}

/// 扫整个 xochitl 镜像目录 → 所有有星的文档。bin/daemon 共用。
pub fn scan_library(src: &std::path::Path, cfg: &StarConfig, cluster_gap: f64) -> Vec<DocStars> {
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(src)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("metadata"))
        .collect();
    entries.sort();
    let mut out = Vec::new();
    for m in entries {
        if let Some(uuid) = m.file_stem().and_then(|s| s.to_str()) {
            if let Some(d) = scan_document_dir(uuid, src, cfg, cluster_gap) {
                out.push(d);
            }
        }
    }
    out
}

/// 扫一页 .rm → 命中的星数（颜色筛 → 合并 → 逐标记判）。对拍 Python scan_document 的每页逻辑。
pub fn scan_page(rm_bytes: &[u8], cfg: &StarConfig, cluster_gap: f64) -> u32 {
    let mut strokes = match read_strokes(rm_bytes) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if let Some(cols) = &cfg.todo_colors {
        strokes.retain(|s| cols.iter().any(|c| c == &s.color));
    }
    let mut n = 0u32;
    for grp in cluster(&strokes, cluster_gap) {
        let pts: Vec<Pt> = grp.iter().flat_map(|&i| strokes[i].points.clone()).collect();
        let color = &strokes[grp[0]].color;
        if is_star(&pts, cfg, color) {
            n += 1;
        }
    }
    n
}
