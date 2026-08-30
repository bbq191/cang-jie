//! v6 .rm 写入器——移植 rmscene.simple_text_document + write_blocks，逆 remarkable_lines 的
//! tagged 读取器。生成一页"打字笔记"（单 RootTextBlock 装整页纯文本），供墨香把云端笔记拉成
//! reMarkable 原生笔记本、设备上可手写批注。对 rmscene 参照逐字节对拍。
//!
//! 除 author uuid / 文本串 / 字符&行计数外，其余 CRDT id 全是 simple_text_document 的固定常量。

pub const HEADER: &[u8] = b"reMarkable .lines file, version=6          "; // 43 字节

struct W {
    b: Vec<u8>,
}
impl W {
    fn new() -> Self {
        W { b: Vec::new() }
    }
    fn u8(&mut self, v: u8) {
        self.b.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn boolv(&mut self, v: bool) {
        self.u8(if v { 1 } else { 0 });
    }
    fn varuint(&mut self, mut v: u32) {
        loop {
            let mut byte = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            self.u8(byte);
            if v == 0 {
                break;
            }
        }
    }
    fn tag(&mut self, index: u32, ty: u32) {
        self.varuint((index << 4) | ty);
    }
    fn crdt(&mut self, id: (u8, u32)) {
        self.u8(id.0);
        self.varuint(id.1);
    }
    fn id(&mut self, index: u32, v: (u8, u32)) {
        self.tag(index, 0xF);
        self.crdt(v);
    }
    fn u8f(&mut self, index: u32, v: u8) {
        self.tag(index, 0x1);
        self.u8(v);
    }
    fn boolf(&mut self, index: u32, v: bool) {
        self.tag(index, 0x1);
        self.boolv(v);
    }
    fn u32f(&mut self, index: u32, v: u32) {
        self.tag(index, 0x4);
        self.u32(v);
    }
    fn f32f(&mut self, index: u32, v: f32) {
        self.tag(index, 0x4);
        self.f32(v);
    }
    /// subblock：tag(index, Length4=0xC) + u32 占位长度 + 内容 + 回填长度。
    fn subblock<F: FnOnce(&mut W)>(&mut self, index: u32, f: F) {
        self.tag(index, 0xC);
        let pos = self.b.len();
        self.u32(0);
        f(self);
        let size = (self.b.len() - pos - 4) as u32;
        self.b[pos..pos + 4].copy_from_slice(&size.to_le_bytes());
    }
    /// 带 index 的字符串：subblock{ varuint(字节长) + bool(is_ascii) + bytes }。
    fn string(&mut self, index: u32, s: &str) {
        self.subblock(index, |w| {
            w.varuint(s.len() as u32);
            w.boolv(s.is_ascii());
            w.b.extend_from_slice(s.as_bytes());
        });
    }
    fn lww_string(&mut self, index: u32, ts: (u8, u32), s: &str) {
        self.subblock(index, |w| {
            w.id(1, ts);
            w.string(2, s);
        });
    }
    fn lww_bool(&mut self, index: u32, ts: (u8, u32), v: bool) {
        self.subblock(index, |w| {
            w.id(1, ts);
            w.boolf(2, v);
        });
    }
    /// uuid：varuint(16) + 16 字节（前 3 段字节反转，与 read_uuid 相逆）。
    fn uuid(&mut self, u: &[u8; 16]) {
        self.varuint(16);
        let mut b = *u;
        b[0..4].reverse();
        b[4..6].reverse();
        b[6..8].reverse();
        self.b.extend_from_slice(&b);
    }
}

// ---------- 读回：按文件顺序抽 RootTextBlock 文本（供 Phase 3 读设备打的想法） ----------
// xochitl 编辑后把整块文本拆成多个 TextItem，但按文件顺序串联即得正确全文（rmscene
// sequence_items 也是取文件顺序，非 toposort）。这里只抽字符串，忽略 styles/x/y/width。

struct R<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> R<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.p)?;
        self.p += 1;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        if self.p + 4 > self.b.len() {
            return None;
        }
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().ok()?);
        self.p += 4;
        Some(v)
    }
    fn varuint(&mut self) -> Option<u32> {
        let mut shift = 0;
        let mut result = 0u32;
        loop {
            let i = self.u8()?;
            result |= ((i & 0x7F) as u32) << shift;
            shift += 7;
            if i & 0x80 == 0 {
                break;
            }
        }
        Some(result)
    }
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.p + n > self.b.len() {
            return None;
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Some(s)
    }
    /// 读一个 tag=varuint → (index, type)。
    fn tag(&mut self) -> Option<(u32, u32)> {
        let x = self.varuint()?;
        Some((x >> 4, x & 0xF))
    }
    /// 若下一个 tag 是 (index,ty) 则消费并返回 true（否则位置不动）。
    fn peek_is(&mut self, index: u32, ty: u32) -> bool {
        let save = self.p;
        if let Some((i, t)) = self.tag() {
            if i == index && t == ty {
                return true;
            }
        }
        self.p = save;
        false
    }
    /// 期望 subblock(index)：tag(index,Length4=0xC)+u32 size → 返回内容结束偏移。
    fn subblock(&mut self, index: u32) -> Option<usize> {
        let (i, t) = self.tag()?;
        if i != index || t != 0xC {
            return None;
        }
        let size = self.u32()? as usize;
        Some(self.p + size)
    }
    /// 跳过一个 id 字段：tag(index,ID)+crdt(u8+varuint)。
    fn skip_id(&mut self) -> Option<()> {
        self.tag()?; // index/ID
        self.u8()?; // crdt part1
        self.varuint()?; // crdt part2
        Some(())
    }
    /// 读一个 id 字段 → (part1, part2)。与 skip_id 同布局，只是返回值。
    fn read_id(&mut self) -> Option<(u8, u32)> {
        self.tag()?; // index/ID
        let p1 = self.u8()?;
        let p2 = self.varuint()?;
        Some((p1, p2))
    }
    /// 跳过一个 u32 字段：tag+u32。
    fn skip_u32(&mut self) -> Option<()> {
        self.tag()?;
        self.u32()?;
        Some(())
    }
}

