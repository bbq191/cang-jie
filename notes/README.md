# notes —— reMarkable 笔记线

> **读者与用途**：想弄清“荧光笔勾画 + 手写批注怎么变成可整理的笔记”的人：先看下面的一句话与总览图，再看服务表；整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)；决策、真机记录与踩坑在 [`docs/reMarkable笔记白皮书.md`](docs/reMarkable笔记白皮书.md)（开头有“5 分钟读懂”）。

## 一句话：它是什么

reMarkable 的强项是**荧光笔勾书 + 在勾出来的内容旁边直接手写**。笔记线把这些勾画与手写**合上书后自动收进一个条目库**，你在手机网页上挑要转成笔记的、修正转写、按条目问 AI，每条内容再选去哪——设备笔记本、Obsidian（md）、或两处；设备笔记本与 md 都由条目库**投影**生成。

> 原则：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影。**
> 不引入任何旧 `knowledge/pkm` 代码，只借鉴功能与踩坑。

![笔记线一图读懂](docs/diagrams/overview.svg)

## 四个服务

都是挂在网关（[`../gateway`](../gateway/README.md)）下的 loopback 服务，机制（注册表 / 反向代理 / 事件汇聚 / 管理台）全是网关与 [`../rmsvc-core`](../rmsvc-core/README.md) 的；网关 `manage::MODULES` 里加四行即接入。

| 服务 | seg / 端口 | 职责 | 状态（详见白皮书 §00b、§05） |
|---|---|---|---|
| `ink-serve` 矿 | `ink` / 8795 | 监听书库 → 只扫变更页 → 勾画 ↔ 旁边手写配对（含无手写的纯勾画）→ **自渲染裁图** → **条目库（唯一写者）**；另有 `POST /koreader/import`（§03al）手动拉 `koreader-serve` 的高亮/生词并入条目库 | 真机 active；浏览态状态机、纯勾画、自渲染裁图、归档/回收站/恢复、KOReader 回流均真机验证；KOReader 回流**无网页按钮，只能 curl** |
| `transcribe-serve` 转写 | `transcribe` / 8796 | 订阅矿的事件 → 裁图喂视觉模型 → 草稿写回（行首标记自动定样式）；只处理 `Pending`；出网 | 真机 active，DashScope 真调过；OpenAI/Gemini/DeepSeek 预置只验证了配置层；转写准确率还在打磨 |
| `mind-serve` 脑 | `mind` / 8797 | 按条目单发：勾「问AI」+ 输入问题 → 拼书名+章节+勾画原文+转写文本+问题 → 文字模型 → `answer`；**无批量循环、无事件订阅**，纯被动；出网 | 真机 active，端到端问答通过（DashScope） |
| `note-serve` 本 | `notes` / 8798 | 注册「笔记」tab；打包 `.rmdoc`（全部 7 种打字样式）+ 上传 + 条目→文档生成（落书本自己所在的设备文件夹）；**md 导出**（落设备 vault + 直接触发浏览器下载）；**单篇 markdown 导入**（`POST /import-md`，独立于条目库，网页入口默认隐藏） | 真机验证：三件套 + 生成编排 + 导出；md 导入只验证了后端管线，前端入口未经人眼确认 |

**模型**：`transcribe`/`mind` 各维护一张预置表（视觉/文字分开），横跨 DashScope / OpenAI / Gemini / DeepSeek 四厂商，**key 按厂商分开存**；入口在网关「管理」tab 的“模型管理”卡片（不在笔记页）。豆包因“模型”是账号自建接入点，不进预置表，走“自定义”。结构图见白皮书 §03u。

## 四步闭环

```
① 合上书  ──事件驱动──►  ink-serve 矿     解析书页 .rm：勾画(GlyphRange 原文+矩形) ↔ 旁边手写(笔画簇) 几何配对（含无手写的纯勾画）→ 条目库(Mined) + 自渲染裁图
① .5 浏览 ──手机网页──►  「浏览」视图  按书→按页看裁图/引用，点「转入笔记」（Mined→Pending，纯勾画直接定稿）或「不需要」（→Skipped）
② 整理    ──手机网页──►  网关「笔记」tab   核对转写、选去处（设备/Obsidian/两处）、勾「问AI」填问题、「不要了」归档（e-ink 上改字太痛苦，设备只负责写）
③ 智能    ──按条目────►  transcribe-serve 转写手写（只处理 Pending）；mind-serve 按条目单发「问AI」
④ 投影    ──note-serve──►  书本自己所在的设备文件夹一章一本（7 种打字样式）· vault/书名/第N章.md（反链）——按每条的“去处”决定投哪边
```

![notes 数据流：四步闭环](docs/diagrams/data-flow.svg)

![notes 架构：条目库唯一写者 = ink-serve](docs/diagrams/architecture.svg)

