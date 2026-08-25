//! B 模型「总结卡片」笔记本 —— 一书一份可编辑 Note，生成一次后只**追加**新卡片页、**绝不覆盖**用户手写。
//!
//! 与被 PKM pivot 砍掉的 notebook.rs 双向同步子系统解耦：只依赖 `notebook_rm`（写/读 .rm 页）+ serde_json。
//! 全设备自足——直写 xochitl 文档库四件套，不走 /upload（设备不插 USB 时上传端点未必可达）。
//!
//! 两个核心操作：
//!   - `create_card_notebook`：新建一本卡片笔记本（首页预填索引/模板）。
//!   - `append_page_to_notebook`：给已存在的笔记本末尾安全追加一页（分数索引，绝不动已有页）。

use device_core::notebook_rm;
use serde_json::{json, Value};

fn now_ms_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string()
}

/// 直写一本卡片笔记本到 xochitl 文档库（首页内容 first_page）。返回 doc_uuid。
/// ⚠ 直写件在用户首次打开时会被 xochitl 重新分配 uuid（re-key）→ 认领规范版靠 visibleName，不记死此 uuid。
pub fn create_card_notebook(dir: &str, title: &str, first_page: &str) -> Result<String, String> {
    let doc_uuid = uuid::Uuid::new_v4().to_string();
    let author = uuid::Uuid::new_v4();
    let author_bytes = *author.as_bytes();
    let page_uuid = uuid::Uuid::new_v4().to_string();
    let rm = notebook_rm::simple_text_document(first_page, &author_bytes);
    let now = now_ms_string();

    let content = json!({
        "cPages": {
            "lastOpened": {"timestamp": "1:1", "value": page_uuid},
            "original": {"timestamp": "0:0", "value": -1},
            "pages": [json!({
                "id": page_uuid,
                "idx": {"timestamp": "1:2", "value": "ba"},
                "modifed": now.clone(),
                "template": {"timestamp": "1:1", "value": "Blank"}
            })],
            "uuids": [{"first": author.to_string(), "second": 1}],
        },
        "coverPageNumber": -1, "customZoomCenterX": 0, "customZoomCenterY": 936,
        "customZoomOrientation": "portrait", "customZoomPageHeight": 1872,
        "customZoomPageWidth": 1404, "customZoomScale": 1, "documentMetadata": {},
        "extraMetadata": {}, "fileType": "notebook", "fontName": "", "formatVersion": 2,
        "lineHeight": 100, "orientation": "portrait", "pageCount": 1, "pageTags": [],
        "sizeInBytes": rm.len().to_string(), "tags": [], "textAlignment": "left",
        "textScale": 1, "zoomMode": "bestFit",
    });
    let metadata = json!({
        "createdTime": now, "lastModified": now, "lastOpened": "0", "lastOpenedPage": 0,
        "new": true, "parent": "", "pinned": false, "source": "com.cangjie.weread",
        "type": "DocumentType", "visibleName": title,
    });

    std::fs::write(format!("{dir}/{doc_uuid}.content"), serde_json::to_string_pretty(&content).unwrap())
        .map_err(|e| format!("写 content: {e}"))?;
    std::fs::write(format!("{dir}/{doc_uuid}.metadata"), serde_json::to_string_pretty(&metadata).unwrap())
        .map_err(|e| format!("写 metadata: {e}"))?;
    std::fs::write(format!("{dir}/{doc_uuid}.local"), json!({"contentFormatVersion": 2}).to_string())
        .map_err(|e| format!("写 local: {e}"))?;
    std::fs::create_dir_all(format!("{dir}/{doc_uuid}")).map_err(|e| format!("建页目录: {e}"))?;
    std::fs::write(format!("{dir}/{doc_uuid}/{page_uuid}.rm"), &rm).map_err(|e| format!("写 rm: {e}"))?;
    Ok(doc_uuid)
}

/// 计算追加页的分数索引值：现有最大值末尾接 "n" → 永远严格大于现有全部（字符串前缀扩展）→ 排在末尾。
fn next_idx_value(max_val: &str) -> String {
    let base = if max_val.is_empty() { "ba" } else { max_val };
    format!("{base}n")
}

