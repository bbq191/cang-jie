//! EPUB 的 zip 层与 zip 内 posix 路径工具（清洗/优化/质量门/分卷/占位共用的最底层）。
//!
//! - [`Entry`]：zip 条目（目录项已剔除）。
//! - [`read_entries`]：整本读入；[`read_skeleton`]：只读"骨架"——图片条目留空占位、其余整份读，图片真实体积从 zip
//!   目录查表（流式优化/分卷投递/漫画转 PDF/漫画识别的阶段一，此前各抄一份循环）。
//! - `posix_norm/dir_of/resolve/relative_to/percent_decode/is_html`：EPUB 内路径与文件名判断。
//!
//! 原先散在 `wash.rs`（Entry+路径工具）与 `check.rs`（read_entries），`wash`/`check` 仍 re-export，旧路径不变。
use std::collections::HashMap;
use std::io::{Read, Seek};
use zip::ZipArchive;

/// zip 条目（目录项已剔除）。
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn is_html(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".xhtml") || l.ends_with(".html") || l.ends_with(".htm")
}

/// [`read_skeleton`] 的结果。
pub struct Skeleton {
    /// 条目表：图片条目（`imgopt::is_downscalable`）的 `data` 为空占位，其余是真实字节。
    pub entries: Vec<Entry>,
    /// 条目名 → zip 目录里的真实解压大小（含图片；查表不解压）。
    pub sizes: HashMap<String, u64>,
}

/// 读"骨架"：非图片条目整份读，图片条目只记名字和大小、`data` 留空（真实字节留到阶段二按需读回）。
/// 流式路径的峰值内存因此是"全书文字 + 一张图"而不是"全书图片"。目录项剔除；任一条目读失败整体报错
/// （绝不能静默跳过条目产出残缺 EPUB）。
pub fn read_skeleton<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Result<Skeleton, String> {
    let mut entries = Vec::with_capacity(zip.len());
    let mut sizes = HashMap::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| format!("读 EPUB 条目 {i}: {e}"))?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        sizes.insert(name.clone(), f.size());
        let data = if crate::imgopt::is_downscalable(&name) {
            Vec::new()
        } else {
            let mut d = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut d).map_err(|e| e.to_string())?;
            d
        };
        entries.push(Entry { name, data });
    }
    Ok(Skeleton { entries, sizes })
}

/// 按名字读一个 zip 条目的全部字节；条目不存在 → `Ok(None)`，其它（损坏/IO）错误 → `Err`。
/// 流式路径"图片按需从源 zip 读回"的统一入口（此前 `by_name` + `with_capacity(size)` + `read_to_end` 在
/// streaming/comic_split/comic_pdf/placeholder 各抄一份）。
pub fn read_by_name_opt<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut v = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut v).map_err(|e| e.to_string())?;
    Ok(Some(v))
}

/// 同 [`read_by_name_opt`]，条目不存在也算错误。
pub fn read_by_name<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>, String> {
    read_by_name_opt(zip, name)?.ok_or_else(|| format!("zip 里没有条目 {name}"))
}

/// 从 zip 字节读条目表（目录项剔除，图片也整份读）。优化器与质量门共用。
pub fn read_entries(epub: &[u8]) -> Result<Vec<Entry>, String> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(epub)).map_err(|e| format!("解 EPUB(非 zip?): {e}"))?;
    let mut out = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| format!("读 EPUB 条目 {i}: {e}"))?;
        if f.is_dir() {
            continue;
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        out.push(Entry { name: f.name().to_string(), data });
    }
    Ok(out)
}

// ───────────────────────── 路径工具（zip 内 posix 路径） ─────────────────────────

pub fn posix_norm(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

pub fn dir_of(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}

pub fn resolve(base_dir: &str, rel: &str) -> String {
    if base_dir.is_empty() {
        posix_norm(rel)
    } else {
        posix_norm(&format!("{base_dir}/{rel}"))
    }
}

/// `target` 相对 `base_dir` 的路径（都是 zip 内绝对路径）。
pub fn relative_to(base_dir: &str, target: &str) -> String {
    let b: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    let t: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    let common = b.iter().zip(t.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = vec!["..".into(); b.len() - common];
    out.extend(t[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (b.get(i + 1), b.get(i + 2)) {
                if let Ok(v) = u8::from_str_radix(&format!("{}{}", *h as char, *l as char), 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let o = zip::write::SimpleFileOptions::default();
            for (n, d) in files {
                if n.ends_with('/') {
                    z.add_directory(*n, o).unwrap();
                } else {
                    z.start_file(*n, o).unwrap();
                    z.write_all(d).unwrap();
                }
            }
            z.finish().unwrap();
        }
        buf
    }

    #[test]
    fn paths() {
        assert_eq!(posix_norm("OEBPS/../a/./b"), "a/b");
        assert_eq!(resolve("OEBPS/text", "../style.css"), "OEBPS/style.css");
        assert_eq!(relative_to("OEBPS", "OEBPS/text/c1.xhtml"), "text/c1.xhtml");
        assert_eq!(relative_to("OEBPS/text", "OEBPS/style.css"), "../style.css");
        assert_eq!(relative_to("", "a.xhtml"), "a.xhtml");
        assert_eq!(percent_decode("%E5%AD%97.xhtml"), "字.xhtml");
    }

    #[test]
    fn skeleton_leaves_images_empty_keeps_text_and_records_all_sizes() {
        let bytes = zip_of(&[("dir/", b""), ("a.xhtml", b"<p>hi</p>"), ("images/p1.JPG", &[7u8; 300]), ("images/p2.gif", &[9u8; 10])]);
        let mut z = ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let sk = read_skeleton(&mut z).unwrap();
        let by: HashMap<&str, &Entry> = sk.entries.iter().map(|e| (e.name.as_str(), e)).collect();
        assert_eq!(sk.entries.len(), 3, "目录项剔除");
        assert_eq!(by["a.xhtml"].data, b"<p>hi</p>");
        assert!(by["images/p1.JPG"].data.is_empty(), "可降采样图片留空占位");
        assert_eq!(by["images/p2.gif"].data.len(), 10, "gif 不在降采样范围，照常整份读");
        assert_eq!(sk.sizes["images/p1.JPG"], 300, "占位条目的真实体积从 zip 目录取");
    }

    #[test]
    fn read_by_name_distinguishes_missing_from_present() {
        let bytes = zip_of(&[("a.txt", b"hello")]);
        let mut z = ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(read_by_name_opt(&mut z, "a.txt").unwrap(), Some(b"hello".to_vec()));
        assert_eq!(read_by_name_opt(&mut z, "nope").unwrap(), None, "条目不存在不是错误");
        assert!(read_by_name(&mut z, "nope").unwrap_err().contains("nope"));
        assert_eq!(read_by_name(&mut z, "a.txt").unwrap(), b"hello");
    }

    #[test]
    fn read_entries_reads_everything_including_images_and_rejects_non_zip() {
        let bytes = zip_of(&[("a.xhtml", b"x"), ("i.png", &[1u8; 5])]);
        let e = read_entries(&bytes).unwrap();
        assert_eq!(e, vec![Entry { name: "a.xhtml".into(), data: b"x".to_vec() }, Entry { name: "i.png".into(), data: vec![1u8; 5] }]);
        assert!(read_entries(b"definitely not a zip").unwrap_err().contains("非 zip"));
    }
}
