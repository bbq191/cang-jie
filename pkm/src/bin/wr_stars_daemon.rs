//! wr-stars-daemon —— ★ 全局待办后台服务（PKM 语义引擎首个能力，B 模型：一书一份打字卡片）。
//!
//! 事件驱动（inotify+防抖，复用 fswatch）→ 扫全库手绘星（专用笔色+自相交，复用 stardetect）→
//! 每本有星的书维护一份可编辑「《书名》- 总结卡片」笔记本：
//!   读回卡片现有打字（read_root_text）→ parse_card 拆出用户批注/自由区 →
//!   render_card 按当前星重生成（老星保批注、新星空批注、消失星批注移自由区不丢）→
//!   pack_rmdoc → /upload 活注入新本 + trash 旧本。
//!
//! 为什么走 /upload 而非直写：**xochitl 维护全内存文档模型、运行时无视磁盘直写（重启才可见）**；
//! /upload 是它自己的原生导入、即时进库（10.11.99.1:80，本机走本地路由可达、无需真插 USB）。
//! 代价=每次换 uuid（reMarkable 本就无跨文件链，无损）+ 必 trash 旧同名本。
//! 用户输入靠"读回打字→重建保留"存活 → **只认 Text 工具打字，手写笔迹读不回不保**。
//!
//! 省电：空闲阻塞睡死、零周期唤醒；systemd 侧 CPUQuota=30% 硬帽。
//! 配置（reading-qol.json）：starTodoEnabled(默认关)/starTodoColor(默认 RED)/starTodoGap(默认 25)。
//! 环境：CANGJIE_FSWATCH_OBSERVE=1 = 只打日志不写。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pkm_device::cardagg;
use pkm_device::cardreview;
use pkm_device::cardstats;
use pkm_device::cardhl;
use pkm_device::cardvocab;
use pkm_device::vocabscan;
use pkm_device::cardsync::{parse_pages, render_card_starspecs, CardTemplate};
use pkm_device::stardetect::{scan_document_dir, scan_library, DocStars, StarConfig};
use pkm_device::{cardindex, cardnote};
use weread_device::epubindex::{chapter_map_hier, page_chapter_label, parse_sections};
use weread_device::{fswatch, inject};

type ChapterInfo = (Vec<(String, u32)>, HashMap<String, (String, Option<String>)>);
#[allow(clippy::type_complexity)]
static CHAP_CACHE: OnceLock<Mutex<HashMap<String, (u64, ChapterInfo)>>> = OnceLock::new();

