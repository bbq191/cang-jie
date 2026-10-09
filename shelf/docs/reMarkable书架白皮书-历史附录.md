# reMarkable 书架白皮书 · 历史附录

> **这是什么**：从[书架白皮书](reMarkable书架白皮书.md)挪出来的纯历史章节——已经移除的能力（2026-10-07 前的设备端优化、入库 PDF 转换、KOReader、漫画转 PDF / 分卷等）的当年做法、真机排查经过，以及第五轮审计（§03bu）的完整清单。
> **为什么留着**：xochitl 的 EPUB 渲染器闭源、行为反常识，这里记的真机量出来的规矩、排查方法和事故教训仍有参考价值；别处文档与代码注释引用的 § 编号也要能查到。**这里描述的功能都不是现役**，现状看主白皮书「给新读者」「现状总览」。
> 主白皮书里这些 § 的标题原样保留、正文换成指向本文的一行链接；本文各节标题与原编号一致。文中提到的其他 § 编号（如 §03bw、§03bx）在主白皮书里。

## 第 B 章坑位表（设备端优化时期）

### 坑位表（一）

> 下面两张坑位表都是设备端优化时期的；涉及 xochitl 行为的规矩（只认外链 CSS、尾分号、`id="ncx"` 等）现在记在 sheng-ren `docs/xochitl.md`。

| 坑 | 症状 / 根因 | 规避 | 见§ |
|---|---|---|---|
| 内联排版规则不生效 | xochitl 只认外链 `.css` | 规则写外链 `cangjie-wash.css`；一个真机活样本胜过七版凭空诊断 | §03q · §03y |
| 声明缺尾分号被吞 | 最后一个无分号声明被丢 | 输出一律带尾分号 | §03y |
| `text-indent:0` 无效/继承 | `0` 被当"没设" | 写 `0.01em` + `cj-flush` 并剥书的类 | §03y |
| 旧版本书升不了级 | 幂等门只看标记存在 | 标记版本与当前比对 | §03q |
| 图标脚注巨大 / 背景图平铺盖正文 | 行内图按固有像素；无视 `no-repeat` | 丢弃图标 marker；剥 `background*` | §03q · §03bs |
| 误诊：banner 当"重复图"删 | 真凶是 CSS 背景图 | 看不到屏幕别猜，让用户拍照 | §03q |
| 中文书零缩进 | 全书无 `<p>`，段首 U+3000 被折叠 | `cjk_paragraphize` | §03y |
| 修了 figure 边距留白没变 | 分页引擎把放不下的图块整体挪页 | 改前后逐页像素一致才是证据 | §03aq |
| 弹窗脚注做不成 | 闭源渲染器无弹窗 | "跳转+浮标"（当年只有 KOReader 有弹窗，09-29 已卸载） | §03q |
| 放开 `background-color` 风险 | 深底浅字块在彩墨屏或更难读（未验证） | 挑带彩色底纹的书真机看 | §03av |

## 第 B 章 优化管线（历史）

### 03e｜Phase 4 原生高质量门 · Phase 5 微读 spike 脚本（2026-09-03，历史）

host 质量门 `check_output.py` 当时移植成 `bookconv::check`；host 门与微读 spike 脚本都已不在仓库。**现状**：`bookconv::check` 09-23 起接入设备端「优化」（§03bs）。**教训**：质量门不在真实流程里跑就只是摆设——09-23 的 `../` 路径 bug 本可被门拦下。

### 03i｜host 优化 vs 设备端优化：对标现状与移植计划（2026-09-03 傍晚，历史）

当时优化器两边同源、清洗层与质量门不对标，用户拍板把 Calibre 那层规则移植进 `bookconv::wash`；host 洗书链 09-18 砍除。当时的清洗规则见规范白皮书 §4 与 bookconv 白皮书 §02（两份 10-07 删除，见 git 历史）。数据留档：《飘·上册》（多看伪 DRM，10.6MB）设备端优化 90s，35 章、10631321→3183269 字节。**教训**：漫画几乎全是图，清洗层对它无意义，对照测试要换文字书。

### 03q｜书籍优化深层优化：做精做细做强（2026-09-04，用户"只做精做细做强"）

四诉求：格式仅留 EPUB+PDF；按 Move 屏参数优化；图片美观、中英文各按习惯、不锁字体、不缺目录、脚注自动呈现；xochitl≈KOReader。

