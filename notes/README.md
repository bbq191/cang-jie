# notes —— reMarkable 笔记线

书架（`shelf/`）补读书短板，笔记线做强 reMarkable 的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。合上书自动摄取成条目（浏览页里挑要转笔记的、不要的直接跳过），手机整理页里修正转写、勾选按条目问 AI；每条内容可以选去哪——设备笔记本、Obsidian、或者干脆不要了；设备笔记本与 md 都由条目库投影生成。**不引入任何旧 `knowledge/pkm` 代码**，只借鉴功能与踩坑。决策/真机/踩坑见 `docs/reMarkable笔记白皮书.md`（开头「现状总览」§00b）。

## 现状（2026-09-08，三期完成）

| 服务 | seg / 端口 | 职责 | 状态 |
|---|---|---|---|
| `ink-serve` 矿 | `ink` / 8795 | 监听书库 → 只扫变更页 → 勾画 ↔ 旁边手写配对（含无手写的纯勾画） → **自渲染裁图**（笔画矢量数据画折线，不依赖缩略图）→ **条目库（唯一写者）**；零网络 | ✅ 真机 active，浏览态状态机 + 纯勾画条目 + 自渲染裁图 + 归档/清空回收站全部真机验证 |
| `transcribe-serve` 转写 | `transcribe` / 8796 | 订阅矿的事件 → 裁图喂视觉模型（预置下拉选，DashScope 缺省，OpenAI 兼容口可换）→ 草稿写回；只处理 `Pending`（用户点了「转入笔记」的）；唯一出网之一 | ✅ 真机 active、key 已配置、真调模型跑过；转写准确率还在打磨 |
| `mind-serve` 脑 | `mind` / 8797 | 按条目单发：勾选「问AI」+ 输入问题 → 拼书名+章节+勾画原文+转写文本+问题 → 文字模型 → `answer` 写回；**没有批量循环/事件订阅**，纯被动等 HTTP，比 transcribe-serve 还轻 | ✅ 真机 active，端到端问答真机验证通过 |
| `note-serve` 本 | `notes` / 8798 | 注册「笔记」tab；打包 `.rmdoc`（全部 7 种打字样式）+ 上传 + 条目→文档生成编排（网页按钮已接线）；**md 导出**（落设备 vault + 直接触发浏览器下载） | ✅ 三件套+编排+导出全部真机验证通过 |

## 三期定案：去处三选一 + 砍掉分区（2026-09-08）

用户提出：转写/校对/问答做完之后，内容最终该由用户决定去哪——落设备笔记本、落 Obsidian、还是不要了。落地：

- **`Entry.destination`**（`Notebook`/`Obsidian`/`Both`，缺省 `Both`——两处都要，不设置就是老行为）：设备笔记本投影只收 `Notebook`/`Both`，Obsidian 导出只收 `Obsidian`/`Both`。
- **`Status::Archived`**（"不要了"，软删终态，跟 `Revoked` 同类但用户主动触发）+ **`Book::purge_terminal()`**（物理清 `Archived`/`Revoked`/`Skipped`，手动触发不自动跑，回答"软删数据会不会无限扩充"——跟书级回收站"不自动清空"同一条纪律）。网页「回收站」子视图**真列出**这三种终态条目的实际内容（页码/原文/转写文本），不是一个盲清的按钮。
- **~~分区~~ 整个概念砍掉**：`Section`/`Book.sections`/`Entry.section`/`Book::section_id_for_name()` 全删——AI 触发早就是 `Entry.ask_ai`/`question` 的事（二期已做），分区兼职的"笔记本排版分组"这半也不要了，条目一律按页序平铺，格式差异全靠 `Entry.style`。手写标记 `## 文字`/`### 文字`两个都覆盖 `Entry.subhead`（不分层级，`##` 曾被误删又按用户要求恢复识别）。

## 模型管理：预置下拉，不用自己填地址（2026-09-08）

`transcribe-serve`/`mind-serve` 的 `config.rs` 各维护一张 `Preset` 表（视觉/文字分开）：`PUT /config` 传 `preset` id，服务端查表原子设置 `model`+`base_url`，未知预置名直接拒绝；`"custom"` 是转义阀，留给真要接非 DashScope 的 OpenAI 兼容口。`GET /config`/`/status` 相应带 `presets`（表本身）+ `activePreset`（当前配置匹配哪个，不匹配算 `custom`）。key 脱敏显示后**只剩删除按钮**，不能直接改写——要换 key 先删再填。网页入口在**「管理」tab 的"模型管理"卡片**（不在笔记专属的「整理」区）。

## 四步闭环

