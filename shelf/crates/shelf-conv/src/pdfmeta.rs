//! 第三方 PDF 的页数：有界、按需从磁盘读（2026-09-30）。
//!
//! 给 book-serve 的「大文件通道」用（>90MB 的 PDF 先传占位、再在磁盘上换成真文件，页数要写进 `.content` 页表）。
//! 此前复用 [`crate::pdfwrite::PdfFileReader::page_count`]，它只认我们自己写的结构（传统 xref 表 + 对象 2 就是
//! `/Pages`），第三方 PDF 常见的交叉引用流（PDF 1.5+，`/Type /XRef`）、对象流（`/Type /ObjStm`）、页树根不在
//! 对象 2，都被整本拒收。
//!
//! 走法（PDF 1.7 规范 §7.5）：文件尾找 `startxref` → 沿 `/Prev` 链把每一节交叉引用登记下来（传统表只记子段位置，
//! 不读条目；交叉引用流只记字典和数据偏移）→ 最新 trailer 的 `/Root` → Catalog 的 `/Pages` → 它的 `/Count`。
//! 查某个对象时按"新节在前"逐节找它的位置，只读那一条；对象在对象流里就解压那一个流。
//!
//! **为什么不用 lopdf**：lopdf 0.45 的 `Document::load_metadata(path)` 也是先 `read_to_end` 整份文件进内存
//! （`reader.rs` `load_metadata_internal`），几百 MB 的书在 2GB 设备上不可接受。
//!
//! **内存上限**（全部是硬上限，超了就 `Err`，不分配）：
//! - 单个字典对象（Catalog/Pages/trailer/流字典）窗口最多 [`MAX_DICT_OBJ_BYTES`]（4MB，从 16KB 起按需翻倍）；
//! - 单个流（交叉引用流、对象流）压缩数据最多 [`MAX_STREAM_RAW`]，解压后最多 [`MAX_STREAM_DECODED`]；
//! - `/Prev` 链最多 [`MAX_XREF_SECTIONS`] 节、传统表子段总数最多 [`MAX_SUBSECTIONS`]；
//! - 解析出的数组/字典元素总数最多 [`MAX_STORED_ELEMS`]（超出的照常跳过、不存）、嵌套深度最多 [`MAX_DEPTH`]。
//!
//! 同一时刻峰值约 `MAX_STREAM_RAW + MAX_STREAM_DECODED`（48MB，只在交叉引用流 / 对象流巨大时出现；常见书几百 KB）。
//! 任何格式不符都返回 `Err`（调用方回退成整本拒收），不 panic、不死循环。
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 单个字典对象窗口上限（跟 `pdfwrite::MAX_DICT_OBJ_BYTES` 同值）。
pub const MAX_DICT_OBJ_BYTES: usize = 4 << 20;
/// 单个流的压缩数据上限。
pub const MAX_STREAM_RAW: usize = 16 << 20;
/// 单个流解压后的上限（交叉引用流每个对象一行、常见 5~7 字节：32MB 够四五百万个对象）。
pub const MAX_STREAM_DECODED: usize = 32 << 20;
/// `/Prev` 链节数上限（增量更新一次一节，正常书个位数）。
pub const MAX_XREF_SECTIONS: usize = 64;
/// 所有传统表子段总数上限。
pub const MAX_SUBSECTIONS: usize = 100_000;
/// 一次解析最多存这么多数组/字典元素（页树根的 `/Kids` 可能很长，我们不需要它的内容）。
const MAX_STORED_ELEMS: usize = 100_000;
const MAX_DEPTH: usize = 32;
/// 间接引用解析的嵌套上限（`/Length`、`/Count` 可能是引用，引用对象又可能在对象流里）。
const MAX_RESOLVE_DEPTH: usize = 8;
/// 页数上限：`.content` 页表每页一条带 UUID 的记录，天文数字页数不能照单写。
pub const MAX_PAGES: usize = 200_000;
const FIRST_WINDOW: usize = 16 << 10;

/// 按路径读页数。
pub fn page_count(path: &Path) -> Result<usize, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("打开 PDF 失败: {e}"))?;
    page_count_from(f)
}

