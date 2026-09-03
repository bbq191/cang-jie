//! TTF/OTF `name` 表家族名解析（fc-scan 不在时的兜底；也用于校验上传的是真字体）。
//! 取 nameID 16（Typographic Family）优先、否则 nameID 1；平台 3(Windows, UTF-16BE) 优先、否则 1(Mac Roman)。
use std::convert::TryInto;

fn u16be(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|x| u16::from_be_bytes(x.try_into().unwrap()))
}
fn u32be(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes(x.try_into().unwrap()))
}

/// 是否 TrueType/OpenType/TTC 魔数。
pub fn is_font(b: &[u8]) -> bool {
    matches!(b.get(0..4), Some(b"\x00\x01\x00\x00") | Some(b"OTTO") | Some(b"true") | Some(b"ttcf"))
}

/// 家族名；TTC 取第一个字体。
pub fn family_name(b: &[u8]) -> Option<String> {
    let base = if b.get(0..4) == Some(b"ttcf") { u32be(b, 12)? as usize } else { 0 };
    let num_tables = u16be(b, base + 4)? as usize;
    let mut name_off = None;
    for i in 0..num_tables {
        let rec = base + 12 + i * 16;
        if b.get(rec..rec + 4)? == b"name" {
            name_off = Some(u32be(b, rec + 8)? as usize);
            break;
        }
    }
    let n = name_off?;
    let count = u16be(b, n + 2)? as usize;
    let str_off = n + u16be(b, n + 4)? as usize;
    let mut best: Option<(u8, String)> = None; // (优先级, 名)
    for i in 0..count {
        let r = n + 6 + i * 12;
        let plat = u16be(b, r)?;
        let name_id = u16be(b, r + 6)?;
        let len = u16be(b, r + 8)? as usize;
        let off = u16be(b, r + 10)? as usize;
        if name_id != 1 && name_id != 16 {
            continue;
        }
        let raw = b.get(str_off + off..str_off + off + len)?;
        let s = match plat {
            3 | 0 => {
                let u: Vec<u16> = raw.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u)
            }
            1 => raw.iter().map(|&c| c as char).collect(),
            _ => continue,
        };
        let s = s.trim().to_string();
        if s.is_empty() {
            continue;
        }
        // 优先级：nameID16 > nameID1；平台 3 > 其它
        let pri = (if name_id == 16 { 2 } else { 0 }) + (if plat == 3 { 1 } else { 0 });
        if best.as_ref().map(|(p, _)| pri > *p).unwrap_or(true) {
            best = Some((pri, s));
        }
    }
    best.map(|(_, s)| s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手搓最小 TTF：只有一张 name 表，两条记录（平台1 nameID1 "Foo"、平台3 nameID16 "Bar Family"）。
    fn tiny_font() -> Vec<u8> {
        let mut names: Vec<u8> = Vec::new();
        let s1 = b"Foo".to_vec();
        let s2: Vec<u8> = "Bar Family".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let count = 2u16;
        let string_offset = 6 + 12 * count as usize;
        names.extend_from_slice(&0u16.to_be_bytes());
        names.extend_from_slice(&count.to_be_bytes());
        names.extend_from_slice(&(string_offset as u16).to_be_bytes());
        for (plat, enc, nid, len, off) in [(1u16, 0u16, 1u16, s1.len() as u16, 0u16), (3, 1, 16, s2.len() as u16, s1.len() as u16)] {
            for v in [plat, enc, 0x409, nid, len, off] {
                names.extend_from_slice(&v.to_be_bytes());
            }
        }
        names.extend_from_slice(&s1);
        names.extend_from_slice(&s2);
        let mut f = Vec::new();
        f.extend_from_slice(b"\x00\x01\x00\x00");
        f.extend_from_slice(&1u16.to_be_bytes());
        f.extend_from_slice(&[0u8; 6]);
        let table_off = 12 + 16;
        f.extend_from_slice(b"name");
        f.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(&(table_off as u32).to_be_bytes());
        f.extend_from_slice(&(names.len() as u32).to_be_bytes());
        f.extend_from_slice(&names);
        f
    }

    #[test]
    fn parses_family_preferring_typographic_windows_name() {
        let f = tiny_font();
        assert!(is_font(&f));
        assert_eq!(family_name(&f).as_deref(), Some("Bar Family"));
        assert!(!is_font(b"PK\x03\x04"));
        assert_eq!(family_name(b"\x00\x01\x00\x00"), None);
    }
}