```
① 合上书  ──事件驱动──►  ink-serve 矿     解析书页 .rm：勾画(GlyphRange 原文+矩形) ↔ 旁边手写(笔画簇) 几何配对（含无手写的纯勾画）→ 条目库(Mined) + 自渲染裁图
① .5 浏览 ──手机网页──►  「浏览」视图  按书→按页看裁图/引用，点「转入笔记」（Mined→Pending，纯勾画直接定稿）或「不需要」（→Skipped）
② 整理    ──手机网页──►  书架网关「笔记」tab   核对转写、选去处（设备/Obsidian/两处都要）、选样式、勾「问AI」填问题、「不要了」归档（e-ink 上改字太痛苦，设备只负责写）
③ 智能    ──按条目────►  transcribe-serve 转写手写（只处理 Pending）；mind-serve 按条目单发「问AI」问题跑文字模型
④ 投影    ──note-serve──►  设备《书名》文件夹一章一本（xochitl 7 种打字样式，去处含 Notebook/Both 才投）· vault/书名/第N章.md（反链，去处含 Obsidian/Both 才投；导出既落设备盘也直接触发浏览器下载）
```

![notes 数据流：四步闭环](docs/diagrams/data-flow.svg)

一句话原则：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影。**

## 架构：挂书架网关的 loopback 服务

```
浏览器 ──► shelf-gateway :8778 ──/api/<seg>/*──┬── ink-serve 127.0.0.1:8795 ──fswatch──► ~/.local/share/remarkable/xochitl（只读）
                 「笔记」tab（note-serve 注册）  ├── transcribe-serve :8796 ──订阅 ink /events──► DashScope（设备 WiFi 直连）
                 「管理」tab 模型管理卡片        ├── mind-serve :8797（纯被动，无订阅）──► DashScope（设备 WiFi 直连）
                 /api/events（area=notes）      ├── note-serve :8798 ──► xochitl /upload + vault/ 落盘
```

![notes 架构：条目库唯一写者 = ink-serve](docs/diagrams/architecture.svg)

- 注册表 / 反向代理 / 事件汇聚 / 管理台三态全是书架的机制（`shelf/README.md`）；网关 `manage::MODULES` 加四行即接入。
- 依赖方向单向无环：`services/* → shelf-core + crates/*`；**不依赖** `bookconv` / `device-core` / `knowledge/pkm` / `reading`。
- **零 xovi 依赖**：没有 qmd、没有 .so；建《书名》文件夹走书架的 `shelf-mkdir-agent.qmd`（依赖留在书架那一份）。
- 服务间只经 HTTP：条目库只有 ink-serve 写，转写/脑/本都 `POST /api/ink/books/{uuid}/entries/{id}` 改字段。

### 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| ink | `GET /books` → `{items:[{uuid,title,chapters,entries,pending}]}`（`list_active`，只列还有活条目的书）· `GET /books/{uuid}`（整份条目库：chapters/entries，**没有 sections 了**）· `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id} {text?|style?|destination?|draft?|answer?|askAi?|question?|subheadHint?}`（给 `text` 即已校对；`draft` 追加最新在前；`destination` 三期新增；**没有 `section`/`sectionHint` 了**）· `POST /books/{uuid}/entries/{id}/request`（浏览态"转入笔记"：`Mined→Pending`，纯勾画条目直接 `Reviewed`）· `POST /books/{uuid}/entries/{id}/skip`（"不需要"：`Mined→Skipped`）· `POST /books/{uuid}/entries/{id}/archive`（三期"不要了"：`→Archived`）· `POST /books/{uuid}/purge`（清空回收站：物理删 `Archived`/`Revoked`/`Skipped`，不可恢复）· `POST /books/{uuid}/rescan` · `GET /events` |
| transcribe | `GET /status` → `{config(无 key), usage, failures, inkReachable, pending}` · `GET /config`（带 `presets`/`activePreset`）· `PUT /config {preset?, apiKey?（只写）, clearKey?, model?, baseUrl?, auto?, maxPerRun?, pauseMs?, timeoutSecs?, maxAttempts?, prompt?}` · `POST /run`（同步跑一轮，回 `{scanned,done,failed,skipped,left,note}`）· `POST /books/{uuid}/entries/{id}`（强制转写一条）· `POST /retry`（清失败记录再跑）· `GET /events` |
| mind | `GET /status` → `{config(无 key), usage}` · `GET /config`（带 `presets`/`activePreset`）· `PUT /config {preset?, apiKey?（只写）, clearKey?, model?, baseUrl?, timeoutSecs?, prompt?}` · `POST /books/{uuid}/entries/{id}/ask`（回答这一条，要求已勾 `askAi` 且填了 `question`，否则 400）——**没有 `/events`**，纯被动，没有需要推送的状态 |
| notes | `GET /status` · `GET /books`（各书章节生成状态）· `GET /books/{uuid}/notebooks` · `POST /books/{uuid}/generate`（全书按需重投影+上传）· `POST /books/{uuid}/chapters/{idx}/generate`（单章，网页已接线）· `POST /books/{uuid}/export`（全书导出 md，落设备 vault）· `POST /books/{uuid}/chapters/{idx}/export`（单章，网页已接线，附带触发浏览器下载）· `GET /books/{uuid}/chapters/{idx}/export.md`（同一份内容当下载吐给浏览器，`Content-Disposition` + RFC 5987 文件名）· `GET /events` |