/// 从一页 .rm 抽 RootTextBlock 全文（按文件 item 顺序串联）。找不到返回空串。
pub fn read_root_text(rm: &[u8]) -> String {
    if rm.len() < HEADER.len() || &rm[..HEADER.len()] != HEADER {
        return String::new();
    }
    let mut r = R { b: rm, p: HEADER.len() };
    // 遍历块，找 type=0x07 RootText
    loop {
        let start = r.p;
        let len = match r.u32() {
            Some(l) => l as usize,
            None => break,
        };
        // [unknown][min][cur][type]
        if r.bytes(3).is_none() {
            break;
        }
        let btype = match r.u8() {
            Some(t) => t,
            None => break,
        };
        let payload_start = r.p;
        let payload_end = payload_start + len;
        if btype == 0x07 {
            return extract_text(&mut R { b: rm, p: payload_start }, payload_end);
        }
        // 跳到下一块
        r.p = payload_end;
        if r.p <= start || r.p > rm.len() {
            break;
        }
    }
    String::new()
}

fn extract_text(r: &mut R, end: usize) -> String {
    let mut out = String::new();
    // block_id
    if r.skip_id().is_none() {
        return out;
    }
    // sb1(2) > sb2(1) > sb3(1) > count + items
    if r.subblock(2).is_none() {
        return out;
    }
    if r.subblock(1).is_none() {
        return out;
    }
    let sb3_end = match r.subblock(1) {
        Some(e) => e,
        None => return out,
    };
    let count = r.varuint().unwrap_or(0);
    for _ in 0..count {
        if r.p >= sb3_end || r.p >= end {
            break;
        }
        let item_end = match r.subblock(0) {
            Some(e) => e,
            None => break,
        };
        // id(2) item_id, id(3) left, id(4) right, u32(5) del_len
        r.skip_id();
        r.skip_id();
        r.skip_id();
        r.skip_u32();
        // 可选 subblock(6)：字符串值
        if r.p < item_end && r.peek_is(6, 0xC) {
            let sb6_end = r.u32().map(|s| r.p + s as usize).unwrap_or(item_end);
            if let Some(slen) = r.varuint() {
                let _ascii = r.u8();
                if let Some(sb) = r.bytes(slen as usize) {
                    // 若其后有 tag(2,Byte4) 则是 FormatCode（段落格式），非文本→跳过
                    let is_format = r.p < sb6_end && r.peek_is(2, 0x4);
                    if !is_format {
                        out.push_str(&String::from_utf8_lossy(sb));
                    }
                }
            }
            r.p = sb6_end;
        }
        r.p = item_end;
    }
    out
}

