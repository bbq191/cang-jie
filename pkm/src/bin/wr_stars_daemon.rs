//! wr-stars-daemon —— ★ 全局待办 + PKM 汇总本 后台服务（**薄派发层**）。
//!
//! 事件驱动（inotify+防抖，复用 `fswatch`，空闲阻塞睡死零唤醒）→ 扫库找手绘星（`stardetect`）→
//!   ① 星→卡片：`starscan::scan_and_sync`（每本有星的书一份可编辑《总结卡片》，打字批注跨重建保留）
//!   ② 5 本自动只读汇总本：死链索引 `cardindex` / 高亮汇编 `cardagg` / 复盘队列 `cardreview` /
//!      阅读仪表 `cardstats` / 生词本 `vocabscan`（查字词，块4系统增强跨块）
//!   注入统一走 `notebook_sync`（/upload + pending-trash，xochitl 无视磁盘直写、只认原生导入）。
//!
//! **本 bin 只做派发**：读配置 + fswatch 循环 + `settle` 决定"扫谁、建哪些本"。逻辑全在库模块里
//! （host 可测）。省电：systemd 侧 CPUQuota=30% 硬帽。
//! 配置（reading-qol.json）：starTodoEnabled(默认关)/starTodoColor(默认 RED)/starTodoGap(默认 25)。
//! 环境：CANGJIE_FSWATCH_OBSERVE=1 = 只打日志不写。

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use pkm_device::notebook_sync::{collect_notebook_texts, is_user_notebook, sync_auto_notebook};
use pkm_device::stardetect::{scan_document_dir, scan_library, DocStars, StarConfig};
use pkm_device::{cardagg, cardindex, cardreview, cardstats, cardvocab, starscan, vocabscan};
use weread_device::fswatch;

const CFG_PATH_DEFAULT: &str = "/home/root/.local/share/cangjie-ime/reading-qol.json";
/// MOC 死链体检报告落点（SSH 可读文件）。
const CARD_INDEX_PATH: &str = "/home/root/weread/card-index.md";
// 5 本自动汇总本的固定 visibleName。必须从 collect_notebook_texts 排除（否则自我摄取 + 上传自触发无限重建）。
const INDEX_TITLE: &str = "🔗 卡片索引 · MOC 死链体检";
const AGG_TITLE: &str = "🎨 高亮汇编";
const REVIEW_TITLE: &str = "📖 复盘队列";
const STATS_TITLE: &str = "📊 阅读仪表";
const VOCAB_TITLE: &str = cardvocab::VOCAB_TITLE;
/// 本地词典（用户自备牛津英汉双解/现汉派生的排序 TSV，个人自用不入库）。缺文件 → 该向不查词。
const EN_DICT_PATH: &str = "/home/root/weread/dict/en.tsv";
const ZH_DICT_PATH: &str = "/home/root/weread/dict/zh.tsv";
/// 汇总本自身标题集：从 `collect_notebook_texts` 排除，防自摄取 + 上传自触发。
const AUTO_TITLES: [&str; 5] = [INDEX_TITLE, AGG_TITLE, REVIEW_TITLE, STATS_TITLE, VOCAB_TITLE];

