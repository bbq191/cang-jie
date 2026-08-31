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
//! ## 验证覆盖（A1 跨场景加固）
//! - `· ` bullet 卡：`hw0.rm`（污染卡）5 组 5/5 + DeepSeek 真机 --apply E2E。
//! - 空槽卡（`is_slot_header`，cardHighlights 关的默认形态）：`renbone_slots.rm` 真机
//!   （帕金森抖动 9 片手写、金句同行 + 洞见下行）→ 2 桶正确 + qwen 真机 dry-run 坐实；
//!   合成锚点补测所有 7 槽同行书写、卡顶无槽不乱塞、顶/底哨兵 None。
//! ⚠ 仍缺真机 geometry 覆盖（低 ROI、需新写才有）：① **bullet+空槽混排卡**（cardHighlights
//!    开）——两分支交叉行为无 fixture；② 下方相邻槽（无空行间隔）「下一行书写」可能吸到隔壁槽
//!    （renbone 洞见靠其下有空行才准）。二者遇到时靠计数护栏兜底（桶数≠识别数则不注入），
//!    要彻底坐实须补对应真机 fixture。

use device_core::notebook_rm::{anchor_char_offset, read_root_text_runs, TextRun};
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

/// 分类槽头（🟡 金句…／🔵 洞见…／🩷 疑问…／🟠 主题…／🟢 可复用…／⚪ 人物…／🔗 关联→ID）。
/// 模板 4 套（通用/原文/悬疑/科幻）标签各异但槽 emoji 前缀固定（见 cardsync::SLOT_EMOJI）。
/// cardHighlights 默认关时卡片只有空槽头、无 `· ` bullet——手写批注需能落到槽头（用户 2026-08-31 定）。
pub fn is_slot_header(line: &str) -> bool {
    const SLOT_EMOJI: [&str; 7] = ["🟡", "🔵", "🩷", "🟠", "🟢", "⚪", "🔗"];
    let s = line.trim_start();
    SLOT_EMOJI.iter().any(|e| s.starts_with(e))
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

/// 把一片手写关联到落点行。**两类卡分别处理**（互不干扰：`· ` 卡无槽头、空槽卡无 bullet）：
/// ① 书摘 bullet 卡（cardHighlights 开）：anchor 常落小节头、bullet 在其下/上 → 方向吸附到 bullet；
/// ② 空槽卡（cardHighlights 关，多数情形）：只有分类槽头、内容永远写在**槽头之下** →
///    落到「书写位所在/上方最近的分类槽头」（据 mean_y 符号估书写行，再向上找最近 `is_slot_header`）。
/// 无锚点/哨兵/anchor 越界/两类都找不到 → `None`（宁缺勿造）。
/// 定案：用户 2026-08-31 真机（《人骨拼图》空槽卡，帕金森抖动笔迹裂成多片）——组6 越空行 overshoot
/// 到隔壁槽的病根 = 拿 bullet 的向下方向套空槽；空槽改「向上就近槽头」后 9 片→2 桶（金句/洞见）正确。
pub fn associate(group: &HwGroup, full: &str, runs: &[TextRun], lines: &[&str]) -> Option<Assoc> {
    let anchor = group.anchor?;
    let off = anchor_char_offset(anchor, runs)?;
    let aline = line_of_offset(full, off)?;
    let my = group.mean_y();
    let a = aline as isize;

    // ① 书摘 bullet：按 mean_y 符号定方向、就近吸附（仅认 `· ` bullet）。
    let mut cands: Vec<isize> = Vec::new();
    if my < -DIR_EPS {
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a - d);
        }
        cands.push(a);
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a + d);
        }
    } else if my > DIR_EPS {
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a + d);
        }
        cands.push(a);
        for d in 1..=SNAP_WINDOW as isize {
            cands.push(a - d);
        }
    } else {
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

    // ② 空槽卡：估书写行（锚行 + 方向），**无界向上**找最近分类槽头。
    // 内容永远属其上方最近的槽（槽下堆多少行都算这个槽），故不设窗口——否则槽内容一多、
    // 手写离槽头远（如洞见下已注入多行），有界搜索够不着就丢（2026-08-31 真机 multislot 踩到）。
    let est = a + if my > DIR_EPS {
        1
    } else if my < -DIR_EPS {
        -1
    } else {
        0
    };
    let mut c = est.min(lines.len() as isize - 1);
    while c >= 0 {
        if is_slot_header(lines[c as usize]) {
            return Some(Assoc { line: c as usize, anchor_line: aline });
        }
        c -= 1;
    }
    None
}

