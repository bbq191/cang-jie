# reMarkable 笔记线（notes）白皮书

> **读者与用途**：维护笔记线、或想弄清“为什么这样设计、真机验证了什么、踩过什么坑”的人。笔记线是什么、服务与端口、API 先看 [`../README.md`](../README.md)；整个系统的位置见 [`../../docs/OVERVIEW.md`](../../docs/OVERVIEW.md)。
>
> 本文记“怎么决定、真机怎么验、踩了什么坑”。**读现状先看下面的“5 分钟读懂”和 §00b；待办 / 已闭环 / 明确不做看 §05；踩坑看 §04。** §03b 之后的各节是按时间排的历史记录：**被后续结论推翻的节已压缩为“结论 + 被哪一节取代 + 关键教训”**，节内的 ~~删除线~~ 表示已过时的旧结论。
> 笔记线**挂在书架网关下**：网关 / 注册表 / 事件汇聚 / 部署链 / 原生回收站代理都是书架与网关的（[`../../gateway/README.md`](../../gateway/README.md)、`shelf/docs/reMarkable书架白皮书.md`），本文只记笔记线自己的决策、服务、真机轮次。

## 5 分钟读懂

**一句话**：reMarkable 的强项是“荧光笔勾书 + 在勾出来的内容旁边直接手写”。笔记线把这些勾画和手写**自动收进一个条目库**，你在手机网页上挑、改、问 AI，最后推送成设备上的笔记本或 Obsidian 的 md。**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都只是它的“投影”。**

![笔记线 5 分钟总览](diagrams/overview.svg)

**四个服务，各管一摊**（都挂在网关下，只听 `127.0.0.1`）

| 服务 | 端口 | 一句话 | 出网？ |
|---|---|---|---|
| ink-serve 矿 | 8795 | 合上书后解析书页 `.rm`，把“勾画 + 旁边手写”配成条目，唯一写条目库 | 否（除手动的 KOReader 回流） |
| transcribe-serve 转写 | 8796 | 把手写裁图喂视觉模型，草稿写回条目 | 是 |
| mind-serve 脑 | 8797 | 按条目单发“问 AI”，纯被动 | 是 |
| note-serve 本 | 8798 | 把条目库投影成设备笔记本 / Obsidian md，管导入 md | 否（只碰 xochitl `/upload`） |

**术语速查**

| 词 | 含义 |
|---|---|
| `.rm` | xochitl 存书页笔迹的文件；本线读 v6 格式 |
| 勾画 / GlyphRange | 荧光笔在书上圈出的原文（自带精确文字与矩形） |
| 条目（Entry） | 一处“勾画 ± 手写”，条目库里的最小单位，有 7 种状态（见 §01 图） |
| 投影 | 从条目库重新生成的只读产物：设备笔记本（一章一本）、Obsidian md（一章一文件） |
| 去处（destination） | 每条内容去哪：设备笔记本 / Obsidian / 两处（缺省两处） |
| 预置（preset） | 模型下拉里一项“厂商 + 模型 + baseUrl”，选一项即原子切换 |

**你最常会读到的三个事实**：① 条目状态是 `Mined→Pending→Draft→Reviewed` 的主线，另有 `Skipped/Revoked/Archived` 三个可恢复的“回收站”终态；② 「整理」页是「未导出 / 已导出」两个 tab，每个 tab 下按章节点选，只显示一章（§03ad）；③ 模型分四家厂商、key 按厂商分存（§03u/§03w）。

**已经不在了的东西**（别当现役）：“分区”概念（三期砍，§03s）· 自动建《书名》文件夹（§03ae）· host `shelf notes pull`（2026-09-18 随 host CLI 砍，§03ak）· 批量勾选工具栏（§03ad）。

## 00｜定位与原则（2026-09-06 用户定）

书架（`shelf/`）补 reMarkable 读书短板；笔记线做强它的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。文学书勾作者名写“查作者”，学术书勾公式写“?/没听懂”或“!/背诵”——凡是勾了、写了的都该汇入笔记。旧 `knowledge/pkm`（画星待办、六色槽卡片、跨书汇编、复盘队列、仪表）判“太花哨”（颜色即路由的产物），**全部退役；新建 `notes/`，不引入任何旧代码，只借鉴功能与踩坑**（`.rm` 解析、`.epubindex` 页→章按书架惯例“剥离移植”成新 crate）。

四步闭环（原始设想，③ 后来两次改版——二期改按条目单发问 AI，三期直接砍掉分区，见 §00b/§03s）：① 合上书自动摄取（事件驱动）→ ② **手机上修正**（e-ink 打字太痛苦；不再引入中文化/输入法）→ ③ ~~按分区调智能~~ → ④ 可选导出 md，一章一文件，与设备笔记本同构，反链。

工程原则同书架四条（XDG · 设计模式去重解耦 · 专项专用可插拔 · 不引旧 crate 不对接旧路径），外加笔记线自己的一句话：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影**。用户另定：四个服务（转写与智能分开）；设备笔记本一章一本、只读（~~放《书名》文件夹~~ 现复用书本自己所在的文件夹，§03ae）；~~分区自定义~~（三期已砍）；首期只 EPUB；**后续开发在 feature 分支上进行**（`dev` 已于 2026-09-16 删除，日常在 master 上开分支）。

## 00b｜现状总览（2026-09-22 按当前代码复核；读其余历史节前先看这里）

> **⚠ 2026-09-18 现状更正**：`shelf/host/`（书架线 host CLI）已随用户“不再使用 PC 端”整个砍掉，源码留档 `/home/afu/Projects/oldbak/cang-jie/shelf-host/`（不随仓库走）。§03ak/§03al 里的 **`shelf notes pull`**（拉设备端 md 到本机 Obsidian vault）随之消失、**无替代**；`GET /books/{uuid}/vault.json` 接口本身还在，只是没有自动化调用方，要把笔记拉到本机只能手动 curl。其余关于四个服务/条目库/投影管线的记录都仍准确。详见 `shelf/docs/reMarkable书架白皮书.md` 附录 B。

**架构**：网关 `manage::MODULES` 里四行（`ink`/`transcribe`/`mind`/`notes`）；四个 loopback 服务全部上机。

![notes 架构：条目库唯一写者 = ink-serve](diagrams/architecture.svg)

- **ink-serve 矿 :8795**：书库监听 → 条目库唯一写者；`POST /koreader/import` 另拉书架线 `koreader-serve` 的高亮/生词并入条目库（§03al），除这一条手动出口，其余零网络。
- **transcribe-serve 转写 :8796**：订阅矿的事件 → 裁图喂视觉模型 → 草稿写回（出网）。
- **mind-serve 脑 :8797**：按条目单发问 AI，**无事件订阅/批量循环**，纯被动等 HTTP（出网，§03p）。
- **note-serve 本 :8798**：注册「笔记」tab；写入/打包/上传三件套 + 条目→文档生成编排（§03h–§03k）+ md 导出（§03r）+ 单篇 markdown 独立导入（§03af）。
- 网页只多一个「笔记」tab（前端组合 `/api/ink` `/api/transcribe` `/api/mind` `/api/notes`）；事件区域 `notes`（矿发 `entries`，转写发 `transcribe`）经网关 `Hub` 汇聚到 `/api/events`，页面零轮询（机制见 [网关白皮书 §00b](../../gateway/docs/reMarkable网关白皮书.md)）。
- **零 xovi 依赖**：无 qmd、无 .so——note-serve 不再新建/确保任何设备文件夹，笔记本复用书本自己所在的文件夹（§03ae）。

**数据流**（含浏览态）：

![notes 数据流：四步闭环](diagrams/data-flow.svg)

合上书 → xochitl 重写 `<uuid>.content/.metadata` → ink `fswatch`（书库目录非递归、4 s 防抖）→ 只扫页 `.rm` mtime 变了的页 → `rmv6` 解析（勾画 GlyphRange + 手写笔画，同一坐标系，墓碑剔除）→ `notecore` 并查集聚簇 + 就近配对（含未配对的纯勾画）+ 增量合并 → 落 `Mined`、自渲染裁片（`render_ink`，有手写才有）→ `~/.local/state/notes/books/<uuid>.json` + `~/.local/share/notes/crops/` → 事件 → 网页「浏览」→ 用户点「转入笔记」（`Mined→Pending`，纯勾画直接 `Reviewed`）或「不需要」（→`Skipped`）→ 转 `Pending` 的被 transcribe 防抖 3 s 捡到 → 视觉模型（OpenAI 兼容口）→ `POST ink /books/{uuid}/entries/{id}` 写 `draft` → 手机「整理」改字（改即存；行首 `-`/`1.`/`口`/`##`/`### ` 自动定样式）、勾「问AI」填问题点提问 → mind-serve 发文字模型 → 写回 `answer` → 「推送本章」→ note 投影为书本自己所在文件夹的一章一本 + md 导出。（独立于这条数据流：「导入 md 文档」子视图直接把 `.md` 转一份新笔记本，不经条目库，§03af，默认隐藏，由书架「实验室」的 `notesImportMdEnabled` 开关控制。）

**「整理」页当前形态**（最迭代频繁的一块，§03t–§03ad 共七八轮反馈，以 §03ad 为准）：

![「整理」页：未导出/已导出 + 章节标签 + 条目卡片](diagrams/organize-page.svg)

**模型管理**（在网关「管理」tab，不在笔记专属页）：以 §03v/§03w 为准，结构见 §03u 的图；**代码里的预置表**是现行事实（含 2026-09-22 复核的 DeepSeek 旧名提示，§05 #3）。

**代码落点**

| 位置 | 内容 |
|---|---|
| `crates/rmv6` | `.rm` v6 解析 + 写入（剥离移植 `remarkable_lines` 0.1.3，MIT，`PROVENANCE.md` 留痕）。`page::Page{strokes,highlights,text}`；`write.rs` 编 `RootTextBlock` + 模板替换拼 `.rm`；`v6/crdt.rs` 的 `CrdtId::Display`（`"part1:part2"`）是条目库落盘 id 字符串唯一定义处 |
| `crates/epubmap` | `.epubindex` 起始页（两张表取首现）+ nav/ncx 目录 → 页号→章/小节 |
| `crates/notecore` | 纯函数领域核心：`model`（条目/样式/状态/去处/来源）· `hash` · `geom`（聚簇/配对）· `ingest`（增量合并）· `koreader`（KOReader 摄取，§03al）· `marker`（行首标记）· `project`（条目库→段落投影+指纹）· `export`（→Markdown）· `mdimport`（markdown→段落） |
| `crates/vendorcfg` | 两个 AI 服务共享：预置模型表/key 按厂商分存/迁移/PATCH（`preset`）· 泛型用量账本（`usage`）· `ConfigCell`（`cell`）· OpenAI 兼容传输（`chat`）· `truncate_chars` |
| `services/ink-serve` | `doc`（书库只读视图）· `ingest`（变更页编排）· `crop`（自渲染裁图）· `bookdb`（Repository）· `config` · `koreader` · `search`（全文搜索，§03an）· `main` |
| `services/transcribe-serve` | `config`/`ledger`（`vendorcfg` 薄封装）· `backend`（`Vision` Strategy）· `prompt` · `ink`（`EntryStore` 客户端，包 `rmsvc_core::registry::SvcClient`）· `worker` · `main`（SSE 订阅 + 防抖） |
| `services/mind-serve` | 同上但 `TextModel`、无后台线程 |
| `services/note-serve` | `rmdoc`（打包）· `chapter_store`（泛型“每书每章一条记录”，`notebooks`/`export_state` 是类型别名）· `publish`（`Uploader` Strategy + 生成编排 + `import_markdown`）· `export`（vault 落盘 + `content_disposition()`）· `ink`/`trash`（跨服务客户端）· `config` · `main` |
| 外部 | `shelf/{build,deploy,install,uninstall}.sh` 的 `NOTES_BINS`/令牌 · `gateway/src/manage.rs::MODULES` · `gateway/ui/app.js` 的 `renderNotes`（网页正文中英文切换，`notes.*` i18n 命名空间，见书架白皮书 §03an） |

**离线门槛**：`cargo test --workspace`（notes）约 **213 个**（rmv6 27 · epubmap 5 · notecore 62 · vendorcfg 20 · ink 20 · transcribe 23 · mind 22 · note 34，按 `#[test]` 计数，含 1 个 `#[ignore]`；2026-09-24 复核）零警告；网关 `node --check ui/app.js`；shell 过 shellcheck。数字是滚动值，改测试数后回来同步（与 README 保持一致）。`koreader-serve` 的 annot/vocab/sqlite_min 测试算在书架线的离线门槛里。

**未闭环**：见 §05 “未闭环”表（前端可视渲染人眼确认、转写质量、三家新厂商预置只验配置层、若干被动触发修复未真机复验）。

## 01｜架构决策

> 白话：这一节回答“为什么拆四个服务、为什么条目库只有一个写者、为什么设备只读、二次识别为什么不会覆盖你改好的字”。

- **四个服务而不是一个 `note-serve`**（用户驳回单服务方案）：失败面分离——矿零网络（解析错只影响新条目）、转写/脑出网（断网只是积压）、本只碰输出物（xochitl `/upload`）；**转写与智能分开**是用户明确要求（转写是 OCR 边界问题，智能是提示词问题，节奏与费用都不同）。
- **不自建网关，挂书架现成的**：笔记线是独立目录（`notes/`），但没有自己的 HTTPS/密码/证书/mDNS——那些是重复劳动；也没塞进书架任何现成服务（失败面与依赖方向都不同，硬塞会把两条线的故障域绑在一起）。折中：复用网关/注册表/事件汇聚/部署链这套**机制**（网关那边只在 `manage::MODULES` 加几行），架构决策、服务划分、数据模型这些笔记线自己的设计全在本文档。代价：`MODULES` 成了跨两个目录的单一事实源，加新服务除了自己这边的代码，还得去网关登记一行。
- **条目库唯一写者 = ink-serve**：转写/脑/本一律经它的 HTTP 改字段（`POST /books/{uuid}/entries/{id}`，底座无 PATCH）。多进程各自读改写同一份 JSON 迟早互相覆盖（书架落库边车早期踩过同类）。
- **设备只写不改，改在手机**：xochitl 不认外部对已有文档的原地修改（书架/PKM 两线都判死），且 e-ink 上改字太痛苦。所以设备笔记本**只读**，由条目库投影生成；一章一本使重建局部化（只重建变过的章），旧本走书架回收站代理软删（`selectionMoveToTrash` 是唯一可靠路，直改 metadata 会被运行中 xochitl 覆写）。
- **事件驱动、零轮询**：ink 只在书库目录上挂非递归 inotify（合上书时 xochitl 重写 `.content/.metadata`；页 `.rm` 的写入不监听——文件多且是 xochitl 内部节奏），触发后按页 `.rm` mtime 只扫变更页；转写订阅矿的 `/events`；网页订阅网关 `/api/events`。
- **增量在数据层**（回答“二次识别会不会把改好的‘您好’覆盖回‘你好’”）：簇指纹＝笔画 id 集合 + 点数 + 量化包围盒的 FNV-1a。指纹不变→不重转写、不动 `text`；共享笔画但指纹变（补了几笔）→同一条目、新 `draft` 只作建议；笔画全没→`Revoked` 留痕；条目 id 按（书、页、最小笔画 id）创建时一次算定永不重算。投影永远取 `text ?? draft`。**校对过的字永远不会被覆盖。**
- ~~分区 = {名字, 简述, 是否调模型, 触发词}~~ **三期（2026-09-08）整个概念已砍**（§03s）——AI 触发早是 `Entry.ask_ai`/`question` 的事，排版分组也不要了，条目按页序平铺。
- **样式判定：OCR 为主**：`-`/实心点/`口` 由 OCR 认（`notecore::marker`，只在条目仍为正文时认，并把标记从正文剥掉——笔记本样式自带编号/符号）；`1.` 数字形状不定同样走 OCR。~~下划线分区头由几何认~~（`has_underline` 三期已删，从没真正接线）。
- ~~**裁图来源 = xochitl 现成缩略图**（384×512）~~ **已被 §03p 推翻**：缩略图精度不够（视口外截不到、混进印刷体）真机踩到了，`crop.rs` 现在是**自渲染**（`render_ink`，从笔画矢量数据画折线），对外仍只暴露“给我这片的 PNG”。页坐标→像素按 `page_width/height` 等比、`x_origin_center` 可配，**EPUB 页真机实测 960×1280、x 原点居中**（不是 1404×1872 物理屏，§03g）——这条坐标结论仍成立。
- **视觉后端 Strategy**：`Vision` trait 一个方法；`vendorcfg::chat` 走 `POST {baseUrl}/chat/completions` + `image_url` data URI，DashScope Qwen 缺省（国内直连；设备自己 WiFi 直连，不经 host 代理——host 的 clash fake-ip 会挡设备流量），任何 OpenAI 兼容口只改配置。编排 `worker::run_once` 全 trait 注入，内存桩单测。
- **rmv6 剥离移植而非依赖 device-core**：vendored `remarkable_lines` 0.1.3（MIT）只留 v6，保留两处兼容补丁（未知 PenColor/ParagraphStyle/Tool 码兜底、块尾多余字节跳过），补 CHECKBOX(6/7) 码。notes 不依赖 `bookconv`/`device-core`/`knowledge/pkm`/`reading`。
- **体积/内存**：musl 全静态，早期实测 ink 2.5 MB · transcribe 2.1 MB · note 1.2 MB；每个单元 `MemoryMax=128M`、`CPUWeight=20`、`Nice=5`（`notes/systemd/*.service`）。

**条目状态机**（`notecore::model::Status` 7 态 + `restore()`）：

![notes 条目状态机：7 态 + restore()](diagrams/entry-status.svg)

**手写约定 ↔ xochitl 3.28 七种打字样式**（格式菜单：Title / Subheading 1 / Subheading 2 / Body / Bulletpoint / NumberedList / CheckboxUnchecked；7 种写入能力全部真机验证，§03f/§03h/§03i）：

| 样式 | .rm 码 | 笔记本用途 | 手写约定 | 判法 |
|---|---|---|---|---|
| Title | 2 (HEADING) | 页标题＝章名 | `#`（**不接**，见下） | epubmap |
| Subheading 1 | 3 (BOLD) + 7 字节标记 `21023403000000` | 目前**没有生产者**（原分区头，三期已砍） | — | — |
| Subheading 2 | 3 (BOLD)，不带标记 | 勾画所在小节 | `## 文字` / `### 文字`（≥2 个 `#` 同一件事，不分层级，覆盖 epubmap 自动填的 `subhead`） | OCR（`marker`）／epubmap 缺省 |
| Body | 1 (PLAIN) | 转写正文 / AI 回答 | 普通书写 | OCR |
| Bulletpoint | 4 (BULLET) | 无序 | 行首短横 / 实心点 | OCR（`- `/`• `/`· `/`* `/`—`/`－`） |
| NumberedList | 10（格式子块 2 字节，同其余样式） | 有序 | 行首 `1.` | OCR |
| Checkbox（未勾选） | 6 (CHECKBOX) | 待办 | 行首空心小方框 | OCR（`口`/`□`/`☐`） |
| Checkbox（勾上号） | 7，只能原生点方框切换，写入器造不出 | — | — | — |

**为什么 `#`（Title）不接**：Title 是整章级别（一份生成的笔记本只有一个 Title，来自 epubmap 章名），条目级 `#` 没有现成字段可落；留给将来“纯手写笔记扫描”（会议/上课，没有 EPUB 章节可依附）那条还没立项的线，不为勾书硬造用不上的字段。

**2026-09-07 用户定案更正了两处过期结论**：① 原计划“Subheading 1 靠几何 `has_underline` 判定”——该函数存在且真机验证过（§03f），但**从没被 `ingest_doc`/`merge_page` 调用过**，等于没接线；用户否掉这条手势路线（“没有学习成本”这个标准更重要），改用 Markdown 标题级别的文字标记，复用已有的 OCR 兜底（`notecore::marker`）。② “Bulletpoint/Checkbox 几何优先、OCR 兜底”的说法核对代码后也是错的——`notecore::geom` 只有过 `has_underline` 一个几何标记函数，无序/待办**只有 OCR 路径**。`##`/`###` 落哪个字段的最终定稿是 §03t：都覆盖 `Entry.subhead`，不分层级（`##` 原是“找/建 `Section`”，三期删分区时被连带误删，用户当场纠正）。`Section.triggers` 死代码随分区整体删除，不再是待办。

