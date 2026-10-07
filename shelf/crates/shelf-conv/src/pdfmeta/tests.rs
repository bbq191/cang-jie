use super::*;
use std::io::Cursor;

fn count(pdf: &[u8]) -> Result<usize, String> {
    page_count_from(Cursor::new(pdf))
}

/// 按传统 xref 表写 PDF：`objs` 是 (对象号, 对象体)，对象号可以不连续；`trailer_extra` 追加进 trailer。
fn classic_pdf(objs: &[(u32, &[u8])], root: u32, trailer_extra: &str) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    append_classic_section(&mut pdf, objs, &format!("/Root {root} 0 R {trailer_extra}"));
    pdf
}

/// 在 `pdf` 末尾追加一组对象 + 一节传统 xref（每个对象单独一个子段）+ trailer + startxref。返回这节 xref 的偏移。
fn append_classic_section(pdf: &mut Vec<u8>, objs: &[(u32, &[u8])], trailer: &str) -> usize {
    let mut offs = Vec::new();
    for (id, body) in objs {
        offs.push((*id, pdf.len()));
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let size = objs.iter().map(|o| o.0).max().unwrap_or(0) + 1;
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 1\n0000000000 65535 f\r\n");
    for (id, off) in offs {
        pdf.extend_from_slice(format!("{id} 1\n{off:010} 00000 n\r\n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {size} {trailer} >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    xref
}

/// 手搓一份"交叉引用流 + 对象流"的 PDF（PDF 1.5 写法，跟 qpdf `--object-streams=generate` 同构）：
/// Catalog(1) 和 Pages(2) 都在对象流 5 里，交叉引用流 6 用 `/W [1 4 2]` + FlateDecode + PNG Up 预测器（Predictor 12）。
fn xref_stream_pdf(page_count: u32) -> Vec<u8> {
    let mut pdf = b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let page3 = pdf.len();
    pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>\nendobj\n");
    // 对象流：头部 "1 0 2 <off>"，接着两个对象
    let o1 = b"<< /Type /Catalog /Pages 2 0 R >>";
    let o2 = format!("<</Type/Pages/Kids[3 0 R]/Count {page_count}>>");
    let head = format!("1 0 2 {} ", o1.len() + 1);
    let mut body = head.clone().into_bytes();
    body.extend_from_slice(o1);
    body.push(b' ');
    body.extend_from_slice(o2.as_bytes());
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&body, 6);
    let objstm5 = pdf.len();
    pdf.extend_from_slice(format!("5 0 obj\n<< /Type /ObjStm /N 2 /First {} /Filter /FlateDecode /Length {} >>\nstream\n", head.len(), z.len()).as_bytes());
    pdf.extend_from_slice(&z);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let xref6 = pdf.len();
    // 行：type(1) field2(4) field3(2)
    let rows: Vec<[u8; 7]> = [(0u8, 0u32, 65535u16), (2, 5, 0), (2, 5, 1), (1, page3 as u32, 0), (0, 0, 0), (1, objstm5 as u32, 0), (1, xref6 as u32, 0)]
        .iter()
        .map(|&(t, a, b)| {
            let a = a.to_be_bytes();
            let b = b.to_be_bytes();
            [t, a[0], a[1], a[2], a[3], b[0], b[1]]
        })
        .collect();
    // PNG Up 预测器编码
    let mut enc = Vec::new();
    let mut prev = [0u8; 7];
    for r in &rows {
        enc.push(2);
        for i in 0..7 {
            enc.push(r[i].wrapping_sub(prev[i]));
        }
        prev = *r;
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&enc, 6);
    pdf.extend_from_slice(
        format!("6 0 obj\n<< /Type /XRef /Size 7 /W [1 4 2] /Root 1 0 R /Filter /FlateDecode /DecodeParms << /Columns 7 /Predictor 12 >> /Length {} >>\nstream\r\n", z.len()).as_bytes(),
    );
    pdf.extend_from_slice(&z);
    pdf.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{xref6}\n%%EOF\n").as_bytes());
    pdf
}

#[test]
fn classic_table_pages_not_object_2() {
    // 对象 2 是 /Outlines（也有 /Count）：旧实现要么拒收、要么把书签数当页数；这里必须顺着 Root→Pages 读到 300。
    let pdf = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 3 0 R /Outlines 2 0 R >>"), (2, b"<< /Type /Outlines /Count 7 >>"), (3, b"<< /Type /Pages /Kids [] /Count 300 >>")], 1, "");
    assert_eq!(count(&pdf), Ok(300));
    // 对象号不连续、Root 不在 1、紧凑写法
    let pdf = classic_pdf(&[(9, b"<</Type/Pages/Kids[]/Count 12>>"), (17, b"<</Type/Catalog/Pages 9 0 R>>")], 17, "");
    assert_eq!(count(&pdf), Ok(12));
}

#[test]
fn placeholder_pdf_has_one_page() {
    assert_eq!(count(&crate::placeholder::pdf_placeholder()), Ok(1));
}

#[test]
fn xref_stream_with_object_stream_and_png_predictor() {
    let pdf = xref_stream_pdf(42);
    assert_eq!(count(&pdf), Ok(42));
    // 走磁盘路径也一样
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x.pdf");
    std::fs::write(&p, &pdf).unwrap();
    assert_eq!(page_count(&p), Ok(42));
}

/// lopdf 生成的现代 PDF（对象流 + 交叉引用流），页树根不在对象 2。
#[test]
fn lopdf_generated_object_and_xref_streams() {
    use lopdf::{dictionary, Document, Object, SaveOptions, Stream};
    let n = 37;
    let mut doc = Document::with_version("1.5");
    // 先占几个对象号，让页树根落在更后面
    for _ in 0..3 {
        doc.add_object(dictionary! { "Filler" => 1 });
    }
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for _ in 0..n {
        let content = doc.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content, "MediaBox" => vec![0.into(), 0.into(), 10.into(), 10.into()] });
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => n as i64 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_with_options(&mut out, SaveOptions::builder().use_object_streams(true).use_xref_streams(true).build()).unwrap();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("/ObjStm") && s.contains("/XRef"), "lopdf 应该产出对象流 + 交叉引用流");
    assert!(!s.contains("\nxref"), "不该有传统 xref 表");
    assert_eq!(count(&out), Ok(n));
}

#[test]
fn prev_chain_incremental_update_newest_wins() {
    // 初版 3 页；增量更新把 Pages（对象 3）改成 5 页、追加一节 xref，/Prev 指回初版
    let mut pdf = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 3 0 R >>"), (3, b"<< /Type /Pages /Kids [] /Count 3 >>")], 1, "");
    assert_eq!(count(&pdf), Ok(3));
    let first_xref = {
        let s = String::from_utf8_lossy(&pdf).to_string();
        s[s.rfind("startxref\n").unwrap() + 10..].lines().next().unwrap().parse::<usize>().unwrap()
    };
    append_classic_section(&mut pdf, &[(3, b"<< /Type /Pages /Kids [] /Count 5 >>")], &format!("/Root 1 0 R /Prev {first_xref}"));
    assert_eq!(count(&pdf), Ok(5), "新节的 Pages 生效；Catalog 只在旧节里，靠 /Prev 找到");

    // 初版是交叉引用流、增量更新是传统表（常见的"工具改过一次"写法）
    let mut pdf = xref_stream_pdf(42);
    let s = String::from_utf8_lossy(&pdf).to_string();
    let prev = s[s.rfind("startxref\n").unwrap() + 10..].lines().next().unwrap().parse::<usize>().unwrap();
    append_classic_section(&mut pdf, &[(2, b"<< /Type /Pages /Kids [3 0 R] /Count 43 >>")], &format!("/Root 1 0 R /Prev {prev}"));
    assert_eq!(count(&pdf), Ok(43));
}

#[test]
fn indirect_count_and_length_are_resolved() {
    let pdf = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, b"<< /Type /Pages /Kids [] /Count 4 0 R >>"), (4, b"88")], 1, "");
    assert_eq!(count(&pdf), Ok(88));
}

