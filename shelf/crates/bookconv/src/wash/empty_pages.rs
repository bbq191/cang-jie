//! 空页清理：删掉纯空白页并把目录/spine 引用改指向邻页。
use super::*;

// ───────────────────────── 5. 空页清理 ─────────────────────────

pub(super) fn is_empty_page(html: &str) -> bool {
    static BODY: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    let body = BODY.get_or_init(|| Regex::new(r#"(?is)<body\b[^>]*>(.*?)</body>"#).unwrap());
    let tag = TAG.get_or_init(|| Regex::new(r#"(?s)<[^>]*>"#).unwrap());
    let inner = body.captures(html).map(|c| c[1].to_string()).unwrap_or_default();
    let low = inner.to_ascii_lowercase();
    if low.contains("<img") || low.contains("<svg") || low.contains("<image") || low.contains("<video") || low.contains("<audio") {
        return false;
    }
    let text = tag.replace_all(&inner, "");
    let text = text.replace("&nbsp;", " ").replace("&#160;", " ").replace('\u{a0}', " ");
    text.trim().is_empty()
}

/// 一遍扫描删掉 OPF 里 `id`/`idref` 属于 `ids` 的 `<item>`/`<itemref>`（连同紧随的空白与空闭合标签）。
/// 此前对每个被删页各编译两个正则、各整份扫描一遍 OPF：Calibre MOBI 转出的书常有上千个 `mbppagebreak` 独占页，
/// 即上千次编译 + 上千次整份 OPF 扫描（`bench_tmp emptypages`：1500 页/750 空页，host 380ms → 见提交说明）。
pub(super) fn drop_opf_refs(opf: &str, ids: &HashSet<&str>) -> String {
    static ITEMREF: OnceLock<Regex> = OnceLock::new();
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static IDREF_ATTR: OnceLock<Regex> = OnceLock::new();
    static ID_ATTR: OnceLock<Regex> = OnceLock::new();
    let itemref = ITEMREF.get_or_init(|| Regex::new(r#"<itemref\b[^>]*>(?:\s*</itemref>)?\s*"#).unwrap());
    let item = ITEM.get_or_init(|| Regex::new(r#"<item\b[^>]*>(?:\s*</item>)?\s*"#).unwrap());
    let idref_attr = IDREF_ATTR.get_or_init(|| Regex::new(r#"\bidref="([^"]*)""#).unwrap());
    let id_attr = ID_ATTR.get_or_init(|| Regex::new(r#"\bid="([^"]*)""#).unwrap());
    let drop_if = |c: &regex::Captures, attr: &Regex| -> String {
        let whole = c.get(0).unwrap().as_str();
        let tag_end = whole.find('>').map_or(whole.len(), |i| i + 1);
        if attr.captures_iter(&whole[..tag_end]).any(|a| ids.contains(&a[1])) {
            String::new()
        } else {
            whole.to_string()
        }
    };
    let out = itemref.replace_all(opf, |c: &regex::Captures| drop_if(c, idref_attr));
    item.replace_all(&out, |c: &regex::Captures| drop_if(c, id_attr)).into_owned()
}

pub(super) fn remove_empty_pages(entries: &mut Vec<Entry>, rep: &mut WashReport) {
    let Some(opf) = parse_opf(entries) else { return };
    let mut removed: Vec<String> = Vec::new();
    for p in &opf.spine {
        if Some(p) == opf.nav_doc.as_ref() {
            continue;
        }
        if let Some(e) = entries.iter().find(|e| &e.name == p) {
            if is_html(&e.name) && is_empty_page(&String::from_utf8_lossy(&e.data)) {
                removed.push(p.clone());
            }
        }
    }
    if removed.is_empty() || removed.len() >= opf.spine.len() {
        return; // 全空不动（别把书删没）
    }
    let removed_set: HashSet<&String> = removed.iter().collect();
    // 替换目标：spine 里下一篇未删的，没有则上一篇
    let replacement = |p: &String| -> Option<String> {
        let i = opf.spine.iter().position(|x| x == p)?;
        opf.spine[i + 1..].iter().chain(opf.spine[..i].iter().rev()).find(|x| !removed_set.contains(x)).cloned()
    };
    let repl: HashMap<String, String> = removed.iter().filter_map(|p| replacement(p).map(|r| (p.clone(), r))).collect();
    // OPF：删 itemref + item
    let ids: HashSet<&str> = opf.items.iter().filter(|(_, v)| removed_set.contains(v)).map(|(k, _)| k.as_str()).collect();
    let text = drop_opf_refs(&String::from_utf8_lossy(&entries[opf.index].data), &ids);
    entries[opf.index].data = text.into_bytes();
    // 目录（ncx/nav）里指向被删页的引用 → 改指替换页
    let toc_files: Vec<String> = entries.iter().filter(|e| is_toc_file(&e.name)).map(|e| e.name.clone()).collect();
    for tf in toc_files {
        let tdir = dir_of(&tf).to_string();
        let Some(e) = entries.iter_mut().find(|e| e.name == tf) else { continue };
        let text = String::from_utf8_lossy(&e.data).into_owned();
        let new = href_re().replace_all(&text, |c: &regex::Captures| {
            let target = resolve(&tdir, &percent_decode(&c[2]));
            match repl.get(&target) {
                Some(r) => format!("{}=\"{}\"", &c[1], relative_to(&tdir, r)),
                None => c[0].to_string(),
            }
        }).into_owned();
        e.data = new.into_bytes();
    }
    entries.retain(|e| !removed_set.contains(&e.name));
    rep.empty_pages_removed = removed;
}