## 02｜XDG 路径表（设备 HOME=/home/root，`Paths::app_{config,data,state}_dir("notes")`）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}`（随书架 `install.sh`，令牌 `ink`/`transcribe`/`mind`/`note`） |
| 配置 | `~/.config/notes/ink.json`（`clusterGap`/`pairGap`/页几何 `pageWidth`/`pageHeight`/`xOriginCenter`/`cropMargin`/`debounceSecs`；首启写出缺省）· `~/.config/notes/transcribe.json`（**0600**；`preset`（预置 id，四厂商共表）+ `keys`（厂商→key）+ `prices`（预置→用户自填单价）+ `customModel`/`customBaseUrl`（“自定义”分支）+ `backend`/`timeoutSecs`/`maxPerRun`/`pauseMs`/`auto`/`maxAttempts`/`prompt`；老配置的 `apiKey`/`model`/`baseUrl` 三件套只作反序列化兼容字段，启动时 `migrate()` 搬进新形状后不再落盘，§03u）· `~/.config/notes/mind.json`（**0600**，同上但没有 maxPerRun/pauseMs/auto/maxAttempts 节流字段） |
| 数据 | `~/.local/share/notes/crops/`（手写裁片 PNG）· `~/.local/share/notes/vault/`（md 导出，§03r） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**，一书一文件，原子写）· `~/.local/state/notes/notebooks/<uuid>.json`、`…/exports/<uuid>.json`（note-serve 的每章生成/导出簿记，`ChapterStore<T>`）· `~/.local/state/notes/transcribe.json`/`mind.json`（用量账本：次数/token/最近错误/上轮报告，不存内容） |
| 运行时 | 与书架共用注册表 `$XDG_RUNTIME_DIR/shelf/services/`（缺省回落 `/tmp/shelf-0/shelf/services`） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`（`<uuid>.{metadata,content,epub,epubindex}`、`<uuid>/<page>.rm`）——**绝不写** |

## 03｜systemd

四个单元 `notes/systemd/*.service`（`ink`/`transcribe`/`mind`/`note`），随书架载荷一起装：`PartOf=shelf.target` + `WantedBy=shelf.target`，`After=home.mount`（transcribe/mind 另 `Wants/After=network-online.target`，出网那两个才需要）；`Restart=on-failure`；`CPUWeight=20`、`MemoryMax=128M`、`Nice=5`。**不给 xochitl 加任何依赖**（红线）。装/卸：`shelf/install.sh --only ink,transcribe,mind,note`、`shelf/uninstall.sh`（条目库不在 `--purge` 范围，绝不删用户笔记）。

## 03b｜地基三 crate（2026-09-06，离线）

> 白话：笔记线要先能“读懂书页的 .rm 文件、知道页在哪一章、把手写和勾画配成对”。这三个 crate 就是干这个的，全是纯函数、零网络，可以离线单测。

| crate | 干什么 | 关键事实 |
|---|---|---|
| **rmv6** | 解析 `.rm` v6（只认 `reMarkable .lines file, version=6`） | `Page::parse` 给 `strokes`（`SceneLineItem`）/ `highlights`（`SceneGlyphItem`＝GlyphRange：原文 + 页文本偏移 + 每行矩形）/ `text`（打字文本）。**勾画与手写笔画同一坐标系**，几何配对不需换算（§03f 真机样本坐实）。擦掉的项是墓碑，有**两种形态**：独立的 `SceneTombstoneItemBlock`，或 `SceneLineItem`/`SceneGlyphItem` 自身 `item.value` 为空（真机样本 105 个 `SceneLineItem` 块里 17 个是后者），两种都已处理 |
| **epubmap** | 页号 → 章/小节 | `.epubindex` 两张表（每条 `u32 长度 + UTF-16BE 路径 + 3×u32`），取每个 basename **首现**的第一个 u32＝起始页（0-based，路径非 ASCII 不认，防误配）；目录 `nav.xhtml` 优先、`toc.ncx` 退回（标签事件流 + 深度栈，不带完整 XML 解析器以省体积）；`chapter_of(page)`＝1 级祖先为章、本条 ≥2 级为小节；`.content` 的 `pages` 是页 id 顺序表，下标＝页号（真机 523 项核过） |
| **notecore** | 领域核心（纯函数，零 I/O） | `geom::cluster` 并查集（任意两笔包围盒间距 ≤ `cluster_gap` 连通，不看笔序/时间——用户会回头补笔）、`pair` 每簇配最近勾画（≤ `pair_gap`，否则算“本页批注”）；`is_handwriting` 排除荧光笔/橡皮/选区；缺省 `cluster_gap=40`/`pair_gap=120`（页坐标单位，§03f 真机验证有效未改）；`ingest::merge_page` 实现 §01 的增量规则；`marker::split_leading_marker` OCR 兜底（`口渴了`/`-3 度`/`2024 年` 不误判）；`model::Style::wire_code`（Body=1/Bullet=4/Numbered=10/Checkbox=6） |

`geom::has_underline`（簇内某笔宽≈簇宽、矮、贴底→分区头候选，§03f 加）**三期随分区一起删除**（从没被真正接线，见 §01、§03s）。

测试 fixture：`testdata/renggu/page.rm`（《人骨拼圖》c65fa2ae 页，1049 B，23 笔全墓碑，只测“解析成功零条目”）；`renggu_marks/`（同页 2026-09-07 重画的真实勾画+手写）；`seven_styles/`（笔记本一页七样式，测段落样式码）。

## 03c｜ink-serve 矿（2026-09-06，真机首轮）

- **路由**（经网关 `/api/ink`）：当时 `GET /books` · `GET /books/{uuid}` · `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id}`（逐字段）· `PUT /books/{uuid}/sections`（**三期已删**，分区整个砍掉，§03s）· `POST /books/{uuid}/rescan` · `GET /events`。**现行路由以 README「主要 API」为准**（另有 `request`/`skip`/`archive`/`restore`/`purge`/`koreader/import`）。
- **摄取**：只处理活的 EPUB（`DocumentType`、非回收站/非 deleted、`fileType=epub`）且有手写页的文档；启动追平一遍，之后 `watch_debounced`（`debounceSecs` 4）按事件文件名认 uuid、只扫涉及的文档；页 mtime 记在条目库 `page_mtimes`。裁图：页包围盒 + `cropMargin` 24 → 缩略图像素（`PageGeom::to_pixels`，夹在图内），文件名带簇指纹，指纹不变不重裁——**这版“吃缩略图”的裁图后来被 §03p 的自渲染取代**。
- **真机首轮**：`active`、注册；追平扫到《人骨拼圖》38 章；`ink.json` 首启写出缺省。当时唯一有 `.rm` 的页 23 笔全是墓碑、扫出 0 条——真样本已于 §03f 补上，之后能摄取真实条目。

## 03d｜网关「笔记」tab + note-serve 骨架（2026-09-06）

- **书架侧改动**：`MODULES` 加行、`AREA['note-serve']='notes'`、`TABS['note-serve']`；`build.sh`/`deploy.sh` 的 `NOTES_BINS`、`install.sh` 的 `ALL` 令牌。note-serve 用 `ServiceSpec.tab=("笔记",25)` 注册 tab；`GET /status`（vault 路径）、`GET /events`。
- **网页首版**（`renderNotes`）：选书 → 按章分卡片 → 每条：左裁图、右文本框（缺省填 `text ?? draft`，改即 `POST entries` 存）、分区下拉、样式下拉、状态徽章、勾画原文引用、智能回答折叠；分区编辑；“重扫”。**分区相关控件三期已删，样式下拉后来改成行首标记自动识别**（§03s/§03u）。真机：tab 出现、两服务 active。

## 03e｜transcribe-serve 转写（2026-09-06 夜，真机 WiFi 部署）

> 白话：转写服务盯着矿有没有新条目，有就把手写裁图喂给视觉模型，把识别结果当“草稿”写回。人在手机上再改。

**触发**：订阅 ink-serve 的 `/events`（注册表找 → 长连接 → 断线重连，现统一用 `rmsvc_core::events::follow`），`area=notes,kind=entries` 且 `auto=true` → 踢工作线程 → 防抖 3 s 合并 → 跑一轮；启动追平一次；`POST /run` 同步跑一轮；`POST /books/{uuid}/entries/{id}` 强制转写一条（忽略指纹与失败上限）；`POST /retry` 清失败记录再踢。自动与手动共用 `run_lock` 互斥。

**一轮**（`worker::run_once`）：`GET /books` 只看 `pending>0` 的书 → 条目 `needs_transcribe()`（`ink.hash` 没有对应草稿且非终态）→ 取裁图 → 提示词（内置：只转写不发挥、保留换行与行首符号、认不出用“？”、无字输出空；**勾画原文截 300 字作语境**帮人名/术语但明说别抄；`temperature=0`）→ 模型 → 样式仍为正文时 `marker` 兜底 → `POST ink entries` 写 `draft{text,backend,at,hash}`（+ `style`）→ 账本记 token。

**节制**（用户对费用/限流的关切）：每轮 `maxPerRun` 20、请求间歇 `pauseMs` 300；同一条同指纹失败 `maxAttempts` 3 次后不再自动试（指纹变了归零）；**一轮连续 3 次失败零成功即停**（key 错/断网别一条条撞 60 s 超时）；账本只记数不记内容。

**key**：`GET /config`/`/status` 只报 `hasKey`/`keySource`（config/env/none）**不回显**；`PUT /config` 的 `apiKey` 非空才改、`clearKey` 清；文件 `0600`；环境变量 `DASHSCOPE_API_KEY` 兜底。（这套配置形状后来在 §03q/§03u 改成预置下拉 + 按厂商分存。）

**取舍**：不直写条目库（§01 唯一写者）；限量 + 即停（一次合书几十条，key 错时不该烧完超时）；行首标记在转写侧兜底（`1.` 几何认不出，表里本就写 OCR 判，几何判出的不覆盖）；勾画原文进提示词（旧 cardhw 经验：上下文救人名/术语，代价是 token 略增）。

**真机部署当轮**：`active`、注册表可见；日志 `后端 qwen qwen3-vl-plus；key None`；`/status` 无 key 时 `lastRun.note="未配置 API key（…）"`；`transcribe.json` 为 `-rw-------`。~~待验：粘 key 后真调模型、事件链~~ **已验**：§03f 样本到手后 key 配好、真调过模型（§03g 的翻车与修复就是这次真调暴露的）。

## 03f｜步骤 0 真机样本标定（2026-09-07）

> 白话：先在设备上真勾真写两份样本拉回来，验证“聚簇/配对阈值对不对”“七种打字样式的 .rm 码是什么”。结果并入 `cargo test --workspace` 常驻回归（不是一次性脚本），并用 Python `rmscene` 0.8.0 交叉解析验证。

**样本**：① `testdata/renggu_marks/`——《人骨拼圖》同一页勾三段原文，每段旁写一行（首字分别 `-`/`1、`/`口`），另写一行字 + 下面一条长横；② `testdata/seven_styles/`——新建笔记本一页，格式菜单逐行打 Title/Subheading 1/Subheading 2/Body/Bulletpoint/NumberedList/Checkbox（含 “Checkbox” 与 “Checkbox finished” 两行）。

**坐实**：

| 项 | 结论 |
|---|---|
| x 原点居中 | 三条高亮矩形 `x` 全为负（左半页），重渲染与真机截图布局吻合→`xOriginCenter=true` 不用改。⚠ 当时只验了“方向”，误以为连“宽 1404”也验了；真实画布是 960×1280，见 §03g |
| 聚簇/配对阈值 | `cluster_gap=40`、`pair_gap=120`：三处批注簇与对应勾画间距均 **0.0**（紧贴/压线），到次近勾画 ≥148pt；第四簇（字+下划线）离最近勾画 305pt，正确判“本页批注”。阈值卡在两类间距中间，安全边界宽，不用改 |
| 荧光笔自留一条笔画 | `SceneGlyphItem` 之外，画高亮的笔本身还留一条 `SceneLineItem`（工具 `Highlighterv2`）；`is_handwriting` 按 `Tool::Highlighter` 排除，真机样本验证有效 |
| `has_underline` | 下划线单独一笔，宽≈簇全宽（487.7）、高≈簇高 1/6（18 vs 113）、贴底；几何足够稳（三条批注簇为假、第四簇为真）——**但后来没接线，随分区一起删除**（§03s） |

**新发现**：
- **NumberedList 真实码 = 10**（`ParagraphStyle::NUMBERED`、`Style::wire_code`）。~~格式子块多 7 字节未解码载荷~~ **§03i 更正：那 7 字节属于 Subheading 1，NumberedList 只有 2 字节，无此前提**。
- **Subheading 1 与 2 共用 wire 码 3（BOLD）**，区分开关是格式子块末尾的 7 字节 `21023403000000`（带＝大字号 Subheading 1）——§03i 更正，此前“原生按位置动态决定”的猜测是错的。
- **Checkbox 勾上号（7）**：打字给不出，是对已渲染方框的一次点击手势；2026-09-07 用户点方框后拉回的文件里那行确为 7（§03i）。只读设计下用不上，写入器也造不出“直接生成已勾选”。
- **软删两种写法并存**：独立墓碑块，或 item 自身 `value` 为空——`Page::from_file` 两条路都处理，记一笔防只测一种。

产物：4 个新测试（rmv6 真机样本解析 2 + notecore 聚簇配对回归 1 + `has_underline` 1，已随后者删除），`cargo test --workspace` 31→35。

## 03g｜真机转写首轮翻车 → 挖出裁图画布尺寸错（2026-09-07）

> 白话：第一次让模型真转写，四条草稿全是“页面别处的印刷体”。顺藤摸瓜发现：裁图坐标换算用错了画布尺寸。**结论：EPUB 页的 `.rm` 坐标画布是 960×1280，不是物理屏 1404×1872。**

**现象**：4 条条目的 `draft` 都没抄用户写的 `-第一段`/`1. 第二段`/`口 第三段`/`这是一段话并画线`，而是抄了页面别处的印刷体（如“如此強烈”≈epigraph、“：30P.M.”≈时间范围行）——不是随机，都是“沿页面往下大致对应位置”的印刷体，说明**裁图裁偏了**，不是模型瞎编。

**定位**：把 4 张裁图拉回来肉眼看，确认是印刷体/空白，问题在 ink-serve 的裁图。用真机缩略图程序化扫像素（高亮橙色底 `r>230,170<g<230,100<b<190`）反推三条高亮真实落点，与 `PageGeom::to_pixels` 用 `pageWidth=1404/pageHeight=1872` 算出的框对比——对不上，偏差随页面下方增大（线性缩放系数错，不是平移错）。

**定量**：三条高亮像素行/列 vs 页坐标最小二乘：`real_row = 0.4019×page_y − 0.36`、`real_col = 0.3997×page_x + 191.4`（两轴缩放≈0.40，验证拟合没错）；反推等效画布 `page_h≈1274`、`page_w≈961`，x 偏移 `191.4/0.3997≈479`≈`page_w/2`（x 原点居中方向没错）。1274×961 极接近 **1280×960**（3:4，同缩略图比例）。

**根因**：EPUB 页 `.rm` 坐标是 xochitl EPUB 排版引擎自己的**虚拟画布**（960×1280，仍 3:4），再整体缩放贴合物理屏；1404×1872 是笔记本/PDF 的物理屏坐标系。`ink.json` 抄了物理屏数字，§03f 只验了 x 的正负方向，没量精确尺寸。

**影响面与修法**：
- `notecore::geom`（聚簇/配对）**不受影响**——只在 `.rm` 原始坐标系内比相对距离。
- 唯一受影响的是 `ink-serve::crop`（页坐标→缩略图像素）：`IngestConfig` 缺省 `page_width 1404→960`、`page_height 1872→1280`；回归测试 `crop::tests::real_page_geometry_lands_on_the_real_highlight`（高亮 1 落在行 264–281／列 51–138 附近；用 1404×1872 会跑到行 179–192／列 96–156，测试变红）。
- **只验过 EPUB 页**：笔记本（typed text/手写自己的画布）没单独测过，note-serve 读写笔记本坐标系时**不能套用这组数**。
- 衍生：第 4 条批注（长句+下划线，无勾画）真实 y 在 1507–1620，超出 960×1280——写在可滚动内容里、缩略图视口截不到；`to_pixels` 硬夹到图边缘裁出宽/高仅 1px 的废图，喂给 Qwen 被拒（`height:1 ... must be larger than 10`）。加 `crop_png::MIN_CROP_PX`（8px）守卫，小于则判“裁不到”返回清楚错误，不再送废图烧 API（回归测试 `crop_png_rejects_offscreen_bbox`）。

**当场真机复验**（重编译部署两轮 + 手改设备上已落盘的 `ink.json` [新二进制不覆盖已存在配置] + 清 4 张旧裁图缓存 [按指纹命中文件即跳过，光换配置不会重裁] + `POST rescan` + 强制重转三条）：

| 条目 | 真实手写 | 修复前草稿 | 修复后草稿 |
|---|---|---|---|
| 高亮 1 旁 | `-第一段` | “如此強烈”（抄印刷体） | “：30P.M.主△ ～第1段”（结构对，**“一”被认成 “1”**，混进边缘一点印刷体） |
| 高亮 2 旁 | `1. 第二段` | “：30P.M.” | “只能叫計程 **1. 第2段** 手提雲膠的”（结构对，**“二”被认成 “2”**，上下混印刷体） |
| 高亮 3 旁 | `口 第三段` | “領取行李 也已錯過” | “常？穂 ？車 這些顏”（完全没读对：分辨率/裁图边界待调） |
| 无勾画批注 | `这是一段话并画线` | 抄页面别处 | 裁不到（视口外），`MIN_CROP_PX` 守卫生效；草稿字段**仍是修复前的错误旧值**（drafts 只增不删），用户手动改/留空即可，不是数据损坏 |

**结论**：坐标根因坐实且真机验证有效（3 条有勾画的裁图对准了真实手写）。**但转写准确率不能算“读对”**：2 条只对了“第□段”结构，汉字数字被认成阿拉伯数字（连笔“一”像“1”），第 3 条整条没读对——这些是转写质量问题（分辨率/边距/OCR），与坐标 bug 是两回事，不能反过来当“现在准了”的证据。遗留：① 汉字数字被转阿拉伯数字（提示词已补规则）；② 裁图边距混进相邻印刷行（后由 §03p 自渲染裁图解决）。

**教训**：`.rm` 坐标“看起来像页面尺寸”不代表是设备物理分辨率——EPUB/PDF/笔记本三种文档各自的排版画布可能不同；下结论前拿“同一份数据里已知语义的东西反推”（这次是高亮该出现的印刷段落位置 vs 缩略图真实像素）比“读一个数字直接信”可靠。以后采样笔记本画布、PDF 定稿画布都该重复用这招。

## 03h｜rmv6 补写能力：`RootTextBlock` 编码器（2026-09-07）

> 白话：note-serve 要往设备写“打字笔记本”，rmv6 之前只会读。评估后只自己写 `RootTextBlock`（正文文字块）这一个块，其余块用“模板替换”原样拼回。

**方案**：`AuthorIdsBlock`/`MigrationInfoBlock`/`PageInfoBlock`/`SceneInfo`（真机样本自带一段没解出的不透明字节）/`SceneTreeBlock`/`TreeNodeBlock`/`SceneGroupItemBlock` 与“页面上打了什么字”无关，从零重建风险高收益低——改用**模板替换**：拿一份真机产出、xochitl 能正常打开的 `.rm` 当模板，扫顶层块序列（`u32 长度+u8 0+u8 min_version+u8 current_version+u8 block_type`，找 `block_type=7`）定位 `RootTextBlock` 字节区间，只重新生成这一块，其余原样拼接。

**写入器**（`rmv6::write`）关键事实（真机样本 + 独立 `rmscene` 交叉验证坐实）：
- 全新文档无编辑历史：`deleted_length` 恒 0、CRDT id 不用留空隙（真机活文档里“隔一个 id”的缝是打字/删改历史的产物）。
- 每段一个 wire 条目，`item_id` 指向本段第一个字符，后续字符隐式 +1；`left_id`＝上一段最后一个字符的 id（首段用 `(0,0)`）；`right_id` 恒 `(0,0)`。
- 样式表的 key＝“结束上一段的换行符”的 id（＝本段 `left_id`），value 是 `{固定字节 17, 样式码}`。
- **⚠ 真机大坑（独立验证抓出）**：文本条目里的 `is_ascii` 字段，真机样本对**含中文的段落也写 1**。按字面老实判（ASCII 写 1、非 ASCII 写 0）能通过我们自己宽松的解析器，但 `rmscene` 有 `assert is_ascii == 1` 会炸——真实设备/规范要求恒为 1，字段名有误导性。**教训：先拿独立实现交叉验证，别只信自己写的解析器**（往返测试完全测不出这个 bug）。
- 首版支持 5 种样式，NUMBERED 因怀疑有未解码载荷而不支持——后证明是分析失误，§03i 补全。

**验证边界**：`cargo test -p rmv6` 全绿（写入→`Page::parse` 读回）+ `rmscene` 独立解析正确读出全部段落与样式。这只证明“两个读取实现都认”，不等于 xochitl 真能打开——由下面的真机验证补上。

**上传通道不是新领域**：`POST /upload` 除 EPUB/PDF 外**也吃 `.rmdoc`**（reMarkable 官方文档包 zip：`<uuid>.metadata` + `<uuid>.content`[+`.pagedata`] + `<uuid>/<page>.rm`，**不含** `.local`，导入端自建）；multipart 字段名 `file`，`.rmdoc` 用 `application/zip`。真机行为：201 “Upload successful”、免重启出现在书库、**导入端会重新分配设备 UUID**（调用方按 `visibleName` 事后认领，与书架 `render_check.rs` 按 createdTime 圈候选是同一类模式）。这些事实来自旧项目（`reading/protocol/inject.py`、`device-core/src/inject.rs`、`knowledge/pkm/src/cardnote.rs` 已摸清）；按“不引入旧代码”的红线不搬实现，只借事实，`note-serve::rmdoc` 是新写的。**未坐实**：xochitl 按文件名后缀还是按 zip 内容分流（结论全是黑盒试出，非反编译）。

**打包器**（`note-serve::rmdoc`）：一页 `.rm` 字节 + 书名 + 父文件夹 → `<uuid>.metadata` + `<uuid>.content`（`fileType:"notebook"`，`formatVersion 2`/`cPages`，参照 `testdata/seven_styles/book.content`，`extraMetadata` 空对象就够）+ `<uuid>/<page>.rm`，STORED 不压缩；上传直接复用 `rmsvc_core::xochitl::Xochitl::upload`（当时叫 `shelf_core`）。Python `zipfile` + `rmscene` 交叉核过 zip 结构与内嵌 `.rm`。

**真机验证通过（2026-09-07 当场）**：6 段测试内容（Title/Subheading/Body/Bullet×2/Checkbox）打包上传，返回 `{"status":"Upload successful"}`：新文档立刻出现、免重启；导入端重新分配了文档 UUID 和页 UUID；`.content` 被 xochitl 重写（`uuids[0].first` 换成它自己的作者 UUID、丢了我们写的 `modifed` 字段，`pageCount`/`fileType` 保留）；xochitl 自己重新渲染生成缩略图（38501 字节）。肉眼核对缩略图：六行内容、五种样式（大标题/加粗小标题/正文换行/两个圆点无序/空心方框待办）全部渲染正确。**这是笔记线第一次真让 xochitl 打开并渲染我们生成的文档**——编码器与打包器至此才算真正闭环。

## 03i｜更正一处误判：NUMBERED 是无辜的，7 字节其实是 Subheading 1 的开关（2026-09-07）

用户打开 §03h 的测试文档反馈“没有有序列表、没有勾选框、没有分区头 1”，并用**原生格式菜单按钮**把三样补打一遍。拉回来一查：待办行确实是 Checkbox（缩略图有空心方框，用户没细看）；但另外两条揭出 §03f 的一处**判断错误**。

**排查**：不走 `TextDocument` 的样式名，直接读每条 `styles` 记录的原始字节 `[17, code, …]`。原生 “Subheading 1” 打出的段，格式子块比裸 BOLD 多 **7 字节** `21 02 34 03 00 00 00`；原生“已编号列表”的两段，格式子块跟其它样式一样只有 **2 字节**。

**结论**：§03f 把“某处有 7 字节没读完”的警告记错了对象。
- **NumberedList（10）没有隐藏内容**，序号由 xochitl 按“连续几个 NUMBERED 段”自动数，不用存；`rmv6::write` 已解除限制。
- **Subheading 1/2 共用 wire 码 BOLD=3**，开关就是那 7 字节：带＝Subheading 1（大字号），不带＝Subheading 2（小字号），**与段落位置无关**（用户验证过，§03h “大概率位置决定”的猜测是错的）。`Paragraph::subheading1()` 补这 7 字节（`SUBHEADING1_MARKER`），`Paragraph::new(BOLD,…)` 保持不带（当 Subheading 2/小节标题用）。

**二次真机验证**：8 段测试文档（含 `subheading1()`、裸 `BOLD`、两段 `NUMBERED`）传真机，缩略图：真 Subheading 1 明显大、裸 BOLD 明显小、有序列表自动编号 “1.”“2.”。**xochitl 3.28 打字格式菜单全部 7 种样式现在都支持写入且真机验证**——只有 Checkbox“已勾选”（码 7）写入器造不出（真限制，不是分析失误）。

**教训**：读 wire 格式时，“警告出现在文件读到一半的位置附近”不足以判断归属；改成显式记录每条 `styles` 记录的 `char_id`，逐条目核对多余字节。以后遇到“某处有没读完的字节”，先绑定到具体 char_id/条目。

## 03j｜note-serve 生成编排：条目库 → 一章一本（2026-09-07，离线写完，当晚真机验证见 §03k）

> 白话：把“从条目库排出一章的段落列表 → 判断要不要重传 → 旧版本怎么处理”这层业务逻辑写完。**本节记的是“代码写完那一刻”的设计，真机结果看 §03k；其中“分区”“建夹”两块已被 §03s、§03ae 取代。**

**投影是纯函数**（`notecore::project`，零 I/O）：`project_chapter(book, idx)` 把一章条目铺成 `rmv6::write::Paragraph` 列表——Title＝章名；条目样式取 `Entry.style`、文本取 `display_text()`（没转写占位“（待转写）”），条目下各跟一段 Body：勾画原文（`〔原文〕`前缀）与 AI 回答（`〔AI〕`前缀）；已撤销条目不投影；空章返回 `None`（调用方不为空章生成文档）。~~按 `Section.order` 排的分区各起一段 `subheading1()`，指向未知分区落“未分区”兜底段~~（**三期分区已删，现在按页序平铺**，§03s）。`fingerprint_chapter` 用同一批输入算 FNV-1a 指纹，只要不变就不重打包上传——这是“重生成”判断的唯一依据（`Paragraph` 无 `PartialEq`，所以指纹在纯输入上算，不是拿投影结果反推）。

已知留白（模块文档里写了，不是漏项）：① `Entry.subhead`（epubmap 的 h2/h3 小节）最初没参与分组（后续见 §03s）；② xochitl 按“连续几个 NUMBERED 段落”自动编号，摘录/回答的 Body 段插在两条有序条目之间会打断连续、编号从 1 重来——真要连续编号的清单，条目之间目前不能有摘录/回答，没解决。

**生成编排**（`note-serve::publish`，Strategy + 纯函数骨架，测试全用内存桩）：`Uploader`/`TrashSink`/`EntryStore` 三个 trait 抽象“传书”/“旧版本入队”/“读条目库”，生产实现分别包 `Xochitl`、跨服务 HTTP 调 `book-serve` 的 `POST /trash/add`、跨服务 HTTP 调 `ink-serve` 的 `GET /books(/{uuid})`。`generate_chapter` 流程：

1. 查指纹 → 没变返回 `Unchanged`（零网络）；
2. 变了 → `build_page_rm` 编码 → `rmdoc::pack` 打包 → `Uploader::upload`；
3. `Uploader::claim` 按 `visibleName` + `createdTime>=now_ms` 认领设备新分配的 uuid（同书架 `render_check.rs` 的“圈时间窗按名字认领”模式）；
4. 这章之前生成过且这次换了新文档 → 旧版本 `TrashSink::add` 入队（用**生成时记录下来的旧 visibleName**，不是按当前书名/章名重算——`book-serve` 按名字核对 uuid 防错删）；
5. `notebooks.rs` 记新记录。

任何一步失败整章标 `Failed{error}`、**不写状态**（下次照常重试，没有“卡在半成品”）；旧版本入队失败**不算这章失败**（新文档已生成，旧的多留一份不是数据丢失，只打日志）。

**簿记是 note-serve 自己的、不是条目库**：`$XDG_STATE_HOME/notes/notebooks/<book_uuid>.json` 记每章 `{doc_uuid, visible_name, fingerprint, generated_at}`；条目库唯一写者仍是 ink-serve。丢了这份簿记最坏后果是“重新判一次要不要重生成”，不丢用户数据。

**新增路由**（经网关 `/api/notes`）：`GET /books`、`GET /books/{uuid}/notebooks`、`POST /books/{uuid}/generate`（全书按需重投影）、`POST /books/{uuid}/chapters/{idx}/generate`（单章）。

**当时的已知缺口**：`Xochitl::upload` 目标文件夹不存在时 best-effort 落库根，本模块不自动建《书名》文件夹——由 §03l 补上，又于 2026-09-09 改成复用书本自己的文件夹（§03ae）。

**离线门槛**：`cargo test --workspace` 41→51（notecore 11→15、note-serve 1→7）；`dump_device_test_doc` 是 `#[ignore]`。真机验证五项清单（文件夹里章节数、缩略图样式/编号、改一条只重传那章、旧版进回收站、新书落根的已知缺口）当晚全部通过，见 §03k。

## 03k｜note-serve 生成编排真机验证通过（2026-09-07 当晚）

§03j 的生成编排第一次真机跑通，比预想更完整——**旧版本真的自动进了回收站，不是只挂在队列里**。

**准备**：交叉编译 + `SHELF_NO_BUILD=1 ./deploy.sh <IP> --only note`（只更新这一个服务）。设备上《人骨拼圖》的 4 条真实条目（§03f/§03g 样本）当时被判“笔画消失”标成 `Revoked`（页面被重写过，ink-serve 正确识别撤销，不是 bug）。按“②手机修正”同一条路径 `POST ink /books/{uuid}/entries/{id}` 把 4 条的 `text`/`style`/`section` 补齐（校对文本用当初真实手写原文：`第一段`/`第二段`/`第三段`/`这是一段话并画线`；样式 Bullet/Numbered/Checkbox/Body），转回 `Reviewed`，凑出一份四样式测试章节。（⚠ 这一步后来在 §03m 被认定为事故根源，见该节。）

| 轮次 | 操作 | 真机结果 |
|---|---|---|
| 1 | `POST /books/{uuid}/chapters/1/generate` | 返回 `{"status":"generated","doc_uuid":"b20ec5fd-…"}`；文档出现（`parent:""`，《人骨拼圖》文件夹当时不存在，如预期落库根）、`.content` 合法（`notebook`/`formatVersion 2`/一页）、xochitl 自渲染缩略图；肉眼核对：标题、分区头、四条条目样式（圆点/编号/方框/正文）、每条下方〔原文〕摘录全部位置正确 |
| 2 | 改一条已校对文本再生成同一章 | 返回新 `doc_uuid`；本地簿记指纹与 uuid 更新；`book-serve GET /trash/pending` 立即可见旧 uuid（用的是生成时记录的旧 visibleName，核对通过）；**几秒后旧文档 `.metadata` 的 `parent` 变成 `"trash"`**——设备停在书库视图，新文档 `/upload` 触发 `entityListModel.rowsInserted`，`shelf-trash-agent.qmd` 的 4s 防抖计时器打了 `selection.add`+`selectionMoveToTrash`，`trash/pending` 随即清空。**“条目库→一章一本”整条闭环第一次端到端全自动**；编辑过的文本正确出现在渲染里 |
| 3 | 不改任何东西再生成 | 返回 `{"status":"unchanged"}`，设备文档总数不变——指纹跳过在真机上成立 |

**结论**：`notecore::project`/`note-serve::publish` 整条链（投影→打包→上传→认领→旧本回收→簿记→指纹跳过）真机全绿，§03j 的“尚未真机验证”撤销。留在设备上的两份历史测试文档（旧的在回收站，新的在库根，标题「第2章 第一部 一天的國王 1」）可人工删。

**顺带证实**：① `shelf-trash-agent.qmd` 这套“纯事件驱动、无需人工确认”的软删机制在真实数据下可靠；② note-serve 全程走**跨服务 HTTP 直连 loopback**（`ink.rs`/`trash.rs` 连 ink-serve:8795、book-serve:8790，不经网关），这套“服务互相直连而不绕网关”的套路（transcribe-serve 早就在用）在多服务同时跑时无端口冲突或注册表查找失败。

**当时遗留缺口**：《书名》文件夹自动创建（§03l 解决、§03ae 又改）；生成按钮还没接进网页（§03r/§03t 已接）。

## 03l｜建夹代理：《书名》文件夹自动创建（2026-09-07，真机验证通过；**2026-09-09 已被 §03ae 废弃**）

> **⚠ 现状：这条链路已不再是 note-serve 的依赖。** 2026-09-09 起 `ensure_folder` 与 `note-serve::mkdir` 整个删掉，改成直接复用书本自己所在的设备文件夹（§03ae）；书架侧的 `book-serve::mkdir`/`shelf-mkdir-agent.qmd` 2026-09-15 起已确认全仓库无消费方、物理删除（见书架白皮书对应段落）。下面只留结论与教训。

§03j 的已知缺口（目标文件夹不存在时落库根）当时这样补：`Uploader` trait 新增 `ensure_folder(folder_name)`，`generate_chapter` 上传前 fire-and-forget 调用（先 `Xochitl::find_folder` 查存在，不存在才跨服务请求 `book-serve POST /mkdir/add`）。QML 反编译 + `book-serve::mkdir` 队列 + `shelf-mkdir-agent.qmd` 的设计与验证记在书架白皮书（按 §01 分工，机制归书架）。真机验证：改一条文本触发重生成 → 建夹请求 → MainView 的 8 s 轮询代理把文件夹建出来 → 再生成一次新文档落进该文件夹（建夹前已生成的旧份原样留在库根，设计内行为）；重复触发不产生重名文件夹；五个 notes/shelf 服务 + xochitl 健康（`NRestarts=0`）。

**一个可复用的真机操作教训**：`/home/root/xovi/start` 第一次跑完显示 `Job for xochitl.service canceled`，事后核对 `/etc/systemd/system/xochitl.service.d/` 是空的（LD_PRELOAD 一个都没进新进程环境），原地重跑一次才成功。**以后跑完 `xovi/start` 别只看 `systemctl is-active`**（进程会正常起、只是没挂 xovi，qmd 全部形同虚设且不报错），要核对新 PID 的环境：`tr '\0' '\n' < /proc/<pid>/environ | grep -E 'LD_PRELOAD|XOVI'`。（另注：xovi 已生效时重启 xochitl 应用 `systemctl restart xochitl`，不要再手动跑 `xovi/start`。）

## 03m｜用户真机核对揪出两个真问题 + 一次数据事故（2026-09-07）

用户在真机上核对《人骨拼圖》笔记 tab 时报了四点，理清后是**一个数据事故 + 两个真代码缺口**。

**数据事故（先认账）**：§03j–§03l 几轮真机验证时，为了让生成的笔记本截图有内容可看，我把用户早前口述过的“真实原文”用 `POST /entries/{id}` **手动打进条目库**——不是转写模型认出来的——而且反复打了几轮，每次都把 ink-serve 已正确判定的“已撤销”（用户当时已擦掉勾画/手写）翻回“已校对”（日志可对上：`10:54:22 撤销 4` 之后到 `15:xx` 还在手动 POST）。用户当场问“识别这么准了吗，还是你帮我填进去的”，如实认了。事后清理又踩同类坑：想清空 `text`，直接 `POST {"text":""}`，没注意该接口“给了 text 就按内容判定状态”，把 4 条从“已撤销”带回“待校对”，用 `/rescan` 修回 `entries:0`。**教训**：真机验证要看到内容时，宁可用一望即知是占位符的假文本，绝不拿用户的真实描述往真实条目库里塞；改字段前先读懂接口对状态字段的副作用。

**真问题①：书清空后仍赖在网页下拉里**。摄取门槛 `doc::annotated_pages()` 看的是页 `.rm` 文件存不存在，笔画擦光文件不会被删；而 `GET /books` 原用 `BookDb::list()`（不过滤）。修法：新增 `list_active()`（只列“至少一条非 Revoked 条目”的书），`GET /books` 换用它；单测 `list_active_hides_books_whose_entries_are_all_revoked`。真机验证：清空的《人骨拼圖》从列表消失，用户在标注的《Gulliver's Travels》（真数据）正确出现。（后来“终态”判据从 `!= Revoked` 扩为 `is_terminal()`，见 §03ag。）

**真问题②：`## `/`### ` 手写标记设计**（用户定案，替换没接线的 `has_underline`）：`notecore::marker::Marker` 从 `(Option<Style>, String)` 改成三态（`Style`/`Section`/`Subhead`）；`#`（Title）明确不接，留给将来“纯手写笔记扫描”那条没立项的线。**当时** `##` 建/找同名分区、`###` 覆盖 `Entry.subhead`；**三期砍分区后 `##`/`###` 合并为同一件事——都覆盖 `subhead`**（§03s/§03t）。改动跨三层：`notecore::marker`/`model` → `transcribe-serve::worker`/`ink`（`post_draft` 签名换成 `Option<Marker>`）→ `ink-serve` 的 `POST /entries/{id}`（接 `sectionHint`/`subheadHint`；`Book::section_id_for_name` 要整个 `&mut Book`，必须在拿到条目可变借用之前调用）。**当时离线单测全绿但无真机验证**：`##`/`###` 会不会被 OCR 转错仍是未知，见 §03o/§05。

## 03n｜二期立项：浏览态触发 + 问AI + 模型配置统一（2026-09-07）

> 白话：一期“写了就自动转写”会浪费转写调用、搅乱列表；分区又身兼两职。二期把“探测到”与“真要转笔记”拆成两个动作，AI 改成按条目单发。

**动机**（用户用了几轮真机后提出）：① 只要写了字/勾了线就自动进条目库、自动转写，但有的只是整理思路、不想转笔记；② “分区”身兼“笔记本怎么分组”和“要不要调 AI”两职，问 AI 前必须先归好分区，不直接。

**二期计划**：探测与转笔记拆成两个独立动作（浏览列表显式点『转入笔记』/『不需要』）；AI 触发从分区简述改成按条目的问题输入框；模型配置/用量做成统一页面。（未入库的计划文件 已改版覆盖。）

**耗电评估**（用户追问新增 `mind-serve` 常驻服务对耗电的影响）：用真机 battop 审计数据（`enhance/battop/FINDINGS.md`/`APP-DESIGN.md`）+ 书架白皮书已算过的“SSE 心跳比轮询少两个数量级”评估——结论：自动改按需，网络调用只少不多，不会更费电。（顺手揪出书架白皮书一处过期数字：“musl 静态服务各约 0.56MB”与当前实测 ~1.3–4.1MB 不符，当时只记着没改。）

**步骤 1/2（当时合入 `dev`，未部署）**：`notecore::model::Status` 拆出 `Mined`（探测到、未被要求转笔记，取代摄取初始态 `Pending`）与 `Skipped`（用户点“不需要”）。`Entry::needs_transcribe()` 收紧：只认 `Pending`/`Draft`/`Reviewed`；已在这三态的条目笔画又变了继续认（校对文本不被覆盖）。新增 `Entry::set_triage(target, now)` 做迁移，已撤销条目拒绝。ink-serve 新条目初始态 `Mined`，新增 `POST /books/{uuid}/entries/{id}/request`（转入笔记）/`.../skip`（不需要），共享 `triage()`。`transcribe-serve` **零代码改动**（门槛继承自 notecore），只加集成测试 `mined_and_skipped_entries_are_never_auto_transcribed`。

**故意没部署**：只改了后端，还没有浏览页 UI，部署会让新批注卡在 `Mined` 没按钮转出，日常使用断流；等浏览页一起做完再一次性部署验证（§03o）。离线 56 个测试零警告。

## 03o｜步骤 3 浏览页真机验证通过 + 一个真机 bug 修复 + 两个记录在案未做的缺口（2026-09-07）

**真机 bug：书进回收站/被删仍赖在「笔记」列表**。根因：`ingest_doc` 发现 `.metadata` 不是活文档（回收站/`deleted:true`/文件没了）就 `Ok(None)` 提前返回，从没告诉条目库“这本书没了”，`list_active()` 只看条目状态所以仍列出。（§03m 修的“清空后赖着”是另一条路径。）修法：非活文档走新增 `ingest::revoke_stale()`——这本书未撤销的条目全标 `Revoked`；`State::ingest` 发事件判据从“只看 `pages>0`”放宽到“`pages>0` 或 `merge.revoked>0`”；启动追平候选从“只找活的 EPUB”扩到“条目库里所有已知 uuid”（否则服务没跑时发生的回收/删除重启也追不上）。三种场景（回收站/彻底删除/从没追平过）有单测。真机：《Gulliver's Travels》从 `/books` 消失，条目状态 `draft`→`revoked`（内容留痕不删）。

**步骤 3（浏览页 UI）真机验证**：网关「笔记」tab 拆「浏览」（按页分组只列 `Mined`，转入笔记/不需要两个按钮）/「整理」（只列 `Pending`/`Draft`/`Reviewed`）。完整闭环：新批注落 `Mined`（transcribe 那轮 `scanned:0`，没被自动捡走）→ `/request` → `Pending`（`pending` 0→1）→ ink 事件唤醒 transcribe 自动转写 → `Draft`（文字与手写对上）；对 `Revoked` 调 `/request` 服务端拒绝（400）；“不需要”→ `Skipped` 后 transcribe 再跑一轮确认没扫到。

**真机顺带发现的两个问题（用户拍板“先记录不改代码”，§03p 当晚又接着做了）**：

1. **纯勾画（旁边没手写）的页抓不到**：摄取是“以手写簇为主”倒找勾画（`geom::cluster` 只聚手写笔画、`pair` 只问“每簇离哪条勾画最近”），未被引用的勾画在 `drafts_of_page` 里直接丢弃。候选设计：补一步，把 `pair()` 没提到的勾画单独生成 `ink:None, quote:Some(..)` 草稿（`PageDraft.ink` 改 `Option<Ink>`）；勾画文字是 `GlyphRange` 原生精确文字无需转写，`set_triage` 对“目标 Pending 但 ink 为 None 且有 quote”直接 `text=quote.text`、跳 `Reviewed`；`merge_page` 用勾画自己的 CRDT id 做指纹（`Quote` 加 `id`，`#[serde(default)]`）。
2. **手写位置靠下时裁图截不到 + 缩略图混进印刷体**：当时裁图吃 xochitl 384×512 缩略图，只画生成那刻视口内可见部分；`MIN_CROP_PX` 守卫正确拒绝，代价是这条批注永远转不出（真机复现：bbox y 1306~1352 超出 `page_h=1280`，裁出 100×1px）。另手写贴印刷勾画行太近时缩略图混进旁边印刷体，视觉模型读串行、抄印刷体（“乱画一通”实测：转写=旁边勾画原文）。候选设计：改成从 `.rm` 笔画矢量数据（`Stroke{points,thickness,color}`）在 `ink.bbox` 范围内自己画折线光栅化白底裁图——天然不受视口限制，顺带解决印刷体泄漏；代价是丢缩略图的印刷体上下文、新写光栅化（`image` crate 已在用）。

**`##`/`###` 标记仍没真机验证成**（试了两轮）：第一轮撞“印刷体泄漏”；第二轮在页面空白处干净写 `## 测试分区2`，裁图确认干净，转写却是“井开阅讨分匹2”——纯粹是视觉模型没读对（手写行草连笔，落进 `b1-handwriking-ocr-derisk` 报告早记的“快写~60% 局部整段崩”区间），与裁图/管线无关。管线本身没问题，卡点是识别准确率；仍缺一个“转写对了、`##` 真被认出来”的正例。离线 57 个测试零警告。

## 03p｜两个缺口补完 + mind-serve（步骤 4）+ 一个前端崩溃 bug，二期真机验证收尾（2026-09-07 晚）

§03o 的两个缺口用户当场改主意让接着做，同晚顺手做完二期步骤 4（`mind-serve`）。

**纯勾画条目（已实现，真机验证通过）**：`PageDraft.ink` 改 `Option<Ink>`；`drafts_of_page` 收尾再扫一遍 `pair()` 没提到的勾画，生成 `ink:None` 草稿；`merge_page` 新增认领路径——靠勾画自己的 CRDT id（`Quote.id`，`#[serde(default)]` 兼容旧数据）匹配，勾画被擦时这类条目同样撤销（守卫从“只认 `ink` 存在”放宽到“`ink` 或 `quote` 有一个在”）。`set_triage`：这类条目“转入笔记”不经 `Pending`/`Draft`，直接 `text=quote.text`+`Reviewed`（没手写可转写，`needs_transcribe()` 要求 `ink` 是 `Some`，卡在 `Pending` 会死等）。真机：单独勾一段不写字，条目库出现 `status:"mined", ink:null`；点“转入笔记”后直接 `reviewed`。

**裁图自渲染（已实现，真机验证通过）**：`ink-serve::crop::render_ink` 直接吃 `.rm` 已解析的笔画矢量数据，在 `ink.bbox`+margin 范围内画折线，白底黑线，不依赖 xochitl 缩略图；笔画按 `stroke_ids` 精确挑（不按 bbox 相交，避免混进邻簇）。旧的 `crop_png`/`PageGeom`/`doc::page_thumb` **整段删除**（零消费者；384×512/960×1280 的标定历史留在 `config.rs` 文档和 §03g）。真机：新写的手写批注转写成功、内容对得上（后来被 mind-serve 引用回答问题）；此后没再复现“没有裁图”。**没有专门验证“写在页面很靠下”这个原始症状**（矢量坐标无视口上限，架构上已不可能复现，但没刻意造边界样本）。

**`##`/`###` 标记**：仍未真机验证成，卡点是手写识别准确率（见 §03o 末）。

**步骤 4：`mind-serve`（真机验证通过）**：新服务 `notes/services/mind-serve/`（loopback 8797），照抄 `transcribe-serve` 的 `config`/`backend`/`ledger`/`ink` 骨架，换成文字模型 + 按条目单发：`backend::TextModel`（纯文本消息，无 `image_url`）；`prompt::build` 拼“书名+章节+勾画原文+旁边批注（已校对/转写文本）+用户问题”；`worker::ask_entry` 要求条目已勾 `ask_ai` 且填了非空 `question`（防误触）；答案写回 `Answer{text, backend, at, brief}`（`brief` 语义从分区简述改成“当时问的问题存档”）。**没有批量循环、没有 SSE 订阅**——纯被动等 HTTP，空闲零 CPU。（当时“两边没抽公共 crate、各几十行胶水”，**后来在 §03ac 抽成 `vendorcfg`**。）`notecore::model::Entry` 加 `ask_ai`+`question`；`POST /entries/{id}` 接 `askAi`/`question`；网页「整理」每条加「问AI」勾选框+问题输入框+提问按钮（改即存、点提问才调 mind-serve）。顺手清理过期 UI 文案：分区编辑器里“给 AI 的要求”+“调模型”勾选框（`Section.ai`/`brief` 早不驱动 AI 逻辑）——该字段本身当时没删，三期随分区整体删除。

**真机验证**：勾一条已转写条目、勾问AI、填问题、点提问——**先踩到真实限制**：这台设备的 DashScope 测试 key 只对 `qwen3-vl-plus`（视觉）开了权限，mind-serve 代码缺省文字模型 `qwen-plus` 被拒（`403 access_denied`），不是 bug。把设备上 mind-serve 模型手动改成 `qwen3-vl-plus`（VL 模型本就能处理纯文字）后拿到切题回答，`answer` 正确写回。**代码缺省仍是 `qwen-plus`**——不为一把权限受限的测试 key 改全局默认。

**顺带揪出的前端 bug**：「浏览」「整理」对分组内条目排序时 `a.ink.bbox[1]-b.ink.bbox[1]` 没考虑纯勾画条目 `ink` 可能为 `null`，一进列表抛 `TypeError`；`renderBrowse` 先清空 `innerHTML` 再崩在排序，用户看到**整个「浏览」子视图空白**且无报错（用户直接反馈定位）。修法：`(a.ink?a.ink.bbox[1]:0)-(b.ink?b.ink.bbox[1]:0)`。**教训**：给模型新增/放宽可选字段时，`grep` 该字段全部引用点，尤其排序/比较这种图省事写“肯定存在”的地方。

离线门槛 68 个测试（notecore 18→20、ink-serve 11→10 [换自渲染裁图删 4 加 3]、mind-serve 全新 10）零警告。

## 03q｜步骤 5：模型配置统一面板，二期收尾（2026-09-08，真机验证通过）

> **⚠ 布局已过期**：这版“视觉/文字并排两栏塞在笔记「整理」区”的面板，三期（§03t）整个搬进了「管理」tab，并换成预置下拉 + 脱敏后只剩删除的 key 语义。**当前设计以 §03v/§03w 为准。**本节只留“当时为什么这样、验证了什么”。

二期唯一剩的一步：`transcribe-serve`/`mind-serve` 的 `GET /config` 加脱敏 key 预览（`keyMasked`：有 key 回最后 4 位如 `...ab12`，没 key 回 `null`，只在服务端算、`public()` 从不整串回显，与 `hasKey`/`keySource` 同一纪律）；网关“模型”统一面板（视觉 + 文字并排、各自 key 框/模型/baseUrl/用量行）。原「转写」折叠块里的 key/模型/baseUrl 编辑**挪进新面板**（同一份配置两处都能编会互相打架），转写块只留自动转写开关、跑一轮/重试失败、失败清单。`mind-serve` 此前**完全没有配置 UI**（只能 SSH 改文件），这是它第一次有网页可配。

**真机验证**（`SHELF_NO_BUILD=1 ./deploy.sh --only gateway,transcribe,mind`，三服务 `active`）：
- loopback `GET /config`：`keyMasked` 真实计算正确（设备上真实 key 算出 `"...yP5A"`，两服务一致——同一把 key，符合 mind 配置注释“图省事共用”）。
- `PUT /config` 空 `apiKey`（前端“没编辑 key 框就保存”发的就是空串）不清已存 key；同值 `model` 的 no-op 写穿。
- `strings` 确认新面板元素 id 被编进部署的网关二进制。
- **补验证（用户给了网关密码后同日）**：全部换成走真实 HTTPS 网关（`curl -k -u shelf:<密码>`，Basic，与浏览器登录后同一条鉴权路径），`GET /api/{transcribe,mind}/status` 的 JSON 字段（`config.keyMasked/model/baseUrl`、`usage.calls/ok/failed/promptTokens/completionTokens/lastError`）与前端 `mFill()` 读取的**逐一对得上**；`PUT` no-op 经网关认证层正确写穿。

**没验证**：纯视觉确认（排版、按钮顺手度）——没有浏览器渲染工具能替代，数据契约已证实，剩体验判断留给用户点开网页看。

## 03r｜三期：Markdown 导出 + 落设备笔记本/Obsidian/删除三选一（2026-09-08，真机验证通过）

> 白话：转写、校对、问答做完后，每条内容该由用户决定去哪：设备笔记本、Obsidian（md）、两处都要，或“不要了”。条目库是唯一事实源，两个去处都是它的“投影”。

**用户提出的缺口**：不该让两条投影管线（`project.rs` 设备笔记本 / 新增的 `export.rs` Obsidian）各自无差别吞掉所有活条目。围绕“删除”先提了软删方案（新终态 + 手动清空回收站），用户追问“软删数据会不会无限扩充”——落地方案专门答了这条。

**落地内容**

| 模块 | 内容 |
|---|---|
| `notecore::export`（新，条目库→Markdown） | 一章一个 `.md`（`vault/<书名>/第N章 章名.md`，文件名与 `note-serve::publish` 给设备笔记本起的 `visibleName` 一致）+ 书索引页 `vault/<书名>/书名.md`。front-matter（`book`/`author`/`chapter`/`pages`/`status`/`tags`，字符串自带 YAML 双引号转义）+ `[[书名]]` 反链 + 条目按样式渲染成列表/正文；稳定 `Entry.id`（16 位小写 hex，天然合法 Obsidian 块 id）当块锚 `^id`——改字段按 id 幂等覆盖同一行，不重复块、不丢反链。当时还有“分区 `## 名`”，**§03s 后按页序平铺** |
| `notecore::model::Destination` + `Entry.destination` | `Notebook`/`Obsidian`/`Both`，`#[serde(default)]` 兼容旧条目库。`project.rs::live_entries` 只收 `wants_notebook()`，`export.rs` 只收 `wants_obsidian()`。**缺省 `Both`**（不是一开始想的 `Notebook`）：缺省“只留设备”会让刚写完的 md 导出对所有已有内容立刻“什么都不导出”，违背“新功能对已有内容立刻可用、不引入先设置一遍的摩擦”的项目默认值哲学 |
| `Status::Archived` + `Book::purge_terminal()` | “不要了”的软删终态（与 `Revoked` 同类，区别是用户主动触发）；`purge_terminal()` 物理清 `Archived`/`Revoked`/`Skipped`，是唯一真腾空间的操作。**手动触发不自动跑**——与书级回收站“不自动清空”同一纪律：软删无过期时间，但清空是用户随时能点的显式动作，不会无限膨胀到被遗忘 |
| 服务端 | ink-serve PATCH 加 `destination`；新增 `POST entries/{id}/archive`（复用 `triage()`/`set_triage()`）、`POST /purge`。`note-serve::export.rs`：`export_book()`/`export_chapter()` 按文件名幂等整文件覆盖（文件小，重写比 diff 简单可靠），`POST /books/{uuid}/export`、`.../chapters/{idx}/export`（单章按钮内部也整本重导，保证索引页“哪些章有内容”始终对） |
| 网关「整理」 | 每条去处下拉 + 「不要了」（`confirm()`）；章头「生成笔记本」「导出 md」（前者端点早就有、网页一直没接，此次补上）；书头「清空回收站」（`confirm()`）。（控件后来多轮重做，见 §03t–§03ad） |

**真机 bug（验证时亲眼发现）**：`live_entries` 原判据是 `!= Revoked`——`Mined`（未被要求转笔记）、`Skipped`（用户点过“不需要”）**一直混在两条投影里**，只是之前没有 md 导出、没人细看才没暴露。导出第 2 章时 `.md` 里赫然出现一条用户点过“不需要”的条目（状态 `skipped`）。根因是**允许列表写成了拒绝列表**（`Mined`/`Skipped` 都不是 `Revoked` 但也不该投影）。改成与网页「整理」过滤（`['pending','draft','reviewed']`，一直是对的）对齐的允许列表，两处各补回归测试；真机重导，那条 `Skipped` 消失。这个 bug 自二期“浏览态”改造（2026-09-07）就存在，离线测试没有“混进 Mined/Skipped 会怎样”的反向断言，**再次印证真机验证不能被离线单测替代**。（同类反模式 2026-09-09 又复发，见 §03ag。）

**真机验证**（走真实 HTTPS 网关认证层）：
- `destination` 在人骨拼圖真实条目上改 `obsidian` 再改回 `both`，每次写穿（可逆，可直接验）；老条目库反序列化兼容（三期前落盘的条目 `GET` 出来 `destination` 都是 `"both"`，无需迁移脚本）。
- `POST chapters/1/export`：落 `/home/root/.local/share/notes/vault/人骨拼圖/`，SSH `cat` 核对：修复前 3 条（含那条 `Skipped`），修复后只剩 1 条真正 `Reviewed`（front-matter `status` 由 `draft` 变 `reviewed`）。
- `POST chapters/1/generate` 首次经网关走通（此前只能 curl 内部测），新文档正确进《人骨拼圖》文件夹。
- **`archive`/`purge` 没对真实历史数据执行**：一次性不可逆（`archive` 软删当时无反悔 UI，`purge` 真物理删）；底层逻辑有 `notecore` 单测、HTTP 层薄直通，代码层信心够，但“没事先问用户就拿设备上的真实历史数据做不可逆操作”本身不该做，留白等用户自己点。

**补一版（同日，用户当场反馈）**：用户看到“✓ 已导出到 vault”追问“vault 在哪里”——落在设备自己的 `$XDG_DATA_HOME/notes/vault/`，无 SSH 根本够不着。用户预期朴素：像正常网页导出那样直接给浏览器下载（存哪由浏览器下载设置决定，无需造“指定目录”界面）。排查发现一个此前没暴露的缺口：网关 `proxy::forward` **只转发 `Content-Type`，其余响应头一律丢弃**（之前证书下载能触发浏览器下载，是因为该端点由网关自己处理、不经代理层）。修法：`proxy::forward` 额外转发 `Content-Disposition`（只这一个，不给后端夹带 `Set-Cookie` 之类的口子）。`note-serve` 新增 `GET /books/{uuid}/chapters/{idx}/export.md`：与 `POST .../export` 读同一份 `notecore::export::export_chapter_md`，当场 `Content-Disposition: attachment` 吐出（不从落盘文件读，避免“盘上是不是最新”的疑问）；文件名走 RFC 5987（`filename*=UTF-8''…`，`filename=` 给非 ASCII→`_` 的兜底），纯函数 `content_disposition()` + 2 个单测。网关「导出 md」按钮：`POST` 落盘（保留，原给 `notes pull` 用；该 host CLI 已砍）成功后 `window.open()` 该 `GET`。真机验证（经真实网关）：响应头带真实章名的 RFC 5987 编码、`Content-Type: text/markdown; charset=utf-8`、内容与落盘一致；本章无内容时走 404 JSON 且无 `Content-Disposition`（没破坏原错误路径）。

离线：`cargo test --workspace` 93→95 个零警告。

## 03s｜三期：砍掉“分区”，条目按页序平铺（2026-09-08，真机验证通过）

用户拍板砍掉从首期就有的“分区”概念：AI 触发早就是 `Entry.ask_ai`/`question`（§03n），分区兼职的“笔记本排版分组”也不要了——条目一律按页序平铺，格式差异全靠 `Style`（正文/无序/有序/待办，来自 7 种手写约定的 OCR 识别），AI 问答产出的条目只是普通 `Body` 条目。

**移除范围**（牵连三层）：

| 层 | 删了什么 |
|---|---|
| `notecore::model` | `Section`、`default_sections()`、`Book.sections`、`Entry.section`、`Book::section_id_for_name()` |
| `notecore::marker` | `Marker::Section` 变体（`## 文字` 一度不再识别，§03t 又按用户要求恢复为覆盖 `subhead`；`### 文字` 不受影响） |
| `notecore::geom` | `has_underline()`——从没被 `ingest_doc`/`merge_page` 接上过的旧死代码，分区没了一并清掉 |
| `notecore::project`/`export` | 不再按分区分组、无“未分区”兜底段，整章活条目按页序平铺；`fingerprint_chapter` 哈希输入相应精简 |
| `ink-serve` | PATCH 的 `section`/`sectionHint` 与 `PUT /books/{uuid}/sections` 整个删掉；保留 `subheadHint`（`### 小节` = `Entry.subhead`） |
| 网关「整理」 | 分区编辑折叠块、每条的分区下拉 |

**兼容**：旧条目库 JSON 里落盘的 `sections`/`section` 字段被 serde 静默忽略，无需迁移（真机用人骨拼圖的带旧字段真实数据验证过）。

**真机验证**（经真实网关）：旧格式条目库正常反序列化、响应里两字段消失；`PUT .../sections` 已 404；重导第 2 章 `.md` 里不再有 `##` 分区头；重生成第 2 章设备笔记本走通。离线 94 个测试零警告（`section_id_for_name` 测试删除）。

## 03t｜「整理」区四点反馈：模型预置下拉 + 移到管理台、回收站显内容、条目卡片重设计（2026-09-08，真机验证通过）

> 本节的模型管理细节已被 §03u（四厂商）、§03v（两级下拉）、§03w（provider 兜底）取代，**当前以 §03v/§03w 为准**；卡片布局后续仍在改，最新看 §03ad。

1. **「转写」折叠层看不懂**：起初以为是文案问题，补了一句说明；下一轮（§03u）用户点破根本问题后整块删除。
2. **模型管理搬进「管理」tab**：用户点出两个问题——不该待在笔记专属的「整理」区（以后可能不止笔记线用模型）；不该让用户自己填 baseUrl，模型该是下拉。落地：`transcribe-serve`/`mind-serve` 各加一张 `Preset` 表（`id`/`label`/`model`/`baseUrl`）；`PUT /config` 新增 `preset` 字段，查表**原子**设置 `model`+`base_url`（不会出现“model 换了 baseUrl 忘换”），未知预置名直接拒绝；`"custom"` 是转义阀，才退回手填；`GET /config`/`/status` 加 `presets` + `activePreset`（服务端算好，不匹配算 `"custom"`，前端不猜）。**key 语义改了**：脱敏显示已保存 key 后**只剩「删除」**（原设计“看着像能编辑一个看不见的旧值，其实一保存整个覆盖”会造成“我没改怎么变了”的困惑）；没有 key 时才有输入框+保存。网页新建通用 `mountModelPanel()`，视觉/文字两张卡片共用一套渲染交互。
3. **回收站不再是纯按钮**：「整理」拆出第三个子视图「🗑 回收站」，真列出 `Skipped`/`Revoked`/`Archived` 条目（页码、章节、原文或转写文本）；「清空回收站」挪进去，点之前取真实条数写进确认文案，空则直接提示“没什么可清”。**零后端改动**（`GET /books/{uuid}` 本来就带全部条目）。
4. **条目卡片重设计**：新 `.entry`/`.entry-head`/`.entry-body`/`.entry-crop`/`.entry-main`/`.entry-quote`/`.entry-text`/`.entry-ops`/`.entry-ask`/`.entry-answer` CSS，拆成五块：手写裁图、勾画原文引用、转写/校对文本框、样式与去处控制、问 AI 区；AI 回答不再折叠。响应式只用一条 `@media(max-width:30em)`，手机/桌面同一份 DOM。

**真机纠错（用户当场指出一处误删）**：删 `Marker::Section` 时把 `## 文字` 这个**手写标记本身**也当死代码删了——用户指出该继续识别，只是不再驱动“分区”。问清落点后：同 `###` 一样覆盖 `Entry.subhead`，不分层级；`split_leading_marker` 判据从“≥3 个 `#`”放宽为“≥2 个 `#`”，`Marker::Subhead` 一个变体同时接住 `##`/`###`。**教训**：删除一个数据结构时，要把“数据结构本身”与“触发它的手写标记语法”分开看，后者可能有用户已养成的书写习惯。

**真机验证**（经真实网关）：`GET /status` 的 `presets`（视觉/文字表分别核对）+ `activePreset`（设备上手动改过的 `qwen3-vl-plus` 正确识别为已知预置，没误判 `"custom"`）；`PUT {preset}` 原子切换并切回工作配置；不存在的预置名 400 且配置不变；回收站真实数据：10 条 `revoked` + 1 条 `skipped`。**前端可视渲染未经人眼确认**（无浏览器渲染工具；只用 `strings` 确认新元素 id 被编进二进制）。离线 100 个测试零警告。

## 03u｜「整理」区第二轮反馈：批量勾选、模型预置横跨四厂商、回收站可恢复（2026-09-08，真机验证通过）

> 本节的“批量勾选工具栏”已在 §03ad 整个删除；“四厂商 + key 按厂商分存 + 用量分账”仍是现行设计（下拉形态以 §03v、provider 兜底以 §03w 为准）。

![模型管理：预置表、key 分厂商存、用量分账](diagrams/model-config.svg)

用户继续用了一轮 §03t 的重设计，又提四点，这次是更深的业务逻辑改动。

**① 删掉「转写」折叠层**：用户点破——「浏览」页已决定一条批注要不要转笔记（点「转入笔记」才转 `Pending`），“转入整理就该自动识别转换了”，折叠层里“立即转写待转写条目”/“重试失败”两个批量按钮是多余入口，会让人误以为不点它们转写就不跑。整块删除；`auto`（合书自动转写）开关挪进「管理」tab 的模型面板（`mountModelPanel` 加 `showAuto`）；单条卡片的“重转”是唯一失败重试入口。

**同一条反馈里“两个下拉去掉，文本规则由 md 符号对标至 rm 笔记符号＝手写识别符号”**：
- **样式**：`notecore::marker::split_leading_marker`（行首 `-`/`1.`/`口`/`##`/`### `→样式/小节）原本只在转写草稿写回时生效，网页手动改字走另一条不认这套规则的路。新增 `Entry::apply_marked_text`，把规则接到 `PATCH text`：打 `- 查作者` → 剥掉标记、`style` 自动 `Bullet`；打 `##`/`###` → 覆盖 `subhead`。与手写识别同一套约定，不用另记“打字该选哪个下拉”。卡片上只留只读样式徽章。
- **去处**：三选一不是删掉不管——AskUserQuestion 给了三个方案，用户选**紧凑图标循环按钮**（一个按钮点一下在三态间循环，功能完整，手机比下拉好点）。
- **“生成笔记本/导出 md/重转/不要了应先勾选多条后操作”**：当时做了每条勾选框 + 顶部 sticky 工具栏（**§03ad 已删**）。踩到的真实坑：批量下载多章 md 时，`await` 后循环里连续 `window.open()`，第一个之后会被浏览器当成“非用户直接触发”拦掉——改成先导出完，再摆出每章的下载按钮，让用户自己点（真用户手势）。
- **失败条目标红**：「整理」渲染前查一次 `/api/transcribe/status`，按 `book`+`id` 匹配失败清单，命中的卡片加 `.entry-failed`（红边框）、按钮变“重转失败”（红色描边）。

**② 模型管理彻底重做——从一家厂商扩到四家，key 按厂商分开存**（用户要 DeepSeek/豆包/Gemini/ChatGPT，且要各模型的用量/花费）。先想清一个原来没暴露的架构问题：原先全局单个 `api_key` 槽位，四家共用会出现“切到 OpenAI 却把 DashScope 的 key 发过去”，或每切一次要重新粘贴。改法：
- `Preset` 加 `provider`（`dashscope`/`openai`/`gemini`/`deepseek`）；`api_key: String` 换成 `keys: BTreeMap<厂商, key>`；`key()`/`key_masked()`/`key_source()` 按“当前预置所属厂商”查表，`apiKey`/`clearKey` 存/删对应厂商格。同厂商换模型不用重粘；切到没配过 key 的厂商就显示“没配 key”，不借别家的（`DASHSCOPE_API_KEY` 环境变量兜底也限定 `provider=="dashscope"` 才生效）。
- **预置表模型 id/baseUrl 是易变信息**：2026-09-08 过 WebSearch/WebFetch 核实官方文档（OpenAI `developers.openai.com/api/docs/models`、Gemini `ai.google.dev/gemini-api/docs/openai`、DeepSeek `api-docs.deepseek.com/quick_start/pricing`），结果与核实日期写进 `vendorcfg` crate 文档。**当前代码里的表**（以 `services/*/src/config.rs` 为准）：

| 厂商 | 视觉表（transcribe） | 文字表（mind） |
|---|---|---|
| DashScope | `qwen3-vl-plus` · `qwen-vl-max` · `qwen-vl-plus` | `qwen-plus` · `qwen-max` · `qwen-turbo` |
| OpenAI | `gpt-5.6-terra` · `gpt-6-astra` | `gpt-5.6-luna` · `gpt-5.6-terra` |
| Gemini | `gemini-3.8-flash` | `gemini-3.8-flash` |
| DeepSeek | `deepseek-flash`（V4.1 Flash，原生多模态） | `deepseek-flash` · `deepseek-v4-pro` |

  > ⚠ **2026-09-22 复核（WebSearch + 官方 pricing 页）**：DeepSeek 官方现行模型名为 `deepseek-flash`（V4.1-Flash，原生多模态、1M 上下文）与 `deepseek-v4-pro`；旧名 `deepseek-v4-flash`、`deepseek-v4-flash-vision-exp` 仍被接受但对应模型已下线、请求由 V4.1-Flash 承接并按 Flash 计价；自 2026-09-14 起 `deepseek-v4-pro` 也暂由 V4.1-Flash 承接，直到 V4.1-Pro 上线。**已据此更新预置表**：文字表 `deepseek-flash` + `deepseek-v4-pro`（标签注明暂由 Flash 承接），视觉表 `deepseek-flash`；老配置里存的 `deepseek-v4-flash` / `deepseek-v4-flash-vision-exp` 启动时经 `vendorcfg::remap_retired_preset` 自动迁到 `deepseek-flash`（自填单价一并搬）。OpenAI 的 GPT-5.6 三档（Sol/Terra/Luna）与 GPT-6 Astra（2026-09-03 发布，先向 Trusted Access 企业开放）确有其名，`gpt-6-astra` 对普通 key 可能尚未开放（未改）。OpenAI/Gemini/DeepSeek 三家至今**只验证了配置层，没有真实 key 走过实际调用**；DeepSeek 视觉输入用新模型名是否沿用同一 `image_url` 请求形状，**未真调验证**。

- **豆包（火山方舟）没收进预置表**：它的“模型”是账号自建的推理接入点 ID（`ep-xxxxxxxx`），不是通用固定字符串，硬填占位名等于给出“已知能用”的假承诺；要接豆包走“自定义”，baseUrl 填 `https://ark.cn-beijing.volces.com/api/v3`，model 填自己的 Endpoint ID。
- **老配置迁移**：老的单一 `model`/`baseUrl`/`apiKey` 三件套作为只读迁移字段（`#[serde(skip_serializing)]`）接住，启动时 `migrate()` 按老 `model`+`baseUrl` 匹配预置（匹配上设 `preset`，否则 `custom` 并保留 `customModel`/`customBaseUrl`），老 key 存进匹配厂商的格；迁移后立即落盘一次。**真机验证（本轮最关键）**：设备上两份真实配置——transcribe 原 `qwen3-vl-plus`（在新视觉表里）正确匹配回预置；mind 原被手动改成 `qwen3-vl-plus`（**不在**文字表里，因为是视觉模型）正确落 `custom` 且 `customModel` 保留；两边 `keyMasked` 迁移前后一致，**已保存的 key 没有丢**。
- **用量按模型分账**：`ledger.rs` 的 `Usage` 改成 `by_model: BTreeMap<模型 key, ModelUsage>`（key 是预置 id 或 `custom:<model>`）。**花费不做官方定价表**（第三方定价比模型 id 还易变，写死一份容易在用户不知情时把“预估花费”做错）——用户在面板自己填“每 1K token 输入/输出单价”（`prices: 预置 id → Price`，缺省 0＝不计费）；`GET /status` 的 `usageByModel`（预置表全量 + 出现过的自定义模型）每行带 `costEstimate`，**没填单价是 `null` 而非 `0`**，界面上与“填了 0 元”区分。

