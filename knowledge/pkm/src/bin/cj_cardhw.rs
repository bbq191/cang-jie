//! 端化 cardhw · A1 编排：读卡片手写页 → **设备自己调云**转写 → 内联注入 → **/upload 重建**
//! （xochitl 直写不可见，必走 /upload；同 ★卡片重建路，`sync_auto_notebook`）。
//!
//! A1 阶段 = 手动触发一次性验证（不接 fswatch/通知，那是 A2/C）。API key 走环境变量。
//! 用法：cj-cardhw --book 人骨拼图 [--provider gemini] [--model X] [--apply]
//!       cj-cardhw --doc <uuid> --apply
//! 默认 dry-run 只报「手写→槽」方案；--apply 才走 /upload 重建（手写消化成文字）。

use device_core::inject;
use pkm_device::{cardhw, notebook_sync};

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let val = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let book = val("--book");
    let doc = val("--doc");
    let provider = val("--provider").unwrap_or_else(|| "deepseek".into());
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
    let folder = inject::find_folder_by_name(&dir, CARD_BOX_FOLDER).unwrap_or_default();

    // 定位卡片文档（重名逐个试，取含手写页的第一本）
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

    for d in &docs {
        eprintln!("-- 处理卡片 {}（设备调 {provider}）…", &d[..8.min(d.len())]);
        match cardhw::process_card_doc(&dir, d, &provider, model.as_deref(), &key, &folder, apply) {
            Ok(None) => continue, // 该本无手写页
            Ok(Some(o)) => {
                println!("《{}》页 {}：", o.visible_name, &o.page_id[..8.min(o.page_id.len())]);
                for a in &o.applied {
                    println!("  ✓ · {a}");
                }
                for lk in &o.leaked {
                    println!("  ⓘ 丢弃疑似打印泄漏：{}（≈已有书摘，vision 误读）", lk.note);
                }
                for u in &o.unmatched {
                    println!("  ✗ 未匹配：anchor={:?} ← {}", u.anchor, u.note);
                }
                match o.action {
                    Some(act) => println!("✅ 卡片已{act}并 /upload（{} 条注入，手写消化）。重开即见。", o.applied.len()),
                    None if !apply => println!("[dry-run] 未写。加 --apply 走 /upload 重建。"),
                    None => println!("（内容无变化或上传失败）"),
                }
                if o.usage.input_tokens > 0 || o.usage.output_tokens > 0 {
                    println!("   token：输入 {} · 输出 {}", o.usage.input_tokens, o.usage.output_tokens);
                }
                return;
            }
            Err(e) => {
                eprintln!("✗ {e}");
                std::process::exit(1);
            }
        }
    }
    eprintln!("!! 这些卡片里没有含手写的页——先在某个书摘旁手写再退出书库");
    std::process::exit(1);
}
