//! 流式 multipart/form-data 解析。每个 part 以 `Read` 形式交给调用方（边读边落盘），
//! 内存只保留一个读缓冲；多文件一次 POST 也不会把整个请求体读进内存（设备 MemoryMax 友好）。
//!
//! 用法：
//! ```ignore
//! let mut mp = MultipartReader::new(body, boundary);
//! while let Some(mut part) = mp.next_part()? {
//!     if let Some(fname) = part.filename.clone() { std::io::copy(&mut part, &mut file)?; }
//! }
//! ```
use std::io::{self, Read};

const CHUNK: usize = 64 * 1024;
const MAX_HEADER: usize = 16 * 1024;

/// 把一个 part（或任意 `Read`）流式写到 `dest` 文件，返回写入字节数（0＝空文件，是否算失败由调用方定）。
/// 单点化"建文件 + io::copy"这块——AssetUploadFlow 与 book-serve 的 spool 落盘共用（各自再决定空文件/归档语义）。
/// 错误不加前缀，调用方按场景包装（如 "接收失败: …"）。
pub fn receive_part_to<R: Read>(dest: &std::path::Path, part: &mut R) -> Result<u64, String> {
    let mut f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    io::copy(part, &mut f).map_err(|e| e.to_string())
}

/// 从 Content-Type 里取 boundary。
pub fn boundary_of(content_type: &str) -> Option<String> {
    let ct = content_type.trim();
    if !ct.to_ascii_lowercase().starts_with("multipart/form-data") {
        return None;
    }
    ct.split(';').map(|s| s.trim()).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k.trim().eq_ignore_ascii_case("boundary") {
            let v = v.trim().trim_matches('"');
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        } else {
            None
        }
    })
}

pub struct MultipartReader<R: Read> {
    r: R,
    delim: Vec<u8>, // "\r\n--<boundary>"
    buf: Vec<u8>,
    start: usize,
    eof: bool,
    done: bool,
    in_body: bool,
    started: bool,
    /// body 分隔符扫描进度（`buf` 里的绝对下标）：`[start, clean_to)` 内已确认没有分隔符起点。
    /// 消费方每次只取一小口（`io::copy` 8KB），此前每次都从头重扫整个缓冲（64KB+）找分隔符，
    /// 同一段字节被扫 8 遍；记住进度后每个字节只扫一次（2026-09-22 审计，200MB body 实测见 `find`）。
    clean_to: usize,
    /// 已在缓冲里定位到的分隔符起点（绝对下标）；分隔符之前的 body 分多口取走期间不必重找。
    found: Option<usize>,
}

/// 一个 part 的头信息；`Read` 读到该 part 结束返回 0。
pub struct Part<'a, R: Read> {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    mp: &'a mut MultipartReader<R>,
}

impl<R: Read> MultipartReader<R> {
    pub fn new(r: R, boundary: &str) -> Self {
        let mut delim = b"\r\n--".to_vec();
        delim.extend_from_slice(boundary.as_bytes());
        MultipartReader { r, delim, buf: Vec::with_capacity(CHUNK * 2), start: 0, eof: false, done: false, in_body: false, started: false, clean_to: 0, found: None }
    }

    fn avail(&self) -> &[u8] {
        &self.buf[self.start..]
    }

    /// 读一块进缓冲；返回是否读到新数据。
    fn fill(&mut self) -> io::Result<bool> {
        if self.eof {
            return Ok(false);
        }
        if self.start > 0 {
            self.buf.drain(..self.start);
            self.clean_to = self.clean_to.saturating_sub(self.start);
            self.found = self.found.map(|f| f.saturating_sub(self.start));
            self.start = 0;
        }
        // 读进栈上临时块再追加：避免每次 resize 清零 64KB（逐字节到达的流会被 memset 拖死）。
        let mut tmp = [0u8; CHUNK];
        let n = self.r.read(&mut tmp)?;
        self.buf.extend_from_slice(&tmp[..n]);
        if n == 0 {
            self.eof = true;
        }
        Ok(n > 0)
    }

    fn consume(&mut self, n: usize) {
        self.start += n;
    }

