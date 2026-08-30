//! 临时：打印 plan_injections 的分桶结果。
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let rm = std::fs::read(&p).unwrap();
    let full = device_core::notebook_rm::read_root_text(&rm);
    let runs = device_core::notebook_rm::read_root_text_runs(&rm);
    let lines: Vec<&str> = full.split('\n').collect();
    let (groups, plans) = pkm_device::cardanchor::plan_injections(&rm, &full, &runs, &lines);
    for g in &groups {
        let a = pkm_device::cardanchor::associate(g, &full, &runs, &lines);
        println!(
            "组 node={:?} anchor={:?} mean_y={:.1} color={} → {:?}",
            g.node_id,
            g.anchor,
            g.mean_y(),
            g.color,
            a.map(|x| (x.anchor_line, x.line, lines[x.line]))
        );
    }
    println!("--- buckets ---");
    for pl in &plans {
        println!("行{} {:?} ← 组{:?}", pl.line, lines[pl.line], pl.group_idx);
    }
}