- **两条判死**：xochitl 弹窗脚注（闭源渲染器不实现、正文点击不通知 QML）；端上 PDF 重排（当时无 musl 友好的纯 Rust 方案）。
- **Phase F 真机（《飘·上册》用户拍照逐页核，v8）**：图标 marker 按固有尺寸渲染→丢弃；内嵌横幅溢出竖屏→降采样到 954 宽；背景图平铺盖正文→剥 `background*`。
- **误诊教训**：把"分卷页多幅图"当成"章头 banner 重复"加了删图规则——错，已 `git revert`；真凶是 CSS 背景图。
- **幂等门只看标记存在、不看版本**（根因，波及全部 v7+ 改进）→ 按版本比对，旧书重优化升级；重优化注释不翻倍由两条测试守住。
- **首行缩进根因终结（v10）**：xochitl 只认外链 `.css`，v6–v9 注入的内联 `<style>` 从未生效 → 排版规则写外链 `cangjie-wash.css` + 每章 `<link>` + manifest 补项；中文 2em / 拉丁 1.2em。段首 `&nbsp;`、U+3000、`inline-block` 占位都是死路。

### 03y｜xochitl CSS 引擎实测规则（2026-09-06，八轮"诊断 EPUB"量渲染缓存，3.28.0.172）

> **规则正文**先迁到《EPUB 优化规范白皮书》§3，2026-10-07 那份删除后由 sheng-ren `docs/xochitl.md` 维护；本节只留方法与配方的由来。

![xochitl 吃 CSS 的规矩](diagrams/xochitl-css-rules.svg)

**方法**（可复用）：造只有一个变量的诊断 EPUB（每章一种写法 + 同章一个对照段落）→ 投原生 → 量 xochitl 渲染出的 `<uuid>.pdf` 里每段首行 x 偏移。不需肉眼，一轮 2 分钟，共 60 个变体。
**七条结论**：尾分号、`0` 当没设、类规则认且压元素、同类先出现者胜、不认内联 `style`、`text-indent` 继承、`!important` 不认（旧"不认类选择器"是尾分号规则的误读）。
**最终配方**：顶格段 → `<div class="cj-flush">`（剥书的类与 style）+ 外链 `.cj-flush{text-indent:0.01em;…;}`；真机章首/场景切换 0pt、续段 14.2pt。**中文书**：《人骨拼圖》全书没有 `<p>`（`<br/>` 分行 + 段首 U+3000）→ `cjk_paragraphize` 按 br 切段、剥全角空格；真机 278 个段首 24.1pt（=2em）。

### 03aq｜真机排查"抓完优化还是大量留白"：wash 补 figure/figcaption 边距 + 证伪 + 找到真正成因（2026-09-10，真机 A/B 验证）

- **结论**：补了真实缺口（`figure{}`、`figcaption{}` 两条**分开写**的边距清零），但**真机 A/B 25 页逐页像素一致**——不是留白成因。真因：图片块在页尾放不下时整体推到下一页、余下空间不回填，是分页引擎行为，EPUB/CSS 层无杠杆。
- **未修**：aeon.co 轮播被整组抽出（内容更全但更长，是取舍）；"1 of 4"页码混入正文（疑来自 readability 转写，未验证）。
- **教训**：结论是"改了一处真实缺口，但没解决用户的问题"，不能写成"已解决"。

### 03av｜书籍优化架构调整：拆 EPUB 线/PDF 线，EPUB 线四原则落地（2026-09-17，功能层真机验证）

〔2026-10-07：设备端优化删除（§03bw），本节压缩成结论，完整表格见 git 历史。〕

用户当时拍板：设备端只收 EPUB/PDF；EPUB 优化全部在设备侧；EPUB 线四原则——① 保留目录并拆分章节序号；② 解锁字号、保留原书颜色/加粗；③ 注释用 `Anchor`（试过"段末插注释"，流式重排做不到"注释在本页最下面"，删掉）；④ 漫画自动识别（图 ≥20 且每图文字 <40 字）+ 裁边。AZW3/MOBI/PRC 连带禁止——**主动放弃一条已验证可用的能力，为规则一致性**。
**教训**："按设计跑通"与"设计是用户想要的"是两件事。

#### 本章坑位表（二）

> 术语：NCX = EPUB2 目录文件 `toc.ncx`；manifest = `content.opf` 里的文件清单；`.epubindex` = xochitl 给每本 EPUB 生成的内部索引（目录条目 → 页码）。

