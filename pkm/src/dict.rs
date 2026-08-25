//! 本地词典 mmap 只读查词。数据 = 离线预处理的**排序 TSV**（`build_dict.py` 从用户自备的
//! 牛津英汉双解 / 现代汉语词典 MOBI 抽出），一行一词条：
//!
//! ```text
//! headword \t 音标/拼音 \t 主释义(body) \t 补充(extra) \n
//! ```
//!
//! 词典数据是用户正版商业词典派生物，**只个人自用、不入库、不分发**（见白皮书查词节 / 计划）。
//! 本模块只提供 mmap + **行首二分查找**：RAM 与词典大小无关（不把整表读进 HashMap），
//! 适配设备内存受限。测试用自造 fixture，不含任何版权词典内容。
//!
//! 排序约定：按 headword 的 **UTF-8 字节序**升序（= Unicode 码位序；Python `sorted()` 一致）。
//! 查词前 key 需按语种规整（`normalize_key`），与建表侧一致。

use crate::locate::canon;
use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;

/// 词条载荷（payload 三段）。空段渲染时略过。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub phonetic: String, // 音标（英）/ 拼音（中）
    pub body: String,     // 主释义（牛津英汉双解正文 / 现汉中文释义）
    pub extra: String,    // 补充（当前留空，备后用）
}

/// 词的语种归属 → 查哪部词典。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

/// 判词属中/英：`canon` 规整后含 CJK(U+4E00..=U+9FFF) → Zh；否则含 ASCII 字母 → En；都不含 → None。
/// 先 canon 是为了让部首区/Kangxi 码位也算作 CJK（否则它们落在 U+2E80..U+2FDF 判不出中文）。
pub fn detect_lang(word: &str) -> Option<Lang> {
    let w = canon(word);
    if w.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
        Some(Lang::Zh)
    } else if w.chars().any(|c| c.is_ascii_alphabetic()) {
        Some(Lang::En)
    } else {
        None
    }
}

/// 查词前把词规整成词典 key：
/// - En：`trim` → 剥两端非字母数字（引号/句读/破折号等）→ 小写。
/// - Zh：`canon` 规整部首区 → 去所有空白（EPUB 渲染偶插空格/软连字）→ trim。
pub fn normalize_key(word: &str, lang: Lang) -> String {
    match lang {
        Lang::En => word
            .trim()
            .trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .to_lowercase(),
        Lang::Zh => canon(word).chars().filter(|c| !c.is_whitespace()).collect(),
    }
}

/// 一部 mmap 词典。
pub struct Dict {
    map: Mmap,
}

impl Dict {
    /// mmap 打开排序 TSV。文件缺失/空 → Err（调用方降级为"该向不查词"）。
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Dict> {
        let file = File::open(path)?;
        // SAFETY: 词典是部署时一次写入的只读文件，运行期不被改写。
        let map = unsafe { Mmap::map(&file)? };
        Ok(Dict { map })
    }

    /// 行首二分查 key（须已 `normalize_key`）。命中返回 Entry，未命中 None。
    pub fn lookup(&self, key: &str) -> Option<Entry> {
        let d: &[u8] = &self.map;
        if d.is_empty() {
            return None;
        }
        let target = key.as_bytes();
        let (mut lo, mut hi) = (0usize, d.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            // 含 mid 的行 [ls, le)：向前找上一个 '\n'（rposition 从 mid 往回，遇首个即停 = 一行长），向后找下一个 '\n'。
            let ls = match d[..mid].iter().rposition(|&b| b == b'\n') {
                Some(p) => p + 1,
                None => 0,
            };
            let le = match d[mid..].iter().position(|&b| b == b'\n') {
                Some(p) => mid + p,
                None => d.len(),
            };
            let line = &d[ls..le];
            let ktab = line.iter().position(|&b| b == b'\t').unwrap_or(line.len());
            let line_key = &line[..ktab];
            match line_key.cmp(target) {
                std::cmp::Ordering::Equal => return Some(parse_payload(&line[ktab..])),
                std::cmp::Ordering::Less => lo = le + 1, // 目标在本行之后
                std::cmp::Ordering::Greater => hi = ls,  // 目标在本行之前
            }
        }
        None
    }
}