**③ 回收站可恢复**（用户问“只能看？能恢复吗？书里已擦除的还能恢复吗？”）：
- `Entry::restore(&mut self, now)`：`Skipped`/`Revoked`/`Archived` 都能恢复，落点按已有内容倒推（不额外存“删除前状态”）——`Skipped` 永远回 `Mined`；`Revoked`/`Archived`：校对文本在→`Reviewed`，没校对有草稿→`Draft`，只有手写未转写→`Pending`，什么都没留→`Mined`。`POST /entries/{id}/restore`，非终态拒绝。
- **“已擦除还能恢复吗”——能，但要讲清恢复的是什么**：裁图（自渲染后是独立文件）、勾画原文（`Quote`）、转写草稿、校对文本，在条目变成终态那一刻起就是条目库里的静态存档，与设备页面当前有没有笔迹脱钩；恢复找回的是“这条已存内容”，**不是让笔迹重新出现在设备页面**。网页文案写明，`Revoked` 条目多一行提示。
- 网页每条「恢复」+「全部恢复」（与「清空回收站」并排：“要回来”与“真删掉”两个反方向操作）。

**真机验证**（经真实网关）：两份真实配置迁移（见上）；对一条 `revoked`（有草稿无校对文本）调 `restore` → `Draft`；`PATCH {text:"- 测试行首标记自动判样式"}` → `style` 自动 `bullet`、`- ` 被剥；`PATCH {text:"## …"}` → `subhead` 被覆盖、样式不变；`destination` 循环并切回 `both`；`PUT {price}` 后 `usageByModel` 对应行 `price` 更新并清零复原；五个服务 `active`、`NRestarts` 全 0；设备首页 HTML 含新前端标识符。**仍未闭环**：只有 DashScope 真实调用过；前端可视渲染未经人眼确认。