/// 从任意可随机读的源读页数（测试用 `Cursor`）。
pub fn page_count_from<R: Read + Seek>(mut src: R) -> Result<usize, String> {
    let len = src.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let mut doc = Doc { src, len, sections: Vec::new(), subsections: 0, resolve_depth: 0 };
    let root = doc.load_sections()?;
    let catalog = doc.resolve(&root)?;
    let catalog = catalog.as_dict().ok_or("Root 不是字典")?;
    let pages_ref = get(catalog, b"Pages").ok_or("Catalog 缺 /Pages")?.clone();
    let pages = doc.resolve(&pages_ref)?;
    let pages = pages.as_dict().ok_or("/Pages 不是字典")?;
    match get(pages, b"Type") {
        Some(Obj::Name(n)) if n == b"Pages" => {}
        None if get(pages, b"Kids").is_some() => {}
        _ => return Err("Catalog 的 /Pages 指向的对象不是 /Type /Pages".into()),
    }
    let count = get(pages, b"Count").ok_or("/Pages 缺 /Count")?.clone();
    let count = doc.resolve(&count)?;
    match count {
        Obj::Int(n) if n > 0 && (n as u64) <= MAX_PAGES as u64 => Ok(n as usize),
        Obj::Int(n) => Err(format!("页数 {n} 不在合理范围（1..={MAX_PAGES}）")),
        _ => Err("/Count 不是整数".into()),
    }
}

// ───────────── 对象模型 + 解析器 ─────────────

/// 只保留判页数要用的信息：字符串、实数的值用不上，不存。
#[derive(Clone, Debug)]
enum Obj {
    Null,
    Bool,
    Int(i64),
    Real,
    Name(Vec<u8>),
    Str,
    Array(Vec<Obj>),
    Dict(Dict),
    /// 间接引用（只留对象号；代数号不参与定位，xref 按对象号查）。
    Ref(u32),
}
type Dict = Vec<(Vec<u8>, Obj)>;

impl Obj {
    fn as_dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) => Some(d),
            _ => None,
        }
    }
    fn as_int(&self) -> Option<i64> {
        match self {
            Obj::Int(n) => Some(*n),
            _ => None,
        }
    }
}

fn get<'a>(d: &'a Dict, key: &[u8]) -> Option<&'a Obj> {
    d.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[derive(Debug)]
enum PErr {
    /// 窗口里的字节不够（还没到文件尾）：调用方把窗口放大再试。
    Incomplete,
    Bad(String),
}
type PRes<T> = Result<T, PErr>;

fn bad<T>(msg: impl Into<String>) -> PRes<T> {
    Err(PErr::Bad(msg.into()))
}

fn is_ws(b: u8) -> bool {
    matches!(b, 0 | b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}
fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

struct Parser<'a> {
    buf: &'a [u8],
    pos: usize,
    /// `buf` 已经是完整数据（文件尾 / 对象流里切好的一段）：读到末尾就是真结束，不再报 `Incomplete`。
    complete: bool,
    budget: usize,
}