/// RootText 的一个文本 run（`TextItem::Text`），按文件顺序。
/// `id`=run 首字符 CrdtId（(part1,part2)）；run 内第 i 字符的 CrdtId=(part1, part2+i)。
/// `char_off`=该 run 首字符在 `read_root_text` 全文里的字符偏移（跳过 FormatCode，与全文串联对齐）。
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub id: (u8, u32),
    pub char_off: usize,
    pub text: String,
}

/// 抽 RootText 的文本 run 序列（file order），每 run 带首字符 CrdtId + 累计字符偏移。
/// 供「手写 group 的 anchor_id（字符 CrdtId）→ 全文字符偏移 → 行」映射：run 内 id 连续，
/// 找 anchor.part2 落在哪个 run 的 [id.part2, id.part2+len) 区间即得偏移。FormatCode 项不产出 run
/// （不含文本、不是 anchor 目标），但其字符偏移贡献为 0——与 `read_root_text` 的串联口径一致。
pub fn read_root_text_runs(rm: &[u8]) -> Vec<TextRun> {
    if rm.len() < HEADER.len() || &rm[..HEADER.len()] != HEADER {
        return Vec::new();
    }
    let mut r = R { b: rm, p: HEADER.len() };
    loop {
        let start = r.p;
        let len = match r.u32() {
            Some(l) => l as usize,
            None => break,
        };
        if r.bytes(3).is_none() {
            break;
        }
        let btype = match r.u8() {
            Some(t) => t,
            None => break,
        };
        let payload_start = r.p;
        let payload_end = payload_start + len;
        if btype == 0x07 {
            return extract_text_runs(&mut R { b: rm, p: payload_start }, payload_end);
        }
        r.p = payload_end;
        if r.p <= start || r.p > rm.len() {
            break;
        }
    }
    Vec::new()
}

fn extract_text_runs(r: &mut R, end: usize) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut char_off = 0usize;
    if r.skip_id().is_none() {
        return runs;
    }
    if r.subblock(2).is_none() || r.subblock(1).is_none() {
        return runs;
    }
    let sb3_end = match r.subblock(1) {
        Some(e) => e,
        None => return runs,
    };
    let count = r.varuint().unwrap_or(0);
    for _ in 0..count {
        if r.p >= sb3_end || r.p >= end {
            break;
        }
        let item_end = match r.subblock(0) {
            Some(e) => e,
            None => break,
        };
        // id(2) item_id, id(3) left, id(4) right, u32(5) del_len
        let item_id = match r.read_id() {
            Some(id) => id,
            None => break,
        };
        r.skip_id(); // left
        r.skip_id(); // right
        r.skip_u32(); // del_len
        if r.p < item_end && r.peek_is(6, 0xC) {
            let sb6_end = r.u32().map(|s| r.p + s as usize).unwrap_or(item_end);
            if let Some(slen) = r.varuint() {
                let _ascii = r.u8();
                if let Some(sb) = r.bytes(slen as usize) {
                    let is_format = r.p < sb6_end && r.peek_is(2, 0x4);
                    if !is_format {
                        let text = String::from_utf8_lossy(sb).to_string();
                        let n = text.chars().count();
                        runs.push(TextRun { id: item_id, char_off, text });
                        char_off += n;
                    }
                }
            }
            r.p = sb6_end;
        }
        r.p = item_end;
    }
    runs
}