离线 112 个测试零警告（notecore 40→42、transcribe 12→18、mind 13→17）。

## 03v｜§03u 真机上手立刻反馈两个 bug + 模型两级下拉（2026-09-08，纯前端修复）

| 问题 | 根因 | 修法 |
|---|---|---|
| **编辑区改字不落盘** | 竞态，不是没发 PATCH：`textarea` 的 `onchange`（失焦才存）发出保存是异步的；用户改完字紧接着点旁边的按钮（去处循环/重转/问AI），按钮收尾的整页重画（`book=await j(...);renderBook()`）几乎同时跑，若保存响应未回，重画读到服务端旧文本，把编辑“盖”回去 | 加 `pendingText`（entry id → 最新未确认值，`oninput` 记、`onchange` 确认后清）+ `flushPendingText()`；所有会重拉数据整页重画的动作（去处循环、单条/批量重转、问AI、批量归档/恢复/生成/导出、浏览态转入/跳过、清空回收站、重扫、切书）之前统一先冲一遍 |
| **点重转列表区闪一下** | `renderBook()` 清空时机：先 `chaps.innerHTML=''`，再 `await` 查转写失败清单，最后拼卡片——网络这段列表是空的 | 清空挪到 `await` 之后、紧挨同步重建，中间不留空档 |

