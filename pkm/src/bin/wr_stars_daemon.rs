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
//! 配置开关（reading-qol.json，设置页「笔记增强」写）：
//!   - starTodoEnabled(主·默认关) → 星→卡片；starTodoColor(默认 RED)/starTodoGap(默认 25) 为其参数。
//!     - cardHighlights(子·划线摘录·默认开)：高亮原文是否入卡片。
//!     - cardAggregates(子·跨书汇总·默认开)：是否建 4 本汇总本。
//!   - vocabEnabled(独立·单词笔记·默认关) → 生词本；脱离画星，与星代办互不依赖。
//!   功能关闭时只读自动本(4汇总+生词本)移回收站清残留；星卡片含批注绝不自动 trash。
//! 环境：CANGJIE_FSWATCH_OBSERVE=1 = 只打日志不写。

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use pkm_device::notebook_sync::{
    collect_notebook_texts, find_docs_by_visible, is_user_notebook, queue_trash, sync_auto_notebook,
};
use pkm_device::stardetect::{scan_document_dir, scan_library, DocStars, StarConfig};
use pkm_device::{cardagg, cardhw, cardindex, cardreview, cardstats, cardvocab, starscan, vocabscan};
use device_core::fswatch;

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
/// 单词笔记基线状态：存"开关打开时刻(ms)"，生词本只纳入 .rm mtime >= 该值的页（"开前灰词不补"）。停用时删。
const VOCAB_SINCE_PATH: &str = "/home/root/weread/vocab-since.txt";
/// 汇总本自身标题集：从 `collect_notebook_texts` 排除，防自摄取 + 上传自触发。
const AUTO_TITLES: [&str; 5] = [INDEX_TITLE, AGG_TITLE, REVIEW_TITLE, STATS_TITLE, VOCAB_TITLE];
/// 卡片盒文件夹名：所有 daemon 生成物（卡片+4汇总+生词本）自动归档进这个文件夹（按名认领 uuid，
/// 上传前 GET /documents/<uuid> 设当前文件夹）。文件夹不存在 → uuid 空 → 兜底落 root。
const CARD_BOX_FOLDER: &str = "zettelkasten";

fn xochitl_dir() -> String {
    std::env::var("CANGJIE_XOCHITL_DIR").unwrap_or_else(|_| device_core::inject::XOCHITL_DIR.to_string())
}
fn cfg_path() -> String {
    std::env::var("CANGJIE_STARS_CFG").unwrap_or_else(|_| CFG_PATH_DEFAULT.to_string())
}

/// 三个能力的门控（reading-qol.json，设置页「笔记增强」写）：
/// - `star_todo`（主，默认关）：红星→《总结卡片》。关则不再生成卡片（已存卡片含批注、绝不自动 trash）。
///   - `card_highlights`（子·划线摘录，默认开）：高亮原文是否收进卡片页。仅 star_todo 开时有意义。
///   - `card_aggregates`（子·跨书汇总，默认开）：是否生成 4 本汇总本。仅 star_todo 开时有意义。
/// - `vocab`（独立·单词笔记，默认关）：灰词查词→《📕生词本》。脱离画星，与 star_todo 互不依赖。
struct Cfg {
    star_todo: bool,
    card_highlights: bool,
    card_aggregates: bool,
    vocab: bool,
    color: String,
    gap: f64,
    // 端化手写识别（块⑥，独立能力）：卡片手写批注→设备调云转写→内联注入→/upload。默认关。
    cardhw_enabled: bool,
    cardhw_provider: String,
    cardhw_model: Option<String>,
}

