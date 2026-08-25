//! 星→卡片编排 —— 从 wr-stars-daemon 抽出的"扫出的星页 → 每本书一份可编辑《总结卡片》"整条编排。
//! daemon 扫库（stardetect）拿到 `DocStars` 后交给这里：取章节/标签/高亮 → 按当前星重生成卡片
//! （老星保批注、新星空批注）→ /upload 新本 + 旧本入 pending-trash。抽出后逻辑集中、daemon 只剩派发。
//!
//! 用户打字批注靠"读回打字→重建保留"存活（`cardsync` merge），**只认 Text 工具打字，手写读不回不保**。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

use crate::cardagg::TITLE_SUFFIX as CARD_SUFFIX; // "- 总结卡片"：源书排除卡片本 + 卡片本命名，单一来源
use crate::cardhl;
use crate::cardsync::{parse_pages, render_card_starspecs, CardTemplate};
use crate::notebook_sync::{doc_age_secs, find_docs_by_visible, queue_trash, read_card_pages, UPLOAD_HOST};
use crate::stardetect::DocStars;
use crate::cardnote;
use device_core::epubindex::{chapter_map_hier, page_chapter_label, parse_sections};
use device_core::inject;

/// 卡片刚被编辑（用户可能正在打字）→ 本轮跳过重建、下次再合，避免读到半刷入的 .rm。
const GUARD_SECS: u64 = 20;

/// 一颗星的规格：(1-based 显示页号, 章节标签[可空], 该星模板集[空=默认], 该页按 6 色槽高亮)。
type StarSpec = (usize, String, Vec<CardTemplate>, Vec<Vec<String>>);

// (spine 起始页表, basename→(章名,父章)) —— 一本 EPUB 的章节信息。
type ChapterInfo = (Vec<(String, u32)>, HashMap<String, (String, Option<String>)>);
#[allow(clippy::type_complexity)]
static CHAP_CACHE: OnceLock<Mutex<HashMap<String, (u64, ChapterInfo)>>> = OnceLock::new();

/// 取一本 EPUB 的章节信息，按 .epub mtime 缓存避免每轮重读。非 EPUB / 缺 .epubindex → None（章名留空）。
pub fn chapter_info(dir: &str, uuid: &str) -> Option<ChapterInfo> {
    let epub_path = format!("{dir}/{uuid}.epub");
    let idx_path = format!("{dir}/{uuid}.epubindex");
    let em = std::fs::metadata(&epub_path).ok()?;
    if !std::path::Path::new(&idx_path).exists() {
        return None;
    }
    let mtime = em.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let cache = CHAP_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((mt, info)) = cache.lock().unwrap().get(uuid) {
        if *mt == mtime {
            return Some(info.clone());
        }
    }
    let idx = std::fs::read(&idx_path).ok()?;
    let epub = std::fs::read(&epub_path).ok()?;
    let info: ChapterInfo = (parse_sections(&idx), chapter_map_hier(&epub));
    cache.lock().unwrap().insert(uuid.to_string(), (mtime, info.clone()));
    Some(info)
}

/// 读一本书的原生标签：文档级 `tags[].name` + 页级 `pageTags`(pageId → [name])。都在 `.content`。
pub fn read_book_tags(dir: &str, uuid: &str) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let content: serde_json::Value = std::fs::read_to_string(format!("{dir}/{uuid}.content"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);
    let doc: Vec<String> = content
        .get("tags")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    let mut page: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(a) = content.get("pageTags").and_then(|x| x.as_array()) {
        for t in a {
            if let (Some(name), Some(pid)) =
                (t.get("name").and_then(|n| n.as_str()), t.get("pageId").and_then(|p| p.as_str()))
            {
                page.entry(pid.to_string()).or_default().push(name.to_string());
            }
        }
    }
    (doc, page)
}

