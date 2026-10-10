//! 文件格式白名单的**单一事实源**：母版库收的书籍格式、字体、壁纸图片（KOReader 用的 StarDict 词典白名单 `DICT_EXTS`
//! 2026-09-30 随 KOReader 相关源码一起删除，见 git 历史）。
//! 网关 UI（`accept=` + 选中即拦）、各服务上传门（`UploadTarget::allowed_ext`）、inbox 追平、CLI 都从这里派生，
//! 改一处全链同步（2026-09-05 用户定：所有上传口都要有格式限制，且网页与服务端同一份）。
//! 扩展名一律**小写、不带点**。

/// 原生 xochitl 直读（两个读器都能去）。母版库收的书籍格式与这份完全相同（见 `BOOK_EXTS`）。
pub const NATIVE_EXTS: &[&str] = &["epub", "pdf"];
/// 母版库收的书籍格式。⚠ 2026-09-18 用户明确要求"从此开始入库只入 PDF 和 EPUB，不论格式是否
/// 支持"——**这是策略收紧，不是技术能力判断**：CBZ/CBR/DJVU/HTML/HTM/RTF/DOC/DOCX/CHM/XPS 这些
/// 格式设备装的 KOReader 本来能读（真机 `documentregistry` 核对过：crengine 收 txt/html/rtf/doc/
/// docx/chm，mupdf 收 cbz/cbr(libarchive 带 rar)/xps，djvu 引擎收 djvu），但用户不想再维护"仅
/// KOReader 能读"这一档，一律拒收，只留原生两读器都能去的 EPUB/PDF。2026-09-17 那次已经砍掉的
/// azw3/mobi/azw/prc/fb2/txt（原"电脑可转"档，靠已砍的 host `shelf push` Calibre 管线转 EPUB）
/// 保持不收，这次是在那次基础上把仅 KOReader 那一档也砍掉，两档收成一档。
pub const BOOK_EXTS: &[&str] = NATIVE_EXTS;
/// TrueType / OpenType 字体（原生 fontconfig 与 KOReader 同一份）。
pub const FONT_EXTS: &[&str] = &["ttf", "otf", "ttc"];
/// 壁纸源图。
pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png"];

/// 文件名的扩展名（小写、不带点）；无扩展名 → 空串。
pub fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// 文件名扩展名是否在白名单里。**空白名单＝接受任意文件**（含无扩展名），对齐 KOReader books「任意格式原样」语义。
pub fn has_ext(name: &str, exts: &[&str]) -> bool {
    exts.is_empty() || exts.contains(&ext_of(name).as_str())
}

/// 白名单的展示形（带点、空格分隔），给拒收提示用：`.epub .pdf …`。
pub fn dotted(exts: &[&str]) -> String {
    exts.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(" ")
}

/// 去掉扩展名的主干（`书.epub` → `书`；无扩展名/点开头 → 原样）。与 [`ext_of`] 的切分规则一致。
pub fn stem_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// 扩展名 → MIME（投 xochitl `/upload`、下载回执、EPUB manifest 共用的单一表）；认不出的给 `application/octet-stream`。
pub fn mime_of(name: &str) -> &'static str {
    match ext_of(name).as_str() {
        "epub" => "application/epub+zip",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        // xochitl 自己的导出包（zip 容器），/upload 认 application/zip
        "rmdoc" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "ttc" => "font/collection",
        "json" => "application/json",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 按文件头魔数认格式（不信扩展名）：返回与 [`ext_of`] 同形的扩展名（`png`/`jpg`/`pdf`/`zip`/`ttf`…），认不出 `None`。
/// 注意 EPUB/CBZ/DOCX 都是 zip，这里只答 `zip`；字体魔数与 `ttf::is_font` 同一套。
pub fn sniff(head: &[u8]) -> Option<&'static str> {
    Some(match head {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xFF, 0xD8, 0xFF, ..] => "jpg",
        [b'%', b'P', b'D', b'F', ..] => "pdf",
        [b'P', b'K', 3, 4, ..] => "zip",
        [b'G', b'I', b'F', b'8', ..] => "gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        [0, 1, 0, 0, ..] | [b't', b'r', b'u', b'e', ..] => "ttf",
        [b'O', b'T', b'T', b'O', ..] => "otf",
        [b't', b't', b'c', b'f', ..] => "ttc",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_mime_and_sniff() {
        assert_eq!(stem_of("书.名.EPUB"), "书.名");
        assert_eq!(stem_of("noext"), "noext");
        assert_eq!(stem_of(".hidden"), ".hidden");
        assert_eq!(mime_of("a.EPUB"), "application/epub+zip");
        assert_eq!(mime_of("a.pdf"), "application/pdf");
        assert_eq!(mime_of("x.JPEG"), "image/jpeg");
        assert_eq!(mime_of("x"), "application/octet-stream");
        assert_eq!(sniff(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0"), Some("jpg"));
        assert_eq!(sniff(b"PK\x03\x04rest"), Some("zip"));
        assert_eq!(sniff(b"%PDF-1.7"), Some("pdf"));
        assert_eq!(sniff(b"OTTO"), Some("otf"));
        assert_eq!(sniff(b"PK"), None, "太短不认");
    }

    #[test]
    fn ext_and_whitelist() {
        assert_eq!(ext_of("A.EPUB"), "epub");
        assert_eq!(ext_of("noext"), "");
        assert_eq!(ext_of(".hidden"), "", "点开头无主干不算扩展名");
        assert!(has_ext("x.Pdf", BOOK_EXTS) && !has_ext("x.jpg", BOOK_EXTS) && !has_ext("x", BOOK_EXTS));
        assert!(has_ext("anything", &[]), "空白名单收任意");
        assert_eq!(dotted(&["a", "b"]), ".a .b");
    }

    #[test]
    fn book_exts_equals_native_exts() {
        // 2026-09-18 起两档收成一档：母版库只收 EPUB/PDF，不再区分"仅 KOReader"。
        assert_eq!(BOOK_EXTS, NATIVE_EXTS);
    }

    #[test]
    fn retired_host_convertible_exts_no_longer_accepted() {
        // 2026-09-17 EPUB 线架构调整：azw3/mobi/azw/prc/fb2/txt 不再自动转 EPUB，母版库也不收。
        for ext in ["azw3", "mobi", "azw", "prc", "fb2", "txt"] {
            assert!(!has_ext(&format!("x.{ext}"), BOOK_EXTS), "{ext} 应已从 BOOK_EXTS 退役");
        }
    }

    #[test]
    fn retired_koreader_only_tier_no_longer_accepted() {
        // 2026-09-18 用户明确要求"入库只入 PDF 和 EPUB，不论格式是否支持"——KOReader 技术上能读
        // 这些格式，但策略上不再收，母版库直接拒收。
        for ext in ["cbz", "cbr", "djvu", "html", "htm", "rtf", "doc", "docx", "chm", "xps"] {
            assert!(!has_ext(&format!("x.{ext}"), BOOK_EXTS), "{ext} 应已从 BOOK_EXTS 退役");
        }
    }
}
