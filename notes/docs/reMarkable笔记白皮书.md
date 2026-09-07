# reMarkable 笔记线（notes）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。读现状先看 §00b；待办/已闭环/明确不做看 §05；踩坑看 §04。总计划 未入库的计划文件。
> 笔记线**挂在书架网关下**：网关 / 注册表 / 事件汇聚 / 部署链 / 原生回收站代理都是书架的（`shelf/docs/reMarkable书架白皮书.md`），本文只记笔记线自己的决策、服务、真机轮次。

## 00｜定位与原则（2026-09-06 用户定）

书架（`shelf/`）补 reMarkable 读书短板；笔记线做强它的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。文学书勾作者名写"查作者"，学术书勾公式写"?/没听懂"或"!/背诵"——凡是勾了、写了的都该汇入笔记。旧 `knowledge/pkm`（画星待办、六色槽卡片、跨书汇编、复盘队列、仪表）判"太花哨"（颜色即路由的产物），**全部退役；新建 `notes/`，不引入任何旧代码，只借鉴功能与踩坑**（`.rm` 解析、`.epubindex` 页→章按书架惯例"剥离移植"成新 crate）。

四步闭环：① 合上书自动摄取（事件驱动）→ ② **手机上修正**（e-ink 打字太痛苦；不再引入中文化/输入法）→ ③ 按分区调智能（**分区 = 名字 + 简述 + 是否调模型，简述就是给 AI 的要求**；"背诵"不调）→ ④ 可选导出 md，一章一文件，与设备笔记本同构，反链。

工程原则同书架四条（XDG · 设计模式去重解耦 · 专项专用可插拔 · 不引旧 crate 不对接旧路径），外加笔记线自己的一句话：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影**。用户另定：四个服务（转写与智能分开）；设备笔记本一章一本、放《书名》文件夹、只读；分区自定义；首期只 EPUB；**后续全部提交 `dev` 分支**。

## 00b｜现状总览（2026-09-07 刷新，读其余历史节前先看这里）

**架构**：书架网关 `manage::MODULES` 加三行（`ink`/`transcribe`/`notes`）；三个 loopback 服务已上机——**ink-serve 矿 8795**（书库监听 → 条目库唯一写者，零网络）· **transcribe-serve 转写 8796**（订阅矿的事件 → 裁图喂视觉模型 → 草稿写回，唯一出网）· **note-serve 本 8798**（注册「笔记」tab；写入/打包/上传三件套 + 条目→文档生成编排**全部真机验证通过**，含旧版本自动软删闭环，见 §03h/§03i/§03j/§03k）；**mind-serve 脑 8797 待建**。网页只多一个「笔记」tab（前端组合 `/api/ink` `/api/transcribe` `/api/notes`），事件区域 `notes`（矿发 `entries`/`sections`，转写发 `transcribe`）经网关 `Hub` 汇聚到 `/api/events`，页面零轮询。**笔记线零 xovi 依赖**（无 qmd、无 .so；将来建《书名》文件夹走书架的 Sidebar 代理 qmd，依赖仍留在书架那一份）。

![notes 架构：条目库唯一写者 = ink-serve](diagrams/architecture.svg)

**数据流**：合上书 → xochitl 重写 `<uuid>.content/.metadata` → ink `fswatch`（书库目录非递归、4 s 防抖）→ 只扫页 `.rm` mtime 变了的页 → `rmv6` 解析（勾画 GlyphRange + 手写笔画，同一坐标系，墓碑剔除）→ `notecore` 并查集聚簇 + 就近配对 + 增量合并 → 从缩略图裁片 → `~/.local/state/notes/books/<uuid>.json` + `~/.local/share/notes/crops/` → 事件 → transcribe 防抖 3 s 取待转写条目 → DashScope `qwen3-vl-plus`（OpenAI 兼容口，改配置即换厂）→ `POST ink /books/{uuid}/entries/{id}` 写 `draft` → 手机网页改字/分区/样式（改即存）→〔待建〕note 投影为《书名》文件夹一章一本 + md 导出。

![notes 数据流：四步闭环](diagrams/data-flow.svg)

