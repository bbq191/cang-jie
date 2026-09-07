# notes —— reMarkable 笔记线

书架（`shelf/`）补读书短板，笔记线做强 reMarkable 的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。合上书自动摄取成条目（浏览页里挑要转笔记的、不要的直接跳过），手机整理页里修正转写、勾选按条目问 AI，设备笔记本与 md 都由条目库投影生成。**不引入任何旧 `knowledge/pkm` 代码**，只借鉴功能与踩坑。决策/真机/踩坑见 `docs/reMarkable笔记白皮书.md`（开头「现状总览」）；二期（浏览态触发/问AI/模型配置统一）计划见 未入库的计划文件——**步骤 1–4 已真机验证通过，只剩步骤 5**（模型配置统一页）。

## 现状（2026-09-07）

| 服务 | seg / 端口 | 职责 | 状态 |
|---|---|---|---|
| `ink-serve` 矿 | `ink` / 8795 | 监听书库 → 只扫变更页 → 勾画 ↔ 旁边手写配对（含无手写的纯勾画） → **自渲染裁图**（笔画矢量数据画折线，不依赖缩略图）→ **条目库（唯一写者）**；零网络 | ✅ 真机 active，浏览态状态机 + 纯勾画条目 + 自渲染裁图全部真机验证（白皮书 §03f/§03g/§03o/§03p） |
| `transcribe-serve` 转写 | `transcribe` / 8796 | 订阅矿的事件 → 裁图喂视觉模型（Qwen 缺省，OpenAI 兼容口可换）→ 草稿写回；只处理 `Pending`（用户点了「转入笔记」的）；唯一出网 | ✅ 真机 active、key 已配置、真调模型跑过；转写准确率还在打磨（§03g/§05） |
| `mind-serve` 脑 | `mind` / 8797 | 按条目单发：勾选「问AI」+ 输入问题 → 拼书名+章节+勾画原文+转写文本+问题 → 文字模型 → `answer` 写回；**没有批量循环/事件订阅**，纯被动等 HTTP，比 transcribe-serve 还轻 | ✅ 真机 active，端到端问答真机验证通过（§03p） |
| `note-serve` 本 | `notes` / 8798 | 注册「笔记」tab；打包 `.rmdoc`（`rmdoc.rs`）+ 写 `RootTextBlock`（`rmv6::write`，全部 7 种打字样式）+ 上传（复用 shelf-core）+ 条目→文档生成编排（`notecore::project` 投影 + `publish.rs` 上传/认领/旧本回收）全部**真机验证通过**；网页「生成」按钮还没接 | ✅ 三件套+编排全部真机验证（2026-09-07，两轮样式渲染 §03h/§03i + 当晚三轮生成/增量重传/旧本自动回收 §03k） |

## 四步闭环

```
① 合上书  ──事件驱动──►  ink-serve 矿     解析书页 .rm：勾画(GlyphRange 原文+矩形) ↔ 旁边手写(笔画簇) 几何配对（含无手写的纯勾画）→ 条目库(Mined) + 自渲染裁图
① .5 浏览 ──手机网页──►  「浏览」视图（✅ 真机验证）  按书→按页看裁图/引用，点「转入笔记」（Mined→Pending，纯勾画直接定稿）或「不需要」（→Skipped）
② 整理    ──手机网页──►  书架网关「笔记」tab   核对转写、选分区、选样式、勾「问AI」填问题（e-ink 上改字太痛苦，设备只负责写）
③ 智能    ──按条目────►  transcribe-serve 转写手写（只处理 Pending）；mind-serve 按条目单发「问AI」问题跑文字模型（✅ 真机验证，不再按分区批量）
④ 投影    ──note-serve──►  设备《书名》文件夹一章一本（xochitl 7 种打字样式）· vault/书名/第N章.md（反链）
```

![notes 数据流：四步闭环](docs/diagrams/data-flow.svg)

一句话原则：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影。**

## 架构：挂书架网关的 loopback 服务

```
浏览器 ──► shelf-gateway :8778 ──/api/<seg>/*──┬── ink-serve 127.0.0.1:8795 ──fswatch──► ~/.local/share/remarkable/xochitl（只读）
                 「笔记」tab（note-serve 注册）  ├── transcribe-serve :8796 ──订阅 ink /events──► DashScope（设备 WiFi 直连）
                 /api/events（area=notes）      ├── mind-serve :8797（纯被动，无订阅）──► DashScope（设备 WiFi 直连）
                                               ├── note-serve :8798 ──► xochitl /upload（打包/写入/上传+生成编排全部真机验证；网页按钮待接）
```

![notes 架构：条目库唯一写者 = ink-serve](docs/diagrams/architecture.svg)