/// 一条书摘行上待注入的批注计划：命中的 bullet 行 + 归属该行的手写组（`CardPage.groups` 下标，first-seen 序）。
#[derive(Debug, Clone)]
pub struct LinePlan {
    pub line: usize,
    pub group_idx: Vec<usize>,
}

/// 一页卡片 .rm 的解析上下文——**只解析一次**：文本 run + 手写组 + 关联分桶。
/// 计数提示（`bucket_count`）与注入（`cardhw::inject_by_anchor`）共用同一份，避免重复解析 .rm。
/// cardanchor 独占".rm→关联计划"的职责；下游只消费本结构、不再碰 .rm 解析。
pub struct CardPage {
    /// 全文（由 `runs` 串联得出，与 `notebook_rm::read_root_text` 同口径：file order、跳 FormatCode）。
    pub full: String,
    /// 文本 run（各 run 首字符 CrdtId + 偏移；供 anchor 字符 id → 行）。
    pub runs: Vec<TextRun>,
    /// `full.split('\n')`（owned）。
    pub lines: Vec<String>,
    /// 手写组（first-seen 序）。
    pub groups: Vec<HwGroup>,
    /// 关联分桶（bullet 行号升序 = top-to-bottom）。关联不上的组已丢弃（不产生杜纂——宁缺勿造）。
    pub plans: Vec<LinePlan>,
}

impl CardPage {
    /// 解析一页 .rm，全程只解析一次（`full` 由 runs 串联、不再单独跑 read_root_text）。
    pub fn parse(rm: &[u8]) -> CardPage {
        let runs = read_root_text_runs(rm);
        let full: String = runs.iter().map(|r| r.text.as_str()).collect();
        let lines: Vec<String> = full.split('\n').map(str::to_string).collect();
        let groups = read_hw_groups(rm);
        let plans = build_plans(&groups, &full, &runs, &lines);
        CardPage { full, runs, lines, groups, plans }
    }

    /// 关联得上 bullet 的批注桶数（= 期望转写条数，供 vision 计数提示稳住条数抖动）。
    pub fn bucket_count(&self) -> usize {
        self.plans.len()
    }
}