**真机现状（2026-09-07 汇总，历史逐轮记录见 §03c–§03k）**：设备 3.28.0.172，WiFi 直连（IP 是 DHCP 分配的，每次连不一定一样，别死记）。三服务 `active`，注册表 8 项（书架 5 + 笔记 3）。ink-serve 已摄取真实勾画+手写样本，聚簇/配对/裁图坐标全部真机验证有效（§03f/§03g）。transcribe-serve key 已配置、真调过模型，转写准确率还在打磨（§03g）。note-serve：写入/打包/上传三件套 + 全部 7 种打字样式两轮真机验证通过（§03h/§03i）；条目库→一章一本的生成编排**三轮真机验证通过**（首次生成、增量重传+旧本自动进回收站、无变化跳过，§03k）——但网页「笔记」tab 还点不了「生成」（路由已开、前端未接，目前只能 curl/wget 手动触发），且《书名》文件夹自动创建还没做（新书首次生成落库根）。**未目视**：手机网页笔记页/转写区完整交互流程（用户看）。

**代码落点**：`crates/rmv6`（`lib.rs` 低层 `RmFile::read` / `page.rs` 高层 `Page{strokes,highlights,text}` + `BBox` / `write.rs` 写 `RootTextBlock` + 模板替换拼 `.rm`，§03h）· `crates/epubmap`（`index.rs` 两张表取首现 / `toc.rs` nav→ncx 两策略 / `lib.rs` `BookMap::chapter_of`）· `crates/notecore`（`model` 条目/分区/样式/状态 · `hash` FNV 簇指纹与条目 id · `geom` 聚簇/配对 · `ingest` 增量合并 · `marker` 行首标记 OCR 兜底 · `project` 条目库→段落列表投影+变更指纹，§03j）· `services/ink-serve`（`doc.rs` 书库只读视图 / `ingest.rs` 变更页编排 / `crop.rs` 页坐标→缩略图像素 / `bookdb.rs` Repository / `config.rs` 阈值与几何 / `main.rs` 路由+监听）· `services/transcribe-serve`（`config` key 与节制参数 / `backend` `Vision` Strategy + `OpenAiCompat` / `prompt` / `ledger` 用量账本 / `ink` `EntryStore` 客户端 / `worker` 一轮编排 / `main.rs` SSE 订阅 + 防抖工作线程）· `services/note-serve`（`rmdoc.rs` 打包 `.rmdoc` + 生产模板常量 / `config.rs` xochitl host/超时/文件夹命名 / `ink.rs` 只读 `EntryStore` 客户端 / `trash.rs` 跨服务调 book-serve 回收站队列 / `notebooks.rs` 每章生成记录簿记 / `publish.rs` `Uploader` Strategy + `generate_chapter/generate_book` 编排，§03j / `main.rs` 路由）· 网关 `ui/app.js` `renderNotes`（尚未接"生成笔记本"按钮）· `shelf/{build,deploy,install,uninstall}.sh` 的 `NOTES_BINS`/令牌。

**离线门槛**：`cargo test --workspace` 51 个（rmv6 7 · epubmap 5 · notecore 15 · ink-serve 9 · transcribe-serve 8 · note-serve 7）零警告；网关 `node --check app.js`；shell 过 shellcheck。

**未闭环**：transcribe 转写质量再打磨（重转复验：坐标已对、结构读对，但汉字数字"一/二/三"被认成阿拉伯数字"1/2/3"，1 条仍混印刷体，见 §03g；提示词已补一条规则，待真机复验）· note-serve 生成编排前端接线（后端三轮真机验证通过，见 §03k；网页「笔记」tab 还没有「生成笔记本」按钮）· 书库动作代理 mkdir 扩展（§05 第 5 项，新书首次生成前《书名》文件夹要么手动建、要么落根目录）· mind-serve · md 导出 + `notes pull`（§05）。步骤 0 真机样本已于 2026-09-07 采回、验证、且真机复验通过（§03f/§03g/§03i）。

## 01｜架构决策

