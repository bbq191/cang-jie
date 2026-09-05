# bookconv 电子书优化白皮书

> `shelf/crates/bookconv` —— 通用电子书内容层：多格式转换 + EPUB 优化器 + 清洗层 + 质量门。记"为什么这么做、真机怎么验、踩了什么坑"，尤其 **xochitl（reMarkable EPUB 渲染器）的硬规则**（§09，本项目最贵的一批真机知识）。上层用法/服务见 `reMarkable书架白皮书.md`（book-serve 母版库「优化」/「转 PDF」、host `shelf push`；KOReader 只从母版库纯复制落书，不再单独优化）。

## 00｜定位与职责

块③ 阅读的**内容层**，2026-09-03 从 `weread-device` 抽成独立 crate（`git mv`，不复制）。四块职责：
1. **格式转换 `convert`**：AZW3/MOBI/FB2 → EPUB、CBZ → PDF，**纯 Rust clean-room、零 C 依赖**。
2. **EPUB 优化器 `optimize`**：吃任意结构 EPUB，就地改每个 (x)html（解字体锁 / 脚注 / 远程图 / 图片降采样 / 对比度 / 双 id），其余文件结构不动、重打包。
3. **清洗层 `wash`**：对标 host Calibre `wash_epub.sh` 的规则（伪 DRM / CSS 锁 / 边距段距 / 自动目录 / 空页 / 外链排版 css），由优化器可选前置调用。
4. **质量门 `check`**：只读体检（真 DRM / 目录命中率 / 双 id 非法），硬失败应拦下投递。

**一份代码两处共用**：设备 `book-serve` 母版库「优化」（`Staging::optimize`）与 host CLI `epub-optimize` 都调 `optimize::optimize_epub_with`；host `wash_epub.sh` 末步也叠加同一个 `epub-optimize` 二进制——**端 / host 优化同源**（书架白皮书 §03i / §03q）。漫画同理：设备母版库「转 PDF」与 host CLI `cbz2pdf` 都调 `convert::cbz::cbz_to_pdf`。〔koreader-serve 自 §03s 起不再优化——落库＝纯复制母版字节。〕

**零 C 依赖原则**：EPUB 组装（`epub.rs`）全条目走 STORED（不压缩，免 zlib C 依赖，设备空间充足）；漫画 PDF 手搓（`pdfwrite`：JPEG 直嵌 `/DCTDecode`、PNG 走 `png` crate + miniz_oxide `/FlateDecode`）；MOBI/KF8 解析不依赖 `mobi` crate（它在真机词典样本上把 `extra_data_flags` 尾字节判错、解压乱码，见 `palm.rs`）。

## 01｜架构：`optimize_epub_with` 两遍 + wash 前置

`optimize_epub_with(epub, &OptimizeOpts{wash, footnote})`：
- **读全条目**（`check::read_entries`，读失败整体报错——绝不静默跳过条目产出残缺 EPUB 破坏原书）。
- **wash 前置**（`opts.wash=Some` 时）：作用于解包后的**条目表**（`Vec<Entry>`），见 §02。
- **重排**：`mimetype` 首个 STORED（EPUB 规范），其余原序；旧优化标记剔除。
- **第一遍**：每个 (x)html → `strip_font_locks`；扫全书 marker 得**被引用**的尾注 frag 集（`referenced`）；判目录页（指向 ≥ 阈值个不同 html）。后半：把被引用的注释块从各章移除、建全书 `aside_index`，交第二遍搬进引用章。
- **第二遍**：`preserve_relink_footnotes`（按 `FootnoteMode`）→ `break_footnote_cycles` → `fix_duokan_markers` → `inline_remote_images` → 图片降采样 → `boost_text_contrast` → `dedup_ids_in_chapter`（跨章 id 去重防撞车）→ 打包。
- **幂等标记**：产物写 `META-INF/com.cangjie.optimized` = `OPTIMIZE_VERSION`。`is_current_version()` 判是否当前版本；旧版本重传**重优化升级**（`double_optimize_*` 测试坐实重优化不翻倍脚注）。

`FootnoteMode`：`Anchor`（缺省，注释移章末 + 同章锚点跳转 + 原生「返回」浮标；weread/pkm/第三方书历史行为）· `Inline`（就地内联 `〔…〕` 常显，native→xochitl 用，见 §04/§09）。

## 02｜清洗层 wash（`wash_entries`，对标 Calibre）

