//! 通用 EPUB 优化器：吃**任意结构**的 EPUB，清洗层（`wash`，可选：字体字号解锁、排版、章节分页、目录、EPUB 3 规范整理）→
//! 每个 (x)html 的字体解锁与注释重排 → 图片按 Move 屏缩放（漫画单趟裁边/缩放/补白）。用于把用户导入的第三方书也拉进优化
//! （尤其"字体改不动"——第三方常内联硬写死 font）。入口是流式的 [`StreamingOptimize`]（路径进路径出）。
//!
//! 2026-09-29 起文字处理层（清洗、注释、XHTML 解析工具 `crate::html`）以 sheng-ren 仓库的同源 bookconv 为参照移植，
//! 设备专有的部分（漫画页框 `comic_frame`、改书名、取消、900 万像素解码上限）保留；翻页方向只保留原书自带的（不改 OPF spine 方向）。

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter};

/// 幂等标记：优化器把这个文件埋进产物 EPUB，内容=优化器版本号。
/// 判"是否优化过"以它为权威——**跟书走**(云同步不丢、换设备仍在、对第三方书和墨香书一视同仁)，
/// 比记设备本地 uuid(会被云同步 churn)或书名后缀(用户改名即失效)都鲁棒。放 META-INF/ 下
/// (EPUB 规范允许该目录放额外文件，阅读器忽略)，STORED 存。
pub const OPTIMIZE_MARKER: &str = "META-INF/com.cangjie.optimized";
/// 优化逻辑版本。升级 strip_font_locks / preserve_relink_footnotes 等行为时 bump，
/// 据此可把旧版本产物挑出来重优化。v2：加 duokan 图片脚注标记修复（fix_duokan_markers）。
/// v3：按 Move 屏规格设备优化——① 图片降采样到 1696px 长边（imgopt）+ ② e-ink 提对比（灰字→纯黑、
/// 细字重→400，作用于 html style / <style> / .css）。v4：远程图内联（微信读书等下载书内嵌
/// `src="https://res.weread.qq.com/..."` 远程图，设备离线加载不到 → reMarkable 破图占位=大放大镜；
/// 优化时抓下来降采样内联进 EPUB，抓不到就删掉该 img 免放大镜）。**新导入书自动应用**；存量书 autoopt 不
/// 会自动重优化（跳过任何已带标记的书），需要时 `POST /optimize {uuid, force:true}` 强制重优化以应用。
/// v5：Calibre 洗书形态——同文件带文件名 href 归一裸锚（normalize_self_hrefs）+ 真 img 版 duokan 标记
/// 换上标且保留 id（`wash_epub.sh` 产物经 host CLI `epub-optimize` 走同一函数）；图片降采样加短边 ≤954
/// 约束（真机探针：块级图缩到正文列宽，方图只卡长边白留 1.8× 像素）。
/// v6：清洗层（`wash`）可选前置——伪 DRM 剥离、CSS 文件级锁剥离、边距/段距归零+2em 缩进、空页清理、
/// 自动目录、单标签双 id 折叠（对标 host `wash_epub.sh`，`OptimizeOpts::wash`；weread 线缺省不开）。
/// v7：做精做强——① 中英文各按阅读习惯注排版（`wash::LangMode` 自动探测：中文首行 2em / 拉丁 1.2em+标题后首段不缩进）；
/// ② 自动目录从 h1/h2 扩到 **h1–h6** 并多级嵌套（只用 h3 当章标题的书不再漏目录）。
/// v8：真机《飘》两修——① 内联脚注丢弃图标 marker（xochitl 按固有尺寸渲染图标=巨大且每条重复）；
/// ② EPUB 内嵌图改竖向框（宽≤954）防行内横幅溢出竖屏；③ 清洗层剥 CSS `background`/`background-image`
/// （xochitl 无视 no-repeat 把背景图平铺满页盖正文，真机《飘》分卷页坐实）——章头 `<img>` 装饰不受影响。
/// v10：真机《缩进诊断6》/《飘》坐实——xochitl **只认外链 `.css` 文件里的规则，完全无视内联 `<style>` 块和元素
/// `style=` 属性**（此前 v6–v9 注入的内联 cj-wash 排版规则在 xochitl 从未生效！）。改：排版规则（首行缩进/边距）
/// 写成**外链 `cangjie-wash.css`** + 每章 `<link>` + OPF manifest 补 item（xochitl/KOReader 都认）。⚠ xochitl css
/// 解析器脆，外链 css **只用裸 `p{}` 元素选择器**（一条类/复杂选择器就让整表失效，《缩进诊断5》坐实）。撤回 v9 的
/// nbsp 段首缩进（nbsp 宽随字体变、且被折叠，做不到精确 2 字；外链 text-indent 精确且字体无关）。
/// v11：真机《疯探》坐实——删掉 `remove_toc_from_spine`（指向 ≥10 个不同 html 文件的页面曾被当
/// "跟原生 TOC 冗余"的目录页从 spine 剥掉）。假设站不住：这类页面是书籍正文本身，不是能丢的冗余物，
/// 违背 EPUB 线原则①"保留目录页"；已优化过的旧书需 `force:true` 重优化才能拿回被剥掉的目录页。
/// v12：真机《疯探》vs《雪人》对照坐实——`toc.ncx` 的 `dtb:uid` 跟 OPF `dc:identifier` 不一致时
/// （第三方生成器常见 bug，如"番茄小说 EPUB Generator"）reMarkable 原生目录面板**直接不显示目录
/// 入口**（不是空列表），navMap 结构再完整都没用；`dtb:uid` 匹配的书目录入口就在。新增
/// `wash::fix_ncx_uid` 把 `dtb:uid` 同步成 OPF 实际标识符（含我们自己 `build_ncx` 生成的也一并
/// 从硬编码 `cj-wash` 改用真实标识符）；旧书需 `force:true` 重优化。
/// v13：dtb:uid 修一致后《疯探》原生目录入口真机复测仍不出现——跟《雪人》剩下唯一的结构性差异是
/// `toc.ncx` 带外部 DTD 引用（`http://www.daisy.org/...dtd`），《雪人》没有。新增
/// `wash::strip_ncx_doctype` 无条件剥掉这个声明（不改变 NCX 语义，纯粹去掉外部依赖，真机 USB/WiFi
/// 隧道环境很可能因为解析器联网取 DTD 卡住/失败而让整份 NCX 被判不可用）；旧书需 `force:true`。
/// v15：EPUB 漫画页补白目标从屏幕比例 954:1696 改成 xochitl 图片框比例 303:462.1（`imgopt::EPUB_FRAME_ASPECT`，画布 954×1458，
/// 补白容差收紧到 0.3%），配合阅读器页边距 1（由 book-serve + `shelf-comic-margins.qmd` 代理设置，实验室开关 `comicMinMargin`）：真机同图 A/B 图片宽 260→303pt、
/// 左右留白 20.0/22.9pt → 约 0.3/0.7pt；旧漫画需重新优化才生效（从原始文件重跑，别对已优化产物二次优化——多一代 JPEG 有损）。
/// v16（2026-09-29，以 sheng-ren 的同源 bookconv 为参照移植）：
/// ① 字体字号解锁但不动别的样式——相对字号保留（正文整体那一层除外）、`line-height` 与 `vh` 高度去掉、`font` 简写留粗斜体、
///   `background` 简写留颜色；**不再把灰字改黑、细字重提到 400**（EPUB 线与 PDF 线一起停）。
/// ② 居中/居右（`align=`、行内样式）换成 `cj-center`/`cj-right` 类（xochitl 不认行内样式）；补 `<html lang>`；两端对齐；
///   中文段首缩进空白（U+3000、nbsp）去掉由 CSS 给 2em；只靠 `<br>` 换行的文件切成段落；章尾空白页。
/// ③ 章节分页：按标题把章节文件拆开，章标题独立一页、节与节/节与章之间换页，漏掉的节补进目录，指错位置的目录条目核实后改指。
/// ④ 注释：标号原样、不加 `[N]`，图标标号保留并限一个字高；注释 0.85em、一条不跨页；注释索引按 (文件, id)。
/// ⑤ 规范整理：产物一律升级 EPUB 3（NCX 与 spine toc 保留，xochitl 靠它），XHTML 修成合法 XML。
/// ⑥ 图片：JPEG 哈夫曼表按图重做（无损，解码逐像素相同）；漫画里的静态 GIF/WebP 页转 PNG/JPEG；透明漫画页合成白底；
///   漫画 OPF 打 `<dc:subject>漫画</dc:subject>`；抓到的远程图补进 manifest。
/// ⑦ 2026-09-30 再对齐 sheng-ren 的 xochitl 模式（v16 未部署，不升版本）：文字书插图框 842×1455（可阅读范围）、最小边距漫画画布
///   952×1457（旧 954×1458 仍认）、漫画文字留边规则整套写进样式表、图片条目不压缩。同一批真书两边产物图片逐字节相同。
pub const OPTIMIZE_VERSION: &str = "16";