impl<'a> Parser<'a> {
    fn new(buf: &'a [u8], complete: bool) -> Self {
        Parser { buf, pos: 0, complete, budget: MAX_STORED_ELEMS }
    }
    fn eof<T>(&self) -> PRes<T> {
        if self.complete {
            bad("数据意外结束")
        } else {
            Err(PErr::Incomplete)
        }
    }
    fn peek(&self) -> Option<u8> {
        self.buf.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if is_ws(b) {
                self.pos += 1;
            } else if b == b'%' {
                while let Some(c) = self.peek() {
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }
    /// 连续的"常规字符"（非空白、非分隔符）。
    fn token(&mut self) -> &'a [u8] {
        let s = self.pos;
        while let Some(b) = self.peek() {
            if is_ws(b) || is_delim(b) {
                break;
            }
            self.pos += 1;
        }
        &self.buf[s..self.pos]
    }
    /// 读一个关键字并核对；token 顶到窗口末尾时可能被截断，按 `Incomplete` 处理。
    fn keyword(&mut self, kw: &[u8]) -> PRes<()> {
        self.skip_ws();
        let t = self.token();
        if self.pos >= self.buf.len() && !self.complete {
            return Err(PErr::Incomplete);
        }
        if t != kw {
            return bad(format!("期望 {}", String::from_utf8_lossy(kw)));
        }
        Ok(())
    }
    fn uint(&mut self) -> PRes<u64> {
        self.skip_ws();
        let t = self.token();
        if self.pos >= self.buf.len() && !self.complete {
            return Err(PErr::Incomplete);
        }
        if t.is_empty() || !t.iter().all(u8::is_ascii_digit) {
            return bad("期望非负整数");
        }
        std::str::from_utf8(t).ok().and_then(|s| s.parse().ok()).map_or_else(|| bad("整数溢出"), Ok)
    }

    fn object(&mut self, depth: usize) -> PRes<Obj> {
        if depth > MAX_DEPTH {
            return bad("对象嵌套过深");
        }
        self.skip_ws();
        let Some(b) = self.peek() else { return self.eof() };
        match b {
            b'/' => {
                self.pos += 1;
                Ok(Obj::Name(self.name_rest()))
            }
            b'(' => self.literal_string().map(|_| Obj::Str),
            b'<' => {
                if self.buf.get(self.pos + 1) == Some(&b'<') {
                    self.pos += 2;
                    self.dict_rest(depth).map(Obj::Dict)
                } else if self.pos + 1 >= self.buf.len() {
                    self.eof()
                } else {
                    self.hex_string().map(|_| Obj::Str)
                }
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    match self.peek() {
                        None => return self.eof(),
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Obj::Array(items));
                        }
                        _ => {
                            let o = self.object(depth + 1)?;
                            if self.budget > 0 {
                                self.budget -= 1;
                                items.push(o);
                            }
                        }
                    }
                }
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.number(),
            _ if is_delim(b) => bad(format!("意外的字符 {:?}", b as char)),
            _ => {
                let t = self.token();
                if self.pos >= self.buf.len() && !self.complete {
                    return Err(PErr::Incomplete);
                }
                match t {
                    b"true" | b"false" => Ok(Obj::Bool),
                    b"null" => Ok(Obj::Null),
                    _ => bad(format!("不认识的关键字 {}", String::from_utf8_lossy(t))),
                }
            }
        }
    }

    fn name_rest(&mut self) -> Vec<u8> {
        let raw = self.token();
        let mut out = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == b'#' && i + 3 <= raw.len() && raw[i + 1].is_ascii_hexdigit() && raw[i + 2].is_ascii_hexdigit() {
                let hex = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
                out.push(hex(raw[i + 1]) << 4 | hex(raw[i + 2]));
                i += 3;
                continue;
            }
            out.push(raw[i]);
            i += 1;
        }
        out
    }

    fn literal_string(&mut self) -> PRes<()> {
        self.pos += 1; // (
        let mut level = 1usize;
        while let Some(b) = self.peek() {
            self.pos += 1;
            match b {
                b'\\' => self.pos += 1,
                b'(' => level += 1,
                b')' => {
                    level -= 1;
                    if level == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        self.eof()
    }

    fn hex_string(&mut self) -> PRes<()> {
        self.pos += 1; // <
        while let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'>' {
                return Ok(());
            }
            if !(b.is_ascii_hexdigit() || is_ws(b)) {
                return bad("十六进制字符串里有非法字符");
            }
        }
        self.eof()
    }

    fn dict_rest(&mut self, depth: usize) -> PRes<Dict> {
        let mut d = Dict::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return self.eof(),
                Some(b'>') => {
                    if self.buf.get(self.pos + 1) == Some(&b'>') {
                        self.pos += 2;
                        return Ok(d);
                    }
                    if self.pos + 1 >= self.buf.len() {
                        return self.eof();
                    }
                    return bad("字典结尾不是 >>");
                }
                Some(b'/') => {
                    self.pos += 1;
                    let k = self.name_rest();
                    if self.pos >= self.buf.len() && !self.complete {
                        return Err(PErr::Incomplete);
                    }
                    let v = self.object(depth + 1)?;
                    if self.budget > 0 {
                        self.budget -= 1;
                        d.push((k, v));
                    }
                }
                Some(_) => return bad("字典键不是名字"),
            }
        }
    }

    /// 数字；非负整数后面紧跟 `<整数> R` 时是间接引用。
    fn number(&mut self) -> PRes<Obj> {
        let t = self.token();
        if self.pos >= self.buf.len() && !self.complete {
            return Err(PErr::Incomplete);
        }
        if t.contains(&b'.') {
            return if t.iter().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'+' | b'-')) { Ok(Obj::Real) } else { bad("数字格式不对") };
        }
        let n: i64 = std::str::from_utf8(t).ok().and_then(|s| s.parse().ok()).map_or_else(|| bad("整数格式不对"), Ok)?;
        if n < 0 || !t[0].is_ascii_digit() {
            return Ok(Obj::Int(n));
        }
        let save = self.pos;
        match self.ref_tail() {
            Ok(Some(())) if n <= u32::MAX as i64 => Ok(Obj::Ref(n as u32)),
            Ok(_) => {
                self.pos = save;
                Ok(Obj::Int(n))
            }
            Err(e) => Err(e),
        }
    }

    /// 看后面是不是 `<gen> R`：是 → `Some(())`；不是 → `None`（调用方回退位置）；窗口不够 → `Incomplete`。
    fn ref_tail(&mut self) -> PRes<Option<()>> {
        self.skip_ws();
        if self.peek().is_none() {
            return if self.complete { Ok(None) } else { Err(PErr::Incomplete) };
        }
        let g = self.token();
        if self.pos >= self.buf.len() && !self.complete {
            return Err(PErr::Incomplete);
        }
        if g.is_empty() || !g.iter().all(u8::is_ascii_digit) {
            return Ok(None);
        }
        if std::str::from_utf8(g).ok().and_then(|s| s.parse::<u16>().ok()).is_none() {
            return Ok(None);
        }
        self.skip_ws();
        if self.peek().is_none() {
            return if self.complete { Ok(None) } else { Err(PErr::Incomplete) };
        }
        let r = self.token();
        if self.pos >= self.buf.len() && !self.complete {
            return Err(PErr::Incomplete);
        }
        Ok((r == b"R").then_some(()))
    }
}

