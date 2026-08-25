//! 离线探针：加载词典 TSV，对每个 argv 词/短语跑 `dict::lookup_phrase`，打印命中。
//! 验证真实词典的精确命中 + 短语拆分（中文词级最长匹配 / 英文按词跳停用词）。
//!
//! 用法：cargo run --release --example vocab_probe -- <en.tsv> <zh.tsv> "踌躇满志" "他很踌躇" "cachet"
//! （某部词典文件不存在 → 该向自动降级为不查，方便只测一部。）

use pkm_device::dict::{lookup_phrase, Dict};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("用法: vocab_probe <en.tsv> <zh.tsv> <词/短语>...");
        std::process::exit(2);
    }
    let en = Dict::open(&a[1]).ok();
    let zh = Dict::open(&a[2]).ok();
    println!("词典: en={} zh={}", en.is_some(), zh.is_some());
    for w in &a[3..] {
        println!("── {w}");
        let r = lookup_phrase(en.as_ref(), zh.as_ref(), w);
        if r.is_empty() {
            println!("  (无命中)");
        }
        for (word, e) in r {
            let phon = if e.phonetic.is_empty() { String::new() } else { format!("  {}", e.phonetic) };
            let body: String = e.body.chars().take(70).collect();
            println!("  ▸ {word}{phon}\n    {body}");
        }
    }
}