**顺手同批改的**：模型面板下拉从“一个框塞七八个跨厂商模型”改成**两级**——先选厂家（DashScope/OpenAI/Gemini/DeepSeek/自定义），第二级只列该厂家模型；选厂家立即原子切到该厂家第一个模型（不二次确认），选“自定义”才露手填框。「管理」tab 的模型管理卡片从并排两栏改成上下堆叠占满宽度（每张卡片加了用量表/单价后内容变多，`min-width:18em` 两卡一排局促）。

**验证**：部署后 5 服务 `active`、`NRestarts` 全 0；设备首页 HTML 含新标识符（`pendingText`/`flushPendingText`/`data-vendor`/`PROVIDER_NAMES`/`modelbox`）；`text` PATCH 服务端行为没变，重跑一次真机 PATCH 无回归。**两个 bug 是否真消失没有人眼确认**——竞态和闪烁本质是“浏览器里发生的事”，只能确认代码时序对了。纯前端改动，Rust 契约不变。

## 03w｜点重转/提问弹出状态+消耗 + 真机顺带挖出的 provider 隔离 bug（2026-09-08，真机验证通过）

**弹出状态 + 消耗**（用户：“点击重转应该弹出状态及当前消耗”）：`transcribe-serve::worker::transcribe_entry` 原只返回文本、token 数只喂账本就丢了；改成返回 `Transcribed{prompt_tokens,completion_tokens}`，`RunReport` 新增同名两字段由 `run_once` 累加（强制单条＝那一次调用的消耗，批量＝整轮累计），单条转写端点响应体带上。`mind-serve::worker::ask_entry` 同款（`Answered`）。前端：「重转」「提问」点击后原地显示“转写中…/提问中… → ✓ 完成·token 入X出Y / ✗ 失败原因”，**停留 1.5 s 才整页重画**——状态文字设完立刻重画会连同 DOM 一起冲掉，用户来不及看见；成功/失败两支都要等。

**真机顺带挖出的 bug（provider 隔离）**：mind-serve 预置表是纯文字模型，没收视觉模型 `qwen3-vl-plus`；老配置迁移时这个型号匹配不上任何预置，`migrate()` 把 key 存进字面意义的 `custom` 格。用户后来在网页切到同厂商真实预置 `qwen-plus`，查 key 去的是 `dashscope` 格——找不到，`/ask` 报“未配置 API key”，尽管是同一个 DashScope 账号的同一把 key。**根因**：把“key 归哪个厂商”与“这个具体型号在不在预置表里”绑得太紧——一把厂商 key 对该厂商旗下所有模型都该通用。**修法**：新增 `provider_for_base_url()`（比对四个 baseUrl 常量），`provider()` 在“预置精确匹配”落空时按 baseUrl 兜底认厂商；`migrate()` 直接复用 `self.provider()`，保证存 key 与查 key 永远同一套判断。transcribe-serve 没触发过此坑（预置表恰好收了老型号），同款兜底一并加上。（此后视觉/文字两服务这套逻辑收进共享的 `vendorcfg`，§03ac。）

**真机验证**（经真实网关）：把被切乱的 key 用 `PUT apiKey` 点回正确的 `dashscope` 格，再切回 `custom`（同一 DashScope baseUrl）——`hasKey` 仍为真（部署新二进制前重试 `provider` 仍是 `"custom"`，证实是二进制没更新；重新部署后变 `"dashscope"`）；`POST /ask` 真调用成功，`promptTokens:111 completionTokens:75`；强制单条转写 `promptTokens:316 completionTokens:13`（真实 DashScope 返回，非缓存/占位）；5 服务 `active`、`NRestarts` 全 0。

离线：transcribe 18→19（新增 `forced_single_entry_run_reports_only_that_calls_tokens`）；mind 新增 `migrate_unmatched_model_but_known_provider_base_url_shares_key_with_real_presets_of_that_provider` 锁定回归；notes 共 114 个测试零警告。

## 03x｜生成/导出去重复 + 同步后自动收起 + 回收站显示去处（2026-09-08，真机验证通过）

> 本节的“批量工具栏”“同步后自动收起 + 显示已同步章节开关”布局已被后续替换（§03aa、§03ad）；**同步状态追踪机制本身（指纹、`/sync`、`ExportState`）仍是现行设计**。

**① “全选后生成笔记本/导出 md 是不是重复了”**——是。两个操作的后端本来就“整章一起投影”（`project_chapter`/`export_chapter_md` 只按章序号取该章全部活条目，不看选中了哪几条），塞进批量勾选工具栏后“先勾选再点”成了绕远路。改法：两按钮挪回章头，工具栏只留重转（对选中里有裁图的生效）与不要了（批量归档）——这两个才是逐条起作用、勾选有意义的操作。

**② “生成完成后是不是应该移出列表”**——加了一整套**同步状态追踪**（本轮最大的一块）：

| 部件 | 内容 |
|---|---|
| `notecore::export::fingerprint_chapter` | 与 `project::fingerprint_chapter` 算法一致（标题+各条目 id/样式/文本/勾画/回答拼起来算 FNV1A），区别是 `live_entries` 收 `wants_obsidian()` 而非 `wants_notebook()`——两条投影各看各的活条目集合，指纹必须分开算（同一条目改了去处，两边“算不算变了”的答案可能不同） |
| `note-serve::export_state.rs` | `ExportState`/`ExportRecord{fingerprint, exported_at}`，照抄 `NotebookState`。导出 md 原先无条件全量重写，现在也有“指纹没变就跳过”的纪律；`export_chapter` 返回三态 `Written`/`Unchanged`/`Empty`（与 `publish::ChapterOutcome` 同构），内容变空时清旧记录（否则「整理」会一直显示“已同步”，与“这章已无内容”不符） |
| `GET /books/{uuid}/sync` | 对每一章算“当前活条目指纹 vs 上次成功生成/导出记的指纹”，输出 `notebookNeeded`/`notebookSynced`/`obsidianNeeded`/`obsidianSynced`（`xxxNeeded` 区分“这章没有条目要这个去处”与“投过了且仍同步”两种不同含义的 true，前端据此决定显示 📓/Obsidian 徽章）。后续还带 `notebookGeneratedAt`/`obsidianExportedAt`（§03ad 的 `everExported` 用它） |
| 前端 | `refreshSync()` 取整本同步状态；章头显示同步徽章；所有会改条目内容/去处/状态的动作之后补 `refreshSync()`，徽章不滞后。（当时全同步章节默认收起 + “显示已同步的章节”开关，已被 §03aa 取代） |

