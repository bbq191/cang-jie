//! 端化 cardhw · A1 编排：读卡片手写页 → **设备自己调云**转写 → 内联注入 → **/upload 重建**
//! （xochitl 直写不可见，必走 /upload；同 ★卡片重建路，`sync_auto_notebook`）。
//!
//! A1 阶段 = 手动触发一次性验证（不接 fswatch/通知，那是 A2/C）。API key 走环境变量。
//! 用法：wr-cardhw --book 人骨拼图 [--provider gemini] [--model X] [--apply]
//!       wr-cardhw --doc <uuid> --apply
//! 默认 dry-run 只报「手写→槽」方案；--apply 才走 /upload 重建（手写消化成文字）。

use device_core::inject;
use pkm_device::{cardhw, cardnote, notebook_sync, stardetect};

const CARD_BOX_FOLDER: &str = "zettelkasten";

fn xochitl_dir() -> String {
    std::env::var("CANGJIE_XOCHITL_DIR").unwrap_or_else(|_| inject::XOCHITL_DIR.to_string())
}

fn key_for(provider: &str) -> Option<String> {
    let envs: &[&str] = match provider {
        "gemini" => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        "deepseek" => &["DEEPSEEK_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "anthropic" => &["ANTHROPIC_API_KEY"],
        _ => &[],
    };
    envs.iter().find_map(|e| std::env::var(e).ok().filter(|v| !v.is_empty()))
}

fn visible_name(dir: &str, uuid: &str) -> String {
    std::fs::read_to_string(format!("{dir}/{uuid}.metadata"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["visibleName"].as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let val = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let book = val("--book");
    let doc = val("--doc");
    let provider = val("--provider").unwrap_or_else(|| "gemini".into());
    let model = val("--model");
    let apply = args.iter().any(|a| a == "--apply");
    if book.is_none() && doc.is_none() {
        eprintln!("需 --book <书名> 或 --doc <uuid>");
        std::process::exit(2);
    }

    let dir = xochitl_dir();
    let key = match key_for(&provider) {
        Some(k) => k,
        None => {
            eprintln!("!! 缺 {provider} 的 API key（设对应环境变量）");
            std::process::exit(1);
        }
    };

    // 1. 定位卡片文档（重名按"含手写页"消歧）
    let docs: Vec<String> = match &doc {
        Some(d) => vec![d.clone()],
        None => {
            let title = format!("《{}》- 总结卡片", book.as_ref().unwrap());
            notebook_sync::find_docs_by_visible(&dir, &title).into_iter().map(|(u, _)| u).collect()
        }
    };
    if docs.is_empty() {
        eprintln!("!! 找不到该书的总结卡片");
        std::process::exit(1);
    }

    // 2. 找含手写的页（SceneLine 笔划 > 0）
    let mut hit: Option<(String, usize, String, Vec<String>, String)> = None;
    for d in &docs {
        let pages = cardnote::list_pages(&dir, d).unwrap_or_default(); // (idx, page_id, text)
        let texts: Vec<String> = pages.iter().map(|(_, _, t)| t.clone()).collect();
        for (pi, (_, page_id, _)) in pages.iter().enumerate() {
            let rm = std::fs::read(format!("{dir}/{d}/{page_id}.rm")).unwrap_or_default();
            let strokes = stardetect::read_strokes(&rm).map(|s| s.len()).unwrap_or(0);
            if strokes > 0 {
                hit = Some((d.clone(), pi, page_id.clone(), texts, visible_name(&dir, d)));
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let (doc_u, page_idx, page_id, mut texts, vn) = match hit {
        Some(h) => h,
        None => {
            eprintln!("!! 这些卡片里没有含手写的页——先在某个书摘旁手写再退出书库");
            std::process::exit(1);
        }
    };
    if docs.len() > 1 {
        eprintln!("⚠ {} 本重名总结卡，处理含手写的 {}（sync 会合并同名、trash 其余）", docs.len(), &doc_u[..8]);
    }

    // 3. 读缩略图 → 设备调云转写
    let thumb_path = format!("{dir}/{doc_u}.thumbnails/{page_id}.png");
    let thumb = match std::fs::read(&thumb_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("!! 读缩略图失败 {thumb_path}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("-- {vn} 页 {}：设备调 {provider} 识别…", &page_id[..8]);
    let anns = match cardhw::transcribe_card(&thumb, &provider, model.as_deref(), &key) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("✗ 识别失败：{e}");
            std::process::exit(1);
        }
    };
    if anns.is_empty() {
        eprintln!("· vision 没认出手写批注");
        std::process::exit(0);
    }

    // 4. 内联注入
    let r = cardhw::inject_inline(&texts[page_idx], &anns);
    println!("关联方案（拼接后条目）：");
    for a in &r.applied {
        println!("  ✓ · {a}");
    }
    for lk in &r.leaked {
        println!("  ⓘ 丢弃疑似打印泄漏：{}（≈已有书摘，vision 误读）", lk.note);
    }
    for u in &r.unmatched {
        println!("  ✗ 未匹配：anchor={:?} ← {}", u.anchor, u.note);
    }
    if r.applied.is_empty() {
        std::process::exit(0);
    }

    if !apply {
        println!("[dry-run] 未写。确认无误加 --apply 走 /upload 重建（手写消化成文字）。");
        std::process::exit(0);
    }

    // 5. 走 /upload 重建（sync_auto_notebook：找同名→pack→/upload→trash 旧）
    texts[page_idx] = r.text;
    let folder = inject::find_folder_by_name(&dir, CARD_BOX_FOLDER).unwrap_or_default();
    match notebook_sync::sync_auto_notebook(&dir, &vn, &texts, &folder) {
        Some(action) => println!("✅ 卡片已{action}并 /upload（{} 条批注注入，手写消化成文字）。重开笔记本即见。", r.applied.len()),
        None => println!("（内容无变化或上传失败）"),
    }
}
