//! 锚点关联（块⑥ cardhw 治本方案）——从 .rm **确定性**提取「手写 group ↔ 所贴书摘行」，
//! 取代 vision 猜关联（杜纂根因：vision 在 384px 缩略图上把手写安到错/空 bullet）。
//!
//! ## 原理（reMarkable v6 anchor 机制，离线 dump hw0.rm 坐实，见 examples/dump_anchor.rs）
//! 卡片是原生打字笔记：用户在上面手写批注时，xochitl 把每片笔迹归成一个 TreeNode 组，
//! 组带 `anchor_id`——指向 RootText 里一个**字符的 CrdtId**（提供该组的锚定 y 基线）。
//! 卡片文本是单个未拆分 run `id=(1,16)`，故 char i 的 CrdtId=(1,16+i)、offset=anchor.part2-16
//! （多 run 情形由 `notebook_rm::anchor_char_offset` 按 run 区间通用处理）。
//!
//! ## 关键实测：anchor 不是精确 bullet 指针，而是 **±1 行的 y-带**
//! anchor 落在 y 最近的字符上，常是 section header 或行边界、而非目标 bullet。
//! 治法：anchor 行 + **笔画均值 y 的符号**定方向（负=写在锚点上方、正=下方），
//! 就近吸附到该方向最近的 bullet 行。hw0.rm 五组 5/5 命中（含用户"红黑蓝写在她旁"）。
//! ⚠ 此吸附规则仅在 hw0.rm（多轮测试污染卡）上验证过，跨场景（单条批注 / 顶底哨兵 /
//!    多 section）须用干净设备 fixture 复核后方可接入生产 daemon。

use device_core::notebook_rm::{anchor_char_offset, TextRun};
use remarkable_lines::v6::block::Block;
use remarkable_lines::v6::crdt::CrdtId;
use remarkable_lines::v6::scene_item::point::Point;
use remarkable_lines::RemarkableFile;

/// 一片手写（一个 TreeNode 组，按 parent_id 聚合其全部笔画）。
#[derive(Debug, Clone)]
pub struct HwGroup {
    pub node_id: (u8, u32),
    /// 锚字符 CrdtId；`None`=无锚点或顶/底哨兵（0xFFFF_FFFE/FF）。
    pub anchor: Option<(u8, u32)>,
    pub anchor_origin_x: f32,
    /// 主色（首笔非 Unknown 色的名字；全 Unknown 则取首笔）。
    pub color: String,
    /// 笔画（每笔一串点，坐标相对 anchor）。渲染 + 几何都用它。
    pub strokes: Vec<Vec<Point>>,
}

impl HwGroup {
    /// 全部点的 y 均值（相对 anchor：负=锚点上方，正=下方）。空组返回 0。
    pub fn mean_y(&self) -> f32 {
        let mut sum = 0.0f32;
        let mut n = 0u32;
        for s in &self.strokes {
            for p in s {
                sum += p.y;
                n += 1;
            }
        }
        if n == 0 {
            0.0
        } else {
            sum / n as f32
        }
    }

    /// 笔画总点数（判是否空组 / 排序稳定用）。
    pub fn point_count(&self) -> usize {
        self.strokes.iter().map(|s| s.len()).sum()
    }
}

fn cid(c: &CrdtId) -> (u8, u32) {
    (c.part1, c.part2)
}

fn color_name(c: &remarkable_lines::shared::pen_color::PenColor) -> String {
    use remarkable_lines::shared::pen_color::PenColor as P;
    match c {
        P::Black => "BLACK",
        P::Grey => "GRAY",
        P::White => "WHITE",
        P::Yellow => "YELLOW",
        P::Green => "GREEN",
        P::Pink => "PINK",
        P::Blue => "BLUE",
        P::Red => "RED",
        P::GreyOverlap => "GRAY_OVERLAP",
        P::Unknown(v) => return format!("UNKNOWN_{v}"),
    }
    .to_string()
}