事件：`{"svc":"ink","area":"notes","kind":"entries"}`、`{"svc":"transcribe","area":"notes","kind":"transcribe"}` → 网页「笔记」tab 自动刷新。

## 目录

```
notes/
├── Cargo.toml · .cargo/               内部 workspace（与 shelf 同款 musl 全静态；`opt-level=z` + lto + strip）
├── crates/rmv6/                       .rm v6 解析+写入（剥离移植 remarkable_lines 0.1.3，MIT，PROVENANCE.md 留痕；page::Page = 笔画 + 勾画 + 打字文本，墓碑剔除；write.rs 编 RootTextBlock，模板替换拼 .rm，全部 7 种打字样式真机验证过）
├── crates/epubmap/                    .epubindex 起始页（两张表取首现）+ nav/ncx 目录 → 页号→章/小节
├── crates/notecore/                   领域核心（纯函数）：model 条目/样式/状态/去处（**没有分区了**）· hash FNV 簇指纹 · geom 聚簇+配对（**没有 has_underline 了**）· ingest 增量合并（含纯勾画路径） · marker 行首标记 OCR 兜底（`##`/`### ` 都覆盖 subhead） · project 条目库→段落列表投影（按页平铺，不分组） · export 条目库→Markdown 导出
├── services/ink-serve/                矿：doc(书库只读视图) · ingest(变更页编排) · crop(**自渲染裁图**，笔画矢量数据画折线，不吃缩略图) · bookdb(Repository) · config · main(路由+监听，接 askAi/question/destination + archive/purge 动作)
├── services/transcribe-serve/         转写：config(key/预置模型表/节制参数) · backend(Vision Strategy + OpenAiCompat) · prompt · ledger(用量) · ink(EntryStore 客户端) · worker(一轮编排) · main(SSE 订阅+防抖)
├── services/mind-serve/               脑：config(key/预置模型表，无节流字段) · backend(TextModel Strategy + OpenAiCompat，纯文本消息) · prompt(拼书名+章节+原文+文本+问题) · ledger(用量) · ink(EntryStore 客户端，book/post_answer) · worker::ask_entry(单条问答) · main(**无后台线程**，纯被动路由)
├── services/note-serve/               本：注册「笔记」tab；rmdoc.rs 打包 .rmdoc（上传复用 shelf-core::xochitl）；export.rs 落盘 vault + 浏览器下载的 content_disposition()；config/ink/trash/mkdir/notebooks/publish 生成编排+建夹
├── systemd/                           四个 .service（PartOf=shelf.target；随书架 install.sh 装，令牌 ink/transcribe/mind/note）
├── host/                              待建：CLI `notes pull`（把设备 vault/ 拉到本机 Obsidian vault；三期只做了"导出到设备"这一半）
├── testdata/renggu/                   真机 fixture（《人骨拼圖》墓碑页 .rm，测"解析成功零条目"）· renggu_marks/（同书真实勾画+手写）· seven_styles/（笔记本一页七样式，rmv6::write 模板）
└── docs/reMarkable笔记白皮书.md          决策 / 真机 / 踩坑（开头「现状总览」§00b）
```

网页部分在书架：`shelf/services/shelf-gateway/ui/app.js` 的 `renderNotes`（「浏览」/「整理」/「回收站」三个子视图）+ `renderManage` 里的"模型管理"卡片（`mountModelPanel()`）。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}` |
| 配置 | `~/.config/notes/ink.json`（clusterGap 40 / pairGap 120 真机验证有效未改；cropMargin 24 / debounceSecs 4）· `~/.config/notes/transcribe.json`（**0600**，含 apiKey，三期加 `preset`）· `~/.config/notes/mind.json`（**0600**，同上，字段比 transcribe.json 少——没有 maxPerRun/pauseMs/auto/maxAttempts） |
| 数据 | `~/.local/share/notes/crops/`（裁片 PNG，自渲染）· `~/.local/share/notes/vault/`（md 导出，§03r 已落地） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**）· `~/.local/state/notes/transcribe.json`（转写用量账本）· `~/.local/state/notes/mind.json`（问 AI 用量账本） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`——**绝不写** |

## 转写（transcribe-serve）

- 触发：订阅 ink-serve `/events`，`entries` 事件防抖 3 s 后跑一轮；启动追平一次；网页「转写状态」里「立即转写」按钮同步跑一轮；单条「转写/重转」强制跑。
- 一轮：列书 → 只看 `pending>0` 的书 → 条目 `needs_transcribe()`（簇指纹没有对应草稿）→ 取裁图 → 提示词（内置：只转写不发挥、汉字数字按原字符抄不转阿拉伯数字、+ 勾画原文作语境）→ 模型 → `POST ink /books/{uuid}/entries/{id}` 写 `draft{text,backend,at,hash}`。
  已校对 `text` 由 ink-serve 保证不被覆盖。行首标记兜底：条目样式仍是正文时，转写文本开头 `-`/`1.`/`口`/`##`/`### ` → 样式或小节标题并剥掉标记（`notecore::marker`）。