/// 取一本 EPUB 的章节信息（(spine起始页表, basename→章名)），按 .epub mtime 缓存避免每轮重读。
/// 非 EPUB / 缺 .epubindex → None（章名留空，只显示页号）。
fn chapter_info(dir: &str, uuid: &str) -> Option<ChapterInfo> {
    let epub_path = format!("{dir}/{uuid}.epub");
    let idx_path = format!("{dir}/{uuid}.epubindex");
    let em = std::fs::metadata(&epub_path).ok()?;
    if !Path::new(&idx_path).exists() {
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

const CFG_PATH_DEFAULT: &str = "/home/root/.local/share/cangjie-ime/reading-qol.json";
const UPLOAD_HOST: &str = "10.11.99.1";
const CARD_SUFFIX: &str = "- 总结卡片";
const PENDING_TRASH: &str = "/home/root/weread/pending-trash.json";
/// MOC 死链体检报告落点（SSH 可读文件）。
const CARD_INDEX_PATH: &str = "/home/root/weread/card-index.md";
/// 库内「🔗 卡片索引」笔记本的固定 visibleName（设备端可视化）。必须被 collect_notebook_texts 排除
/// （否则自我摄取 + 上传→自触发无限重建），且正文不含 [ID:]/触发词双保险（见 cardindex）。
const INDEX_TITLE: &str = "🔗 卡片索引 · MOC 死链体检";
const AGG_TITLE: &str = "🎨 高亮汇编";
const REVIEW_TITLE: &str = "📖 复盘队列";
const STATS_TITLE: &str = "📊 阅读仪表";
/// 生词本（⚪灰色荧光笔划词→查本地词典）。同样须被 collect_notebook_texts 排除（防自摄取/自触发）。
const VOCAB_TITLE: &str = cardvocab::VOCAB_TITLE;
/// 本地词典（用户自备牛津英汉双解/现汉派生的排序 TSV，个人自用不入库）。缺文件 → 该向不查词。
const EN_DICT_PATH: &str = "/home/root/weread/dict/en.tsv";
const ZH_DICT_PATH: &str = "/home/root/weread/dict/zh.tsv";
/// 卡片最近被编辑（用户可能正在打字）→ 本轮跳过重建、下次再合，避免读到半刷入的 .rm。
const GUARD_SECS: u64 = 20;

fn xochitl_dir() -> String {
    std::env::var("CANGJIE_XOCHITL_DIR").unwrap_or_else(|_| inject::XOCHITL_DIR.to_string())
}
fn cfg_path() -> String {
    std::env::var("CANGJIE_STARS_CFG").unwrap_or_else(|_| CFG_PATH_DEFAULT.to_string())
}
fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

struct Cfg {
    enabled: bool,
    color: String,
    gap: f64,
}

fn read_cfg() -> Cfg {
    let v: serde_json::Value = std::fs::read_to_string(cfg_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);
    Cfg {
        enabled: v.get("starTodoEnabled").and_then(|x| x.as_bool()).unwrap_or(false),
        color: v.get("starTodoColor").and_then(|x| x.as_str()).unwrap_or("RED").to_uppercase(),
        gap: v.get("starTodoGap").and_then(|x| x.as_f64()).unwrap_or(25.0),
    }
}

/// 按 visibleName 精确匹配、在库（parent != trash）的文档，(uuid, lastModified) 降序（最新在前）。
fn find_docs_by_visible(dir: &str, visible: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
                continue;
            }
            let v: serde_json::Value = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(serde_json::Value::Null);
            if v.get("visibleName").and_then(|x| x.as_str()) != Some(visible) {
                continue;
            }
            if v.get("parent").and_then(|x| x.as_str()) == Some("trash") {
                continue;
            }
            let uuid = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let lm = v.get("lastModified").and_then(|x| x.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            out.push((uuid, lm));
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

/// 读一本笔记本各页 RootText（cPages 顺序）。
fn read_card_pages(dir: &str, uuid: &str) -> Vec<String> {
    cardnote::list_pages(dir, uuid).map(|rows| rows.into_iter().map(|(_, _, t)| t).collect()).unwrap_or_default()
}

/// 一个文档是否是"用户笔记本"（含卡片本 + MOC 本）：metadata type=DocumentType 且 content fileType=notebook。
/// 用于判断本轮 dirty 是否需要重建死链索引（epub/文件夹不算）。
fn is_user_notebook(dir: &str, uuid: &str) -> bool {
    let meta: serde_json::Value = std::fs::read_to_string(format!("{dir}/{uuid}.metadata"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);
    if meta.get("type").and_then(|x| x.as_str()) != Some("DocumentType") {
        return false;
    }
    let content: serde_json::Value = std::fs::read_to_string(format!("{dir}/{uuid}.content"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);
    content.get("fileType").and_then(|x| x.as_str()) == Some("notebook")
}

/// 扫全库笔记本文本 → (笔记本名, 全页文本 join)。**只读文本、不碰笔迹几何**（cardindex 死链体检用）。
/// 只收 fileType=notebook（卡片本 + 用户 MOC 本），跳过 epub（无 [ID:]、天然轻）与回收站。
fn collect_notebook_texts(dir: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("metadata") {
                continue;
            }
            let uuid = match p.file_stem().and_then(|s| s.to_str()) {
                Some(u) => u.to_string(),
                None => continue,
            };
            let meta: serde_json::Value = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(serde_json::Value::Null);
            if meta.get("type").and_then(|x| x.as_str()) != Some("DocumentType") {
                continue;
            }
            if meta.get("parent").and_then(|x| x.as_str()) == Some("trash") {
                continue;
            }
            if !is_user_notebook(dir, &uuid) {
                continue;
            }
            let title = meta.get("visibleName").and_then(|x| x.as_str()).unwrap_or("").to_string();
            // 索引本 / 汇编本 / 生词本自身排除：否则回喂（自指）+ 上传后自触发无限重建。
            if title == INDEX_TITLE || title == AGG_TITLE || title == REVIEW_TITLE || title == STATS_TITLE || title == VOCAB_TITLE {
                continue;
            }
            let text = read_card_pages(dir, &uuid).join("\n");
            out.push((title, text));
        }
    }
    out
}

/// 把 daemon 自动生成的只读本（卡片索引 / 高亮汇编）同步成库内笔记本（复用卡片的 pack_rmdoc+upload+trash 链）。
/// 内容未变（对最新本逐字相同）→ 绝不上传（防 churn + 防自触发循环）；多余旧本入 trash 队列。
fn sync_auto_notebook(dir: &str, title: &str, pages: &[String]) -> Option<String> {
    let existing = find_docs_by_visible(dir, title);
    let new_text = pages.join("\n");
    if let Some((u, _)) = existing.first() {
        let cur = read_card_pages(dir, u).join("\n");
        if cur == new_text {
            // 内容一致：把多余同名旧本入队列，最新那本留着，本轮不上传。
            for (u2, _) in existing.iter().skip(1) {
                queue_trash(u2);
            }
            return None;
        }
    }
    let new_uuid = uuid::Uuid::new_v4().to_string();
    let refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
    let rmdoc = cardnote::pack_rmdoc(&new_uuid, title, &refs).ok()?;
    let net = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    if inject::upload_document(&net, UPLOAD_HOST, &rmdoc, &format!("{title}.rmdoc"), "application/zip").is_err() {
        return None;
    }
    for (u, _) in &existing {
        queue_trash(u);
    }
    Some(if existing.is_empty() { "创建" } else { "更新" }.to_string())
}

/// 重建 MOC 死链索引：写 SSH 报告文件 + 同步库内「🔗 卡片索引」笔记本。observe=true 只打摘要不写。
fn rebuild_card_index(dir: &str, observe: bool) {
    let idx = cardindex::build_index(&collect_notebook_texts(dir));
    let dead = cardindex::dead_links(&idx);
    if observe {
        println!("[stars] observe: 卡片索引 {} 锚点 · {} 软链接 · {} 死链", idx.defined.len(), idx.refs.len(), dead.len());
        return;
    }
    let report = cardindex::render_report(&idx);
    if let Err(e) = std::fs::write(CARD_INDEX_PATH, &report) {
        println!("[stars] 写死链报告失败: {e}");
    }
    // 库内可视化笔记本（内容未变则静默跳过）。
    if let Some(m) = sync_auto_notebook(dir, INDEX_TITLE, &cardindex::render_notebook_pages(&idx)) {
        println!("[stars] 卡片索引笔记本已{m}（{} 锚点 · {} 死链）", idx.defined.len(), dead.len());
    } else if !dead.is_empty() {
        println!("[stars] 卡片索引：{} 锚点，⚠ {} 处死链（见 {CARD_INDEX_PATH}）", idx.defined.len(), dead.len());
    }
}

/// 重建「🎨 高亮汇编」：全库卡片本按 6 色横向聚合成 1 本 6 节。observe=true 只打摘要不写。
fn rebuild_card_agg(dir: &str, observe: bool) {
    let slots = cardagg::build_agg(&collect_notebook_texts(dir));
    let total: usize = slots.iter().map(|v| v.len()).sum();
    if observe {
        let per: Vec<String> = (0..6).map(|s| format!("{}{}", cardagg::SECTION_NAME[s], slots[s].len())).collect();
        println!("[stars] observe: 高亮汇编 {total} 条（{}）", per.join(" "));
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, AGG_TITLE, &cardagg::render_notebook_pages(&slots)) {
        println!("[stars] 高亮汇编笔记本已{m}（{total} 条高亮）");
    }
}

/// 重建「📖 复盘队列」：全库卡片中「有摘录、无提炼」的卡（渐进总结提醒）。observe=true 只打摘要不写。
fn rebuild_card_review(dir: &str, observe: bool) {
    let items = cardreview::build_review(&collect_notebook_texts(dir));
    if observe {
        println!("[stars] observe: 复盘队列 {} 张待消化卡", items.len());
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, REVIEW_TITLE, &cardreview::render_notebook_pages(&items)) {
        println!("[stars] 复盘队列笔记本已{m}（{} 张待消化）", items.len());
    }
}

/// 重建「📊 阅读仪表」：全库按书统计星/高亮/待消化。observe=true 只打摘要不写。
fn rebuild_card_stats(dir: &str, observe: bool) {
    let (books, totals) = cardstats::build_stats(&collect_notebook_texts(dir));
    if observe {
        println!("[stars] observe: 阅读仪表 {} 本 · {} 星 · {} 高亮", totals.books, totals.stars, totals.highlights);
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, STATS_TITLE, &cardstats::render_notebook_pages(&books, &totals)) {
        println!("[stars] 阅读仪表笔记本已{m}（{} 本 · {} 星 · {} 高亮）", totals.books, totals.stars, totals.highlights);
    }
}

/// 文档目录最近改动时间（秒，取页目录 mtime）——判断用户是否刚在编辑。
fn doc_age_secs(dir: &str, uuid: &str) -> u64 {
    let p = format!("{dir}/{uuid}");
    std::fs::metadata(&p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| {
            let now = now_ms() / 1000;
            now.saturating_sub(d.as_secs() as u128) as u64
        })
        .unwrap_or(u64::MAX)
}

/// 把旧卡 uuid 推进 pending-trash 队列，由 trash-agent.qmd 走 xochitl 原生
/// selectionMoveToTrash 代码路移回收站（**内存即时生效、无形无感**）。
/// ⚠ 绝不直接改磁盘 metadata parent=trash——xochitl 内存不认（界面残留旧卡）、
/// 且会让 wr-serve 误判已完成而提前出队（trash-agent 拿不到 → 永不清）。
fn queue_trash(uuid: &str) {
    let mut q: Vec<String> = std::fs::read_to_string(PENDING_TRASH)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if !q.iter().any(|u| u == uuid) {
        q.push(uuid.to_string());
        let _ = std::fs::write(PENDING_TRASH, serde_json::to_string(&q).unwrap_or_else(|_| "[]".into()));
    }
}

/// 读一本书的原生标签：文档级 `tags[].name` + 页级 `pageTags`(pageId → [name])。都在 `.content`。
/// 结构 2026-08-24 真机摸清：`tags`=`[{name,timestamp}]`、`pageTags`=`[{name,pageId,timestamp}]`。
fn read_book_tags(dir: &str, uuid: &str) -> (Vec<String>, HashMap<String, Vec<String>>) {
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
/// 文档级=全书镜片(如整本 #原文)，页级=该页镜片(如某页 #悬疑)，对该页的星合并。
fn templates_for(doc_tags: &[String], page_tags: &[String]) -> Vec<CardTemplate> {
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
fn page_highlights(dir: &str, doc_uuid: &str, page_uuid: &str) -> Vec<Vec<String>> {
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

/// 重建《📕 生词本》汇总本（跨块=块4系统增强，编排全在 `vocabscan`；daemon 只触发+注入）。
/// observe 只打条数。词典路径从常量传入 vocabscan（模块本身路径无关）。
fn rebuild_vocab(dir: &str, observe: bool) {
    let entries = vocabscan::collect(dir, EN_DICT_PATH, ZH_DICT_PATH);
    if observe {
        println!("[stars] observe: 生词本 {} 个生词", entries.len());
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, VOCAB_TITLE, &cardvocab::render_notebook_pages(&entries)) {
        println!("[stars] 生词本已{m}（{} 个生词）", entries.len());
    }
}

/// 同步一本书的卡片。stars=(1-based 页号, 章节标签[可空], 该星模板集[空=默认], 该页按槽高亮)。返回一句状态或 None。
fn sync_one_card(dir: &str, book_title: &str, stars: &[(usize, String, Vec<CardTemplate>, Vec<Vec<String>>)], observe: bool) -> Option<String> {
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
    // 但若有多余同名旧卡（trash-agent 还没清完）→ 把多余的进队列，最新那张留着。
    let content_same = !existing.is_empty() && new_text == cur_text;
    if content_same {
        if existing.len() > 1 {
            for (u, _) in existing.iter().skip(1) {
                queue_trash(u);
            }
            return Some(format!("《{book_title}》无改动，去重 {} 张旧卡入队列", existing.len() - 1));
        }
        return None; // 恰好 1 张且无改动 → 什么都不做
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
    // 上传成功 → 旧同名本进 pending-trash 队列（trash-agent 走原生代码路移回收站）。
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

/// 一次结算：扫库 → 每本有星的书同步卡片。observe=true 只打日志。
/// `dirty`：None=全库扫（冷启动基线）；Some(uuid 集)=**只扫本轮变更的书**（增量，事件驱动省电的关键）。
/// 每本卡片彼此独立、无跨书全局态 → 只扫变更书语义等价于全扫，且我们自己 /upload 卡片触发的
/// 事件只会命中卡片本身（下方 suffix 跳过）→ 不再自触发整库重扫。
/// 边界：某书清空全部星时，增量扫该书返回无星（不产 DocStars）→ 旧卡不会被 trash —— 此为
/// **既存行为**（全扫同样如此，`scan_library` 无星即不产 DocStars），非本次增量引入的回归。
fn settle(cfg: &Cfg, observe: bool, dirty: Option<&HashSet<String>>) {
    if !cfg.enabled {
        return;
    }
    let dir = xochitl_dir();
    let mut scfg = StarConfig::default();
    scfg.todo_colors = Some(vec![cfg.color.clone()]);
    let docs: Vec<DocStars> = match dirty {
        None => scan_library(Path::new(&dir), &scfg, cfg.gap),
        Some(set) => set
            .iter()
            .filter_map(|uuid| scan_document_dir(uuid, Path::new(&dir), &scfg, cfg.gap))
            .collect(),
    };
    // observe 对账诊断：直观显示本轮扫描范围（全库 vs 增量几本），生产模式不打。
    if observe {
        match dirty {
            None => println!("[stars] observe: 冷启动全库扫 → {} 本有星", docs.len()),
            Some(set) => println!(
                "[stars] observe: 增量扫 {} 本变更书 → 其中 {} 本有星（变更集 {:?}）",
                set.len(),
                docs.len(),
                set.iter().take(8).collect::<Vec<_>>()
            ),
        }
    }

    let mut msgs = Vec::new();
    for d in &docs {
        // 卡片笔记本自身不当"源书"处理（防 《《X》-总结卡片》 套娃）。
        if d.title.ends_with(CARD_SUFFIX) {
            continue;
        }
        // EPUB 书取章节信息（页 0-based → 章名）；非 EPUB / 笔记本 → None，章名留空。
        let chap = chapter_info(&dir, &d.uuid);
        // 原生标签（文档级 + 页级）→ 每星按其所在页 pageId 合并出模板集。
        let (doc_tags, page_tags_map) = read_book_tags(&dir, &d.uuid);
        // 星页 → (1-based 显示页号, 章名标签, 该星模板集)，按 0-based page_index 去重排序。
        let mut seen = std::collections::BTreeSet::new();
        let mut stars: Vec<(usize, String, Vec<CardTemplate>, Vec<Vec<String>>)> = Vec::new();
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
            let hl = page_highlights(&dir, &d.uuid, &h.page_uuid); // 该星页荧光笔高亮，按 6 色槽分组
            stars.push((pi + 1, label, tmpls, hl));
        }
        stars.sort_by_key(|(p, _, _, _)| *p);
        if stars.is_empty() {
            continue;
        }
        // observe 对账：打印本书文档级标签 + 每颗星解析到的模板集（验证 read_book_tags/templates_for）。
        if observe && (!doc_tags.is_empty() || stars.iter().any(|(_, _, t, _)| !t.is_empty())) {
            let per: Vec<String> = stars.iter().map(|(p, _, t, _)| format!("p{p}={t:?}")).collect();
            println!("[stars] observe:《{}》docTags={:?} 每星模板 {}", d.title, doc_tags, per.join(" "));
        }
        if let Some(m) = sync_one_card(&dir, &d.title, &stars, observe) {
            msgs.push(m);
        }
    }
    if !msgs.is_empty() {
        println!("[stars] {}", msgs.join(" · "));
    } else if observe {
        println!("[stars] observe: 无有星的书需要同步");
    }

    // MOC 死链体检：冷启动全建；增量时仅当有笔记本变更（卡片 /upload 后其 uuid 落 dirty、
    // 或用户改 MOC 本）才重建——源书画星本身不触发，但引发的卡片 upload 会在下轮命中，秒级延迟。
    // 只读笔记本文本、跳过 epub 笔迹，不吐回增量扫的省电成果。
    let need_index = match dirty {
        None => true,
        Some(set) => set.iter().any(|u| is_user_notebook(&dir, u)),
    };
    if need_index {
        rebuild_card_index(&dir, observe);
        rebuild_card_agg(&dir, observe); // 跨书按色聚合，与死链索引同触发条件
        rebuild_card_review(&dir, observe); // 渐进总结复盘队列，同触发条件
        rebuild_card_stats(&dir, observe); // 按书 PKM 仪表，同触发条件
    }

    // 生词本：依赖**源书** .rm 的灰高亮（非笔记本文本），故独立触发——冷启动全建，
    // 增量时仅当 dirty 里有源书变更（画了灰词的书存盘）才重扫全库灰词。与死链索引触发条件不同。
    let need_vocab = match dirty {
        None => true,
        Some(set) => set.iter().any(|u| vocabscan::is_source_book(&dir, u)),
    };
    if need_vocab {
        rebuild_vocab(&dir, observe);
    }
}

fn main() {
    let observe = std::env::var("CANGJIE_FSWATCH_OBSERVE").ok().as_deref() == Some("1");
    println!("wr-stars-daemon 启动{}（B 模型：一书一份打字卡片）", if observe { "（observe 观察模式）" } else { "" });

    // 冷启动：全库扫一遍建基线（此时无变更集可依）。
    settle(&read_cfg(), observe, None);

    // 稳态：只扫本轮变更的书（fswatch 已把变更 doc uuid 集交到手上），不再无差别整库重扫。
    fswatch::watch_debounced(&xochitl_dir(), Duration::from_secs(8), false, |changed| {
        settle(&read_cfg(), observe, Some(changed));
    });
}
