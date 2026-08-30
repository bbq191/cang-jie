//! 只读探针（throwaway，验证锚点重构布局）：dump 一页 .rm 的
//!   ① RootText 文本项（item_id CrdtId + run + char 长度）
//!   ② TreeNode 组（node_id / anchor_id / anchor_origin_x）
//!   ③ SceneGroupItem 组嵌套（parent → child node）
//!   ④ SceneLineItem 笔画（parent_id / color / y-range / 点数）
//! 用法：cargo run --release --example dump_anchor -- <path.rm>

use remarkable_lines::v6::block::Block;
use remarkable_lines::v6::crdt::CrdtId;
use remarkable_lines::v6::scene_item::text::TextItem;
use remarkable_lines::RemarkableFile;

fn cid(c: &CrdtId) -> String {
    format!("({},{})", c.part1, c.part2)
}

fn main() {
    let p = std::env::args().nth(1).expect("用法: dump_anchor <path.rm>");
    let bytes = std::fs::read(&p).expect("读 .rm");

    // 参考：file-order 全文（notebook_rm 己方读取器）
    let full = device_core::notebook_rm::read_root_text(&bytes);
    println!("=== RootText (file-order 全文) ===\n{full}\n");

    let rf = RemarkableFile::read(bytes.as_slice()).expect("解析 .rm");
    let blocks = match rf {
        RemarkableFile::V6 { blocks, .. } => blocks,
        _ => panic!("非 v6"),
    };

    println!("=== ① RootText items（按 part2 排序）===");
    for b in &blocks {
        if let Block::RootText(rtb) = b {
            let mut items: Vec<_> = rtb.text.items.items.values().collect();
            items.sort_by_key(|it| (it.item_id.part1, it.item_id.part2));
            for it in items {
                match &it.value {
                    TextItem::Text(s) => println!(
                        "  id={:<9} len={:<3} left={:<8} right={:<8} del={} text={:?}",
                        cid(&it.item_id),
                        s.chars().count(),
                        cid(&it.left_id),
                        cid(&it.right_id),
                        it.deleted_length,
                        s
                    ),
                    TextItem::FormatCode(f) => println!(
                        "  id={:<9} FORMAT({f}) left={:<8} right={:<8} del={}",
                        cid(&it.item_id),
                        cid(&it.left_id),
                        cid(&it.right_id),
                        it.deleted_length
                    ),
                }
            }
            println!(
                "  [text block origin x={} y={} width={}]",
                rtb.text.x, rtb.text.y, rtb.text.width
            );
        }
    }

    println!("\n=== ② TreeNode 组（node_id / anchor）===");
    for b in &blocks {
        if let Block::TreeNode(tn) = b {
            let g = &tn.group;
            let anchor = g
                .anchor_id
                .as_ref()
                .map(|l| cid(&l.value))
                .unwrap_or_else(|| "-".into());
            let ox = g
                .anchor_origin_x
                .as_ref()
                .map(|l| l.value.to_string())
                .unwrap_or_else(|| "-".into());
            println!(
                "  node={:<9} anchor_id={:<10} origin_x={:<10} label={:?} vis={}",
                cid(&g.node_id),
                anchor,
                ox,
                g.label.value,
                g.visible.value
            );
        }
    }

    // ── 锚点 → 字符偏移 → 行 解析（核心验证）──
    // 卡片文本是单 run id=(1,16)：char i 的 CrdtId=(1,16+i) ⇒ offset=anchor.part2-16。
    println!("\n=== ★ anchor → offset → 行 解析 ===");
    let chars: Vec<char> = full.chars().collect();
    // 每个字符所属行号（\n 归上一行末尾）
    let mut line_of: Vec<usize> = Vec::with_capacity(chars.len());
    let mut ln = 0usize;
    for &c in &chars {
        line_of.push(ln);
        if c == '\n' {
            ln += 1;
        }
    }
    let lines: Vec<&str> = full.split('\n').collect();
    let run_start = 16u32; // item_id (1,16)
    for b in &blocks {
        if let Block::TreeNode(tn) = b {
            let g = &tn.group;
            if let Some(a) = &g.anchor_id {
                let (p1, p2) = (a.value.part1, a.value.part2);
                if p1 != 1 {
                    println!("  node={} anchor=({p1},{p2}) [非文本 run/哨兵]", cid(&g.node_id));
                    continue;
                }
                let off = p2.wrapping_sub(run_start) as usize;
                let l = line_of.get(off).copied().unwrap_or(usize::MAX);
                let ctx: String = chars
                    .iter()
                    .skip(off.saturating_sub(4))
                    .take(9)
                    .collect();
                let line_txt = lines.get(l).copied().unwrap_or("<越界>");
                println!(
                    "  node={:<9} anchor=(1,{p2}) off={off:<3} 行{l}={:?}  ⟨±4:{:?}⟩",
                    cid(&g.node_id),
                    line_txt,
                    ctx
                );
            }
        }
    }

    println!("\n=== ③ SceneGroupItem（parent → child node）===");
    for b in &blocks {
        if let Block::SceneGroupItem(sg) = b {
            let child = sg
                .item
                .value
                .as_ref()
                .map(cid)
                .unwrap_or_else(|| "-".into());
            println!("  parent={:<9} child_node={}", cid(&sg.parent_id), child);
        }
    }

    println!("\n=== ④ SceneLineItem 笔画（parent_id / color / y-range）===");
    for b in &blocks {
        if let Block::SceneLineItem(sib) = b {
            if let Some(line) = &sib.item.value {
                if line.points.is_empty() {
                    continue;
                }
                let (mut y0, mut y1, mut x0, mut x1) =
                    (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for pt in &line.points {
                    y0 = y0.min(pt.y);
                    y1 = y1.max(pt.y);
                    x0 = x0.min(pt.x);
                    x1 = x1.max(pt.x);
                }
                println!(
                    "  parent={:<9} color={:<6?} pts={:<4} y=[{:.0},{:.0}] x=[{:.0},{:.0}]",
                    cid(&sib.parent_id),
                    line.color,
                    line.points.len(),
                    y0,
                    y1,
                    x0,
                    x1
                );
            }
        }
    }
}