**③ “回收站是否显示此笔记导出到哪里”**——每条显示去处徽章（`DEST_ICON`，与去处循环按钮同一套图标）+ 所在章节当前同步徽章。**如实标注局限**：章节维度徽章反映“这章大致什么状态”，不是“这条当初有没有被打进上次生成”——条目一旦归档/撤销就已不在活条目集合里，架构上无法逆推它历史上是否被包含。

**顺手**：Obsidian 图标从占位 🔗 换成简化多面体 SVG（紫色宝石形，配色贴近品牌色；有意不精确描摹官方 logo，商标图形不该随手照抄）。

**真机验证**（经真实网关）：`GET /sync` 对人骨拼圖：首次查两个 synced 均假；`generate`+`export` 后变真；第二次 `export`（内容没变）响应 `status:"unchanged"`——没白重写；5 服务 `active`、`NRestarts` 全 0。**一个没能干净复现的点（如实）**：想单独验证“改 `destination` 后对应指纹立刻变”，但该书被本轮反复测试、`auto` 常开，测试期间在没直接改动时也观察到指纹漂移（疑似后台自动转写摸过别的条目），单变量条件不干净；此结论由离线单测 `notecore::export::tests::fingerprint_changes_on_content_change_and_is_scoped_to_obsidian_destined_entries` 覆盖，真机未单独复现。

离线：notecore 42→43、note-serve 13→16；notes 共 118 个测试零警告。

## 03y｜生成笔记本/导出 md 合并成「同步本章」一个按钮（2026-09-08，真机验证通过，纯前端）

> 按钮后来改名「推送本章」（§03aa）。

§03x 刚把两按钮挪回章头，用户接着指出更根本的一层：它们与条目已有的「去处」字段重复——去处早就决定了这章该不该落设备笔记本、该不该落 Obsidian，分两个按钮让用户再选一遍“点哪个”是多余的，“一个『导出』按钮就解决了”。

**改法**：不动后端接口（`generate`/`export` 两端点本来就各自基于 `destination` 过滤，没有对应条目就是正常的 `Empty`）。前端合并成一个按钮，依次调 generate + export，按两边各自 outcome 拼消息（“✓ 笔记本已更新 · ✓ md 已导出”，谁没变化谁不提，两边都没变化是“跟当前去处对应的内容都已经同步，没有变化”），md 写出后照旧触发浏览器下载。真机：5 服务 `active`、`NRestarts` 全 0，新按钮标识符生效；纯前端，服务端契约没变。

**用户当场追问的两个“数据会不会出问题”（都不是代码改动，是把设计讲清）**：
1. **“已同步章节不删除，数据会不会无限膨胀”**——不会：隐藏已同步章节是纯前端显示过滤；`NotebookState`/`ExportState` 按“章”存一条记录、`set()` 覆盖不追加，大小恒定，不随点击次数增长。
2. **“书被删了笔记会不会也被删”**——不会：`ink-serve::ingest::revoke_stale()` 检测到源书被删/进回收站时只把条目标 `Revoked`（软删终态），全项目搜过没有任何路径会因源书消失物理删除条目库数据或已生成/导出的文档（设备笔记本、Obsidian md 都是独立文件）；`Revoked` 条目可在回收站「恢复」。

## 03z｜章节默认折叠：长列表不用划到底（2026-09-08，纯前端，真机验证通过）

> **已被 §03aa 取代**（折叠机制 `expandedChapters` 整个换成“双层 tab + 只显示一章”）。留下结论与动机。

§03x 的“全同步章节自动收起”只解决“处理完的章节别占地方”；用户把问题问得更具体：“1 章 10 条，10 章就 100 条，手机划几分钟才到底”——**还没处理完**的章节条目一多照样没边，是纯粹的信息密度问题。当时改法：每章卡片默认只露头（标题/条数/同步徽章/按钮），条目本体折叠，点章头展开（只一章时直接展开）；展开状态记 `expandedChapters` Set，只是本地 DOM 显隐，不重拉数据，切书才清。

用户当场追问“显示已同步的章节，笔记一多会不会太长”——不会：列表只属于当前选中的这本书、且只列实际写过批注的章节（无批注的章节不进 `groups`），上限是“这本书有多少章你动过笔”，是结构性的一次性的量，不是跨时间累积的量（与“不清空就一直涨”的回收站不同）。

真机：新标识符（`expandedChapters`/`data-chaphead`/`data-caret`）生效，5 服务 `active`、`NRestarts` 全 0。纯前端。

## 03aa｜「未导出/已导出」双层 tab 取代复选框 + 按钮改名去歧义（2026-09-08，纯前端，真机验证通过）

> 「整理」页的当前布局以 §03ad 为准，图见 [`organize-page.svg`](diagrams/organize-page.svg)。

用户提议：与其用“显示已同步的章节”复选框过滤一条长列表，不如「未导出/已导出」两个顶层 tab，tab 下章节再做成第二层可点标签，点哪章只看哪章——比 §03z 的折叠更彻底（折叠只是不用看见内容，滚动轴还在；双层 tab 让“任意时刻屏幕上最多一章内容”，滚动本身消失）。讨论确认：① “章节标签只显示有笔记的章节”本就是现状（分组从活条目反推，零条目章节不进 `groups`）；② “每条笔记显示导出过哪种”受限于同步状态只精确到**整章**（`fingerprint_chapter` 对整章活条目拼一个哈希；逐条指纹收益存疑，「推送本章」本就按章触发），退而求其次把章节级徽章也贴一份到每条笔记行；③ 顺带治两个按钮名的歧义。

**改法**（`gateway/ui/app.js` 前身，纯前端不碰后端契约）：
- 复用 `fullySynced`（`notebookSynced && obsidianSynced`；`/sync` 对“没有条目要那个去处”恒真，因此**没有第三态**）把章节 key 分成 `pendingKeys`（未导出）/`syncedKeys`（已导出），顶层 `#nexporttabs` 两个 tab；`nshowsynced` 复选框与提示行删除。
- `#nchapters` 拆成 `#nchaptertabs`（第二层章节标签，只列当前 tab 下章节）+ `#nchapterbody`（只渲染选中章那一张卡片）；`expandedChapters` 折叠机制被 `exportTab`/`selectedChapter` 两个状态取代。
- **自动跳下一章是设计的自然结果**：「推送本章」把章从未导出变成已导出后，它从 `pendingKeys` 消失，`selectedChapter` 校验发现选中章不在可见列表就选第一个——“一章一章处理完”的工作流的副产物，不是额外特性。
- 两个 tab 都空显示空状态（未导出空＝“🎉 都同步了，没有待处理的章节”；已导出空＝“还没有章节完成同步”）；“未归章”（`chapter==null`）没法调 `/sync`/`generate`/`export`（都按章 idx 定位），永远在未导出 tab、无同步徽章/按钮。
- `.entry-head` 追加章节级 `syncBadges(s)`。
- **按钮改名**：「同步本章」→「推送本章」（“同步”暗示双向，其实只单向推）；「重转」/「重转失败」→「重新转写」/「转写失败」（与「浏览」里另一动作「转入笔记」共享“转”字易混）。

CSS：章节标签条复用 `.subnav` 视觉（`subtabs()` 用 `$('.subnav',sec)` 单选第一个匹配，不受额外 `.subnav` 影响），但**不给新内容容器套 `.subpanel`**——该 class 被 `subtabs()` 用 `querySelectorAll` 收集、按下标对应最外层「浏览/整理/回收站」三子视图，混进去会打乱下标对应。

**验证**：`node --check` 与 gateway 测试无回归；部署后 `active`、`NRestarts=0`，新标识符（`nexporttabs`/`nchaptertabs`/`nchapterbody`/`推送本章`…）生效、旧的 `nshowsynced` 消失；用人骨拼圖真实 `/sync` 数据核对分组（2 条活条目都在第 2 章且完全同步，前端会分到「已导出」）。**tab 点击/单章切换的真实交互没有人眼确认。**

## 03ab｜真机反馈两个真 bug：点落点后画面跳章 + 徽章显示跟本条无关的去处（2026-09-08，纯前端，真机验证通过）

用户真机点验 §03aa 后报了两点，查证都是真 bug，不是数据错乱：

| 现象 | 根因 | 修法 |
|---|---|---|
| **点条目自己的「去处」按钮，画面跳到别的章节** | §03aa 的“选中章节必须在当前 tab 可见列表里”校验，本为「推送本章」成功后“自动跳下一章”设计，却对所有改变章节同步状态的动作（含编辑一条的去处这种小动作）一视同仁——用户刚编辑完当前章，视图立刻跳到不相干的章节，像“数据错乱” | 默认“**跟随**”：选中章仍有活条目就继续显示它，只把顶层 tab 高亮切到它现在实际所在那边；只有两种情况真换章：显式点 tab（`selectedChapter` 先置空走原逻辑）、显式推送完成（`renderBook({advance:true})`，只有「推送本章」成功回调这么传） |
| **徽章显示跟本条无关的去处**（用户：“p.5 我只导了 Obsidian，实际显示两者都有”） | 每条笔记行的同步徽章直接复用章头那份**整章聚合**状态；同章里有的条目只要 Obsidian、有的要设备笔记本，两行显示同一份“这章缺什么” | `syncBadges` 新增 `only` 参数，条目行传自己的 `destination`，按 `wants_notebook`/`wants_obsidian` 过滤掉无关徽章；章头不传仍显示整章聚合；回收站列表同样处理 |

验证：`node --check` 与 gateway 测试无回归；部署后 `active`、`NRestarts=0`，新标识符（`advance:true`/`syncBadges(s,dv)`）生效。视觉/交互仍无人眼确认，这轮靠手动追踪 `renderBook` 分支路径走查 + 部署健康检查。纯前端。

## 03ac｜合理使用设计模式消除重复代码：新增 vendorcfg 共享 crate + note-serve 的 ChapterStore\<T\>（2026-09-08，真机验证通过）

用户要求“合理使用设计模式消除重复代码、合理抽象解耦”。逐文件比对整条线约 8500 行后，两处真正跨文件重复达到值得抽象的规模（`backend.rs` 那种“看着像”但有真实业务差异的重复此前已判“几十行胶水不值当抽公共 crate”，这次维持原判；**注意 2026-09-20 其网络传输部分又抽进了 `vendorcfg::chat`**，见下）：

| 重复处 | 抽象 | 只抽行为、不抽数据结构 |
|---|---|---|
| `transcribe-serve`/`mind-serve` 的 `config.rs`（~85% 重叠）+ `ledger.rs`（~90% 重叠）：预置表选择、key 按厂商分格存取（baseUrl 兜底认厂商）、老配置迁移、PUT /config 的 PATCH 语义、对外 JSON 整形、用量按模型分账 | 新 `notes/crates/vendorcfg`：`preset.rs`（`Preset`/`Price`/`KeySource` + 厂商 baseUrl 常量 + `resolve_provider`/`resolve_key`/`migrate_legacy`/`apply_common`/`public_json`）+ `usage.rs`（`ModelUsage` + 泛型 `UsageBook<Extra>`/`Ledger<Extra>`，`Extra` 是“有没有一轮报告”：transcribe 用 `RunReport`，mind 用 `()`）。仍不同的只有各自预置表数据、transcribe 独有的节流四件套与 `RunReport` | `TranscribeConfig`/`MindConfig`/`Usage` 仍各自定义，serde 落盘形状**逐字节不变**，方法体委托给 `vendorcfg` 自由函数——设备上已有用户配好的 key 与累计真实用量，磁盘格式零风险优先于“抽得更彻底” |
| `note-serve` 的 `notebooks.rs` + `export_state.rs`：同一个“按书一文件、按章存一条记录”骨架 | 泛型 `chapter_store.rs::ChapterStore<T>`，两原文件只剩记录类型 + `pub type XxxState = ChapterStore<XxxRecord>` 一行别名；`export_state` 独有的 `clear()` 提到泛型里 | 外部调用方零改动 |

**后续追加**（代码现状，非当时记录）：`vendorcfg` 现有五个模块——`preset`/`usage` + 2026-09-15 补的 `preset::VendorConfig` trait（两边共有的只读派生方法 `provider()`/`model()`/`key()`/`public()` 等，各自 Config 实现六个字段访问器即得全部默认方法）+ `cell.rs`（`ConfigCell<C>`：运行期配置的“内存副本 + 落盘”盒子，收编两边逐行相同的“读→迁移→0600→落盘一次 / PUT 时锁→克隆→apply→存盘→换入”）+ `chat.rs`（OpenAI 兼容 `POST {base_url}/chat/completions` 的传输与应答解析 `ChatReply`/`agent`，2026-09-20 收编，业务 trait `Vision`/`TextModel` 与请求体拼装仍各服务自己的）+ `truncate_chars`（§03aj）。

**磁盘格式零风险验证**：每处改动都拿真机 2026-09-08 实测采样的真实文件形状（key/内容脱敏，字段名/大小写/数值原样）写成回归测试。**真机验证**（经真实网关）：三服务 `active`、`NRestarts=0`；`GET /config` 的 hasKey/keyMasked/model/provider/preset 与改动前逐字段一致（真实 key `...yP5A` 没丢没错位），`usage.byModel`/`lastRun` 累计原样读出；**不止“老数据读得出来”，还验证“新数据能写进去”**：强制单条重转走完整链路（vendorcfg 解析 key/model → 视觉后端真调 DashScope → `vendorcfg::Ledger` 记账），`usage.byModel.qwen3-vl-plus` 累加正确（8→9 次、2528→2867/104→110 token），`lastRun` 更新为这次的 339/6 token。

离线：当时 136 个测试（vendorcfg 新增 12、transcribe 19→21、mind 18→20）；`clippy --workspace` 零新增警告（`migrate_legacy` 8 参数超阈值加 `#[allow]`+理由；顺手清掉 `transcribe-serve/worker.rs` 一处预置的 `sort_by`→`sort_by_key`）；aarch64-musl release 零警告。

## 03ad｜第七轮反馈：去掉批量勾选层 + 已导出改判"存在性"（2026-09-08，纯前端，真机验证通过）

> **「整理」页当前布局以本节为准**（图：[`organize-page.svg`](diagrams/organize-page.svg)）。这块是整条线迭代最频繁的，改之前先看最新一节。

用户两点：① 每条已有独立的「重新转写」「不要了」，批量勾选（复选框/全选本章/顶部隐藏工具栏）是纯重复入口；② “已导出/未导出”靠 `notebookSynced && obsidianSynced` 组合判断，编辑任意一条的内容/落点都可能让整章指纹对不上，已导出章节因一次小编辑弹回未导出——“多条数据的组合判断，实际只在乎导出过没有”。

**点 1 直接采纳**：`#npickbar`、每条复选框、「全选本章」、`picked`/`syncPickbar`/`selectedEntries` 整段删除（顺手清掉 `style.css` 里孤立的 `#npickbar`/`#npicklinks`）。

**点 2 没有直接采纳原话**：“只要推送过就永远算已导出、不再判断内容是否匹配”有信息丢失风险——推送后又编辑内容，用户会看不到“这里有新东西还没推送”，设备文件与条目库悄悄脱节。跟用户核实后按推荐方案：新增 `everExported(k)`（只看 `/sync` 已返回的 `notebookGeneratedAt`/`obsidianExportedAt` 是否非空）**只决定 tab 归属**——推送过一次就稳定留在「已导出」，不再随内容变化跳来跳去；`fullySynced`/`syncBadges` 没被替换，继续管章头/每条的 ✓/… 徽章，“有没有新改动待推送”从“决定进哪个 tab”降级成“已导出 tab 内的一个提示”，信息没丢。

验证：`node --check` 与 gateway 测试无回归；部署后 `active`、`NRestarts=0`，新标识符 `everExported` 生效、旧的 `npickbar`/`npicklinks`/`data-selall`/`data-pick` 消失；人骨拼圖真实 `/sync`：第 2 章 `notebookGeneratedAt`/`obsidianExportedAt` 均非空→判「已导出」。纯前端。

## 03ae｜去掉建夹逻辑，复用书本自己的设备文件夹（2026-09-09，真机验证通过）

用户提出：不再自动新建《书名》文件夹，笔记本直接复用书本自己已在的设备文件夹；名字用章节标题（不再带“第N章”前缀），撞名在同一文件夹范围内加数字后缀。审计坐实了 §03l 那条链路的问题：note-serve→book-serve 排队→真机 qmd 每 8 秒轮询 `Library.createCollection` 兜底建夹，fire-and-forget、无重名保护——`shelf-mkdir-agent.qmd` 自己注释都承认“重复调用会不会建出两个同名文件夹”没验证过。

**方案**：`rmsvc_core::xochitl`（当时叫 `shelf_core`）新增两个纯读函数：`parent_folder_of(dir, uuid)` 读文档 `.metadata` 的 `parent`（回收站视为查不到）；`unique_document_name(dir, folder, base_name)` 在同一文件夹内查重、撞名加数字后缀。`note-serve::publish::Uploader` 的 `ensure_folder` 换成 `parent_folder`/`unique_name` 两个方法（缺省 `None`/原样返回，方便测试桩）；`generate_chapter` 改成查书本父文件夹直传 `upload`。

**写测试时抓到的坑**：重新生成同一章时若照常 `unique_name` 去重，会把“这次要被替换、但还没入回收站队列”的旧文档也算成重名，平白多加后缀（“楔子”变“楔子 2”）——旧文档要等这次上传成功、`claim` 拿到新 uuid 后才入队。**修法：只有首次生成才去重，重新生成沿用 `ChapterRecord.visible_name` 里记录的名字**。测试 `folder_reused_from_book_and_dedup_only_runs_once_not_on_regenerate` 用“每次调用都变返回值的去重桩”钉住（重新生成时调用次数不再增加）。

**移除**：`NoteConfig::folder_name_pattern`/`folder_name()`（连带 `/status` 的 `folderPattern`）、`Uploader::ensure_folder`、`note-serve/src/mkdir.rs`。当时 book-serve 的 `MkdirQueue`/`/mkdir/add` 与真机 `shelf-mkdir-agent.qmd` 没动（涉及卸载已部署的真机注入组件，按纪律不与功能改动捆绑）；**2026-09-15 单独评估确认全仓库无其它消费方，已物理删除**（见书架白皮书），已部署的旧 qmd 设计成后端消失就静默失活，无需去设备摘除。

**离线**：`rmsvc-core` 新增 2 测（`parent_folder_of_reads_parent_field_and_treats_trash_as_none`/`unique_document_name_appends_suffix_only_within_same_folder`），note-serve 新增 1 测（去重时机）删 1 测（随字段删除）；当时 notes 164 个、shelf-core 49 个测试全绿零警告。

**✅ 真机验证通过（2026-09-09）**：拿真实《人骨拼圖》条目（临时改一条已校对文本触发重生成，验证完立刻改回，设备上无测试痕迹）——① 新文档 `.metadata` `parent=""`，落书本自己所在的根目录，不在旧《人骨拼圖》文件夹；② 8 秒轮询建夹链路没被触发；③ 重新生成沿用 `ChapterRecord` 记录的旧名字（`"第2章 第一部 一天的國王 1"`），没被误判重名加后缀；④ 旧版本入队 `book-serve` 回收站，**真机上的 `shelf-trash-agent.qmd` 几秒内把旧文档实际移进回收站**（`parent` 变 `"trash"`），“生成→入队→真机代理消费→实际软删”端到端打通。

## 03af｜单篇 markdown → 设备笔记本（"导入"，2026-09-09，后端管线真机验证通过）

> 白话：把一份现成的 `.md` 直接转成一本设备上的打字笔记本页面，独立于条目库。

**需求澄清（很关键）**：起初以为要配合“host `notes pull`（vault 拉到本机 Obsidian）”做双向同步，用户纠正——vault/云端那套需要 WebDAV/云同步机制，规模不对；这里只是“单篇 markdown 转一个 rm 笔记本页面”，专门为此建一整套 host CLI crate 是过度设计。用户还明确要求入口**不放进现有「整理」**——那里是审阅真被要求转笔记的条目（浏览态状态机驱动），与“拿一段现成 markdown 直接生成新笔记”是两件事。

**协议层限制**（xochitl 打字格式的限制，非实现选择，见 `rmv6::write` 模块文档）：

