//! 荧光笔生词本 —— ⚪灰色高亮的词 → 查本地词典 → 汇成一本《📕 生词本》（纯逻辑部分 host 可测）。
//!
//! **分块归属**：查字词概念属**块4 系统增强**（阅读辅助 UX），代码在 pkm 是**跨块**——复用了 pkm 的
//! 荧光笔读回（`cardhl`）+ EPUB 章映射（`epubindex`）+ 笔记本注入（`sync_auto_notebook`）管线，不单拆
//! 二进制（同荧光笔汉字吸附的跨块处理）。完整设计=**系统增强白皮书 §08**，非 PKM 白皮书。
//!
//! 数据流（IO 在 daemon 侧 `collect_vocab`，本模块只做纯逻辑）：
//!   每页 `.rm` 的 ⚪Gray 荧光笔高亮（`cardhl`，脱离画星）→ 词 → `dict` 查释义 →
//!   `epubindex::page_fulltext` 得章全文 → `sentence_of` 扩出原句 → `VocabEntry` →
//!   `render_notebook_pages` 出笔记本页文本 → daemon `sync_auto_notebook` 注入。
//!
//! 与 星→卡片 管线**完全解耦**：Gray 高亮照常进卡片灰槽，这里是并行附加扫描，另出一本。
//! 词典数据是用户自备正版商业词典派生物，只个人自用、不入库（见 `dict` 模块头 / 白皮书）。

use crate::dict::Entry;
use crate::locate;
use std::collections::BTreeSet;

/// 笔记本标题（daemon 注入 + `collect_notebook_texts` 排除均用它，保持一致）。
pub const VOCAB_TITLE: &str = "📕 生词本";

/// 一条生词。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabEntry {
    pub word: String,     // 划过的词（已 normalize 前的展示形）
    pub phonetic: String, // 音标/拼音
    pub body: String,     // 主释义（牛津英汉双解 / 现汉）
    pub extra: String,    // 补充（当前留空）
    pub sentence: String, // 所在原句（可空，定位失败时）
    pub book: String,     // 书名
    pub chapter: String,  // 章-节名（可空）
    pub page: String,     // 1-based 显示页号（字符串，可空）
}

impl VocabEntry {
    /// 从 `dict::Entry` + 定位信息组装。
    pub fn from_dict(word: &str, e: &Entry, sentence: String, book: &str, chapter: &str, page: &str) -> VocabEntry {
        VocabEntry {
            word: word.to_string(),
            phonetic: e.phonetic.clone(),
            body: e.body.clone(),
            extra: e.extra.clone(),
            sentence,
            book: book.to_string(),
            chapter: chapter.to_string(),
            page: page.to_string(),
        }
    }
}

/// 句子终止符（中英）。换行也当句界（EPUB 段落间）。
fn is_sentence_end(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '.' | '!' | '?' | '\n' | '；' | ';' | '…')
}

const SENT_MAX_LEFT: usize = 60; // 左扩上限（字符），防无句号时吞整段
const SENT_MAX_RIGHT: usize = 120; // 右扩上限

/// 在章节全文里定位 `word`，向两侧扩到句界，返回所在整句。定位失败 → None（生词本该条不带原句）。
/// 复用 `locate::locate_range`（canon 长度 1:1，返回字符索引对原文有效）。被句界上限截断的一侧补「…」。
pub fn sentence_of(chapter_text: &str, word: &str) -> Option<String> {
    let r = locate::locate_range(word, chapter_text, 0)?;
    let chars: Vec<char> = chapter_text.chars().collect();
    if r.start >= chars.len() || r.end > chars.len() {
        return None;
    }
    // 左扩到上一句终止符之后（或上限）
    let left_bound = r.start.saturating_sub(SENT_MAX_LEFT);
    let mut s = r.start;
    while s > left_bound && !is_sentence_end(chars[s - 1]) {
        s -= 1;
    }
    // 右扩到下一句终止符（含）（或上限）
    let right_bound = (r.end + SENT_MAX_RIGHT).min(chars.len());
    let mut e = r.end;
    while e < right_bound && !is_sentence_end(chars[e - 1]) {
        e += 1;
    }
    let head_cut = s > 0 && !is_sentence_end(chars[s - 1]); // 因上限截断而非句界
    let tail_cut = e < chars.len() && !is_sentence_end(chars[e - 1]);
    let core: String = chars[s..e].iter().collect();
    let core = core.trim();
    if core.is_empty() {
        return None;
    }
    let mut out = String::new();
    if head_cut {
        out.push('…');
    }
    out.push_str(core);
    if tail_cut {
        out.push('…');
    }
    Some(out)
}

/// 释义过长时截断（牛津双解正文常含多义项/例句）。在 [max*3/5, max] 找自然断点（义项/例句/句号），
/// 找不到硬截 max，尾补「…」。短释义原样返回。
pub fn cap_body(body: &str, max: usize) -> String {
    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= max {
        return body.trim().to_string();
    }
    let lo = max * 3 / 5;
    let cut = (lo..max)
        .rev()
        .find(|&i| matches!(chars[i], '。' | '；' | ';' | '.' | '|' | '\n'))
        .map(|i| i + 1)
        .unwrap_or(max);
    let mut s: String = chars[..cut].iter().collect::<String>().trim().to_string();
    s.push('…');
    s
}

const BODY_CAP: usize = 140;