- **四个服务而不是一个 `note-serve`**（用户驳回单服务方案）：失败面分离——矿零网络（解析错只影响新条目）、转写/脑出网（断网只是积压）、本只碰输出物（xochitl `/upload`）；**转写与智能分开**是用户明确要求（转写是 OCR 边界问题，智能是提示词问题，节奏与费用都不同）。
- **不自建网关，挂书架现成的**：笔记线是独立仓库（`notes/`），但没有自己的 HTTPS/密码/证书/mDNS——那些是重复劳动（书架已经踩全了这些坑）；也没有塞进书架任何一个现成服务（`book-serve` 等），失败面与依赖方向都不同，硬塞会把两条线的故障域绑在一起。折中：复用书架的网关/注册表/事件汇聚/部署链这套**机制**（书架那边只加 `manage::MODULES` 几行映射，机制本身早就是通用的），但架构决策、服务划分、数据模型这些**笔记线自己的设计**完全在本文档，书架白皮书只记"它那边为了接住我们、改了什么"（见 `shelf/docs/reMarkable书架白皮书.md` §03ac）。代价：`shelf-gateway` 的 `MODULES` 成了跨两个仓库的单一事实源，笔记线加新服务除了自己这边的代码，还得去书架那个文件登记一行。
- **条目库唯一写者 = ink-serve**：转写/脑/本一律经它的 HTTP 改字段（`POST /books/{uuid}/entries/{id}`，缺省底座无 PATCH）。多进程各自读改写同一份 JSON 迟早互相覆盖（书架落库边车早期踩过同类）。
- **设备只写不改，改在手机**：xochitl 不认外部对已有文档的原地修改（书架/PKM 两线都判死），且 e-ink 上改字太痛苦。所以设备笔记本**只读**，由条目库投影生成；一章一本使重建局部化（只重建变过的章），旧本走书架回收站代理软删（`selectionMoveToTrash` 是唯一可靠路，直改 metadata 会被运行中 xochitl 覆写）。
- **事件驱动、零轮询**：ink 只在书库目录上挂非递归 inotify（合上书时 xochitl 重写 `.content/.metadata`，页 `.rm` 的写入不监听——文件多且是 xochitl 内部节奏），触发后按页 `.rm` mtime 只扫变更页；转写订阅矿的 `/events`；网页订阅网关 `/api/events`。
- **增量在数据层**（回答用户"二次识别会不会把改好的您好覆盖回你好"）：簇指纹 = 笔画 id 集合 + 点数 + 量化包围盒的 FNV-1a；指纹不变 → 不重转写不动 `text`；共享笔画但指纹变（补了几笔）→ 同一条目、新 `draft` 只作建议；笔画全没 → `Revoked` 留痕；条目 id 按 (书, 页, 最小笔画 id) 创建时一次算定永不重算。投影永远取 `text ?? draft`。
- **分区 = {名字, 简述, 是否调模型, 触发词}**：简述就是提示词，"背诵"分区空简述不调模型；缺省四区（查询 / 解释 / 背诵 / 其他），按书可改；行首触发词（`?` `查` `!` `背`）给缺省归属。
- **样式判定：几何优先，OCR 兜底**：`-`/实心点/`口`/下划线分区头由几何认（ink-serve；`cluster_gap=40`/`pair_gap=120` 已用真机样本验证，见 §03f，`has_underline` 判据已实现）；`1.` 数字形状不定走 OCR（transcribe 侧 `notecore::marker`，只在条目仍为正文时认，并把标记从正文剥掉——笔记本样式自带编号/符号）。
- **裁图来源 = xochitl 现成缩略图**（384×512，3:4）：零渲染成本、与 de-risk 结论一致（工整 ≈100% / 快写 ~60–91%）；精度不够再换高分辨率自渲染，`crop.rs` 只暴露"给我这片的 PNG"可替换。页坐标 → 像素按 `page_width/height` 等比、`x_origin_center` 可配，**EPUB 页真机测得 960×1280、x 原点居中，见 §03g**（不是 1404×1872 物理屏——真机踩过、改过、验证过）。
- **视觉后端 Strategy**：`Vision` trait 一个方法；`OpenAiCompat` 走 `POST {baseUrl}/chat/completions` + `image_url` data URI，DashScope Qwen 缺省（国内直连、设备自己 WiFi 不经 host 代理——host clash fake-ip 会挡），任何 OpenAI 兼容口只改配置。编排 `worker::run_once` 全 trait 注入，内存桩单测。
- **rmv6 剥离移植而非依赖 device-core**：vendored `remarkable_lines` 0.1.3（MIT）只留 v6，保留两处兼容补丁（未知 PenColor/ParagraphStyle/Tool 码兜底、块尾多余字节跳过），补 CHECKBOX(6/7) 码；`PROVENANCE.md` 留痕。notes 不依赖 `bookconv`/`device-core`/`knowledge/pkm`/`reading`。
- **体积/内存**：musl 全静态 ink 2.5 MB · transcribe 2.1 MB · note 1.2 MB；单元 `MemoryMax=128M` `CPUWeight=20` `Nice=5`。