// ───────────── 交叉引用 ─────────────

enum Section {
    /// 传统表：每个子段 (起始对象号, 条目数, 条目区在文件里的偏移)。
    Table(Vec<(u32, u32, u64)>),
    /// 交叉引用流：字典 + 流数据在文件里的偏移。
    Stream(Dict, u64),
}

enum Loc {
    Offset(u64),
    InStream(u32, u32),
    Free,
}

struct Doc<R> {
    src: R,
    len: u64,
    /// 新节在前。
    sections: Vec<Section>,
    subsections: usize,
    resolve_depth: usize,
}

impl<R: Read + Seek> Doc<R> {
    fn read_at(&mut self, off: u64, max: usize) -> Result<Vec<u8>, String> {
        if off > self.len {
            return Err(format!("偏移 {off} 超出文件长度 {}", self.len));
        }
        let n = (self.len - off).min(max as u64) as usize;
        self.src.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; n];
        self.src.read_exact(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    }

    /// 从 `off` 起用逐步放大的窗口跑 `f`：窗口不够（`Incomplete`）且还没到文件尾/上限就翻 4 倍再来。
    fn with_window<T>(&mut self, off: u64, mut f: impl FnMut(&mut Parser) -> PRes<T>) -> Result<T, String> {
        let mut size = FIRST_WINDOW;
        loop {
            let buf = self.read_at(off, size)?;
            let complete = off + buf.len() as u64 >= self.len;
            let mut p = Parser::new(&buf, complete);
            match f(&mut p) {
                Ok(v) => return Ok(v),
                Err(PErr::Bad(e)) => return Err(format!("偏移 {off} 处：{e}")),
                Err(PErr::Incomplete) if complete || size >= MAX_DICT_OBJ_BYTES => {
                    return Err(format!("偏移 {off} 处的对象超出 {} 字节上限或被截断", MAX_DICT_OBJ_BYTES));
                }
                Err(PErr::Incomplete) => size = (size * 4).min(MAX_DICT_OBJ_BYTES),
            }
        }
    }