- **零 xovi 依赖**：没有 qmd、没有 .so；笔记本直接复用书本自己已在的设备文件夹。
- 依赖方向单向无环：`services/* → ../rmsvc-core + crates/*`；**不依赖** `bookconv` / `device-core` / `knowledge/pkm` / `reading`。
- 服务间只经 HTTP：条目库只有 ink-serve 写，转写/脑/本都 `POST /api/ink/books/{uuid}/entries/{id}` 改字段。

## 「整理」页与条目

「整理」页是「未导出 / 已导出」两个 tab，tab 下章节做成第二层标签、点哪章只显示哪一章；每条一张卡片（裁图、勾画引用、文本框、去处循环按钮、问 AI 区、重新转写/不要了两个独立按钮）；章头「推送本章」按每条的去处一次做完笔记本与 md。详见白皮书 §03ad，图见 [`docs/diagrams/organize-page.svg`](docs/diagrams/organize-page.svg)。

**去处、删除与回收站**：`Entry.destination`（`Notebook`/`Obsidian`/`Both`，缺省 `Both`）决定它出现在哪个投影。“不要了”→ `Archived`（软删，两处投影摘掉，条目库里还留着）；「回收站」列出 `Skipped`/`Revoked`/`Archived` 三种终态条目的实际内容，每条可「恢复」（`Entry::restore()` 按已有内容倒推落点）或「全部恢复」；「清空回收站」（`Book::purge_terminal()`）才是不可恢复的真删，手动触发、不自动跑。恢复找回的是条目库存档，**不会让笔迹重新出现在设备页面**。三处写入口对终态条目都有守卫。

![notes 条目状态机：7 态 + restore()](docs/diagrams/entry-status.svg)

**手写约定**：条目文本行首 `-`/`1.`/`口`/`##`/`### ` 自动识别成无序/有序/待办/小节（`notecore::marker`；网页打字与手写转写是同一套约定，不用另选样式）。

**增量规则**（回答“二次识别会不会把改好的字覆盖回去”）：每片手写按笔画集合算指纹；指纹不变→不重识别、不动校对文本；补了几笔→同一条目、新草稿只作建议；笔画全擦→标 `Revoked` 不删。**校对过的字永远不会被覆盖**；条目 id 创建时一次算定、永不重算。

## 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| ink | `GET /books`（`{items:[{uuid,title,chapters,entries,pending}]}`，只列还有活条目的书）· `GET /books/{uuid}`（整份条目库）· `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id} {text?\|style?\|destination?\|draft?\|answer?\|askAi?\|question?\|subheadHint?}`（`text` 走 `Entry::apply_marked_text`：行首标记自动定样式/覆盖 subhead 并剥掉标记；`draft` 追加最新在前）· `POST …/entries/{id}/request`（转入笔记：`Mined→Pending`，纯勾画直接 `Reviewed`）· `…/skip`（不需要）· `…/archive`（不要了）· `…/restore`（`Skipped`/`Revoked`/`Archived` 恢复，非终态拒绝）· `POST /books/{uuid}/purge`（清空回收站，不可恢复）· `POST /books/{uuid}/rescan` · `POST /koreader/import` · `GET /events` |
| transcribe | `GET /status`（`{config(无 key), usage, usageByModel, failures, inkReachable, pending}`）· `GET /config`（带 `presets`/`activePreset`/`price`）· `PUT /config {preset?, apiKey?（只写，存进当前厂商）, clearKey?, price?{input,output}, model?, baseUrl?（仅 preset="custom"）, auto?, maxPerRun?, pauseMs?, timeoutSecs?, maxAttempts?, prompt?}` · `POST /run`（同步跑一轮，回 `{scanned,done,failed,skipped,left,note,…}`）· `POST /books/{uuid}/entries/{id}`（强制转写一条，回 token 消耗）· `POST /retry`（清失败记录再跑）· `GET /events` |
| mind | `GET /status` · `GET /config` · `PUT /config`（同 transcribe，但没有 `auto/maxPerRun/pauseMs/maxAttempts`）· `POST /books/{uuid}/entries/{id}/ask`（要求条目已勾 `askAi` 且填了 `question`，否则 400）——**没有 `/events`** |
| notes | `GET /status` · `GET /books`（各书章节生成状态）· `GET /books/{uuid}/notebooks` · `GET /books/{uuid}/exports` · `GET /books/{uuid}/sync`（每章设备笔记本/Obsidian 是否与当前条目同步，含 `*Needed`/`*Synced`/`*GeneratedAt`）· `POST /books/{uuid}/generate`（全书）· `POST /books/{uuid}/chapters/{idx}/generate`（单章）· `POST /books/{uuid}/import-md {title, markdown}` · `POST /books/{uuid}/export`（全书导出 md，指纹没变跳过）· `POST /books/{uuid}/chapters/{idx}/export`（单章，回 `status`：written/unchanged/empty）· `GET /books/{uuid}/chapters/{idx}/export.md`（同一份内容当下载吐给浏览器，`Content-Disposition` + RFC 5987 文件名）· `GET /books/{uuid}/vault.json`（读回已落盘的 vault 文件，纯读；原给已砍的 host `shelf notes pull` 用，现无自动化消费方）· `GET /events` |

