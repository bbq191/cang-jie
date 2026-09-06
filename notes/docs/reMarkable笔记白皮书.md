# reMarkable 笔记线（notes）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。现状看 §00b；待办看 §05；踩坑看 §04。总计划 未入库的计划文件。

## 00｜定位与原则（2026-09-06 用户定）

书架（`shelf/`）补 reMarkable 读书短板；笔记线做强它的强项：**荧光笔勾书 + 在勾出来的内容旁边直接手写**。文学书勾作者名写"查作者"，学术书勾公式写"?/没听懂"或"!/背诵"——凡是勾了、写了的都该汇入笔记。旧 `knowledge/pkm`（画星待办、六色槽卡片、跨书汇编、复盘队列、仪表）判"太花哨"（颜色即路由的产物），**全部退役；新建 `notes/`，不引入任何旧代码，只借鉴功能与踩坑**。

四步闭环：① 合上书自动摄取（事件驱动）→ ② **手机上修正**（e-ink 打字太痛苦；不再引入中文化/输入法）→ ③ 按分区调智能（**分区 = 名字 + 简述 + 是否调模型，简述就是给 AI 的要求**；"背诵"不调）→ ④ 可选导出 md，一章一文件，与设备笔记本同构，反链。

工程原则同书架：XDG · 设计模式去重解耦 · 专项专用可插拔 · **设备只负责写，不负责改；改在手机，回写靠重建；条目库是唯一事实源，笔记本与 md 都是投影**。用户另定：四个服务（转写与智能分开）；设备笔记本一章一本、放《书名》文件夹、只读；分区自定义；首期只 EPUB。

## 00b｜现状总览（2026-09-06 晚）

- `crates/rmv6`：`.rm` v6 只读解析，剥离移植 vendored `remarkable_lines` 0.1.3（MIT，PROVENANCE 留痕）只留 v6；补 CHECKBOX(6/7) 样式码、未知工具兜底；高层 `page::Page`（笔画 / 勾画 GlyphRange / 打字文本，墓碑剔除）。
- `crates/epubmap`：`.epubindex`（两张表取首现的第一个 u32 = 起始页）+ nav/ncx 两策略统一成带层级目录 → `chapter_of(page)` 给章（1 级祖先）与小节。
- `crates/notecore`：条目/分区模型、FNV 簇指纹、并查集聚簇 + 就近配对勾画、增量合并规则。
- `services/ink-serve`（8795）：监听书库（fswatch 非递归：合上书 xochitl 重写 `.content/.metadata` 触发）→ 只扫变更页 → 摄取 → 缩略图裁片 → 条目库 `~/.local/state/notes/books/<uuid>.json`（**唯一写者**：其它服务经 `POST /books/{uuid}/entries/{id}` 改字段）。真机：active、注册、扫到《人骨拼圖》38 章。
- `services/transcribe-serve`（8796，2026-09-06 夜建、**真机待验**）：订阅 ink 事件 → 待转写条目取裁图 → `OpenAiCompat`（Strategy：`POST {baseUrl}/chat/completions` + `image_url` data URI，DashScope Qwen `qwen3-vl-plus` 缺省，任何 OpenAI 兼容口只改配置）→ 草稿经 ink-serve HTTP 写回（不直碰条目库）。编排 `worker::run_once` 全 trait 注入（`EntryStore`/`Vision`），内存桩单测覆盖：限量、失败上限、强制重转、指纹变计数归零、连续失败即停。行首标记 OCR 兜底 `notecore::marker`（几何优先，仍是正文时才按转写文本认 `-`/`1.`/`口` 并剥标记）。key 只写不读、文件 0600、环境变量兜底；用量账本只记数。网页笔记页加「转写」区（key/模型/baseUrl/自动开关/跑一轮/重试失败/失败清单）与每条「转写/重转」。
- `services/note-serve`（8798）：注册「笔记」tab；投影/导出待建。
- 网关「笔记」tab：按书→按章列条目，左裁图右文本框，改即存；分区编辑（名/简述/调模型）。
- 部署：随书架 `deploy.sh` 一起（build.sh 顺带编 `../notes`，载荷带其二进制与单元，install/uninstall 令牌 `ink`/`transcribe`/`note`，网关 `MODULES` 三行）。分支：2026-09-06 起笔记线全部提交到 `dev`（用户定）。