    /// 子串查找：先按首字节跳（二进制 body 里 `\r` 只占 1/256），命中再比整段。
    /// 比逐窗口 `windows().position(==)` 快数倍（后者每个位置一次 memcmp 调用）。
    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || hay.len() < needle.len() {
            return None;
        }
        let first = needle[0];
        let last_start = hay.len() - needle.len();
        let mut i = 0;
        while i <= last_start {
            i += hay[i..=last_start].iter().position(|&b| b == first)?;
            if &hay[i..i + needle.len()] == needle {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// body 分隔符在 `avail()` 里的相对位置；只扫 `clean_to` 之后的新字节，见字段说明。
    fn locate_delim(&mut self) -> Option<usize> {
        if let Some(f) = self.found {
            return Some(f - self.start);
        }
        let from = self.clean_to.clamp(self.start, self.buf.len());
        match Self::find(&self.buf[from..], &self.delim) {
            Some(k) => {
                self.found = Some(from + k);
                Some(from + k - self.start)
            }
            None => {
                // 尾部 dlen-1 字节可能是分隔符的前半截，下次补数据后要重扫。
                self.clean_to = self.buf.len().saturating_sub(self.delim.len() - 1).max(from);
                None
            }
        }
    }

    /// 跳过当前 part 未读完的 body（调用方没读完就要下一个 part）。
    fn drain_body(&mut self) -> io::Result<()> {
        let mut sink = io::sink();
        while self.in_body {
            let mut tmp = [0u8; 4096];
            let n = self.read_body(&mut tmp)?;
            if n == 0 {
                break;
            }
            io::Write::write_all(&mut sink, &tmp[..n])?;
        }
        Ok(())
    }

    /// 定位到首个 boundary（"--boundary" 可在流开头，无前导 CRLF）。
    fn skip_preamble(&mut self) -> io::Result<()> {
        let first = self.delim[2..].to_vec(); // "--boundary"
        loop {
            if let Some(i) = Self::find(self.avail(), &first) {
                self.consume(i + first.len());
                return self.after_boundary();
            }
            // 保留尾部可能的半个 boundary
            let keep = first.len().saturating_sub(1);
            let drop_n = self.avail().len().saturating_sub(keep);
            self.consume(drop_n);
            if !self.fill()? {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "multipart: 找不到起始 boundary"));
            }
        }
    }

    /// boundary 之后：`--` 收尾 / `\r\n` 进入下一 part 头。
    fn after_boundary(&mut self) -> io::Result<()> {
        while self.avail().len() < 2 {
            if !self.fill()? {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "multipart: boundary 后截断"));
            }
        }
        let a = self.avail();
        if &a[..2] == b"--" {
            self.done = true;
            self.consume(2);
        } else if &a[..2] == b"\r\n" {
            self.consume(2);
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "multipart: boundary 后非法字节"));
        }
        Ok(())
    }

    /// 下一个 part；流结束返回 None。
    pub fn next_part(&mut self) -> io::Result<Option<Part<'_, R>>> {
        if self.in_body {
            self.drain_body()?;
        }
        if !self.started {
            self.started = true;
            self.skip_preamble()?;
        }
        if self.done {
            return Ok(None);
        }
        // 读头到 \r\n\r\n
        let header_end = loop {
            if let Some(i) = Self::find(self.avail(), b"\r\n\r\n") {
                break i;
            }
            if self.avail().len() > MAX_HEADER {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "multipart: part 头过长"));
            }
            if !self.fill()? {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "multipart: part 头截断"));
            }
        };
        let head = String::from_utf8_lossy(&self.avail()[..header_end]).to_string();
        self.consume(header_end + 4);
        let (name, filename, content_type) = parse_headers(&head);
        self.in_body = true;
        Ok(Some(Part { name, filename, content_type, mp: self }))
    }

    /// 读当前 part 的 body。到 part 结束返回 0 并推进到 boundary 之后。
    fn read_body(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.in_body || out.is_empty() {
            return Ok(0);
        }
        loop {
            let dlen = self.delim.len();
            if let Some(i) = self.locate_delim() {
                if i > 0 {
                    let n = i.min(out.len());
                    out[..n].copy_from_slice(&self.avail()[..n]);
                    self.consume(n);
                    return Ok(n);
                }
                // body 读尽：跨过 delim，看结尾/下一 part
                self.consume(dlen);
                self.found = None;
                self.in_body = false;
                self.after_boundary()?;
                return Ok(0);
            }
            // 未见 delim：除最后 dlen-1 字节外都是安全 body
            let safe = self.avail().len().saturating_sub(dlen - 1);
            if safe > 0 {
                let n = safe.min(out.len());
                out[..n].copy_from_slice(&self.avail()[..n]);
                self.consume(n);
                return Ok(n);
            }
            if !self.fill()? {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "multipart: part body 未终止"));
            }
        }
    }
}