**手写约定 ↔ xochitl 3.28 七种打字样式**（格式菜单 qml_00db4610：Title / Subheading 1 / Subheading 2 / Body / Bulletpoint / NumberedList / CheckboxUnchecked；.rm 段落样式码 + 全部 7 种真机验证通过的写入能力，见 §03f/§03h/§03i）：

| 样式 | 码 | 笔记本用途 | 手写约定 | 判法 |
|---|---|---|---|---|
| Title | 2 (HEADING) | 页标题 = 章名 | 无 | epubmap |
| Subheading 1 | 3 (BOLD) + 7 字节标记 `21023403000000` | 分区头（名 + 简述 = AI 要求） | 一行字 + 下面长横 | 几何（`has_underline`） |
| Subheading 2 | 3 (BOLD)，不带标记 | 勾画所在小节 | 无 | epubmap |
| Body | 1 (PLAIN) | 转写正文 / AI 回答 | 普通书写 | OCR |
| Bulletpoint | 4 (BULLET) | 无序 | 行首短横 / 实心点 | 几何（OCR 兜底 `- `/`• `） |
| NumberedList | 10，格式子块跟其余样式一样只有 2 字节 | 有序 | 行首 `1.` | OCR（`marker`） |
| Checkbox（未勾选） | 6 (CHECKBOX) | 待办 | 行首空心小方框 | 几何（OCR 兜底 `□`/`口 `） |
| Checkbox（勾上号） | 7，只能原生点方框切换，写入器造不出 | — | — | — |