待建：mind-serve、note-serve 投影（7 样式写入器）与 md 导出、书库动作代理扩展（`createCollection` 建《书名》夹）。transcribe-serve 待真机验（设备当时离线）：服务 active/注册、网页粘 key、有样本后看转写与校对流。

## 01｜真机事实

- EPUB 页 `.rm`：勾画 = `SceneGlyphItem`（GlyphRange：原文 + 字符偏移 + 每行矩形），手写 = `SceneLineItem` 笔画，**同一坐标系**；擦掉的项是墓碑（`item.value` 为空）。《人骨拼圖》c65fa2ae 页 23 笔全是墓碑（用户画了又擦）——fixture 只能测"解析成功零条目"，真样本待步骤 0。
- 3.28 打字段落样式 7 种（格式菜单 qml_00db4610）：Title / Subheading 1 / Subheading 2 / Body / Bulletpoint / NumberedList / CheckboxUnchecked；rmscene 已知码 0–7（CHECKBOX=6/7），有序列表码待样本读回。
- `.content`（formatVersion 1）：`pages` 是页 id 顺序表（523 项），下标 = 页号，与 `.epubindex` 起始页对齐；`.epubindex` 有两张表（第一张 `路径+起始页+flag+?`，第二张 `路径+字符偏移+长度+起始页`）。
- 缩略图 `<uuid>.thumbnails/<page>.png` **384×512**（3:4；封面 954 宽）——页坐标系按经典 1404×1872、x 原点页中线假设，待样本核。
- 3.28 QML：`Library.createCollection`（`import xofm.libs.library`）、`explorer.removeAllTrashed()`（emptyTrash 的内核）、`selectionRestoreTrashed`——建夹/清空回收站都能由 Sidebar 代理代劳。

## 02｜手写约定 ↔ 7 种样式

| 样式 | 笔记本用途 | 手写约定 | 判法 |
|---|---|---|---|
| Title | 页标题 = 章名 | 无 | epubmap |
| Subheading 1 | 分区头（名 + 简述 = AI 要求） | 一行字 + 下面长横 | 几何 |
| Subheading 2 | 勾画所在小节 | 无 | epubmap |
| Body | 转写正文 / AI 回答 | 普通书写 | OCR |
| Bulletpoint | 无序 | 行首短横 / 实心点 | 几何 |
| NumberedList | 有序 | 行首 `1.` | OCR |
| Checkbox | 待办 | 行首空心小方框 | 几何 |

## 03｜设计取舍（转写）

- **为什么转写不直接写条目库文件**：条目库唯一写者是 ink-serve，其它服务一律走它的 HTTP；否则两进程同写一个 JSON 迟早互相覆盖（书架 sidecar 早期踩过同类问题）。
- **为什么一轮限量 + 连续失败即停**：一次合书可能几十条；key 错或断网时不该一条条撞到超时（每条 60 s）。停下来只影响本轮，事件再来会再试，失败上限保证不会无限烧。
- **为什么行首标记在转写这边兜底**：`1.` 几何认不出（数字形状不定），表里本就写"OCR 判"；几何（ink-serve）判出的不覆盖，只在样式仍为正文时认。标记剥掉是因为笔记本样式自带编号/符号。
- **ureq 2 默认特性没有 `json`**：`Response::into_json` 不可用，用 `serde_json::from_reader(resp.into_reader())`；TLS 根用 webpki-roots（已在锁文件里），设备直连 DashScope 不经 host 代理（fake-ip 会挡）。

## 04｜踩坑

- 外部进程直改 `.metadata` `parent="trash"` 会被运行中 xochitl 覆写（阅读线判死）；软删/建夹只能走 QML 代理（书架 `shelf-trash-agent.qmd`）。
- 书架规则沿用：别跑 `cargo fmt --all`；qmd 先离线 `qmldiff apply-diffs`；重启 xochitl 只用 `xovi/start`。

## 05｜待办

步骤 0 真机 de-risk（用户在设备上：勾三段写三行 + `-`/`1.`/`口`/下划线；七样式笔记本；定稿 PDF 勾画）→ 样本进 `testdata/`，标定聚簇/配对阈值与页坐标几何 → transcribe-serve 真机验（部署 + 粘 key + 看草稿）→ note-serve 投影 → 代理扩展（mkdir）→ mind-serve → 导出 + `notes pull` → 文档/合并 `dev`。