    /// 读 `off` 处的间接对象 `N G obj ...`：返回 (对象号, 对象, 流数据偏移——是流对象才有)。
    fn indirect_at(&mut self, off: u64) -> Result<(u32, Obj, Option<u64>), String> {
        self.with_window(off, |p| {
            let id = p.uint()?;
            p.uint()?;
            p.keyword(b"obj")?;
            let id = u32::try_from(id).map_err(|_| PErr::Bad("对象号溢出".into()))?;
            let o = p.object(0)?;
            p.skip_ws();
            let rest = &p.buf[p.pos..];
            let mut data = None;
            if rest.starts_with(b"stream") {
                let mut i = p.pos + 6;
                match (p.buf.get(i), p.buf.get(i + 1)) {
                    (None, _) => return p.eof(),
                    (Some(b'\r'), None) if !p.complete => return p.eof(),
                    (Some(b'\r'), Some(b'\n')) => i += 2,
                    (Some(b'\n' | b'\r'), _) => i += 1,
                    _ => return bad("stream 关键字后缺换行"),
                }
                data = Some(off + i as u64);
            } else if rest.len() < 6 && !p.complete && b"stream".starts_with(rest) {
                // 窗口正好截在 `stream` 关键字中间（或对象刚好顶到窗口尾）：放大窗口再看
                return Err(PErr::Incomplete);
            }
            Ok((id, o, data))
        })
    }

    /// 沿 `startxref` → `/Prev` 登记所有节，返回 `/Root`。
    fn load_sections(&mut self) -> Result<Obj, String> {
        let tail_len = self.len.min(2048);
        let tail = self.read_at(self.len - tail_len, tail_len as usize)?;
        let at = tail.windows(9).rposition(|w| w == b"startxref").ok_or("文件尾没找到 startxref")?;
        let mut p = Parser::new(&tail[at + 9..], true);
        let mut next = Some(p.uint().map_err(|_| "startxref 后缺偏移")?);
        let mut root = None;
        let mut seen = std::collections::HashSet::new();
        while let Some(off) = next.take() {
            if !seen.insert(off) {
                return Err("交叉引用 /Prev 链成环".into());
            }
            if self.sections.len() >= MAX_XREF_SECTIONS {
                return Err(format!("交叉引用超过 {MAX_XREF_SECTIONS} 节"));
            }
            let trailer = self.load_section(off)?;
            if root.is_none() {
                root = get(&trailer, b"Root").cloned();
            }
            // 混合式文件（传统表 + `/XRefStm`）：交叉引用流紧跟这张表之后查，先于 /Prev。
            if let Some(xs) = get(&trailer, b"XRefStm").and_then(Obj::as_int) {
                let xs = u64::try_from(xs).map_err(|_| "XRefStm 偏移非法")?;
                if seen.insert(xs) {
                    let (_, o, data) = self.indirect_at(xs)?;
                    let d = o.as_dict().cloned().ok_or("XRefStm 不是流")?;
                    self.sections.push(Section::Stream(d, data.ok_or("XRefStm 不是流")?));
                }
            }
            next = match get(&trailer, b"Prev") {
                None => None,
                Some(Obj::Int(n)) if *n >= 0 => Some(*n as u64),
                Some(_) => return Err("/Prev 不是非负整数".into()),
            };
        }
        root.ok_or_else(|| "trailer 缺 /Root".to_string())
    }

    /// 登记 `off` 处的一节，返回它的 trailer 字典（交叉引用流就是流字典本身）。
    fn load_section(&mut self, off: u64) -> Result<Dict, String> {
        let head = self.read_at(off, 64)?;
        let start = head.iter().position(|b| !is_ws(*b)).unwrap_or(head.len());
        if head[start..].starts_with(b"xref") {
            return self.load_table(off + start as u64 + 4);
        }
        let (_, o, data) = self.indirect_at(off)?;
        let d = o.as_dict().cloned().ok_or("startxref 指向的既不是 xref 表也不是交叉引用流")?;
        if !matches!(get(&d, b"Type"), Some(Obj::Name(n)) if n == b"XRef") {
            return Err("startxref 指向的对象不是 /Type /XRef".into());
        }
        let data = data.ok_or("交叉引用流缺 stream 数据")?;
        self.sections.push(Section::Stream(d.clone(), data));
        Ok(d)
    }