/// 手写组按「关联到的 bullet 行」分桶，行号升序（top-to-bottom）。同一 bullet 被 xochitl 拆成的
/// 多组（连写切成几片）自然并进同一桶；关联不上的组（无锚/哨兵/窗口内无 bullet）被丢弃。
fn build_plans(groups: &[HwGroup], full: &str, runs: &[TextRun], lines: &[String]) -> Vec<LinePlan> {
    use std::collections::BTreeMap;
    let lref: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut buckets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, g) in groups.iter().enumerate() {
        if let Some(a) = associate(g, full, runs, &lref) {
            buckets.entry(a.line).or_default().push(i);
        }
    }
    buckets.into_iter().map(|(line, group_idx)| LinePlan { line, group_idx }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HW0: &[u8] = include_bytes!("../testdata/cardhw/hw0.rm");

    #[test]
    fn cardpage_full_matches_read_root_text() {
        // CardPage.full 由 runs 串联；须与 device-core 验证过的 read_root_text 逐字一致
        // （否则 anchor 偏移→行 会错位）。
        let page = CardPage::parse(HW0);
        assert_eq!(page.full, device_core::notebook_rm::read_root_text(HW0));
        assert_eq!(page.lines.len(), page.full.split('\n').count());
    }

    #[test]
    fn hw0_bucket_count() {
        assert_eq!(CardPage::parse(HW0).bucket_count(), 5, "hw0 有 5 个关联桶");
    }

    // 空槽卡（cardHighlights 关）：真机《人骨拼图》总结卡，用户手写 2 处——
    // 金句同行「我是你爸爸」、洞见下一行「我换行了」（帕金森抖动笔迹裂成 9 片 stroke-group）。
    // 病根修前：向下方向吸附把洞见的碎片 overshoot 到隔壁「疑问」槽→3 桶、计数护栏拦。
    // 修后：空槽卡走「向上就近槽头」，9 片正确归 2 桶（金句 / 洞见）。
    const RENBONE: &[u8] = include_bytes!("../testdata/cardhw/renbone_slots.rm");

    #[test]
    fn renbone_empty_slots_two_buckets() {
        let page = CardPage::parse(RENBONE);
        assert_eq!(page.groups.len(), 9, "帕金森抖动裂成 9 片");
        assert_eq!(page.bucket_count(), 2, "9 片应归 2 桶（金句 / 洞见），不 overshoot 到疑问");
        let slots: Vec<&str> = page
            .plans
            .iter()
            .map(|p| page.lines[p.line].trim())
            .collect();
        assert_eq!(slots, vec!["🟡 金句·要记的句：", "🔵 洞见："], "两桶=金句+洞见（top-to-bottom）");
        // 每桶落点都是槽头。
        for p in &page.plans {
            assert!(is_slot_header(&page.lines[p.line]), "落点须是槽头：{:?}", page.lines[p.line]);
        }
    }

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
        let page = CardPage::parse(HW0);
        let lref: Vec<&str> = page.lines.iter().map(String::as_str).collect();
        // 逐组期望的命中 bullet 行文本（吸附规则的黄金对账）。
        let want: &[(usize, &str)] = &[
            (0, "只想睡觉"), // (2,288) 行7 bullet，锚点即本行
            (1, "她"),       // (2,297) 行5 header→下吸附行6 她
            (4, "她站在候车队伍中"), // (2,351) 行10 header→上吸附行9
            (5, "踢翻一个红蚂蚁窝"), // (2,396) 行14 header→上吸附行13
            (6, "很不高兴"), // (2,447) 行18 footer→上吸附行17
        ];
        for (gi, needle) in want {
            let a = associate(&page.groups[*gi], &page.full, &page.runs, &lref)
                .unwrap_or_else(|| panic!("组 {gi} 应关联到某 bullet"));
            assert!(
                lref[a.line].contains(needle),
                "组 {gi} 期望命中含{:?}的行，实得 行{}={:?}",
                needle,
                a.line,
                lref[a.line]
            );
        }
    }

    #[test]
    fn hw0_plan_buckets_top_to_bottom() {
        let page = CardPage::parse(HW0);
        // 行号严格升序（top-to-bottom）。
        for w in page.plans.windows(2) {
            assert!(w[0].line < w[1].line, "计划按行升序");
        }
        // 红色 group（写在"她"旁）中 mean_y 明显偏移的三片吸附到行6"她"、归并同桶
        // （用户实测：红黑蓝写在"她"旁）。mean_y≈0 的边界组归相邻行7，属 ±1 固有歧义。
        let she = page.plans.iter().find(|p| page.lines[p.line].trim_end() == "   · 她").unwrap();
        assert!(
            she.group_idx.len() >= 3,
            "「她」行应聚拢≥3 组红片，实得 {:?}",
            she.group_idx
        );
        // 覆盖的 bullet 行都是真 bullet。
        for p in &page.plans {
            assert!(is_bullet(&page.lines[p.line]), "计划命中行须是 bullet：{:?}", page.lines[p.line]);
        }
    }

    // ── A1 跨场景加固：用 renbone 真实文本几何 + 合成锚点，覆盖真数据没碰到的槽位/边界 ──
    // （renbone 真手写只落在 金句/洞见 两个顶部槽；下方槽 疑问/主题/可复用/人物 及边界靠合成验证）。
    use remarkable_lines::v6::scene_item::point::Point;

    /// char 偏移 → CrdtId（`anchor_char_offset` 的逆：run 内字符 id 从 run.id 连续 +1）。
    fn crdt_at(off: usize, runs: &[TextRun]) -> (u8, u32) {
        for r in runs {
            let n = r.text.chars().count();
            if off >= r.char_off && off < r.char_off + n {
                return (r.id.0, r.id.1 + (off - r.char_off) as u32);
            }
        }
        panic!("offset {off} 不在任何 run 内");
    }
    /// 行 li 首字符的绝对 char 偏移（每行 +1 计 `\n`）。
    fn line_start(lines: &[String], li: usize) -> usize {
        lines[..li].iter().map(|l| l.chars().count() + 1).sum()
    }
    /// 合成一片手写：锚 + 给定 mean_y（单点即可，mean_y 只读 y）。
    fn mk_group(anchor: (u8, u32), mean_y: f32) -> HwGroup {
        HwGroup {
            node_id: (99, 99),
            anchor: Some(anchor),
            anchor_origin_x: 0.0,
            color: "BLACK".into(),
            strokes: vec![vec![Point { x: 0.0, y: mean_y, speed: 0.0, direction: 0.0, width: 0.0, pressure: 0.0 }]],
        }
    }
    fn assoc_at(page: &CardPage, off: usize, mean_y: f32) -> Option<usize> {
        let g = mk_group(crdt_at(off, &page.runs), mean_y);
        let lref: Vec<&str> = page.lines.iter().map(String::as_str).collect();
        associate(&g, &page.full, &page.runs, &lref).map(|a| a.line)
    }

    #[test]
    fn slot_same_line_all_positions() {
        // 每个分类槽头「同行书写」（mean_y≈0）都吸到自己那行——含下方槽（renbone 真数据只覆盖顶部两槽）。
        let page = CardPage::parse(RENBONE);
        for label in ["🟡", "🔵", "🩷", "🟠", "🟢", "⚪", "🔗"] {
            let li = page.lines.iter().position(|l| l.starts_with(label)).unwrap();
            let off = line_start(&page.lines, li) + 2; // 槽头行内（越过 emoji+空格）
            assert_eq!(assoc_at(&page, off, 0.0), Some(li), "同行写 {label} 槽应吸到本行");
        }
    }

    #[test]
    fn top_of_card_no_slot_above_returns_none() {
        // 卡顶标题行（其上无任何槽头）书写 → None（宁缺勿造，不往上乱塞）。
        let page = CardPage::parse(RENBONE);
        assert_eq!(assoc_at(&page, 0, 0.0), None);
    }

    #[test]
    fn sentinel_anchor_none() {
        // 顶/底哨兵锚（0xFFFF_FFFE/FF）→ None。
        let page = CardPage::parse(RENBONE);
        let lref: Vec<&str> = page.lines.iter().map(String::as_str).collect();
        for sent in [0xFFFF_FFFEu32, 0xFFFF_FFFF] {
            let g = mk_group((1, sent), 0.0);
            assert_eq!(associate(&g, &page.full, &page.runs, &lref), None, "哨兵 {sent:#x} 应 None");
        }
    }

    #[test]
    fn multislot_unbounded_upward_reaches_deep_slot() {
        // 真机多槽卡：洞见下已注入多行（第一行…行长）把新手写推到离洞见头 6 行；
        // 人物/关联下各写一处。无界上吸后 → 3 个槽都识别到（洞见含在内），不因窗口小丢洞见。
        // （有界 SLOT_WINDOW=3 时洞见手写会被丢——本测试锁死该回归。）
        let page = CardPage::parse(include_bytes!("../testdata/cardhw/multislot.rm"));
        let slots: std::collections::HashSet<&str> =
            page.plans.iter().map(|p| page.lines[p.line].trim()).collect();
        assert!(slots.iter().any(|s| s.starts_with("🔵 洞见")), "洞见（深处）应够到，实得 {slots:?}");
        assert!(slots.iter().any(|s| s.starts_with("⚪ 人物")), "人物应识别");
        assert!(slots.iter().any(|s| s.starts_with("🔗 关联")), "关联应识别");
        assert!(page.plans.len() >= 3, "≥3 槽有手写");
    }
}