/// 脚注呈现方式。xochitl 无弹窗脚注（穷尽真机实测判死）；母版库「优化」用 `Anchor`（章末可见 + 同章锚点跳转 +
/// 原生「返回」浮标）。⚠ 2026-09-17 曾短暂加过"注释移到引用它的段落末尾"，真机验证后撤回删除——用户真实期望是
/// "翻到哪页注释固定在那页最下面"，EPUB 流式重排做不到（"页"是阅读器翻页时才算出来的），见书架白皮书 §03av 补记。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FootnoteMode {
    /// 跳转（xochitl）：注释移章末 `<div class="footnotes">`，标号改同章锚点，点了跳过去、用阅读器的"返回"回来。
    #[default]
    Anchor,
    /// 弹窗（KOReader）：同 `Anchor`，另给标号标 `epub:type="noteref"`、注释块用 `<aside epub:type="footnote">`。
    /// 设备上两个阅读器共用同一份产物、以 xochitl 为主，母版库不用它（留给测试与命令行）。
    Popup,
    /// 注释文字就地内联显示在引用处 `<span class="cj-fnote">〔…〕</span>`，始终可见、不跳转。
    Inline,
}

/// 优化选项：`wash=Some` 时先过清洗层（书架母版库「优化」与 host `epub-optimize` 缺省开；weread 线不开）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptimizeOpts {
    pub wash: Option<crate::wash::WashOpts>,
    /// 脚注呈现方式（缺省 `Anchor`；母版库「优化」也传 `Anchor`，见 book-serve `staging/optimizing.rs`）。
    pub footnote: FootnoteMode,
    /// 漫画页补白到哪种页框（缺省 `Screen` = 历史行为）。由 book-serve 按"实验室→漫画页边距"开关传入。
    pub comic_frame: crate::imgopt::EpubComicFrame,
}