| 坑 | 症状 / 判据 | 根因 | 规避 | 见§ |
|---|---|---|---|---|
| 优化/落库是同步 HTTP | 大漫画请求卡死；处理中点删除会真删 | 同步阻塞 + 无"正被处理"状态 | 服务端忙锁 + 异步 + `busy` 字段 | §03aw · §03ay |
| 书内 HTML 目录页从 spine 消失 | 翻页翻不到目录页 | 早期启发式"链到 ≥10 个 html 就当冗余目录页" | 整段删除；目录页是作者放的正文 | §03ay |
| 原生目录入口整个不出现 | `.epubindex` 每条只有路径、没有标题 | xochitl **硬编码查** manifest `id="ncx"` | NCX 条目 id 必须叫 `ncx` | §03bc |
| 对照两本书就下"根因" | 修完字节全对，入口仍不出现 | 两书差异不止一项 | 逐项单变量隔离；"字节正确"≠"设备认" | §03az · §03bb |
| 优化进度条不动 | 所有阶段都无进度 | `OptimizeCheck` 没有 `progress` 字段 | 逐条目回调、每 5 条节流 | §03bg |
| 封面缩略图取不到 | `got null cover image` | 封面 id 带点且只有 meta；或 meta 指向非图片 | meta 与 `cover-image` 同时写，清洗前补 | §03bo |
| 注释源自带 `<p>` 得嵌套 `<p><p>` | 内容模型非法 | Anchor 分支固定包 `<p id>` | **已修（09-23）**：容器改 `<div id>` | §03aw · §03bs |
| 字体锁"清不掉" | 某几章字体始终锁死、脚注没处理 | 章节文件无扩展名，按扩展名判断整体跳过 | 按内容嗅探（`is_html_entry`） | §03bs |
| 优化产物有坏链接却照样替换母版 | 图片 src 指向不存在的文件，没人发现 | 质量门没接入设备端流程 | 门在替换前跑，不过就放弃产物 | §03bs |

#### 《疯探》"没有 TOC"排查链总览（§03ay → §03az → §03bb → §03bc）

同一本书（"番茄小说 EPUB Generator"产物，94 章）连续多轮反馈，前几轮归因都不对——这是整条线最值得复用的方法论样板。

| 轮次 | 假设 | 结果 |
|---|---|---|
| ① §03ay | 优化器把书内目录页摘出了 spine | **成立但是另一个 bug**：修后目录页保住，原生目录入口仍没有 |
| ② §03az | `dtb:uid` 与 OPF 标识符不一致 | **被证伪**（修复保留，规范要求） |
| ③ §03ba | `toc.ncx` 外部 DTD 让解析卡住 | **被证伪**（修复保留，无害） |
| ④ §03bb | 逆向 `.epubindex` 当判据，"真书重打包 + 单点替换"二分 20+ 轮 | 定位到 `content.opf`，没锁定字段 |
| ⑤ §03bc | 反编译 xochitl | **根因坐实**：硬编码查 manifest `id="ncx"`；只改这一行，94 条标题全出 |

**方法论**：① 先造可量化判据；② 二分覆盖"名字/格式/顺序/内容"所有维度（20+ 轮只测了内容、没测 id 命名）；③ 二分穷尽而黑盒不透明时，反编译比继续猜划算。

### 03aw｜真机反馈第二轮：脚注撤回复核+异步优化真机全链路验证+防双击（2026-09-18，真机通）

- **脚注**：`Anchor` 版真机重跑，marker 可点、注释回章末。顺手发现 Anchor 分支对自带 `<p>` 的注释源产出嵌套 `<p><p>`——**09-23 已修**（容器改 `<div id>`，§03bs）。
- **异步化**：忙锁进程内、不落盘，重启清零（现为 `OpRegistry`）；`spawn_optimize` 先同步校验再加锁起后台线程，`catch_unwind` 保证锁一定清；结果写边车 + SSE；`GET /staging` 每条带 `busy`。真机：优化期间删除被拒、约 2 分钟后 `status:ok`。
- **防双击**：十几处 `onclick=async` 统一 `guardClick(el,fn)`。
- **教训**：忙判断必须在服务端；前端禁用只挡同一标签页的同一按钮。

### 03ay｜第四轮真机反馈：落库改异步（补齐"投书按钮"防双击）、真书《疯探》坐实并修复"目录页被优化器吃掉"的老 bug（2026-09-18，真机通）

- **落库异步**：`spawn_deliver()` 照抄 `spawn_optimize`，渲染自检挪进后台线程；忙时再发 deliver/delete 回"正在处理中"。
- **《疯探》目录页消失**：`remove_toc_from_spine`（最早期启发式、从无单测）把书内目录页摘出 spine——违背"保留目录页"原则，整段删除，`OPTIMIZE_VERSION` 10→11；回归测试 `html_toc_page_kept_in_spine_not_stripped_as_redundant`。
- **教训**：这一轮修了真 bug，但**没解决用户要的原生目录入口**（见排查链）。

### 03az｜第五轮真机反馈：《疯探》"没有 TOC"根因是 dtb:uid 不匹配……（2026-09-19，**标题结论已被 §03bc 证伪**，已并入上面「排查链总览」）