按序作用于条目表（`WashOpts{keep_para_spacing, auto_toc, filter_props, lang}`）：
1. **伪 DRM 剥离** `strip_pseudo_drm`：`META-INF/encryption.xml` 只加密样式/字体/脚本（字体混淆合法）→ 丢弃这些文件 + 删 OPF manifest 项。**加密了正文/图片 = 真 DRM → 报错停下**（不产残书）。
2. **空页清理** `remove_empty_pages`：无文字无图的页（Calibre MOBI 转出的 `mbppagebreak` 独占页）→ 从 spine / manifest / zip 删。
3. **主语言探测** `detect_dominant_script`：全书 CJK vs 拉丁字符占比 → `LangMode::{Cjk,Latin}`（`Auto` 在此解析）。
4. **CSS 锁剥离** `filter_css`：独立 `.css` / `<style>` / `style=` 三处剥 `DEFAULT_FILTER_PROPS` = `font-family, font-size, font, color, background-color, background-image, background, text-align`。`@font-face` 的 `src:url` 豁免。`@media{}` 嵌套从内向外匹配最内层规则。
5. **边距/段距** `wash_html`：`style=` 按标签名定策略（body/html 全删边距、p/div 上下归零左右保留——不伤 blockquote/列表缩进）。
6. **外链排版 css**（§09④ 治本）：`add_wash_css_entry` 写外链 `cangjie-wash.css`（`p{text-indent:2em;margin-top:0;…}`）+ 每章 `<link>` + OPF manifest 补 item。
7. **自动目录** `auto_toc`（§06）· **单标签双 id 折叠** `collapse_dup_id_attrs`（§09 双 id 白屏）。

`background`/`background-image` 进 `filter_props` 是 v8 真机《飘》修（§09②）。`keep_para_spacing` 给诗集/剧本（靠空行分节，不归零段距）。

## 03｜优化器核心遍（always-on，不依赖 wash）

- **字体解锁** `strip_font_locks`：第三方书常内联硬写死 `font-family`——设备字体设置改不动。剥内联/`<style>`/`.css` 的字体锁（与 wash 的 filter_css 互补，wash 关时仍剥）。
- **脚注** `preserve_relink_footnotes` / `inline_footnotes`（§04）。
- **远程图内联** `inline_remote_images`：微读等下载书内嵌 `res.weread.qq.com` 远程图，设备离线加载不到 → reMarkable 破图占位 = **大放大镜**。抓下降采样内联进 zip（与本章同目录、src 改本地名）；抓不到 → **删该 `<img>`**（放大镜必消，图丢但离线本就是放大镜，删胜于留）。⚠ reMarkable 按 src 直渲图、**不查 manifest**（连不在 manifest 的远程 URL 都尝试渲染才有放大镜），故本地图同样不进 manifest 也直渲（真机验证）。
- **图片降采样** `imgopt`（§05）· **e-ink 提对比** `boost_text_contrast`（灰字→纯黑、细字重→400）· **双 id 去重** `dedup_ids_in_chapter`。

## 04｜脚注：四形态 + 两引擎 + FootnoteMode

**四种脚注形态**（真机《13·67》《人骨拼图》混合）：① 同章 `noteref`↔`aside`；② 跨文件 `<a href="notes.xhtml#nX">` + `<p id="nX">` 尾注；③ duokan 图片脚注标记；④ 双向互指对（marker↔note 互链——reMarkable 索引器遇互指**整对丢弃**，`break_footnote_cycles` 拆环）。

**抽取管线**：第一遍 `referenced_note_frags` 扫出被引用的注释 id → `collect_footnote_notes` 把被引用的注释块（aside/p/li/div，嵌套 div 跳过零丢失）从各章移除、建 `aside_index` → 第二遍 `preserve_relink_footnotes(html, index, mode)` 搬进引用它的那一章。未被引用的块原样留原处（零丢失原则）。

**FootnoteMode**：
- `Anchor`：marker 改**同章朴素文字锚点** `<a href="#nX">1</a>`（xochitl 可点铁律：只有同章朴素文字锚点可点，`<a><img></a>` 不可点，注释区必须在 `</body>` 内），注释移章末 `<div class="footnotes">`，原生浮标返回。
- `Inline`：注释文字就地内联 `<span class="cj-fnote">〔纯文本〕</span>` 始终可见、不跳转——xochitl **无弹窗脚注**（穷尽真机判死），Inline 是它的「自动呈现」。⚠ 必须**丢弃原 marker**（若 marker 是图标 `<img>` 会按固有尺寸巨大重复，§09①）+ **去标签取纯文本**（防块级标签塞进 `<p>` 致严格 XML 整章白屏，§09 双 id）。