| markdown | 处理 |
|---|---|
| 行内 `**加粗**`/`*斜体*`/`` `代码` `` | 样式是段落粒度、不是字符粒度，没法半句加粗——只剥离**配对**出现的分隔符，无视觉效果；不成对的符号原样留着，不瞎猜 |
| `- [x]` 已勾选待办 | 写入器造不出勾选态，统一降级成未勾选，正文不留 “x” |
| `####` 及以上标题 | 没有第三级原生小节样式，降级成 Subheading 2（同 `###`） |
| 有序列表 | 只剥数字本身，编号由 xochitl 按“连续几个 NUMBERED 段”自动算（连续性限制早记于 `rmv6::write`） |
| 追加/合并到已有页面 | 不支持，只“生成一个全新页面”（复用 `build_page_rm` 模板整块替换） |

**实现**：`notecore::mdimport`（与 `marker` 方向相反：`marker` 从 OCR 纯文本反推行首标记，`mdimport` 从规范 markdown 直接识别块级结构），逐行处理，一行对应一段 `.rm` 段落：`#`→`HEADING`、`##`→`subheading1()`、`###` 起→`BOLD`（Subheading 2）、`- `/`* `/`+ `→`BULLET`、`1.`/`1)`→`NUMBERED`（数字丢弃）、`- [ ]`/`- [x]`→`CHECKBOX`、其余→`PLAIN`。10 个测试覆盖每种映射与边界（光秃秃 `#标签` 不算标题、标题符号剥完为空段落丢弃、checkbox 与 bullet 共享前缀的优先级、行内符号只在配对时剥离、端到端混合文档）。

`note-serve::publish::import_markdown(c, book_uuid, title, markdown)`：复用 `generate_chapter` 同一套“查书本父文件夹→去重→打包→上传→认领”（§03ae 的产物），**不写 `NotebookState`**——一次性导入，没有源数据可比对指纹，重复导入同一内容会各生成新文档（靠去重后缀区分）。返回 `(实际用上的设备文档名, uuid)`——名字可能因撞名加了后缀，调用方必须显示真名，不能回显用户输入的 `title`。端点 `POST /books/{uuid}/import-md`（body `{title, markdown}`）。

**前端**：笔记 tab 第 4 个入口「📝 导入」（与「浏览/整理/回收站」并列，不是整理页内部），标题框 + markdown 文本框 + 「生成到设备」，页面文案直接把上面的限制讲给用户。**2026-09-10 起**：交互改成“选一个 .md 文件上传”（`FileReader` 读文件，仍 POST 同一份 `{title,markdown}`）、默认隐藏，由书架「管理→实验室」的 `notesImportMdEnabled` 开关控制显示；端点/body 自接线以来没变（见书架白皮书 §03ak）。

**✅ 真机验证通过（后端管线，2026-09-09）**：真调 `POST /import-md`，传含 `#`/`##`/无序/有序/`- [ ]`/`- [x]`/行内加粗斜体的示例，落到《人骨拼圖》根目录（`parent=""`，正确复用书本文件夹）；`scp` 拉回设备真实生成的 `.rm`，用 `rmv6::page::Page::parse`（与生产同一解析器）逐段核对——9 段顺序/样式/文字对得上设计：`HEADING/"大标题"`、`BOLD(subheading1)/"小节"`、`BULLET×2`、`NUMBERED×2`（数字已剥）、`CHECKBOX×2`（`- [x]` 降级成未勾选、无残留 “x”）、`PLAIN/"普通段落，含加粗与斜体文字。"`（符号被正确剥离）。测试产物已排队软删。**前端「导入」面板本身没在浏览器里人眼点开验证。**

离线：notecore 新增 10 测（`mdimport`）、note-serve 2 测（成功路径含落进书本文件夹+去重+不写 state、书不存在提前报错不碰网络）；notes 176 个测试全绿。

## 03ag｜摄取路径"排除法"反模式复发修复：`Skipped`/`Archived` 不再被误判成 `Revoked`（2026-09-09，离线）

三维审计（代码结构/业务闭环/UI）挖出一个白皮书自己记录过的反模式在摄取路径复发：**§04 早写过“排除法（`!= X`）比允许列表更容易悄悄纳入不该要的状态”**。同日审计已把 `project.rs`/`export.rs`/三处写入口守卫都改成允许列表（`is_live_for_projection()`/`is_terminal()`），唯独 `notecore::ingest::merge_page` 与 `ink-serve::ingest::revoke_stale` 这两处“给条目打 Revoked”的判据漏改，仍是 `!= Revoked`。

**后果**：`Skipped`（“不需要”）、`Archived`（“不要了”）都是终态，却被排除法漏判——① `merge_page` 里“没被任何草稿认领的条目自动转 Revoked”（本为“笔迹被擦”设计）把还没擦掉、只是用户已否决的条目也改判 `Revoked`；② `revoke_stale`（书被删/进回收站，旧条目全标 Revoked）同理覆盖已是终态的条目。共同后果：`Entry::restore()` 看到的 status 已被改成错的 `Revoked`，跳过“`Skipped` 固定回 `Mined`”的专门规则，改走“按内容倒推”——`ink` 还留着旧值就恢复成 `Pending`，**用户明确否决过的内容被拉回自动转写队列**。

**修复**：两处改成 `!e.is_terminal()`（`Entry::is_terminal()` 是单一事实源，与三处写入口守卫用同一方法）。**有意没改 `merge_page` 里 `same_page` 匹配判据**（仍 `!= Revoked`）——它管“这是不是同一份还在原地的内容、别重复建条目”；若连匹配都排除 Skipped/Archived，笔迹没动、只是别处改动触发整页重扫时，会给同一份已“不需要”的内容重新生成一条 `Mined`（理由写在 `ingest.rs` 注释里）。这是“审计建议照单全收会引入新 bug、需要多想一层”的例子。

离线：notecore 新增 `skipped_and_archived_entries_are_not_silently_flipped_to_revoked_when_ink_disappears`，ink-serve 新增 `revoke_stale_leaves_already_terminal_entries_alone`；178 个测试全绿。**⚠ 未真机验证**：改的是被动触发的后台逻辑，难以主动构造（需真擦掉已跳过条目的笔迹、或删一本带已跳过条目的书），判据本身已离线覆盖。

## 03ah｜生成笔记本"上传成功但认领失败"加短暂重试，降低孤儿文档风险（2026-09-09，离线）

**风险**（三维审计的业务闭环一路挖出）：`generate_chapter` 里 `upload()`（设备已真实建好文档）与 `claim()`（按 `visibleName`+时间窗回查设备 uuid）是两次独立网络调用，不是事务。`claim()` 失败（常见原因是设备处理没跟上，不是真丢了）会让本次生成整体判失败、`NotebookState` 不落记录；用户手动重试时 `generate_chapter` 当成“首次生成”重走去重（`unique_name`），而设备上已有同名文档，于是新的一次拿到“标题 2”——旧的那份追踪不到，成了条目库管不着的**孤儿文档**，不会自愈。测试桩留了 `fail_claim` 字段却从没写过测试，是顺手挖出的第二个空白。

**修复**：在生产实现 `XochitlUploader::claim` 里放弃前先原地重试 4 次、间隔 1.5 s（总上限约 4.5 s，一次同步 HTTP 请求内可接受）——给设备缓冲，绝大多数场景几秒内能认领到，用户感知不到失败，也就不走不安全的手动重试路径。**这不是事务保护**：设备真卡住/离线时重试耗尽仍会失败、仍可能留孤儿，错误文案如实提示“多次重试有极小概率在设备上留下同名孤儿文档，看着重复可以手动去设备上删掉多的那份”。评估后认为完整事务化（记录待确认状态、下次生成先复用）的复杂度与触发频率不成比例。

离线：新增 `claim_failure_uploads_a_document_that_becomes_untracked_orphan_this_is_the_known_risk`（补上 `fail_claim` 空白，并断言 `upload()` 确已真实执行一次——这就是孤儿的来源）；`XochitlUploader::claim` 两条测试（`find_documents_since` 只碰本地文件，可用真实临时目录）：重试真会等到文档出现再认领（后台线程延迟 300 ms 写文件模拟设备稍慢）、耗尽后文案说明“已经重试过”；181 个测试全绿（两条真吃约 1.5–4.5 s 的重试测试是为了真的证明重试在跑）。**⚠ 未真机验证**：低概率时序窗口，真机难主动触发。

## 03ai｜消掉 InkHttp 客户端骨架重复：`SvcClient` 进 rmsvc-core（2026-09-09，离线；当时叫 `shelf_core`）

三个服务各自的 `ink.rs`（访问 ink-serve 的 HTTP 客户端）与 `note-serve::trash.rs`（访问 book-serve 回收站队列）——四处 `struct { paths, agent }`、`new()`（`ureq::AgentBuilder` 建带标准超时的 agent）、`base()`（查注册表拿 base_url，查不到报“XXX 未运行”）、`get_json()`、`enc()` 几乎逐字节相同，只有目标服务名、超时秒数、业务方法不同。

**修复**：`registry`（本就管服务发现）新增 `SvcClient`：`new(paths, service, timeout_secs)` + `base()`/`get_json()`/`post_json()` + 逃生舱 `agent()`（`transcribe-serve::crop()` 要下载原始字节，走它直接发请求）；`enc()` 收成 `registry::enc()`。四个消费者（`mind-serve::ink::InkHttp`、`note-serve::ink::InkHttp`、`transcribe-serve::ink::InkHttp`、`note-serve::trash::BookServeTrash`）内部改包一个 `SvcClient`；各自的 `EntryStore`/`TrashSink` trait、方法签名、业务错误语义不变——那是各服务自己的关注点，合并会耦合不同语义，只抽传输样板。（后来网关 `batch.rs` 也用它调 book-serve/koreader-serve。）

离线：新增 2 测（`svc_client_base_url_uses_registry_and_errors_with_service_name_when_not_running`/`enc_percent_encodes_path_segments`）；四个消费者原测试全部不变、全绿（不改可观察行为）。

## 03aj｜低优先级收尾：字符截断函数消重复 + 有意识跳过的两项（2026-09-09，离线）

三维审计低优先级清单的处理结果：

| 项 | 处理 |
|---|---|
| `trunc`/`take` 三处重复（两份字节级相同的 `trunc()` + `mind-serve::prompt::take()` 多一步 `trim()`） | 收成 `vendorcfg::truncate_chars`（1 个测试钉住“按字符数不按字节数截断”这个最易踩的坑——中文一个字不该被腰斩），`take()` 变一行胶水 |
| 网关 `proxy` 模块文档“body 流式透传”与实现不符（只有请求方向真流式，响应整体缓冲） | **只改注释、不改行为**：现有流式通道走 `tiny_http` 的 `upgrade("sse", …)` 直接接管裸 socket，专为 SSE 设计；拿去代理任意大小下载前要先确认对非 SSE（Content-Length/chunked 头协商）是否语义正确，这条低优先级项不值得承担该不确定性。评估过程写进 `proxy.rs` 新模块注释（现状见网关白皮书 §00b） |
| `mind-serve`/`transcribe-serve` 的 `OpenAiCompat` 大段重复 | **当时有意跳过**（两个文件模块注释早写明“专项专用，抽公共 crate 不值当”，审计报告自己标注为“已知情并权衡过的决定”）。⚠ **2026-09-20 已翻案**：网络传输与应答解析收进 `vendorcfg::chat`（见 §03ac 后续追加），业务 trait 与请求体拼装仍各自持有 |
| 投原生“超时误判+投完清母版”边界（业务闭环 #3） | 有意跳过：做对需把 `deliver()` 从同步一次性改成与 `render_check`（最长 10 分钟后台轮询）挂钩，改动面与“默认关闭 + 需真上传失败 + 需误判 408”三重窄触发不成比例 |
| “加入 KOReader”三步非原子（业务闭环 #4） | 有意跳过：`postJ` 全局 `alert` 兜底（`mark`/`delete` 任一步失败用户会看到），残余风险只是徽章短暂滞后、不丢数据；跨服务事务收益对不上改动量 |

离线：vendorcfg 新增 1 测；notes 182 个测试零回归。**三维审计到此全部处理完**：高优先级 3 项（摄取排除法反模式 §03ag、触屏关键信息只在 title、认领失败孤儿风险 §03ah）、中优先级打包（UI 小修、InkHttp/PendingQueue 消重复 §03ai）、本次低优先级收尾——判断不值得做的都写清了理由，不是漏做。

## 03ak｜三期真正收尾：host `shelf notes pull`（设备 → 本机 Obsidian vault，2026-09-16；⚠ **host 侧已于 2026-09-18 整个砍掉**）

> **⚠ 现状**：`shelf/host/`（书架线 host CLI）已随用户“不再使用 PC 端”整个砍掉，`shelf notes pull` **随之消失且无替代**（见 §00b 更正框）。下面只留设计动机与仍有效的接口事实；`GET /books/{uuid}/vault.json` 与 `GET /books` 的 `title` 字段仍在，只是没有自动化调用方，要把笔记拉到本机只能手动 curl。

§03r 落盘那半做完后，“host 拉走”一直是独立待办，这次补上。**设计选型**：没另起 `notes/host` CLI crate（§03af 已论证“专门起一套 host CLI crate 过度设计”，对批量拉取同样成立），而是挂进已有 `shelf` host CLI，复用它的 `HttpTransport`（网关 HTTPS+密码）与 `koreader pull` 立好的“`--out` 覆盖，缺省 XDG 目录”模式——“host 侧统一走网关”这套既有架构的又一个消费方。

**协议设计**：不造“打包下载整本书”的 zip/tar 端点，而把 `note-serve` 已有的“落盘”与“读回”拆成两个直交动作：

| 端点 | 作用 |
|---|---|
| `POST /books/{uuid}/export`（§03r 已有） | 刷新 `$XDG_DATA_HOME/notes/vault/<书名>/` 落盘，指纹没变的章节服务端自己跳过 |
| `GET /books/{uuid}/vault.json`（新增） | **纯读**：把已落盘 `.md` 原样读回 JSON（`export::manifest()`），不触发导出、无副作用。与 §03r 的单章浏览器下载端点（GET 当场重算、不读盘）故意反过来——这次要读整本所有已落盘文件，重算每章太浪费，调用方本就应先 `POST` 保证新鲜。响应 `dir` 是服务端已跑过 `sanitize()` 的目录名，客户端直接当本机子目录名（转义规则唯一事实源留在 Rust 侧）。`GET /books` 顺手补 `title` 字段（ink-serve 早在吐，`note-serve::ink::BookBrief` 原来没接） |

当时 `shelf notes pull` 的流程：`GET /api/notes/books` → 对每本先 `POST .../export` → `GET .../vault.json` → 按 `dir`/`files[].name` 落本机，无已导出内容的书跳过不建空目录；落地目录 `--out` > `config.toml` 的 `notes_vault`（用户要求：vault 在哪只有用户自己知道）> 缺省 `$XDG_DATA_HOME/shelf/notes-vault`。**是镜像不是合并**：本机文件被服务端内容整篇覆盖（手改会被覆盖，有意的简单化，与设备端“整文件替换、不做增量 diff”同一纪律）；要与 Obsidian 双向编辑是另一个更大的功能（真正的同步），与 §03af 的“规模不对”同一判断。

验证：note-serve 3 个 `manifest_*` 单测（从没导出过→空、读回排序、只认 `.md`）；host 侧用本机真实 ink-serve+note-serve+gateway 三个二进制（临时 XDG 隔离）+ fixture 条目库 + `gateway passwd` 走真实 HTTPS Basic 端到端通；设备恢复可达后（2026-09-16）真机拉取真实条目库，本机落地文件与设备端 `vault.json` 字节一致（与 KOReader 回流同一轮，见 §03al）。

## 03al｜KOReader 高亮/生词回流（2026-09-16，真机通；网页按钮 2026-09-23 补上，见 §03an）

> 白话：把用户在 KOReader 里划的线、查的生词，也收进笔记条目库，走同一套“浏览→转入笔记→整理→推送”流程。范围出处：书架白皮书“2026-09-06 用户定留给笔记线”与本文 §05 的一句 handoff，这次从零起草设计。

**架构选型**：KOReader 的原始数据在设备上归 `koreader-serve` 管，条目库写权限归 `ink-serve`（唯一写者，§01）。拆成两层：`koreader-serve` 新增两个**纯读**端点吐原始数据；`ink-serve` 的 `POST /koreader/import` 拉这两个端点、按 `notecore::koreader` 的合并规则写回条目库——同进程内互斥锁保证仍只有一个写者。

**`notecore::model::Entry.source`**：`Xochitl`（缺省，兼容旧条目库）/`KoreaderHighlight`/`KoreaderVocab`。两条 KOReader 摄取线都没有笔画坐标（`ink` 永远 `None`），天然走 `Entry::set_triage` 早有的“纯勾画直接定稿”快路径（§03o/§03p）：点「转入笔记」直接 `Reviewed`。因此**浏览态前端/`set_triage` 零改动**。

| 数据 | 来源与解析 | 要点 |
|---|---|---|
| **高亮** | `<book>.sdr/metadata.<ext>.lua`（KOReader 原生标注 sidecar，路径规则抄 `docsettings.lua` 的 `getSidecarDir`/`getSidecarFilename`：`mybook.sdr/metadata.epub.lua`——第一版想当然写成 `mybook.epub.sdr`，读源码才核对出来）。不在 Rust 里写 Lua 解析器，交给 KOReader 自带 `luajit` 跑 `shelf/koreader/annot.lua`（`dofile` 出真表 + 手写 JSON 序列化 `jval`，逻辑抄自 `merge.lua`，两脚本各自独立进程，重复一小段比硬凑 `require` 路径简单） | 一本书一个 `Book`；**`uuid` 不能直接用 `books/` 下相对路径**（含 `/`：`BookDb::path()` 会建出嵌套目录、`list()` 只读一层找不到；URL 路径段会被拆成两段）——本地端到端冒烟真踩中，改成对相对路径 FNV-1a 哈希 `koreader:<hex>`，书名留 `Book.title`。认领 id 按 `(书路径, pos0, pos1, datetime)` 哈希（annotations 数组无天然 id）；KOReader 里删掉一条高亮，下次拉取认领不到就标 `Revoked`（不物理删，同 xochitl 线“笔迹被擦”终态纪律） |
| **生词** | `settings/vocabulary_builder.sqlite3`（内置生词本插件库，schema 读 `plugins/vocabbuilder.koplugin/db.lua` 的 `CREATE TABLE` 核实）。**路径是 `settings/` 不是 `data/`**（真机才发现，已修） | 表本身跨书全局去重（`word` 主键），所以不拆成“一书一 Book”，用全局一份 `koreader-vocab` 合集 Book，`chapters` 借来存来源书名分组 |

**SQLite 读取的交叉编译坑与绕法**：起初用 `rusqlite`（bundled C 源码）作生产依赖，host 编译测试都过，交叉到 `aarch64-unknown-linux-musl` 链接失败——`sqlite3.c` 调 glibc LFS64 符号（`open64`/`stat64`/`pread64`），而项目交叉工具链是“`aarch64-linux-gnu-gcc`（glibc 头）编 C + `rust-lld` 链 musl”，“C 产物 libc 无关能链进 musl”这句对 `ring`（纯计算）成立，对真读文件的 SQLite 不成立（本机没装 musl 原生交叉 gcc）。给用户三选项（`koreader-serve` 单独退到 gnu 动态链 / 手写纯 Rust 只读解析器 / 装 musl 交叉工具链），**用户选手写解析器**：`shelf/services/koreader-serve/src/sqlite_min.rs`，零 C 依赖，只实现读表最小子集（文件头 + table b-tree interior/leaf + 溢出页 + record 变长编码；格式是十余年没变的公开稳定格式）。`rusqlite` 降为**仅 host 测试的 dev-dependency**，用它现造真实 `.sqlite3` 当 fixture 做差分测试（单页小表含 `INTEGER PRIMARY KEY` rowid 别名、3000 行强制 interior page、长文本强制溢出页链、真实 `vocabulary`/`title` 两表端到端）——14 个测试全绿，交叉编译恢复、产物仍全静态。印证“没有真机就靠差分测试 + 扩大规模暴露边界”这条纪律同样适用于文件格式解析。