fn xochitl_dir() -> String {
    std::env::var("CANGJIE_XOCHITL_DIR").unwrap_or_else(|_| weread_device::inject::XOCHITL_DIR.to_string())
}
fn cfg_path() -> String {
    std::env::var("CANGJIE_STARS_CFG").unwrap_or_else(|_| CFG_PATH_DEFAULT.to_string())
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

// ─────────── 5 本自动只读汇总本（薄接线：各自 build/render + notebook_sync 注入）───────────

/// 重建 MOC 死链索引：写 SSH 报告文件 + 同步库内「🔗 卡片索引」笔记本。observe=true 只打摘要不写。
fn rebuild_card_index(dir: &str, observe: bool) {
    let idx = cardindex::build_index(&collect_notebook_texts(dir, &AUTO_TITLES));
    let dead = cardindex::dead_links(&idx);
    if observe {
        println!("[stars] observe: 卡片索引 {} 锚点 · {} 软链接 · {} 死链", idx.defined.len(), idx.refs.len(), dead.len());
        return;
    }
    if let Err(e) = std::fs::write(CARD_INDEX_PATH, cardindex::render_report(&idx)) {
        println!("[stars] 写死链报告失败: {e}");
    }
    if let Some(m) = sync_auto_notebook(dir, INDEX_TITLE, &cardindex::render_notebook_pages(&idx)) {
        println!("[stars] 卡片索引笔记本已{m}（{} 锚点 · {} 死链）", idx.defined.len(), dead.len());
    } else if !dead.is_empty() {
        println!("[stars] 卡片索引：{} 锚点，⚠ {} 处死链（见 {CARD_INDEX_PATH}）", idx.defined.len(), dead.len());
    }
}

/// 重建「🎨 高亮汇编」：全库卡片本按 6 色横向聚合成 1 本 6 节。observe=true 只打摘要不写。
fn rebuild_card_agg(dir: &str, observe: bool) {
    let slots = cardagg::build_agg(&collect_notebook_texts(dir, &AUTO_TITLES));
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
    let items = cardreview::build_review(&collect_notebook_texts(dir, &AUTO_TITLES));
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
    let (books, totals) = cardstats::build_stats(&collect_notebook_texts(dir, &AUTO_TITLES));
    if observe {
        println!("[stars] observe: 阅读仪表 {} 本 · {} 星 · {} 高亮", totals.books, totals.stars, totals.highlights);
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, STATS_TITLE, &cardstats::render_notebook_pages(&books, &totals)) {
        println!("[stars] 阅读仪表笔记本已{m}（{} 本 · {} 星 · {} 高亮）", totals.books, totals.stars, totals.highlights);
    }
}

/// 重建《📕 生词本》（查字词=块4系统增强，编排全在 `vocabscan`；此处只触发+注入）。observe 只打条数。
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

/// 一次结算：扫库 → 星→卡片 + 汇总本。observe=true 只打日志。
/// `dirty`：None=全库扫（冷启动基线）；Some(uuid 集)=**只扫本轮变更的书**（增量，事件驱动省电的关键）。
/// 每本卡片彼此独立、无跨书全局态 → 只扫变更书语义等价于全扫；自己 /upload 卡片触发的事件只命中卡片本身
/// （starscan 内 suffix 跳过）→ 不自触发整库重扫。
fn settle(cfg: &Cfg, observe: bool, dirty: Option<&HashSet<String>>) {
    if !cfg.enabled {
        return;
    }
    let dir = xochitl_dir();
    let mut scfg = StarConfig::default();
    scfg.todo_colors = Some(vec![cfg.color.clone()]);
    let docs: Vec<DocStars> = match dirty {
        None => scan_library(Path::new(&dir), &scfg, cfg.gap),
        Some(set) => set.iter().filter_map(|u| scan_document_dir(u, Path::new(&dir), &scfg, cfg.gap)).collect(),
    };
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

    // ① 星→卡片
    starscan::scan_and_sync(&dir, &docs, observe);

    // ② MOC 死链/汇编/复盘/仪表：冷启动全建；增量时仅当有笔记本变更（卡片 /upload 后其 uuid 落 dirty、
    // 或用户改 MOC 本）才重建——源书画星本身不触发，其引发的卡片 upload 下轮命中，秒级延迟。
    let need_index = match dirty {
        None => true,
        Some(set) => set.iter().any(|u| is_user_notebook(&dir, u)),
    };
    if need_index {
        rebuild_card_index(&dir, observe);
        rebuild_card_agg(&dir, observe);
        rebuild_card_review(&dir, observe);
        rebuild_card_stats(&dir, observe);
    }

    // ③ 生词本：依赖**源书** .rm 的灰高亮（非笔记本文本），故独立触发——冷启动全建，
    // 增量时仅当 dirty 里有源书变更（画了灰词的书存盘）才重扫全库灰词。
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