事件：`{"svc":"ink","area":"notes","kind":"entries"}`、`{"svc":"transcribe","area":"notes","kind":"transcribe"}` → 网页「笔记」tab 自动刷新。

## 目录

```
notes/
├── Cargo.toml · .cargo/               内部 workspace（musl 全静态；opt-level=z + lto + strip；panic=unwind）
├── crates/rmv6/                       .rm v6 解析+写入（剥离移植 remarkable_lines 0.1.3，MIT，PROVENANCE.md 留痕；write.rs 编 RootTextBlock，7 种打字样式真机验证过）
├── crates/epubmap/                    .epubindex 起始页 + nav/ncx 目录 → 页号→章/小节
├── crates/notecore/                   领域核心（纯函数）：model · hash · geom · ingest · koreader · marker · project · export · mdimport
├── crates/vendorcfg/                  AI 厂商预置/key 按厂商分存/迁移/PATCH/用量账本/ConfigCell/OpenAI 兼容传输，transcribe/mind 共用；只抽行为不抽数据结构
├── services/ink-serve/                矿：doc · ingest · crop（自渲染裁图）· bookdb · config · koreader · main
├── services/transcribe-serve/         转写：config/ledger（vendorcfg 薄封装）· backend · prompt · ink · worker · main
├── services/mind-serve/               脑：同上，文字模型；main 无后台线程
├── services/note-serve/               本：rmdoc · export · chapter_store · publish · config · ink/trash · main
├── systemd/                           四个 .service（PartOf=shelf.target；随书架 install.sh 装，令牌 ink/transcribe/mind/note）
├── testdata/                          真机 fixture（renggu 墓碑页 / renggu_marks 真实勾画+手写 / seven_styles 七样式）
└── docs/                              白皮书 + diagrams/（overview · data-flow · architecture · entry-status · organize-page · model-config）
```

网页部分在网关：[`../gateway/ui/app.js`](../gateway/ui/app.js) 的 `renderNotes`（「浏览」/「整理」/「回收站」/「导入 md 文档」四个子视图；「导入 md 文档」默认隐藏，由书架「管理→实验室」的 `notesImportMdEnabled` 开关控制，交互是选 `.md` 文件上传）+ `renderManage` 里的“模型管理”卡片（`mountModelPanel()`）。网页正文支持中英文切换（`notes.*` i18n 命名空间）。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}` |
| 配置 | `~/.config/notes/ink.json`（聚簇/配对阈值、页几何 960×1280、裁图边距、防抖）· `transcribe.json` / `mind.json`（**0600**；`preset` + `keys`（厂商→key）+ `prices`） |
| 数据 | `~/.local/share/notes/crops/`（裁片 PNG）· `~/.local/share/notes/vault/`（md 导出） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**）· `notebooks/`、`exports/`（每章生成/导出簿记）· `transcribe.json`、`mind.json`（用量账本） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`——**绝不写** |

## 构建 · 部署

**前置依赖**：与书架共用同一套交叉编译环境（`rustup target add aarch64-unknown-linux-musl` + aarch64 交叉 gcc/ar），见 `../shelf/README.md`「构建」。改代码前先看工程纪律；日常在 master 上开 feature 分支（`dev` 已于 2026-09-16 删除）。

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host：约 200 个测试（rmv6 27 · epubmap 5 · notecore 62 · vendorcfg 18 · ink 15 · transcribe 22 · mind 21 · note 32，含 1 ignored；claim 重试两条测试真吃约 1.5–4.5 s）
cd ../shelf && ./build.sh && ./deploy.sh <设备IP>                  # 随书架一起交叉编译/打包/装机（NOTES_BINS；设备在 WiFi 上时给 WiFi IP）
ssh root@<设备IP> sh /home/root/shelf-pkg/shelf/install.sh --only ink,transcribe,mind,note   # 只装/更新笔记线
```

卸载走书架 `uninstall.sh`（笔记线四令牌同在；条目库不在 `--purge` 范围）。

## 还没做的

完整清单（含依据）见白皮书 §05“未闭环”表，要点：

- 前端可视渲染人眼确认（浏览页 / 模型管理 / 条目卡片 / 「整理」双层 tab / 「导入 md 文档」入口）；
- 转写质量（汉字数字被认成阿拉伯数字；`##`/`###` 标记至今没有一次“转写对了、标记被认出”的真机正例）；
- OpenAI / Gemini / DeepSeek 预置只验证了配置层，没真实调用过（并注意 DeepSeek 的两个预置 id 已是官方遗留名，见白皮书 §05）；
- KOReader 回流没有网页触发按钮；
- `archive` / `purge` 没对真实历史数据实测；被动触发的两处修复（终态判据、认领重试）没在真机复验。

演进记录、每一步的真机验证细节、踩过的坑，见 [`docs/reMarkable笔记白皮书.md`](docs/reMarkable笔记白皮书.md)。
