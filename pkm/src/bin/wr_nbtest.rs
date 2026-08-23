//! wr-nbtest —— B 模型笔记本追加机制的真机验证工具（一次性/调试用，非常驻服务）。
//!
//!   wr-nbtest create <title> [首页文本]     直写一本卡片笔记本，打印 uuid
//!   wr-nbtest append <uuid> <整页文本>       给已存在笔记本末尾安全追加一页
//!   wr-nbtest list   <uuid>                  只读列出每页 idx.value + 首段文本（校验）
//!
//! 目录：默认 xochitl 生产目录，可用 CANGJIE_XOCHITL_DIR 覆盖。

use pkm_device::cardnote;
use weread_device::inject::XOCHITL_DIR;

fn dir() -> String {
    std::env::var("CANGJIE_XOCHITL_DIR").unwrap_or_else(|_| XOCHITL_DIR.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "用法: wr-nbtest create <title> [首页文本] | append <uuid> <文本> | list <uuid>";
    let cmd = match args.get(1) {
        Some(c) => c.as_str(),
        None => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let d = dir();
    match cmd {
        "create" => {
            let title = args.get(2).cloned().unwrap_or_else(|| "《测试》- 总结卡片".into());
            let first = args.get(3).cloned().unwrap_or_else(|| {
                "《测试》- 总结卡片\n\n一、待办清单（自动追加区）\n\n（等待第一颗 ★）".into()
            });
            match cardnote::create_card_notebook(&d, &title, &first) {
                Ok(u) => println!("{u}"),
                Err(e) => {
                    eprintln!("create 失败: {e}");
                    std::process::exit(1);
                }
            }
        }
        "append" => {
            let uuid = match args.get(2) {
                Some(u) => u,
                None => {
                    eprintln!("{usage}");
                    std::process::exit(2);
                }
            };
            let text = args.get(3).cloned().unwrap_or_else(|| "追加页".into());
            match cardnote::append_page_to_notebook(&d, uuid, &text) {
                Ok(p) => println!("追加页 {p}"),
                Err(e) => {
                    eprintln!("append 失败: {e}");
                    std::process::exit(1);
                }
            }
        }
        "upload" => {
            // wr-nbtest upload <uuid> <npages> [title]  —— 构造 rmdoc 传 10.11.99.1/upload，测 update-vs-dup
            let u = args.get(2).cloned().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let n: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
            let title = args.get(4).cloned().unwrap_or_else(|| "《UP验证》- 总结卡片".into());
            let pages: Vec<String> = (0..n).map(|i| format!("第 {} 页 · upload 测试", i + 1)).collect();
            let refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
            let rmdoc = match cardnote::pack_rmdoc(&u, &title, &refs) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("pack 失败: {e}");
                    std::process::exit(1);
                }
            };
            let agent = ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(15))
                .build();
            match weread_device::inject::upload_document(&agent, "10.11.99.1", &rmdoc, &format!("{title}.rmdoc"), "application/zip") {
                Ok(resp) => println!("upload uuid={u} pages={n} -> 响应: {resp}"),
                Err(e) => {
                    eprintln!("upload 失败: {e}");
                    std::process::exit(1);
                }
            }
        }
        "carddemo" => {
            // 上传一张真卡片模板(含★待办+自由区),给用户打字验证 read_root_text 读回
            let title = args.get(2).cloned().unwrap_or_else(|| "读回验证书".into());
            let stars = vec![(3usize, "第一章《测试》".to_string()), (8usize, "第二章《样例》".to_string())];
            let pages = pkm_device::cardsync::render_card(&title, &stars, &Default::default());
            let refs: Vec<&str> = pages.iter().map(|s| s.as_str()).collect();
            let u = uuid::Uuid::new_v4().to_string();
            let visible = format!("《{title}》- 总结卡片");
            let rmdoc = match cardnote::pack_rmdoc(&u, &visible, &refs) {
                Ok(b) => b,
                Err(e) => { eprintln!("pack 失败: {e}"); std::process::exit(1); }
            };
            let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build();
            match weread_device::inject::upload_document(&agent, "10.11.99.1", &rmdoc, &format!("{visible}.rmdoc"), "application/zip") {
                Ok(resp) => println!("carddemo visibleName=《{title}》- 总结卡片 -> {resp}\n请在设备打开它、用Text工具打字、退出，再跑 readback"),
                Err(e) => { eprintln!("upload 失败: {e}"); std::process::exit(1); }
            }
        }
        "readback" => {
            // readback <uuid>  —— 读该文档所有页 .rm 的 RootText，解析成 CardModel 打印
            let uuid = match args.get(2) { Some(u) => u, None => { eprintln!("readback <uuid>"); std::process::exit(2); } };
            match cardnote::list_pages(&d, uuid) {
                Ok(rows) => {
                    let pages: Vec<String> = rows.iter().map(|(_, _, t)| t.clone()).collect();
                    println!("=== 各页读回文本 ===");
                    for (i, t) in pages.iter().enumerate() {
                        println!("--- page {i} ---\n{t}");
                    }
                    let model = pkm_device::cardsync::parse_pages(&pages);
                    println!("=== 解析出的用户批注(notes_by_page) ===\n{:?}", model.notes_by_page);
                    println!("=== 自由区 ===\n{:?}", model.free_notes);
                }
                Err(e) => { eprintln!("readback 失败: {e}"); std::process::exit(1); }
            }
        }
        "chapter" => {
            // chapter <uuid> [page0based...]  —— 验证设备 EPUB 的 页→章名 映射
            let uuid = match args.get(2) { Some(u) => u, None => { eprintln!("chapter <uuid> [page...]"); std::process::exit(2); } };
            let idx = std::fs::read(format!("{d}/{uuid}.epubindex")).unwrap_or_default();
            let epub = std::fs::read(format!("{d}/{uuid}.epub")).unwrap_or_default();
            let secs = weread_device::epubindex::parse_sections(&idx);
            let titles = weread_device::epubindex::chapter_map(&epub);
            println!("=== spine 起始页表 ({} 条) ===", secs.len());
            for (b, s) in &secs { println!("  {b} 起 page{s}  章名={:?}", titles.get(b)); }
            let pages: Vec<usize> = args[3..].iter().filter_map(|s| s.parse().ok()).collect();
            let pages = if pages.is_empty() { vec![0, 2, 40, 73, 113, 200] } else { pages };
            println!("=== 页 → 章名 ===");
            for p in pages {
                println!("  page{p} (第{}页) -> {:?}", p + 1, weread_device::epubindex::page_chapter(&secs, &titles, p));
            }
        }
        "list" => {
            let uuid = match args.get(2) {
                Some(u) => u,
                None => {
                    eprintln!("{usage}");
                    std::process::exit(2);
                }
            };
            match cardnote::list_pages(&d, uuid) {
                Ok(rows) => {
                    println!("{} 页:", rows.len());
                    for (i, (val, id, text)) in rows.iter().enumerate() {
                        let first_line = text.lines().next().unwrap_or("");
                        println!("  [{i}] idx={val} {id} | {first_line}");
                    }
                }
                Err(e) => {
                    eprintln!("list 失败: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}