**验证**：notecore::koreader 4 单测（合并/幂等/直接定稿/删除即撤销）+ koreader-serve `annot` 3 + `vocab` 2 + `sqlite_min` 5 + ink-serve `koreader` 3（`Fake` 桩）；host 侧用真实 `koreader-serve`+`ink-serve` 二进制（临时 XDG + 独立 `SHELF_KOREADER_ROOT`）+ 手写 sidecar fixture（真 `luajit` 解析）+ Python 造的真实 sqlite3 端到端通（含设计阶段发现并加守卫的漏洞：`ingest_doc` 对未知 uuid 前缀的排除法会让重启 ink-serve 时把 KOReader 条目误判成“书被删了”整批撤销）。

**真机验证（2026-09-16 设备重新可达后；当天先因主机内核/模块目录不匹配连不上，重启主机解决，与项目代码无关）**：
- **先摸真实数据抓到真 bug**：拉真机上已用一段时间的 6 本书（3 漫画 + 3 小说，`metadata.cbz.lua`/`metadata.epub.lua` 两种后缀）的 `.sdr`，`annot.lua` 全部解析正确（含“纯书签无文字”被正确过滤、空 `annotations` 表、`doc_props.title` 字段名）；但 `vocabulary_builder.sqlite3` 路径推断错（应在 `settings/`），已修。
- **部署**：`ink-serve`/`koreader-serve` 各自备份原二进制（`/home/root/backups/`）后逐个部署+重启+健康检查（`active`/新 PID/`NRestarts=0`）；先部署低风险的 `koreader-serve`（只读新增端点），再部署 `ink-serve`（条目库唯一写者，风险更高），确认现有条目库完好。
- **真实内容端到端**：用户在 KOReader 真实划一条高亮（《雪人》“再吃一頓早”，`02 卵石眼`）+ 真实查一个生词（“早上”），`POST /koreader/import` 两次分别识别出新内容、正确落库；点“转入笔记”对高亮直接 `Reviewed`；重复拉取幂等（第二次 0 新增）；全程 `NRestarts=0`。

**真机又揪出一个真缺口**：把那条高亮定稿后 `POST .../export` 返回 `{"files":0}`——`merge_highlights` 从来没给条目设过 `chapter`（恒 `None`），而两处投影都按 `entry.chapter == Some(idx)` 匹配 `Book.chapters[idx]` 分组，`chapter` 为 `None` 的条目**能定稿、但永远导不出、永远生成不了设备笔记本**。设计阶段没想到，只有真走到“定稿后尝试导出”才暴露。修法：`merge_highlights` 加 `chapter_index_of` 闭包参数（同 `merge_vocab`），`ink-serve::koreader::import` 按 annotations 给的**阅读顺序**（不是字典序，“第一章”/“第十章”字典序会乱）去重出 `Book.chapters`，每条高亮按 `chapter_title` 落下标；已落盘旧条目重新 `import` 时这条 `chapter` 变化本身触发“内容有变”判据，自动补上，无需迁移脚本。补 1 个 notecore 测试（多章各落不同下标）+ ink-serve 断言。重新部署后真机：《雪人》那条 `chapter` 补上（校对文本原样保留）、`export` 吐出 2 个文件（章节 md + 索引 md，front-matter/块锚/反链格式正确）；当时的 host `shelf notes pull` 拉到本机与设备端 `vault.json` 字节一致——“**从 KOReader 划线 → 条目库 → Obsidian md 落到本机磁盘**”全程真实数据零替身（host CLI 此后已砍，§03ak）。

**⚠ 仍未做到**：没有网页按钮触发 `POST /koreader/import`，只能 curl；PDF 类型 `pos0` 坐标对象形状、单本标注量很大触发溢出页/`sqlite_min` interior page 等真机罕见场景仍只有离线合成数据验证。

## 03am｜真机 bug：切换条目去处，另一个去处的"已推送"状态凭空消失（2026-09-17，真机通，两轮修复）

用户真机反馈：“笔记→整理→已导出中，落点切换会让另一个落点丢失，比如推送至原生、推送至 obsidian，刚刚的推送至原生状态就丢了。”

| 轮次 | 查到什么 | 结果 |
|---|---|---|
| **第一轮（治标）** | 显示层判据：`GET /books/{uuid}/sync` 每章给两组字段——`xxxNeeded`（此刻还有没有活条目要这个去处，条目 `destination` 一改就可能翻）与 `xxxGeneratedAt`/`xxxExportedAt`（历史事实：这章曾推过笔记本/导出过 Obsidian）。`syncBadges()` 原来只看 `xxxNeeded`；改成 `xxxNeeded` 或 `xxxGeneratedAt/xxxExportedAt` 非空，部署真机并用当时现场数据核对翻转正确 | 以为修完了——**用户重新验证仍“没修好”**，逼回去重查 |
| **第二轮（真根因，后端）** | `publish.rs::generate_chapter`/`export.rs::export_chapter` 在 `project_chapter`/`fingerprint_chapter` 返回 `None`（这章“没有可投影/可导出的条目”）时**无条件** `state.clear()` 清空历史记录（理由：防“幽灵已导出”——章内容全撤销/归档后记录永远非空会让「整理」误判已导出）。但 `None` 有两种成因没区分：① 这章真没有活条目了（该清）；② 条目活着，只是这次不想要**这一个**去处了（如 `Notebook`→`Obsidian`），设备上的笔记本文档没被删（清空路径从不碰真文档）、条目也在，只是撞上了①的判据——**把一条依然有效的历史记录真的删掉了**。用户点「推送本章」触发的 `POST .../generate` 内部无条件调这条清空，不是显示层能背的锅 | 真修：`notecore::model::Book::chapter_has_live_entries(idx)`（只看条目死没死 `status.is_live_for_projection()`，不管去处），两处清空前多判一层：只有这章**一个活条目都没有**才 `clear()`；`Empty` 结果照常返回（该跳过就跳过，不瞎生成/导出），历史记录原样保留 |

两层修复叠加才完整：后端不再抹记录 + 前端不再只看“现在要不要”决定是否显示徽章，单独哪一层都不够。

**验证**：notecore 新增 1 测（`chapter_has_live_entries_ignores_destination_and_terminal_status`）；note-serve 新增 2 测（`switching_destination_away_from_notebook_keeps_the_record_not_ghost_exported`/`…_obsidian_keeps_the_record_not_ghost_synced`，与已有“幽灵已导出”回归测试并排——一个测①该清、一个测②不该清）；195 个测试全绿。**真机端到端复现用户原话的完整两步序列**：把「雪人」那条高亮设 `Notebook` 推送（`notebookGeneratedAt` 记下时间戳）→ 切 `Obsidian` 再推送一次（`generate` 返回 `Empty`，模拟“推送本章”内部无条件触发那次）→ 查 `/sync`：**`notebookGeneratedAt` 仍是同一时间戳**（改之前会变 `null`）。

**教训**：第一轮只信了“改个前端判据、真机 curl 核对过”，但只用**单次静态快照**核对，没走一遍用户描述的**两步操作序列**——若第一轮就走完整序列，会立刻看到后端把 `notebookGeneratedAt` 清了。以后“状态在某个操作后消失”的报告，验证方式应是**真按用户描述的步骤走一遍、看最终状态**，而不是“找到一个说得通的判据问题就当作证实了全部”。

## 03an｜全系统审查两批修补：条目库防覆盖 + 配置防覆盖 + 全文搜索（2026-09-24）

**条目库解析失败不再被当成空书覆盖**。`BookDb` 原来读失败、解析失败都返回 `None`，`update` 接着用 `seed()` 的空书整本写回——降级部署遇到不认识的状态值、文件被改坏，校对文本和 AI 回答就静默全丢。现在 `read()` 区分三种情况：不存在 → `Ok(None)`；读失败/解析失败 → `Err`，另存一份 `<uuid>.json.corrupt`（已有就不重复拷），原文件不动，之后的写入一律拒绝，等人处理。网页上打开这本书会看到 500 和错误原因。

**AI 服务配置损坏时不再被缺省值覆盖**（`vendorcfg::ConfigCell::load`）：原来启动时"读 → 迁移 → 落盘一次"无条件写回，损坏的文件（含 API key）被换成缺省。现在损坏就跳过这次落盘、另存 `.corrupt` 副本，等用户在网页上主动保存才写新文件。另外含 key 的文件改为创建时就是 0600（`rmsvc_core::fs::write_atomic_mode`），不再先按默认权限落出来再 chmod。

**全文搜索**（`GET /api/ink/search?q=&limit=`，`services/ink-serve/src/search.rs`）：跨所有书搜勾画原文、定稿文本、最新转写草稿、提问、AI 回答、书名，不区分大小写，已撤销的条目不搜；每条条目只报第一处命中，片段前后各留 30 字。个人笔记量级，每次现读条目库全扫，不建索引。网页在「笔记」页顶部加搜索框，点一条结果切到那本书的「浏览」。

测试：条目库损坏 1 条、配置损坏 1 条、搜索 3 条。**真机**：部署后条目库与配置都未触发损坏路径（正常数据）；搜索的网页端尚未真机点过。

## 04｜踩坑

> 集中成坑位表；每条的详细过程在括号里的 § 节。

**设计/代码层**

| 坑 | 教训 / 做法 | 详见 |
|---|---|---|
| **“排除法”过滤（`!= X`）比“允许列表”（`matches!(status, A\|B\|C)`）更容易悄悄纳入不该要的状态** | 排除法只挡住“写判据那一刻脑子里想到的那个状态”，新增状态默认漏网通过；允许列表反过来，新增状态默认被挡，出错方向更安全。`live_entries` 一开始写成 `!= Revoked`，二期加 `Mined`/`Skipped` 后一直混进两条投影，三期真机导 md 才亲眼看见（离线测试从没写过“混进 Mined/Skipped 会怎样”的反向断言）。同一批状态判据有多处（两条投影 + 一个网页视图，各自独立维护）时，以**最先写对的那处**（网页视图的 `['pending','draft','reviewed']`）为基准对齐其余。**⚠ 2026-09-09 复发一次**：`merge_page`/`revoke_stale` 两处“给条目打 Revoked”的判据没同改，`Skipped`/`Archived` 被误判成 `Revoked`——同一反模式、不同代码路径。教训要跟着“排除法本身”记，不是跟着“当年改的那几个文件”记：改到任何状态过滤判据，先想“这是不是又在用 `!= X`” | §03r、§03ag |
| **模型字段从必有改成可选（`Entry.ink` 因纯勾画变 `Option`），光改生成端不够** | 前端排序/比较容易裸访问（`a.ink.bbox[1]`），一变可选就崩，且崩得无报错（渲染函数先清空容器再崩，用户只看到“空白”）。`grep` 该字段全部引用点，比事后靠用户反馈定位可靠 | §03p |
| **`.rm` 坐标看起来像页面尺寸，不代表是物理屏分辨率** | EPUB 页 960×1280 的虚拟画布 ≠ 物理屏 1404×1872；用“同份数据里已知语义的东西反推”而不是读一个数字直接信 | §03g |
| **读 wire 格式时“某处多出来的字节”要绑到具体 char_id/条目再判归属** | Subheading 1 的 7 字节曾被误记到 NumberedList 头上 | §03i |
| **自己写的解析器/编码器要拿独立实现交叉验证** | `is_ascii` 字段对中文也写 1，自家宽松解析器测不出，`rmscene` 的 `assert` 才抓到 | §03h |
| **状态“操作后消失”类 bug：要按用户描述的步骤真走一遍再宣称修好** | 第一轮只核对单次静态快照，漏掉后端把历史记录清空的真根因 | §03am |
| **删除数据结构 ≠ 删除触发它的手写标记语法** | 删“分区”时连 `## 文字` 手写标记一并删，用户当场纠正 | §03t |
| **真机验证造数据，用一望即知的占位假文本，别塞用户的真实描述；改字段前先读懂接口对状态字段的副作用** | 数据事故：把口述原文反复打进条目库，把“已撤销”翻回“已校对” | §03m |
| **前端 `await` 之后的重画会冲掉刚设的状态 / 覆盖未确认编辑** | 用 `pendingText` + `flushPendingText()`；状态文字停留 1.5 s 再重画；清空容器紧挨同步重建 | §03v、§03w |
| **批量下载多个文件用 `await` 后连续 `window.open()` 会被浏览器拦** | 先导出完再摆出每章下载按钮，让用户自己点（真用户手势） | §03u |
| **一把厂商 key 对该厂商旗下所有模型都该通用** | key 归属不该绑“这个型号在不在预置表”；`provider()` 按 baseUrl 兜底 | §03w |
| **`Marker`/`fingerprint` 等按“章”粒度的判据** | 同步状态精确到整章，不到单条；徽章要按条目自己的去处过滤 | §03aa、§03ab |

**环境/工具层**

| 坑 | 做法 |
|---|---|
| 外部进程直改 `.metadata` 的 `parent="trash"` 会被运行中 xochitl 覆写（阅读线判死） | 软删只能走 QML 代理（书架 `shelf-trash-agent.qmd`），note-serve 的旧版本回收即走这条 |
| 真机样本页可能全是墓碑（《人骨拼圖》c65fa2ae 页 23 笔全擦过，`Page` 剔除后零条目） | 采样前先用 `rmv6` 解析看非墓碑数，别拿它标阈值 |
| ureq 2 默认特性没有 `json`（`Response::into_json` 不存在） | 用 `serde_json::from_reader(resp.into_reader())`；TLS 根用 webpki-roots；设备出网直连，不经 host 代理 |
| 设备可能只在 WiFi 上：USB 网卡没起来时 `10.11.99.1` 不通、mDNS 也没有 | 局域网扫 `443` 找到 IP，再 `deploy.sh <ip>` |
| busybox：`head -1` 不认（要 `-n 1`）、无 `timeout`、`ls` 中文名显示 `?`（验名用 `find \| hexdump -C`）；`grep -c` 零匹配退出码 1 会断 `&&` 链 | 写设备端命令时避开这些 |
| SQLite C 依赖交叉编译到 musl 链接失败 | 手写纯 Rust 只读解析器（§03al） |
| 别跑 `cargo fmt --all`（仓库非 rustfmt 风格，2026-09-06 混进 64 文件重排回滚重放） | 手写对齐；qmd 先离线 `qmldiff apply-diffs`；xovi 已生效时重启 xochitl 用 `systemctl restart xochitl`，不手动跑 `xovi/start`；cwd 会漂移，路径写绝对 |
| python 改文件留了尾逗号把表达式变 tuple、测试文件括号未闭合各踩一次 | 改完立刻 `cargo test`/`node --check` |

## 05｜真机待办（2026-09-22 按当前代码与各节结论重新整理；原 25 条按时间堆叠的清单已压缩为下面两张表）

### 未闭环（按重要性）

| # | 事项 | 现状 | 详见 |
|---|---|---|---|
| 1 | **前端可视渲染的人眼确认** | 浏览页 / 模型管理面板 / 条目卡片 / 「整理」双层 tab / 「导入 md 文档」入口（含文件上传与可见性开关），后端数据链路都真机走通，但纯前端渲染与交互一直没人眼确认；`gateway/tools/screenshot-walkthrough/` 能自动起服务+灌 fixture+截图，但只覆盖顶层 nav + 一层子 tab，没针对这几处专门跑过一轮 | §03o/§03q/§03t/§03u/§03v/§03aa/§03af |
| 2 | **转写质量** | 汉字数字“一/二/三”被认成阿拉伯数字（提示词 2026-09-07 已补规则，待真机重转复验）；原“裁图混进印刷行”已被自渲染裁图（§03p）从机制上解决，旧样本没专门复验；`###`/`##` 小节标记至今**没有一次“转写对了、标记真被认出”的真机正例**（卡在手写行草连笔的 OCR 准确率，非代码问题；找工整样本再试） | §03g/§03o/§03p |
| 3 | **OpenAI / Gemini / DeepSeek 预置只验证了配置层**（预置表匹配、key 按厂商隔离、老配置迁移），没有真实 key 走过实际调用；只有 DashScope 真调过。**2026-09-22 复核并更新**：DeepSeek 预置已换成现行名 `deepseek-flash`/`deepseek-v4-pro`，老 id 自动迁移（见 §03u 复核框）；`gpt-6-astra`（2026-09-03 发布）先向 Trusted Access 企业开放，普通 key 可能用不了——未改 | §03u |
| 4 | `POST /koreader/import` 的网页按钮（「笔记」页顶部「导入 KOReader 批注」）2026-09-23 已加；PDF 类型 `pos0` 坐标形状、单本标注量很大触发溢出页等罕见场景只有离线合成数据验证 | §03al |
| 5 | **`archive`/`purge` 两端点没对真实历史数据实测**（一次性不可逆，没事先问用户不该拿真实数据练手）；`restore` 是可逆反方向操作，已在真实历史数据上验证 | §03r/§03u |
| 6 | **被动触发的修复没主动构造真机场景复验**：终态排除判据（`is_terminal()`，需真擦掉已跳过条目的笔迹或删带已跳过条目的书）、生成笔记本认领失败重试（低概率时序窗口）——判据/机制离线已覆盖 | §03ag/§03ah |
| 7 | “改条目 `destination` 后对应导出指纹立刻变”只有离线单测干净覆盖，真机因活跃测试书状态漂移没能单变量复现 | §03x |
| 8 | 手写行首标记对真实工整手写的端到端（同 #2）；`checkbox 勾上号（码 7）`写入器造不出（真限制，只读设计下用不上） | §03i |

### 已闭环（真机）——一览

| 事项 | 节 | 备注 |
|---|---|---|
| 步骤 0 样本标定（聚簇/配对阈值 `cluster_gap=40`/`pair_gap=120` 无需改）| §03f | |
| 揪出并修复裁图画布尺寸错（EPUB 页 960×1280），后续被自渲染裁图取代 | §03g/§03p | 转写准确率另计 |
| `rmv6::write` + `note-serve::rmdoc` + 上传三件套；全部 7 种打字样式渲染 | §03h/§03i | 已勾选待办（码 7）造不出 |
| 生成编排：首次生成 / 增量重传 + 旧本自动回收 / 无变化跳过 | §03j/§03k | |
| 二期：浏览态状态机 + 浏览页 + 纯勾画条目 + 自渲染裁图 + `mind-serve` + 模型面板 | §03n–§03q | 二期五步全完成 |
| 三期：md 导出、去处三选一、回收站（可恢复）、砍分区 | §03r/§03s/§03u | |
| 「整理」区多轮反馈落地（现行布局以 §03ad 为准） | §03t–§03ad | 均纯前端或后端小改 |
| 代码质量重构：`vendorcfg`、`ChapterStore<T>`、`SvcClient` | §03ac/§03ai | 真实配置/用量零丢失 |
| 复用书本自己的设备文件夹（取代建夹链路） | §03ae | 真机端到端 |
| 单篇 markdown 导入（后端管线真机字节级核对） | §03af | 前端入口未人眼确认（见 #1） |
| KOReader 高亮/生词回流（含真实划线/查词端到端） | §03al | 见 #4 |
| 切换去处后另一去处“已推送”凭空消失（前后端两层修复） | §03am | 真机走完两步序列 |
| 提示词“汉字数字原样抄” | §03g | 待复验（#2） |
| 原 host `shelf notes pull`（设备 → 本机 Obsidian vault） | §03ak | **随 host CLI 2026-09-18 已砍，无替代**；`vault.json` 接口还在 |
| `notebooks.rs` 内容清空时不清旧记录的小不一致 | §03x/§03am | `generate_chapter` 已接上 `clear()`，且后来加了 `chapter_has_live_entries` 判别，不再误清 |

离线门槛（滚动数）：`cargo test --workspace`（notes）当前约 **202 个**（rmv6 27 · epubmap 5 · notecore 62 · vendorcfg 18 · ink 15 · transcribe 22 · mind 21 · note 32，按 `#[test]` 计数，含 1 个 `#[ignore]`）零警告；网关 `node --check ui/app.js`；shell 过 shellcheck。

### 明确不做（本期）

扫描件 PDF、定稿 PDF（等步骤 0 ④）、笔记本手写批注回读（设备只读）、颜色语义（只进 tags）、自动清空回收站（网页按钮走 `emptyTrash()`，用户显式点）、Anki/Todoist/Readwise 外发（有 md 与稳定 id 之后再谈）、回收站长列表的折叠/按时间过滤（用户明确要求暂不加）。KOReader 高亮/生词回流已在 §03al 落地，不再属“不做”。
