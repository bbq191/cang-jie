//! 生词本扫描编排（查字词的 IO 层）—— 把"全库源书 → ⚪灰词 → 查词 → 生词条"整条编排从 daemon
//! 抽出成自足模块，daemon 只剩"触发 + 注入"两件事。
//!
//! **分块归属**：查字词=**块4 系统增强**跨块能力（代码在 pkm、概念属块4），完整设计见系统增强白皮书 §08。
//!
//! 解耦要点：
//! - 词典路径**作参数传入**（不硬编设备路径）→ 路径无关、可拿 fixture 目录测。
//! - 只依赖共享**基础设施**（`cardhl` 读高亮 / `epubindex` 页→章+章全文 / `stardetect::page_order`
//!   页序 / `dict` 查词 / `cardvocab` 纯逻辑），**不碰** daemon 的星/卡片路径（`cardsync`/`sync_one_card`）。
//! - 无全局状态：章全文按 basename 在单次 `collect` 内缓存（局部 HashMap），不依赖 daemon 的 CHAP_CACHE。
//! - 渲染与注入不在此（渲染=`cardvocab::render_notebook_pages` 纯逻辑；注入=daemon 复用的 sync_auto_notebook）。

use std::collections::{HashMap, HashSet};

use crate::cardagg::TITLE_SUFFIX; // "- 总结卡片"：源书排除卡片本，单一来源不重定义
use crate::cardhl;
use crate::cardvocab::{self, VocabEntry};
use crate::dict::{self, Dict};
use crate::stardetect::page_order;
use device_core::epubindex::{
    chapter_map_hier, page_chapter_label, page_fulltext, page_section, parse_sections,
};

fn read_json(path: &str) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// 是否"源书"（epub/pdf 等被读的书，非笔记本/卡片/回收站）。daemon 的 need_vocab 触发也用它。
pub fn is_source_book(dir: &str, uuid: &str) -> bool {
    let meta = read_json(&format!("{dir}/{uuid}.metadata"));
    if meta.get("type").and_then(|x| x.as_str()) != Some("DocumentType") {
        return false;
    }
    if meta.get("parent").and_then(|x| x.as_str()) == Some("trash") {
        return false;
    }
    if meta.get("visibleName").and_then(|x| x.as_str()).unwrap_or("").ends_with(TITLE_SUFFIX) {
        return false;
    }
    // fileType == notebook 的是笔记本（卡片/汇总本/MOC）；epub/pdf 才是源书。
    read_json(&format!("{dir}/{uuid}.content")).get("fileType").and_then(|x| x.as_str()) != Some("notebook")
}

/// 枚举一本书**全部已标注页**（脱离画星）→ (page_index 0-based[-1=不在页序], page_uuid)。
/// 复用 `stardetect::page_order` 的页序映射；只有画过东西的页才有 .rm，天然轻。
fn enumerate_pages(dir: &str, uuid: &str) -> Vec<(i64, String)> {
    let order = page_order(&read_json(&format!("{dir}/{uuid}.content")));
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(format!("{dir}/{uuid}")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("rm") {
                continue;
            }
            let page_uuid = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let idx = order.iter().position(|x| x == &page_uuid).map(|i| i as i64).unwrap_or(-1);
            out.push((idx, page_uuid));
        }
    }
    out
}

/// 某页 .rm 的最后修改时间（ms）。用于单词笔记基线门控（只纳入开关打开后画的页）。读不到→0。
fn page_rm_mtime_ms(dir: &str, uuid: &str, page_uuid: &str) -> u64 {
    std::fs::metadata(format!("{dir}/{uuid}/{page_uuid}.rm"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 某页 .rm 的 ⚪灰色荧光笔高亮文字（只 Gray 槽；不复用 daemon 的 page_highlights 以免依赖卡片路径）。
fn gray_words(dir: &str, uuid: &str, page_uuid: &str) -> Vec<String> {
    let rm = format!("{dir}/{uuid}/{page_uuid}.rm");
    let bytes = match std::fs::read(&rm) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    let hls = match cardhl::extract_highlights(&bytes) {
        Ok(h) => h,
        Err(_) => return Vec::new(),
    };
    let gray = cardhl::HlColor::Gray.slot();
    hls.into_iter().filter(|h| h.slot == gray).map(|h| h.text).collect()
}

type Titles = HashMap<String, (String, Option<String>)>;

/// 一本书的取原句上下文（spine 起始页表 + basename→章名 + epub 字节）。缺 .epubindex/.epub → 空/None。
fn epub_ctx(dir: &str, uuid: &str) -> (Vec<(String, u32)>, Titles, Option<Vec<u8>>) {
    let sections = std::fs::read(format!("{dir}/{uuid}.epubindex")).ok().map(|b| parse_sections(&b)).unwrap_or_default();
    let epub = std::fs::read(format!("{dir}/{uuid}.epub")).ok();
    let titles = epub.as_ref().map(|b| chapter_map_hier(b)).unwrap_or_default();
    (sections, titles, epub)
}

/// 全库源书 → 生词条。**整体精确优先、miss 词级拆分**（`dict::lookup_phrase`）；查不到丢弃；同书同词去重。
/// `en_path`/`zh_path` 缺文件 → 该向不查（功能降级）。原句靠 epub 章全文 + `sentence_of`（epub 缺则空）。
/// `since_ms`=单词笔记开关基线：>0 时**只纳入 .rm mtime >= since_ms 的页**（"开前画的灰词不补"）；0=全纳入。
pub fn collect(dir: &str, en_path: &str, zh_path: &str, since_ms: u64) -> Vec<VocabEntry> {
    let en = Dict::open(en_path).ok();
    let zh = Dict::open(zh_path).ok();
    if en.is_none() && zh.is_none() {
        return Vec::new(); // 无任何词典 → 不查
    }
    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new(); // (书名, 词) 去重
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
            continue;
        }
        let uuid = match p.file_stem().and_then(|s| s.to_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };
        if !is_source_book(dir, &uuid) {
            continue;
        }
        let title = read_json(&format!("{dir}/{uuid}.metadata"))
            .get("visibleName")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            continue;
        }
        let (sections, titles, epub) = epub_ctx(dir, &uuid);
        let mut chapcache: HashMap<String, Option<String>> = HashMap::new(); // basename → 章全文
        for (pi, page_uuid) in enumerate_pages(dir, &uuid) {
            if pi < 0 {
                continue;
            }
            // 基线门控：只纳入开关打开后画的页（.rm mtime >= since_ms）。since_ms=0 全纳入。
            if since_ms > 0 && page_rm_mtime_ms(dir, &uuid, &page_uuid) < since_ms {
                continue;
            }
            let pidx = pi as usize;
            let grays = gray_words(dir, &uuid, &page_uuid);
            if grays.is_empty() {
                continue;
            }
            let label = page_chapter_label(&sections, &titles, pidx).unwrap_or_default();
            for hl in &grays {
                for (word, entry) in dict::lookup_phrase(en.as_ref(), zh.as_ref(), hl) {
                    if !seen.insert((title.clone(), word.to_lowercase())) {
                        continue;
                    }
                    let sentence = sentence_for(&epub, &sections, pidx, &word, &mut chapcache);
                    out.push(VocabEntry::from_dict(&word, &entry, sentence, &title, &label, &(pidx + 1).to_string()));
                }
            }
        }
    }
    out
}