#[test]
fn prev_loop_and_bad_values_are_errors() {
    // /Prev 指向自己：必须报错，不能死循环
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 1\n0000000000 65535 f\r\ntrailer\n<< /Size 1 /Root 1 0 R /Prev {xref} >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    assert!(count(&pdf).unwrap_err().contains("成环"));
    // 页数为 0 / 负数 / 天文数字 / 不是整数
    for c in ["0", "-3", "99999999999", "(abc)"] {
        let body = format!("<< /Type /Pages /Count {c} >>");
        let pdf = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, body.as_bytes())], 1, "");
        assert!(count(&pdf).is_err(), "Count {c}");
    }
    // /Pages 指向的不是页树
    let pdf = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, b"<< /Type /Outlines /Count 7 >>")], 1, "");
    assert!(count(&pdf).is_err());
    // xref 说对象 2 在某偏移，那里却是别的对象
    let good = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, b"<< /Type /Pages /Count 1 >>")], 1, "");
    let s = String::from_utf8_lossy(&good).replace("2 0 obj", "7 0 obj");
    assert!(count(s.as_bytes()).is_err());
}

#[test]
fn oversized_dict_object_is_refused_without_reading_it_all() {
    let mut body = b"<< /Type /Pages /Count 1 /Pad [".to_vec();
    body.extend(std::iter::repeat_n(b'1', MAX_DICT_OBJ_BYTES + 10));
    body.extend_from_slice(b"] >>");
    let pdf = classic_pdf(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, &body)], 1, "");
    let err = count(&pdf).unwrap_err();
    assert!(err.contains("上限"), "{err}");
}