`normalize_self_hrefs`：Calibre 写法 `part0004.html#x` 写在 `part0004.html` 里 → 先归一成裸锚 `#x`，否则被误当跨文件脚注。

## 05｜图片：按 Move 屏两个降采样盒

Move 屏 = **954×1696 px、7.3″、264 PPI、Gallery 3 彩色墨水屏**。书常带 2000–4000px 高清图。常量 `MAX_EDGE=1696` / `MAX_SHORT_EDGE=954`、JPEG 质量 85、Lanczos3。**两个降采样函数**（v8 真机《飘》拆分）：
- `downscale_for_epub`（EPUB 内嵌图，**竖向框** 954×1696、宽绝不超 954）：防行内横幅图（`class="logo"` 1696×630）按固有 1696px 宽**溢出竖屏**（§09①）。
- `downscale_for_device`（CBZ 漫画整页，**朝向框**：横图 1696×954 / 竖图 954×1696）：漫画横读满宽。

真机探针：xochitl 块级 `<img>` 缩到正文列宽、行内 `<img>` 按**固有像素**渲染、**都不认 CSS em**（公式图偏小的根因）；长边 ≤1696 且短边 ≤954。彩图饱和度阈值 `COLOR_KEEP_CHROMA` 判是否保色（墨水屏波形按内容分档，彩重/灰中/1bit 轻）。

**取尺寸只读头**（2026-09-05）：`header_dims` 用 `ImageReader::into_dimensions` 只解 JPEG/PNG 头，达标页零解码零重编码（原先为判尺寸整张解码甚至两次，2473 页漫画 2m36s → 1m49s，产物字节不变）。

**1-bit 抖动的体积真相**：Floyd–Steinberg 位图对 Flate 是高熵噪点——927×1327 一页压后仍 ~90KB，只比 120KB JPEG 小 1/4；"~1/8"是相对 8-bit 灰 Flate 说的。要真省体积得换 CCITT G4 / JBIG2（待办）。用户定漫画默认**原图**（`EinkTone::Off`），`--mono` 可选。

## 06｜自动目录（`auto_toc`，缺目录才建）

`AutoToc::IfMissing`（缺省）仅在无 nav/ncx 或零条目时生成。`heading_re` 从 h1/h2 **扩到 h1–h6**（v7：只用 h3 当章标题的书不再漏目录）；`dense_ranks` + level-stack 生成**多级嵌套** navPoint/`<li>`（`d = ranks[i].min(depth+1)` 钳制层级不跳级）。质量门 `check` 按目录锚点命中率告警（丢失则 xochitl 退化到文件级跳转）。

## 07｜格式转换 `convert`（纯 Rust，零 C）

xochitl 原生只开 EPUB/PDF：**文本类 → EPUB、漫画类 → PDF**，产物再走优化器 + `/upload`。
- **共享底座 `palm`**：MOBI6 与 KF8/AZW3 是同一 PalmDB + PalmDOC + EXTH 容器（记录表 / 解压 / `extra_data_flags` 尾字节剥离 / EXTH / 图片 / 封面统一在此）。
- **`mobi`**：PalmDOC 解压得整本 HTML（`<mbp:pagebreak>` 分页、`<img recindex>` 引图、`<a filepos>` 内链）→ `epub::Book`。
- **`kf8`**：AZW3 clean-room（依 KF8/MobileRead wiki，**不抄 GPL 的 KindleUnpack**）→ 预组装 XHTML 序列。
- **`fb2`**：quick-xml serde 反序列化 → `epub::{Book,Chapter,Resource}`（EPUB 组装 + 优化全交给 epub.rs/optimize.rs，同一 assemble→optimize 路）。
- **`cbz`**：解包 + 图片自然序排（`natural_cmp`）→ 每页 `downscale_for_device` → `pdfwrite` 手搓 PDF。CLI `cbz2pdf [--mono] in.cbz out.pdf`。
- 各格式的 EPUB 字节组装统一交 `epub.rs`（最小合规 EPUB3，移植自 protocol/epub.py）。
- **谁在用（2026-09-05）**：`cbz` 被 shelf 母版库「转 PDF」与 host 漫画通道（`shelf push` 漫画 → `comic2cbz.py` → `cbz2pdf`）调用；`mobi`/`kf8`/`fb2` 只剩 `reading/device-rs`（ingest / 旧上传页）在用——shelf 里杂格式进原生统一走电脑 Calibre（书架白皮书 §03s），设备端不再转。`is_ingestible`/`precheck` 同理属 reading 线。

