# reMarkable 笔记线（notes）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。读现状先看 §00b；待办/已闭环/明确不做看 §05；踩坑看 §04。总计划 未入库的计划文件。
> 笔记线**挂在书架网关下**：网关 / 注册表 / 事件汇聚 / 部署链 / 原生回收站代理都是书架的（`shelf/docs/reMarkable书架白皮书.md`），本文只记笔记线自己的决策、服务、真机轮次。

## 00｜定位与原则（2026-09-06 用户定）

书架（`shelf/`）补 reMarkable 读书短板；笔记线做强它的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。文学书勾作者名写"查作者"，学术书勾公式写"?/没听懂"或"!/背诵"——凡是勾了、写了的都该汇入笔记。旧 `knowledge/pkm`（画星待办、六色槽卡片、跨书汇编、复盘队列、仪表）判"太花哨"（颜色即路由的产物），**全部退役；新建 `notes/`，不引入任何旧代码，只借鉴功能与踩坑**（`.rm` 解析、`.epubindex` 页→章按书架惯例"剥离移植"成新 crate）。

四步闭环（原始设想，③ 后来两次改版——二期改按条目单发问 AI，三期直接砍掉分区，见 §00b/§03s）：① 合上书自动摄取（事件驱动）→ ② **手机上修正**（e-ink 打字太痛苦；不再引入中文化/输入法）→ ③ ~~按分区调智能（分区 = 名字 + 简述 + 是否调模型，简述就是给 AI 的要求；"背诵"不调）~~ → ④ 可选导出 md，一章一文件，与设备笔记本同构，反链。

工程原则同书架四条（XDG · 设计模式去重解耦 · 专项专用可插拔 · 不引旧 crate 不对接旧路径），外加笔记线自己的一句话：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影**。用户另定：四个服务（转写与智能分开）；设备笔记本一章一本、放《书名》文件夹、只读；~~分区自定义~~（三期已砍，见 §03s）；首期只 EPUB；**后续全部提交 `dev` 分支**。

## 00b｜现状总览（2026-09-07 刷新，读其余历史节前先看这里）

**架构**：书架网关 `manage::MODULES` 加四行（`ink`/`transcribe`/`mind`/`notes`）；四个 loopback 服务全部上机——**ink-serve 矿 8795**（书库监听 → 条目库唯一写者，零网络）· **transcribe-serve 转写 8796**（订阅矿的事件 → 裁图喂视觉模型 → 草稿写回，出网）· **mind-serve 脑 8797**（按条目单发问 AI，**没有事件订阅/批量循环**，纯被动等 HTTP，出网，§03p）· **note-serve 本 8798**（注册「笔记」tab；写入/打包/上传三件套 + 条目→文档生成编排**全部真机验证通过**，含旧版本自动软删闭环，见 §03h/§03i/§03j/§03k）。网页只多一个「笔记」tab（前端组合 `/api/ink` `/api/transcribe` `/api/mind` `/api/notes`），事件区域 `notes`（矿发 `entries`，转写发 `transcribe`）经网关 `Hub` 汇聚到 `/api/events`，页面零轮询。**笔记线零 xovi 依赖**（无 qmd、无 .so；将来建《书名》文件夹走书架的 Sidebar 代理 qmd，依赖仍留在书架那一份）。

![notes 架构：条目库唯一写者 = ink-serve](diagrams/architecture.svg)

**数据流（二期，含浏览态）**：合上书 → xochitl 重写 `<uuid>.content/.metadata` → ink `fswatch`（书库目录非递归、4 s 防抖）→ 只扫页 `.rm` mtime 变了的页 → `rmv6` 解析（勾画 GlyphRange + 手写笔画，同一坐标系，墓碑剔除）→ `notecore` 并查集聚簇 + 就近配对（含未配对的纯勾画） + 增量合并 → 落 `Mined` 态、自渲染裁片（`render_ink`，有手写才有） → `~/.local/state/notes/books/<uuid>.json` + `~/.local/share/notes/crops/` → 事件 → 网页「浏览」子视图 → 用户点「转入笔记」（`Mined→Pending`，纯勾画条目直接落 `Reviewed`）或「不需要」（`Mined→Skipped`）→ 转 `Pending` 的才会被 transcribe 防抖 3 s 捡到 → DashScope 视觉模型（OpenAI 兼容口，改配置即换厂）→ `POST ink /books/{uuid}/entries/{id}` 写 `draft` → 手机网页「整理」改字/分区/样式（改即存）+ 勾「问AI」填问题点提问 → mind-serve 拼书名+章节+原文+文本+问题发文字模型 → 写回 `answer` → 〔待建〕note 投影为《书名》文件夹一章一本 + md 导出。

![notes 数据流：四步闭环](diagrams/data-flow.svg)

**真机现状（2026-09-08 汇总，历史逐轮记录见 §03c–§03q）**：设备 imx93-chiappa，WiFi 直连（mDNS `shelf.local`，或直查 `getent hosts`；IP 是 DHCP 分配的别死记）。四个笔记线服务（`ink`/`transcribe`/`mind`/`notes`）全部 `active`，注册表 9 项（书架 5 + 笔记 4）。ink-serve 已摄取真实勾画+手写样本，聚簇/配对全部真机验证有效（§03f/§03g）；裁图**已换成自渲染**（`render_ink`，不再依赖缩略图，§03p）。transcribe-serve key 已配置、真调过模型，转写准确率还在打磨（§03g）。note-serve：写入/打包/上传三件套 + 全部 7 种打字样式两轮真机验证通过（§03h/§03i）；条目库→一章一本的生成编排**三轮真机验证通过**（首次生成、增量重传+旧本自动进回收站、无变化跳过，§03k）；《书名》文件夹自动创建**也真机验证通过**（`shelf-mkdir-agent.qmd` + `book-serve::mkdir`，§03l）——目前只剩网页「笔记」tab 还点不了「生成」（路由已开、前端未接，目前只能 curl/wget 手动触发）。**二期浏览态状态机 + 浏览页 UI + 纯勾画条目 + mind-serve 问 AI 全部真机验证通过**（步骤 1–4 收尾，§03n/§03o/§03p）；过程中修了两个真机 bug（书进回收站/删除仍赖在列表里；「浏览」对纯勾画条目排序时崩溃变空白）。**mind-serve 的模型手动改成了 `qwen3-vl-plus`**（这台设备的测试 key 对代码缺省的 `qwen-plus` 没权限，403；代码缺省没跟着改，见 §03p）。**二期步骤 5（模型配置统一页）已完成**（§03q）：`keyMasked` 脱敏预览 + 网关「模型」统一面板，拿到网关登录密码后**经真实 HTTPS 网关认证代理层**验证过 `GET`/`PUT /config` 的字段形状与前端读取逻辑完全对得上。二期五步全部完成。**这个面板后来在三期（§03t）整个搬进了「管理」tab、换成预置下拉 + 脱敏后只剩删除的 key 语义**——§03q 这段"视觉/文字并排两栏塞在笔记「整理」区"的布局描述已经过期，§03t 那版布局描述也过期了——预置表随后（§03u）从单一 DashScope 扩到四家厂商、key 按厂商分开存、加了按模型分账的用量/花费小表，当前设计以 §03u 为准。

**代码落点**：`crates/rmv6`（`lib.rs` 低层 `RmFile::read` / `page.rs` 高层 `Page{strokes,highlights,text}` + `BBox` / `write.rs` 写 `RootTextBlock` + 模板替换拼 `.rm` / `v6/crdt.rs` `CrdtId` 的 `Display`（`"part1:part2"`，条目库落盘 id 字符串的唯一定义处），§03h/§03p）· `crates/epubmap`（`index.rs` 两张表取首现 / `toc.rs` nav→ncx 两策略 / `lib.rs` `BookMap::chapter_of`）· `crates/notecore`（`model` 条目/样式/状态/去处（`Entry.ask_ai`/`question`/`destination`、`Quote.id`，分区三期已删见 §03s）· `hash` FNV 簇指纹与条目 id · `geom` 聚簇/配对 · `ingest` 增量合并（含纯勾画路径，§03p）· `marker` 行首标记 OCR 兜底 · `project` 条目库→段落列表投影+变更指纹（三期改按页平铺，§03s）· `export` 条目库→Markdown 导出，§03r）· `services/ink-serve`（`doc.rs` 书库只读视图 / `ingest.rs` 变更页编排 / `crop.rs` **自渲染裁图**（`render_ink`，从笔画矢量数据画折线，不再吃缩略图，§03p）/ `bookdb.rs` Repository / `config.rs` 阈值与几何 / `main.rs` 路由+监听，接 `askAi`/`question`/`destination` 字段 + `archive`/`purge` 动作，§03r）· `services/transcribe-serve`（`config` key 与节制参数 / `backend` `Vision` Strategy + `OpenAiCompat` / `prompt` / `ledger` 用量账本 / `ink` `EntryStore` 客户端 / `worker` 一轮编排 / `main.rs` SSE 订阅 + 防抖工作线程）· `services/mind-serve`（**新增**，§03p：`config` key/模型（无节流字段）/ `backend` `TextModel` Strategy + `OpenAiCompat`（纯文本消息） / `prompt` 拼书名+章节+原文+文本+问题 / `ledger` 用量账本（无 `lastRun`） / `ink` `EntryStore` 客户端（只有 `book`/`post_answer` 两个动作） / `worker::ask_entry` 单条问答 / `main.rs` **无后台线程**，纯被动路由）· `services/note-serve`（`rmdoc.rs` 打包 `.rmdoc` + 生产模板常量 / `config.rs` xochitl host/超时/文件夹命名 / `ink.rs` 只读 `EntryStore` 客户端 / `trash.rs` 跨服务调 book-serve 回收站队列 / `notebooks.rs` 每章生成记录簿记 / `publish.rs` `Uploader` Strategy + `generate_chapter/generate_book` 编排，§03j / `export.rs` 落盘 vault + 浏览器下载的 `content_disposition()`，§03r / `main.rs` 路由）· 网关 `ui/app.js` `renderNotes`（「浏览」/「整理」两个子视图 + 每条「问AI」勾选框/问题框/提问按钮/去处下拉/「不要了」按钮 + 章头「生成笔记本」「导出 md」+ 书头「清空回收站」，§03o/§03p/§03r/§03s）· `shelf/{build,deploy,install,uninstall}.sh` 的 `NOTES_BINS`/令牌（含 `mind`）· `shelf-gateway::manage::MODULES` 注册表。

**离线门槛**：`cargo test --workspace` **118 个**（rmv6 7 · epubmap 5 · notecore 43 · ink-serve 10 · transcribe-serve 19 · mind-serve 18 · note-serve 16）零警告；网关 `node --check app.js`；shell 过 shellcheck。

**未闭环**：transcribe 转写质量再打磨（重转复验：坐标已对、结构读对，但汉字数字"一/二/三"被认成阿拉伯数字"1/2/3"，1 条仍混印刷体，见 §03g；提示词已补一条规则，待真机复验）· `### ` 小节标记真机验证（`## ` 分区那半已随三期砍掉分区一起删除，不再是待办，见 §03s；`### 小节名` 覆盖 `subhead` 后端已接线、离线单测全绿，真机复验两轮都没成功——第一轮撞印刷体泄漏、第二轮排除了管线问题但手写行草连笔视觉模型读错，属于识别准确率而非代码问题，见 §03o/§03p）· 浏览页/模型配置面板/条目卡片新控件 UI 人眼确认（后端数据链路都真机走通，但纯前端可视渲染都没人眼确认过，见 §03o/§03q/§03r/§03t/§03u）· `notes pull`（host CLI 拉 md 到本机 Obsidian vault，见 §05——三期已做的是设备端落盘导出，host 侧拉取还没做）· archive/purge 两个端点没对真实历史数据实测过（不可逆操作，见 §03r；`restore` 是反方向的可逆操作，已在真实历史数据上验证过，见 §03u）· **模型预置横跨四厂商后只有 DashScope 真实调过**（OpenAI/Gemini/DeepSeek 三家新预置只验证了配置层——预置表匹配、key 按厂商隔离、老配置迁移，没有真实 key 走一遍实际转写/问答调用，见 §03u）。步骤 0 真机样本已于 2026-09-07 采回、验证、且真机复验通过（§03f/§03g/§03i）。

## 01｜架构决策

- **四个服务而不是一个 `note-serve`**（用户驳回单服务方案）：失败面分离——矿零网络（解析错只影响新条目）、转写/脑出网（断网只是积压）、本只碰输出物（xochitl `/upload`）；**转写与智能分开**是用户明确要求（转写是 OCR 边界问题，智能是提示词问题，节奏与费用都不同）。
- **不自建网关，挂书架现成的**：笔记线是独立仓库（`notes/`），但没有自己的 HTTPS/密码/证书/mDNS——那些是重复劳动（书架已经踩全了这些坑）；也没有塞进书架任何一个现成服务（`book-serve` 等），失败面与依赖方向都不同，硬塞会把两条线的故障域绑在一起。折中：复用书架的网关/注册表/事件汇聚/部署链这套**机制**（书架那边只加 `manage::MODULES` 几行映射，机制本身早就是通用的），但架构决策、服务划分、数据模型这些**笔记线自己的设计**完全在本文档，书架白皮书只记"它那边为了接住我们、改了什么"（见 `shelf/docs/reMarkable书架白皮书.md` §03ac）。代价：`shelf-gateway` 的 `MODULES` 成了跨两个仓库的单一事实源，笔记线加新服务除了自己这边的代码，还得去书架那个文件登记一行。
- **条目库唯一写者 = ink-serve**：转写/脑/本一律经它的 HTTP 改字段（`POST /books/{uuid}/entries/{id}`，缺省底座无 PATCH）。多进程各自读改写同一份 JSON 迟早互相覆盖（书架落库边车早期踩过同类）。
- **设备只写不改，改在手机**：xochitl 不认外部对已有文档的原地修改（书架/PKM 两线都判死），且 e-ink 上改字太痛苦。所以设备笔记本**只读**，由条目库投影生成；一章一本使重建局部化（只重建变过的章），旧本走书架回收站代理软删（`selectionMoveToTrash` 是唯一可靠路，直改 metadata 会被运行中 xochitl 覆写）。
- **事件驱动、零轮询**：ink 只在书库目录上挂非递归 inotify（合上书时 xochitl 重写 `.content/.metadata`，页 `.rm` 的写入不监听——文件多且是 xochitl 内部节奏），触发后按页 `.rm` mtime 只扫变更页；转写订阅矿的 `/events`；网页订阅网关 `/api/events`。
- **增量在数据层**（回答用户"二次识别会不会把改好的您好覆盖回你好"）：簇指纹 = 笔画 id 集合 + 点数 + 量化包围盒的 FNV-1a；指纹不变 → 不重转写不动 `text`；共享笔画但指纹变（补了几笔）→ 同一条目、新 `draft` 只作建议；笔画全没 → `Revoked` 留痕；条目 id 按 (书, 页, 最小笔画 id) 创建时一次算定永不重算。投影永远取 `text ?? draft`。
- ~~分区 = {名字, 简述, 是否调模型, 触发词}~~ **三期（2026-09-08）整个概念已砍掉**，见 §03s——AI 触发早就是 `Entry.ask_ai`/`question` 的事，笔记本排版分组也不要了，条目按页序平铺。这条历史结论原样留着，不再是当前设计。
- **样式判定：OCR 为主**：`-`/实心点/`口` 由 OCR 认（transcribe 侧 `notecore::marker`，只在条目仍为正文时认，并把标记从正文剥掉——笔记本样式自带编号/符号）；`1.` 数字形状不定同样走 OCR。~~下划线分区头由几何认~~ 分区没了，`has_underline` 三期已删（从没被真正接上过，见下方历史结论）。
- **裁图来源 = xochitl 现成缩略图**（384×512，3:4）：零渲染成本、与 de-risk 结论一致（工整 ≈100% / 快写 ~60–91%）；精度不够再换高分辨率自渲染，`crop.rs` 只暴露"给我这片的 PNG"可替换。页坐标 → 像素按 `page_width/height` 等比、`x_origin_center` 可配，**EPUB 页真机测得 960×1280、x 原点居中，见 §03g**（不是 1404×1872 物理屏——真机踩过、改过、验证过）。
- **视觉后端 Strategy**：`Vision` trait 一个方法；`OpenAiCompat` 走 `POST {baseUrl}/chat/completions` + `image_url` data URI，DashScope Qwen 缺省（国内直连、设备自己 WiFi 不经 host 代理——host clash fake-ip 会挡），任何 OpenAI 兼容口只改配置。编排 `worker::run_once` 全 trait 注入，内存桩单测。
- **rmv6 剥离移植而非依赖 device-core**：vendored `remarkable_lines` 0.1.3（MIT）只留 v6，保留两处兼容补丁（未知 PenColor/ParagraphStyle/Tool 码兜底、块尾多余字节跳过），补 CHECKBOX(6/7) 码；`PROVENANCE.md` 留痕。notes 不依赖 `bookconv`/`device-core`/`knowledge/pkm`/`reading`。
- **体积/内存**：musl 全静态 ink 2.5 MB · transcribe 2.1 MB · note 1.2 MB；单元 `MemoryMax=128M` `CPUWeight=20` `Nice=5`。

**手写约定 ↔ xochitl 3.28 七种打字样式**（格式菜单 qml_00db4610：Title / Subheading 1 / Subheading 2 / Body / Bulletpoint / NumberedList / CheckboxUnchecked；.rm 段落样式码 + 全部 7 种真机验证通过的写入能力，见 §03f/§03h/§03i）：

| 样式 | 码 | 笔记本用途 | 手写约定 | 判法 |
|---|---|---|---|---|
| Title | 2 (HEADING) | 页标题 = 章名 | `#`（**这条线不接**，见下方说明） | epubmap |
| Subheading 1 | 3 (BOLD) + 7 字节标记 `21023403000000` | ~~分区头（名 + 简述 = AI 要求）~~ 三期已砍掉分区，这一级目前没有任何生产者在用，见 §03s | — | — |
| Subheading 2 | 3 (BOLD)，不带标记 | 勾画所在小节 | `## 文字` / `### 文字`（两个/三个及以上 `#` 是同一件事，不分层级，覆盖 epubmap 自动填的值；三期先误删了 `##` 半又按用户要求恢复，见 §03t） | OCR（`notecore::marker`）／epubmap 缺省 |
| Body | 1 (PLAIN) | 转写正文 / AI 回答 | 普通书写 | OCR |
| Bulletpoint | 4 (BULLET) | 无序 | 行首短横 / 实心点 | OCR（`marker`：`- `/`• `/`· `/`* `/`—`/`－`） |
| NumberedList | 10，格式子块跟其余样式一样只有 2 字节 | 有序 | 行首 `1.` | OCR（`marker`） |
| Checkbox（未勾选） | 6 (CHECKBOX) | 待办 | 行首空心小方框 | OCR（`marker`：`口`/`□`/`☐`） |
| Checkbox（勾上号） | 7，只能原生点方框切换，写入器造不出 | — | — | — |

**2026-09-07 用户定案，更正了两处过期结论**：① 原计划"Subheading 1 靠几何 `has_underline`（一行字+下划线）判定"——查证 `has_underline` 函数确实存在且真机验证过（§03f），但**从没被 `ingest_doc`/`merge_page` 调用过，只在自己的单元测试里跑**，等于没接线；用户否掉了这条手势路线（"不好，没有学习成本"这个标准更重要），改用 Markdown 标题级别的文字标记：`## 文字`＝分区头、`### 文字`＝小节，复用已有的 OCR 兜底识别路径（`notecore::marker`），不需要额外的几何判定代码。② 表里"Bulletpoint/Checkbox 几何优先、OCR 兜底"这个说法核对代码后也是错的——`notecore::geom` 目前只有 `has_underline` 一个几何标记函数，无序/待办目前**只有 OCR 路径**（转写完成后才认，见下）。`#`（对应 Title）明确不接：Title 是整章级别的（一份生成的笔记本只有一个 Title，来自 epubmap 章名），条目级 `#` 没有现成字段可落——用户说这是留给以后"纯手写笔记扫描"（会议/上课，没有 EPUB 章节可依附）那条还没立项的线，不要为了勾书这条硬造用不上的字段。