/// 优化统计，供回执。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub wash: Option<crate::wash::WashReport>,
    pub total_files: usize,
    pub html_files: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

use crate::epubzip::is_html_entry;

// 按职责拆成子模块：`html_pass`（逐条变换）· `streaming`（流式写出）· `marker`（幂等标记）；
// `pub` 项在这里 glob re-export。
mod html_pass;
mod marker;
mod streaming;

use self::html_pass::*;
pub use self::marker::*;
pub use self::streaming::*;

#[cfg(test)]
mod tests;

/// 阶段一的产物。
struct Prepared {
    /// (条目名, 字节, 是否 html)，已过封面声明/清洗/第一遍 html 处理/注释块搬出；首个条目是重写过的 `mimetype`。
    entries: Vec<(String, Vec<u8>, bool)>,
    /// 全书"被引用的注释块"索引（(所在文件, id) → 块 html），第二遍 `preserve_relink_footnotes` 搬进引用它的那一章。
    aside_index: HashMap<crate::htmlproc::NoteKey, String>,
    /// 不做注释搬移的页：导航文档、目录文件、目录样的页（它们的链接不算注释引用，也不往它们里面搬注释）。
    skip_notes: HashSet<String>,
    is_comic_book: bool,
    /// 要改 OPF 时（改书名、书里有远程图要补 manifest、漫画打标签、清洗过的书标 properties）的 OPF 条目名。
    opf_name: Option<String>,
    /// 有章节引用远程图：OPF 推迟到最后写，好把抓到的图补进 manifest（见 `streaming`）。
    has_remote_imgs: bool,
    /// 改书名（见 [`EntryXform::title`]）。
    title: Option<String>,
    /// 输入书里带着本优化器（任意版本）的标记：是在重优化自己的产物，见 [`html_pass::first_pass_html`]、[`html_pass::transform_image_bytes`]。
    reoptimize: bool,
    rep: Report,
}