// ───── 短语/句子查词：整体精确优先，miss 才拆（中文词级最长匹配、英文按词）─────

const ZH_MAX_WORD: usize = 8; // 中文最长匹配上限（成语 4，个别更长）
const PHRASE_MAX_ZH: usize = 12; // 超此长度 = 句子，不拆（划整句更像"金句"非查词）
const PHRASE_MAX_EN_TOKENS: usize = 5; // 英文超此 token 数 = 句子，不拆
const PER_HL_CAP: usize = 5; // 一段高亮最多产出的词条数（防刷屏）
/// 英文虚词/极常见词停用表：短语拆分时跳过（用户要查的是实词生词）。
const EN_STOP: &[&str] = &[
    "the", "a", "an", "of", "to", "and", "in", "on", "is", "it", "its", "was", "were", "be", "am",
    "are", "for", "with", "as", "at", "by", "or", "so", "if", "no", "not", "that", "this", "these",
    "those", "he", "she", "they", "his", "her", "him", "them", "their", "i", "you", "we", "us",
    "but", "then", "than", "from", "up", "out", "into", "over", "under", "about", "which", "who",
    "whom", "had", "has", "have", "do", "did", "does", "will", "would", "can", "could", "s",
];

/// 一段灰高亮 → 词条列表（(展示词, Entry)）。**整体精确命中就返回整条**（单词/成语/短语在词典里）；
/// miss 时拆分：英文按词（跳停用词）、中文词级最长匹配（只取 ≥2 字的词，单字连接词天然被跳过）；
/// **整句不拆**（长度门控）。查不到全丢弃（人物名/专名/纯常用词短语 → 空）。上限 `PER_HL_CAP` 防刷屏。
pub fn lookup_phrase(en: Option<&Dict>, zh: Option<&Dict>, text: &str) -> Vec<(String, Entry)> {
    let lang = match detect_lang(text) {
        Some(l) => l,
        None => return Vec::new(),
    };
    let d = match lang {
        Lang::En => en,
        Lang::Zh => zh,
    };
    let d = match d {
        Some(x) => x,
        None => return Vec::new(),
    };
    // 1) 整体精确（单词 / 成语 / 词典里的短语）
    let key = normalize_key(text, lang);
    if !key.is_empty() {
        if let Some(e) = d.lookup(&key) {
            return vec![(text.trim().to_string(), e)];
        }
    }
    // 2) miss → 拆分
    match lang {
        Lang::En => segment_en(d, text),
        Lang::Zh => segment_zh(d, &key),
    }
}