/// 渲染《生词本》页文本（按书分组，首次出现序）。**自摄取防线**：不含 `[ID:`、不含"指向/链接…ID:"
/// 触发词（且 daemon `collect_notebook_texts` 按标题排除本本，双保险）。空列表打「✓ 暂无生词」。
pub fn render_notebook_pages(entries: &[VocabEntry]) -> Vec<String> {
    let books: BTreeSet<&str> = entries.iter().map(|e| e.book.as_str()).collect();
    let mut s = String::new();
    s.push_str(VOCAB_TITLE);
    s.push('\n');
    s.push_str("🔔 DORAEMON 魔法生成 · 只读 —— 你用⚪灰色荧光笔在书里划过的词，它翻遍词典连音带义给你变出来（别在这本手写，一动就变回去）\n\n");
    s.push_str(&format!("{} 个生词 · {} 本书\n", entries.len(), books.len()));
    if entries.is_empty() {
        s.push_str("\n✓ 暂无生词——用⚪灰色荧光笔在书里划生词即可\n");
        return vec![s];
    }
    let mut order: Vec<&str> = Vec::new();
    for e in entries {
        if !order.contains(&e.book.as_str()) {
            order.push(&e.book);
        }
    }
    for book in order {
        s.push_str(&format!("\n━━ 《{book}》 ━━\n"));
        for e in entries.iter().filter(|e| e.book == book) {
            let head = if e.phonetic.is_empty() {
                e.word.clone()
            } else {
                format!("{}  {}", e.word, e.phonetic)
            };
            s.push_str(&format!("\n▸ {head}\n"));
            let body = cap_body(&e.body, BODY_CAP);
            if !body.is_empty() {
                s.push_str(&format!("  {}\n", body.replace('\n', " ")));
            }
            if !e.extra.is_empty() {
                s.push_str(&format!("  {}\n", e.extra.replace('\n', " ")));
            }
            if !e.sentence.is_empty() {
                s.push_str(&format!("  原句：{}\n", e.sentence.replace('\n', " ")));
            }
            // 来源：《书》章 · P页（章/页可空）
            let mut loc = format!("  —《{book}》");
            if !e.chapter.is_empty() {
                loc.push_str(&e.chapter);
                loc.push(' ');
            }
            if !e.page.is_empty() {
                loc.push_str(&format!("P{}", e.page));
            }
            loc.push('\n');
            s.push_str(&loc);
        }
    }
    vec![s]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_middle_zh() {
        let doc = "他走进房间。骆督察一直很讨厌医院的气味。窗外下着雨。";
        let sent = sentence_of(doc, "讨厌").unwrap();
        assert_eq!(sent, "骆督察一直很讨厌医院的气味。");
    }

    #[test]
    fn sentence_english_word() {
        let doc = "She smiled. It carried a certain cachet among the elite. Nobody argued.";
        let sent = sentence_of(doc, "cachet").unwrap();
        assert_eq!(sent, "It carried a certain cachet among the elite.");
    }

    #[test]
    fn sentence_at_paragraph_start_and_end() {
        // 词在段首（前面就是换行=句界）
        let doc = "第一行\n跌宕起伏的剧情让人着迷";
        let sent = sentence_of(doc, "跌宕").unwrap();
        assert_eq!(sent, "跌宕起伏的剧情让人着迷");
    }

    #[test]
    fn sentence_bounded_adds_ellipsis() {
        // 无任何句号的超长串 → 两侧按上限截断且补「…」
        let long: String = "字".repeat(300);
        let doc = format!("{long}目标{long}");
        let sent = sentence_of(&doc, "目标").unwrap();
        assert!(sent.starts_with('…') && sent.ends_with('…'), "两端应补省略号: {sent}");
        assert!(sent.contains("目标"));
        // 长度受上限约束（远小于 600）
        assert!(sent.chars().count() < SENT_MAX_LEFT + SENT_MAX_RIGHT + 10);
    }

    #[test]
    fn sentence_miss_returns_none() {
        assert!(sentence_of("随便一句话。", "不存在的词").is_none());
    }

    #[test]
    fn cap_body_short_unchanged_long_truncated() {
        assert_eq!(cap_body("短释义", 140), "短释义");
        let long: String = "a".repeat(200);
        let capped = cap_body(&long, 140);
        assert!(capped.ends_with('…'));
        assert!(capped.chars().count() <= 141);
    }

    fn mk(word: &str, book: &str, page: &str) -> VocabEntry {
        VocabEntry {
            word: word.into(),
            phonetic: "".into(),
            body: format!("{word} 的释义"),
            extra: "".into(),
            sentence: format!("含 {word} 的原句。"),
            book: book.into(),
            chapter: "第一章".into(),
            page: page.into(),
        }
    }

    #[test]
    fn render_groups_by_book_no_id_marker() {
        let entries = vec![
            mk("cachet", "英文书", "42"),
            mk("恻隐", "中文书", "8"),
            mk("prestige", "英文书", "43"),
        ];
        let p = render_notebook_pages(&entries).join("\n");
        assert!(p.contains("3 个生词 · 2 本书"));
        assert!(p.contains("━━ 《英文书》 ━━"));
        assert!(p.contains("━━ 《中文书》 ━━"));
        assert!(p.contains("▸ cachet"));
        assert!(p.contains("原句：含 cachet 的原句。"));
        assert!(p.contains("—《英文书》第一章 P42"));
        // 自摄取防线：不含 [ID: 和触发词
        assert!(!p.contains("[ID:"), "生词本正文不得含 [ID: 锚点");
    }

    #[test]
    fn render_empty() {
        let p = render_notebook_pages(&[]).join("\n");
        assert!(p.contains("✓ 暂无生词"));
        assert!(p.contains("0 个生词 · 0 本书"));
    }

    #[test]
    fn render_phonetic_shown() {
        let mut e = mk("cachet", "书", "1");
        e.phonetic = "kæˈʃeɪ".into();
        let p = render_notebook_pages(&[e]).join("\n");
        assert!(p.contains("▸ cachet  kæˈʃeɪ"));
    }
}