/// EPUB 规范：`mimetype` 必须是 zip 的第一个条目、STORED、内容就是这串（不带换行）。
const MIMETYPE: &[u8] = b"application/epub+zip";

/// 阶段一：`raw` → 封面声明 → 清洗 → 排序（mimetype 置首、旧标记剔除）→ 漫画识别 → 第一遍 html → 注释块搬出。
/// 图片条目是空占位——这里所有判断只看 html 文字与 `<img>` 引用，不需要图片真实字节。
fn prepare_entries(mut raw: Vec<crate::epubzip::Entry>, opts: &OptimizeOpts, bytes_before: usize, title: Option<&str>) -> Result<Prepared, String> {
    // 保证 OPF 声明了有效封面（见 `wash::ensure_cover_declared`）。
    // 必须在清洗之前：清洗会把只含 SVG 封面的 titlepage 当空页删掉。
    let reoptimize = raw.iter().any(|e| e.name == OPTIMIZE_MARKER);
    crate::wash::ensure_cover_declared(&mut raw);
    let (wash_rep, washed_comic) = match &opts.wash {
        Some(w) => {
            let (r, c) = crate::wash::wash_entries_detect(&mut raw, w)?;
            (Some(r), Some(c))
        }
        None => (None, None),
    };
    let has_remote_imgs = raw.iter().any(|e| is_html_entry(&e.name, &e.data) && std::str::from_utf8(&e.data).is_ok_and(has_remote_img));
    // mimetype 一律重写成规范内容放在最前（源书缺它、内容不规范都修正），其余原序；旧标记剔除（结尾统一重写当前版本）。
    let mut ordered: Vec<crate::epubzip::Entry> = Vec::with_capacity(raw.len() + 1);
    ordered.push(crate::epubzip::Entry { name: "mimetype".into(), data: MIMETYPE.to_vec() });
    ordered.extend(raw.into_iter().filter(|e| e.name != "mimetype" && e.name != OPTIMIZE_MARKER));

    // 漫画识别（图 ≥20 张且平均每张图配的文字 <40 字）：决定图片走漫画单趟处理还是普通降采样。清洗过的书用清洗层判好的
    // （清洗层已把空页清理、目录归一，判定更准；不再判第二遍）。
    let is_comic_book = washed_comic.unwrap_or_else(|| crate::comic_detect::is_comic(&ordered));
    let opf = crate::wash::parse_opf(&ordered);
    // 只在真要改 OPF 时才记它：改书名、补远程图的 manifest 项、给漫画打标签、（清洗过的书）按最终内容标 manifest 的 properties。
    let opf_name: Option<String> = opf.as_ref().filter(|_| title.is_some() || has_remote_imgs || is_comic_book || opts.wash.is_some()).map(|o| ordered[o.index].name.clone());
    // 导航文档与目录文件：不收它们里面的注释引用，也不往里面搬注释。
    let mut skip_notes: HashSet<String> = ordered.iter().filter(|e| crate::wash::is_toc_file(&e.name)).map(|e| e.name.clone()).collect();
    skip_notes.extend(opf.and_then(|o| o.nav_doc));

    let mut rep = Report { wash: wash_rep, total_files: 0, html_files: 0, bytes_before, bytes_after: 0 };

    // 第一遍：xhtml → strip_font_locks；同时扫全书 marker 得**被引用**的注释 (文件, id) 集（referenced），供下一步
    // "只搬被引用的注释块"用。目录样的页（链接文字占大半）不算。
    let mut entries: Vec<(String, Vec<u8>, bool)> = Vec::with_capacity(ordered.len());
    let mut referenced: HashMap<String, HashSet<String>> = HashMap::new(); // 注释所在文件 → 被 marker 引用的 id
    for crate::epubzip::Entry { name, data } in ordered {
        rep.total_files += 1;
        let ish = is_html_entry(&name, &data);
        // 非 UTF-8 的 html 原样保留（`from_utf8` 失败时把字节还回来，不克隆）。
        let data = if ish {
            match String::from_utf8(data) {
                Ok(text) => {
                    let stripped = first_pass_html(&text, &name, reoptimize);
                    if !skip_notes.contains(&name) {
                        let refs = crate::htmlproc::referenced_note_keys(&stripped, &name);
                        if !refs.is_empty() && crate::wash::is_toc_like_page(&stripped) {
                            skip_notes.insert(name.clone());
                        } else {
                            for (file, id) in refs {
                                referenced.entry(file).or_default().insert(id);
                            }
                        }
                    }
                    rep.html_files += 1;
                    stripped.into_bytes()
                }
                Err(e) => e.into_bytes(),
            }
        } else {
            data
        };
        entries.push((name, data, ish));
    }
    let aside_index = collect_notes(&mut entries, &mut referenced, &skip_notes);
    // 漫画 + 页边距最小化页框：纯文字页（版权/前情提要/章节标题）补左右留白，否则边距 1 下文字贴屏幕边。
    // 放在最后：此时 html 已过第一遍、注释块已搬出，判"纯文字页"看到的就是最终页面结构。
    if is_comic_book && opts.comic_frame == crate::imgopt::EpubComicFrame::MinMargin {
        crate::comic_pad::ensure_all_css_rules(&mut entries);
        crate::comic_pad::pad_text_pages(&mut entries);
        crate::comic_pad::free_media_pages(&mut entries);
        crate::comic_pad::pad_mixed_text_blocks(&mut entries);
    }
    Ok(Prepared { entries, aside_index, skip_notes, is_comic_book, opf_name, has_remote_imgs, title: title.map(str::to_string), reoptimize, rep })
}