/// 英文短语按词拆：非字母数字/连字符切 token，跳停用词，逐词查，命中收（去重、封顶）。整句不拆。
fn segment_en(d: &Dict, text: &str) -> Vec<(String, Entry)> {
    let toks: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
        .collect();
    if toks.is_empty() || toks.len() > PHRASE_MAX_EN_TOKENS {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in toks {
        let k = normalize_key(t, Lang::En);
        if k.is_empty() || EN_STOP.contains(&k.as_str()) {
            continue;
        }
        if let Some(e) = d.lookup(&k) {
            if seen.insert(k.clone()) {
                out.push((k, e));
                if out.len() >= PER_HL_CAP {
                    break;
                }
            }
        }
    }
    out
}

/// 中文词级最长匹配：逐位置试 [8..=2] 字子串查词典，命中取最长、跳过其长度；**只取 ≥2 字的词**
/// （单字连接词天然被跳过，避免把"他/很/的"刷进生词本）。整句（>PHRASE_MAX_ZH 字）不拆。key 须已 normalize。
fn segment_zh(d: &Dict, key: &str) -> Vec<(String, Entry)> {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 2 || chars.len() > PHRASE_MAX_ZH {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        let mut matched = 0;
        let maxl = ZH_MAX_WORD.min(chars.len() - i);
        let mut l = maxl;
        while l >= 2 {
            let w: String = chars[i..i + l].iter().collect();
            if let Some(e) = d.lookup(&w) {
                if seen.insert(w.clone()) {
                    out.push((w, e));
                }
                matched = l;
                break;
            }
            l -= 1;
        }
        i += if matched > 0 { matched } else { 1 };
        if out.len() >= PER_HL_CAP {
            break;
        }
    }
    out
}

/// 解析 `\t phon \t body \t extra`（payload，含前导 '\t'；缺段容错、多余段并入 extra 的尾部忽略）。
fn parse_payload(rest: &[u8]) -> Entry {
    // rest 以 '\t' 开头（key 后那个）；split 出 ["", phon, body, extra...]。
    let s = String::from_utf8_lossy(rest);
    let mut it = s.split('\t');
    it.next(); // 前导空段（key 与 phon 之间的 '\t'）
    let phonetic = unescape(it.next().unwrap_or(""));
    let body = unescape(it.next().unwrap_or(""));
    let extra = unescape(it.next().unwrap_or(""));
    Entry { phonetic, body, extra }
}

/// 反转义建表侧对字段内 `\t`/`\n`/`\\` 的转义（`\\t`→TAB、`\\n`→LF、`\\\\`→`\\`）。
fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // 造一个排序 TSV fixture（不含任何版权词典内容），返回临时路径。
    fn fixture(name: &str, lines: &[&str]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("pkm_dict_test_{name}.tsv"));
        let mut f = File::create(&p).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        p
    }

    #[test]
    fn detect_lang_zh_en_none() {
        assert_eq!(detect_lang("apple"), Some(Lang::En));
        assert_eq!(detect_lang("苹果"), Some(Lang::Zh));
        assert_eq!(detect_lang("中Amix"), Some(Lang::Zh)); // 含 CJK 优先判中
        assert_eq!(detect_lang("\u{2f52}"), Some(Lang::Zh)); // 部首区「氏」canon 后算中
        assert_eq!(detect_lang("123"), None);
        assert_eq!(detect_lang("——"), None);
    }

    #[test]
    fn normalize_en_strips_and_lowers() {
        assert_eq!(normalize_key("  “Cachet.” ", Lang::En), "cachet");
        assert_eq!(normalize_key("Well-known", Lang::En), "well-known"); // 内部连字保留
        assert_eq!(normalize_key("RUNNING", Lang::En), "running");
    }

    #[test]
    fn normalize_zh_removes_space_and_canon() {
        assert_eq!(normalize_key("踌 躇", Lang::Zh), "踌躇");
        assert_eq!(normalize_key("姓\u{2f52}", Lang::Zh), "姓氏"); // 部首区规整
    }

    #[test]
    fn binary_search_hit_miss_edges() {
        // 按 UTF-8 字节序升序（ASCII 小写词）。
        let p = fixture(
            "en",
            &[
                "apple\tˈæpl\tn. a round fruit\t",
                "banana\tbəˈnɑːnə\tn. a long curved fruit\t",
                "cachet\tkæˈʃeɪ\tn. prestige; 声望\t",
                "zebra\tˈzebrə\tn. a striped animal\t",
            ],
        );
        let d = Dict::open(&p).unwrap();
        // 首条 / 末条 / 中间条都命中
        assert_eq!(d.lookup("apple").unwrap().phonetic, "ˈæpl");
        assert_eq!(d.lookup("zebra").unwrap().body, "n. a striped animal");
        let c = d.lookup("cachet").unwrap();
        assert_eq!(c.phonetic, "kæˈʃeɪ");
        assert_eq!(c.body, "n. prestige; 声望");
        // 未命中：不存在 / 排在最前 / 排在最后
        assert!(d.lookup("aardvark").is_none());
        assert!(d.lookup("mango").is_none());
        assert!(d.lookup("zzz").is_none());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn payload_missing_and_escaped_segments() {
        // 须按 headword 码位序：含(U+542B) < 空(U+7A7A) < 缺(U+7F3A)。
        let p = fixture(
            "zh",
            &[
                "含制表\thán\t释义里有\\t制表符和\\n换行\t备注",
                "空补充\tkōng\t只有释义没补充\t",
                "缺段\t\t\t", // 全空段
            ],
        );
        let d = Dict::open(&p).unwrap();
        let e = d.lookup("空补充").unwrap();
        assert_eq!(e.body, "只有释义没补充");
        assert_eq!(e.extra, "");
        let e2 = d.lookup("含制表").unwrap();
        assert_eq!(e2.body, "释义里有\t制表符和\n换行"); // 反转义还原
        assert_eq!(e2.extra, "备注");
        let e3 = d.lookup("缺段").unwrap();
        assert_eq!(e3, Entry { phonetic: String::new(), body: String::new(), extra: String::new() });
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn empty_file_returns_none() {
        let p = fixture("empty", &[]);
        let d = Dict::open(&p).unwrap();
        assert!(d.lookup("anything").is_none());
        std::fs::remove_file(&p).ok();
    }

    // 短语/句子查词 fixture：两部小词典（自造，无版权内容）。
    fn phrase_dicts() -> (Dict, Dict) {
        // en 按 ASCII 序
        let pe = fixture(
            "phrase_en",
            &[
                "cachet\tkæˈʃeɪ\tn. prestige\t",
                "carried\t\tv. carry 过去式\t",
                "certain\tˈsɜːtn\tadj. 某些\t",
                "the\tðə\tart. 定冠词\t",
            ],
        );
        // zh 按码位序：他(U+4ED6) < 踌躇(U+8DCC…) < 踌躇满志
        let pz = fixture(
            "phrase_zh",
            &[
                "他\ttā\t代词\t",
                "踌躇\tchóuchú\t犹豫\t",
                "踌躇满志\tchóuchú-mǎnzhì\t形容对现状很得意\t",
            ],
        );
        (Dict::open(&pe).unwrap(), Dict::open(&pz).unwrap())
    }

    #[test]
    fn phrase_exact_idiom_hit() {
        let (en, zh) = phrase_dicts();
        let r = lookup_phrase(Some(&en), Some(&zh), "踌躇满志");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "踌躇满志"); // 整体精确命中，不拆
    }

    #[test]
    fn phrase_zh_segments_to_word_not_char() {
        let (en, zh) = phrase_dicts();
        // "他很踌躇"：很不在词典、他是单字（拆分只取≥2字词）→ 只应拆出「踌躇」
        let r = lookup_phrase(Some(&en), Some(&zh), "他很踌躇");
        let words: Vec<&str> = r.iter().map(|(w, _)| w.as_str()).collect();
        assert_eq!(words, vec!["踌躇"], "词级最长匹配，不拆单字、不含常用单字他");
    }

    #[test]
    fn phrase_en_skips_stopwords() {
        let (en, zh) = phrase_dicts();
        let r = lookup_phrase(Some(&en), Some(&zh), "the cachet of certain");
        let mut words: Vec<String> = r.iter().map(|(w, _)| w.clone()).collect();
        words.sort();
        assert_eq!(words, vec!["cachet".to_string(), "certain".to_string()], "跳过 the/of 虚词");
    }

    #[test]
    fn phrase_sentence_not_split() {
        let (en, zh) = phrase_dicts();
        // 英文 >5 token = 句子 → 不拆（即便含 cachet）
        assert!(lookup_phrase(Some(&en), Some(&zh), "it carried a certain cachet among us").is_empty());
        // 中文 >12 字 = 句子 → 不拆（即便含踌躇满志）
        let long = "他一直很踌躇满志地走来走去思考人生真谛啊";
        assert!(long.chars().count() > 12);
        assert!(lookup_phrase(Some(&en), Some(&zh), long).is_empty());
    }
}