/// anchor 字符 CrdtId → 全文字符偏移。找 anchor 落在哪个 run 的 id 区间。
/// 哨兵 part2==0xFFFF_FFFE（顶）/0xFFFF_FFFF（底）返回 None（不指向具体字符）。
pub fn anchor_char_offset(anchor: (u8, u32), runs: &[TextRun]) -> Option<usize> {
    let (p1, p2) = anchor;
    if p2 == 0xFFFF_FFFE || p2 == 0xFFFF_FFFF {
        return None;
    }
    for run in runs {
        if run.id.0 != p1 {
            continue;
        }
        let n = run.text.chars().count() as u32;
        if p2 >= run.id.1 && p2 < run.id.1 + n {
            return Some(run.char_off + (p2 - run.id.1) as usize);
        }
    }
    None
}

fn frame(out: &mut Vec<u8>, btype: u8, min: u8, cur: u8, content: &[u8]) {
    out.extend_from_slice(&(content.len() as u32).to_le_bytes());
    out.push(0x00); // unknown
    out.push(min);
    out.push(cur);
    out.push(btype);
    out.extend_from_slice(content);
}

fn write_text(w: &mut W, text: &str) {
    // caller 已写 block_id = id(1,(0,0))
    w.subblock(2, |w| {
        // sb1（含 sb2 文本 + sb4 样式）
        w.subblock(1, |w| {
            // sb2
            w.subblock(1, |w| {
                // sb3
                w.varuint(1); // amount_items
                w.subblock(0, |w| {
                    // item wrapper
                    w.id(2, (1, 16)); // item_id（固定）
                    w.id(3, (0, 0)); // left
                    w.id(4, (0, 0)); // right
                    w.u32f(5, 0); // deleted_length
                    w.subblock(6, |w| {
                        w.varuint(text.len() as u32); // 字节长
                        w.boolv(true); // _is_ascii：rmscene 恒写 True（解析时忽略此位）
                        w.b.extend_from_slice(text.as_bytes());
                        // 纯文本，无 FormatCode tag
                    });
                });
            });
        });
        w.subblock(2, |w| {
            // sb4
            w.subblock(1, |w| {
                // sb5
                w.varuint(1); // amount_styles
                w.crdt((0, 0)); // style id（裸 CrdtId，无 tag）
                w.id(1, (1, 15)); // timestamp
                w.subblock(2, |w| {
                    w.u8(0x11); // c（未知格式前缀，对拍确认）
                    w.u8(1); // ParagraphStyle::PLAIN
                });
            });
        });
    });
    w.subblock(3, |w| {
        w.f64(-468.0); // x（固定 bounding box）
        w.f64(234.0); // y
    });
    w.f32f(4, 936.0); // width
}