留下的修复：`fix_ncx_uid` 让 `dtb:uid` 与 OPF 标识符一致（规范要求）；自家 `build_ncx` 的 uid 也改接真实标识符；《雪人》扁平目录拆两级由 `restructure_existing_toc_parts` 处理。**教训**：对照两本书只挑最显眼的差异就下结论——《雪人》与《疯探》其实有三处差异（`id="ncx"`、无 DOCTYPE、uid）。

### 03bb｜《疯探》目录入口深度排查：二分定位到 content.opf，未锁定字段（2026-09-19，已并入上面「排查链总览」，终结于 §03bc）

留下的工具：逆向出 `.epubindex`（魔数头 `rM epub index` 的私有格式）当判据——标题位全空＝入口会消失。**教训**：20+ 轮二分只变了 OPF 的内容字段、没变 id 命名；"能用的样本"会掩盖隐含约定。

### 03bc｜《疯探》目录入口真正根因：反编译 xochitl 二进制坐实——硬编码死查 manifest `id="ncx"`，不走 `<spine toc="IDREF">`（2026-09-19，真机通，✅已解决）

![xochitl 如何查找 NCX，以及 wash_entries 的对应修复](diagrams/xochitl-ncx-lookup.svg)

- **工具链（可复用）**：Ghidra 12.1 + PyGhidra Python API（不依赖 `jep`）离线全量分析 23.6MB 的 xochitl；`strings` 必须**含 UTF-16LE**（Qt `QStringLiteral`）。
- **根因**：找 NCX 的代码用硬编码 `"ncx"` 查 manifest id 表；《疯探》合规的 `id="toc"` + `<spine toc="toc">` 查不到，标题提取整体失败，原生目录入口消失。
- **验证**：只改一行 `id="toc"`→`id="ncx"`，`.epubindex` 7188→15558 字节、94 条标题全对；`fix_ncx_manifest_id` 进 `wash`，端到端逐字节一致。同源活 bug：自动目录自己插的条目曾写 `id="cj-ncx"`，已改。
- `wash_entries` 现行目录修复顺序：`fix_ncx_manifest_id` → `restructure_existing_toc_parts` → `auto_toc` → `fix_ncx_uid` → `strip_ncx_doctype`。

### 03bg｜优化也补真实分步进度（不是漫画专属），顺带确认这不能"实质省内存"（2026-09-19，真机通，✅已解决）

- **前提纠正**：所有 EPUB 都走流式（不是只有漫画）；缺的是 `OptimizeCheck.progress`。
- **实现**：`StepProgress{done,total}` 共用；流式阶段二逐条目回调，每 5 条节流落盘/推 SSE。真机《飘·上册》`36/56 → 41/56 → 46/56 → ok`。
- **不能省内存**（置信度高）：进度回调不改数据存活时长；瓶颈是图片，已按张处理。

### 03bo｜封面声明规则与无损补封面工具（2026-09-20，真机对照实验）

**实验**（同一张封面图造 3 个最小 EPUB，只改声明写法）：封面 id 带点 + 只有 `<meta name="cover">` → 取不到；再加 `properties="cover-image"` → 正常；id 简单 + 只有 meta → 正常。
**规则**（`wash::ensure_cover_declared`）：两种声明同时写；必须在清洗**之前**调用；封面文件不是图片时兜底取前 12 个 spine 页里第一张真实图。
**工具**：`cover-fix in.epub out.epub [cover.png]`——只改 OPF、图片零重编码，已对设备上 3 本已投的书原地修补。

### 03bs｜2026-09-23 EPUB 优化线一轮真机修复：无扩展名章节、注释样式、质量门接入 book-serve

〔2026-10-07：设备端优化删除（§03bw），本节压缩成结论，完整记录见 git 历史。〕

用户用《甲午：摇摆的战争》反馈"注释有问题、无法改字体"。根因与修法：书里 4 章是**没有扩展名**的章节文件，清洗按扩展名判断整章跳过 → 扩展名优先、无扩展名时嗅探内容（真机识别章数 8 → 29）；多看脚注图标的 class 标在外层 `<a>` 上 → 两处都判、图标换上标数字；注释容器去加粗、字号小一档；Anchor 分支包 `<p id>` 产生嵌套 → 容器改 `<div id>`。同日把质量门接进设备端「优化」：产物写到临时文件后先过 `check_epub_file`，不过门原文件完全不动。用户确认"注释正常了"。
**教训**：质量门不在真实流程里跑就只是摆设。

### 03bv｜2026-10-07 优化引擎换成 sheng-ren 的 bookconv（**未部署，同日被 §03bw 取代，已并入 §03bw**）

