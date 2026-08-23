//! wr-stars —— 扫 xochitl 镜像目录 → ★ 全局待办清单（Markdown）。
//!
//! 移植自 pkm-semantic/proto/star_scan.py，与 Python 逐结果对拍。扫描逻辑在 stardetect::scan_library
//! （daemon 共用同一份）。只读，绝不回写设备。
//!
//! 用法: wr-stars <镜像目录> [--todo-color RED] [--cluster-gap 25] [--json out.json]

use std::path::PathBuf;

use pkm_device::stardetect::{render_todo_markdown, scan_library, DocStars, StarConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: wr-stars <镜像目录> [--todo-color RED] [--cluster-gap 25] [--json out.json]");
        std::process::exit(2);
    }
    let src = PathBuf::from(&args[1]);
    let mut cfg = StarConfig::default();
    let mut gap = 25.0f64;
    let mut json_out: Option<String> = None;
    let mut colors: Vec<String> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--todo-color" => {
                i += 1;
                colors.push(args[i].to_uppercase());
            }
            "--cluster-gap" => {
                i += 1;
                gap = args[i].parse().unwrap_or(25.0);
            }
            "--json" => {
                i += 1;
                json_out = Some(args[i].clone());
            }
            other => eprintln!("[warn] 忽略未知参数 {other}"),
        }
        i += 1;
    }
    if !colors.is_empty() {
        cfg.todo_colors = Some(colors.clone());
    } else {
        eprintln!("[警告] 未设 --todo-color：黑对黑纯几何，笔记页会误判草书汉字为星。生产请指定专用笔色。");
    }

    let docs: Vec<DocStars> = scan_library(&src, &cfg, gap);
    print!("{}", render_todo_markdown(&docs));

    if let Some(jp) = json_out {
        let arr: Vec<_> = docs
            .iter()
            .map(|d| {
                serde_json::json!({
                    "title": d.title,
                    "uuid": d.uuid,
                    "hits": d.hits.iter().map(|h| serde_json::json!({"page_index": h.page_index, "count": h.count})).collect::<Vec<_>>(),
                })
            })
            .collect();
        let _ = std::fs::write(jp, serde_json::to_string_pretty(&arr).unwrap());
    }

    let total: u32 = docs.iter().flat_map(|d| d.hits.iter().map(|h| h.count)).sum();
    let gate = cfg.todo_colors.as_ref().map(|c| format!("（仅 {} 笔色）", c.join("/"))).unwrap_or_default();
    eprintln!("✅ {total} 处 ★ · {} 本文档{gate}", docs.len());
}