- 节制：每轮最多 `maxPerRun`（20）条、请求间歇 `pauseMs`（300）；同一条同指纹失败 `maxAttempts`（3）次后不再自动试（网页「重试失败」清）；一轮连续失败 3 次零成功即停（key 错/断网不一条条撞）。
- key：文件 → 环境 `DASHSCOPE_API_KEY`；`GET /config` 只报 `hasKey/keySource/keyMasked` **不回显**整串。模型选预置下拉（网关「管理」tab），不用自己填 baseUrl。用量账本只记次数/token/最近错误，不存内容。

## 问 AI（mind-serve）

- 触发：**只有网页点「提问」按钮**——不像 transcribe-serve 订阅事件自动跑，`mind-serve` 没有后台线程、没有 `/events`，纯被动等 HTTP 请求，空闲零 CPU。
- 前提：条目要先勾「问AI」+ 填了问题（两者都是 `ink-serve` 存的字段，`askAi`/`question`）——`POST /entries/{id}/ask` 会先查这两条，没满足直接 400，不会误触。
- 一次问答：拼提示词（书名 + 章节 + 勾画原文 + 已校对/转写文本 + 问题，字段可选、超长截断）→ 文字模型 → 写回 `answer{text,backend,at,brief}`（`brief` 存的是问题原文）。
- key/模型选择跟 transcribe-serve 同一套模式（各自独立的 `mind.json`/账本/预置表），但**没有节流字段**（没有批量循环可节流）。

## 去处、删除与回收站

条目校对/问答完之后，`destination` 决定它出现在哪：`Notebook`（只留设备笔记本）/ `Obsidian`（只导出）/ `Both`（缺省，两处都要）。不想要了点「不要了」→ `Archived`（软删，两处投影都摘掉，但条目库里还留着）。「回收站」子视图列出 `Skipped`/`Revoked`/`Archived` 三种终态条目的实际内容（不是纯按钮），确认无误后点「清空回收站」才是真删（`Book::purge_terminal()`，不可恢复，手动触发不自动跑）。

## 增量规则（回答"二次识别会不会把改好的字覆盖回去"）

条目库是事实源，笔记本与 md 只是投影。每片手写按笔画集合算指纹：指纹不变 → 不重识别、不动校对文本；补了几笔 → 同一条目、新草稿只作建议；
笔画全擦 → 标「已撤销」不删。校对过的字永远不会被覆盖。条目 id 创建时按 (书, 页, 首笔 id) 一次算定，之后永不重算。

## 构建 · 部署

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host：100 个测试（rmv6 7 · epubmap 5 · notecore 40 · ink 10 · transcribe 12 · mind 13 · note 13）
cd ../shelf && ./build.sh && ./deploy.sh <设备IP>                  # 随书架一起交叉编译/打包/装机（NOTES_BINS；设备在 WiFi 上时给 WiFi IP）
ssh root@<设备IP> sh /home/root/shelf-pkg/shelf/install.sh --only ink,transcribe,mind,note   # 只装/更新笔记线
```
卸载走书架 `uninstall.sh`（笔记线四令牌同在；条目库不在 `--purge` 范围）。

## 还没做的

- host `notes/host/bin/notes pull`：把设备 `vault/` 拉到本机 Obsidian vault——三期只做了"导出到设备+浏览器下载"这一半。
- 前端可视渲染人眼确认：浏览页/回收站/模型管理面板/条目卡片重设计，后端数据链路都真机走通，但没有浏览器渲染工具，实际排版效果没人看过。
- `### `/`## ` 小节标记真机复验：后端已接线、离线单测全绿，两轮真机复验卡在手写行草连笔的 OCR 准确率，不是代码问题。
- transcribe 转写质量持续打磨（汉字数字误认、裁图边界样本）。
- `archive`/`purge` 两个端点没有对真实历史数据实测过（一次性不可逆动作，底层逻辑单测覆盖充分，没事先问用户不该拿真实数据练手）。

演进记录、每一步的真机验证细节、踩过的坑，见 `docs/reMarkable笔记白皮书.md`。