- 注册表 / 反向代理 / 事件汇聚 / 管理台三态全是书架的机制（`shelf/README.md`）；网关 `manage::MODULES` 加四行即接入。
- 依赖方向单向无环：`services/* → shelf-core + crates/*`；**不依赖** `bookconv` / `device-core` / `knowledge/pkm` / `reading`。
- **零 xovi 依赖**：没有 qmd、没有 .so；将来建《书名》文件夹走书架的 Sidebar 代理 qmd（依赖留在书架那一份）。
- 服务间只经 HTTP：条目库只有 ink-serve 写，转写/脑/本都 `POST /api/ink/books/{uuid}/entries/{id}` 改字段。

### 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| ink | `GET /books` → `{items:[{uuid,title,chapters,entries,pending}]}`（`list_active`，只列还有活条目的书）· `GET /books/{uuid}`（整份条目库：chapters/sections/entries）· `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id} {text?|section?|style?|draft?|answer?|askAi?|question?}`（给 `text` 即已校对；`draft` 追加最新在前；`askAi`/`question` 是「问AI」勾选框+问题框）· `POST /books/{uuid}/entries/{id}/request`（浏览态"转入笔记"：`Mined→Pending`，纯勾画条目直接 `Reviewed`）· `POST /books/{uuid}/entries/{id}/skip`（"不需要"：`Mined→Skipped`）· `PUT /books/{uuid}/sections {sections}` · `POST /books/{uuid}/rescan` · `GET /events` |
| transcribe | `GET /status` → `{config(无 key), usage, failures, inkReachable, pending}` · `GET /config` · `PUT /config {apiKey?（只写）, clearKey?, model?, baseUrl?, auto?, maxPerRun?, pauseMs?, timeoutSecs?, maxAttempts?, prompt?}` · `POST /run`（同步跑一轮，回 `{scanned,done,failed,skipped,left,note}`）· `POST /books/{uuid}/entries/{id}`（强制转写一条）· `POST /retry`（清失败记录再跑）· `GET /events` |
| mind | `GET /status` → `{config(无 key), usage}` · `GET /config` · `PUT /config {apiKey?（只写）, clearKey?, model?, baseUrl?, timeoutSecs?, prompt?}` · `POST /books/{uuid}/entries/{id}/ask`（回答这一条，要求已勾 `askAi` 且填了 `question`，否则 400）——**没有 `/events`**，纯被动，没有需要推送的状态 |
| notes | `GET /status` · `GET /books`（各书章节生成状态）· `GET /books/{uuid}/notebooks` · `POST /books/{uuid}/generate`（全书按需重投影+上传，✅ 真机验证）· `POST /books/{uuid}/chapters/{idx}/generate`（单章）· `GET /events`（md 导出待建） |

事件：`{"svc":"ink","area":"notes","kind":"entries|sections"}`、`{"svc":"transcribe","area":"notes","kind":"transcribe"}` → 网页「笔记」tab 自动刷新。

## 目录

```
notes/
├── Cargo.toml · .cargo/               内部 workspace（与 shelf 同款 musl 全静态；`opt-level=z` + lto + strip）
├── crates/rmv6/                       .rm v6 解析+写入（剥离移植 remarkable_lines 0.1.3，MIT，PROVENANCE.md 留痕；page::Page = 笔画 + 勾画 + 打字文本，墓碑剔除；write.rs 编 RootTextBlock，模板替换拼 .rm，全部 7 种打字样式真机验证过）
├── crates/epubmap/                    .epubindex 起始页（两张表取首现）+ nav/ncx 目录 → 页号→章/小节
├── crates/notecore/                   领域核心（纯函数）：model 条目/分区/状态机（含 `ask_ai`/`question`）· hash FNV 簇指纹 · geom 聚簇+配对 · ingest 增量合并（含纯勾画路径） · marker 行首标记 OCR 兜底
├── services/ink-serve/                矿：doc(书库只读视图) · ingest(变更页编排) · crop(**自渲染裁图**，笔画矢量数据画折线，不吃缩略图) · bookdb(Repository) · config · main(路由+监听，接 askAi/question)
├── services/transcribe-serve/         转写：config(key/节制) · backend(Vision Strategy + OpenAiCompat) · prompt · ledger(用量) · ink(EntryStore 客户端) · worker(一轮编排) · main(SSE 订阅+防抖)
├── services/mind-serve/               脑：config(key，无节流字段) · backend(TextModel Strategy + OpenAiCompat，纯文本消息) · prompt(拼书名+章节+原文+文本+问题) · ledger(用量) · ink(EntryStore 客户端，book/post_answer) · worker::ask_entry(单条问答) · main(**无后台线程**，纯被动路由)
├── services/note-serve/               本：注册「笔记」tab；rmdoc.rs 打包 .rmdoc（上传复用 shelf-core::xochitl）；config/ink/trash/mkdir/notebooks/publish 生成编排+建夹（真机验证通过，网页按钮待接）
├── systemd/                           四个 .service（PartOf=shelf.target；随书架 install.sh 装）
├── host/                              待建：CLI `notes pull`（md 同步到 Obsidian vault）
├── testdata/renggu/                   真机 fixture（《人骨拼圖》墓碑页 .rm，测"解析成功零条目"）· renggu_marks/（同书真实勾画+手写）· seven_styles/（笔记本一页七样式，rmv6::write 模板）
└── docs/reMarkable笔记白皮书.md          决策 / 真机 / 踩坑（开头「现状总览」）
```