/// 读一页 .rm 的手写组：按 SceneLineItem.parent_id 聚笔画，配上该组 TreeNode 的锚点。
/// root/layer 组（(0,1)/(0,11) 等无笔画的结构组）自然不出现（无 SceneLineItem 指向）。
/// 无笔画的组不产出。
pub fn read_hw_groups(rm: &[u8]) -> Vec<HwGroup> {
    let rf = match RemarkableFile::read(rm) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let blocks = match rf {
        RemarkableFile::V6 { blocks, .. } => blocks,
        _ => return Vec::new(),
    };

    // ① node_id → (anchor, origin_x)
    use std::collections::HashMap;
    type NodeAnchor = (Option<(u8, u32)>, f32);
    let mut anchors: HashMap<(u8, u32), NodeAnchor> = HashMap::new();
    for b in &blocks {
        if let Block::TreeNode(tn) = b {
            let g = &tn.group;
            let a = g.anchor_id.as_ref().map(|l| cid(&l.value));
            let ox = g.anchor_origin_x.as_ref().map(|l| l.value).unwrap_or(0.0);
            anchors.insert(cid(&g.node_id), (a, ox));
        }
    }

    // ② parent_id 聚笔画（保 first-seen 顺序）
    let mut order: Vec<(u8, u32)> = Vec::new();
    let mut by_parent: HashMap<(u8, u32), Vec<Vec<Point>>> = HashMap::new();
    let mut color_of: HashMap<(u8, u32), String> = HashMap::new();
    for b in &blocks {
        if let Block::SceneLineItem(sib) = b {
            if let Some(line) = &sib.item.value {
                if line.points.is_empty() {
                    continue;
                }
                let pid = cid(&sib.parent_id);
                by_parent.entry(pid).or_insert_with(|| {
                    order.push(pid);
                    Vec::new()
                });
                by_parent.get_mut(&pid).unwrap().push(line.points.clone());
                // 主色：记第一个非 Unknown 色
                let cn = color_name(&line.color);
                let e = color_of.entry(pid).or_insert_with(|| cn.clone());
                if e.starts_with("UNKNOWN") && !cn.starts_with("UNKNOWN") {
                    *e = cn;
                }
            }
        }
    }

    order
        .into_iter()
        .map(|pid| {
            let (anchor, ox) = anchors.get(&pid).copied().unwrap_or((None, 0.0));
            HwGroup {
                node_id: pid,
                anchor,
                anchor_origin_x: ox,
                color: color_of.remove(&pid).unwrap_or_else(|| "BLACK".into()),
                strokes: by_parent.remove(&pid).unwrap_or_default(),
            }
        })
        .collect()
}

/// char offset → 行号（\n 归上一行末）。
fn line_of_offset(full: &str, off: usize) -> Option<usize> {
    let mut ln = 0usize;
    for (i, c) in full.chars().enumerate() {
        if i == off {
            return Some(ln);
        }
        if c == '\n' {
            ln += 1;
        }
    }
    None
}

/// 某行是否书摘 bullet（`   · ...`）。
fn is_bullet(line: &str) -> bool {
    let s = line.trim_start();
    s.starts_with("· ") || s.starts_with('·')
}

/// 一片手写关联到的书摘行结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Assoc {
    /// 命中的 bullet 行号（`full.split('\n')` 下标）。
    pub line: usize,
    /// anchor 原始落点行（诊断用；吸附前）。
    pub anchor_line: usize,
}

/// 方向阈值：笔画均值 y 绝对值 < 此值视为「就在锚点行」（经验值，相对 anchor 坐标）。
const DIR_EPS: f32 = 15.0;
/// 吸附搜索窗口（行）。
const SNAP_WINDOW: usize = 2;

/// 把一片手写关联到书摘 bullet 行：anchor→行，笔画 y 符号定方向，就近吸附到 bullet。
/// 无锚点/哨兵/anchor 越界/窗口内无 bullet → `None`。
pub fn associate(group: &HwGroup, full: &str, runs: &[TextRun], lines: &[&str]) -> Option<Assoc> {
    let anchor = group.anchor?;
    let off = anchor_char_offset(anchor, runs)?;
    let aline = line_of_offset(full, off)?;
    let my = group.mean_y();

    // 候选行的搜索顺序：按方向优先。
    let mut cands: Vec<isize> = Vec::new();
    let a = aline as isize;
    if my < -DIR_EPS {
        // 写在锚点上方：优先往上
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a - d);
        }
        cands.push(a);
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a + d);
        }
    } else if my > DIR_EPS {
        // 写在锚点下方：优先往下
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a + d);
        }
        cands.push(a);
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a - d);
        }
    } else {
        // 就在锚点行：先本行，再上下交替
        cands.push(a);
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a - d);
            cands.push(a + d);
        }
    }

    for c in cands {
        if c < 0 || c as usize >= lines.len() {
            continue;
        }
        if is_bullet(lines[c as usize]) {
            return Some(Assoc { line: c as usize, anchor_line: aline });
        }
    }
    None
}

/// 一条书摘行上待注入的批注计划：命中的 bullet 行 + 归属该行的手写组（按 first-seen 序）。
/// daemon 消费：对每个 bucket 转写（整页缩略图或按几何裁剪/渲染，策略见模块头），注入到 `line`。
#[derive(Debug, Clone)]
pub struct LinePlan {
    pub line: usize,
    pub group_idx: Vec<usize>, // 指向 read_hw_groups 返回的下标
}