/// 该词所在整句：取所属章全文（按 basename 缓存避免重复解压）→ `cardvocab::sentence_of` 扩句。epub 缺 → 空。
fn sentence_for(
    epub: &Option<Vec<u8>>,
    sections: &[(String, u32)],
    pidx: usize,
    word: &str,
    cache: &mut HashMap<String, Option<String>>,
) -> String {
    let bytes = match epub {
        Some(b) => b,
        None => return String::new(),
    };
    if sections.is_empty() {
        return String::new();
    }
    // 无所属章（页落在首个 spine 起始页之前）→ 无从取原句。page_fulltext 内部同样以 page_section 起头、
    // 会一致返回 None；这里直接短路，既显式化该不变量、又避免用空串 key 缓存（防不同无章页误命中）。
    let base = match page_section(sections, pidx) {
        Some(b) => b.to_string(),
        None => return String::new(),
    };
    let ft = cache.entry(base).or_insert_with(|| page_fulltext(bytes, sections, pidx));
    ft.as_ref().and_then(|t| cardvocab::sentence_of(t, word)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // 造一个临时 xochitl 镜像目录，写 <uuid>.metadata + <uuid>.content。
    fn setup(tag: &str) -> std::path::PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("pkm_vocabscan_{tag}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn write_doc(dir: &std::path::Path, uuid: &str, meta: &str, content: &str) {
        std::fs::File::create(dir.join(format!("{uuid}.metadata"))).unwrap().write_all(meta.as_bytes()).unwrap();
        std::fs::File::create(dir.join(format!("{uuid}.content"))).unwrap().write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn is_source_book_discriminates() {
        let dir = setup("srcbook");
        let ds = dir.to_str().unwrap();
        // epub 源书 → true
        write_doc(&dir, "book", r#"{"type":"DocumentType","parent":"","visibleName":"《13 67》"}"#, r#"{"fileType":"epub"}"#);
        assert!(is_source_book(ds, "book"));
        // 笔记本（卡片/汇总本）→ false
        write_doc(&dir, "nb", r#"{"type":"DocumentType","parent":"","visibleName":"随便笔记"}"#, r#"{"fileType":"notebook"}"#);
        assert!(!is_source_book(ds, "nb"));
        // 卡片本（visibleName 带后缀）→ false（即便 fileType 万一非 notebook 也排除）
        write_doc(&dir, "card", r#"{"type":"DocumentType","parent":"","visibleName":"《X》- 总结卡片"}"#, r#"{"fileType":"notebook"}"#);
        assert!(!is_source_book(ds, "card"));
        // 回收站 → false
        write_doc(&dir, "trashed", r#"{"type":"DocumentType","parent":"trash","visibleName":"《旧书》"}"#, r#"{"fileType":"epub"}"#);
        assert!(!is_source_book(ds, "trashed"));
        // 文件夹 → false
        write_doc(&dir, "folder", r#"{"type":"CollectionType","parent":"","visibleName":"library"}"#, r#"{}"#);
        assert!(!is_source_book(ds, "folder"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collect_degrades_without_dicts() {
        let dir = setup("nodict");
        let ds = dir.to_str().unwrap();
        write_doc(&dir, "book", r#"{"type":"DocumentType","parent":"","visibleName":"《书》"}"#, r#"{"fileType":"epub"}"#);
        // 两部词典路径都不存在 → 该向不查 → 空（功能降级不 panic）
        let out = collect(ds, "/nonexistent-en.tsv", "/nonexistent-zh.tsv", 0);
        assert!(out.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