    /// 传统表：只读子段头、跳过条目区（每条固定 20 字节），不把条目读进内存。
    fn load_table(&mut self, mut cur: u64) -> Result<Dict, String> {
        let mut subs = Vec::new();
        loop {
            let head = self.read_at(cur, 128)?;
            let mut p = Parser::new(&head, true);
            p.skip_ws();
            if head[p.pos..].starts_with(b"trailer") {
                let t = cur + p.pos as u64 + 7;
                let d = self.with_window(t, |p| match p.object(0)? {
                    Obj::Dict(d) => Ok(d),
                    _ => bad("trailer 不是字典"),
                })?;
                self.sections.push(Section::Table(subs));
                return Ok(d);
            }
            let first = p.uint().map_err(|_| "xref 子段头格式不对")?;
            let count = p.uint().map_err(|_| "xref 子段头格式不对")?;
            p.skip_ws(); // 条目以数字开头，跳空白不会吃掉条目
            let (Ok(first), Ok(count)) = (u32::try_from(first), u32::try_from(count)) else { return Err("xref 子段头数值溢出".into()) };
            let entries = cur + p.pos as u64;
            let end = (count as u64).checked_mul(20).and_then(|n| n.checked_add(entries)).filter(|&e| e <= self.len).ok_or("xref 条目区超出文件")?;
            self.subsections += 1;
            if self.subsections > MAX_SUBSECTIONS {
                return Err(format!("xref 子段超过 {MAX_SUBSECTIONS} 个"));
            }
            subs.push((first, count, entries));
            cur = end;
        }
    }

    /// 按"新节在前"找对象 `id` 的位置；某节标记为空闲就继续往旧的找（混合式文件的表把流里的对象记作空闲）。
    fn locate(&mut self, id: u32) -> Result<Loc, String> {
        for i in 0..self.sections.len() {
            let loc = match &self.sections[i] {
                Section::Table(subs) => {
                    let hit = subs.iter().find(|(s, c, _)| id >= *s && (id - *s) < *c).map(|(s, _, off)| off + (id - s) as u64 * 20);
                    match hit {
                        None => None,
                        Some(at) => {
                            let e = self.read_at(at, 20)?;
                            if e.len() < 18 {
                                return Err("xref 条目被截断".into());
                            }
                            let off = std::str::from_utf8(&e[..10]).ok().and_then(|s| s.trim().parse::<u64>().ok()).ok_or("xref 条目偏移格式不对")?;
                            match e[17] {
                                b'n' => Some(Loc::Offset(off)),
                                b'f' => Some(Loc::Free),
                                _ => return Err("xref 条目类型不认识".into()),
                            }
                        }
                    }
                }
                Section::Stream(d, data) => {
                    let (d, data) = (d.clone(), *data);
                    self.locate_in_stream(&d, data, id)?
                }
            };
            match loc {
                Some(Loc::Free) | None => continue,
                Some(l) => return Ok(l),
            }
        }
        Err(format!("交叉引用里找不到对象 {id}"))
    }

    fn locate_in_stream(&mut self, d: &Dict, data: u64, id: u32) -> Result<Option<Loc>, String> {
        let w: Vec<usize> = match get(d, b"W") {
            Some(Obj::Array(a)) if a.len() == 3 => {
                a.iter().map(|o| o.as_int().filter(|n| (0..=8).contains(n)).map(|n| n as usize)).collect::<Option<_>>().ok_or("交叉引用流 /W 非法")?
            }
            _ => return Err("交叉引用流 /W 非法".into()),
        };
        let row = w.iter().sum::<usize>();
        if row == 0 || w[1] == 0 {
            return Err("交叉引用流 /W 非法".into());
        }
        let size = get(d, b"Size").and_then(Obj::as_int).ok_or("交叉引用流缺 /Size")?;
        let ranges: Vec<(i64, i64)> = match get(d, b"Index") {
            None => vec![(0, size)],
            Some(Obj::Array(a)) if a.len() % 2 == 0 => {
                a.chunks(2).map(|c| Some((c[0].as_int()?, c[1].as_int()?))).collect::<Option<_>>().ok_or("交叉引用流 /Index 非法")?
            }
            _ => return Err("交叉引用流 /Index 非法".into()),
        };
        let mut before: u64 = 0;
        let mut hit = None;
        for (s, c) in ranges {
            if s < 0 || c < 0 {
                return Err("交叉引用流 /Index 非法".into());
            }
            let id = id as i64;
            if id >= s && id - s < c {
                hit = Some(before + (id - s) as u64);
                break;
            }
            before = before.saturating_add(c as u64);
        }
        let Some(r) = hit else { return Ok(None) };
        let bytes = self.stream_data(d, data)?;
        let at = (r as usize).checked_mul(row).filter(|&a| a + row <= bytes.len()).ok_or("交叉引用流数据比 /Index 声明的短")?;
        let field = |k: usize| -> u64 {
            let s = at + w[..k].iter().sum::<usize>();
            bytes[s..s + w[k]].iter().fold(0u64, |acc, b| (acc << 8) | *b as u64)
        };
        let ty = if w[0] == 0 { 1 } else { field(0) };
        Ok(match ty {
            0 => Some(Loc::Free),
            1 => Some(Loc::Offset(field(1))),
            2 => {
                let stm = u32::try_from(field(1)).map_err(|_| "对象流编号溢出")?;
                let idx = u32::try_from(field(2)).map_err(|_| "对象流下标溢出")?;
                Some(Loc::InStream(stm, idx))
            }
            _ => None, // 规范：未知类型当作空引用
        })
    }

