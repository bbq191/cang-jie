//! URL 百分号编解码（查询串、路径段、`filename*=`）。2026-10-10 从 `multipart` 搬来：路由、查询串、网关都用，
//! 放在 multipart 里是历史原因（审计 CORE-9）；`multipart::percent_*` 仍再导出。

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// 百分号解码（查询串 / 表单：`+` 当空格，即 `application/x-www-form-urlencoded` 语义）。
pub fn percent_decode(s: &str) -> String {
    decode(s, true)
}

/// 百分号解码（URL 路径段 / RFC 5987 `filename*=`：`+` 就是 `+`）。`+` 当空格只是表单编码的约定，
/// 此前路由参数也套用它，`/fonts/C++.ttf` 这类没被客户端转义的名字会被解成 `C  .ttf`（网页走
/// `encodeURIComponent` 会把 `+` 编成 `%2B`，不受影响；curl/脚本手写路径会踩到）。
pub fn percent_decode_path(s: &str) -> String {
    decode(s, false)
}

fn decode(s: &str, plus_as_space: bool) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        // 按字节取两位 hex，不对 &str 按字节下标切片：`%` 后若跟多字节 UTF-8 字符，`&s[i+1..i+3]`
        // 会切在字符中间直接 panic（当年 release 是 panic=abort，一条恶意查询串就能摔掉整个进程；现在是 unwind，
        // 由 HTTP 层兜成 500，但照样不该 panic）。
        // 同时不再借 `from_str_radix`（它会把 `+1` 当合法输入）。
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(if plus_as_space && b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 百分号编码（RFC 3986 unreserved 之外全编）：查询串 / `?next=` 跳转共用，与 [`percent_decode`] 成对。
pub fn percent_encode(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}

