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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pkm_device::cardsync::{parse_pages, render_card};
use pkm_device::stardetect::{scan_library, StarConfig};
use pkm_device::cardnote;
use weread_device::epubindex::{chapter_map, page_chapter, parse_sections};
use weread_device::{fswatch, inject};

type ChapterInfo = (Vec<(String, u32)>, HashMap<String, String>);
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
    let info: ChapterInfo = (parse_sections(&idx), chapter_map(&epub));
    cache.lock().unwrap().insert(uuid.to_string(), (mtime, info.clone()));
    Some(info)
}

const CFG_PATH_DEFAULT: &str = "/home/root/.local/share/cangjie-ime/reading-qol.json";
const UPLOAD_HOST: &str = "10.11.99.1";
const CARD_SUFFIX: &str = "- 总结卡片";
const PENDING_TRASH: &str = "/home/root/weread/pending-trash.json";
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

/// 同步一本书的卡片。stars=(1-based 页号, 章节标签[可空])。返回一句状态或 None（无改动/跳过）。
fn sync_one_card(dir: &str, book_title: &str, stars: &[(usize, String)], observe: bool) -> Option<String> {
    let visible = format!("《{book_title}》{CARD_SUFFIX}");
    let existing = find_docs_by_visible(dir, &visible);

    let (prev, cur_text) = match existing.first() {
        Some((u, _)) => {
            let pages = read_card_pages(dir, u);
            (parse_pages(&pages), pages.join("\n"))
        }
        None => (Default::default(), String::new()),
    };
    let new_pages = render_card(book_title, stars, &prev);
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
fn settle(cfg: &Cfg, observe: bool) {
    if !cfg.enabled {
        return;
    }
    let dir = xochitl_dir();
    let mut scfg = StarConfig::default();
    scfg.todo_colors = Some(vec![cfg.color.clone()]);
    let docs = scan_library(Path::new(&dir), &scfg, cfg.gap);

    let mut msgs = Vec::new();
    for d in &docs {
        // 卡片笔记本自身不当"源书"处理（防 《《X》-总结卡片》 套娃）。
        if d.title.ends_with(CARD_SUFFIX) {
            continue;
        }
        // EPUB 书取章节信息（页 0-based → 章名）；非 EPUB / 笔记本 → None，章名留空。
        let chap = chapter_info(&dir, &d.uuid);
        // 星页 → (1-based 显示页号, 章名标签)，按 0-based page_index 去重排序。
        let mut seen = std::collections::BTreeSet::new();
        let mut stars: Vec<(usize, String)> = Vec::new();
        for h in &d.hits {
            if h.page_index < 0 {
                continue;
            }
            let pi = h.page_index as usize;
            if !seen.insert(pi) {
                continue;
            }
            let label = chap.as_ref().and_then(|(secs, titles)| page_chapter(secs, titles, pi)).unwrap_or_default();
            stars.push((pi + 1, label));
        }
        stars.sort_by_key(|(p, _)| *p);
        if stars.is_empty() {
            continue;
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
}

fn main() {
    let observe = std::env::var("CANGJIE_FSWATCH_OBSERVE").ok().as_deref() == Some("1");
    println!("wr-stars-daemon 启动{}（B 模型：一书一份打字卡片）", if observe { "（observe 观察模式）" } else { "" });

    settle(&read_cfg(), observe);

    fswatch::watch_debounced(&xochitl_dir(), Duration::from_secs(8), false, |_changed| {
        settle(&read_cfg(), observe);
    });
}