/// 第一遍后半：把**被引用**的注释块（aside/p/li/div 且带注释语义）从各章移除、建全书索引 (文件, id) → 块，交给第二遍
/// `preserve_relink_footnotes` 搬进引用它的那一章。未被引用的块原样留在原处。
///
/// **搬走前核对每条都有人接**（2026-09-28 审计）：按第二遍真正会用的文字（先拆互指环、换 duokan 标记，这两步可能改掉 marker）
/// 重新扫一遍全书的注释引用；收集了却没有任何一章会接的注释，从原文重新收集时不再收它——放回原处。放回的注释里要是还引用着
/// 别的注释，下一轮会看到，所以反复到没有落空的为止。
fn collect_notes(entries: &mut [(String, Vec<u8>, bool)], referenced: &mut HashMap<String, HashSet<String>>, skip: &HashSet<String>) -> HashMap<crate::htmlproc::NoteKey, String> {
    let mut index: HashMap<crate::htmlproc::NoteKey, String> = HashMap::new();
    let mut originals: HashMap<usize, Vec<u8>> = HashMap::new();
    let collect_one = |text: &str, name: &str, referenced: &HashMap<String, HashSet<String>>| referenced.get(name).map(|ids| crate::htmlproc::collect_footnote_notes(text, ids, true));
    for (i, (name, data, ish)) in entries.iter_mut().enumerate() {
        if !*ish || !referenced.contains_key(name.as_str()) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else { continue };
        let Some((cleaned, notes)) = collect_one(text, name, referenced) else { continue };
        if !notes.is_empty() {
            index.extend(notes.into_iter().map(|(id, inner)| ((name.clone(), id), inner)));
            originals.insert(i, std::mem::replace(data, cleaned.into_bytes()));
        }
    }
    while !index.is_empty() {
        let mut claimed: HashSet<crate::htmlproc::NoteKey> = HashSet::new();
        for (name, data, ish) in entries.iter() {
            if !*ish || skip.contains(name) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(data) else { continue };
            let t = crate::htmlproc::fix_duokan_markers(&crate::htmlproc::break_footnote_cycles(text));
            claimed.extend(crate::htmlproc::referenced_note_keys(&t, name).into_iter().filter(|k| index.contains_key(k)));
        }
        let unclaimed: Vec<crate::htmlproc::NoteKey> = index.keys().filter(|k| !claimed.contains(*k)).cloned().collect();
        if unclaimed.is_empty() {
            break;
        }
        let files: HashSet<String> = unclaimed.iter().map(|k| k.0.clone()).collect();
        for (file, id) in &unclaimed {
            if let Some(ids) = referenced.get_mut(file) {
                ids.remove(id);
            }
        }
        index.retain(|k, _| !files.contains(&k.0));
        for (&i, orig) in &originals {
            let (name, data, _) = &mut entries[i];
            if !files.contains(name.as_str()) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(orig) else { continue };
            let (cleaned, notes) = collect_one(text, name, referenced).unwrap_or_else(|| (text.to_string(), Vec::new()));
            index.extend(notes.into_iter().map(|(id, inner)| ((name.clone(), id), inner)));
            *data = cleaned.into_bytes();
        }
    }
    index
}