    /// 读一个流的数据（按 `/Length`，可能是间接引用）并按 `/Filter` 解码。只支持无过滤 / FlateDecode（+PNG 预测器）。
    fn stream_data(&mut self, d: &Dict, data: u64) -> Result<Vec<u8>, String> {
        let len = get(d, b"Length").ok_or("流缺 /Length")?.clone();
        let len = match self.resolve(&len)? {
            Obj::Int(n) if n >= 0 => n as u64,
            _ => return Err("流 /Length 非法".into()),
        };
        if len > MAX_STREAM_RAW as u64 {
            return Err(format!("流数据 {len} 字节超出 {MAX_STREAM_RAW} 上限"));
        }
        if data.checked_add(len).is_none_or(|e| e > self.len) {
            return Err("流数据超出文件".into());
        }
        let raw = self.read_at(data, len as usize)?;
        let filters: Vec<Vec<u8>> = match get(d, b"Filter") {
            None => vec![],
            Some(Obj::Name(n)) => vec![n.clone()],
            Some(Obj::Array(a)) => a.iter().map(|o| if let Obj::Name(n) = o { Ok(n.clone()) } else { Err("流 /Filter 非法") }).collect::<Result<_, _>>()?,
            _ => return Err("流 /Filter 非法".into()),
        };
        let parms = match get(d, b"DecodeParms") {
            Some(Obj::Dict(p)) => Some(p.clone()),
            Some(Obj::Array(a)) => a.first().and_then(Obj::as_dict).cloned(),
            _ => None,
        };
        match filters.as_slice() {
            [] => Ok(raw),
            [f] if f == b"FlateDecode" || f == b"Fl" => {
                let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&raw, MAX_STREAM_DECODED).map_err(|e| {
                    if e.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput {
                        format!("流解压后超出 {MAX_STREAM_DECODED} 字节上限")
                    } else {
                        format!("流解压失败: {:?}", e.status)
                    }
                })?;
                drop(raw);
                unpredict(out, parms.as_ref())
            }
            _ => Err("流用了不支持的过滤器（只认 FlateDecode）".into()),
        }
    }

    /// 间接引用 → 对象（非引用原样返回）。
    fn resolve(&mut self, o: &Obj) -> Result<Obj, String> {
        let Obj::Ref(id) = *o else { return Ok(o.clone()) };
        if self.resolve_depth >= MAX_RESOLVE_DEPTH {
            return Err("间接引用嵌套过深".into());
        }
        self.resolve_depth += 1;
        let r = self.resolve_inner(id);
        self.resolve_depth -= 1;
        r
    }

    fn resolve_inner(&mut self, id: u32) -> Result<Obj, String> {
        match self.locate(id)? {
            Loc::Offset(off) => {
                let (got, o, _) = self.indirect_at(off)?;
                if got != id {
                    return Err(format!("xref 说对象 {id} 在偏移 {off}，那里却是对象 {got}"));
                }
                Ok(o)
            }
            Loc::InStream(stm, idx) => {
                let Loc::Offset(off) = self.locate(stm)? else { return Err(format!("对象流 {stm} 位置非法")) };
                let (got, o, data) = self.indirect_at(off)?;
                if got != stm {
                    return Err(format!("对象流 {stm} 偏移处是对象 {got}"));
                }
                let d = o.as_dict().cloned().ok_or("对象流不是流")?;
                if !matches!(get(&d, b"Type"), Some(Obj::Name(n)) if n == b"ObjStm") {
                    return Err(format!("对象 {stm} 不是 /Type /ObjStm"));
                }
                let n = get(&d, b"N").and_then(Obj::as_int).filter(|n| *n >= 0).ok_or("对象流缺 /N")? as u64;
                let first = get(&d, b"First").and_then(Obj::as_int).filter(|n| *n >= 0).ok_or("对象流缺 /First")? as usize;
                if idx as u64 >= n {
                    return Err(format!("对象流 {stm} 只有 {n} 个对象，下标 {idx} 越界"));
                }
                let bytes = self.stream_data(&d, data.ok_or("对象流缺 stream 数据")?)?;
                if first > bytes.len() {
                    return Err("对象流 /First 超出数据".into());
                }
                let mut p = Parser::new(&bytes[..first], true);
                let pair = |p: &mut Parser| -> Result<(u64, u64), String> {
                    let a = p.uint().map_err(|_| "对象流头格式不对")?;
                    let b = p.uint().map_err(|_| "对象流头格式不对")?;
                    Ok((a, b))
                };
                let mut hit = None;
                for i in 0..=idx {
                    let (num, rel) = pair(&mut p)?;
                    if i == idx {
                        let next = pair(&mut p).ok().map(|(_, r)| r);
                        hit = Some((num, rel, next));
                    }
                }
                let (num, rel, next) = hit.ok_or("对象流头读不到")?;
                if num != id as u64 {
                    return Err(format!("对象流 {stm} 第 {idx} 项是对象 {num}，不是 {id}"));
                }
                let s = first.checked_add(rel as usize).filter(|&s| s <= bytes.len()).ok_or("对象流内偏移越界")?;
                let e = next.map(|r| first.saturating_add(r as usize)).filter(|&e| e >= s && e <= bytes.len()).unwrap_or(bytes.len());
                Parser::new(&bytes[s..e], true).object(0).map_err(|e| match e {
                    PErr::Bad(m) => format!("对象流里的对象 {id}：{m}"),
                    PErr::Incomplete => format!("对象流里的对象 {id} 被截断"),
                })
            }
            Loc::Free => Err(format!("对象 {id} 是空闲条目")),
        }
    }
}