当天上午做过一版"删本仓库 `bookconv`、改用 sheng-ren 的 `bookconv` 继续在设备上优化"，书架独有的部分搬进新 crate `shelf/crates/shelf-conv`。没上过真机，做到一半就发现"两边各留一份优化逻辑"本身是负担，下午改成彻底不优化（§03bw）；留下来的只有 `shelf-conv` 这个 crate。教训并入 §03bw「为什么这么定」。

## 第 A 章：入库 PDF 转换（历史）

### 03br｜入库 PDF 的「优化」：有文字层按原格式转 EPUB / 无文字层只裁边（2026-09-19 起，2026-09-23 重做；**2026-10-07 删除**）

〔2026-10-07：随书架不再优化书整个删除（§03bw）：`pdf_ingest`、本地 fork `pdf-extract-cj`、pdfwrite / pdfimg / pdf_epub、原 PDF 7 天备份与恢复接口都没了；第三方 PDF 现在原样加入 xochitl。本节压缩成历史，完整记录与代码在 git 历史（`git log -- shelf/crates/bookconv` 找删除前的版本）。〕

**当时的做法**：有文字层（每页平均可提取字符 ≥ 40）→ 按原格式转 EPUB（2026-09-23 用户拍板"按原格式（颜色，连接，图片位置）生成"）；漫画 / 扫描件 → 只裁白边、仍是 PDF；判不准一律只裁边。转成功后原 PDF 挪进 `staging/.pdf-originals/` 留 7 天、网页可恢复；母版库已有同名 EPUB 时不转。关键取舍：fork `pdf-extract` 只补颜色算子、图片位置、CID 字宽，不碰字形解码；颜色只用外链 class（xochitl 不认内联 `style=`）；图片按**视觉坐标**落位（Word 导出的 PDF 先画字后画图）；有跨章跳转就合成单文件（xochitl 只认同文件锚点）；每页只进一章。

**真机排查链**（Word 导出的《移动互联软件安装使用手册》8 页、calibre 导出的《2026-09-19 T.E.双语》540 页；当时两本都在真机上修好）：

![2026-09-23 两本真实 PDF 转 EPUB 的真机排查链](diagrams/sh-pdf-debug-chain.svg)

**仍可迁移的教训**：① 核对跳转要分两套——目录看 `.epubindex`，正文链接看预览 PDF 的具名目标表；只看目录会误判"能跳"（这类 xochitl 规则现在记在 sheng-ren `docs/xochitl.md`）。② 先用真书量化（字符数、图片引用数、链接数）再改代码。③ 破坏性的格式转换默认要留回头路——"转成功才删原件"不够，转成功不等于转得让人满意。

## 第 C 章：KOReader 与漫画旧通道（历史）

### 03d｜Phase 3 KOReader 配置即代码（2026-09-03；2026-09-29 随 KOReader 卸载退役，源码 09-30 删除）

**当时做了什么**：koreader-serve 用 KOReader 自带 `luajit` 跑 `merge.lua`，把仓库里的补丁合并进 KOReader 配置（写前备份、合并后 `dofile` 回读、失败自动还原；KOReader 在跑时拒写，因为它退出时会回写配置）。**为什么没了**：09-29 用户从设备卸载 KOReader（附录 B）。**仍可迁移的教训**：改别的程序的配置文件要"备份 → 合并 → 用它自己的解析器回读校验 → 失败还原"四步；同一秒内多次备份要加后缀，别让第二份盖掉真正的原件。

### 03t｜漫画通道：AZW3 漫画 → CBZ + 固定版式 PDF（2026-09-05，用户"AZW3 漫画该怎么转 / 镖人为何投不了原生"）

- **结论**：《镖人》投不进原生是撞 `/upload` 体积上限；漫画判据"图/字比"（图 ≥20 且每图配字 <40）后移植成现役 `comic_detect`。
- **被取代**：CBZ 通道随 host 砍除，漫画现在是 EPUB，超限走占位通道。
- **教训**：1-bit 抖动不省体积（297→223MB，位图高熵压不动；§03ad 的《镖人》283→687MB 同理）——降灰阶省的是刷新闪烁，不是体积；文档脚本的切片锚点必须先确认只出现一次（曾切掉一整段，已恢复）。

### 03ad｜漫画跨页拆分 + 白边裁切放大 + 小体积漫画可选投原生（2026-09-08，host 管线已砍）