/// 端化 cardhw 的 API key（明文，Phase B 设置面板写；与 reading-qol.json 同目录）。读不到→None。
const CARDHW_KEY_PATH: &str = "/home/root/.local/share/cangjie-ime/cardhw.key";
fn cardhw_key() -> Option<String> {
    std::fs::read_to_string(CARDHW_KEY_PATH).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 该文档是否是**在库的**「总结卡片」（cardhw 只处理这类，普通手写笔记本不碰）。
/// **必须排除 trash**：处理后 queue_trash 旧手写卡 → trash-agent 改其 metadata=trash 又是个写事件，
/// 若不排除，step④ 会拿"已消化但尚未删的旧卡 .rm（笔划仍在）"重复转写 → 模型输出微变 → sync 总认为
/// 有改动 → 无限 /upload 自循环（2026-08-29 真机踩过）。故 parent=trash 一律跳过。
fn is_active_summary_card(dir: &str, uuid: &str) -> bool {
    std::fs::read_to_string(format!("{dir}/{uuid}.metadata"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .map(|v| {
            v["visibleName"].as_str().map(|s| s.contains("总结卡片")).unwrap_or(false)
                && v["parent"].as_str() != Some("trash")
        })
        .unwrap_or(false)
}

fn read_cfg() -> Cfg {
    let v: serde_json::Value = std::fs::read_to_string(cfg_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);
    let b = |k: &str, dflt: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(dflt);
    Cfg {
        star_todo: b("starTodoEnabled", false),
        card_highlights: b("cardHighlights", false), // 子开关默认关：星代办单开=空白模板，需再开「划线摘录/荧光笔」才摄取（规则a）
        card_aggregates: b("cardAggregates", true),
        vocab: b("vocabEnabled", false), // 独立能力显式 opt-in（且需词典文件，缺则本就降级）
        color: v.get("starTodoColor").and_then(|x| x.as_str()).unwrap_or("RED").to_uppercase(),
        gap: v.get("starTodoGap").and_then(|x| x.as_f64()).unwrap_or(25.0),
        cardhw_enabled: b("cardhwEnabled", false),
        cardhw_provider: v.get("cardhwProvider").and_then(|x| x.as_str()).unwrap_or("gemini").to_string(),
        cardhw_model: v.get("cardhwModel").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()),
    }
}

/// 配置文件 reading-qol.json 的最后修改时间（ms）≈ 用户最近一次改开关的时刻。读不到→0。
fn config_mtime_ms() -> u64 {
    std::fs::metadata(cfg_path())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 单词笔记基线时间（ms）：生词本只纳入 .rm mtime >= 该值的页 → "开前画的灰词不补"。
/// 已有状态文件 → 用它（跨重启稳定）。首次激活：**若已有生词本**（说明之前就在用）→ 基线 0
/// grandfather 全保留；否则基线=配置 mtime（≈开关打开时刻，避开 fswatch 8s 防抖竞态）。停用时调用方删状态文件。
fn vocab_since(dir: &str) -> u64 {
    if let Ok(s) = std::fs::read_to_string(VOCAB_SINCE_PATH) {
        if let Ok(v) = s.trim().parse::<u64>() {
            return v;
        }
    }
    let has_existing = !find_docs_by_visible(dir, VOCAB_TITLE).is_empty();
    let baseline = if has_existing { 0 } else { config_mtime_ms() };
    let _ = std::fs::write(VOCAB_SINCE_PATH, baseline.to_string());
    baseline
}

/// 把某只读自动本（4 汇总/生词本之一）的所有在库副本移回收站——对应开关关闭时清残留。
/// 幂等：已 trash → find 返回空 → 不重复入队。返回本轮入队数（observe 打印用）。
fn trash_auto_notebook(dir: &str, title: &str) -> usize {
    let existing = find_docs_by_visible(dir, title);
    for (u, _) in &existing {
        queue_trash(u);
    }
    existing.len()
}

// ─────────── 5 本自动只读汇总本（薄接线：各自 build/render + notebook_sync 注入）───────────

/// 重建 MOC 死链索引：写 SSH 报告文件 + 同步库内「🔗 卡片索引」笔记本。observe=true 只打摘要不写。
/// `texts`=本轮已采集一次的全库笔记本文本（4 本汇总本共用一份，避免各自重复整库解析）。
fn rebuild_card_index(dir: &str, texts: &[(String, String)], folder: &str, observe: bool) {
    let idx = cardindex::build_index(texts);
    let dead = cardindex::dead_links(&idx);
    if observe {
        println!("[stars] observe: 卡片索引 {} 锚点 · {} 软链接 · {} 死链", idx.defined.len(), idx.refs.len(), dead.len());
        return;
    }
    if let Err(e) = std::fs::write(CARD_INDEX_PATH, cardindex::render_report(&idx)) {
        println!("[stars] 写死链报告失败: {e}");
    }
    if let Some(m) = sync_auto_notebook(dir, INDEX_TITLE, &cardindex::render_notebook_pages(&idx), folder) {
        println!("[stars] 卡片索引笔记本已{m}（{} 锚点 · {} 死链）", idx.defined.len(), dead.len());
    } else if !dead.is_empty() {
        println!("[stars] 卡片索引：{} 锚点，⚠ {} 处死链（见 {CARD_INDEX_PATH}）", idx.defined.len(), dead.len());
    }
}

/// 重建「🎨 高亮汇编」：全库卡片本按 6 色横向聚合成 1 本 6 节。observe=true 只打摘要不写。
fn rebuild_card_agg(dir: &str, texts: &[(String, String)], folder: &str, observe: bool) {
    let slots = cardagg::build_agg(texts);
    let total: usize = slots.iter().map(|v| v.len()).sum();
    if observe {
        let per: Vec<String> = (0..6).map(|s| format!("{}{}", cardagg::SECTION_NAME[s], slots[s].len())).collect();
        println!("[stars] observe: 高亮汇编 {total} 条（{}）", per.join(" "));
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, AGG_TITLE, &cardagg::render_notebook_pages(&slots), folder) {
        println!("[stars] 高亮汇编笔记本已{m}（{total} 条高亮）");
    }
}

/// 重建「📖 复盘队列」：全库卡片中「有摘录、无提炼」的卡（渐进总结提醒）。observe=true 只打摘要不写。
fn rebuild_card_review(dir: &str, texts: &[(String, String)], folder: &str, observe: bool) {
    let items = cardreview::build_review(texts);
    if observe {
        println!("[stars] observe: 复盘队列 {} 张待消化卡", items.len());
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, REVIEW_TITLE, &cardreview::render_notebook_pages(&items), folder) {
        println!("[stars] 复盘队列笔记本已{m}（{} 张待消化）", items.len());
    }
}

/// 重建「📊 阅读仪表」：全库按书统计星/高亮/待消化。observe=true 只打摘要不写。
fn rebuild_card_stats(dir: &str, texts: &[(String, String)], folder: &str, observe: bool) {
    let (books, totals) = cardstats::build_stats(texts);
    if observe {
        println!("[stars] observe: 阅读仪表 {} 本 · {} 星 · {} 高亮", totals.books, totals.stars, totals.highlights);
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, STATS_TITLE, &cardstats::render_notebook_pages(&books, &totals), folder) {
        println!("[stars] 阅读仪表笔记本已{m}（{} 本 · {} 星 · {} 高亮）", totals.books, totals.stars, totals.highlights);
    }
}

/// 重建《📕 生词本》（查字词=块4系统增强，编排全在 `vocabscan`；此处只触发+注入）。observe 只打条数。
fn rebuild_vocab(dir: &str, folder: &str, since_ms: u64, observe: bool) {
    let entries = vocabscan::collect(dir, EN_DICT_PATH, ZH_DICT_PATH, since_ms);
    if observe {
        println!("[stars] observe: 生词本 {} 个生词", entries.len());
        return;
    }
    if let Some(m) = sync_auto_notebook(dir, VOCAB_TITLE, &cardvocab::render_notebook_pages(&entries), folder) {
        println!("[stars] 生词本已{m}（{} 个生词）", entries.len());
    }
}

/// 一次结算：按三个开关分别门控 星→卡片 / 跨书汇总 / 生词本。observe=true 只打日志。
/// `dirty`：None=全库扫（冷启动基线）；Some(uuid 集)=**只扫本轮变更的书**（增量，事件驱动省电的关键）。
/// 每本卡片彼此独立、无跨书全局态 → 只扫变更书语义等价于全扫；自己 /upload 卡片触发的事件只命中卡片本身
/// （starscan 内 suffix 跳过）→ 不自触发整库重扫。
///
/// 门控与清理：功能关闭时——只读自动本（4 汇总 + 生词本）移回收站清残留（无用户数据，安全）；
/// 星卡片含打字批注 → **绝不自动 trash**，仅停止再生成。主与独立能力全关且非冷启动 → 空转返回。
fn settle(cfg: &Cfg, observe: bool, dirty: Option<&HashSet<String>>) {
    let dir = xochitl_dir();

    // 主(星代办)与独立(单词笔记/手写识别)全关：稳态直接返回（daemon 睡死零唤醒）；冷启动多走一遍清理落残留。
    if !cfg.star_todo && !cfg.vocab && !cfg.cardhw_enabled {
        if dirty.is_none() {
            for t in AUTO_TITLES {
                let n = trash_auto_notebook(&dir, t);
                if observe && n > 0 {
                    println!("[stars] observe: 「{t}」功能已关，{n} 本旧本入回收队列");
                }
            }
        }
        return;
    }

    // 归档目标：所有生成物（卡片+4汇总+生词本）落 zettelkasten 卡片盒。按名认领 uuid，各 sync 上传前
    // GET /documents/<uuid> 设当前文件夹（全局态，真机验证）。文件夹不存在→空串→兜底落 root。
    let card_folder = device_core::inject::find_folder_by_name(&dir, CARD_BOX_FOLDER).unwrap_or_default();
    if observe && card_folder.is_empty() {
        println!("[stars] observe: 未找到「{CARD_BOX_FOLDER}」文件夹，生成物将落 root（建个同名文件夹即自动归档）");
    }

    // ① 星→卡片（划线摘录门控高亮是否入卡）
    if cfg.star_todo {
        let mut scfg = StarConfig::default();
        scfg.todo_colors = Some(vec![cfg.color.clone()]);
        let docs: Vec<DocStars> = match dirty {
            None => scan_library(Path::new(&dir), &scfg, cfg.gap),
            Some(set) => set.iter().filter_map(|u| scan_document_dir(u, Path::new(&dir), &scfg, cfg.gap)).collect(),
        };
        if observe {
            match dirty {
                None => println!("[stars] observe: 冷启动全库扫 → {} 本有星（划线摘录{}）", docs.len(), if cfg.card_highlights { "开" } else { "关" }),
                Some(set) => println!(
                    "[stars] observe: 增量扫 {} 本变更书 → 其中 {} 本有星（变更集 {:?}）",
                    set.len(), docs.len(), set.iter().take(8).collect::<Vec<_>>()
                ),
            }
        }
        starscan::scan_and_sync(&dir, &docs, &card_folder, cfg.card_highlights, observe);
    }

    // ② 跨书汇总（MOC 死链/汇编/复盘/仪表 4 本）：仅 星代办 && 跨书汇总 都开。冷启动全建；增量时仅当有
    // 笔记本变更（卡片 /upload 后其 uuid 落 dirty、或用户改 MOC 本）才重建——源书画星本身不触发，秒级延迟。
    if cfg.star_todo && cfg.card_aggregates {
        let need_index = match dirty {
            None => true,
            Some(set) => set.iter().any(|u| is_user_notebook(&dir, u)),
        };
        if need_index {
            // 全库笔记本文本只采集一次，4 本汇总本共用（原先各自 collect 一遍=整库解析 4 遍）。
            let texts = collect_notebook_texts(&dir, &AUTO_TITLES);
            rebuild_card_index(&dir, &texts, &card_folder, observe);
            rebuild_card_agg(&dir, &texts, &card_folder, observe);
            rebuild_card_review(&dir, &texts, &card_folder, observe);
            rebuild_card_stats(&dir, &texts, &card_folder, observe);
        }
    } else {
        // 跨书汇总关（或星代办关）→ 清 4 本残留（幂等，已 trash 则 no-op）。
        for t in [INDEX_TITLE, AGG_TITLE, REVIEW_TITLE, STATS_TITLE] {
            let n = trash_auto_notebook(&dir, t);
            if observe && n > 0 {
                println!("[stars] observe: 跨书汇总关，「{t}」{n} 本入回收队列");
            }
        }
    }

    // ③ 生词本（单词笔记，独立）：依赖**源书** .rm 的灰高亮（非笔记本文本），故独立触发——冷启动全建，
    // 增量时仅当 dirty 里有源书变更（画了灰词的书存盘）才重扫全库灰词。关则清残留。
    if cfg.vocab {
        let since = vocab_since(&dir); // 基线：开前画的灰词不补（首次激活据是否已有生词本决定 grandfather）
        let need_vocab = match dirty {
            None => true,
            Some(set) => set.iter().any(|u| vocabscan::is_source_book(&dir, u)),
        };
        if need_vocab {
            rebuild_vocab(&dir, &card_folder, since, observe);
        }
    } else {
        let _ = std::fs::remove_file(VOCAB_SINCE_PATH); // 停用→删基线，下次重新激活时重设
        let n = trash_auto_notebook(&dir, VOCAB_TITLE);
        if observe && n > 0 {
            println!("[stars] observe: 单词笔记关，生词本 {n} 本入回收队列");
        }
    }

    // ④ 端化手写识别（块⑥，独立能力）：卡片手写批注 → 设备调云转写 → 内联注入 → /upload 重建。
    // **仅事件驱动**（dirty=Some）：冷启动不自动跑，避免开机批量调云/计费；只处理「总结卡片」类文档。
    // 自触发安全：处理后卡片笔划=0，下轮事件 find_handwritten_page=None → no-op。
    // 网络/识别失败**只记不崩、不阻塞画星主流程**（网络归网络）。observe 模式跳过（不真调云）。
    if cfg.cardhw_enabled && !observe {
        if let Some(set) = dirty {
            match cardhw_key() {
                None => println!("[cardhw] 已开但缺 key（{CARDHW_KEY_PATH}），跳过——设置面板填 key 后生效"),
                Some(key) => {
                    let mut done = load_cardhw_done(); // 已处理过的手写页 .rm md5（幂等+防自循环）
                    for u in set.iter().filter(|u| is_active_summary_card(&dir, u)) {
                        // 先拿手写页 .rm 的 md5：这份手写已处理过 → 跳（旧卡消化后 /upload 新卡，旧卡被 trash
                        // 前仍 active，aggregate 自触发会反复回到这里；靠内容哈希幂等根治自循环，不依赖 trash 时序）。
                        let page = match cardhw::find_handwritten_page(&dir, u) {
                            Some(p) => p,
                            None => continue,
                        };
                        let rm = std::fs::read(format!("{dir}/{u}/{}.rm", page.1)).unwrap_or_default();
                        let hash = format!("{:x}", md5::compute(&rm));
                        if done.contains(&hash) {
                            continue;
                        }
                        match cardhw::process_card_doc(&dir, u, &cfg.cardhw_provider, cfg.cardhw_model.as_deref(), &key, &card_folder, true) {
                            Ok(Some(o)) if o.action.is_some() => {
                                done.insert(hash.clone());
                                append_cardhw_done(&hash);
                                println!("[cardhw] 《{}》注入 {} 条并 /upload（手写消化）", o.visible_name, o.applied.len());
                            }
                            Ok(_) => {} // 无手写/全泄漏/未匹配：不记 done（下次内容变了仍可处理）
                            Err(e) => eprintln!("[cardhw] 卡片 {} 处理失败（不阻塞）：{e}", &u[..8.min(u.len())]),
                        }
                    }
                }
            }
        }
    }
}

/// 已处理手写页 .rm 的 md5 集（幂等/防自循环）。文件小、每轮读一次即可。
const CARDHW_DONE_PATH: &str = "/home/root/weread/cardhw-done.txt";
fn load_cardhw_done() -> HashSet<String> {
    std::fs::read_to_string(CARDHW_DONE_PATH)
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}
fn append_cardhw_done(hash: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(CARDHW_DONE_PATH) {
        let _ = writeln!(f, "{hash}");
    }
}

fn main() {
    let observe = std::env::var("CANGJIE_FSWATCH_OBSERVE").ok().as_deref() == Some("1");
    println!("wr-stars-daemon 启动{}（B 模型：一书一份打字卡片）", if observe { "（observe 观察模式）" } else { "" });

    // 冷启动：全库扫一遍建基线（此时无变更集可依）。
    settle(&read_cfg(), observe, None);

    // 稳态：只扫本轮变更的书（fswatch 已把变更 doc uuid 集交到手上），不再无差别整库重扫。
    // 例外：设置页改开关只写 config、不动文档树 —— 附加监听 config（CONFIG_SIGNAL），见到即全库重扫，
    // 否则拨开关后（尤其对存量已画星的书）daemon 永远不重扫、看着"开关没用"。
    fswatch::watch_debounced(&xochitl_dir(), Some(&cfg_path()), Duration::from_secs(8), false, |changed| {
        if changed.contains(fswatch::CONFIG_SIGNAL) {
            // 开关变更 → 现读新配置、全库重扫（dirty=None），存量书一并回扫。
            settle(&read_cfg(), observe, None);
        } else {
            settle(&read_cfg(), observe, Some(changed));
        }
    });
}