## 02｜XDG 路径表（设备 HOME=/home/root，`Paths::app_{config,data,state}_dir("notes")`）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,note-serve}`（随书架 `install.sh`，令牌 `ink`/`transcribe`/`note`） |
| 配置 | `~/.config/notes/ink.json`（聚簇/配对阈值、页几何、防抖；首启写出缺省）· `~/.config/notes/transcribe.json`（**0600**，含 apiKey；backend/baseUrl/model/timeoutSecs/maxPerRun/pauseMs/auto/maxAttempts/prompt） |
| 数据 | `~/.local/share/notes/crops/`（手写裁片 PNG）· `~/.local/share/notes/vault/`（md 导出，待建） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**，一书一文件，原子写）· `~/.local/state/notes/transcribe.json`（用量账本：次数/token/最近错误/上轮报告，不存内容） |
| 运行时 | 与书架共用注册表 `$XDG_RUNTIME_DIR/shelf/services/`（缺省回落 `/tmp/shelf-0/shelf/services`） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`（`<uuid>.{metadata,content,epub,epubindex}`、`<uuid>/<page>.rm`、`<uuid>.thumbnails/<page>.png`）——**绝不写** |

## 03｜systemd

三个单元 `notes/systemd/*.service`，随书架载荷一起装：`PartOf=shelf.target` + `WantedBy=shelf.target`，`After=home.mount`（transcribe 另 `Wants/After=network-online.target`）；`Restart=on-failure`。**不给 xochitl 加任何依赖**（红线）。装/卸：`shelf/install.sh --only ink,transcribe,note`、`shelf/uninstall.sh`（条目库不在 `--purge` 范围，绝不删用户笔记）。

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

## 04｜踩坑

- **外部进程直改 `.metadata` `parent="trash"` 会被运行中 xochitl 覆写**（阅读线判死）；软删/建夹只能走 QML 代理（书架 `shelf-trash-agent.qmd`）。
- **真机样本页可能全是墓碑**：《人骨拼圖》c65fa2ae 页 23 笔全被擦过，`Page` 剔除后零条目——采样前先 `rmv6` 解析看非墓碑数，别拿它标阈值。
- **ureq 2 默认特性没有 `json`**：`Response::into_json` 不存在，用 `serde_json::from_reader(resp.into_reader())`；TLS 根用 webpki-roots（锁文件已有），出网设备直连不经 host 代理。
- **设备可能只在 WiFi 上**：USB 网卡没起来时 `10.11.99.1` 不通、mDNS 也没有；局域网扫 `8778` 找到 IP 后 `deploy.sh <ip>`（host 参数）。
- busybox：`head -1` 不认（要 `-n 1`）、无 `timeout`、`ls` 中文名显示 `?`（验名 `find | hexdump -C`）；`grep -c` 零匹配退出码 1 会断 `&&` 链。
- 书架规则沿用：**别跑 `cargo fmt --all`**（仓库非 rustfmt 风格，2026-09-06 混进 64 文件重排回滚重放）；qmd 先离线 `qmldiff apply-diffs`；重启 xochitl 只用 `xovi/start`；shell 里 cwd 会在 `cang-jie/`、`shelf/`、`notes/` 间漂移，路径写绝对。
- 会话里 python 改文件时留了个尾逗号把表达式变 tuple、测试文件括号未闭合各踩一次——改完立刻 `cargo test`/`node --check`。

## 05｜真机待办（2026-09-07 刷新）

**未闭环（按依赖顺序）**：
1. **转写质量两项**：① entry3（高亮 3 旁 `口 第三段`）重转后仍没读对，裁图上下混进了相邻印刷行——把 `cropMargin` 从固定 24 改成按簇高度动态收紧，或者干脆换自渲染高分辨率裁图（§01 早留的口子）；② ~~entry1/2 把手写的汉字数字"一/二"认成阿拉伯数字"1/2"~~ 提示词已加规则（2026-09-07），待真机重转复验是否真的不再转数字。entry4 那条旧的错误草稿是裁图坐标错时期的遗留数据，手机网页上人工清一下（一改字段就覆盖）。
2. ~~NumberedList 补样本~~ ✅ 用户真机核对时一并补了：那 7 字节根本不属于 NumberedList，是分析失误，已更正、已解除限制（§03i）。
3. ~~rmdoc 打包器 + 真机小范围验证~~ ✅ 2026-09-07 当场做完两轮：全部 7 种打字样式真机上传+渲染验证通过（§03h/§03i）。
4. ~~note-serve 条目→文档编排~~ ✅ 2026-09-07 当晚三轮真机验证通过：首次生成（渲染全对）、改文本增量重传+旧本自动进回收站（`shelf-trash-agent.qmd` 全自动消费队列，没人手动点）、无变化跳过（见 §03k）。**剩的是前端接线**：网页「笔记」tab 还没有「生成笔记本」按钮，目前只能 curl/wget 手动触发 `POST /books/{uuid}/chapters/{idx}/generate`。
5. 书库动作代理扩展：`shelf-trash-agent.qmd` → 通用 `{action: trash|mkdir}` 队列，`Library.createCollection` 建《书名》夹（现在新书首次生成落库根，人工挪一次之后就找得到）；先离线 `apply-diffs` 再上机。
6. mind-serve：按分区跑文本模型（简述 = 提示词，`ai=false` 跳过），`answer` 写回再投影。
7. md 导出 `vault/<书名>/第N章.md`（front-matter、`^id` 块锚、`[[书名]]` 反链、索引页）+ host `notes/host/bin/notes pull`。
8. 文档收尾、旧 PKM 白皮书加"已退役、由 notes/ 取代"头注、`dev` 以 `--no-ff` 合入 `feature/shelf-p1`。

**已闭环（真机）**：§03c ink-serve 首轮（active/注册/追平 38 章）· §03d 「笔记」tab 注册 · §03e transcribe-serve 部署（active/注册/0600/追平记 note）· §03f 步骤 0 样本标定（聚簇/配对阈值验证通过）· §03g 揪出裁图画布尺寸错（960×1280）并修复部署复验（3 条有勾画的条目裁图都对准了手写位置，但转写准确率另计——2 条数字被认错、1 条完全读错；1 条裁不到已优雅降级）· §03h `rmv6::write`/`note-serve::rmdoc`/上传三件套首次真机验证通过 · §03i 更正 NUMBERED 误判、解出 Subheading 1/2 区分开关、二次真机验证全部 7 种打字样式渲染正确 · §03j/§03k note-serve 生成编排离线写完当晚三轮真机验证通过（生成/增量重传+旧本自动回收/无变化跳过全绿）。离线：五 crate+服务 **51** 测。

**明确不做（本期）**：扫描件 PDF、定稿 PDF（等步骤 0 ④）、笔记本手写批注回读（设备只读）、颜色语义（只进 tags）、自动清空回收站（网页按钮走 `emptyTrash()` 用户显式点）、Anki/Todoist/Readwise 外发（有 md 与稳定 id 之后再谈）、KOReader 高亮回流（书架砍下来留给笔记线，排在导出之后）。