判据留档：跨页识别＝宽高比 >1.05 且在 35%–65% 宽度找到内容最稀的竖直带当装订缝（《火影忍者》卷 1 96 页拆成 192 页）；白边裁切单轴超 20% 放弃；灰阶 CBZ ×1.5 估 PDF 体积。当时现役只有 `trim_margins`（白边裁切）与 `comic_split`（超限拆卷，2026-09-30 已移除），**跨页拆分在 Rust 里没有对应实现**。体积反涨的教训见 §03t。

### 03ar｜koreader-serve 高亮/生词只读端点与手写 SQLite 解析器（2026-09-16；2026-09-29 随 KOReader 卸载退役）

- **当时**：`GET /annotations` 解析 KOReader 的 `.sdr` 标注，`GET /vocabulary` 读生词本；笔记线 `ink-serve` 从这里导入。09-29 入口删除，09-30 源码删除（见 git 历史）。
- **仍要留的一处**：笔记线 `ingest.rs` 里对 `koreader:` 条目的早退判断**不能删**——条目库里还有旧 KOReader 书，去掉早退，启动追平会把它们当成"书已删除"整批撤销。
- **可迁移的教训**：`rusqlite` 交叉编译到 musl 链接失败（`sqlite3.c` 调 glibc LFS64 符号）时，当时手写了零 C 依赖的只读 SQLite 解析器 `sqlite_min.rs`，并把 `rusqlite` 降为仅测试依赖做差分对拍；以后再遇到 musl 下 C 库链接失败，这套"手写只读 + 测试里用正版对拍"仍可参考。另：生词本路径想当然写 `data/`，真机在 `settings/`——路径先在设备上核。

### 03bt｜KOReader 文字书 / 漫画两套阅读方案（2026-09-24 真机通；2026-09-29 KOReader 卸载，本节只留教训）

**当时**：用户给了两份网上流传的 `settings.reader.lua` 模板，要求生成"文字书 / 漫画"两套方案。最终用 KOReader 自带的三层机制拼出来（全局设置＝文字书、按文件夹的默认设置＝漫画、配置档按路径切状态栏），用户确认两套都生效。示意图随本节压缩一并删除（见 git 历史）。

**最重要的教训：先核键名，再动手**。把设备上 KOReader 的 Lua 源码拉回本机逐个核对，两份模板里的 `status_bar`、`cre_engine_controls`、`eink_refresh_every`、`default_profile` 等**在 KOReader 里根本不存在**，写进去会被静默忽略。网上的模板看着专业，键名却可能是编的；源码就在设备上，`tar` 拉回来 grep 比凭印象可靠。

### 03bk｜漫画 EPUB 优化改产出带书签 PDF，根治左右留白硬限制（2026-09-19，真机通，✅已解决）

- **结论**：PDF 直传左右留白可到 0%（EPUB 有 xochitl 固定内边距）。
- **被取代（09-20）**：用户拍板"优化不改格式"——转 PDF 会丢漫画夹带的文字页；后来另找到阅读器内 qmd 代理调 `setMargins(1)` 的路（当时是实验室开关 `comicMinMargin`，10-07 删除开关、改为带标记一律设，现状见传书线架构 §3）。`comic_pdf` 当时只服务超限 PDF 分卷；分卷 09-30 移除，`comic_pdf.rs` 同日由第五轮审计整个删除。
- **教训**：四轮内存排查数据见已删的 bookconv 白皮书 §16（git 历史；optimize 595MB → 206MB、split 峰值 67.8MB）。

## 第 E 章：第五轮全系统审计书架部分（2026-09-30）

### 03bu｜第五轮全系统审计：书架线部分（2026-09-30；已部署，功能待手测）

> **本节是第五轮审计书架线改动的权威清单**。〔2026-10-07：下表里优化、PDF 转换、远程图、抓网文相关的修法随设备端优化一起删除（§03bw）；仍在用的是 `write_atomic` 临时名、`ScratchFile`、文件戳缓存、Basic 认证缓存、`pdfmeta` 与边车短名。〕
> **状态**：审计本体 09-30 14:10 随 `deploy.sh` 部署（连同之前已合进 master 的 v16 规则移植、漫画 q95、撤按书方向、撤按卷拆分）；文末「审计后补修」两项 15:23 随 `install-all.sh` 部署。部署自检都通过（见「现状总览 → 部署状态」），**功能本身还没逐项手测**，要抽查的项目见 §05 #19。
> 当时开发机测试：`shelf` 521 个通过（含随后删除的 koreader-serve 26 个）、`rmsvc-core` 108 个通过，网关与网页测试、浏览器冒烟也过了，交叉编译零警告。
> `OPTIMIZE_VERSION` 仍是 16：正常书的产物字节不变，受影响的只有下表列出的几类特殊书。规则层的变化当时写在《EPUB 优化规范白皮书》4.5、4.7、4.8、5.3、5.6、5.7（10-07 删除，见 git 历史）；数据流与接口见传书线架构。