网页部分在书架：`shelf/services/shelf-gateway/ui/app.js` 的 `renderNotes`（按书→按章列条目：裁图 / 文本框改即存 / 分区 / 样式 / 转写区）。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}` |
| 配置 | `~/.config/notes/ink.json`（clusterGap 40 / pairGap 120 真机验证有效未改；cropMargin 24 / debounceSecs 4；`pageWidth`/`pageHeight`/`xOriginCenter` 字段还在但裁图自渲染后已不消费，历史见白皮书 §03g/§03p）· `~/.config/notes/transcribe.json`（**0600**，含 apiKey）· `~/.config/notes/mind.json`（**0600**，含 apiKey，字段比 transcribe.json 少——没有 maxPerRun/pauseMs/auto/maxAttempts） |
| 数据 | `~/.local/share/notes/crops/`（裁片 PNG，现在是自渲染的）· `~/.local/share/notes/vault/`（md，待建） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**）· `~/.local/state/notes/transcribe.json`（转写用量账本）· `~/.local/state/notes/mind.json`（问 AI 用量账本） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`——**绝不写** |

## 转写（transcribe-serve）

- 触发：订阅 ink-serve `/events`，`entries` 事件防抖 3 s 后跑一轮；启动追平一次；网页「转写」按钮同步跑一轮；单条「重转」强制跑。
- 一轮：列书 → 只看 `pending>0` 的书 → 条目 `needs_transcribe()`（簇指纹没有对应草稿）→ 取裁图 → 提示词（内置：只转写不发挥、汉字数字按原字符抄不转阿拉伯数字、+ 勾画原文作语境）→ 模型 → `POST ink /books/{uuid}/entries/{id}` 写 `draft{text,backend,at,hash}`。
  已校对 `text` 由 ink-serve 保证不被覆盖。行首标记兜底：条目样式仍是正文时，转写文本开头 `-`/`1.`/`口` → 无序/有序/待办并剥掉标记（`notecore::marker`）。
- 节制：每轮最多 `maxPerRun`（20）条、请求间歇 `pauseMs`（300）；同一条同指纹失败 `maxAttempts`（3）次后不再自动试（网页「重试失败」清）；一轮连续失败 3 次零成功即停（key 错/断网不一条条撞）。
- key：文件 → 环境 `DASHSCOPE_API_KEY`；`GET /config` 只报 `hasKey/keySource` **不回显**。用量账本只记次数/token/最近错误，不存内容。
- 后端可换：`baseUrl`/`model` 改成任何 OpenAI 兼容口（`POST {baseUrl}/chat/completions` + `image_url` data URI）；设备走自己的 WiFi 直连，不经 host 代理。

## 问 AI（mind-serve）

- 触发：**只有网页点「提问」按钮**——不像 transcribe-serve 订阅事件自动跑，`mind-serve` 没有后台线程、没有 `/events`，纯被动等 HTTP 请求，空闲零 CPU。
- 前提：条目要先勾「问AI」+ 填了问题（两者都是 `ink-serve` 存的字段，`askAi`/`question`）——`POST /entries/{id}/ask` 会先查这两条，没满足直接 400，不会误触。
- 一次问答：拼提示词（书名 + 章节 + 勾画原文 + 已校对/转写文本 + 问题，字段可选、超长截断）→ 文字模型 → 写回 `answer{text,backend,at,brief}`（`brief` 存的是问题原文，不是分区简述）。
- key/后端跟 transcribe-serve 同一套模式（各自独立的 `mind.json`/账本），但**没有节流字段**（没有批量循环可节流）。
- 默认模型 `qwen-plus`；如果配的 key 对这个模型没权限（DashScope 按模型限权限），会拿到 `403 access_denied`，换个模型或换把权限完整的 key 即可，不是 bug。

## 增量规则（回答"二次识别会不会把改好的字覆盖回去"）

