//! 预览：读一页 .rm，提取荧光笔高亮，按 6 色槽渲染成卡片（假设该页有星）。
use pkm_device::cardhl::{extract_highlights, SLOT_COUNT};
use pkm_device::cardsync::{render_card_starspecs, CardModel, CardTemplate};
fn main() {
    let path = std::env::args().nth(1).expect("用法: card_preview <page.rm> [模板]");
    let tmpl = std::env::args().nth(2).unwrap_or_else(|| "原文+悬疑".into());
    let bytes = std::fs::read(&path).expect("读文件");
    let mut slots: Vec<Vec<String>> = vec![Vec::new(); SLOT_COUNT];
    for h in extract_highlights(&bytes).expect("extract") {
        slots[h.slot].push(h.text);
    }
    let tmpls = match tmpl.as_str() {
        "原文" => vec![CardTemplate::Original],
        "悬疑" => vec![CardTemplate::Mystery],
        "通用" => vec![CardTemplate::General],
        _ => vec![CardTemplate::Original, CardTemplate::Mystery],
    };
    let stars = vec![(10usize, "第一章".to_string(), tmpls, slots)];
    let pages = render_card_starspecs("13·67", &stars, &CardModel::default());
    println!("{}", pages.join("\n"));
}