/// 第二遍的"文本类条目"变换器：html 章节 / （改书名、打漫画标签时）OPF。跨条目状态（远程图计数、全书 id 去重表、
/// 抓到的远程图）都在这里。图片条目不归它管（走 `imgpool` 并行）。
struct EntryXform<'a> {
    aside_index: &'a HashMap<crate::htmlproc::NoteKey, String>,
    skip_notes: &'a HashSet<String>,
    footnote: FootnoteMode,
    /// 漫画：OPF 里打上漫画标签（`comic_detect::tag_opf_as_comic`）。
    comic: bool,
    opf_name: Option<&'a str>,
    seen_ids: HashSet<String>, // 跨章累积，dedup_ids_in_chapter 用
    /// 改书名（`Some` 时把 OPF 的 `<dc:title>` 改成它，见 `StreamingOptimize::title`）。
    title: Option<&'a str>,
    img_agent: ureq::Agent, // 远程图抓取（仅当章内有远程 img 才发请求；抓不到 → 原样保留）
    remote_counter: usize,
    /// zip 里已有的条目名（含已抓到的远程图）：新抓的图不能跟它们重名。
    taken_names: HashSet<String>,
    /// 抓到的远程图 (zip 路径, 字节)，结尾写进 zip 并补进 manifest。
    fetched_imgs: Vec<(String, Vec<u8>)>,
    /// 清洗过的书：各 XHTML 最终内容用到的特性（zip 路径 → `wash::normalize::content_properties`），结尾写进 manifest 的 `properties`。
    content_props: Option<HashMap<String, u8>>,
}