条目库是事实源，笔记本与 md 只是投影。每片手写按笔画集合算指纹：指纹不变 → 不重识别、不动校对文本；补了几笔 → 同一条目、新草稿只作建议；
笔画全擦 → 标「已撤销」不删。校对过的字永远不会被覆盖。条目 id 创建时按 (书, 页, 首笔 id) 一次算定，之后永不重算。

## 构建 · 部署

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host：68 个测试（rmv6 7 · epubmap 5 · notecore 20 · ink 10 · transcribe 9 · mind 10 · note 7）
cd ../shelf && ./build.sh && ./deploy.sh <设备IP>                  # 随书架一起交叉编译/打包/装机（NOTES_BINS；设备在 WiFi 上时给 WiFi IP）
ssh root@<设备IP> sh /home/root/shelf-pkg/shelf/install.sh --only ink,transcribe,mind,note   # 只装/更新笔记线
```
卸载走书架 `uninstall.sh`（笔记线四令牌同在；条目库不在 `--purge` 范围）。

## 演进记录（详见白皮书各节）

| 阶段 | 内容 | 状态 |
|---|---|---|
| 定案 | 旧 pkm 退役；四服务；条目库唯一事实源；手机修正设备只读；一章一本《书名》夹；分区 = 名字+简述+是否调模型 | ✅ 2026-09-06 用户拍板（§00/§01） |
| 地基 | `rmv6`（剥离移植 + CHECKBOX 码）· `epubmap`（两张表取首现 + nav/ncx）· `notecore`（模型/指纹/聚簇配对/增量合并/行首标记） | ✅ 离线 31 测（§03b） |
| 矿 | ink-serve：fswatch 只扫变更页、缩略图裁片、条目库唯一写者、HTTP + 事件 | ✅ 真机 active、扫到 38 章（§03c） |
| 网关接入 | MODULES 三行、「笔记」tab（裁图/文本/分区/样式改即存）、部署链 NOTES_BINS/令牌 | ✅ 真机 tab 注册（§03d） |
| 转写 | transcribe-serve：Vision Strategy（Qwen 缺省）、限量/失败上限/即停、key 只写不读 0600、用量账本、网页转写区 | ✅ 修完裁图坐标（§03g）真机重转复验：裁图都对准了手写位置，但转写准确率另计——2 条"第一/二段"被认成"第1/2段"（汉字数字读成阿拉伯数字）、1 条完全读错（裁图边距混印刷体）；1 条无勾画批注裁不到，新守卫优雅跳过 |
| 步骤 0 | 真机样本标定阈值/页几何/样式码 | ✅ 2026-09-07：聚簇/配对阈值验证通过、NumberedList 码=10（§03f）；★页坐标画布尺寸原假设是错的，真机反测坐实 960×1280、已修复部署复验（§03g） |
| 写入底座 | rmv6::write 编 RootTextBlock（全部 7 种打字样式）+ note-serve::rmdoc 打包 `.rmdoc` + 上传（复用 shelf-core::xochitl） | ✅ 真机验证通过（2026-09-07 两轮：Subheading 1/2 区分开关、NumberedList 自动编号，§03h/§03i） |
| 生成编排 | `notecore::project` 投影一章 + 变更指纹 · `note-serve::publish` 上传/认领/旧本回收编排（Strategy trait 全桩单测） | ✅ 真机验证通过（2026-09-07 当晚三轮：生成/增量重传+旧本自动进回收站/无变化跳过，§03j/§03k） |
| 建夹代理 | `shelf-mkdir-agent.qmd`（MainView 锚点）+ book-serve `mkdir.rs`，《书名》文件夹缺失时自动建 | ✅ 真机验证通过（§03l） |
| 二期·浏览态状态机 + 浏览页 UI | `notecore::model::Status` 拆 `Mined`/`Skipped`，`ink-serve` `request`/`skip` 两端点，网关"浏览"/"整理"两个子视图 | ✅ 真机验证通过（部署+完整闭环，§03o） |
| 二期·纯勾画条目 + 裁图自渲染 | 只勾线不写字的页也能落条目；裁图从缩略图改成吃 `.rm` 矢量数据自己画，不受视口限制、不混印刷体 | ✅ 真机验证通过（§03p） |
| 二期·mind-serve 问 AI | 按条目单发（取代首期按分区批量）：勾「问AI」+ 填问题 + 点提问 → `answer` 写回 | ✅ 真机验证通过（§03p） |
| 二期·模型配置统一页 | `transcribe-serve`/`mind-serve` `GET /config` 加 `keyMasked`；网关"模型"面板（视觉+文字并排、用量卡片） | ⏳ **二期唯一没做的一步**（§05，未入库的计划文件） |
| 网页「生成笔记本」按钮 / md 导出 | note-serve 生成 API 已就绪（真机验证过），前端按钮未接；导出到 Obsidian vault | ⏳ 待建（§05） |