/// 解 PNG 预测器（Predictor ≥ 10）；1/缺省原样；TIFF 预测器（2）不支持。
fn unpredict(data: Vec<u8>, parms: Option<&Dict>) -> Result<Vec<u8>, String> {
    let int = |k: &[u8], dflt: i64| parms.and_then(|p| get(p, k)).and_then(Obj::as_int).unwrap_or(dflt);
    let pred = int(b"Predictor", 1);
    if pred == 1 {
        return Ok(data);
    }
    if pred < 10 {
        return Err(format!("不支持的预测器 {pred}"));
    }
    let (colors, bpc, cols) = (int(b"Colors", 1), int(b"BitsPerComponent", 8), int(b"Columns", 1));
    if !(1..=32).contains(&colors) || ![1, 2, 4, 8, 16].contains(&bpc) || !(1..=1 << 20).contains(&cols) {
        return Err("预测器参数非法".into());
    }
    let bpp = ((colors * bpc + 7) / 8) as usize;
    let rowlen = ((colors * bpc * cols + 7) / 8) as usize;
    if rowlen > 1 << 16 {
        return Err("预测器行宽过大".into());
    }
    let mut out = Vec::with_capacity(data.len() / (rowlen + 1) * rowlen);
    let mut prev = vec![0u8; rowlen];
    for chunk in data.chunks(rowlen + 1) {
        if chunk.len() < rowlen + 1 {
            break; // 不完整的尾行丢掉
        }
        let (ft, src) = (chunk[0], &chunk[1..]);
        let mut cur = vec![0u8; rowlen];
        for i in 0..rowlen {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            let x = src[i];
            cur[i] = match ft {
                0 => x,
                1 => x.wrapping_add(a),
                2 => x.wrapping_add(b),
                3 => x.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
                    x.wrapping_add(if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c })
                }
                _ => return Err(format!("PNG 预测器行类型 {ft} 非法")),
            };
        }
        out.extend_from_slice(&cur);
        prev = cur;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