/// 给已存在的笔记本安全追加一页（text 为整页文本）。**绝不改动任何已有页**，只在 cPages.pages[]
/// 末尾插一项 + 写新 `<uuid>/<page>.rm`。返回新 page_uuid。
///
/// 分数索引：新页 idx.value = max(现有 value) + "n"；timestamp 第二段 = max(现有) + 1。
/// author 复用笔记本 cPages.uuids[0].first（对齐页 .rm 的 AuthorIds）。
/// 写盘顺序：先写 .rm（被引用者先落地）再写 .content，避免中途崩溃留悬空引用。
pub fn append_page_to_notebook(dir: &str, doc_uuid: &str, text: &str) -> Result<String, String> {
    let content_path = format!("{dir}/{doc_uuid}.content");
    let raw = std::fs::read_to_string(&content_path).map_err(|e| format!("读 content: {e}"))?;
    let mut content: Value = serde_json::from_str(&raw).map_err(|e| format!("解析 content: {e}"))?;

    let author = content["cPages"]["uuids"][0]["first"]
        .as_str()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .unwrap_or_else(uuid::Uuid::new_v4);
    let author_bytes = *author.as_bytes();
    let page_uuid = uuid::Uuid::new_v4().to_string();
    let rm = notebook_rm::simple_text_document(text, &author_bytes);

    let pages = content["cPages"]["pages"]
        .as_array_mut()
        .ok_or("content.cPages.pages 不是数组")?;
    let mut max_val = String::new();
    let mut max_ts = 1u64;
    for p in pages.iter() {
        if let Some(v) = p["idx"]["value"].as_str() {
            if v > max_val.as_str() {
                max_val = v.to_string();
            }
        }
        if let Some(sec) = p["idx"]["timestamp"]
            .as_str()
            .and_then(|ts| ts.split(':').nth(1))
            .and_then(|s| s.parse::<u64>().ok())
        {
            max_ts = max_ts.max(sec);
        }
    }
    let new_val = next_idx_value(&max_val);
    pages.push(json!({
        "id": page_uuid,
        "idx": {"timestamp": format!("1:{}", max_ts + 1), "value": new_val},
        "modifed": now_ms_string(),
        "template": {"timestamp": "1:1", "value": "Blank"}
    }));
    let new_count = pages.len();
    content["pageCount"] = json!(new_count);
    let prev_sz = content["sizeInBytes"].as_str().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
    content["sizeInBytes"] = json!((prev_sz + rm.len()).to_string());

    // 先写 .rm（被引用者先落地），再写 .content。
    std::fs::create_dir_all(format!("{dir}/{doc_uuid}")).map_err(|e| format!("建页目录: {e}"))?;
    std::fs::write(format!("{dir}/{doc_uuid}/{page_uuid}.rm"), &rm).map_err(|e| format!("写 rm: {e}"))?;
    std::fs::write(&content_path, serde_json::to_string_pretty(&content).unwrap())
        .map_err(|e| format!("写 content: {e}"))?;

    // bump metadata.lastModified（让 xochitl/同步层察觉变更）。
    let meta_path = format!("{dir}/{doc_uuid}.metadata");
    if let Ok(mraw) = std::fs::read_to_string(&meta_path) {
        if let Ok(mut m) = serde_json::from_str::<Value>(&mraw) {
            m["lastModified"] = json!(now_ms_string());
            let _ = std::fs::write(&meta_path, serde_json::to_string_pretty(&m).unwrap());
        }
    }
    Ok(page_uuid)
}

/// 把「标题 + 多页文本」打成 .rmdoc（zip STORED：<uuid>.metadata/.content/<uuid>/<page>.rm）字节。
/// 用于 /upload 导入路径测试（验证 xochitl 原生导入的 update-vs-duplicate 语义）。
/// 打包 .rmdoc（落 root）。
pub fn pack_rmdoc(doc_uuid: &str, title: &str, pages: &[&str]) -> Result<Vec<u8>, String> {
    pack_rmdoc_in(doc_uuid, title, pages, "")
}