许可：clean-room 依格式规范实现，不抄 GPL 代码；GPL 数据（词典等）不编译进产物。

## 08｜质量门 `check`（对标 host `check_output.py` EPUB 项）

只读、不改书。硬失败（`ok=false`）拦下投递：① 真 DRM（`encryption.xml` 加密非字体项）；② 目录命中率（`require_toc` 时无目录升为失败）；③ 单标签双 id 非法（会让 reMarkable 整章白屏，§09）。告警（不拦）：无 nav/ncx、目录锚点丢失。PDF 门（pymupdf）**不移植**——PDF 定稿只在 host 产出，门留 host。**现状（§03s 后）**：质量门只在 host `shelf push`（`_gate` → `check_output.py`）和 `epub-optimize --check` 跑；设备端母版库「优化」不跑门（优化器自身已折叠双 id）。

## 09｜★xochitl 渲染硬规则（真机坐实，做优化器必须绕开）

reMarkable 的 EPUB 渲染器闭源，行为多次跟 host / 常识不一致。以下全部真机坐实（多为《飘·上册》逐页核 + 缩进 7 版诊断书）：

**① 行内 `<img>` 按固有像素尺寸渲染、不认 CSS**（块级 img 才适配列宽）。后果：图标脚注 marker（`<img alt="note" 70×95>`）内联保留 = 巨大且每页重复；行内横幅（1696×630）固有宽 1696 > 954 → 溢出竖屏。修：`FootnoteMode::Inline` 丢弃 marker 只留纯文本；EPUB 图走竖向框 `downscale_for_epub`（宽 ≤954）。

**② 无视 CSS `background-repeat:no-repeat`/`background-size`，把 `background-image` 平铺满页盖正文**。《飘》分卷页 `body.fen{background:url() no-repeat}` 被铺成多幅风景盖住"第一卷"。修：`filter_props` 加 `background`/`background-image`（`@font-face src:url` 豁免、章头 `<img>` 装饰不受影响）。

**③ 章头装饰 `<img>`（每章一张 banner/logo）是要保留的正常内容**，不是"重复图"。曾误当"同图跨 ≥5 章重复 → 删其 img"，被用户照片（IMG_0447 判「对」）纠正、`git revert`。跨章重复的 `<img>` ≠ 可删。

**④ ★只认外链 `.css` 文件里的规则，完全无视内联 `<style>` 块和元素 `style=` 属性**（v10 根因）。⟹ **v6–v9 注入的内联 `<style class="cj-wash">` 排版规则（边距归零 / 首行缩进）在 xochitl 从来没生效过**！能生效的只是"删改源文件"类（剥字体锁 / 去背景图）。活样本：《飘》自带外链 `p{text-indent:2em}` 显示缩进，我们内联注入都不显示。且 **css 解析器极脆**：**不认 `!important`**（带它整条失效）、**不认类/相邻/at-rule 选择器**（外链 css 里混一条 `.x{}` 就让整表失效）——**只吃裸元素选择器 `p{}`**。修：`wash` 写外链 `cangjie-wash.css`（`p{text-indent:2em;margin-top:0;…}` 裸选择器、无 `!important`）+ 每章 `<link>`（`relative_to` 算相对路径）+ manifest item。text-indent 用 em = **字体无关、精确 2 字**。

**⑤ 无弹窗脚注**（穷尽实测判死，正文点击不通知可注入的 QML 层）→ Inline 内联常显是唯一"自动呈现"。**⑥ 单标签双 id → 整章空白**（严格 XML 解析吞整章，`collapse_dup_id_attrs` 折叠）。**⑦ 无 PDF 重排**（固定版式）→ PDF 重排只在 host。

**缩进死路（v9 试尽已撤回，记录以免重走）**：段首 `&nbsp;`（宽随字体变 + 连续被折叠，做不到精确 2 字）、全角空格 U+3000 / em 空格 U+2003（被 xochitl 吞 = 前置空白折叠）、`inline-block` 占位（无视）。唯外链 css `text-indent` 可行。

**方法论**：**看不到屏幕别猜渲染**——让用户拍照（HEIC 用 `heif-convert` 转 jpg；模糊/倾斜时投影去斜量左边缘，但 e-ink 残影常测不准，不如让用户直接文字答）。**有真机上「有效」的活样本（如《飘》自带缩进）时，第一时间解剖它＞凭空造七版诊断**。诊断书投递用 `optimize=off` 保原始标记不被清洗。

## 10｜版本演进（`OPTIMIZE_VERSION`，bump 后旧书重传升级）