/// 生成一页"打字笔记" .rm。author = 16 字节 uuid（大端标准序，内部转存储序）。
pub fn simple_text_document(text: &str, author: &[u8; 16]) -> Vec<u8> {
    let char_count = text.chars().count() as u32 + 1;
    let line_count = text.matches('\n').count() as u32 + 1;
    let mut out = Vec::new();
    out.extend_from_slice(HEADER);

    // 0x09 AuthorIds (1,1)
    {
        let mut w = W::new();
        w.varuint(1);
        w.subblock(0, |w| {
            w.uuid(author);
            w.u16(1);
        });
        frame(&mut out, 0x09, 1, 1, &w.b);
    }
    // 0x00 MigrationInfo (1,1)：id + is_device + _unknown(bool，Rust 解析器忽略但 rmscene 会写)
    {
        let mut w = W::new();
        w.id(1, (1, 1));
        w.u8f(2, 1); // is_device
        w.u8f(3, 0); // _unknown = False
        frame(&mut out, 0x00, 1, 1, &w.b);
    }
    // 0x0A PageInfo (0,1)
    {
        let mut w = W::new();
        w.u32f(1, 1); // loads_count
        w.u32f(2, 0); // merges_count
        w.u32f(3, char_count);
        w.u32f(4, line_count);
        w.u32f(5, 0); // type_folio_use_count
        frame(&mut out, 0x0A, 0, 1, &w.b);
    }
    // 0x01 SceneTree (1,1)
    {
        let mut w = W::new();
        w.id(1, (0, 11)); // tree_id
        w.id(2, (0, 0)); // node_id
        w.boolf(3, true); // is_update
        w.subblock(4, |w| {
            w.id(1, (0, 1)); // parent_id
        });
        frame(&mut out, 0x01, 1, 1, &w.b);
    }
    // 0x07 RootText (1,1)
    {
        let mut w = W::new();
        w.id(1, (0, 0)); // block_id
        write_text(&mut w, text);
        frame(&mut out, 0x07, 1, 1, &w.b);
    }
    // 0x02 TreeNode (1,2) —— root group (0,1)
    {
        let mut w = W::new();
        w.id(1, (0, 1));
        w.lww_string(2, (0, 0), "");
        w.lww_bool(3, (0, 0), true);
        frame(&mut out, 0x02, 1, 2, &w.b);
    }
    // 0x02 TreeNode (1,2) —— layer group (0,11) "Layer 1"
    {
        let mut w = W::new();
        w.id(1, (0, 11));
        w.lww_string(2, (0, 12), "Layer 1");
        w.lww_bool(3, (0, 0), true);
        frame(&mut out, 0x02, 1, 2, &w.b);
    }
    // 0x04 SceneGroupItem (1,1)
    {
        let mut w = W::new();
        w.id(1, (0, 1)); // parent_id
        w.id(2, (0, 13)); // item_id
        w.id(3, (0, 0)); // left
        w.id(4, (0, 0)); // right
        w.u32f(5, 0); // deleted_length
        w.subblock(6, |w| {
            w.u8(2); // SceneItemType::SceneGroupItemBlock
            w.id(2, (0, 11)); // value = group node id
        });
        frame(&mut out, 0x04, 1, 1, &w.b);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_roundtrip_single_item() {
        // simple_text_document 写单 run id=(1,16)；读回应得一个 run、text 一致、char_off=0。
        let text = "第一行\n第二行 abc\n第三行";
        let author = [7u8; 16];
        let rm = simple_text_document(text, &author);
        let runs = read_root_text_runs(&rm);
        assert_eq!(runs.len(), 1, "单 run");
        assert_eq!(runs[0].id, (1, 16), "首字符 CrdtId=(1,16)");
        assert_eq!(runs[0].char_off, 0);
        assert_eq!(runs[0].text, text, "run 文本==原文");
        // 与 read_root_text 全文一致
        assert_eq!(read_root_text(&rm), text);
    }

    #[test]
    fn anchor_offset_within_run() {
        let text = "abcdefghij"; // 10 chars, ids (1,16)..(1,25)
        let rm = simple_text_document(text, &[1u8; 16]);
        let runs = read_root_text_runs(&rm);
        // 首字符
        assert_eq!(anchor_char_offset((1, 16), &runs), Some(0));
        // 第 5 字符 'e'
        assert_eq!(anchor_char_offset((1, 20), &runs), Some(4));
        // 末字符 'j'
        assert_eq!(anchor_char_offset((1, 25), &runs), Some(9));
        // 越界（run 只到 25）
        assert_eq!(anchor_char_offset((1, 26), &runs), None);
        // part1 不匹配
        assert_eq!(anchor_char_offset((2, 20), &runs), None);
        // 哨兵
        assert_eq!(anchor_char_offset((0, 0xFFFF_FFFE), &runs), None);
        assert_eq!(anchor_char_offset((0, 0xFFFF_FFFF), &runs), None);
    }
}

