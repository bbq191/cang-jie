//! URL 百分号编解码（查询串、路径段、`filename*=`）。2026-10-10 从 `multipart` 搬来：路由、查询串、网关都用，
//! 放在 multipart 里是历史原因（审计 CORE-9）。

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_roundtrip() {
        assert_eq!(percent_encode("a b/中"), "a%20b%2F%E4%B8%AD");
        assert_eq!(percent_decode(&percent_encode("x=1&y=中 文")), "x=1&y=中 文");
        assert_eq!(percent_decode_path("C++%20a%2B.ttf"), "C++ a+.ttf", "路径段里 + 不是空格");
        assert_eq!(percent_decode("C++%2B"), "C  +", "查询串仍按表单语义");
    }

    /// 回归：`%` 后跟多字节 UTF-8 字符曾在 `&s[i+1..i+3]` 处 panic（切到字符中间）。
    #[test]
    fn percent_decode_never_panics_on_non_ascii_or_malformed() {
        assert_eq!(percent_decode("%aé!"), "%aé!", "非法转义原样保留、不 panic");
        assert_eq!(percent_decode("%é"), "%é");
        assert_eq!(percent_decode("中%中文"), "中%中文");
        assert_eq!(percent_decode("%+1"), "% 1", "`+1` 不是合法 hex，不该被 from_str_radix 式地吞掉");
        assert_eq!(percent_decode("%4"), "%4");
        assert_eq!(percent_decode("%41"), "A");
        assert_eq!(percent_decode("%e4%b8%ad+x"), "中 x");
        // 穷举：任意由 % 与若干多字节/ASCII 字符拼出的短串都不能 panic
        let alphabet = ["%", "a", "F", "é", "中", "+", "\u{1F600}"];
        for x in alphabet {
            for y in alphabet {
                for z in alphabet {
                    let _ = percent_decode(&format!("{x}{y}{z}"));
                    let _ = percent_decode(&format!("{x}{y}{z}!"));
                }
            }
        }
    }
}