impl<'a> EntryXform<'a> {
    fn new(prep: &'a Prepared, opts: &OptimizeOpts) -> EntryXform<'a> {
        EntryXform {
            aside_index: &prep.aside_index,
            skip_notes: &prep.skip_notes,
            taken_names: prep.entries.iter().map(|e| e.0.clone()).collect(),
            footnote: opts.footnote,
            comic: prep.is_comic_book,
            opf_name: prep.opf_name.as_deref(),
            title: prep.title.as_deref(),
            seen_ids: HashSet::new(),
            img_agent: crate::netimg::http_agent(15),
            remote_counter: 0,
            fetched_imgs: Vec::new(),
            content_props: opts.wash.as_ref().map(|_| HashMap::new()),
        }
    }

    /// 章节 html 最终变换链：解双向脚注互指环 → duokan 图片脚注标记换上标 → 封面拉伸/SVG 修复 → 脚注就地关联重排 →
    /// 远程图内联 → 全书 id 去重。要用到第一遍扫全书才拿得到的 `aside_index`，所以与第一遍分开、顺序不能换。
    fn transform_html_chapter(&mut self, text: &str, name: &str) -> Vec<u8> {
        let t = crate::htmlproc::break_footnote_cycles(text);
        let t = crate::htmlproc::fix_duokan_markers(&t);
        let t = crate::htmlproc::number_icon_note_links(&t);
        let t = fix_cover_aspect(&t);
        let t = svg_cover_to_img(&t);
        let t = if self.skip_notes.contains(name) { t } else { crate::htmlproc::preserve_relink_footnotes(&t, name, self.aside_index, self.footnote) };
        let chap_dir = std::path::Path::new(name).parent().and_then(|p| p.to_str()).unwrap_or("");
        let (t, imgs) = inline_remote_images(&t, chap_dir, &mut self.remote_counter, &mut self.taken_names, remote_img_fetcher(&self.img_agent));
        self.fetched_imgs.extend(imgs);
        crate::htmlproc::dedup_ids_in_chapter(&t, &mut self.seen_ids).into_bytes()
    }

    /// 文本类条目 → `Some(最终字节)`（无法按 UTF-8 解读的原样借回）；不是文本类（图片/其它）→ `None`，调用方自己处理。
    fn transform_text<'d>(&mut self, name: &str, data: &'d [u8], is_html: bool) -> Option<std::borrow::Cow<'d, [u8]>> {
        use std::borrow::Cow;
        if is_html {
            return Some(match std::str::from_utf8(data) {
                Ok(text) => {
                    let out = self.transform_html_chapter(text, name);
                    if let (Some(props), Ok(t)) = (self.content_props.as_mut(), std::str::from_utf8(&out)) {
                        props.insert(name.to_string(), crate::wash::normalize::content_properties(t));
                    }
                    Cow::Owned(out)
                }
                Err(_) => Cow::Borrowed(data),
            });
        }
        if self.opf_name == Some(name) && (self.title.is_some() || self.comic) {
            let Ok(text) = std::str::from_utf8(data) else { return Some(Cow::Borrowed(data)) };
            let mut text = Cow::Borrowed(text);
            if let Some(title) = self.title {
                text = Cow::Owned(crate::placeholder::set_opf_title(&text, title));
            }
            if self.comic {
                if let Some(t) = crate::comic_detect::tag_opf_as_comic(&text) {
                    text = Cow::Owned(t);
                }
            }
            return Some(match text {
                Cow::Borrowed(_) => Cow::Borrowed(data),
                Cow::Owned(t) => Cow::Owned(t.into_bytes()),
            });
        }
        None
    }
}

/// 内存版：字节进字节出（格式转换器组装后的收尾、测试与小书用）。内部把字节落到临时文件走同一条流式路径
/// （[`StreamingOptimize`]），两条路径的业务逻辑只有一份，不会分叉走样。缺省选项（不清洗、`Anchor` 注释）。
pub fn optimize_epub(epub: &[u8]) -> Result<(Vec<u8>, Report), String> {
    optimize_epub_with(epub, &OptimizeOpts::default())
}

/// 同 [`optimize_epub`]，带选项：`opts.wash` 有值则先过清洗层（真 DRM 在此报错、原样不动）。
pub fn optimize_epub_with(epub: &[u8], opts: &OptimizeOpts) -> Result<(Vec<u8>, Report), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir();
    let tag = format!("bookconv-opt-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    let (input, output) = (dir.join(format!("{tag}.in.epub")), dir.join(format!("{tag}.out.epub")));
    let result = (|| {
        std::fs::write(&input, epub).map_err(|e| format!("写临时文件失败: {e}"))?;
        let mut rep = StreamingOptimize::new(&input, &output, opts).run(|_, _| {})?;
        let out = std::fs::read(&output).map_err(|e| format!("读临时产物失败: {e}"))?;
        rep.bytes_before = epub.len();
        rep.bytes_after = out.len();
        Ok((out, rep))
    })();
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    result
}