/// 该星页的模板集 = 合并(文档级 tags + 该页 pageTags) → `from_tag` → 去重保序。空 = 调用方退默认 General。
pub fn templates_for(doc_tags: &[String], page_tags: &[String]) -> Vec<CardTemplate> {
    let mut out: Vec<CardTemplate> = Vec::new();
    for name in doc_tags.iter().chain(page_tags.iter()) {
        if let Some(t) = CardTemplate::from_tag(name) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// 读某星页的荧光笔高亮，按 6 色槽分组（长度 SLOT_COUNT）。无 .rm/无高亮 → 全空槽。
/// A 式：画星那刻该页 .rm 已含之前画的高亮，随新星骨架一次注入、之后随批注保留。
pub fn page_highlights(dir: &str, doc_uuid: &str, page_uuid: &str) -> Vec<Vec<String>> {
    let mut slots: Vec<Vec<String>> = vec![Vec::new(); cardhl::SLOT_COUNT];
    let rm = format!("{dir}/{doc_uuid}/{page_uuid}.rm");
    if let Ok(bytes) = std::fs::read(&rm) {
        if let Ok(hls) = cardhl::extract_highlights(&bytes) {
            for h in hls {
                if h.slot < slots.len() {
                    slots[h.slot].push(h.text);
                }
            }
        }
    }
    slots
}

/// 同步一本书的卡片。返回一句状态或 None。observe=true 只报告不写。
pub fn sync_one_card(dir: &str, book_title: &str, stars: &[StarSpec], observe: bool) -> Option<String> {
    let visible = format!("《{book_title}》{CARD_SUFFIX}");
    let existing = find_docs_by_visible(dir, &visible);

    let (prev, cur_text) = match existing.first() {
        Some((u, _)) => {
            let pages = read_card_pages(dir, u);
            (parse_pages(&pages), pages.join("\n"))
        }
        None => (Default::default(), String::new()),
    };
    let new_pages = render_card_starspecs(book_title, stars, &prev);
    let new_text = new_pages.join("\n");

    // 无内容改动（最新卡与新内容逐字相同）→ **绝不上传新卡**（否则开书等无关 fswatch 也重生成→爆炸）。
    let content_same = !existing.is_empty() && new_text == cur_text;
    if content_same {
        if existing.len() > 1 {
            for (u, _) in existing.iter().skip(1) {
                queue_trash(u);
            }
            return Some(format!("《{book_title}》无改动，去重 {} 张旧卡入队列", existing.len() - 1));
        }
        return None;
    }
    // 防竞态：卡片刚被编辑（用户可能正打字）→ 本轮不动，下次再合。
    if let Some((u, _)) = existing.first() {
        if doc_age_secs(dir, u) < GUARD_SECS {
            return Some(format!("《{book_title}》卡片刚编辑过，暂缓重建"));
        }
    }
    if observe {
        return Some(format!(
            "《{book_title}》: {} 处待办, 现存卡片 {} 本{}",
            stars.len(),
            existing.len(),
            if existing.is_empty() { "（将新建）" } else { "（将更新）" }
        ));
    }

    let new_uuid = uuid::Uuid::new_v4().to_string();
    let refs: Vec<&str> = new_pages.iter().map(|s| s.as_str()).collect();
    let rmdoc = match cardnote::pack_rmdoc(&new_uuid, &visible, &refs) {
        Ok(b) => b,
        Err(e) => return Some(format!("《{book_title}》pack 失败: {e}")),
    };
    let net = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    if let Err(e) = inject::upload_document(&net, UPLOAD_HOST, &rmdoc, &format!("{visible}.rmdoc"), "application/zip") {
        return Some(format!("《{book_title}》/upload 失败: {e}"));
    }
    for (u, _) in &existing {
        queue_trash(u);
    }
    Some(format!(
        "《{book_title}》卡片{}（{} 处待办, {} 页{}）",
        if existing.is_empty() { "已创建" } else { "已更新" },
        stars.len(),
        new_pages.len(),
        if existing.len() > 1 { format!(", trash 旧 {} 本", existing.len()) } else { String::new() }
    ))
}

/// 星→卡片一整趟：对每本有星的书取章节/标签/高亮 → 组 StarSpec → `sync_one_card`。打印汇总。
pub fn scan_and_sync(dir: &str, docs: &[DocStars], observe: bool) {
    let mut msgs = Vec::new();
    for d in docs {
        // 卡片笔记本自身不当"源书"处理（防 《《X》-总结卡片》 套娃）。
        if d.title.ends_with(CARD_SUFFIX) {
            continue;
        }
        let chap = chapter_info(dir, &d.uuid);
        let (doc_tags, page_tags_map) = read_book_tags(dir, &d.uuid);
        let mut seen = std::collections::BTreeSet::new();
        let mut stars: Vec<StarSpec> = Vec::new();
        for h in &d.hits {
            if h.page_index < 0 {
                continue;
            }
            let pi = h.page_index as usize;
            if !seen.insert(pi) {
                continue;
            }
            let label = chap.as_ref().and_then(|(secs, titles)| page_chapter_label(secs, titles, pi)).unwrap_or_default();
            let ptags = page_tags_map.get(&h.page_uuid).map(|v| v.as_slice()).unwrap_or(&[]);
            let tmpls = templates_for(&doc_tags, ptags);
            let hl = page_highlights(dir, &d.uuid, &h.page_uuid);
            stars.push((pi + 1, label, tmpls, hl));
        }
        stars.sort_by_key(|(p, _, _, _)| *p);
        if stars.is_empty() {
            continue;
        }
        if observe && (!doc_tags.is_empty() || stars.iter().any(|(_, _, t, _)| !t.is_empty())) {
            let per: Vec<String> = stars.iter().map(|(p, _, t, _)| format!("p{p}={t:?}")).collect();
            println!("[stars] observe:《{}》docTags={:?} 每星模板 {}", d.title, doc_tags, per.join(" "));
        }
        if let Some(m) = sync_one_card(dir, &d.title, &stars, observe) {
            msgs.push(m);
        }
    }
    if !msgs.is_empty() {
        println!("[stars] {}", msgs.join(" · "));
    } else if observe {
        println!("[stars] observe: 无有星的书需要同步");
    }
}
