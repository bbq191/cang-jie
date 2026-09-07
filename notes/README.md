# notes —— reMarkable 笔记线

书架（`shelf/`）补读书短板，笔记线做强 reMarkable 的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。合上书自动摄取成条目，手机上修正，按分区调智能，设备笔记本与 md 都由条目库投影生成。**不引入任何旧 `knowledge/pkm` 代码**，只借鉴功能与踩坑。决策/真机/踩坑见 `docs/reMarkable笔记白皮书.md`（开头「现状总览」）。

## 现状（2026-09-06 晚）

| 服务 | seg / 端口 | 职责 | 状态 |
|---|---|---|---|
| `ink-serve` 矿 | `ink` / 8795 | 监听书库 → 只扫变更页 → 勾画 ↔ 旁边手写配对 → 裁图 → **条目库（唯一写者）**；零网络 | ✅ 真机 active，扫到《人骨拼圖》38 章 |
| `transcribe-serve` 转写 | `transcribe` / 8796 | 订阅矿的事件 → 裁图喂视觉模型（Qwen 缺省，OpenAI 兼容口可换）→ 草稿写回；唯一出网 | ✅ 真机 active；真调模型待 key + 样本 |
| `note-serve` 本 | `notes` / 8798 | 注册「笔记」tab；条目库 → 《书名》文件夹一章一本 + md 导出 | 骨架（tab 已注册），投影待建 |
| `mind-serve` 脑 | `mind` / 8797 | 按分区跑文本模型（分区简述 = 提示词），回答写回 | 待建 |

**卡点**：真机样本（勾三段写三行 + `-`/`1.`/`口`/下划线；七样式笔记本）用来标定聚簇/配对阈值与页坐标几何；现有样本页全是墓碑笔画。

## 四步闭环

```
① 合上书  ──事件驱动──►  ink-serve 矿     解析书页 .rm：勾画(GlyphRange 原文+矩形) ↔ 旁边手写(笔画簇) 几何配对 → 条目库 + 裁图
② 修正    ──手机网页──►  书架网关「笔记」tab   核对转写、选分区、选样式（e-ink 上改字太痛苦，设备只负责写）
③ 智能    ──按分区────►  transcribe-serve 转写手写；mind-serve 按「分区名 + 简述」跑模型（"背诵"类不调）
④ 投影    ──note-serve──►  设备《书名》文件夹一章一本（xochitl 7 种打字样式）· vault/书名/第N章.md（反链）
```

一句话原则：**设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影。**

## 架构：挂书架网关的 loopback 服务

```
浏览器 ──► shelf-gateway :8778 ──/api/<seg>/*──┬── ink-serve 127.0.0.1:8795 ──fswatch──► ~/.local/share/remarkable/xochitl（只读）
                 「笔记」tab（note-serve 注册）  ├── transcribe-serve :8796 ──订阅 ink /events──► DashScope（设备 WiFi 直连）
                 /api/events（area=notes）      ├── note-serve :8798 ──► xochitl /upload（待建）
                                               └── mind-serve :8797（待建）
```

- 注册表 / 反向代理 / 事件汇聚 / 管理台三态全是书架的机制（`shelf/README.md`）；网关 `manage::MODULES` 加三行即接入。
- 依赖方向单向无环：`services/* → shelf-core + crates/*`；**不依赖** `bookconv` / `device-core` / `knowledge/pkm` / `reading`。
- **零 xovi 依赖**：没有 qmd、没有 .so；将来建《书名》文件夹走书架的 Sidebar 代理 qmd（依赖留在书架那一份）。
- 服务间只经 HTTP：条目库只有 ink-serve 写，转写/脑/本都 `POST /api/ink/books/{uuid}/entries/{id}` 改字段。