/// 打包 .rmdoc 并指定 `parent` 文件夹 uuid（空=root）。用于「daemon 自动归档进 zettelkasten」验证/落地。
pub fn pack_rmdoc_in(doc_uuid: &str, title: &str, pages: &[&str], parent: &str) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let author = uuid::Uuid::new_v4();
    let author_bytes = *author.as_bytes();
    let now = now_ms_string();
    let mut page_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut cpages: Vec<Value> = Vec::new();
    let mut total = 0usize;
    for (i, text) in pages.iter().enumerate() {
        let pu = uuid::Uuid::new_v4().to_string();
        let rm = notebook_rm::simple_text_document(text, &author_bytes);
        total += rm.len();
        let val = format!("b{}", (b'a' + i as u8) as char); // ba, bb, bc...
        cpages.push(json!({
            "id": pu, "idx": {"timestamp": format!("1:{}", i + 2), "value": val},
            "modifed": now, "template": {"timestamp": "1:1", "value": "Blank"}
        }));
        page_files.push((pu, rm));
    }
    let content = json!({
        "cPages": {
            "lastOpened": {"timestamp": "1:1", "value": page_files[0].0},
            "original": {"timestamp": "0:0", "value": -1}, "pages": cpages,
            "uuids": [{"first": author.to_string(), "second": 1}],
        },
        "coverPageNumber": -1, "customZoomCenterX": 0, "customZoomCenterY": 936,
        "customZoomOrientation": "portrait", "customZoomPageHeight": 1872,
        "customZoomPageWidth": 1404, "customZoomScale": 1, "documentMetadata": {},
        "extraMetadata": {}, "fileType": "notebook", "fontName": "", "formatVersion": 2,
        "lineHeight": 100, "orientation": "portrait", "pageCount": pages.len(), "pageTags": [],
        "sizeInBytes": total.to_string(), "tags": [], "textAlignment": "left",
        "textScale": 1, "zoomMode": "bestFit",
    });
    let metadata = json!({
        "createdTime": now, "lastModified": now, "lastOpened": now, "lastOpenedPage": 0,
        "new": false, "parent": parent, "pinned": false, "source": "com.cangjie.weread",
        "type": "DocumentType", "visibleName": title,
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file(format!("{doc_uuid}.metadata"), stored).map_err(|e| e.to_string())?;
        z.write_all(serde_json::to_string_pretty(&metadata).unwrap().as_bytes()).map_err(|e| e.to_string())?;
        z.start_file(format!("{doc_uuid}.content"), stored).map_err(|e| e.to_string())?;
        z.write_all(serde_json::to_string_pretty(&content).unwrap().as_bytes()).map_err(|e| e.to_string())?;
        for (pu, rm) in &page_files {
            z.start_file(format!("{doc_uuid}/{pu}.rm"), stored).map_err(|e| e.to_string())?;
            z.write_all(rm).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// 只读列出笔记本每页 (idx.value, page_uuid, .rm 首段文本)——真机验证/调试用。
pub fn list_pages(dir: &str, doc_uuid: &str) -> Result<Vec<(String, String, String)>, String> {
    let raw = std::fs::read_to_string(format!("{dir}/{doc_uuid}.content")).map_err(|e| format!("读 content: {e}"))?;
    let content: Value = serde_json::from_str(&raw).map_err(|e| format!("解析 content: {e}"))?;
    let mut out = Vec::new();
    if let Some(pages) = content["cPages"]["pages"].as_array() {
        for p in pages {
            let id = p["id"].as_str().unwrap_or("").to_string();
            let val = p["idx"]["value"].as_str().unwrap_or("").to_string();
            let text = std::fs::read(format!("{dir}/{doc_uuid}/{id}.rm"))
                .ok()
                .map(|b| notebook_rm::read_root_text(&b))
                .unwrap_or_else(|| "<无 .rm>".into());
            out.push((val, id, text));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_idx_strictly_greater() {
        // 追加值永远严格大于现有最大值（字符串序）。
        for base in ["ba", "bb", "bz", "bza", "ban"] {
            let n = next_idx_value(base);
            assert!(n.as_str() > base, "{n} 应 > {base}");
        }
        assert_eq!(next_idx_value(""), "ban");
    }

    #[test]
    fn create_then_append_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cardnote-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();

        let u = create_card_notebook(d, "《测试》- 总结卡片", "第一页").unwrap();
        let p1 = list_pages(d, &u).unwrap();
        assert_eq!(p1.len(), 1);
        assert_eq!(p1[0].0, "ba");
        assert_eq!(p1[0].2, "第一页");

        let _pg = append_page_to_notebook(d, &u, "第二页 · 追加").unwrap();
        let p2 = list_pages(d, &u).unwrap();
        assert_eq!(p2.len(), 2, "应追加成一共两页");
        // 第一页原封不动。
        assert_eq!(p2[0].0, "ba");
        assert_eq!(p2[0].2, "第一页");
        // 第二页排在后面、内容正确。
        assert!(p2[1].0.as_str() > p2[0].0.as_str(), "第二页 idx 应更大");
        assert_eq!(p2[1].2, "第二页 · 追加");

        // 再追加一页，仍严格递增、前两页不变。
        append_page_to_notebook(d, &u, "第三页").unwrap();
        let p3 = list_pages(d, &u).unwrap();
        assert_eq!(p3.len(), 3);
        assert_eq!(p3[0].2, "第一页");
        assert_eq!(p3[1].2, "第二页 · 追加");
        assert_eq!(p3[2].2, "第三页");
        assert!(p3[2].0.as_str() > p3[1].0.as_str());

        // content.pageCount 同步。
        let c: Value = serde_json::from_str(&std::fs::read_to_string(format!("{d}/{u}.content")).unwrap()).unwrap();
        assert_eq!(c["pageCount"].as_u64(), Some(3));

        std::fs::remove_dir_all(&dir).ok();
    }
}
