//! 离线：打印一页卡片 .rm 的 anchor 关联分桶（CardPage）。
//! 用法：cargo run --release --example dump_plan -- <path.rm>
use pkm_device::cardanchor::{associate, CardPage};

fn main() {
    let p = std::env::args().nth(1).unwrap();
    let rm = std::fs::read(&p).unwrap();
    let page = CardPage::parse(&rm);
    let lref: Vec<&str> = page.lines.iter().map(String::as_str).collect();
    for g in &page.groups {
        let a = associate(g, &page.full, &page.runs, &lref);
        println!(
            "组 node={:?} anchor={:?} mean_y={:.1} color={} → {:?}",
            g.node_id,
            g.anchor,
            g.mean_y(),
            g.color,
            a.map(|x| (x.anchor_line, x.line, page.lines[x.line].clone()))
        );
    }
    println!("--- buckets（桶数={}）---", page.bucket_count());
    for pl in &page.plans {
        println!("行{} {:?} ← 组{:?}", pl.line, page.lines[pl.line], pl.group_idx);
    }
}
