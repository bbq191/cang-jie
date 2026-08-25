//! 离线预览：读真实卡片本 .rm root text → cardagg 汇编。args: "书名::目录" 多个。
use pkm_device::cardagg::{build_agg, render_notebook_pages};
use weread_device::notebook_rm::read_root_text;
fn main() {
    let mut docs = Vec::new();
    for arg in std::env::args().skip(1) {
        let (title, dir) = arg.split_once("::").expect("书名::目录");
        let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().flatten()
            .map(|e| e.path()).filter(|p| p.extension().map(|x| x == "rm").unwrap_or(false)).collect();
        files.sort();
        let mut text = String::new();
        for f in files { text.push_str(&read_root_text(&std::fs::read(&f).unwrap())); text.push('\n'); }
        docs.push((format!("《{title}》- 总结卡片"), text));
    }
    let slots = build_agg(&docs);
    for p in render_notebook_pages(&slots) { println!("{p}"); }
}