**`## 文字`/`### 文字` 落到哪个字段（当前状态，§03t 定稿）**：两个/三个及以上 `#` 都直接覆盖 `Entry.subhead`（平时这个字段由 epubmap 从 EPUB 目录 h2/h3 自动填，手写标记可以人工盖过去），不分层级——`##` 那半原来是"找/建 `Section`"，三期删分区时被连带误删成完全不识别，用户当场纠正"这个手写标记本身要继续认"，改成跟 `###` 合并成同一件事（见 §03t）。

**`Section.triggers` 死代码**：随分区整体删除，连字段带这段悬而未决的历史都一并消失了，不再是待办，见 §03s。

## 02｜XDG 路径表（设备 HOME=/home/root，`Paths::app_{config,data,state}_dir("notes")`）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}`（随书架 `install.sh`，令牌 `ink`/`transcribe`/`mind`/`note`） |
| 配置 | `~/.config/notes/ink.json`（聚簇/配对阈值、页几何、防抖；首启写出缺省）· `~/.config/notes/transcribe.json`（**0600**；`preset`（预置 id，四厂商共表）+ `keys`（厂商→key）+ `prices`（预置→用户自填单价）+ `customModel`/`customBaseUrl`（"自定义"分支）+ `backend`/`timeoutSecs`/`maxPerRun`/`pauseMs`/`auto`/`maxAttempts`/`prompt`；老配置的 `apiKey`/`model`/`baseUrl` 三件套只作反序列化兼容字段，启动时 `migrate()` 搬进新形状后不再落盘，见 §03u）· `~/.config/notes/mind.json`（**0600**，同上但没有 maxPerRun/pauseMs/auto/maxAttempts 这些节流字段） |
| 数据 | `~/.local/share/notes/crops/`（手写裁片 PNG）· `~/.local/share/notes/vault/`（md 导出，§03r 已落地，设备端落盘这一半；host 拉取 `notes pull` 还没做） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**，一书一文件，原子写）· `~/.local/state/notes/transcribe.json`/`~/.local/state/notes/mind.json`（用量账本：次数/token/最近错误/上轮报告，不存内容） |
| 运行时 | 与书架共用注册表 `$XDG_RUNTIME_DIR/shelf/services/`（缺省回落 `/tmp/shelf-0/shelf/services`） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`（`<uuid>.{metadata,content,epub,epubindex}`、`<uuid>/<page>.rm`、`<uuid>.thumbnails/<page>.png`）——**绝不写** |

## 03｜systemd

四个单元 `notes/systemd/*.service`（`ink`/`transcribe`/`mind`/`note`），随书架载荷一起装：`PartOf=shelf.target` + `WantedBy=shelf.target`，`After=home.mount`（transcribe/mind 另 `Wants/After=network-online.target`，出网那两个才需要）；`Restart=on-failure`。**不给 xochitl 加任何依赖**（红线）。装/卸：`shelf/install.sh --only ink,transcribe,mind,note`、`shelf/uninstall.sh`（条目库不在 `--purge` 范围，绝不删用户笔记）。

## 03b｜地基三 crate（2026-09-06，离线）

- **rmv6**：`RmFile::read` 只认 `reMarkable .lines file, version=6`；高层 `Page::parse` 给 `strokes`（`SceneLineItem`，未删）/ `highlights`（`SceneGlyphItem` = GlyphRange：原文 + 页文本偏移 + 每行矩形）/ `text`（打字文本）。真机事实：**勾画与手写笔画同一坐标系**，几何配对不需换算（2026-09-07 真机样本坐实，见 §03f）；擦掉的项是墓碑，两种形态——独立 `SceneTombstoneItemBlock`，或 `SceneLineItem`/`SceneGlyphItem` 本身 `item.value` 为空（同块内软删，数量可能不小：真机样本 105 个 `SceneLineItem` 块里 17 个是后者）。fixture `testdata/renggu/page.rm`（《人骨拼圖》c65fa2ae 页，1049 B）23 笔全墓碑——只测"解析成功零条目"；`testdata/renggu_marks/`（同页，2026-09-07 重画）测真实勾画+手写；`testdata/seven_styles/`（笔记本一页七样式）测段落样式码。
- **epubmap**：`.epubindex` 两张表（每条 `u32 长度 + UTF-16BE 路径 + 3×u32`），第一张第一个 u32 = 起始页（0-based），第二张第三个 u32 也是（前两个是字符偏移/长度）；取每个 basename **首现**的第一个 u32；路径非 ASCII 不认（防误配）。目录 `nav.xhtml`（嵌套 `<ol>`）优先、`toc.ncx`（嵌套 `navPoint`）退回，标签事件流 + 深度栈解析（不带完整 XML 解析器，省体积）。`chapter_of(page)` = 1 级祖先为章、本条 ≥2 级为小节。`.content` `pages` 是页 id 顺序表，下标 = 页号，与起始页对齐（真机 523 项核过）。
- **notecore**：纯函数零 I/O。`geom::cluster` 并查集（任意两笔包围盒间距 ≤ `cluster_gap` 连通，不看笔序/时间——用户会回头补笔）、`pair` 每簇最近勾画（≤ `pair_gap`，否则本页批注）、`has_underline`（簇内某笔宽≈簇宽、矮、贴底 → 分区头候选，§03f 新增）；`is_handwriting` 排除荧光笔/橡皮/选区；缺省 `cluster_gap=40`/`pair_gap=120` 页坐标单位，**2026-09-07 真机样本验证有效、未改**（§03f）。`model::Style::wire_code` 给出 .rm 段落样式码（Body=1/Bullet=4/Numbered=10/Checkbox=6）。`ingest::merge_page` 实现 §01 增量四规则。`marker::split_leading_marker` OCR 兜底（`口渴了`/`-3 度`/`2024 年` 不误判）。

## 03c｜ink-serve 矿（2026-09-06，真机首轮）

- 路由（经网关 `/api/ink`）：`GET /books`（每本 `{uuid,title,chapters,entries,pending}`）· `GET /books/{uuid}`（整份条目库）· `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id}`（`text`/`section`/`style`/`draft`/`answer` 逐字段；给 `text` 即 `Reviewed`，清空回 `Draft`/`Pending`）· `PUT /books/{uuid}/sections` · `POST /books/{uuid}/rescan`（清页 mtime 记录整本重扫）· `GET /events`。
- 摄取：只处理活的 EPUB（`DocumentType` 且非回收站/非 deleted，`fileType=epub`）且有手写页的文档；启动追平一遍，之后 `watch_debounced`（`debounceSecs` 4）按事件文件名认 uuid 只扫涉及的文档；页 mtime 记在条目库 `page_mtimes`。裁图：页包围盒 + `cropMargin` 24 → 缩略图像素（`PageGeom::to_pixels`，夹在图内），文件名带簇指纹，指纹不变不重裁。
- 真机：`active`、注册；追平扫到《人骨拼圖》38 章；`ink.json` 首启写出缺省。当时唯一有 `.rm` 的页 23 笔全是墓碑、扫出 0 条——**真样本已于步骤 0 补上（§03f），现在能摄取出真实条目**。

## 03d｜网关「笔记」tab + note-serve 骨架（2026-09-06）

- 书架侧：`MODULES` 加行、`AREA['note-serve']='notes'`、`TABS['note-serve']`；`build.sh/deploy.sh` `NOTES_BINS`、`install.sh` `ALL` 令牌。note-serve `ServiceSpec.tab=("笔记",25)` 注册 tab；`GET /status`（vault 路径）· `GET /events`。
- 网页（`renderNotes`）：选书 → 按章分卡片 → 每条：左裁图（`/api/ink/.../crops/`）、右文本框（缺省填 `text ?? draft`，改即 `POST entries` 存）、分区下拉、样式下拉、状态徽章、勾画原文引用、智能回答折叠；分区编辑（名 / 简述 / 调模型 / 增删 / 保存）；「重扫」。真机：tab 出现、两服务 active（用户手机端目视待补）。

## 03e｜transcribe-serve 转写（2026-09-06 夜，真机 WiFi 部署）

**触发**：订阅 ink-serve `/events`（与网关 Hub 同套路：注册表找 → 长连接 → 断线 3 s 重连），`area=notes,kind=entries` 且 `auto=true` → 踢工作线程 → 防抖 3 s 合并 → 跑一轮；启动追平一次；`POST /run` 同步跑一轮（网页「转写」）；`POST /books/{uuid}/entries/{id}` 强制转写一条（网页每条「转写/重转」，忽略指纹与失败上限）；`POST /retry` 清失败记录再踢。自动与手动共用 `run_lock` 互斥。

**一轮**（`worker::run_once`）：`GET /books` 只看 `pending>0` 的书 → 条目 `needs_transcribe()`（`ink.hash` 没有对应草稿且非 Revoked）→ 取裁图 → 提示词（内置：只转写不发挥、保留换行与行首符号、认不出用「？」、无字输出空；**勾画原文截 300 字作语境**帮人名/术语，但明说别抄进来；`temperature=0`）→ 模型 → 样式仍为正文时 `marker` 兜底 → `POST ink entries` 写 `draft{text,backend,at,hash}`（+ `style`）→ 账本记 token。

**节制**（用户费用/限流关切）：每轮 `maxPerRun` 20、请求间歇 `pauseMs` 300；同一条同指纹失败 `maxAttempts` 3 次后不再自动试（指纹变了计数归零）；**一轮连续 3 次失败零成功即停**（key 错/断网别一条条撞 60 s 超时）；账本只记数不记内容。

**key**：`GET /config`/`/status` 只报 `hasKey`/`keySource`（config/env/none）**不回显**；`PUT /config` 的 `apiKey` 非空才改、`clearKey` 清；文件 `0600`；环境 `DASHSCOPE_API_KEY` 兜底。网页转写区：key 输入框（`type=password`，保存后清空）、模型/baseUrl、合书自动开关、跑一轮/重试失败、失败清单、用量与上轮报告。

**真机部署当轮**：`active`、注册表见 `transcribe-serve`；日志 `后端 qwen qwen3-vl-plus；key None`；`/status`：`hasKey=false keySource=none inkReachable=true pending=0`，`lastRun.note="未配置 API key（…）"`；`transcribe.json` `-rw-------`。~~**待验**：粘 key 后真调一次模型、事件链 ink→transcribe→网页刷新~~ **✅ 后续都验过了**：步骤 0 样本到手后 key 配置好、真调过模型（§03g 那轮翻车＋修复就是这次真调暴露出来的）。

**取舍**：为什么不直写条目库（§01 唯一写者）；为什么限量 + 即停（一次合书几十条，key 错时不该烧完超时）；为什么行首标记在转写侧兜底（`1.` 几何认不出，表里本就写 OCR 判；几何判出的不覆盖）；为什么勾画原文进提示词（旧 cardhw 经验：上下文救人名/术语，代价是 token 略增）。

## 03f｜步骤 0 真机样本标定（2026-09-07）

用户在设备上按 §05 步骤 0 清单采回两份样本，用 `rmscene` 0.8.0（host venv 已有，反解交叉验证）+ rmv6 自身单测双路核验，**全部并入 `cargo test --workspace` 常驻回归**（不是一次性脚本）：

- **样本一**（`testdata/renggu_marks/`）：《人骨拼圖》同一页（沿用 c65fa2ae，旧墓碑样本换成真内容）勾三段原文，每段旁写一行（首字分别 `-`/`1、`/`口`），另写一行字+ 下面一条长横。
- **样本二**（`testdata/seven_styles/`）：新建笔记本一页，格式菜单逐行打 Title/Subheading 1/Subheading 2/Body/Bulletpoint/NumberedList/Checkbox（含"Checkbox"与"Checkbox finished"两行）。

**坐实（聚簇/配对不用改代码缺省值；页坐标画布尺寸要改，见 §03g）**：
- **x 原点居中**：三条高亮矩形 `x` 全为负（落在左半页），自己拿笔画/矩形原始坐标重渲染一遍、跟真机截图核对布局完全吻合——**方向**判对了；`ink.json` 的 `xOriginCenter=true` 不用改。但当时以为"宽 1404"也一并验证了，其实只验了"x 为负=左半页"这个粗粒度方向，没有验精确画布尺寸——真实尺寸是 960×1280，不是 1404×1872，是后来跑真机转写发现草稿全错、倒查裁图坐标才挖出来的（§03g）。这条错误结论在本节最初版本里存在过，特此更正。
- **聚簇/配对阈值**：`cluster_gap=40`、`pair_gap=120` 拿真实笔画包围盒验证——3 处批注簇与对应勾画间距均为 **0.0**（紧贴甚至压线），到次近勾画都在 148pt 以上；第四簇（字+下划线）离最近勾画 305pt，正确判"本页批注"。阈值卡在两类间距中间，安全边界宽，**不用改**（`notecore::geom::real_device_sample_clusters_and_pairs_correctly`）。
- **荧光笔自留一条笔画**：`SceneGlyphItem`（语义高亮矩形）之外，画高亮的笔本身还留一条 `SceneLineItem`（工具 `Highlighterv2`）。`is_handwriting` 已按 `Tool::Highlighter` 排除，真机样本验证有效——**没有二次踩这个坑**。
- **`has_underline` 判据成立**：下划线是单独一笔，宽度≈簇全宽（本样本 487.7 实测）、高约簇高 1/6（18 vs 113）、贴簇底部；三条批注簇上该判据均为假，第四簇为真——几何足够稳，不必等 OCR。

**新发现（改了代码/文档）**：
- **NumberedList 真实码 = 10**（写进 `rmv6::v6::scene_item::text::ParagraphStyle::NUMBERED`，`notecore::model::Style::wire_code`）。~~⚠️ 它的格式子块比其余样式多 7 字节未解码载荷，note-serve 写入器落这码前必须再采多行样本~~ **✏️ 2026-09-07 更正（§03i）：这条是分析失误，那 7 字节其实属于 Subheading 1，NumberedList 格式子块跟其余样式一样只有 2 字节，`rmv6::write` 已支持且真机验证编号自动生成正确，不存在这个前提**。
- **Subheading 1 与 Subheading 2 共用同一个 .rm 码（3=BOLD）**——原计划设想的"7 个不同样式码各管一种"不成立。~~原生靠某种不在 styles 里的机制区分两级大小，大概率按段落在文档大纲里的层级动态决定，这块不打算深挖~~ **✏️ 2026-09-07 更正（§03i）：机制找到了，就在 styles 里——格式子块末尾多 7 字节 `21023403000000` = Subheading 1（大字号），不带 = Subheading 2（小字号），`rmv6::write` 的 `Paragraph::subheading1()` 已支持、真机验证过，不需要"靠缩进/前缀文字区分"这种退而求其次的办法**。
- **Checkbox 勾上号（7）：打字给不出，但确认真实存在**：格式菜单把两行都打成"Checkbox"样式（其中一行文字打的是"Checkbox finished"），.rm 里两行的码都是 6，说明**打字模式给不出 7**，勾上号是对已渲染方框的一次点击手势，不是段落样式菜单的选项。2026-09-07 用户在真机上点了一次方框，拉回来的文件里那一行码确实是 7（§03i）——码本身没问题，只是设备只读设计下用不上、写入器也造不出"直接生成一个已勾选的待办"。
- **`SceneLineItem` 的软删有两种写法**：既有独立的 `SceneTombstoneItemBlock`（旧样本 23 笔全是这个），也有 `SceneLineItem`/`SceneGlyphItem` 自身 `item.value` 为空（真机新样本 105 个 `SceneLineItem` 块里 17 个是这样）。两条路径 `Page::from_file` 已经都在处理（`if let Some(...) = &it.item.value`），只是这次才第一次在同一个真实文件里看到两种形态并存，记一笔防止以后只测了一种就以为够了。

**产物**：`testdata/renggu_marks/`（`page.rm`/`page.png`/`book.content`/`book.epubindex`/`toc.ncx`/`content.opf`）、`testdata/seven_styles/`（`page.rm`/`page.png`/`book.content`）；新增 4 个测试（rmv6 2 个真机样本解析 + notecore 2 个：聚簇配对回归 + `has_underline`），`cargo test --workspace` 从 31 涨到 35。

## 03g｜真机转写首轮翻车 → 挖出裁图画布尺寸错（2026-09-07）

样本落地当天顺手让 transcribe-serve 在真机上真跑了一轮（key 已配置，非本文档动作），4 条条目全部产出 `draft`。**核对内容发现全错**——没有一条是用户实际写的 `-第一段`/`1. 第二段`/`口 第三段`/`这是一段话并画线`，而是分别抄了页面别处的**印刷体原文**（如"如此強烈"≈epigraph 里的"如此強勢"、"：30P.M."≈时间范围行、"領取行李\n也已錯過"/"，但她只想著一件事：\n分，好想換上睡衣，倒"≈相邻印刷段落原句）。四条草稿没有一条随机——都是"沿页面往下大致对应位置"的印刷体，说明**裁图本身裁偏了**，不是模型瞎编。

**定位**：把 4 条 `entries[].ink.crop` 从设备现场拉回来肉眼看，确认裁图内容确实是印刷体/空白，不是手写——问题在 ink-serve 生成裁图这一步，不在转写侧。用真机缩略图（`highlight_page_thumb.png`）程序化扫像素（高亮的橙色底色 `r>230,170<g<230,100<b<190`）反推三条高亮真实落在缩略图的第几行第几列，跟 `crop.rs::PageGeom::to_pixels` 用配置里 `pageWidth=1404/pageHeight=1872` 算出来的框一比——**完全对不上**（真实行比算出来的行靠后一大截，且越往页面下方偏差越大，是线性缩放系数错，不是平移量错）。

**定量**：三条高亮真实像素行/列 vs. 页坐标做最小二乘线性拟合（`numpy.polyfit`），y 轴 `real_row = 0.4019×page_y − 0.36`、x 轴 `real_col = 0.3997×page_x + 191.4`——两轴缩放系数几乎相等（≈0.40，本该相等，验证了拟合没错）；反推等效画布 `page_h = 512/0.4019 ≈ 1274`、`page_w = 384/0.3997 ≈ 961`，x 偏移 `191.4/0.3997 ≈ 479`，跟 `page_w/2 ≈ 480` 吻合（**x 原点居中的方向判定本身没错**，见 §03f 更正）。1274×961 极接近整数 **1280×960**（3:4，跟缩略图同比例）——取整后代入验证，误差在真机缩略图 384px 分辨率的量化噪声内。

**根因**：EPUB 页的 `.rm` 坐标系是 xochitl **EPUB 排版引擎自己的虚拟画布**，不是设备物理屏像素——物理屏（笔记本/PDF 用的坐标系）才是 1404×1872，EPUB 重排文本另开一个更小的虚拟画布（960×1280，仍是 3:4）来排版、再整体缩放贴合物理屏显示。`ink.json` 抄了"经典 1404×1872"这个物理屏数字，从一开始就没对——§03f 当时验证"x 原点居中"时只看了正负号方向，没量精确尺寸，误以为验证完整了。

**影响面与修法**：
- **`notecore::geom`（聚簇 `cluster`/配对 `pair`/`has_underline`）完全不受影响**——这几步只在 `.rm` 原始坐标系内比较相对距离（间距、宽度占比），不需要换算成像素，§03f 的验证结论站得住。
- **`ink-serve::crop`（页坐标 → 缩略图像素）是唯一受影响的地方**——已改 `IngestConfig` 缺省 `page_width: 1404.0→960.0`、`page_height: 1872.0→1280.0`；新增回归测试 `crop::tests::real_page_geometry_lands_on_the_real_highlight`（拿高亮 1 的真实 rect 跑 `to_pixels`，断言落在真机测得的行 264–281／列 51–138 附近，用 1404×1872 会跑到行 179–192／列 96–156，测试会红）。
- **只验过 EPUB 页**：笔记本（typed text/手写自己的画布）没有独立测过，`page_width/page_height` 目前只对 ink-serve 唯一处理的 EPUB 页有意义；以后 note-serve 读/写笔记本坐标系时**不能想当然套用这组数**，得单独测。

**教训**：`.rm` 坐标"看起来像页面尺寸"不代表就是设备物理分辨率——EPUB/PDF/笔记本三种文档各自的排版画布可能不同，下结论前拿"同一份数据里已知语义的东西反推"（这次是拿高亮矩形该出现的印刷段落位置去比对缩略图真实像素）比"读一个数字直接信"可靠得多。这条经验后续采样（比如笔记本画布、PDF 定稿画布）要重复用。

**当场真机复验（同一天完成，不是留待办）**：改完配置又发现一个衍生问题——第 4 条批注（长句+下划线，无勾画配对）真实 y 落在 1507–1620，超出 960×1280 画布之外，说明**这条批注写在了这页可滚动内容里、缩略图视口截不到的地方**（缩略图只固定截这页物理一屏，页面比一屏长时下面部分不进缩略图）；`to_pixels` 硬夹到图边缘后裁出宽/高仅 1px 的废图，喂给 Qwen 被直接拒收（`InternalError.Algo.InvalidParameter: height:1 ... must be larger than 10`）。加了 `crop_png::MIN_CROP_PX`（8px）守卫：裁剪区域小于这个数直接判"裁不到"返回清楚的错误，不再真送废图去烧一次 API 调用（新增回归测试 `crop_png_rejects_offscreen_bbox`）。

重编译部署（`shelf/build.sh` + `deploy.sh --only ink,transcribe,note`）、手改设备上已落盘的 `ink.json`（`pageWidth`/`pageHeight` 1404/1872→960/1280，新装的二进制不会覆盖已存在的配置文件）、清掉 4 张旧裁图缓存（裁图按"指纹不变文件已存在就跳过"复用，光换配置不删缓存不会重裁）、`POST rescan` 触发重裁 + 强制重转三条有勾画的：

| 条目 | 真实手写（用户核对更正） | 修复前草稿（抄印刷体） | 修复后草稿 |
|---|---|---|---|
| 高亮 1 旁 | `-第一段` | "如此強烈"（≈epigraph"如此強勢"） | "：30P.M.主△ \n～第1段"（结构对："第□段"识别出来了；**但"一"被认成阿拉伯数字"1"**，上方还混进裁图边缘一点印刷体） |
| 高亮 2 旁 | `1. 第二段` | "：30P.M."（原样抄了页面别处的印刷体） | "只能叫計程\n**1. 第2段**\n手提雲膠的"（同上：结构对，**"二"被认成"2"**，上下各混进一点印刷体） |
| 高亮 3 旁 | `口 第三段` | "領取行李\n也已錯過"（抄了页面别处） | "常？穂\n？車 這些顏"（完全没读对，跟"口 第三段"毫无关系——分辨率/裁图边界还需再调，留作后续质量项） |
| 无勾画批注 | `这是一段话并画线` | "，但她只想著一件事：…"（抄了页面别处） | 裁图裁不到（视口外），`MIN_CROP_PX` 守卫生效跳过；这条**草稿字段仍是修复前的错误旧值**（drafts 只增不删，没有"清空"接口）——手机网页上看到会是这条旧值，用户手动一改（或留空）即可，不是数据损坏 |

**结论**：坐标根因坐实且真机验证有效——3 条有勾画的条目，裁图现在都对准了真实手写所在的位置（不再是印刷体或空白），这个层面的 bug 确认修复。但**转写准确率不能算"读对"**：2 条只对了"第□段"的结构，汉字数字"一/二"被模型认成阿拉伯数字"1/2"（连笔手写的"一"确实容易跟"1"混，"二"跟"2"形近，但这仍是错读，不是四舍五入的小瑕疵）；第 3 条（"口 第三段"）完全没读对，混进了裁图边距外的印刷体。这些都是**转写质量问题（分辨率/边距/OCR），跟本节修的坐标 bug 是两回事**，坐标修复不能反过来当成"转写现在准了"的证据。1 条无勾画批注遇到裁图覆盖范围的新边界情况，已优雅降级而非报废调用。

**产物**：`ink-serve` 缺省配置改字段 2 处、`crop.rs` 新增 `MIN_CROP_PX` 守卫、`crop.rs`/`config.rs`/`geom.rs`（notecore）文档注释同步更正、新增 2 个测试（`cargo test --workspace` 35→**37**）；真机重编译部署两轮、`ink.json` 手改、清缓存、重裁、强制重转三条，全部当场验证。**遗留待办**：转写质量两项——① 汉字数字"一/二/三"被认成"1/2/3"（entry1/2）；② 裁图边距混进相邻印刷行导致整条读错（entry3）。下一步该把裁图边距按行高动态收紧、或换自渲染高分辨率裁图（白皮书早留的口子），顺带看能不能在提示词里强调"数字用汉字原样抄、不要转阿拉伯数字"；entry4 的旧错误草稿留给手机网页人工清。

## 03h｜rmv6 补写能力：`RootTextBlock` 编码器（2026-09-07）

note-serve 投影要往设备写打字文本，rmv6 之前是纯只读解析。评估后判定**只值得自己写 `RootTextBlock` 这一个块**：`AuthorIdsBlock`/`MigrationInfoBlock`/`PageInfoBlock`/`SceneInfo`（真机样本自带一段没解出来的不透明字节）/`SceneTreeBlock`/`TreeNodeBlock`/`SceneGroupItemBlock` 这些和"页面上打了什么字"无关，从零精确重建风险高、收益低——改用**模板替换**：拿一份真机产出、已知能被 xochitl 正常打开的 `.rm` 文件当模板，扫它的顶层块序列（`u32 长度+u8 0+u8 min_version+u8 current_version+u8 block_type`，找 `block_type=7`）定位 `RootTextBlock` 的字节区间，只重新生成这一块，其余原样拼接。

**写入器**（`rmv6::write`，5 个测试）：给一组 `(样式, 文本)` 段落，编码出合法的 `RootTextBlock`。关键事实（这几轮真机样本 + 2026-09-07 用独立的 Python `rmscene` 交叉验证坐实，不只是"我们自己的解析器认"）：
- 全新文档没有编辑历史，`deleted_length` 恒 0、CRDT id 不用留空隙——真机活文档里那些"隔一个 id"的缝是打字/删改历史的产物，从零生成不需要模拟。
- 每段一个 wire 条目，`item_id` 指向本段第一个字符，后续字符隐式 +1；`left_id` = 上一段最后一个字符的 id（首段用 `(0,0)`）；`right_id` 恒 `(0,0)`。
- 样式表的 key = "结束上一段的换行符"的 id（等于本段的 `left_id`），value 是 `{固定字节 17, 样式码}`。
- **⚠ 真机大坑，独立验证抓出来的**：文本条目里那个叫 `is_ascii` 的字段，真机样本对**含中文的段落也写 1**——按字面意思、老实按内容判断（ASCII 写 1 非 ASCII 写 0）会通过我们自己宽松的 rmv6 解析器，但喂给 `rmscene`（它对这个字段有 `assert is_ascii == 1`）直接炸；这说明真实设备/规范要求恒为 1，字段名具有误导性。这正是"先拿独立实现交叉验证、别只信自己写的解析器"救回来的一个真实 bug——如果没交叉验证，这份文件大概率传到真机也会被拒或崩，而我们自己的往返测试完全测不出来。
- 首版只支持 5 种样式（PLAIN/HEADING/BOLD/BULLET/CHECKBOX），`NUMBERED` 当时因为怀疑格式子块有未解码载荷故意不支持——**这条限制后来证明是分析失误，§03i 已更正并补全支持**。

**验证方式与边界**：`cargo test -p rmv6` 全绿（往返：写入→用 rmv6 自己的 `Page::parse` 读回，断言条目数/文本/样式对得上）+ 额外用独立的 `rmscene`（Python，MIT，已在本仓库 venv）交叉解析生成的文件、正确读出全部 5 段文字与样式。**⚠ 尚未真机验证**——这只证明"两个独立的读取实现都认为这份文件合法、内容对"，不等于 xochitl 真的会正常打开渲染。

**上传这条路不是新领域，是本项目已经踩过、真机验证过的坑**：查了一遍才发现 `reading/protocol/inject.py`（2026-08-16 真机验证批注）+ Rust 版 `device-core/src/inject.rs` + `knowledge/pkm/src/cardnote.rs` 早就摸清楚了——xochitl 的 `POST /upload` 除 EPUB/PDF 外**也吃 `.rmdoc`**（reMarkable 官方文档包 zip：`<uuid>.metadata` + `<uuid>.content`[+`.pagedata`] + `<uuid>/<page>.rm`，**不含** `.local`，导入端自建）；multipart 字段名 `file`，`.rmdoc` 用 `application/zip`。真机行为：201 "Upload successful"、免重启出现在书库、**导入端会重新分配设备 UUID**（不是包里写的那个）——调用方按 `visibleName` 事后认领（跟书架 `render_check.rs` 的"按 createdTime 圈候选"是同一类模式）。`cardnote.rs` 甚至已经是"纯 `.rm` 组装成笔记本再传"的先例，不是 EPUB 派生的笔记。**按"不引入任何旧代码到 notes/"的红线，这些实现不能直接搬——但摸清楚的格式/字段名/UUID 重分配这几条事实可以借鉴**，note-serve 要写的 rmdoc 打包器是参照这些事实全新写的。唯一没坐实的一点：xochitl 到底是按文件名后缀（`.rmdoc`）还是按 zip 内容本身分流——现有结论全是黑盒试出来的，不是反编译坐实的，notes 线接入前该用最小样本（比如同一份 zip 换几种文件名）自己再核一遍，不能直接照搬旧结论当真理。

**打包器已经写了**（`note-serve::rmdoc`，1 测）：给一页 `.rm` 字节 + 书名 + 父文件夹，拼出 `<uuid>.metadata`+`<uuid>.content`（`fileType:"notebook"`，`formatVersion 2`/`cPages` 结构，参照真机样本 `testdata/seven_styles/book.content` 与上述旧代码印证过的最小字段集——`extraMetadata` 空对象就够，不用填一堆画笔工具状态）+`<uuid>/<page>.rm`，STORED 不压缩（zip crate，workspace 里 `epubmap` 已经在用，不是新依赖）。上传本身**直接复用 `shelf_core::xochitl::Xochitl::upload`**（共享底座、非"旧代码"，笔记线本来就已经间接依赖 shelf-core）。落地文件用 Python `zipfile` + `rmscene` 交叉核过一遍：zip 结构对、`.metadata`/`.content` JSON 合法、内嵌 `.rm` 独立解析出正确文字与样式。

**真机验证通过（2026-09-07 当场做的）**：拿 6 段测试内容（Title/Subheading/Body/Bullet×2/Checkbox 全部 5 种支持样式都占一个）打包、经临时诊断工具直接调 `shelf_core::xochitl::Xochitl::upload`（`reachable=true`）传到设备，返回 `{"status":"Upload successful"}`。真机行为跟 §03h 前半引用的旧结论完全对上：**新文档立刻出现在书库、免重启**；**导入端确实重新分配了文档 UUID 和页 UUID**（都不是包里写的那两个）；**`.content` 被 xochitl 重写过一遍**（`uuids[0].first` 换成它自己的作者 UUID、丢了我们写的 `modifed` 字段，但 `pageCount`/`fileType` 原样保留）；xochitl **自己重新渲染生成了缩略图**（`<uuid>.thumbnails/<page>.png`，38501 字节）。拉这张缩略图下来肉眼核对：**六行内容、五种样式（大标题/加粗小标题/正文换行/两个圆点无序/空心方框待办）全部渲染正确**，没有白屏、没有样式错位、没有乱码。这是笔记线第一次真正让 xochitl 打开并渲染我们自己生成的文档——`rmv6::write` 的 `RootTextBlock` 编码器与 `note-serve::rmdoc` 打包器到这里才算真正闭环验证，不再只是"两个独立读取实现互相认可"。测试文档留在设备书库根目录，标题「cangjie 笔记线真机测试」，人工核对完可以在设备上直接删除。

## 03i｜更正一处误判：NUMBERED 是无辜的，7 字节其实是 Subheading 1 的开关（2026-09-07）

用户在设备上打开 §03h 那份测试文档，反馈"没有有序列表、没有勾选框、没有分区头1"，并**用真机原生格式菜单按钮**（不是手敲字符）把这三样都补打了一遍到同一份文档里。拉回来的文件里那三样其实都在——"待办事项"那行确实是 Checkbox 未勾选（缩略图上有空心方框），用户没细看是他没注意，不是 bug；但另外两条一细查，发现**§03f 当时的结论有一处真判断错了**。

**排查过程**：把补打过的文件重新按 char_id 逐条目核对格式子块的原始字节（不是走 `TextDocument` 的样式名字，是直接读每条 `styles` 记录的 `[17, code, ...剩下的字节]`）——用户原生"Subheading 1"按钮打出来的那段，格式子块比"裸 BOLD"多整整 **7 字节**：`21 02 34 03 00 00 00`；而用户原生"已编号列表"按钮打出来的两段，格式子块跟其余样式一样只有 **2 字节**，干干净净。

**结论**：§03f 那时候看到"某处有 7 字节没读完"的警告，**把账记错了对象**——那 7 字节从来不是 NumberedList 的，是 Subheading 1 的。两件事一次性理清：
- **NumberedList（10）没有任何隐藏内容**，跟 PLAIN/BULLET/CHECKBOX 一样只是 `[17, 10]` 两字节；序号是 xochitl 渲染时按"连续几个 NUMBERED 段落"自动数出来的，不用自己存序号，**不存在"补样本才能安全写"的前提**——`rmv6::write` 已解除限制，正常支持。
- **Subheading 1 与 Subheading 2 真机确实共用 wire 码（BOLD=3）**，但区分开关就是这 7 字节 `21023403000000`：带 = Subheading 1（大字号），不带 = Subheading 2（小字号）。**跟段落在文档里的位置无关**——用户明确验证过，§03h 当时"大概率是位置决定"的猜测是错的，两级大小完全由这段固定字节决定，可控。`rmv6::write` 新增 `Paragraph::subheading1()` 补这 7 字节，`Paragraph::new(BOLD, ..)` 保持不带（语义上当 Subheading 2/小节标题用）。

**二次真机验证**：改完当场生成第二份 8 段测试文档（含一段 `subheading1()`、一段裸 `BOLD`、两段 `NUMBERED`）传真机，缩略图核对：「真 Subheading 1」明显大字号、「裸 BOLD」明显小字号，两段有序列表正确自动编号"1."/"2."。**xochitl 3.28 打字格式菜单全部 7 种样式（Title/Subheading 1/Subheading 2/Body/Bulletpoint/NumberedList/Checkbox）本模块现在都支持写入且真机验证过**——只有 Checkbox 的"已勾选"（码 7）还是只能靠原生点方框切换，写入器造不出勾上号的待办（这条限制是真的，不是分析失误）。

**教训**：读 wire 格式的"哪个字段属于哪个条目"，光凭"警告出现在文件读到一半的位置附近"去猜容易猜错——这次改成显式记录每条 `styles` 记录自己的 `char_id`，逐条目核对多余字节，才把账算清楚。以后遇到"某处有没读完的字节"，先把它跟具体的 char_id/条目绑定，别凭空间顺序臆测归属。

**产物**：`rmv6::write` 解除 NUMBERED 限制、新增 `Paragraph::subheading1()`/`SUBHEADING1_MARKER`；`notecore::model::Style::wire_code` 文档注释更正；两个真机测试文档（8 段版含全部 7 种样式）二次验证通过。

## 03j｜note-serve 生成编排：条目库 → 一章一本（2026-09-07，离线写完，✏️ 当晚已真机验证见 §03k）

§03h/§03i 把"写一份合法 `.rmdoc` 并传上真机"这条路走通了，但那时候段落列表都是测试里手写死的。这一节把"从条目库自动排出这份段落列表、判断要不要重传、旧版本怎么处理"这层业务逻辑写完——**这一节写完时只做到离线单测全绿，还没有一次真机调用**（真机验证结果见 §03k，别把这一节当"已完成"的证据，本节记的是"代码写完那一刻"的状态）。

**投影是纯函数**（`notecore::project`，零 I/O，4 个测试）：`project_chapter(book, idx)` 把一章的条目铺成 `rmv6::write::Paragraph` 列表——Title=章名，按 `Section.order` 排的每个分区起一段 `subheading1()`（名+简述），分区内条目按 `page_index` 排、样式取 `Entry.style`（映射同 §01 样式表）、文本取 `display_text()`（没转写占位"（待转写）"），条目下再各跟一段 Body 摘录勾画原文（`〔原文〕`前缀，有的话）与 AI 回答（`〔AI〕`前缀，有的话）；指向未知/已删分区的条目落一个"未分区"兜底段；已撤销条目不投影；空章返回 `None`（调用方不该为空章生成文档，见下方"未生成"档位）。`fingerprint_chapter` 用同一批输入（章名/分区名简述顺序/条目样式分区文本原文回答）算 FNV-1a 指纹，只要它不变就不重新打包上传——这是"重生成"判断的唯一依据，`Paragraph` 本身没有 `PartialEq`（跨 crate 内部字段拿不到）无法直接比较，所以指纹是单独在纯输入上算的，不是拿投影结果反推。

已知留白，写进了模块文档、不是漏项：① `Entry.subhead`（epubmap 给的 h2/h3 小节）暂不参与分组，只是简单按分区铺开，Subheading 2 细分小节头留作以后精修；② xochitl 按"连续几个 NUMBERED 段落"自动编号，摘录/回答的 Body 段插在两条有序条目之间会打断连续、编号从 1 重来——真要连续编号的清单，条目之间目前不能有摘录/回答，这次不解决。

**生成编排**（`note-serve::publish`，Strategy + 纯函数骨架，4 个测试全用内存桩、不碰网络）：`Uploader`/`TrashSink`/`EntryStore` 三个 trait 抽象出"传书"/"旧版本入队"/"读条目库"，生产实现分别包 `shelf_core::xochitl::Xochitl`、跨服务 HTTP 调 `book-serve` 的 `POST /trash/add`、跨服务 HTTP 调 `ink-serve` 的 `GET /books(/{uuid})`（同款 trait+HTTP 客户端套路复刻自 `transcribe-serve::backend::Vision`/`ink::EntryStore`，不是新发明）。`generate_chapter`：查指纹→没变返回 `Unchanged`（零网络）→变了就 `build_page_rm` 编码、`rmdoc::pack` 打包、`Uploader::upload` 传进 `《书名》`文件夹、`Uploader::claim` 按 `visibleName`+`createdTime>=now_ms` 认领设备新分配的 uuid（复用 `shelf_core::xochitl::find_documents_since`，跟书架 `render_check.rs` 是同一类"圈时间窗按名字认领"模式）→ 如果这一章之前生成过且这次真的换了新文档，旧版本 `TrashSink::add` 入队（用**生成时记录下来的旧 visibleName**，不是拿当前书名/章名重算——`book-serve` 按名字核对 uuid 防错删，算错了名字会直接被拒）→ `notebooks.rs` 记新记录。任何一步失败整章标 `Failed{error}`、**不写状态**（下次照常重试，没有"卡在半成品"的风险）；旧版本入队失败**不算这一章失败**（新文档已经生成好了，旧的多留一份不是数据丢失，只是打日志）。

**簿记是 note-serve 自己的、不是条目库**：`$XDG_STATE_HOME/notes/notebooks/<book_uuid>.json` 记每章 `{doc_uuid, visible_name, fingerprint, generated_at}`，条目库唯一写者仍是 ink-serve（本模块只读它）。丢了这份簿记文件最坏后果是"重新判一次要不要重生成"，不丢用户数据。

路由（经网关 `/api/notes`）新增：`GET /books`（每本书当前的章节生成状态）· `GET /books/{uuid}/notebooks`（同一本书的细节）· `POST /books/{uuid}/generate`（全书按需重投影）· `POST /books/{uuid}/chapters/{idx}/generate`（单章）。

**⚠️ 已知缺口，留给下一步"代理扩展 mkdir"**（§05 第 5 项）：`Xochitl::upload` 目标文件夹不存在时 best-effort 落书库根——本模块目前**不会自动建《书名》文件夹**，一本新书第一次生成前，文件夹要么已经手动建过，要么第一份文档会落进根目录（人工挪一次即可，之后 `find_folder_by_name` 就找得到、后续章节会正确落进去）。

**离线门槛**：`cargo test --workspace` 从 41 涨到 **51**（notecore 11→15、note-serve 1→7：`config`1/`notebooks`2/`publish`4，加原有 `rmdoc`1，`dump_device_test_doc` 仍是 `#[ignore]` 不计入）；零警告。**真机验证清单（当晚就做完了，结果见 §03k）**：拿一本已有真实条目的书跑 `POST /generate`，核对①《书名》文件夹里出现正确数量的章节文档、②缩略图渲染的分区/样式/编号都对、③改一条已校对文本后再跑一次只重传那一章、④旧版本确实进了回收站（`GET /trash` 能看到）、⑤新书第一次生成时落根目录这个已知缺口是否符合预期——五条全部核对通过，见 §03k。

## 03k｜note-serve 生成编排真机验证通过（2026-09-07 当晚）

§03j 写完的生成编排第一次真机跑通，而且比预想的更完整——**旧版本真的自动进了回收站，不是只挂在队列里等着**。

**准备**：交叉编译+部署 `note-serve`（`shelf/build.sh` + `SHELF_NO_BUILD=1 ./deploy.sh 10.42.0.224 --only note`，只更新这一个服务，book-serve/ink-serve/transcribe-serve 不动）。设备上《人骨拼圖》那本书之前 4 条真实条目（§03f/§03g 步骤 0 样本）当时全被判"笔画消失"标成了 `Revoked`（页面被重写过，ink-serve 正确识别成撤销，不是 bug），拉回来核对确认后，按"②手机修正"设计好的同一条路径——`POST ink /books/{uuid}/entries/{id}`——把 4 条的 `text`/`style`/`section` 补齐（校对文本用当初真实手写的原文：`第一段`/`第二段`/`第三段`/`这是一段话并画线`，样式分别对应 Bullet/Numbered/Checkbox/Body），4 条全部转回 `Reviewed`，凑出一份真实的四样式测试章节，不是编的假数据。

**第一轮（`POST /books/{uuid}/chapters/1/generate`）**：返回 `{"status":"generated","doc_uuid":"b20ec5fd-…"}`。设备书库当场核对：新文档确实出现（`parent:""`，文件夹《人骨拼圖》不存在，如预期落了根目录，见 §03j 已知缺口）、`.content` 是合法 `notebook`/`formatVersion 2`/一页、xochitl 自己渲染了缩略图。拉缩略图肉眼核对——**标题、分区头「其他」、四条条目样式（圆点/编号/方框/正文）、每条下方的〔原文〕勾画摘录，全部位置正确、无乱码无错位**，跟 `notecore::project` 设计的版式一字不差。

**第二轮（改一条已校对文本再生成同一章）**：返回新 `doc_uuid=6be690f8-…`（跟第一轮不同）；本地簿记 `notebooks/<uuid>.json` 指纹与 uuid 都更新到新值；`book-serve GET /trash/pending` 当场就能看到旧 uuid 排在队列里（`TrashSink::add` 用的是**生成时记录的旧 visibleName**，`book-serve` 按名字核对通过）。**没有额外做任何操作**，几秒后再查旧文档的 `.metadata`：`parent` 已经变成 `"trash"`——设备当时屏幕停在书库视图，新文档 `/upload` 进来触发了 `entityListModel.rowsInserted`，`shelf-trash-agent.qmd` 的 4s 防抖计时器如期打了 `selection.add`+`selectionMoveToTrash`，`trash/pending` 随即清空。**这是"条目库→一章一本"整条闭环第一次端到端全自动跑通**：生成、传书、认领、旧版本软删，中间没有人手动点一下。拉第二轮缩略图核对，编辑过的文本正确出现在渲染结果里。

**第三轮（不改任何东西再生成一次）**：返回 `{"status":"unchanged"}`，设备文档总数不变——指纹跳过重传的判断在真机上也成立，不是只在内存桩测试里成立。

**结论**：`notecore::project`/`note-serve::publish` 整条链路（投影→打包→上传→认领→旧本回收→簿记→指纹跳过）**真机全绿**，§03j 记的"⚠ 尚未真机验证"到此撤销。§05 待办第 4 项可以划掉。留在设备上的两份历史文档（旧的那份已在回收站，新的那份在书库根目录，标题都是「第2章 第一部 一天的國王 1」）人工核对完可以直接删/清空回收站。

**当场验证顺带证实的两件事**（不是这次改的代码，但值得记一笔）：① `shelf-trash-agent.qmd` 这套"纯事件驱动、无需人工确认"的软删机制在有真实数据的场景下确实可靠，不需要人守在设备旁点确认；② note-serve 这次全程走**跨服务 HTTP**（`ink.rs`/`trash.rs` 直连 ink-serve:8795、book-serve:8790 的 loopback 端口，不经网关），这个"服务互相直连而不绕网关"的套路（transcribe-serve 早就在用）在多服务真机同时跑的场景下也验证有效，没有端口冲突或注册表查找失败。

**仍是已知缺口，没打算这次解决**：《书名》文件夹自动创建（`shelf-trash-agent.qmd` 扩成 `{action: trash|mkdir}` 通用代理，§05 第 5 项）；生成按钮还没接进网页「笔记」tab（现在只能用 curl/wget 手动触发 `POST /generate`）。

## 03l｜建夹代理：《书名》文件夹自动创建（2026-09-07，真机验证通过）

§03j 记的已知缺口——目标文件夹不存在时新文档落库根——这次补上了。**QML 反编译 + `book-serve::mkdir` 队列 + `shelf/xovi/shelf-mkdir-agent.qmd` 的完整设计、反编译过程与真机验证记在 `shelf/docs/reMarkable书架白皮书.md`**（书架那条"原生建文件夹代理"段落）——按 §01 的分工原则，qmd/book-serve 是书架的机制，笔记线这边只记 note-serve 这半：

- `note-serve::publish::Uploader` trait 新增 `ensure_folder(folder_name)`：`generate_chapter` 打包完、上传前调用，fire-and-forget（不等待、不阻塞、失败只记日志）。生产实现 `XochitlUploader::ensure_folder` 先查文件夹是否已存在（`Xochitl::find_folder`），存在就什么都不做；不存在才请求 `note-serve::mkdir::BookServeMkdir`（跨服务 HTTP 调 `book-serve POST /mkdir/add`，跟 `trash.rs` 同一套路）。
- 4 个新离线测试（`ensure_folder_calls` 断言：上传前必调、空章不调）+ book-serve `mkdir.rs` 5 个测试（去重/已存在不入队/`pending()` 剔除），`cargo test --workspace` 两个仓库都零警告全绿。
- **真机验证（同日，跟《人骨拼圖》真实条目一起做的）**：改一条已校对文本触发重生成 → `ensure_folder` 正确请求建夹 → `book-serve /mkdir` 队列即时出现《人骨拼圖》→ MainView 的 8 s 轮询代理把文件夹建出来 → **再生成一次，新文档正确落进这个文件夹**（旧的、建夹前生成的那份原样留在根目录，不会被追加挪动，这是设计内行为）；重复触发不产生重名文件夹。全程五个 notes/shelf 服务 + xochitl 健康检查干净（`NRestarts=0`）。
- **一个真机操作教训**（记在这里，不是这次代码的问题）：`/home/root/xovi/start` 第一次跑完显示 `Job for xochitl.service canceled`，事后核对 `/etc/systemd/system/xochitl.service.d/` 是空的（LD_PRELOAD 等一个都没进新进程的环境），本机没装 `xovi-reenable.service`、纯 vellum tmpfs 机制这次没吃上；**原地重跑一次干净成功**。以后跑完 `xovi/start` 别只看 `systemctl is-active`（进程会正常起、只是没挂 xovi，qmd 全部形同虚设且不报错），要核对新 PID 的 `LD_PRELOAD`/`XOVI_ROOT` 环境变量（`tr '\0' '\n' < /proc/<pid>/environ | grep -E 'LD_PRELOAD|XOVI'`）。

**仍是已知缺口**：网页「笔记」tab 还没有「生成笔记本」按钮（§05 第 4 项剩的那半）；`Library.createCollection` 重复调用传同名文件夹会不会建出两个重名文件夹这条风险，本轮验证走的是"add()/pending() 两层不重复请求"的正常路径，没有刻意去撞"两次并发请求建同名夹"这种边界，留意但不阻塞。

## 03m｜用户真机核对揪出两个真问题 + 一次数据事故（2026-09-07）

用户在真机上核对《人骨拼圖》的笔记 tab 时报了四点，理清后是**一个数据事故 + 两个真代码缺口**：

**数据事故（先认账）**：§03j/§03k/§03l 那几轮真机验证时，为了让生成的笔记本截图有内容可看，我直接把用户早前口述过的"真实原文"（第一段/第二段/第三段/这是一段话并画线）用 `POST /entries/{id}` 手动打进条目库，**不是转写模型认出来的**，而且反复打了好几轮，每次都把 ink-serve 已经正确判定的"已撤销"（用户当时已经擦掉了勾画/手写）状态重新翻回"已校对"——真机日志能对上：`10:54:22 撤销 4` 之后一直到 `15:xx` 我这边还在手动 POST。用户当场问"识别这么准了吗，还是你帮我填进去的"，如实认了。事后清理时手滑又踩了一次同类坑：想清空这几条的 `text` 字段，直接 `POST {"text":""}`，没注意这条更新接口"给了 text 就按内容判定状态"的逻辑不认已撤销这个前提，把 4 条又从"已撤销"带回了"待校对"——立刻用 `/rescan` 修正回去确认 `entries:0`。这次事故记了条产品反馈（内部工单，不外发），教训是：真机验证要看到内容时，宁可用一望即知是占位符的假文本，绝不能拿用户的真实描述往真实条目库里塞，且改字段前要先读懂接口对状态字段的副作用。

**真问题①：书清空后仍赖在网页下拉列表里**。`ink-serve::doc::annotated_pages()`（摄取门槛）看的是"页 `.rm` 文件存不存在"，笔画擦光了文件不会被删，所以`ingest_doc` 会持续正确运行、持续把条目标成 Revoked，但 `GET /books` 原来列的是 `BookDb::list()`——**不过滤**，一本书只要曾经被摄取过（哪怕现在一条活条目都没有）就永远出现在下拉框。修法：`bookdb.rs` 新增 `list_active()`（只列"至少有一条非 Revoked 条目"的书），`GET /books` 换用它；新增单测 `list_active_hides_books_whose_entries_are_all_revoked`。**真机验证过**：部署后《人骨拼圖》（已清空）正确从列表消失，同时看到用户已经在真实标注的《Gulliver's Travels》正确出现（1 条活条目，真数据不是我填的）。

**真问题②：`## `/`### ` 标记设计（用户定案，替换掉没接线的 `has_underline`）**：详见 §01 表格下方新增说明段落——`notecore::marker::Marker` 从 `(Option<Style>, String)` 改成三态（`Style`/`Section`/`Subhead`），`## 文字`→找/建同名分区（`Book::section_id_for_name`，新方法+单测）、`### 文字`→覆盖 `Entry.subhead`；`#`（Title）明确不接，留给以后"纯手写笔记扫描"（会议/上课）那条还没立项的线。改动跨三层：`notecore::marker`/`model`（纯函数+新方法）→ `transcribe-serve::worker`/`ink`（`post_draft` 签名从 `Option<Style>` 换成 `Option<Marker>`，HTTP body 按变体挑字段）→ `ink-serve` 的 `POST /entries/{id}`（新接 `sectionHint`/`subheadHint`，注意 `Book::section_id_for_name` 要整个 `&mut Book`，必须在拿到条目的可变借用之前调用，不然过不了借用检查）。**离线单测全绿（notecore 17／ink-serve 10／transcribe-serve 8），但这条链路还没有一次真机验证**——没有拿真实手写走一遍"写 `## 分区名` → 转写 → 自动建分区 → 网页看到"，`##`/`###` 会不会被 OCR 转写错、`Book::section_id_for_name` 的自动建分区在真实网页交互下体验如何都还不知道，按纪律不能算闭环，见 §05。

## 03n｜二期立项：浏览态触发 + 问AI + 模型配置统一（2026-09-07）

用户用了几轮真机后提出体验重做，理清后是二期计划（**未入库的计划文件 已改版覆盖，旧首期计划原文降级为文件里的历史记录**），核心是两处：① 只要写了字、勾了线就自动进条目库、自动转写，但有的画线/写字只是整理思路、不想转笔记，浪费转写调用还搅乱列表；② "分区"身兼"笔记本怎么分组"和"要不要调 AI"两职，问 AI 前必须先想好归哪个分区，不够直接。二期把"探测"和"转笔记"拆成两个独立动作（浏览列表显式点『转入笔记』/『不需要』），AI 触发从分区简述改成按条目的问题输入框，模型配置/用量做成统一页面。用户还追问了这一整套改动（尤其新增的 `mind-serve` 常驻服务）对设备耗电的影响——计划文件里专列一节，用真机 battop 审计数据（`misc/battery-audit/FINDINGS.md`/`APP-DESIGN.md`）和书架白皮书已经算过的"SSE 心跳比轮询少两个数量级"（`shelf/docs/reMarkable书架白皮书.md:460`）来评估：结论是这版改动只会更省电（自动改按需，网络调用只少不多），不会更费；顺手在书架白皮书里揪出一处过期数字（"musl 静态服务各约 0.56MB"跟当前实测 ~1.3–4.1MB 对不上，先记着没改）。

**二期步骤 1/2 已完成（合入 `dev`，未部署真机）**：`notecore::model::Status` 拆出 `Mined`（ink-serve 探测到、还没被要求转笔记，取代原来摄取时给的初始态 `Pending`）与 `Skipped`（用户点了"不需要"）。`Entry::needs_transcribe()` 收紧：只认 `Pending`/`Draft`/`Reviewed`（真正被请求过的），`Mined`/`Skipped`/`Revoked` 都排除；已经在这三态里的条目笔画又变了仍然继续认（校对文本不会被覆盖，增量规则不变）。新增 `Entry::set_triage(target, now)` 做状态迁移，已撤销的条目拒绝操作。`ink-serve` 新条目初始态改 `Mined`，新增两个路由 `POST /books/{uuid}/entries/{id}/request`（转入笔记）/`.../skip`（不需要），共享 `triage()` 处理函数。`transcribe-serve` **零代码改动**——它的自动转写门槛直接继承自 `notecore`，只加了个集成测试 `mined_and_skipped_entries_are_never_auto_transcribed` 确认新行为。

**故意没部署真机**：这一步只改了后端，还没有浏览页 UI（步骤 3）。现在部署会让用户新写的批注卡在 `Mined` 态却没有按钮能转出来——自动转写不再捡它们，等于让日常使用断流。所以先攒着，等浏览页一起做完再一次性部署 + 真机验证（会**一并**验证 `request`/`skip` 两个端点的真机行为，不单独为步骤 1/2 开一轮）。

**离线门槛**：`cargo test --workspace` 56 个（`notecore` 17→18，`transcribe-serve` 8→9，其余不变）零警告。

## 03o｜步骤 3 浏览页真机验证通过 + 一个真机 bug 修复 + 两个记录在案未做的缺口（2026-09-07）

**真机 bug：书进回收站/被删仍赖在「笔记」列表里**。用户真机操作时报的——把《Gulliver's Travels》移进回收站甚至删除，网页「笔记」的书选择器里还在。根因：`ingest_doc` 一旦发现 `.metadata` 不是活文档（回收站/`deleted:true`/文件整个没了）就直接 `Ok(None)` 提前返回，从没告诉条目库"这本书没了"；`BookDb::list_active()` 只看条目状态过滤，没有条目被撤销，这本书就永远符合"还有活条目"。§03m 修的"清空后书还赖着"是另一条路径（`merge_page` 处理"页还在、笔画被擦了"），管不到"书本身不见了"这条。修法：非活文档时改走新增的 `ingest::revoke_stale()`——条目库里这本书还没撤销的条目全标 `Revoked`；`State::ingest` 发事件的判据从"只看 `pages>0`"放宽到"`pages>0` 或 `merge.revoked>0`"；启动追平的候选集合从"只找活的 EPUB"扩到"条目库里所有已知 uuid 都追平一遍"（不然回收站/删除动作发生在服务没跑的时候，重启也追不上）。新增单测覆盖回收站/彻底删除/从没追平过三种场景。**真机验证过**：部署后直接查 `/books`，《Gulliver's Travels》从列表消失，条目状态从 `draft` 变成 `revoked`（内容留痕没删）。

**步骤 3（浏览页 UI）真机验证通过**：网关「笔记」tab 拆「浏览」（按页分组只列 `Mined`，转入笔记/不需要两个按钮）/「整理」（只列 `Pending`/`Draft`/`Reviewed`）两个子视图（`shelf/services/shelf-gateway/ui/app.js::renderNotes`）。部署后用真实勾画+手写走了一遍完整闭环：新批注正确落 `Mined`（`transcribe-serve` 那一轮 `scanned:0`，确认没被自动转写捡走）→ 调 `/request` → `Mined→Pending`（`pending` 计数 0→1）→ ink-serve 事件唤醒 transcribe-serve 自动转写 → 状态自动变 `Draft`（转写文字与手写内容对上）→ 对一条 `Revoked` 条目调 `/request` 服务端正确拒绝（400）→「不需要」把条目转 `Skipped` 后 transcribe-serve 再跑一轮确认没扫到它。这就是二期步骤 3 计划里定的验收标准，全部真机确认。

**真机验证时顺带发现两个新问题，用户拍板"先记录、不改代码"，留到二期后续排期**：

1. **纯勾画（没有旁边手写）的页抓不到内容**。现在的摄取管线是"以手写簇为主"倒着找勾画的：`notecore::geom::cluster()` 只聚手写笔画，`pair()` 只回答"每一簇手写离哪条勾画最近"；`drafts_of_page()` 按簇遍历生成草稿，没被任何簇引用的勾画在这一步直接被丢弃，连候选都不会生成。想收，画了线不写字这种最简单的用法，现在的架构完全接不住。
   **候选设计**（未实现）：`drafts_of_page` 补一步——`pair()` 结果里没出现的勾画下标，单独生成一条 `ink: None, quote: Some(...)` 的草稿（`PageDraft.ink` 要从 `Ink` 改成 `Option<Ink>`）；这类"纯勾画条目"一样从 `Mined` 起步、一样进浏览页，只是没有裁图（前端 `(e.ink&&e.ink.crop)` 判断已经处理"无裁图"情形，不用改 UI）。因为勾画文字是 `GlyphRange` 原生给的精确文字、不需要转写，`Entry::set_triage` 要加一条：目标是 `Pending` 但 `ink` 是 `None` 且有 `quote` 时，直接 `text = quote.text.clone()`、状态跳到 `Reviewed`（点"转入笔记"这个动作本身就是确认）。`merge_page` 的认领/失效逻辑要加一条用勾画自己的 `Highlight.id`（CRDT id，不是笔画 id）做指纹，勾画被擦掉时这类条目也要能撤销——`Quote` 需要新增一个 `id` 字段（`#[serde(default)]` 兼容旧数据）。
2. **手写位置靠下时裁图截不到**。裁图吃的是 xochitl 现成的 384×512 缩略图，这张图只画了生成那一刻**视口内看得到的部分**；页面逻辑高度可能超出这个视口（长页/滚动过的页），下面的内容缩略图压根没画到，`crop.rs::MIN_CROP_PX` 守卫会正确报错拒绝（不喂废图给视觉模型），但代价是这条批注**永远转不出来**（真机复现：`8b2acb92ac47e313` 那条批注 bbox y 落在 1306~1352，超出配置的 `page_h=1280`，裁出 100×1 px，`revoke_stale` 之外新增的失败上限打满仍不成）。真机测的时候还顺带撞见一个衍生问题：手写贴印刷勾画行太近时，就算裁到了图，缩略图混进旁边的印刷体，视觉模型会读串行、原样抄印刷体而不是手写（"乱画一通"那条实测：转写结果 = 旁边勾画原文，不是用户写的字）。
   **候选设计**（未实现，`crop.rs` 文件头注释其实早留了口子"精度不够时再换高分辨率自渲染"）：不依赖 xochitl 的缩略图，改成从 `.rm` 的笔画矢量数据（`rmv6::page::Stroke{points,thickness,color}`）自己在 `ink.bbox` 范围内画折线光栅化出一张白底裁图。天然不受"视口"限制（矢量坐标没有滚动上限），顺手把"印刷体泄漏"也一起解决（自己画的图背景是白的，不会混进印刷体）；代价是丢了缩略图自带的印刷体上下文，且要新写一个简单折线光栅化（`image` crate 已经在用，不用加依赖）。

`## `/`### ` 分区/小节标记这条（§03m 已知的老缺口）这次也没能真机验证成，试了两轮：第一轮撞见上面第 2 点的"印刷体泄漏"（手写贴印刷勾画太近，裁图混进印刷体，转写结果原样抄了印刷体）；第二轮换到页面空白处干净写"`## 测试分区2`"，裁图这次确认干净（拉下来肉眼看过，没有印刷体混入），但转写结果是"井开阅讨分匹2"——纯粹是**视觉模型没读对这几个字**，跟裁图/管线都无关，是这次手写偏行草连笔，落进了 `b1-handwriking-ocr-derisk` 那份 de-risk 报告早记过的"快写~60%局部整段崩"区间。所以现在能确认的是：管线本身没问题（干净输入会正常喂给模型），卡点纯粹是这一次样本的手写识别准确率，不是这版新代码的缺陷。marker 功能端到端仍然没有一次"转写对了、`##` 真的被认出来"的正例，等哪次手写工整一点的样本或自渲染裁图（可能顺带提高分辨率/清晰度）落地后再看有没有机会真机坐实。

**离线门槛**：`cargo test --workspace` 57 个（`ink-serve` 10→11，其余不变）零警告。

## 03p｜两个缺口补完 + mind-serve（步骤 4）+ 一个前端崩溃 bug，二期真机验证收尾（2026-09-07 晚）

§03o 记的两个缺口本来是"先记录、不改代码"，用户当场改主意让接着做；同一晚顺手把二期步骤 4（`mind-serve`）也做完了。

**纯勾画条目：已实现，真机验证通过**。按 §03o 的候选设计落地：`PageDraft.ink` 改 `Option<Ink>`；`drafts_of_page` 收尾再扫一遍 `pair()` 没提到的勾画，单独生成 `ink:None` 草稿；`merge_page` 新增一条认领路径——纯勾画草稿没有笔画指纹，靠勾画自己的 CRDT id（`Quote` 新增 `id` 字段，`#[serde(default)]` 兼容旧数据）匹配，勾画被擦掉时这类条目一样会撤销（撤销守卫从"只认 `ink` 存在"放宽到"`ink` 或 `quote` 有一个在"）。`Entry::set_triage`：这类条目"转入笔记"不经过 `Pending`/`Draft`——直接落 `text=quote.text`+`Reviewed`（没有手写可转写，`needs_transcribe()` 要求 `ink` 是 `Some`，卡在 `Pending` 会死等）。**真机验证**：单独勾一段不写字，合上书，条目库正确出现一条 `status:"mined", ink:null` 的条目；点「转入笔记」后直接是 `reviewed`（跳过转写）。

**裁图自渲染：已实现，真机验证通过**。`ink-serve::crop::render_ink` 直接吃 `.rm` 已解析出的笔画矢量数据（`Stroke.points/thickness`，`ink-serve::ingest` 里 `page` 变量本来就有），在 `ink.bbox`+`margin` 范围内画折线，白底黑线，不依赖 xochitl 缩略图；笔画按 `stroke_ids` 精确挑（不按 bbox 相交，避免混进邻簇）。旧的 `crop_png`/`PageGeom`/`doc::page_thumb` **整段删除**（改自渲染后零消费者，`cargo build` 报 dead_code——两条路径不值得同时维护；384×512/960×1280 那套真机坐标标定的历史仍留在 `config.rs` 文档和 §03g，代码退役不影响记录）。真机验证：新写的一条手写批注正常转写成功、内容对得上（后来被 mind-serve 引用去回答问题，见下文）；至此没再复现过"没有裁图"的转写失败。**没有专门再验证"写在页面很靠下的位置"这个原始症状**（架构上已经不可能再复现——矢量坐标没有视口上限——但没有刻意造一个"接近旧配置 `page_h=1280` 边界"的样本去踩一遍旧 bug 的确切复现步骤，跟之前一样靠自然写字撞上）。

**`## `/`### ` 标记：仍未真机验证成**，但结论更清楚了——见 §03o 末尾，第二轮已经排除了裁图/印刷体泄漏，卡点纯粹是这次手写行草连笔的识别准确率（`b1-handwriking-ocr-derisk` 报告早记过的已知限制），不是这两个缺口修完后还剩的代码问题。

**步骤 4：`mind-serve`，真机验证通过**。新服务 `notes/services/mind-serve/`（loopback 8797），照抄 `transcribe-serve` 的 `config`/`backend`/`ledger`/`ink` 骨架，换成文字模型 + 按条目单发：`backend::TextModel` trait（消息体纯文本，没有 `image_url`，跟 `transcribe-serve::backend::Vision` 同一模式但两边没抽公共 crate——各自几十行胶水，专项专用不值当）；`prompt::build` 拼"书名+章节+勾画原文+旁边批注（已校对/转写文本）+用户问题"；`worker::ask_entry` 要求条目已勾 `ask_ai` 且填了非空 `question`（都是 ink-serve 存的字段，防止绕开勾选框误触）；答案写回 `Answer{text, backend, at, brief}`——`brief` 语义从首期"分区简述存档"改成"当时问的问题存档"。**没有批量循环、没有 SSE 订阅**——不像 `transcribe-serve` 监听 ink-serve 事件自动跑，`mind-serve` 纯被动等 HTTP 请求，空闲零 CPU，比 `transcribe-serve` 还轻（耗电评估第 5 点原话）。`notecore::model::Entry` 加 `ask_ai: bool`+`question: Option<String>`；`ink-serve` 的 `POST /entries/{id}` 接 `askAi`/`question` 两个新字段。网页「整理」视图每条新增「问AI」勾选框+问题输入框+提问按钮（改即存、点提问才真的调 `mind-serve`）。

顺手清理了一处过期 UI 文案：首期分区编辑器写着"给 AI 的要求"+"调模型"勾选框，二期 `Section.ai`/`Section.brief` 早就不驱动任何 AI 逻辑了（AI 改按条目单发）——`Section.brief` 唯一还有用的地方是印进笔记本分区标题（`project.rs`），文案改成"说明（会印进笔记本标题里）"，"调模型"勾选框直接删（显示一个不做事的开关比没有更误导人）。`Section.ai` 字段本身没删（dead 但无害，跟悬而未决的 `Section.triggers` 是同一类遗留，见 §05）。

**真机验证过程**：勾一条已经转写好的条目、勾「问AI」、填问题、点提问——**先踩到一个真实限制**：这台设备配的 DashScope 测试 key 只对 `qwen3-vl-plus`（视觉模型）开了权限，`mind-serve` 代码缺省的文字模型 `qwen-plus` 被拒（`403 access_denied`），不是代码 bug。把这台设备上 `mind-serve` 的模型手动改成 `qwen3-vl-plus`（VL 模型本来就能处理纯文字请求）后重试，拿到具体切题的回答，`answer` 正确写回。**代码里的默认模型仍然是 `qwen-plus`**——没理由为一把权限受限的测试 key 改全局默认，等真正要发布/换一把权限完整的 key 时再重新评估这个默认值是否合适。

**真机验证过程中顺带揪出一个前端 bug**：「浏览」「整理」两个视图给同一分组内的条目排序时，`a.ink.bbox[1]-b.ink.bbox[1]` 没考虑纯勾画条目 `ink` 可能是 `null`——一进列表就抛 `TypeError`，`renderBrowse` 先清空 `innerHTML` 再崩在排序这一步，用户看到的是**整个「浏览」子视图空白**，没有任何报错提示（真机验证时用户直接反馈"「浏览」子视图空白"，靠这条线索定位到的）。修法：`(a.ink?a.ink.bbox[1]:0)-(b.ink?b.ink.bbox[1]:0)`，纯勾画条目排序退化成"分组内排最前"，不影响手写条目原有的按位置排序。**教训记一笔**：给条目模型新增一个可选字段（这里是 `ink` 从必有变可选）时，光顾着改生成条目的后端逻辑容易漏掉——所有*消费*这个字段的前端代码（尤其是排序/比较这种容易图省事写"肯定存在"的地方）都要过一遍，`grep` 一下字段名全部引用点是比较可靠的排查方法（这次就是这么定位到第二处遗漏的）。

**离线门槛**：`cargo test --workspace` **68 个**（`notecore` 18→20、`ink-serve` 11→10〔换自渲染裁图，删 4 个旧缩略图测试加 3 个新的〕、**`mind-serve` 全新 10 个**，其余不变）零警告。

## 03q｜步骤 5：模型配置统一面板，二期收尾（2026-09-08，真机验证通过）

二期唯一剩的一步：`transcribe-serve`/`mind-serve` `GET /config` 加脱敏 key 预览、网关"模型"统一设置面板（视觉+文字并排、用量卡片）。落地：

- **`keyMasked`**：`TranscribeConfig`/`MindConfig::public()` 各加一个 `key_masked()`——有 key 就回最后 4 位（如 `...ab12`），没 key 回 `null`；只在服务端算，`public()` 从不整串回显（跟 `hasKey`/`keySource` 同一条纪律）。两边各补一条单测断言（有 key 的脱敏格式、清 key 后变 `null`）。
- **网关「模型」统一面板**：`renderNotes` 的「整理」子视图里新增一个折叠块，视觉（transcribe）/文字（mind）并排两栏，各自：key 输入框（脱敏预览行 + 保存 + 清 key）+ 模型/baseUrl 输入框 + 用量行（调用/成/败、token 入/出、最近错误）。**原「转写」折叠块的 key/模型/baseUrl 编辑挪到这个新面板**——同一份配置两处都能编会互相打架（改了一处另一处显示的还是旧值），「转写」折叠块只留自动转写开关、跑一轮/重试失败按钮、失败清单这几项转写工作流本身的操作，不再管"是什么模型"。`mind-serve` 之前**完全没有配置 UI**（§03p 记的"手动改成 qwen3-vl-plus"是没有网页入口、只能 SSH 改配置文件），这是它第一次有网页可配。

**真机验证过程**：`SHELF_NO_BUILD=1 ./deploy.sh --only gateway,transcribe,mind` 部署后三服务 `active`。逐项走了后端到设备文件系统这条链路（网关本身的登录密码这次没有，无法从浏览器点一遍，见下方"没验证的"）：
- `GET /config`（loopback 8796/8797）：`keyMasked` 真实计算正确——设备上已配置的真实 key 算出 `"...yP5A"`，两个服务一致（同一把 key，`mind-serve` 配置注释里写的"图省事共用"符合实测）。
- `PUT /config` 空 `apiKey`（前端"没编辑 key 框就保存"时发的就是空串）不清已存 key、`keyMasked` 原样不变；改 `model` 为同一个值（no-op）后重新 `GET` 确认写穿——`apply()`/`public()` 这条路径在真实配置文件（不是测试 fixture）上端到端走通。
- 二进制里 `strings` 确认新面板的元素 id（`mvsave`/`mtsave`/`keyMasked` 等）被正确编译进部署的 `shelf-gateway`。

**补验证（用户给了登录密码后，同一天）**：拿到网关密码后，把上面那几步全部**换成走真实 HTTPS 网关**（`curl -k -u shelf:<密码>`，走 Basic 认证，跟浏览器登录后是同一条鉴权路径，不再是绕过网关直查 loopback）重跑一遍——`GET /api/transcribe/status`/`/api/mind/status`（网关代理 + auth 层，不是直连 8796/8797）返回的 JSON 形状与字段名（`config.keyMasked`/`config.model`/`config.baseUrl`/`usage.calls`/`usage.ok`/`usage.failed`/`usage.promptTokens`/`usage.completionTokens`/`usage.lastError`）跟前端 `mFill()` 读取的字段**逐一对得上**；`PUT /api/transcribe/config`/`/api/mind/config` 空 `apiKey`/同值 `model` 的 no-op 请求也经网关认证层正确写穿、`keyMasked` 不变——这是浏览器点「保存」按钮实际会走的那条完整路径，不再只是 loopback 旁路。**唯一还没做的是纯视觉确认**（面板排版好不好看、按钮点着顺不顺手）——这层我没有浏览器渲染工具能替代，数据契约已经证实完全对，剩下的是留给用户自己点开网页看一眼的体验判断，不是功能风险。

**离线**：`cargo test --workspace` 仍 **68 个**（这次是往既有测试加断言，没加新测试函数）零警告；`cargo build --workspace`/`--target aarch64-unknown-linux-musl` 零警告；`node --check app.js` 通过。

**二期状态：步骤 1–5 全部完成、真机验证通过**（模型面板经真实网关认证层验证过数据契约，浏览页/模型面板两处纯视觉排版仍未经人眼确认，已记录在案，不影响功能层面已验证的结论）。

## 03r｜三期：Markdown 导出 + 落设备笔记本/Obsidian/删除三选一（2026-09-08，真机验证通过）

用户提出的缺口：转写/校对/问答都做完之后，内容最终该由用户决定去哪——落设备笔记本、落 Obsidian、还是不要了，不该是两条投影管线（`project.rs` 设备笔记本 / 新增的 `export.rs` Obsidian）各自无差别吞掉所有活条目。围绕"删除"这条我先提了个软删方案（新终态 + 手动清空回收站），用户认可但追问"软删数据会不会无限扩充"——这条问到点上了，落地方案里专门有一节答这个。

**`notecore::export`（新模块，条目库 → Markdown）**：一章一个 `.md`（`vault/<书名>/第N章 章名.md`，文件名跟 `note-serve::publish` 给设备笔记本起的 `visibleName` 完全一致）+ 一个书索引页（`vault/<书名>/书名.md`）。跟 `project.rs`（设备笔记本投影）用同一批判据（下面会讲这个判据本身修了一个真机发现的 bug）、同一套排布逻辑（按分区、分区内按页序），只是产物是纯文本：front-matter（`book`/`author`/`chapter`/`pages`/`status`/`tags`，字符串字段自己写了个 YAML 双引号转义防冒号/引号把解析搞错）+ `[[书名]]` 反链 + 各分区 `## 分区名` + 条目按样式渲染成列表/正文，稳定 `Entry.id`（16 位小写 hex，天然合法 Obsidian 块 id）当块锚 `^id`——改字段按 id 幂等覆盖同一行，不会因为重新导出产生重复块或丢反链。11 个新单测（front-matter 各字段、分区排序、块锚、引用块缩进、YAML 转义、空章/空书判空）。

**`notecore::model::Destination`**（`Notebook`/`Obsidian`/`Both`，`#[serde(default)]` 兼容旧条目库）+ `Entry.destination`：`project.rs` 的 `live_entries` 只收 `wants_notebook()`，`export.rs` 的只收 `wants_obsidian()`。**缺省 `Both`**——两处都要——不是一开始想的 `Notebook`：如果缺省是"只留设备"，我刚写完的 md 导出对所有已有内容会立刻变成"什么都不导出"（得先给每条条目手动选一遍才肯导出），这跟"新功能应该对已有内容立刻可用、不引入需要用户先做一遍设置的新摩擦"这条项目一贯的默认值哲学正相反；`Both` 才是真正"不改变现状"的缺省——旧条目库不设置这个字段，两条投影该收的还收，用户想收窄再手动改成 `Notebook`/`Obsidian`。

**软删数据无限扩充的问题，答案是"清空回收站"这个既有模式的复用，不是新发明**：新增 `Status::Archived`（三期"不要了"，跟 `Revoked` 同类的终态，区别是触发方是用户主动点删除而不是笔画被擦掉）+ `Book::purge_terminal()`——物理移除 `Archived`/`Revoked`/`Skipped` 这三种"终态、不再活跃"的条目，是唯一真正腾空间的操作。**手动触发，不自动跑**，跟项目里书级回收站"不自动清空回收站"（§05 明确不做清单，网页按钮走 `emptyTrash()` 用户显式点）是同一条纪律——软删本身没有过期时间，但清空是用户随时能点的显式动作，不会无限膨胀到用户忘了它存在。

**服务端接线**：`ink-serve` 的通用 PATCH 加 `destination` 字段（跟 `style`/`section` 同一个模式）；新增 `POST entries/{id}/archive`（复用现成的 `triage()`/`set_triage()` 机制，`Archived` 落到 `set_triage` 的通用分支，不用特殊处理）、`POST /purge`。`note-serve::export.rs`（新模块）落盘 `export_book()`/`export_chapter()`——按文件名幂等覆盖（`fs::write` 整文件替换，不做增量 diff，文件很小重写比比对更简单可靠），`POST /books/{uuid}/export`（全书）/`.../chapters/{idx}/export`（单章，为了保证索引页"哪些章有内容"始终对，单章按钮内部也是整本重导，文件都很小这个代价可以忽略）。

**网关「整理」视图补齐三处控件**（这次不是"页面设计"，是让新功能可用——跟本会话里每个功能落地时都顺手加最简控件的一贯做法一致）：每条加去处下拉（`DEST_NAMES`）+ 「不要了」按钮（点了要 `confirm()`，因为是不可逆的单向操作，只是软删不是真删）；章头加「生成笔记本」「导出 md」两个按钮——后者是这次新加的，前者其实是 `note-serve` 很早就有、`POST /chapters/{idx}/generate` 端点一直在，只是网页一直没接（§05 老早记的缺口"网页「笔记」tab 还没有「生成笔记本」按钮"），这次一起补上；书头加「清空回收站」按钮（也要 `confirm()`）。

**真机 bug（真机验证时亲眼发现，不是纸上谈兵）**：`project.rs`/`export.rs` 的 `live_entries` 原来判据是 `!= Revoked`——这意味着 `Mined`（还没被要求转笔记）、`Skipped`（用户点了"不需要"）**一直在混进两条投影**，只是之前从没写过 md 导出、也没人细看过设备笔记本里混进来的内容才没暴露。真机拿人骨拼圖那本书导出第 2 章时，导出的 `.md` 里赫然出现一条用户早前测试时点过「不需要」的条目（"他指著一塊…譚美珍咕噥說：「亂畫一通。」"，状态 `skipped`），肉眼一眼就看出来不对——这条明明是用户明确拒绝过的内容，不该出现在任何投影里。查代码发现判据写反了：本该是"只收真被要求转笔记的"（`Pending`/`Draft`/`Reviewed`）这个允许列表，写成了"不是撤销的"这个拒绝列表，两者不等价（`Mined`/`Skipped` 都不是 `Revoked`，但也不该被投影）。改成跟「整理」网页视图自己的过滤条件（`['pending','draft','reviewed'].includes(e.status)`，这条一直是对的）对齐的允许列表写法，两处各补一条回归测试，真机重新导出确认那条 `Skipped` 的内容从 `.md` 里消失了。**这个 bug 从二期"浏览态"改造（2026-09-07）落地时就存在，一直没被发现，是这次真机测试三期新功能时顺带揪出来的**——再次印证"真机验证不能拿离线单测替代"，这个 bug 离线单测测不出来是因为原来的测试从来没写过"混进 Mined/Skipped 会怎样"这种反向断言。

**真机验证**（10.42.0.224，走真实 HTTPS 网关认证层，用户给了登录密码）：
- `destination` 字段 PATCH：对人骨拼圖真实条目改成 `obsidian` 再改回 `both`，`GET` 确认每次都正确写穿——可逆操作，直接在真实数据上验证。
- 老条目库反序列化兼容：真实设备上已存在、三期改动之前落盘的条目，`GET` 出来 `destination` 全部正确显示 `"both"`（`#[serde(default)]` 生效，不用迁移脚本）。
- Chapter export：`POST chapters/1/export` 落盘到 `/home/root/.local/share/notes/vault/人骨拼圖/`，`SSH cat` 出文件内容核对——bug 修复前导出文件里有 3 条内容（含那条 `Skipped`），修复后只剩 1 条真正 `Reviewed` 的（且 front-matter `status` 从 `draft` 正确变成 `reviewed`，因为唯一剩下的条目状态就是 `Reviewed`）。
- Chapter generate：`POST chapters/1/generate` 首次经网关走通（这条路由存在很久，一直只能 curl 内部测，这次是第一次从 HTTP 认证层完整验证），设备上新文档正确出现在《人骨拼圖》文件夹（`parent` 字段核对过）。
- **archive/purge 两个端点没有对着人骨拼圖这本真实历史验证数据执行**——这两个是一次性、不可逆的动作（`archive` 软删且当前没有反悔 UI；`purge` 是真物理删），底层逻辑（`set_triage` 接受 `Archived`、`purge_terminal` 只删三种终态）已经有充分的 `notecore` 单测覆盖，HTTP 包装层薄到只是直通调用，代码层面的信心足够；但"没事先问用户就拿真实设备上的历史数据去执行不可逆操作"这件事本身不该做，所以留白，等用户自己在网页上点，或者明确要求时再验。

**离线**：`cargo test --workspace` **93 个**（`notecore` 31→41、`note-serve` 11→15，其余不变）零警告；`cargo build --workspace`/`--target aarch64-unknown-linux-musl` 零警告；`node --check app.js` 通过。

**补一版（同一天，用户当场反馈）**：用户看到"✓ 已导出到 vault"这句话追问"vault 在哪里"——落到设备自己的 `$XDG_DATA_HOME/notes/vault/` 里，普通用户没有 SSH 根本够不着，这句提示等于什么都没说。用户的预期很朴素：应该跟正常网页导出一样直接给浏览器一个文件下载，存到哪由浏览器自己的下载设置决定（没配置就是系统默认下载目录，配了"每次询问"会弹框选）——这正是**标准浏览器下载行为免费提供的**，不需要额外造"用户指定目录"的界面（Web 应用本来就管不到、也不该管本地文件系统的具体路径，那是浏览器的权限边界）。

排查时发现一个此前没暴露过的缺口：`shelf-gateway::proxy::forward`（反代 `/api/<seg>/*` 转给后端服务的那层）**只转发 `Content-Type`，其余响应头一律丢弃**（`headers: vec![]` 硬编码）——之前证书下载端点能正常触发浏览器下载，是因为那个端点是网关自己直接处理、根本不经过 `proxy::forward` 这层，没人踩过这个坑。修法：`proxy::forward` 额外转发 `Content-Disposition`（只转发这一个，不给后端服务开口子夹带别的头，比如不会转发 `Set-Cookie` 这类敏感头）。`note-serve` 新增 `GET /books/{uuid}/chapters/{idx}/export.md`——跟 `POST .../export` 读同一份 `notecore::export::export_chapter_md`，内容当场用 `Content-Disposition: attachment` 吐给浏览器（不是从落盘文件读，避免"盘上文件是不是最新"的疑问）；文件名走 RFC 5987（`filename*=UTF-8''...`，中文章名要这个；`filename=` 给一个非 ASCII 字符替换成 `_` 的兜底，老客户端至少存成不乱码的文件名），新增 `content_disposition()` 纯函数 + 2 个单测。网关「导出 md」按钮改成：`POST` 落盘（还留着，给以后 `notes pull` 用）成功后紧接着 `window.open()` 那个新 `GET` 端点，真触发浏览器下载。

**真机验证**（经真实 HTTPS 网关认证层）：`GET .../export.md` 响应头 `Content-Disposition` 正确带着人骨拼圖真实章名的 RFC 5987 编码（`filename*=UTF-8''%E7%AC%AC2%E7%AB%A0...`）、`Content-Type: text/markdown; charset=utf-8`、内容跟落盘那份一致（延续本节前面修的 `Mined`/`Skipped` 过滤，确认没有回归）；本章没内容时正确走 404 JSON、没有 `Content-Disposition` 头（确认这次改动没有影响原有的错误响应路径）。离线：`cargo test --workspace` **95 个**（`note-serve` 13→15）零警告。

## 03s｜三期：砍掉"分区"，条目按页序平铺（2026-09-08，真机验证通过）

同一天，用户拍板砍掉"分区"这个从首期就有的概念：AI 触发早就是 `Entry.ask_ai`/`question` 的事（二期已改成按条目单发，见 §03n），分区兼职的"笔记本排版分组"这半也不要了——条目一律按页序平铺，格式差异全靠各自的 `Style`（正文/无序/有序/待办，来自现成的 7 种手写约定 OCR 识别），AI 问答产出的条目同样只是普通 `Style`（缺省 `Body`）的条目，不特殊对待。

**移除范围**（比听起来的大，牵连到三层）：
- `notecore::model`：`Section` 结构体、`default_sections()`、`Book.sections`、`Entry.section`、`Book::section_id_for_name()` 整个删掉。
- `notecore::marker`：`Marker::Section` 变体删掉，`## 文字`（两个 `#`）不再识别成任何东西（`### 文字` 小节标题不受影响，那是 `Entry.subhead`，跟分区是两回事，继续有效）。
- `notecore::geom`：`has_underline()`（"一行字+下划线=分区头候选"的几何判定）——模块自己的注释早就写着"只给谓词，不建分区"，从没被 `ingest_doc`/`merge_page` 真正接上过；分区没了，这条判据彻底失去存在理由，一并删除（不是这次新增的死代码，是清理一处更早就该清的旧死代码）。
- `notecore::project`/`export`：`project_chapter`/`export_chapter_md` 不再按分区分组、不再有"未分区"兜底段——直接把本章活条目按页序整章平铺。`fingerprint_chapter` 的哈希输入相应精简（去掉分区名/简述/顺序）。
- `ink-serve`：PATCH 里 `section`/`sectionHint` 字段处理、`PUT /books/{uuid}/sections` 端点整个删掉；`subheadHint`（`### 小节`）保留。
- 网关「整理」视图：分区编辑折叠块（名字/简述/增删/保存）、每条的分区下拉，整个删掉。

**兼容性**：旧条目库 JSON 文件里落盘的 `sections`/`section` 字段会被 serde 静默忽略（未知字段默认丢弃），不需要迁移脚本、不会读出错——真机验证过（人骨拼圖这本书之前的真实数据里就带着这两个字段）。

**真机验证**（经真实 HTTPS 网关认证层）：
- `GET` 读回人骨拼圖真实条目库（磁盘上的 JSON 仍是三期以前带 `sections`/`section` 字段的旧格式）：新版本正常反序列化，响应里两个字段都正确消失，没有任何解析错误。
- `PUT .../sections` 确认已 404（端点真的没了）。
- 重新导出第 2 章：`.md` 里不再有任何 `##` 分区头，唯一的活条目直接跟在 `[[人骨拼圖]]` 反链后面——跟三期前那版（§03r 里贴的、带"## 未分区"的导出样本）对比着看差异很直观。
- 重新生成第 2 章设备笔记本：走通，没有因为删掉分区相关代码而崩溃或报错。

**离线**：`cargo test --workspace` **94 个**（`notecore` 41→40，删了 `section_id_for_name` 一个测试，其余相关测试改名字/改断言但数量不变）零警告；`cargo build --workspace`/`--target aarch64-unknown-linux-musl` 零警告；`node --check app.js` 通过。

## 03t｜「整理」区四点反馈：模型预置下拉 + 移到管理台、回收站显内容、条目卡片重设计（2026-09-08，真机验证通过）

用户拿真机用了「整理」区之后提了四点，逐条落地：

**① 「转写」折叠层看不懂是干嘛的**：不是功能问题，是文案问题——三期步骤 5 把 key/模型配置搬出这个折叠块之后，剩下的自动转写开关/立即跑一次/重试失败/失败清单这几项，脱离了原本"配置+状态"的上下文，单看确实不知道在干嘛。补一句说明："合上书后台会自动转写手写批注，正常情况不用管这里——这里是给你看进度、失败了手动重试、或临时关掉自动转写用的。"没删任何功能。

**② 模型管理搬进「管理」tab，重新设计**：用户点出两个问题——不该待在笔记专属的「整理」区（以后可能不止笔记线用到模型）；不该让用户自己填 baseUrl，模型该是下拉选。落地：
- `transcribe-serve`/`mind-serve` 的 `config.rs` 各加一张 `Preset` 表（视觉/文字分开维护，`id`/`label`/`model`/`baseUrl` 四元组的静态常量数组）：视觉表 `qwen3-vl-plus`/`qwen-vl-max`/`qwen-vl-plus`，文字表 `qwen-plus`/`qwen-max`/`qwen-turbo`/`qwen3-vl-plus`（VL 模型本来就能答纯文字问题，之前真机踩过这个事实，见 §03p）。
- `PUT /config` 新增 `preset` 字段：查表原子设置 `model`+`base_url`（一步到位，不会出现"model 换了 baseUrl 忘换"这种半吊子状态），未知预置名直接拒绝报错。`"custom"` 是转义阀，留给真要接非 DashScope 的 OpenAI 兼容口——这种时候才退回手填 `model`/`baseUrl` 那条老路径（没删，只是不再是默认路）。`GET /config`/`/status` 相应加 `presets`（表本身）+ `activePreset`（当前配置匹配哪个预置，不匹配算 `"custom"`，服务端算好直接给，前端不用自己猜）。
- key 语义也改了：脱敏显示已保存的 key 之后，**网页上不再能直接在原处改写**——只剩「删除」按钮；真要换 key 得先删再填。没保存 key 时才给输入框+保存按钮。原来的设计是"看着像能编辑一个看不见的旧值，其实一保存就是整个覆盖"，容易造成"我明明没改怎么变了"的困惑；现在两种状态各自只有一套明确的操作，没有歧义状态。
- 网页新建 `mountModelPanel()` 通用函数（视觉/文字两张卡片共用同一套渲染+交互逻辑，各自挂一个实例），挪进「管理」tab 新增的"模型管理"卡片。

**③ 回收站不再是纯按钮**：原来「清空回收站」是「整理」页顶部一个按钮，点了就是 `confirm()` + 全清，看不到要丢的是什么。改成「整理」拆出第三个子视图「🗑 回收站」，真列出 `Skipped`/`Revoked`/`Archived` 状态的条目（页码、章节、原文或转写文本），「清空回收站」按钮挪进这个视图，点之前先取真实条数放进确认文案（"永久清掉这 N 条……"），回收站本来就空时点了直接提示"没什么可清"，不再空转一次网络请求。**这个功能零后端改动**——`GET /books/{uuid}` 本来就带全部条目（含三种终态的，`bookdb` 从不过滤），前端按 `status` 过滤即可，之前只是没人把这份数据显示出来。

**④ 条目卡片重新设计**：新增一套 `.entry`/`.entry-head`/`.entry-body`/`.entry-crop`/`.entry-main`/`.entry-quote`/`.entry-text`/`.entry-ops`/`.entry-ask`/`.entry-answer` CSS 类（`style.css`），把原来一个裸 `flex` 行（图+一大坨内容糊在一起）拆成五块视觉分区：手写裁图、勾画原文引用、转写/校对文本框、样式与去处控制、问 AI 区——卡片化（圆角+边框+内边距，不再是无边界的行）。AI 回答不再藏在 `<details>` 折叠里（"问了就该看得见"，折叠反而多一次点击）。响应式只用一条 `@media(max-width:30em)`：窄屏裁图撑满宽度、宽屏裁图侧栏对齐，手机和桌面共用同一份 DOM，不用 JS 判断视口宽度分别渲染两套。

**真机纠错（用户当场指出一处误删）**：`Marker::Section` 变体删除时，我把 `## 文字`这个**手写标记本身**也一并当死代码删掉了（只识别 `### `）——用户明确指出这不对：`##` 这个手写约定要继续识别，只是不再驱动已经删掉的"分区"数据结构。问清楚具体该映射到哪之后（同 `### ` 一样覆盖 `Entry.subhead`，两者不分层级），改了 `notecore::marker::split_leading_marker` 的判据从"三个及以上 `#`"放宽成"两个及以上 `#`"，`Marker::Subhead` 一个变体同时接住 `##`/`###`。这次教训：删除一个数据结构（分区）时，要把"这个数据结构本身"和"触发它的手写标记语法"分开看——后者可能有独立于前者的、用户已经养成的书写习惯，不能因为底层字段没了就连带默认干掉。

**真机验证**（经真实 HTTPS 网关认证层）：
- `GET /status` 正确带 `presets`（3/4 条，视觉/文字表分别核对过）+ `activePreset`（这台设备 mind-serve 之前手动改过的 `qwen3-vl-plus` 正确识别为已知预置，不是误判成 `"custom"`）。
- `PUT {preset:"qwen-plus"}` 原子切换 model+baseUrl 验证过，切完再切回 `qwen3-vl-plus`（这台设备的测试 key 只对这个模型开了权限，来回切换验证完整后确认切回工作配置，不留手尾）。
- `PUT {preset:"gpt-4o"}`（不存在的预置名）正确 400 拒绝，配置不变。
- 拿人骨拼圖真实条目库核对回收站该显示的内容：10 条 `revoked` + 1 条 `skipped`，状态分布跟预期一致。
- **前端可视渲染仍未经人眼确认**——没有浏览器渲染工具，只验证到 `strings` 确认新元素 id（`ntrashlist`/`mountModelPanel`/`entry-crop`/`trash-item` 等）已编译进部署的 `shelf-gateway` 二进制、以及上面这些后端数据契约全部走通。跟浏览页/模型面板（§03o/§03q）是同一类已知缺口，等用户自己打开网页看一眼。

**离线**：`cargo test --workspace` **100 个**（`transcribe-serve` 9→12、`mind-serve` 10→13，`marker` 测试改名不增减，其余不变）零警告；`cargo build --workspace`/`--target aarch64-unknown-linux-musl` 零警告（notes + shelf 两个 workspace）；`node --check app.js` 通过。

## 03u｜「整理」区第二轮反馈：批量勾选、模型预置横跨四厂商、回收站可恢复（2026-09-08，真机验证通过）

用户拿真机继续用了一轮 §03t 的重设计之后，又提了四点，这次是更深一层的业务逻辑改动（不只是文案/布局）：

**① 「转写」折叠层——直接删掉，不再是补说明能解决的**：用户点破了根本问题——「浏览」页已经决定了一条批注要不要转笔记（点「转入笔记」才转 `Pending`），"转入整理就该自动识别转换了"，折叠层里"立即转写待转写条目"/"重试失败"这两个批量按钮就是多余的入口，会让人误以为不点它们转写就不跑。整个折叠块删掉；`auto`（合书自动转写）开关是真正有意义的服务级配置，挪进「管理」tab 的模型面板（`mountModelPanel` 加 `showAuto` 参数）。**下方列表独立重试**：单条卡片原有的"重转"按钮保留，是唯一的失败重试入口，不再需要一个全局按钮。

同一条反馈里还有一句"两个下拉列表去掉，文本规则由 md 符合对标至 rm 笔记符号＝手写识别符号"——条目卡片原来有 `style`（正文/无序/有序/待办）和 `destination`（设备笔记本/Obsidian/都要）两个下拉，都去掉：
- **样式**：`notecore::marker::split_leading_marker` 早就是"行首 `-`/`1.`/`口`/`##`/`### ` → 样式/小节"这套规则（转写草稿写回时用的就是它，见 §03g/`worker.rs`），只是原来只在**转写草稿**这一条路径上生效，用户在网页文本框手动改字走的是另一条不认这套规则的路。新增 `Entry::apply_marked_text`（`notecore::model`），把这套规则也接到 `PATCH text` 上——用户在浏览器里打 `- 查作者`，服务端识别出行首标记、剥掉标记字符、`style` 自动设成 `Bullet`；打 `## 分区名`/`### 分区名` 自动覆盖 `subhead`。跟手写识别是同一套约定，不用另外记一遍"打字该选哪个下拉"。卡片上留一个只读徽章显示当前样式，不能直接改（改文本触发重判）。
- **去处**：三选一（设备笔记本/Obsidian/都要）不是"删掉不管"，问了用户想要什么替代——AskUserQuestion 给了三个方案（紧凑图标循环按钮 / 折进批量工具栏 / 两者都要），用户选了**紧凑图标循环按钮**：条目卡片上一个按钮，点一下在三态间循环，不是 `<select>` 但功能完整保留，手机上比下拉更好点。

「生成笔记本/导出md/重转/不要了应该先勾选多条后操作」：每条卡片头部加一个勾选框，「整理」子视图顶部固定一条选取工具栏（`sticky` 定位，选中才出现），四个按钮：生成笔记本/导出 md（按选中条目所在的章节去重，各章各调一次对应端点——原来挂在每个章头的即时按钮删掉，改成先勾后点；章头留一个「全选本章」快捷链接，单章场景两下点完，跟原来一样快）/重转（只对有裁图的选中条目生效）/不要了（一次确认批量归档，不再逐条弹窗）。**导出的下载链接有个真实踩坑**：批量场景要给用户下载多个章节的 md，如果在 `await` 之后的循环里连续 `window.open()`，第一个之后的弹窗会被浏览器当成"非用户直接触发"拦掉——改成先把选中的章节都导出完（写进设备 vault），再把每章的下载按钮摆出来，用户自己点每个按钮才是真正的用户手势，不会被拦。

**失败的条目标红、按钮文案变「重转失败」**：「整理」子视图渲染前先查一次 `/api/transcribe/status`，按 `book`+`id` 匹配失败清单，命中的条目卡片加 `.entry-failed` 样式（边框偏红）、"重转"按钮变 `.btn-bad`（红色描边）+ 文案变"重转失败"，不用再去翻已经删掉的转写折叠层里的失败清单。

**② 模型管理"彻底重做"——从一家厂商扩到四家，key 按厂商分开存**：用户要求还能选 DeepSeek/豆包/Gemini/ChatGPT，且要有各模型自己的用量/花费。落地前先想清楚一个原来没暴露的架构问题：`TranscribeConfig`/`MindConfig` 原来只有一个全局 `api_key` 槽位——只要还是 DashScope 一家没事，但四家厂商共用一把 key 槽位，切换预置会出现"切到 OpenAI 却把 DashScope 的 key 发过去"这种事，或者每切一次模型就要求用户重新粘贴 key。改法：

- `Preset` 结构体加一个 `provider` 字段（`dashscope`/`openai`/`gemini`/`deepseek`）；`TranscribeConfig`/`MindConfig` 的 `api_key: String` 单字段换成 `keys: BTreeMap<String,String>`（厂商 id → key），`key()`/`key_masked()`/`key_source()` 都按"当前预置所属厂商"去查表，`apply()` 收到 `apiKey`/`clearKey` 也是存/删对应厂商那一格。同厂商换模型（比如 Qwen-Plus 换 Qwen-Max）不用重新粘贴；切到没配过 key 的厂商，界面照常显示"没配 key"，不会借用别家的（含 `DASHSCOPE_API_KEY` 环境变量兜底也限定死只在 `provider=="dashscope"` 时生效，不会被误当成别家的 key）。
- 预置表模型 id/baseUrl 是易变信息，没凭记忆写——过 WebSearch/WebFetch 核实了官方文档（`developers.openai.com/api/docs/models`、`ai.google.dev/gemini-api/docs/openai`、`api-docs.deepseek.com/quick_start/pricing`），核实结果写进了 `config.rs` 模块文档并注明核实日期（2026-09-08），方便以后发现跑不通时先去官方文档核对是不是又改名了，而不是怀疑这段代码有 bug。新增：OpenAI（`gpt-5.6-terra`/`gpt-6-astra` 视觉、加 `gpt-5.6-luna` 文字）、Gemini（`gemini-3.8-flash`，视觉/文字通用）、DeepSeek（视觉表 `deepseek-v4-flash-vision-exp` 实验性、文字表 `deepseek-v4-flash`/`deepseek-v4-pro`）。**豆包（火山方舟）没有收进预置表**——核实下来它的"模型"实际是账号自建的推理接入点 ID（`ep-xxxxxxxx`），不是所有用户通用的固定字符串，硬填一个占位模型名等于在提供一个保真不了的"已知能用"承诺，跟预置表"网页下拉给的都是确定能用的组合"这条设计前提冲突；用户要接豆包走"自定义"，baseUrl 填 `https://ark.cn-beijing.volces.com/api/v3`，model 填自己建的 Endpoint ID。
- 老配置文件（重做前，单一 `model`/`baseUrl`/`apiKey` 三件套）不会因为升级配置形状丢失已保存的 key——新增 `migrate()`：反序列化时把这三个老字段单独接进只读迁移字段（`#[serde(skip_serializing)]`，新格式落盘后自然消失），启动时调一次，按老 `model`+`baseUrl` 匹配预置表（匹配上就设 `preset`，匹配不上落 `custom` 且保留 `customModel`/`customBaseUrl`），有老 key 就按匹配到的厂商（或 `custom`）存进新的 `keys` 表；`main()` 迁移完立即落盘一次，让文件形状跟着更新，不用等下次 PUT。**真机验证**：这台设备的两份真实配置——`transcribe-serve` 原本 `model: qwen3-vl-plus`（在新视觉预置表里）、`mind-serve` 原本也被手动改成了 `qwen3-vl-plus`（**不在**新文字预置表里，因为它是视觉模型）。部署后经真实网关拉 `GET /config`：`transcribe-serve` 正确匹配回 `qwen3-vl-plus` 预置，`mind-serve` 正确落 `custom` 且 `customModel` 是 `qwen3-vl-plus`；两边 `keyMasked` 迁移前后一致（脱敏预览最后 4 位对得上），**真机已保存的 key 没有因为这次重做而丢**。
- 用量按模型分账，不再是一个全局聚合数字：`ledger.rs` 的 `Usage` 从单一计数结构改成 `by_model: BTreeMap<模型key, ModelUsage>`（模型 key 是预置 id，或 `custom:<model名>`），`worker.rs` 记账时传 `cfg.usage_key()`。「花费」没有做官方定价表——第三方 API 定价比模型 id 还易变（区域价/促销价随时变），写死一份价格表反而更容易在用户不知情的情况下把"预估花费"这个数字做错，干脆让用户在模型面板自己填"每 1K token 输入/输出单价"（`prices: BTreeMap<预置id, Price>`，缺省 0＝不计费），`GET /status` 新增 `usageByModel`（预置表全量 + 账本里出现过的自定义模型，每行算好 `costEstimate`，没填单价是 `null` 不是 `0`，界面上跟"填了 0 元"区分开）。管理台模型面板加一张用量/花费小表 + 单价输入框。

**③ 回收站可恢复**：用户追问"只能看？能不能恢复删除（一键/单独）？书中已擦除了还能恢复吗？"——三个问题一起回答：
- 新增 `Entry::restore(&mut self, now)`：`Skipped`/`Revoked`/`Archived` 三种终态都能恢复，落点按条目已有内容倒推（不额外存一份"删除前的状态"字段）——`Skipped` 永远回 `Mined`（它就是从 `Mined` 点"不需要"过去的，恢复也回那一步）；`Revoked`/`Archived` 看内容：校对文本还在 → `Reviewed`，没校对但有草稿 → `Draft`，只有手写没转写过 → `Pending`，什么都没留下 → `Mined`。`ink-serve` 新增 `POST /entries/{id}/restore` 端点，非终态条目调用直接拒绝。
- "书中已擦除了还能恢复吗"——**能，但要讲清楚恢复的是什么**：条目库存的裁图（`crops/*.png`，自渲染后是独立文件，不依赖设备实时状态）、勾画原文（`Quote`，摄取时抄进来的静态文本）、转写草稿、校对文本，这些数据在条目变成 `Revoked`/`Archived`/`Skipped` 的那一刻起就已经是条目库里的静态存档，跟设备当前页面上是否还有对应笔迹完全脱钩——`Revoked` 本来就是"ink-serve 扫到笔画消失了"触发的，不是物理删除数据。所以恢复找回的是"这条已经存下来的内容"，不是"让笔迹重新出现在设备页面上"——网页文案里明确写了这条限制（"恢复只找回已保存的内容，不会让笔迹重新出现在原页面"），`Revoked` 条目在回收站列表里单独多一行提示。
- 网页每条一个「恢复」按钮（回收站视图），加一个「全部恢复」批量按钮（跟「清空回收站」并排，分别对应"要回来"和"真删掉"两种反方向操作）。

**真机验证**（经真实 HTTPS 网关认证层，非 loopback 旁路）：
- 两份真实配置文件的迁移见上（②）——**这是本轮最关键的一条验证**：升级配置数据结构没有让真机已经保存的 key 消失。
- 对人骨拼圖真实条目库里一条 `status:revoked`（有草稿无校对文本）的条目调 `restore`，正确落 `Draft`（跟离线单测的"有草稿→Draft"分支对上）。
- `PATCH {text:"- 测试行首标记自动判样式"}` 验证：`style` 自动变 `bullet`，正文剥掉了 `- ` 前缀；`PATCH {text:"## 真机验证分区标记"}` 验证 `subhead` 被覆盖、样式不受影响（没有新样式标记，旧样式保留）。
- `PATCH {destination:"obsidian"}` 循环验证通过，测试完手动切回 `both` 不留手尾。
- `PUT {price:{input:0.01,output:0.03}}` 验证 `GET /status` 的 `usageByModel` 里对应模型行 `price` 字段正确更新，测试完清零复原。
- 部署后 `ink-serve`/`transcribe-serve`/`mind-serve`/`note-serve`/`shelf-gateway` 五个服务 `systemctl is-active` 全 `active`、`NRestarts` 全 0；从设备实际吐出的首页 HTML 里确认新前端代码（`npickbar`/`DEST_ICON`/`usageByModel`/`entry-failed` 等标识符）已经在served 页面里。
- **仍未闭环**：只有 DashScope 一把真实 key，OpenAI/Gemini/DeepSeek 三家新预置的实际调用没有真机跑通过（只验证了预置表/key 隔离/迁移这些不需要真实调用就能测的部分）；等有对应 key 了再补真机调用验证。**前端可视渲染仍未经人眼确认**——没有浏览器渲染工具，跟 §03o/§03q/§03t 是同一类持续存在的已知缺口。

**离线**：`cargo test --workspace` **112 个**（`notecore` 40→42、`ink-serve` 不变 10、`transcribe-serve` 12→18、`mind-serve` 13→17、`note-serve` 不变 13）零警告；`cargo build --workspace`/`--target aarch64-unknown-linux-musl` 零警告（notes + shelf 两个 workspace）；`node --check app.js` 通过。

## 03v｜§03u 真机上手立刻反馈两个 bug + 模型两级下拉（2026-09-08，纯前端修复）

用户部署 §03u 之后立刻真机试，报了两个问题：

**① 编辑区改字不落盘**：根因是竞态，不是没发 PATCH——`textarea` 的 `onchange`（失焦才存）发出保存请求是异步的；用户改完字通常紧接着点旁边的按钮（去处循环/重转/问AI 提问），点按钮本身会让文本框先失焦（触发保存请求出发），但按钮自己的收尾动作（`book=await j(...);renderBook()`）几乎同时也在跑——如果保存请求的响应还没回来，按钮收尾这次的整页重画读到的是服务端的旧文本，把编辑"盖"回去了，看起来就是"改了不存"。修法：加一个 `pendingText`（entry id → 最新未确认值，`textarea.oninput` 记，`onchange` 确认后清）+ `flushPendingText()`，所有会重新拉数据整页重画的动作（去处循环、单条/批量重转、问AI、批量归档/恢复/生成/导出、浏览态转入/跳过、清空回收站、重扫、切书）之前统一先冲一遍，保证重画时读到的一定是最新值。

**② 点重转列表区闪一下**：根因是 `renderBook()` 的清空时机——原来是"先 `chaps.innerHTML=''` 清空 → 再 `await` 查一次转写失败清单 → 才重新拼卡片"，网络这段时间列表是空的，肉眼看就是"列表出来一下又消失"。改成清空挪到 `await` 之后、紧挨着同步重建那一步，中间不再留空档。

**顺手做的一件事（不是这两个 bug，是同一批一起改的）**：模型面板的下拉从"一个框塞七八个跨厂商模型"改成两级——先选厂家（DashScope/OpenAI/Gemini/DeepSeek/自定义），第二级只列该厂家自己的模型；选厂家立即原子切换到该厂家第一个模型（不用二次确认），选"自定义"才露出手填框。「管理」tab 的模型管理卡片从并排两栏改成上下堆叠占满宽度——这轮给每张卡片加了用量表/单价输入之后内容明显变多，`min-width:18em` 两卡硬挤一排在正常屏宽下就是局促；堆叠后两张卡片结构、宽度完全对齐。

**真机验证**：部署后 5 个服务 active、`NRestarts` 全 0；从设备吐出的首页 HTML 里确认新标识符（`pendingText`/`flushPendingText`/`data-vendor`/`PROVIDER_NAMES`/`modelbox`）已经在served 页面里；`text` PATCH 服务端行为本身没变（这轮是纯前端时序修复），重跑一次真机 PATCH 确认没有回归。**这三处改动的实际视觉/交互效果依旧没有人眼确认过**——异步竞态和列表闪烁这类问题本质上是"浏览器里发生的事"，没有浏览器渲染工具没法从我这边确认修复是否真的解决了用户看到的现象，只能确认代码逻辑上时序对了；下一轮真机试用是唯一能确认这两个 bug 真的消失了的办法。

**离线**：纯前端改动，Rust 契约不变，`cargo test --workspace`（shelf 109+15）零回归；`node --check app.js` 通过。

## 03w｜点重转/提问弹出状态+消耗 + 真机顺带挖出的 provider 隔离 bug（2026-09-08，真机验证通过）

用户反馈"点击重转应该弹出状态及当前消耗"。

**弹出状态+消耗**：`transcribe-serve::worker::transcribe_entry` 原来只返回转写文本，token 数只喂进账本记账就丢了，调用方拿不到；改成返回 `Transcribed{prompt_tokens,completion_tokens}`，`RunReport` 新增同名两个字段由 `run_once` 累加（强制单条时就是那一次调用的实际消耗，批量跑一轮是整轮累计），单条转写端点响应体带上这两个字段（`/run` 批量端点本来就回传整个 `RunReport`，自动带上）。`mind-serve::worker::ask_entry` 同款改法（`Answered` 结构体）。前端：「重转」「提问」按钮点击后原地弹出状态文字（转写中…/提问中… → ✓ 完成·这次 token 入X出Y，或 ✗ 失败原因），**停留 1.5s 才触发整页重画**——这是关键：如果状态文字设完立刻重画，紧跟着的 `renderBook()` 会把这行状态连同它所在的 DOM 一起冲掉，用户根本来不及看见，等于白显示；成功/失败两种结果都要等，不能只等成功那支。批量重转工具栏的汇总消息也加上本次累计消耗。

**真机测试顺带挖出的 bug**（不是这次要修的功能本身，是验证 mind-serve「提问」时踩到的）：`mind-serve` 预置表是纯文字模型，没有收视觉模型 `qwen3-vl-plus`；老配置迁移时这个型号精确匹配不上任何预置，`migrate()` 判成"没匹配上"，key 存进字面意义的 `custom` 格。用户后来在网页把预置切到同厂商的真实预置 `qwen-plus`（DashScope），新预置查 key 时去的是 `dashscope` 格——找不到，因为 key 其实躺在 `custom` 格，`/ask` 报"未配置 API key"，尽管这是同一个 DashScope 账号的同一把 key。根因是设计上把"key 该归哪个厂商"跟"这个具体型号在不在我们的预置表里"绑得太紧——一把厂商 key 对该厂商旗下所有模型都该通用，不该因为某个具体型号没被收进某条服务的预置表就找不到。修法：新增 `provider_for_base_url()`（比对四个已有的 base_url 常量），`provider()` 在"当前预置精确匹配"落空时用它按 baseUrl 兜底认厂商；`migrate()` 直接复用 `self.provider()` 而不是自己重复一遍匹配逻辑，保证存 key 和查 key 永远是同一套判断——这样不管选的是预置表里的型号还是"自定义"填的同一个 baseUrl，都能找到同一把 key。`transcribe-serve` 目前没有触发过这个坑的真实案例（它的预置表恰好收了老配置那个型号），但同款兜底一并加上，防患于未然。

**真机验证**（经真实 HTTPS 网关认证层）：
- 用 `PUT apiKey` 把这台设备被切乱的 key 重新点回正确的 `dashscope` 格（`hasKey`/`keyMasked` 确认落位），再切回 `custom`（baseUrl 是同一个 DashScope 地址）——`hasKey` 依旧为真，验证了 baseUrl 兜底确实在起作用（部署新二进制前重试一次，`provider` 字段还是 `"custom"`，证实是二进制没更新，重新部署后再测 `provider` 正确变成 `"dashscope"`）。
- `POST /ask` 真实调用成功，拿到切题的回答，`promptTokens:111 completionTokens:75`——不是缓存/占位数字。
- `POST /entries/{id}` 强制单条转写：`promptTokens:316 completionTokens:13`，同样是真实 DashScope 调用的返回值。
- 部署后 5 个服务 `active`、`NRestarts` 全 0。

**离线**：`transcribe-serve` 18→19（新增 `forced_single_entry_run_reports_only_that_calls_tokens`，既有测试补 token 断言）；`mind-serve` 新增 `migrate_unmatched_model_but_known_provider_base_url_shares_key_with_real_presets_of_that_provider` 复现并锁定这个 provider 隔离 bug 的回归；`cargo test --workspace`（notes）**114 个**零警告；两个 workspace aarch64-musl 交叉编译零警告；`node --check app.js` 通过。

## 03x｜生成/导出去重复 + 同步后自动收起 + 回收站显示去处（2026-09-08，真机验证通过）

用户三点反馈：

**① "全选后生成笔记本/导出 md 是不是重复了"**——是。这两个操作的后端实现本来就是"整章一起投影"（`notecore::project::project_chapter`/`notecore::export::export_chapter_md` 只按章序号取该章全部活条目，压根不看调用方到底选中了哪几条），第二轮反馈把它们塞进批量勾选工具栏之后，"先勾选再点"变成了纯粹的绕远路——选中子集对最终生成的内容没有任何过滤效果，"全选本章"这个快捷方式实际上就是在做"点一次等于点原来那个按钮"这件事。改法：两个按钮挪回章头，直接点、不需要先勾选；批量勾选工具栏收窄成只留重转（对选中里有裁图的条目生效）和不要了（批量归档）——这两个才是真正逐条起作用、勾选有意义的操作。

**② "生成完成后是不是应该移出列表"**——加了一整套同步状态追踪，这是本轮最大的一块：
- `notecore::export` 新增 `fingerprint_chapter`，跟 `notecore::project::fingerprint_chapter` 算法完全一致（标题+各条目 id/样式/文本/勾画/回答拼起来算 FNV1A），唯一区别是这边的 `live_entries` 收 `wants_obsidian()` 不是 `wants_notebook()`——两条投影路径各看各的活条目集合，指纹也必须分开算（同一条目改了去处，两边"算不算变了"的答案可能不一样）。
- `note-serve` 新增 `export_state.rs`（`ExportState`/`ExportRecord{fingerprint, exported_at}`），结构和用途照抄现成的 `notebooks.rs`（`NotebookState`）——落设备笔记本那条投影路径本来就有"指纹没变就跳过重传"的纪律，导出 md 这边原来是无条件全量重写，这次也补上同一套纪律，顺带记账。`export_chapter` 现在返回三态结果（`Written`/`Unchanged`/`Empty`，跟 `publish::ChapterOutcome` 同构），内容变空时 `state.clear()` 掉旧记录（不然「整理」页会一直显示"已同步"，跟"这章已经没内容了"的事实对不上）。
- `note-serve` 新增 `GET /books/{uuid}/sync`：对全书每一章算一次"当前活条目的指纹" vs "上次成功生成/导出时记的指纹"，输出 `notebookNeeded`/`notebookSynced`/`obsidianNeeded`/`obsidianSynced`（`xxxNeeded` 区分"这章没有条目要投这个去处"和"投过了且还同步着"这两种不同含义的"true"，前端要用这个决定该不该显示对应的 📓/Obsidian 徽章）。
- 前端：一次 `refreshSync()` 取整本书的同步状态，「整理」章头显示同步徽章、全同步的章节默认从列表里收起（改字/新条目会让指纹变，下次重画自然又出现——不是"生成过一次就永久隐藏"那种一次性标记），配一个「显示已同步的章节」开关随时翻出来看。所有会改动条目内容/去处/状态的动作（改字、去处循环、重转、问AI、批量重转/归档、触发/恢复）后面都补了 `refreshSync()`，保证徽章不会显著滞后于实际状态。

**③ "回收站里是不是应该显示此笔记导出到哪里"**——每条显示去处徽章（`DEST_ICON`，跟「整理」区已有的去处循环按钮同一套图标，用户提醒"下方列表里已有生成目的地"，不用另起一套视觉语言）+ 所在章节当前的同步徽章（复用同一个 `syncMap`/`syncBadges()`）。这里有意如实标注了一个局限：章节维度的同步徽章反映的是"这一章目前大致是什么状态"，不是"这一条当初有没有被打进上次那次生成"——条目一旦归档/撤销就已经不在活条目集合里，架构上没法逆推它历史上是否被包含在某次具体的生成结果里，只能给一个诚实的、章节粒度的参考信息。

**顺手**：Obsidian 相关图标从占位用的 🔗 换成一个简化多面体 SVG（真机反馈"能否用它自己的图标"）——不是精确描摹官方 logo 的贝塞尔路径（商标图形不该随手照抄），是一个视觉上明显是"紫色多面体/宝石"的简化形状，配色贴近 Obsidian 品牌色，够让人一眼认出"这是 Obsidian"、不构成对其图形商标的精确复制。

**真机验证**（经真实 HTTPS 网关认证层）：
- `GET /sync` 对人骨拼圖真实条目库：章节 1 首次查 `notebookSynced`/`obsidianSynced` 均为假；`POST chapters/1/generate`+`export` 后重查两个都变真；`POST chapters/1/export` 第二次调用（内容没变）响应 `status:"unchanged"`——确认没有白重写。
- 部署后 5 个服务 `active`、`NRestarts` 全 0；served 页面确认新前端标识符已生效。
- **一个真机验证没能干净复现的点，如实记录**：想在真机上单独验证"改一条条目的 `destination` 之后，对应指纹立刻变"，但人骨拼圖这本书被本轮反复用来测试，`transcribe-serve` 的 `auto` 常开，测试期间在没有直接改动的情况下也观察到指纹漂移（疑似别的条目被后台自动转写流程摸过），"单变量"验证条件不干净。这条结论已经有离线单测（`notecore::export::tests::fingerprint_changes_on_content_change_and_is_scoped_to_obsidian_destined_entries`，全控字段）覆盖过，真机上没有单独把这一步复现出来。

**离线**：`notecore` 42→43（新增 `export::fingerprint_chapter` 及其测试）、`note-serve` 13→16（`export_state.rs` 新模块 1 测 + `export.rs` 补 2 测）；`cargo test --workspace`（notes）**118 个**零警告；两个 workspace aarch64-musl 交叉编译零警告；`node --check app.js` 通过。

## 03y｜生成笔记本/导出 md 合并成「同步本章」一个按钮（2026-09-08，真机验证通过，纯前端）

§03x 刚把这两个按钮从批量勾选工具栏挪回章头，用户接着指出更根本的一层：这两个按钮本身就跟条目已有的「去处」字段（`Entry.destination`）重复——去处早就决定了这一章该不该落设备笔记本、该不该落 Obsidian，分两个按钮让用户自己再选一遍"点哪个"是多余的一步，"一个『导出』按钮就解决了"。

改法很轻：不动后端接口，`generate`/`export` 两个端点本来就各自基于 `destination` 过滤"这一章有没有条目要这个去处"（没有就是 `Empty`/没有可导出内容，这是两个端点早就有的正常返回，不是这次新加的）。前端把两个按钮合并成一个「同步本章」，点击依次调用 generate + export，按两边各自的 outcome 拼一条结果消息（"✓ 笔记本已更新 · ✓ md 已导出"，谁没变化谁不提，两边都没变化就是"跟当前去处对应的内容都已经同步，没有变化"），md 写出后照旧触发浏览器下载。

**真机验证**：部署后 5 个服务 `active`、`NRestarts` 全 0；served 页面确认新按钮标识符（`同步本章`/`data-sync`）已生效。纯前端改动，服务端契约完全没变，不需要新的后端真机验证。

**顺手回答了两个用户当场追问的关于"数据会不会因为这次改动出问题"的问题**（都不是代码改动，是把已有设计讲清楚）：
1. "已同步的章节不删除的话，会不会数据无限膨胀"——不会：隐藏已同步章节是纯前端显示过滤，不碰任何存储；`NotebookState`/`ExportState` 这两份新的同步记账文件是按"章"存一条记录、`set()` 覆盖不追加，大小恒定跟着书的章数走，不会随点击次数增长。
2. "书被删了笔记会不会也被删"——不会：`ink-serve::ingest::revoke_stale()` 检测到源书被删/进回收站时，只是把条目状态标成 `Revoked`（跟"笔画被撤回"同一个软删终态），全项目搜过一遍确认没有任何代码路径会因为源书消失就物理删除条目库数据或已经生成/导出的文档——已生成的设备笔记本、已导出的 Obsidian md 都是独立文件，不受源书删除影响；`Revoked` 条目本身也能在回收站用这轮新增的「恢复」按钮找回。

**离线**：纯前端改动，Rust 契约不变，`cargo test --workspace`（shelf 109+15）零回归；`node --check app.js` 通过。

## 03z｜章节默认折叠：长列表不用划到底（2026-09-08，纯前端，真机验证通过）

§03x 的"全同步章节自动收起"只解决了"已经处理完的章节别老占地方"，用户接着把问题问得更具体："1 章 10 条笔记，10 章就 100 条，手机划几分钟才到底"——**还没处理完**的那几章，每章条目一多，列表照样长得没边，跟"同不同步"无关，是纯粹的信息密度问题。

改法：每章卡片默认只露头（标题/条数/同步徽章/「同步本章」按钮/「全选本章」），条目本体（裁图+文本+问AI，一张卡片里最占屏幕的那部分）折叠在下面，点章头展开/收起——只有一章要显示时直接展开，不用多点一次。展开状态记在一个 `expandedChapters` Set 里，点击折叠/展开只是本地 DOM 隐藏/显示（不重新拉数据、不整页重画），响应快；切换书才清空这个 Set。这样列表长度只取决于"这本书有几章要处理"，不再取决于"每章写了多少条"——10 章就是 10 行标题，想看哪章展开哪章。

**顺手回答了两个用户当场追问、都不涉及代码改动的问题**：
1. "显示已同步的章节，笔记一多会不会太长"——不会，这个列表只属于当前选中的这一本书、且只列实际写过批注的章节（没批注的章节压根不进 `groups` 分组），上限是"这本书有多少章你动过笔"，不是"笔记做了多少年积累了多少"——是结构性的一次性的量，不是跨时间累积的量，跟回收站那种"不清空就一直涨"的增长模式不是一回事。
2. 针对"1 章 10 条，10 章 100 条"这个更具体的场景，才引出这次真正的修复（见上）。

**真机验证**：部署后新标识符（`expandedChapters`/`data-chaphead`/`data-caret`）已在 served 页面生效，5 个服务 `active`、`NRestarts` 全 0。

**离线**：纯前端改动，Rust 契约不变，`cargo test --workspace`（shelf 109+15）零回归；`node --check app.js` 通过。

## 04｜踩坑

- **外部进程直改 `.metadata` `parent="trash"` 会被运行中 xochitl 覆写**（阅读线判死）；软删/建夹只能走 QML 代理（书架 `shelf-trash-agent.qmd`）。
- **真机样本页可能全是墓碑**：《人骨拼圖》c65fa2ae 页 23 笔全被擦过，`Page` 剔除后零条目——采样前先 `rmv6` 解析看非墓碑数，别拿它标阈值。
- **ureq 2 默认特性没有 `json`**：`Response::into_json` 不存在，用 `serde_json::from_reader(resp.into_reader())`；TLS 根用 webpki-roots（锁文件已有），出网设备直连不经 host 代理。
- **设备可能只在 WiFi 上**：USB 网卡没起来时 `10.11.99.1` 不通、mDNS 也没有；局域网扫 `8778` 找到 IP 后 `deploy.sh <ip>`（host 参数）。
- busybox：`head -1` 不认（要 `-n 1`）、无 `timeout`、`ls` 中文名显示 `?`（验名 `find | hexdump -C`）；`grep -c` 零匹配退出码 1 会断 `&&` 链。
- 书架规则沿用：**别跑 `cargo fmt --all`**（仓库非 rustfmt 风格，2026-09-06 混进 64 文件重排回滚重放）；qmd 先离线 `qmldiff apply-diffs`；重启 xochitl 只用 `xovi/start`；shell 里 cwd 会在 `cang-jie/`、`shelf/`、`notes/` 间漂移，路径写绝对。
- 会话里 python 改文件时留了个尾逗号把表达式变 tuple、测试文件括号未闭合各踩一次——改完立刻 `cargo test`/`node --check`。
- **给模型字段从必有改成可选（这次是 `Entry.ink` 因为纯勾画条目变成 `Option`）时，光改生成端不够**：前端排序/比较这类代码容易图省事裸访问（`a.ink.bbox[1]`），新字段一变可选就崩，而且崩得没有报错提示（渲染函数先清空容器再崩，用户看到的只是"空白"）——`grep` 一遍字段名的全部引用点，比事后靠用户反馈"哪里空白了"定位可靠得多，见 §03p。
- **"排除法"过滤条件（`!= X`）比"允许列表"（`matches!(status, A|B|C)`）更容易悄悄纳入不该要的状态**：`project.rs`/`export.rs` 的 `live_entries` 一开始写的是 `status != Revoked`，加新状态（`Mined`/`Skipped`）时很容易忘记它们也该被排除——排除法只挡住了写判据那一刻脑子里想到的那一个状态，新增状态默认"漏网通过"；允许列表反过来，新增状态默认"漏网被挡"，出错方向更安全。这个 bug 从二期"浏览态"状态机落地（2026-09-07，引入 `Mined`/`Skipped`）起就存在，离线单测测不出来是因为原来的测试从没写过"混进 Mined/Skipped 会怎样"这种反向断言，真机验证三期新功能（导出 md）时才亲眼看见了才发现，见 §03r。同一批状态判据有多处时（这里是两条投影 + 一个网页视图，三处独立维护），优先把其中最先写对的那处（这次是「整理」网页视图的 `['pending','draft','reviewed'].includes(...)`）当基准去对齐其余几处，而不是假设"先写的两条投影是对的、后写的网页视图跟它们对齐"。

## 05｜真机待办（2026-09-07 刷新）

**未闭环（按依赖顺序）**：
1. **转写质量两项**：① ~~entry3（高亮 3 旁 `口 第三段`）重转后仍没读对，裁图上下混进了相邻印刷行~~ 裁图已换自渲染（§03p），结构上不会再混印刷体了（自己画的图背景纯白），这条旧样本没有专门回归复验，但机制上已经解决；② ~~entry1/2 把手写的汉字数字"一/二"认成阿拉伯数字"1/2"~~ 提示词已加规则（2026-09-07），待真机重转复验是否真的不再转数字。entry4 那条旧的错误草稿是裁图坐标错时期的遗留数据，手机网页上人工清一下（一改字段就覆盖）。
2. ~~NumberedList 补样本~~ ✅ 用户真机核对时一并补了：那 7 字节根本不属于 NumberedList，是分析失误，已更正、已解除限制（§03i）。
3. ~~rmdoc 打包器 + 真机小范围验证~~ ✅ 2026-09-07 当场做完两轮：全部 7 种打字样式真机上传+渲染验证通过（§03h/§03i）。
4. ~~note-serve 条目→文档编排~~ ✅ 2026-09-07 当晚三轮真机验证通过：首次生成（渲染全对）、改文本增量重传+旧本自动进回收站（`shelf-trash-agent.qmd` 全自动消费队列，没人手动点）、无变化跳过（见 §03k）。~~剩的是前端接线~~ ✅ 2026-09-08 补上（§03r/§03t）：网页「整理」章头「生成笔记本」按钮已接线，`POST /books/{uuid}/chapters/{idx}/generate` 不再只能 curl/wget 手动触发。
5. ~~书库动作代理扩展~~ ✅ 2026-09-07 真机验证通过：新增独立 `shelf-mkdir-agent.qmd`（锚点 MainView 而非 Sidebar，见 §03l 为什么不是"扩展"而是新文件）+ book-serve `mkdir.rs`，`Library.createCollection` 建《书名》夹真机确认能建、能落对、不重复。
6. ~~`## `/`### ` 分区/小节标记真机验证~~ **`##` 半已随分区整体砍掉，不再是待办**（三期，见 §03s）；`### 小节名` 覆盖 `subhead` 那半继续有效，但仍然只有离线单测覆盖，没有真机拿真实手写走一遍完整闭环（写 `### 小节名` → 转写 → 核对 `subhead` 被覆盖）——2026-09-07 两轮真机复验撞的是手写行草连笔识别准确率（§03o/§03p），管线本身没问题，找一份写得工整一点的样本再试。
7. ~~二期浏览页 UI + 状态机真机验证~~ ✅ 2026-09-07 真机验证通过（§03o）：`Mined`/`Skipped` 状态机、`request`/`skip` 两个端点、网关"浏览"/"整理"两个子视图全部部署，走了完整闭环（新批注落 `Mined`→不被自动转写→点"转入笔记"转 `Pending`→自动转写成 `Draft`→对 `Revoked` 条目操作被拒绝→"不需要"转 `Skipped` 后确认不再被扫到）。**剩浏览页 UI 本身还没人眼确认过截图**（这几轮验证走的是 SSH 直调 loopback API + 用户口头反馈，没有正式截图核对）。
8. ~~纯勾画（无手写）条目~~ ✅ 2026-09-07 晚真机验证通过（§03p）：`drafts_of_page` 认出未配对的勾画单独生成 `ink:None` 草稿，`Entry::set_triage` 对这类条目"转入笔记"直接落 `text=quote.text`+`Reviewed`；真机确认条目正确落 `Mined`（`ink:null`）、浏览页正确显示引用文字无裁图、转入笔记后直接定稿。
9. ~~裁图自渲染（治本）~~ ✅ 2026-09-07 晚真机验证通过（§03p）：`ink-serve::crop::render_ink` 从 `.rm` 笔画矢量数据自己画折线，不依赖缩略图；真机新写的批注正常转写成功。**没有专门复验"写在页面很靠下的位置"这个原始症状**（架构上已经不可能复现，但没有刻意造一个边界样本去踩一遍确切步骤）。
10. ~~`Section.triggers`~~ **随分区整体删除，不再是待办**（三期，见 §03s）。
11. ~~`mind-serve`（二期重新设计，取代原计划的"按分区批量跑"）~~ ✅ 2026-09-07 晚真机验证通过（§03p）：按条目单发——`ask_ai`+`question` 输入框触发，拼"书名+章节+勾画原文+转写文本+用户问题"发文字模型，写回 `answer`；`Section` 只保留笔记本排版分组用途，不再驱动 AI（网页"调模型"勾选框已删）。真机点验一次踩到 key 权限限制（403，不是代码 bug），换模型后拿到正确回答。~~剩配套~~ ✅ 2026-09-08 完成（§03q）：`transcribe-serve`/`mind-serve` `GET /config` 加了 `keyMasked`（脱敏预览）、网关统一"模型"设置面板（视觉+文字并排、用量卡片）；经真实网关认证代理层验证过数据契约（不是绕过网关的 loopback 旁路），纯视觉排版未经人眼确认。**二期五步全部完成**。
12. ~~md 导出~~ ✅ 2026-09-08 真机验证通过（§03r）：`vault/<书名>/第N章.md`（front-matter、`^id` 块锚、`[[书名]]` 反链、索引页）设备端落盘已完成。**剩 host `notes/host/bin/notes pull`**（拉到本机 Obsidian vault 路径）没做——三期只做了"导出到设备"这一半，"host 拉走"是新的独立待办。
13. 文档收尾、旧 PKM 白皮书加"已退役、由 notes/ 取代"头注、`dev` 以 `--no-ff` 合入 `feature/shelf-p1`。
14. ~~「整理」区第二轮反馈~~ ✅ 2026-09-08 真机验证通过（§03u）：转写折叠层删掉+批量勾选工具栏、模型预置横跨四厂商（key 按厂商分开存、老配置迁移真机验证不丢 key）、回收站可恢复（`Entry::restore`，对真实历史 `revoked` 条目验证过）。**新增两项待办**：① OpenAI/Gemini/DeepSeek 三家新预置只有配置层验证过，没有真实 key 走过实际调用；② `notes/host/bin/notes pull` 仍未做（见第 12 条，跟这轮无关但仍然是同一块空缺）。
15. ~~「整理」区第三轮反馈~~ ✅ 2026-09-08 真机验证通过（§03v/§03w/§03x）：编辑丢失竞态+列表闪烁修复、模型两级下拉、重转/提问弹出消耗（顺带修复 provider baseUrl 隔离 bug）、生成/导出去重复+同步状态追踪+全同步章节自动收起、回收站显示去处/同步徽章、Obsidian 专属图标。**遗留**：destination 改变导出指纹这一步只有离线单测干净覆盖，没有在真机上单独复现（活跃测试书并发状态漂移导致单变量实验条件不干净，见 §03x）；`notebooks.rs` 的 `ChapterRecord` 在章节内容变空时不会像 `export_state.rs` 那样自动清掉旧记录，会一直显示"曾经生成过"，是这次顺手发现但没有一并修的小不一致（影响很小：只是回收站/整理页的同步徽章在这个边界情况下会显示"未同步"而不是"没什么要同步的"，不影响任何数据正确性）。
16. ~~「整理」区第四轮反馈~~ ✅ 2026-09-08 真机验证通过（§03y）：生成笔记本/导出 md 合并成一个「同步本章」按钮（去处字段本来就决定了该做哪样，不该分两个按钮让用户自己再选一遍），纯前端改动、服务端契约不变。顺手书面回答了两个用户当场追问但不需要改代码的问题：已同步章节隐藏不影响存储（纯显示过滤）、`NotebookState`/`ExportState` 按章记账不随点击次数膨胀；书被删不会连带删笔记（`revoke_stale()` 只改状态不删数据，已生成/导出的独立文件不受影响）。回收站列表长期使用后会不会太长——用户明确要求暂不加收纳（折叠/按时间过滤），先不做。
17. ~~「整理」区第五轮反馈~~ ✅ 2026-09-08 真机验证通过（§03z）：章节默认折叠——用户把"列表会不会太长"的问题问得更具体（"1章10条，10章就100条，手机划几分钟才到底"），揪出"已同步章节自动收起"没解决的另一半（还没处理完的章节，条目一多照样长），改成每章默认只露头、点了才展开条目本体，纯前端改动。

**已闭环（真机）**：§03c ink-serve 首轮（active/注册/追平 38 章）· §03d 「笔记」tab 注册 · §03e transcribe-serve 部署（active/注册/0600/追平记 note）· §03f 步骤 0 样本标定（聚簇/配对阈值验证通过）· §03g 揪出裁图画布尺寸错（960×1280）并修复部署复验（3 条有勾画的条目裁图都对准了手写位置，但转写准确率另计——2 条数字被认错、1 条完全读错；1 条裁不到已优雅降级）· §03h `rmv6::write`/`note-serve::rmdoc`/上传三件套首次真机验证通过 · §03i 更正 NUMBERED 误判、解出 Subheading 1/2 区分开关、二次真机验证全部 7 种打字样式渲染正确 · §03j/§03k note-serve 生成编排离线写完当晚三轮真机验证通过（生成/增量重传+旧本自动回收/无变化跳过全绿）· §03l 建夹代理真机验证通过（《书名》文件夹自动创建、新文档正确落进去、不重复建夹）· §03m 修正 `list_active` 真机验证通过（书清空后正确从列表消失）· §03o 二期浏览态状态机 + 浏览页 UI 全套真机验证通过、顺带修复"回收站/删除仍赖在列表里"bug · §03p 纯勾画条目 + 裁图自渲染 + `mind-serve` 全部真机验证通过、顺带修复"浏览页对纯勾画条目排序崩溃"bug · §03q 模型配置统一面板（步骤 5）真机验证通过（经真实网关认证代理层验证数据契约，纯视觉排版未经人眼确认），**二期五步全部完成** · §03r 三期：md 导出 + 落设备笔记本/Obsidian/删除三选一真机验证通过，顺带修复"`Mined`/`Skipped` 混进两条投影"的真机 bug（`live_entries` 判据从排除法改允许列表）+ 导出改直接触发浏览器下载（顺带修了 `shelf-gateway::proxy::forward` 丢弃 `Content-Disposition` 头的缺口） · §03s 三期：砍掉分区，条目按页序平铺真机验证通过（含旧数据向后兼容验证）· §03t「整理」区四点反馈：模型预置下拉+搬进管理台、回收站显内容、条目卡片重设计，真机验证数据契约通过（前端可视渲染仍未经人眼确认）· §03u「整理」区第二轮反馈：删转写折叠层+批量勾选工具栏、模型预置横跨四厂商（key 按厂商分开存+老配置迁移不丢 key）、回收站可恢复，真机验证数据契约通过（含对真实历史 `revoked` 条目跑通 `restore`；只有 DashScope 有真实 key 验证过实际调用，前端可视渲染仍未经人眼确认）· §03v「整理」区编辑丢失竞态+列表闪烁修复+模型两级下拉（纯前端）· §03w 点重转/提问弹出状态+消耗，顺带修复真机踩到的 provider baseUrl 隔离 bug（`mind-serve` 老配置 key 落错格、切换同厂商预置后找不到），真实调用（`/ask`+强制单条转写）双双验证通过，非占位数字 · §03x 生成/导出去重复（挪回章头直接按钮）+ 同步状态追踪（`export_state.rs` 补齐导出这条路径"指纹没变跳过"的纪律）+ 全同步章节默认收起 + 回收站显示去处/同步徽章，真机验证生成→导出→同步、二次导出跳过重写；destination 改变指纹这条只有离线单测干净覆盖，真机因活跃测试书的并发状态漂移没能单独复现 · §03y 生成笔记本/导出 md 合并成「同步本章」一个按钮（去处字段本来就决定了该做哪样，纯前端改动、服务端契约不变），真机验证新按钮已生效、服务健康 · §03z 章节默认折叠（"1章10条，10章就100条"，纯前端），真机验证新标识符已生效、服务健康。离线：七 crate+服务 **118** 测（这轮全是前端改动，测试数不变）。

**明确不做（本期）**：扫描件 PDF、定稿 PDF（等步骤 0 ④）、笔记本手写批注回读（设备只读）、颜色语义（只进 tags）、自动清空回收站（网页按钮走 `emptyTrash()` 用户显式点）、Anki/Todoist/Readwise 外发（有 md 与稳定 id 之后再谈）、KOReader 高亮回流（书架砍下来留给笔记线，排在导出之后）。