/// 字节级替换（压缩数据不是 UTF-8，不能走 `String::from_utf8_lossy` 再按字符串偏移改）。
fn replace_bytes(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let at = hay.windows(from.len()).position(|w| w == from).expect("找不到要替换的字节");
    [&hay[..at], to, &hay[at + from.len()..]].concat()
}

#[test]
fn oversized_stream_length_and_bad_w_are_errors() {
    let pdf = xref_stream_pdf(3);
    // /W 字段宽度超过 8 字节（等长替换，偏移不变）
    let bad_w = replace_bytes(&pdf, b"/W [1 4 2]", b"/W [1 9 2]");
    assert!(count(&bad_w).unwrap_err().contains("/W"));
    // 交叉引用流的 /Length 声明成天文数字：不照单分配。流字典在 xref 对象开头，改长度不影响 startxref 指向。
    let at = pdf.windows(11).position(|w| w == b"/Type /XRef").unwrap();
    let len_at = at + pdf[at..].windows(8).position(|w| w == b"/Length ").unwrap() + 8;
    let len_end = len_at + pdf[len_at..].iter().position(|b| *b == b' ').unwrap();
    let mut huge = pdf.clone();
    huge.splice(len_at..len_end, b"999999999999".iter().copied());
    assert!(count(&huge).unwrap_err().contains("上限"));
    // 对象流用了不认识的过滤器：报错而不是乱读（等长替换，对象流在交叉引用流之前，第一处就是它）
    let bad_filter = replace_bytes(&pdf, b"/Filter /FlateDecode", b"/Filter /LZWDecode  ");
    assert!(count(&bad_filter).unwrap_err().contains("过滤器"));
}

#[test]
fn corrupt_inputs_are_errors_not_panics() {
    for junk in [&b""[..], b"%PDF-1.4", b"startxref\n", b"startxref\n999999\n%%EOF", b"startxref\n0\n%%EOF", b"xref\ntrailer<<>>startxref\n0"] {
        assert!(count(junk).is_err(), "{:?}", String::from_utf8_lossy(junk));
    }
    // 截断到任意长度、逐字节改坏：只要求不 panic（结果是 Ok 还是 Err 都行：比如只截掉末尾换行，照样能读）
    for pdf in [xref_stream_pdf(9), classic_pdf(&[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, b"<< /Type /Pages /Kids [3 0 R] /Count 9 >>")], 1, "")] {
        for n in 0..pdf.len() - 1 {
            let _ = count(&pdf[..n]);
        }
        for i in 0..pdf.len() {
            for v in [0u8, 0xFF, b'9', b'<', b'('] {
                let mut m = pdf.clone();
                m[i] = v;
                let _ = count(&m);
            }
        }
    }
}