**会让服务整个倒掉或丢内容的（高）**

| 问题 | 修法 | 在哪 |
|---|---|---|
| 损坏 PDF 的 `/Parent` 指回自己或绕成环 → 无限递归栈溢出，book-serve 整进程崩（`catch_unwind` 接不住） | 沿 `/Parent` 找继承属性改成最多 64 层的循环（`MAX_PARENT_DEPTH`） | `bookconv::pdf_ingest::classify::get_inherited_media_box`、`pdf-extract-cj` 的 `get_inherited` |
| PDF 解压炸弹 | 嵌套流合计 64MB 封顶（此前只限页内容）；PDF→EPUB 的图片 Flate 流按声明的宽 × 高 × 3 封顶 | `pdf-extract-cj` `get_contents`；`pdf_ingest` |
| `opf:` 前缀写法的 OPF，分页拆出的第 2 份起进不了 spine（后半章在阅读器里消失） | 登记时认前缀（`opf::is_local`） | `paginate::register_in_opf` |
| 中文 80 来个字的书名：原子写临时名 = 目标名 + 后缀，超过 255 字节报 `File name too long`（入库、抓网文都会撞） | 临时名里的目标名截到 200 字节（按字符边界） | `rmsvc_core::fs::write_atomic`（`TMP_BASE_MAX`） |
| 优化临时文件按书名起名：长书名优化失败；panic 后半成品和补封面副本（可达数百 MB）一直留到下次重启 | 改用 `ScratchFile`：名字 `.<pid>.<序号>.<种类>.tmp`（种类 optimizing / cover / landing），Drop 时自动删 | `book-serve` `staging/mod.rs` |
| 大文件通道读第三方 PDF 页数：对象 2 恰好是 `/Outlines` 时把书签数当页数；对象跨度没有上限 | 页数只认 `/Type /Pages` 字典，字典对象最多读 4MB；**同日审计后补修**：改走通用有界解析，交叉引用流 / 对象流 / 页树根不在对象 2 的第三方 PDF 也能整本加入（见本节末「审计后补修」） | `bookconv::convert::pdfwrite` → `pdfmeta` |
| `pdfwrite::jpeg_to_image` 读 SOF 段差一越界 panic | 修边界 | `pdfwrite` |

**中、低**

- 启动恢复 `recover_interrupted` 此前漏清 `.cover.tmp` 与边车原子写残留（`.<书名>.delivered.<pid>.<序号>.tmp`）→ 现在母版库目录下所有"点前缀 + `.tmp` 结尾"的普通文件都清。
- 有 `<body>` 没 `</body>` 的截断 XHTML 不再被当空页删；PDF→EPUB 单通道 ICC 图不再被当 RGB 丢图、`[Flate DCT]` 滤镜链不再被当裸 JPEG 出坏图；分类不再把 `\0` 算文字；抽字时第三方代码 panic 改为退到"无文字层"（只裁边），不再让整次优化失败。
- 远程图（含抓网文）改用 `downscale_for_epub` 按插图竖框 842×1455 缩，超过 20MB 的整张不要（此前按整屏框缩、网文图缩两遍）；`jpegopt` 哈夫曼表号 >3 越界、采样因子 0 除零。
- `opf:` 前缀另外三处（`ncx_fix` 的 spine toc、`opfmeta` 的 `unique-identifier`、`preserve_relink` 认大写 `</BODY>`）；未接入的 palm/kf8 转换器 panic；`epub_optimize` 小工具判断"输入输出是同一文件"改用 `canonicalize`。
- 落库拒收文案去掉「请加入 KOReader」。

**耗电与效率**

- `GET /staging` 列表加文件戳缓存：`rmsvc_core::cache::{FileStamp, StampCache}`，戳 = 长度 + mtime + inode（同一时钟节拍里等长改写只看 mtime 会误判没变）；book-serve 里原先各写一份的三处（大小, mtime）缓存收编进来（优化等级 / 落库边车、阅读方向判 zip）。
- 网关 Basic 认证的密码校验结果缓存 `rmsvc_core::auth::VerifyCache`：10 分钟、最多 8 条、**只缓存通过的**（猜错照样现算 60 万轮 PBKDF2，暴力破解成本不变），键 = SHA-256(进程随机密钥 ‖ 存储哈希 ‖ 密码)，改密码清空。此前带 Basic 头的脚本每个请求都要算一遍几百毫秒的 PBKDF2。
- 网关闸门等忙完后的全量 `GET /staging` 至少隔 5 秒；撤掉 koreader-serve 事件订阅线程（此前它不在时每 5 分钟白醒一次）；母版库全量刷新 6→4 个请求；新建 xochitl 文件夹后不再每 2 秒轮询 12 次。
- bookconv：mobi 正则只编译一次；注释图标换数字改为整章分词一次（此前 O(标号数 × 章长)）；`png_to_image` 少拷一份。