| v | 改了什么 |
|---|---|
| v2 | duokan 图片脚注标记修复 `fix_duokan_markers` |
| v3 | 按 Move 屏设备优化：图片降采样 1696 长边 + e-ink 提对比 |
| v4 | 远程图内联（微读书内嵌远程 img 离线破图→抓取降采样内联 / 抓不到删） |
| v5 | Calibre 洗书形态：`normalize_self_hrefs` 归一裸锚 + 真 img 版 duokan 换上标保 id + 图短边 ≤954 约束 |
| v6 | 清洗层 `wash` 可选前置（伪 DRM / CSS 锁 / 边距段距 / 空页 / 自动目录 / 双 id） |
| v7 | 做精做强：中英文各按习惯排版（`LangMode` 探测）+ 自动目录 h1/h2 扩到 h1–h6 多级嵌套 |
| v8 | 真机《飘》三修：内联脚注丢图标 marker（§09①）+ EPUB 图竖向框防溢出 + 剥 CSS 背景图（§09②） |
| v9 | 〔已撤回〕段首 nbsp 首行缩进——nbsp 宽随字体变 + 被折叠，做不到精确 2 字（§09 死路） |
| **v10** | **首行缩进根治：排版规则改外链 `cangjie-wash.css`**（xochitl 只认外链 / 不认内联 `!important` / 类选择器，§09④）。撤回 nbsp。 |

（幂等门修：`is_optimized` 曾只看标记存在不看版本 → 旧版本重传被整步跳过、拿不到新改进；改 `is_current_version` 按版本升级。）

## 11｜host / 端一致 + CLI

**同源**：设备 `book-serve` 母版库「优化」与 host `epub-optimize` 二进制都调 `optimize_epub_with`；host `wash_epub.sh` 末步叠加同一 `epub-optimize`。host 只多一层 Calibre 级 CSS 拍平 + 非 EPUB/PDF 转码 + 质量门。

**CLI `epub-optimize`**（`cargo build --release -p bookconv --bin epub-optimize`）：`[--no-wash] [--keep-spacing] [--auto-toc] [--footnote-anchor] [--check] [--require-toc] 输入.epub 输出.epub`。缺省 = 清洗 + 优化 + 脚注 Inline（对齐 native→xochitl）。退出码 0 成功 / 1 用法 / 2 优化失败（输入不动）/ 3 质量门未过。

**CLI `cbz2pdf`**（`--bin cbz2pdf`）：`[--mono] 输入.cbz 输出.pdf`，退出码 0/1/2。host 漫画通道与设备母版库「转 PDF」同一函数。

**目标脚注**：母版库「优化」传 `Inline`（xochitl 无弹窗，内联常显）；KOReader 从母版库纯复制拿到的也是 Inline 产物（crengine 弹窗需 `Anchor`+`epub:type`，是否为 KOReader 另跑 Anchor 待用户看观感，书架白皮书 §05①）；weread/pkm 线 `Anchor` 兜底。

## 12｜踩坑

- **重优化跨版本脚注不得翻倍**：v6 产物（注释已移章末 + 同章锚点 marker）跑 v10 重优化——`double_optimize_inline_footnote_no_dup` / `reoptimize_relinked_footnote_no_dup` 测试坐实注释只出现一次。
- **非 ASCII 字符边界 panic**：`whole[..4].eq_ignore_ascii_case("<div")` 在中文字节边界 panic → 改 `as_bytes().get(..4)` 字节比对。
- **Rust 裸串**：`r#"...href="#fn1"..."#` 被 `"#` 提前闭合 → 用 `r##"..."##`。
- **读条目失败必须整体报错**：静默跳过条目 = 产残缺 EPUB 破坏原书。
- **wash 撞名**：PDF 重排产物 `<work>/X.epub` 被 wash 按同标题算出同名 → `ebook-convert` 报 in==out；`wash_epub.sh` 规范化路径比对，撞了换 `.washed.epub`。

## 13｜真机待办

英文书拉丁排版（1.2em）真机观感；KOReader 里 Inline 内联脚注能否接受（否则落库时另跑 Anchor + `epub:type` 触发弹窗）；PDF 结构化重排（host `pdf_reflow_move.py`）杂志观感已通（财新 v3），学术论文多列/公式待验；公式图 intrinsic 放大阈值；mono 档 CCITT G4 编码。诊断法：xochitl 导入渲染 `<uuid>.pdf` scp 回 host、pymupdf 量列宽/图尺寸/outline/内链 kind。