/// 把一页 .rm 的全部手写组按「关联到的 bullet 行」分桶，返回按行号升序（top-to-bottom）的注入计划。
/// 同一 bullet 被 xochitl 拆成的多组（如连写被切成几笔片）自然并进同一桶。
/// 关联不上的组（无锚/哨兵/窗口内无 bullet）被丢弃（不产生杜纂——宁缺勿造）。
pub fn plan_injections(rm: &[u8], full: &str, runs: &[TextRun], lines: &[&str]) -> (Vec<HwGroup>, Vec<LinePlan>) {
    let groups = read_hw_groups(rm);
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, g) in groups.iter().enumerate() {
        if let Some(a) = associate(g, full, runs, lines) {
            buckets.entry(a.line).or_default().push(i);
        }
    }
    let plans = buckets
        .into_iter()
        .map(|(line, group_idx)| LinePlan { line, group_idx })
        .collect();
    (groups, plans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use device_core::notebook_rm::read_root_text_runs;

    const HW0: &[u8] = include_bytes!("../testdata/cardhw/hw0.rm");

    #[test]
    fn hw0_groups_extracted() {
        let groups = read_hw_groups(HW0);
        // dump_anchor 实测：7 个手写组（4 红 + 蓝 + 黑 + Unknown12）。
        assert_eq!(groups.len(), 7, "手写组数");
        let node_ids: Vec<_> = groups.iter().map(|g| g.node_id).collect();
        assert_eq!(
            node_ids,
            vec![(2, 288), (2, 297), (2, 324), (2, 339), (2, 351), (2, 396), (2, 447)]
        );
        // 锚点
        assert_eq!(groups[0].anchor, Some((1, 113)));
        assert_eq!(groups[1].anchor, Some((1, 100)));
        assert_eq!(groups[4].anchor, Some((1, 153)));
        assert_eq!(groups[5].anchor, Some((1, 216)));
        assert_eq!(groups[6].anchor, Some((1, 276)));
    }

    #[test]
    fn hw0_association_snaps_to_bullets() {
        let full = device_core::notebook_rm::read_root_text(HW0);
        let runs = read_root_text_runs(HW0);
        let lines: Vec<&str> = full.split('\n').collect();
        let groups = read_hw_groups(HW0);

        // 逐组期望的命中 bullet 行文本（吸附规则的黄金对账）。
        let want: &[(usize, &str)] = &[
            (0, "只想睡觉"), // (2,288) 行7 bullet，锚点即本行
            (1, "她"),       // (2,297) 行5 header→下吸附行6 她
            (4, "她站在候车队伍中"), // (2,351) 行10 header→上吸附行9
            (5, "踢翻一个红蚂蚁窝"), // (2,396) 行14 header→上吸附行13
            (6, "很不高兴"), // (2,447) 行18 footer→上吸附行17
        ];
        for (gi, needle) in want {
            let a = associate(&groups[*gi], &full, &runs, &lines)
                .unwrap_or_else(|| panic!("组 {gi} 应关联到某 bullet"));
            assert!(
                lines[a.line].contains(needle),
                "组 {gi} 期望命中含{:?}的行，实得 行{}={:?}",
                needle,
                a.line,
                lines[a.line]
            );
        }
    }

    #[test]
    fn hw0_plan_buckets_top_to_bottom() {
        let full = device_core::notebook_rm::read_root_text(HW0);
        let runs = read_root_text_runs(HW0);
        let lines: Vec<&str> = full.split('\n').collect();
        let (_groups, plans) = plan_injections(HW0, &full, &runs, &lines);

        // 行号严格升序（top-to-bottom）。
        for w in plans.windows(2) {
            assert!(w[0].line < w[1].line, "计划按行升序");
        }
        // 红色 group（写在"她"旁）中 mean_y 明显偏移的三片吸附到行6"她"、归并同桶
        // （用户实测：红黑蓝写在"她"旁）。mean_y≈0 的边界组归相邻行7，属 ±1 固有歧义。
        let she = plans.iter().find(|p| lines[p.line].trim_end() == "   · 她").unwrap();
        assert!(
            she.group_idx.len() >= 3,
            "「她」行应聚拢≥3 组红片，实得 {:?}",
            she.group_idx
        );
        // 覆盖的 bullet 行都是真 bullet。
        for p in &plans {
            assert!(is_bullet(lines[p.line]), "计划命中行须是 bullet：{:?}", lines[p.line]);
        }
    }
}