impl<R: Read> Read for Part<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.mp.read_body(out)
    }
}

/// 按 `;` 切分头参数，**引号内的 `;` 不切**：此前直接 `split(';')`，浏览器发来的 `filename="甲; 乙.epub"` 被切成
/// `"甲`，书名截半、扩展名丢失被当成"不支持的格式"拒收（2026-09-25 第四轮审计）。浏览器对文件名里的 `"` 编成 `%22`、
/// 不用反斜杠转义，所以这里只认成对的双引号。
fn split_params(line: &str) -> impl Iterator<Item = &str> {
    let mut parts = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&line[start..]);
    parts.into_iter()
}

fn header_param(line: &str, key: &str) -> Option<String> {
    split_params(line).map(|s| s.trim()).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k.trim().eq_ignore_ascii_case(key) {
            Some(v.trim().trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn parse_headers(head: &str) -> (String, Option<String>, Option<String>) {
    let mut name = String::new();
    let mut filename = None;
    let mut ctype = None;
    for line in head.split("\r\n") {
        let Some((k, v)) = line.split_once(':') else { continue };
        let k = k.trim().to_ascii_lowercase();
        if k == "content-disposition" {
            name = header_param(v, "name").unwrap_or_default();
            // RFC 5987 filename* 优先；否则 filename
            filename = header_param(v, "filename*")
                .and_then(|s| s.rsplit_once("''").map(|(_, enc)| percent_decode_path(enc)))
                .or_else(|| header_param(v, "filename"));
        } else if k == "content-type" {
            ctype = Some(v.trim().to_string());
        }
    }
    (name, filename, ctype)
}

// 百分号编解码 2026-10-10 搬到 `http::encoding`（路由、查询串、网关跨模块用的是它，不是 multipart 的事），
// 旧路径 `multipart::percent_*` 的再导出第二阶段已删。
use crate::http::{percent_decode_path, percent_encode};

/// 下载响应的 `Content-Disposition`：ASCII 兜底名（非 ASCII 与 `"` 换成 `_`）+ RFC 5987 的 UTF-8 真名。
/// 笔记导出（note-serve）与母版库原件下载（book-serve）共用。
pub fn content_disposition(filename: &str) -> String {
    let ascii: String = filename.chars().map(|c| if c.is_ascii() && c != '"' && !c.is_ascii_control() { c } else { '_' }).collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{}", percent_encode(filename))
}

/// 只取文件名的 basename（防路径穿越），空则用 default。
pub fn safe_basename(filename: &str, default: &str) -> String {
    let base = filename.replace('\\', "/");
    let base = base.rsplit('/').next().unwrap_or("").trim();
    if base.is_empty() || base == "." || base == ".." {
        default.to_string()
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(boundary: &str, parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
        let mut v = Vec::new();
        for (name, fname, data) in parts {
            v.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"").as_bytes());
            if let Some(f) = fname {
                v.extend_from_slice(format!("; filename=\"{f}\"\r\nContent-Type: application/octet-stream").as_bytes());
            }
            v.extend_from_slice(b"\r\n\r\n");
            v.extend_from_slice(data);
            v.extend_from_slice(b"\r\n");
        }
        v.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        v
    }

    /// 每次只吐 n 字节的 Reader，逼出跨块边界。
    struct Trickle<'a> {
        d: &'a [u8],
        pos: usize,
        n: usize,
    }
    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let k = self.n.min(out.len()).min(self.d.len() - self.pos);
            out[..k].copy_from_slice(&self.d[self.pos..self.pos + k]);
            self.pos += k;
            Ok(k)
        }
    }

    fn collect(mut mp: MultipartReader<impl Read>) -> Vec<(String, Option<String>, Vec<u8>)> {
        let mut out = Vec::new();
        while let Some(mut p) = mp.next_part().unwrap() {
            let mut d = Vec::new();
            p.read_to_end(&mut d).unwrap();
            out.push((p.name.clone(), p.filename.clone(), d));
        }
        out
    }

    #[test]
    fn boundary_extraction() {
        assert_eq!(boundary_of("multipart/form-data; boundary=abc"), Some("abc".into()));
        assert_eq!(boundary_of("multipart/form-data; boundary=\"q q\""), Some("q q".into()));
        assert_eq!(boundary_of("application/json"), None);
    }

    #[test]
    fn multiple_files_with_binary_crlf_bytes() {
        let bin: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).chain(b"\r\n--not-a-boundary\r\n".iter().copied()).collect();
        let b = body("XyZ", &[("target", None, b"native"), ("file", Some("a.epub"), &bin), ("file", Some("b.pdf"), b"%PDF")]);
        let got = collect(MultipartReader::new(&b[..], "XyZ"));
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], ("target".into(), None, b"native".to_vec()));
        assert_eq!(got[1].1.as_deref(), Some("a.epub"));
        assert_eq!(got[1].2, bin);
        assert_eq!(got[2].2, b"%PDF");
    }

    #[test]
    fn boundary_split_across_read_chunks() {
        let big: Vec<u8> = vec![b'x'; 200_000];
        let b = body("bnd", &[("file", Some("x.bin"), &big), ("file", Some("y.bin"), b"yy")]);
        for n in [1usize, 3, 7, 64, 1000, 65537] {
            let got = collect(MultipartReader::new(Trickle { d: &b, pos: 0, n }, "bnd"));
            assert_eq!(got.len(), 2, "chunk {n}");
            assert_eq!(got[0].2.len(), 200_000, "chunk {n}");
            assert_eq!(got[1].2, b"yy");
        }
    }

    #[test]
    fn skipping_unread_part_still_reaches_next() {
        let b = body("q", &[("file", Some("a"), b"aaaa"), ("file", Some("b"), b"bb")]);
        let mut mp = MultipartReader::new(&b[..], "q");
        let p = mp.next_part().unwrap().unwrap();
        assert_eq!(p.filename.as_deref(), Some("a"));
        drop(p);
        let mut p2 = mp.next_part().unwrap().unwrap();
        let mut d = Vec::new();
        p2.read_to_end(&mut d).unwrap();
        assert_eq!(d, b"bb");
        assert!(mp.next_part().unwrap().is_none());
    }

    #[test]
    fn truncated_body_errors() {
        let mut b = body("q", &[("file", Some("a"), b"aaaa")]);
        b.truncate(b.len() - 8);
        let mut mp = MultipartReader::new(&b[..], "q");
        let mut p = mp.next_part().unwrap().unwrap();
        let mut d = Vec::new();
        assert!(p.read_to_end(&mut d).is_err());
    }

    #[test]
    fn filename_star_and_basename_safety() {
        let head = "Content-Disposition: form-data; name=\"file\"; filename=\"x.txt\"; filename*=UTF-8''%E4%B8%AD.epub";
        let (_, f, _) = parse_headers(head);
        assert_eq!(f.as_deref(), Some("中.epub"));
        assert_eq!(safe_basename("../../etc/passwd", "d"), "passwd");
        assert_eq!(safe_basename("C:\\x\\y.epub", "d"), "y.epub");
        assert_eq!(safe_basename("..", "d"), "d");
        assert_eq!(safe_basename("", "d"), "d");
    }

    /// 回归：引号里的 `;` 不是参数分隔符（书名带分号的上传曾被截成半截、扩展名丢失被拒收）。
    #[test]
    fn filename_with_semicolon_inside_quotes() {
        let (n, f, _) = parse_headers("Content-Disposition: form-data; name=\"file\"; filename=\"甲; 乙=丙.epub\"");
        assert_eq!((n.as_str(), f.as_deref()), ("file", Some("甲; 乙=丙.epub")));
        let (_, f, _) = parse_headers("Content-Disposition: form-data; filename=\"a;b.pdf\"; name=\"x\"");
        assert_eq!(f.as_deref(), Some("a;b.pdf"));
        let (n, f, _) = parse_headers("Content-Disposition: form-data; name=plain; filename=x.epub");
        assert_eq!((n.as_str(), f.as_deref()), ("plain", Some("x.epub")), "不带引号的旧写法照旧");
    }

    #[test]
    fn filename_star_keeps_plus() {
        let (_, f, _) = parse_headers("Content-Disposition: form-data; name=\"file\"; filename*=UTF-8''C++.epub");
        assert_eq!(f.as_deref(), Some("C++.epub"));
    }

    /// 差分测试：body 里塞满 `\r`、`\n`、`-` 和"差一点就是分隔符"的片段，用不同大小的到达块 + 不同大小的
    /// 读缓冲读，逐字节对拍。专门覆盖"分隔符扫描进度缓存"在跨块/分多口取走/分隔符在缓冲尾部时的正确性。
    #[test]
    fn scan_cache_matches_reference_on_adversarial_bodies() {
        let mut seed = 0x9E3779B97F4A7C15u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let frags: [&[u8]; 8] = [b"\r", b"\n", b"-", b"\r\n", b"\r\n--", b"\r\n--bnd", b"\r\n--bn", b"x"];
        for round in 0..40 {
            let mut a = Vec::new();
            let mut c = Vec::new();
            for _ in 0..(rnd() % 3000 + 1) {
                a.extend_from_slice(frags[(rnd() % 8) as usize]);
            }
            for _ in 0..(rnd() % 200) {
                c.extend_from_slice(frags[(rnd() % 8) as usize]);
            }
            // 真分隔符只能出现在 part 之间：数据里的 "\r\n--bnd" 后面必须不是 "\r\n" / "--"，否则它就是合法分隔符，
            // 语义上会把 body 截断——那不是要测的场景，这里把这类片段替换成不会与分隔符冲突的形状。
            let sanitize = |v: &[u8]| -> Vec<u8> {
                let mut s = String::from_utf8_lossy(v).to_string();
                while s.contains("\r\n--bnd") {
                    s = s.replace("\r\n--bnd", "\r\n--bnX");
                }
                s.into_bytes()
            };
            let (a, c) = (sanitize(&a), sanitize(&c));
            let b = body("bnd", &[("f", Some("a.bin"), &a), ("g", Some("c.bin"), &c)]);
            for chunk in [1usize, 2, 5, 13, 100, 4096, 70_000] {
                for outsz in [1usize, 3, 8, 1000, 100_000] {
                    let mut mp = MultipartReader::new(Trickle { d: &b, pos: 0, n: chunk }, "bnd");
                    let mut got: Vec<Vec<u8>> = Vec::new();
                    while let Some(mut p) = mp.next_part().unwrap() {
                        let mut d = Vec::new();
                        let mut buf = vec![0u8; outsz];
                        loop {
                            let n = p.read(&mut buf).unwrap();
                            if n == 0 {
                                break;
                            }
                            d.extend_from_slice(&buf[..n]);
                        }
                        got.push(d);
                    }
                    assert_eq!(got.len(), 2, "round {round} chunk {chunk} out {outsz}");
                    assert!(got[0] == a && got[1] == c, "round {round} chunk {chunk} out {outsz}: body 不一致");
                }
            }
        }
    }

    #[test]
    fn find_handles_edges() {
        assert_eq!(MultipartReader::<&[u8]>::find(b"abcabd", b"abd"), Some(3));
        assert_eq!(MultipartReader::<&[u8]>::find(b"aab", b"ab"), Some(1), "首字节命中但整段不符后要继续");
        assert_eq!(MultipartReader::<&[u8]>::find(b"ab", b"abc"), None);
        assert_eq!(MultipartReader::<&[u8]>::find(b"", b"a"), None);
        assert_eq!(MultipartReader::<&[u8]>::find(b"abc", b"abc"), Some(0));
        assert_eq!(MultipartReader::<&[u8]>::find(b"xxabc", b"abc"), Some(2), "命中在最后一个可能起点");
    }

    /// 下载文件名形状（笔记导出、母版库原件下载共用；从 note-serve 挪来）：非 ASCII 换 `_` 的兜底名 + RFC 5987 UTF-8 真名。
    #[test]
    fn content_disposition_gives_ascii_fallback_and_rfc5987_utf8_name() {
        let v = content_disposition("第1章 人骨拼圖.md");
        assert!(v.starts_with("attachment; filename=\"_1_ ____.md\""), "非 ASCII 字符原样替换成 _，ASCII 字符（数字/空格/.md）保留: {v}");
        assert!(v.contains("filename*=UTF-8''%E7%AC%AC1%E7%AB%A0%20%E4%BA%BA%E9%AA%A8%E6%8B%BC%E5%9C%96.md"), "{v}");
    }

    #[test]
    fn content_disposition_plain_ascii_name_is_unmangled() {
        assert_eq!(content_disposition("index.md"), "attachment; filename=\"index.md\"; filename*=UTF-8''index.md");
    }
}