### 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| ink | `GET /books` → `{items:[{uuid,title,chapters,entries,pending}]}` · `GET /books/{uuid}`（整份条目库：chapters/sections/entries）· `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id} {text?|section?|style?|draft?|answer?}`（给 `text` 即已校对；`draft` 追加最新在前）· `PUT /books/{uuid}/sections {sections}` · `POST /books/{uuid}/rescan` · `GET /events` |
| transcribe | `GET /status` → `{config(无 key), usage, failures, inkReachable, pending}` · `GET /config` · `PUT /config {apiKey?（只写）, clearKey?, model?, baseUrl?, auto?, maxPerRun?, pauseMs?, timeoutSecs?, maxAttempts?, prompt?}` · `POST /run`（同步跑一轮，回 `{scanned,done,failed,skipped,left,note}`）· `POST /books/{uuid}/entries/{id}`（强制转写一条）· `POST /retry`（清失败记录再跑）· `GET /events` |
| notes | `GET /status` · `GET /events`（投影/导出待建） |

事件：`{"svc":"ink","area":"notes","kind":"entries|sections"}`、`{"svc":"transcribe","area":"notes","kind":"transcribe"}` → 网页「笔记」tab 自动刷新。

## 目录

```
notes/
├── Cargo.toml · .cargo/               内部 workspace（与 shelf 同款 musl 全静态；`opt-level=z` + lto + strip）
├── crates/rmv6/                       .rm v6 只读解析（剥离移植 remarkable_lines 0.1.3，MIT，PROVENANCE.md 留痕；page::Page = 笔画 + 勾画 + 打字文本，墓碑剔除）
├── crates/epubmap/                    .epubindex 起始页（两张表取首现）+ nav/ncx 目录 → 页号→章/小节
├── crates/notecore/                   领域核心（纯函数）：model 条目/分区 · hash FNV 簇指纹 · geom 聚簇+配对 · ingest 增量合并 · marker 行首标记 OCR 兜底
├── services/ink-serve/                矿：doc(书库只读视图) · ingest(变更页编排) · crop(页坐标→缩略图像素) · bookdb(Repository) · config · main(路由+监听)
├── services/transcribe-serve/         转写：config(key/节制) · backend(Vision Strategy + OpenAiCompat) · prompt · ledger(用量) · ink(EntryStore 客户端) · worker(一轮编排) · main(SSE 订阅+防抖)
├── services/note-serve/               本：骨架（注册「笔记」tab）
├── systemd/                           三个 .service（PartOf=shelf.target；随书架 install.sh 装）
├── host/                              待建：CLI `notes pull`（md 同步到 Obsidian vault）
├── testdata/renggu/                   真机 fixture（《人骨拼圖》.content/.epubindex/toc.ncx/content.opf/墓碑页 .rm + 缩略图）
└── docs/reMarkable笔记白皮书.md          决策 / 真机 / 踩坑（开头「现状总览」）
```