**删掉的（死代码与 KOReader 残留）**

- bookconv：`comic_pdf.rs` 整个模块（`optimize_comic_epub_to_pdf_streaming` 等，漫画 EPUB→PDF 转换器彻底没了）、`comic_detect` 三个无调用函数、`pdfwrite` 内存版 `page_count` 等、`PageDirection::as_str`；`epubzip::find_cover` 与 `convert::common::char_floor` 去重。
- book-serve：host CLI 专用的 `SourceRef` / `set_source` / 入库参数 `?srcName=&srcBytes=`；`DeliverCheck.progress`；边车写入合并到 `update_sidecar`；`optimizable` / `deliverable`；`agent_wait`。
- 网关与网页：`MODULES` 的 koreader 项（`/api/koreader/*` 不再代理）、`/api/foundation` 的 appload/koreader/weread 探测、批量动作 `koreader`、闸门 `KoreaderAdopt`、词典格式；网页「其他」的 KOReader 子标签、母版库 KOReader 位置与「已加入KO」徽章、笔记页「KOReader 回流」、引导页两读器对比、基石列表的 appload/KOReader/WeRead，删 44 个语言键。
- **行为变化**：「已完成」只认加入过 xochitl，只加入过 KOReader 的旧书会回到「待处理」；旧 `batch.json` 残留的 koreader 任务读回时只剔除这几项。
- 当时保留：`koreader-serve` 源码；ink-serve 的 `/koreader/import` 与 `koreader.rs`（见 §03ar）。**同日稍后这些也从仓库删除**（见附录 A 演进记录表“09-30 末”一行），旧数据兼容部分（条目来源枚举、`koreader:` 早退、边车 `koreader` 字段）保留。〔2026-10-07 代码审查（§03by，未部署）：边车 `koreader` 字段也从结构体删了，旧边车里的当未知字段忽略。〕

**安装与卸载**：`install.sh --only` 也刷新设备上的 `shelf-uninstall` 与 `~/.local/lib/shelf/{manifest.sh,devlib.sh}`（此前只有整包安装才刷，`--only` 部署新 qmd 后卸载会拿旧清单漏删）；卸载时 `[ -d ] && rmdir` 在 `set -e` 下遇到非空目录会让整段卸载退出，已改；每次安装清掉旧设备上的 `koreader-serve` 单元与二进制（`manifest.sh` 的 `SHELF_LEGACY_*`）。全套脚本改动见 `packaging/README.md`。

**记录下来、这轮没改的**：PDF 裁边整本输出仍攒在内存里；分类/裁边逐字收集位置；CMYK JPEG 裁边报错；hayro 2 倍渲染没有像素上限；`read_skeleton` 对非图片条目没有总量上限；`upload_large_file` 等占位用 200ms 轮询（最多 20 秒）；领域错误一律回 400；`fswatch` 出错每 5 秒重试。（原列在这里的"带交叉引用流的现代 PDF 走不了大文件通道""边车名比书名长 11 字节"两项同日已补修，见下。）

**审计后补修（2026-09-30，15:23 已部署，待手测）**

- **超过 90MB 的第三方 PDF 能整本加入了**：大文件通道要把真页数写进 `.content`，此前读页数只认本项目自己写的结构（传统 xref 表 + 对象 2 是 `/Pages`），第三方 PDF 常用的交叉引用流 / 对象流（PDF 1.5+）、页树根不在对象 2 都整本拒收。新模块 `bookconv::convert::pdfmeta` 按规范从 `startxref` 沿 `/Prev` 链找到 `/Root` → `/Pages` → `/Count`，只读用到的几个对象、只解压用到的那个流；内存硬上限（字典 4MB、流 16MB 压缩 / 32MB 解压、`/Prev` 64 节、页数 20 万，最坏约 48MB），坏文件一律报错不 panic。lopdf 的 `load_metadata` 也是整份读进内存，没复用。详见传书线架构 §6.1。
- **书名很长也能看到处理状态了**：边车 `.<书名>.delivered` 超过 255 字节时改用 `.<书名截到 200 字节>.<sha256 前 16 位>.delivered`；普通书名的边车名逐字节不变（设备上已有记录照常认）。读写、改名、删除、孤儿清理、启动修复都走同一个 `sidecar::file_name_for`；孤儿清理按"目录里现存书 → 边车名"的反查表认主，不误删。详见传书线架构 §2.2。