网页部分在书架：`shelf/services/shelf-gateway/ui/app.js` 的 `renderNotes`（按书→按章列条目：裁图 / 文本框改即存 / 分区 / 样式 / 转写区）。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,note-serve}` |
| 配置 | `~/.config/notes/ink.json`（clusterGap 40 / pairGap 120 真机验证有效未改；**pageWidth 960 / pageHeight 1280**——2026-09-07 真机测出旧 1404×1872 是错的，见白皮书 §03g；xOriginCenter / cropMargin 24 / debounceSecs 4）· `~/.config/notes/transcribe.json`（**0600**，含 apiKey） |
| 数据 | `~/.local/share/notes/crops/`（裁片 PNG）· `~/.local/share/notes/vault/`（md，待建） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**）· `~/.local/state/notes/transcribe.json`（用量账本） |
| 只读外部 | xochitl 书库 `~/.local/share/remarkable/xochitl/`——**绝不写** |

## 转写（transcribe-serve）

- 触发：订阅 ink-serve `/events`，`entries` 事件防抖 3 s 后跑一轮；启动追平一次；网页「转写」按钮同步跑一轮；单条「重转」强制跑。
- 一轮：列书 → 只看 `pending>0` 的书 → 条目 `needs_transcribe()`（簇指纹没有对应草稿）→ 取裁图 → 提示词（内置 + 勾画原文作语境）→ 模型 → `POST ink /books/{uuid}/entries/{id}` 写 `draft{text,backend,at,hash}`。
  已校对 `text` 由 ink-serve 保证不被覆盖。行首标记兜底：条目样式仍是正文时，转写文本开头 `-`/`1.`/`口` → 无序/有序/待办并剥掉标记（`notecore::marker`）。
- 节制：每轮最多 `maxPerRun`（20）条、请求间歇 `pauseMs`（300）；同一条同指纹失败 `maxAttempts`（3）次后不再自动试（网页「重试失败」清）；一轮连续失败 3 次零成功即停（key 错/断网不一条条撞）。
- key：文件 → 环境 `DASHSCOPE_API_KEY`；`GET /config` 只报 `hasKey/keySource` **不回显**。用量账本只记次数/token/最近错误，不存内容。
- 后端可换：`baseUrl`/`model` 改成任何 OpenAI 兼容口（`POST {baseUrl}/chat/completions` + `image_url` data URI）；设备走自己的 WiFi 直连，不经 host 代理。

## 增量规则（回答"二次识别会不会把改好的字覆盖回去"）

条目库是事实源，笔记本与 md 只是投影。每片手写按笔画集合算指纹：指纹不变 → 不重识别、不动校对文本；补了几笔 → 同一条目、新草稿只作建议；
笔画全擦 → 标「已撤销」不删。校对过的字永远不会被覆盖。条目 id 创建时按 (书, 页, 首笔 id) 一次算定，之后永不重算。

## 构建 · 部署

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host：37 个测试（rmv6 4 · epubmap 5 · notecore 11 · ink 9 · transcribe 8）
cd ../shelf && ./build.sh && ./deploy.sh <设备IP>                  # 随书架一起交叉编译/打包/装机（NOTES_BINS；设备在 WiFi 上时给 WiFi IP）
ssh root@<设备IP> sh /home/root/shelf-pkg/shelf/install.sh --only ink,transcribe,note   # 只装/更新笔记线
```
卸载走书架 `uninstall.sh`（笔记线三令牌同在；条目库不在 `--purge` 范围）。

## 演进记录（详见白皮书各节）

| 阶段 | 内容 | 状态 |
|---|---|---|
| 定案 | 旧 pkm 退役；四服务；条目库唯一事实源；手机修正设备只读；一章一本《书名》夹；分区 = 名字+简述+是否调模型 | ✅ 2026-09-06 用户拍板（§00/§01） |
| 地基 | `rmv6`（剥离移植 + CHECKBOX 码）· `epubmap`（两张表取首现 + nav/ncx）· `notecore`（模型/指纹/聚簇配对/增量合并/行首标记） | ✅ 离线 31 测（§03b） |
| 矿 | ink-serve：fswatch 只扫变更页、缩略图裁片、条目库唯一写者、HTTP + 事件 | ✅ 真机 active、扫到 38 章（§03c） |
| 网关接入 | MODULES 三行、「笔记」tab（裁图/文本/分区/样式改即存）、部署链 NOTES_BINS/令牌 | ✅ 真机 tab 注册（§03d） |
| 转写 | transcribe-serve：Vision Strategy（Qwen 缺省）、限量/失败上限/即停、key 只写不读 0600、用量账本、网页转写区 | ✅ 修完裁图坐标（§03g）真机重转复验：3 条有勾画的条目 2 条读对手写、1 条仍读错（裁图边距混印刷体，留质量项）；1 条无勾画批注裁不到，新守卫优雅跳过 |
| 步骤 0 | 真机样本标定阈值/页几何/样式码 | ✅ 2026-09-07：聚簇/配对阈值验证通过、NumberedList 码=10（§03f）；★页坐标画布尺寸原假设是错的，真机反测坐实 960×1280、已修复部署复验（§03g） |
| 本 / 脑 / 导出 | note-serve 投影（7 样式）+ 代理 mkdir · mind-serve · md + `notes pull` | ⏳ 待建（§05）；NumberedList 写入前还差一份多行样本 |
