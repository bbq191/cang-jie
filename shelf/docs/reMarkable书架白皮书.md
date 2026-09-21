# reMarkable 书架（shelf）白皮书

> **读者与用途**：写给要维护、扩展或排查 shelf（书架：把书弄进设备、优化、再选去哪个读器）的人。
> 想 10 分钟看懂全项目，先读仓库根的 [`docs/OVERVIEW.md`](../../docs/OVERVIEW.md)；想 5 分钟看懂**书架这一块**，读下面的「5 分钟读懂」；想知道"现在长什么样"，读「现状总览」和各章开头的「现状结论」；
> 想知道"当初为什么这么定、真机怎么验、踩过什么坑"，读各章后面的决策小节（每节标题里的 §编号是稳定的引用锚点，别处文档写"见 §03bh"指的就是它，**编号不会变**）。
> 本文按**主题**（A 入库 / B 优化 / C 落库与大文件 / D 网关与网页 / E 稳定性内存耗电 / F 设备与固件）组织，章内保持原先的时间顺序；被后来决策取代的旧结论在原位标注"已被取代"，多数已压缩成"结论 + 关键教训"；每章开头有大白话导语和「本章坑位表」。
> **书籍优化引擎（`bookconv`）的深度细节**（清洗层 / 优化遍 / 脚注 / 图片 / 格式转换 / ★xochitl 渲染硬规则 / 版本演进）在 [`bookconv优化白皮书.md`](bookconv优化白皮书.md)；传书 EPUB 线"当前长什么样"的参考文档是 [`传书EPUB线架构.md`](传书EPUB线架构.md)。

## 5 分钟读懂

**一句话**：shelf 是跑在 reMarkable Paper Pro Move 上的一组网页服务。你用手机/电脑浏览器把书（EPUB/PDF）传进设备的**母版库**（书架里永久保存原书的暂存池），需要时点「优化」（让书在墨水屏上排版更好看），再选择「加入 xochitl」（官方阅读器）或「加入 KOReader」（第三方阅读器）。它**不修改 xochitl 本体**：只用 xochitl 自带的网页上传接口、直接写它的书库目录，以及注入少量 qmd 补丁。

![一本书的旅程](diagrams/book-journey.svg)

**三个互相独立的动作**（这是全书最重要的心智模型）：

| 动作 | 做什么 | 关键约束 |
|---|---|---|
| ① 入库 | 网页上传 / 抓网文 / scp 进 `inbox/`，**原样**落进母版库 | 只收 EPUB/PDF；网页没有"直投读器"的路径 |
| ② 优化（可选） | 清洗 + 排版 + 目录 + 封面保证；漫画自动识别；PDF 有文字层转 EPUB | 不强制；产物仍在母版库，可重优化 |
| ③ 落库 | 复制母版字节进读器：xochitl（分小文件/大文件/兜底拆分三条路）或 KOReader | 母版永久保留，可反复落库、两读器对照 |

**模块地图**：

| 模块 | 端口 / 位置 | 职责 | 详见 |
|---|---|---|---|
| `gateway` | `0.0.0.0:443`（`../gateway`） | HTTPS + 登录密码 + 单页 UI + 反向代理 + 批量队列 + 并发/内存闸门；**唯一对外** | 第 D 章、`gateway/docs/` |
| `book-serve` | 127.0.0.1:8790（`shelf/services/book-serve`） | 母版库、优化调度、落库、回收站/建文件夹/漫画页边距的设备端代理队列 | 第 A/C/E 章 |
| `bookconv`（crate） | 被 book-serve 链接，非独立进程 | 清洗 / 优化 / 图片 / 漫画 / PDF 入库 / 占位文档 | `bookconv优化白皮书.md` |
| `koreader-serve` | 127.0.0.1:8791 | 落书进 KOReader、字体/词典/配置同步、高亮/生词只读端点 | §03d、§03ar |
| `font-serve` / `wallpaper-serve` | 8792 / 8793（源码在 `../enhance/`） | 字体上传即装 / 壁纸上传即用（写原生 `SleepScreenPath` 键） | 第 F 章 |
| 笔记线四服务 | 8795–8798（`../notes`） | 与书架同网关，书架文档不讲 | `notes/README.md` |
| `shelf/xovi/*.qmd` | 注入 xochitl 的补丁 | 字体菜单动态项、回收站代理、建文件夹代理、漫画页边距代理 | 第 C/F 章 |

![shelf 架构：网关 + 领域服务](diagrams/architecture.svg)

**名词小词典**（首次出现处都会再解释一次）：

| 词 | 意思 |
|---|---|
| xochitl | reMarkable 官方的界面 + 阅读器进程；书架不改它，只和它"握手" |
| qmd / qmldiff | 对 xochitl 界面（QML）打补丁的文件 / 工具；xovi 是加载这类补丁与扩展的机制 |
| 母版库 | `~/.local/state/shelf/books/staging/`，原书永久保存处（"母版"=优化和落库都从它出发） |
| 边车（sidecar） | 每本书旁边的小 JSON，记"已落到哪个读器、渲染自检结果" |
| 占位 + 磁盘替换 | 绕开 xochitl 上传体积上限：先上传几 KB 的占位文档让它建条目，再把磁盘上文件替换成真书（§03bn） |
| SSE | 服务端事件推送；网页据此即时刷新，不用轮询（§03z） |
| OOM | 内存耗尽被系统杀进程；设备内存有限，是第 E 章的主线 |

**去哪儿找细节**：

| 我想知道… | 读这里 |
|---|---|
| 一本书从上传到能读，代码怎么走 | [`传书EPUB线架构.md`](传书EPUB线架构.md)（当前状态参考） |
| 优化到底改了书里什么、为什么 | `bookconv优化白皮书.md`；xochitl 渲染硬规则看它的 §09 |
| 某个 bug/决策当初的来龙去脉 | 本文对应 § 节（章首「现状结论」→ 节内「结论」→ 附录 §04 踩坑总表） |
| 网页 UI / 批量队列 / 并发闸门 | 第 D 章；网关自身见 `../../gateway/docs/reMarkable网关白皮书.md` |
| 设备升级固件后怎么恢复 | [`docs/INSTALL.md`](../../docs/INSTALL.md)「固件升级（OTA）之后」 |
| 已经砍掉的功能 | 附录 B（电脑端命令行）与「现状总览」的"已砍"一行 |
| 用户可见的更新历史 | [`docs/CHANGELOG.md`](../../docs/CHANGELOG.md) |

## 如何阅读本文

| 章 | 主题 | 包含的 § 节 |
|---|---|---|
| 0 | 定位、原则与基础 | §00 · §01 · §02 · §03 |
| A | 入库与母版库 | §03b · §03r · §03s · §03u · §03ao · §03ap · §03as · §03br |
| B | 优化管线 | §03e · §03i · §03q · §03y · §03aq · §03av · §03aw · §03ay · §03az · §03bb · §03bc · §03bg · §03bo |
| C | 落库、大文件与漫画 | §03d · §03l · §03t · §03aa · §03ad · §03ar · §03ax · §03be · §03bf · §03bk · §03bn |
| D | 网关与网页 UI | §03g · §03h · §03j · §03m · §03n · §03z · §03ac · §03ae · §03af · §03ah · §03aj · §03ak · §03al · §03am · §03an · §03au · §03bj · §03bl · §03bp |
| E | 稳定性、内存与耗电 | §03p · §03ab · §03ag · §03ai · §03ba · §03bh · §03bi · §03bm · §03bq |
| F | 设备、字体壁纸与固件 | §03c · §03f · §03k · §03o · §03v · §03w · §03x · §03at · §03bd |
| 附录 | 踩坑合集 · 旧版现状总览 · 真机待办 · 演进记录表 · 已移除的能力 | §04 · §00b · §05 · 附录 A · 附录 B · 附录 C · 附录 D |

## 现状总览（2026-09-20 刷新）

> 2026-09-22 复核：以下表格与当前源码逐项核对过（端口、`OPTIMIZE_VERSION`、体积门、闸门档位）；2026-09-21 新增的漫画页边距最小化（bookconv 白皮书 §20）已并入。

**shelf 是什么**：见上文「5 分钟读懂」。

**服务与端口**（均已与 `systemd` 单元核对）：对外只有网关 `gateway` `0.0.0.0:443`（HTTPS 私有 CA + 登录密码 + mDNS `shelf.local`）；领域服务只听本机：`book-serve` 8790（母版库）、`koreader-serve` 8791、`font-serve` 8792、`wallpaper-serve` 8793（后两者源码在 `enhance/`）；笔记线四服务 `ink` 8795 / `transcribe` 8796 / `mind` 8797 / `note` 8798 挂在同一网关上。

**读书线 = 三层 · 三个正交动作**：内容源 →（入库，原样）→ **母版库**（永久保留）→（可选「优化」）→（落库：加入 xochitl / 加入 KOReader）。入库不强制优化，优化后不强制落库，落库只是复制母版字节。全项目的传书全貌图见 [`docs/diagrams/transfer-flow.svg`](../../docs/diagrams/transfer-flow.svg)。

**当前有效的关键规则**

| 项 | 现状 | 详见 |
|---|---|---|
| 收什么格式 | 只收 EPUB / PDF（`rmsvc_core::formats`）；AZW3/MOBI/CBZ 等一律拒收，是策略收紧不是技术判断 | 第 A 章 |
| 书名 | 入库/优化时规范成 `书名 - 02卷`（数字在前），无卷标记的原样保留 | §03bn |
| 优化 | 只有「完整清洗 + 优化」一档（不再分档位）；`OPTIMIZE_VERSION`=15；漫画自动识别、保画质、优化不改格式（仍是 EPUB） | 第 B 章、§03bk（已被取代说明） |
| 漫画页边距 | 实验室开关 `comicMinMargin`（缺省关）：开启后漫画 EPUB 按 xochitl 图片框比例补白，并由阅读器内 qmd 代理把页边距设 1，左右留白约 20pt → 约 0；旧漫画需重新优化才生效 | bookconv 白皮书 §20 |
| PDF 优化 | 有文字层转 EPUB；无文字层/漫画只裁边保持 PDF | §03br |
| 加入 xochitl | ≤90MB 流式 `/upload`；>90MB 优先"占位+磁盘替换"（不分卷，上限 1GiB）；不可用才回退按卷拆分 | 第 C 章、§03bn |
| 批量与并发 | 批量队列在网关（顺序逐本、落盘续跑、可全部中止）；重活先过并发/内存闸门（>90MB 大档 1 个、小档 3 个） | §03bp、`gateway/docs/reMarkable网关白皮书.md` |
| 稳定性 | book-serve 用 `panic="unwind"`+`catch_unwind`，忙锁/取消合并为 `OpRegistry`，启动时修正被中断的 `pending` | §03bq |
| KOReader 入口 | appload ≥ 0.6.0（官方版已含 3.28 支持）；配置补丁经 koreader-serve `/config/*` 应用，没有一键命令行 | §03v、`../koreader/README.md` |

**已被砍/已被取代的（别再找）**：电脑端 `shelf` 命令行（2026-09-18，无网页等价物，见附录 B）；母版库"优化档位"与"投完自动删除"（2026-09-19）；漫画"优化转 PDF"（2026-09-19 做、2026-09-20 用户拍板换回 EPUB，§03bk）；三档格式（原生/电脑可转/仅 KOReader，2026-09-17/18 收成一档）；微信读书内容源（2026-09-05）；appload 补丁工具链（appload 0.6.0 官方已含 3.28，2026-09-21 删）；bind-mount 壁纸（改写原生 `SleepScreenPath` 键，§03x）。

**未闭环 / 未验证（如实）**：网页 UI i18n 与触屏交互只做过数据链路和无头浏览器验证，没有真实浏览器人眼确认；图片密集网文的大片留白是分页引擎行为，无 CSS 层杠杆（§03aq）；批量"加入 xochitl / 加入 KOReader"两条在设备上没有端到端实测（§03bp）；>153MB 文件走占位通道的首次渲染内存/耗时没验证；入库 PDF 转 EPUB 合并部署后功能本身未经真机验证（§03br）。

**OTA（固件升级）后怎么恢复**：权威说明（恢复流程、逐项对照表、流程图）在 [`docs/INSTALL.md`](../../docs/INSTALL.md)「固件升级（OTA）之后」；旧版 OTA 表见附录 D，§05 里的"OTA 后固定五步"是 2026-09-06 的旧说法。

## 第 0 章 定位、原则与基础

> **现状结论**
> - 四条硬原则至今有效：XDG 基目录规范；设计模式去重解耦；专项专用可插拔（按领域拆服务）；不引用旧项目 crate、不对接旧路径（能力只许剥离移植）。
> - 架构是"网关 + loopback 领域服务 + 运行时注册表"：装/卸一个服务 = 一个二进制 + 一个 systemd 单元；网关按注册表出 tab、按 URL 段转发。
> - 本章 §00/§01 里提到的"host CLI / 微读线 / 三种投递目标"是 2026-09-03 的设想，后续被取代，取代关系已在各自段落里标注。

### 00｜定位与原则

2026-09-03 用户提出"全面重构，补齐短板增强优势"：① 阅读系统增加 KOReader 与微信读书；② 全面支持 AZW3/PDF/EPUB；
③ 字体与屏保图片上传即可用。澄清：先「有书读」（原生 / KOReader / 微读三条投递线）再「高质量读」；上传时手选目标；
设备网页 + host CLI 共用同一 API；微读 = 设备上直接开网页版在线读（门控）。〔后续变化：微读线 2026-09-05 砍掉（§03u）；"上传时手选目标"被母版库三层架构取代（§03r），去向在母版库里选；host CLI 2026-09-18 砍掉（附录 B）；AZW3 等格式 2026-09-17/18 起不再收（第 A 章）；现状看「现状总览」。〕

四条硬原则（用户两轮驳回后定）：
1. **XDG 基目录规范**：路径表单一事实源是 Rust `rmsvc_core::paths`（原 `shelf_core::paths`，2026-09-11 起基座搬到顶层 `rmsvc-core`）；shell / qmd 处用同一张表的缺省展开值，env 可注入测试。
2. **设计模式去重解耦**（现役的，都在 `rmsvc-core` 与各服务里）：
   - Repository + Template Method：资产上传 `AssetStore`/`AssetUploadFlow`，font/wallpaper/koreader/母版库四家共用，拒收/成功文案由仓库定；
   - 领域模块 + 纯适配层：book-serve `staging/` 是领域，`api.rs` 只取参、回执；
   - 服务启动模板 `service::ServiceSpec` + `run`；配置读写模板 `config::load_or_default/seed/save`；
   - 端口/适配器：HTTP 只在 `http.rs`（`bind` / `Router::any` / `JsonBody` 取参门面）；
   - Registry（服务发现）· Facade（网关代理，`manage::MODULES` 单一目录表）· 单一事实源（`formats` 格式白名单、`fs::plain_name`/`unique_path`）；
   - 共享原语：`fs::write_atomic`、`multipart::receive_part_to`、`asset::receipt/all_ok`。
   - 原则：移动优先于复制；**旧 crate 只许剥离 + re-export，不直接引用**。
   - 〔已退役：Strategy（投递目标）/ Pipeline（处理链）随直投路于 §03s 删除——规则统一后没有调用方，模式要适配需求而非反之；Command（CLI）与 host 侧 `receipts`/`transport` 随 host CLI 一起砍除。〕
3. **专项专用可插拔**：不做单体，按领域拆服务。
4. **不引用旧项目 crate、不对接旧路径**：能力只许剥离移植；不读写 `/home/root/weread/**`（旧微读线的运行期目录）。

### 01｜架构决策

**三种拆法**：A 每服务独立对外端口（暴露面多、UI/CLI 要记端口）；**B 网关 + loopback 服务 + 注册表（采纳）**；C 单二进制 feature 编译期插拔（不能运行时拔）。
- 注册表放 `$XDG_RUNTIME_DIR`（重启即清）+ 读时按 `/proc/<pid>` 清陈旧条目：SIGTERM 杀进程 Drop 不跑也不会误报。
- 代理**剥掉服务段**（`/api/fonts/x` → 后端 `/x`）：后端直连与经网关同一套路由，`/health` 也能穿过网关。
- 每请求一线程：大文件上传不阻塞其它请求；**请求体方向流式透传**（`ureq::send(reader)`，上传大文件不额外占内存），**响应方向整体缓冲**（§03ah 更正了早期"body 流式透传"的说法）。
- 内存：musl 静态服务各约 0.56MB 二进制（网关 1.6MB 含 ureq/TLS）；空闲 RSS 数 MB，五个可接受（2026-09-03 数据，之后服务数量与体积有增长，未重测）。

**内容层抽离 `bookconv`**（移动，不复制；2026-09-03）：把原先夹在 weread-device 里的转换器、优化器、图片处理、EPUB 组装 `git mv` 成独立 crate `shelf/crates/bookconv`；唯一反常的反向依赖 `optimize → readlater::fetch_image` 切开成 `bookconv::netimg`（`http_agent` 在 netimg 独立实现，bookconv 不依赖 device-core）。当时的验收：旧 crate 全绿、新旧 `epub-optimize` 对同一合成 EPUB 输出 md5 完全一致。现状：weread-device 已不在仓库里（搬到仓库外的 `oldbak/`），bookconv 是 shelf 内部 workspace 的成员，只被 book-serve 链接。

### 02｜XDG 路径表

见 `shelf/README.md`「路径」。qmd 的 XHR 只能写绝对路径，写的是缺省值展开 `/home/root/.local/share/shelf/fonts.json`。

### 03｜systemd

`shelf.target`（WantedBy=multi-user）；服务 `PartOf=shelf.target` + `WantedBy=shelf.target`；网关 `ExecStartPre=-…/lo-alias.sh`（同一份脚本，让 10.11.99.1 常驻可达以便 `/upload` 注入；脚本源在 `enhance/lo-alias/`，2026-09-11 起去掉 `cangjie-` 前缀，往后新命名不再用这个前缀）与 `ExecStartPre=-/usr/bin/fc-cache`（tmpfs 索引真重启后丢）。`install.sh` 写 /usr 前实检 dm-verity。**不给 xochitl 加任何依赖**（改核心服务启动依赖曾导致变砖）。

## 第 A 章 入库与母版库

> **大白话导语**
> - 本章讲"一本书怎么进到设备上的书架"：书先**原样**放进一个叫**母版库**的暂存池（设备上永久保存原书的目录），之后想优化排版、想放进哪个阅读器，都是另外的独立动作。
> - 术语：**xochitl** = reMarkable 官方阅读器/UI 进程；**KOReader** = 设备上装的第二个阅读器；**book-serve** = 设备上管母版库的后台服务（本机 8790 端口，浏览器经网关访问）；**落库** = 把母版里的书复制进某个读器的书库；**inbox** = 设备上一个"scp 丢文件就自动入库"的目录。
> - 建议读法：先看"现状结论"和数据流图，再看"本章坑位表"；§03b、§03r、§03s 是**已被推翻/演进的设计过程**，只读其中的"结论 / 被取代 / 教训"即可；§03ao、§03as 讲已砍的电脑端命令行，§03u 讲已砍的微读线，都是历史记录；§03ap（抓网文同步优化）和 §03br（PDF 优化）是仍然有效的现役功能。

> **现状结论**（2026-09-22 已与代码核对）
> - 所有书**只落母版库**（设备上 `~/.local/state/shelf/books/staging/`，`rmsvc_core::paths::staging_dir`）：入库是入库、优化是优化、落库是落库，三个动作互相独立，网页没有"直投读器"的路径；母版永久保留，可反复落库、两读器对照。
> - 入库来源三条：网页上传（多文件、进度、上传前核对"同名同大小=已落地"则跳过）、抓网文（Readability 抽正文组成 EPUB，网页「同步优化」复选框缺省勾选）、scp 进设备 `inbox/` 的追平队列（fswatch 监听，8 秒防抖）。
> - 格式只收 **EPUB / PDF**（`rmsvc_core::formats::BOOK_EXTS` = `NATIVE_EXTS`）；其它格式（AZW3/MOBI/CBZ 等）一律拒收——这是 2026-09-17/18 用户明确的**策略收紧，不是技术判断**（KOReader 本来能读 CBZ/DjVu/HTML 等）；已在母版库里的旧条目不受影响，`koreader-serve` 的 `POST /books/adopt` 不设格式门，仍可加入 KOReader。
> - 入库时 EPUB 书名按规则整理成 `书名 - 02卷`（数字在前，`bookconv::naming`，§03bn）；同名不覆盖、撞名各自入库（`unique_path` 加数字前缀），回执区分"按规范命名存为"与"已有同名，存为"。
> - 优化与落库都是**异步**：HTTP 立即回"已开始"，进度靠边车（sidecar，书旁的隐藏 JSON 记录）+ SSE 事件（服务端推送）呈现；优化只有一档（清洗+优化，2026-09-19 起不再分档位）；落库永远保留母版（不再有"投完自动删除"）。
> - PDF 也能点「优化」：有文字层转 EPUB（转成后原 PDF 删除），无文字层/漫画只裁边（§03br）。
> - 电脑端 `shelf` 命令行（含 Calibre 深洗、`doctor --render`、`notes pull`）已于 2026-09-18 整体砍除（附录 B），现在没有网页等价物；§03ao / §03as 是它们的历史记录。

![shelf 读书线数据流：三层·三动作正交](diagrams/data-flow.svg)

#### 本章坑位表

| 坑 | 症状/判据 | 根因 | 教训/规避 | 见§ |
|---|---|---|---|---|
| "已优化"徽章说谎 | 网文/转换产物显示"已优化"、「优化」按钮被隐藏，其实没洗缩进/脚注 | 无 wash 的产物写了和完整优化同一个标记 | 标记分层：完整写版本号、核心遍写 `<版本>-core`；`level`=full/core/old/none | §03r |
| inbox 追平绕过母版库 | scp 进 `inbox/` 的书自动优化并直投 xochitl | `process_inbox` 是直投路遗留 | 统一规则要审到**所有入口**，不只网页 | §03r |
| 验证 inbox"没触发" | scp 5 秒后去查，以为 watcher 坏了 | watcher 是 8 秒防抖 | 验证前先读防抖参数 | §03r |
| `df -k` 解析出错 | 母版库剩余空间读不到 | busybox `df` 设备名过长会把数字换到下一行 | 拍平表头后所有行再取第 4 个 token（`Staging::free_bytes`） | §03r |
| PDF 重排后整页空白（host 旧管线） | 《财新周刊》重排后一页占 1.3 屏、留 70% 空白 | 每页一章 + 段落碎成行 + 图近整屏高 + 双栏页图块被静默丢 | 先解剖产物再改；管线已砍，只留教训 | §03r |
| 网文正文大片留白 | 抓来的文章段距巨大 | 白名单不留 class/style，xochitl 只认外链 css；"同步优化"只治这一层，图片夹正文的留白另有成因 | 复选框是必要不充分修复 | §03ap、§03aq |
| 同名同大小去重快照过期 | 一次拖两份同名同大小文件，第二份仍成 `1_x` | 上传前只查一次母版库快照 | 每成功一项要补进快照 | 附录 §04 |
| 撞名文案误报 | 规范命名后的文件被提示"已有同名" | 落地名≠请求名有两种原因 | 用 `canonical_staged_name(requested)` 比对再分文案（`staging/mod.rs`） | 现状结论 |
| 上传超 100MB | 大书 `Connection reset` / `413` | xochitl `/upload` 硬上限 100,000,000 字节；缺省体积门 `nativeUploadLimitMb`=90 | 占位+替换通道 | §03bn |

### 03b｜Phase 1 统一投递（2026-09-03，离线完成）

> **⚠ 已被取代**：本节的"上传时手选目标 + 直投读器"在 2026-09-05 被母版库三层架构取代（§03r）；直投路（`POST /?target=`、`Native`/`Annot` Strategy、Pipeline）已于 §03s 删除，现行唯一入口是 `/staging*`。

**当时做了什么**：book-serve 多文件流式落 `.work`→按目标 Strategy 处理→done/failed 归档；koreader-serve 直传 `POST /books`（先 `.part` 再 rename，KOReader 扫目录看不到半成品）；网关传书 tab 目标下拉三档；host `shelf push` 的 `decide_route` 纯函数 + Calibre 桥。冒烟（三服务干净 env）：注册/代理、KOReader 中文名+子目录落盘字节正确、xochitl 不可达时失败入 failed 可重试/删除均通；真机验证当时设备离线，未验（见 §05）。

**仍然有效的部分**（已核对代码）：
- 自有 spool `$XDG_STATE_HOME/shelf/books/`，现只剩 `inbox/`（scp 追平）、`.work/`（认领中/上传暂存，崩溃残留启动时移回）、`failed/`（封顶 50MB，带 `.reason`）；`done/` 与 LRU 已删。配置 `~/.config/shelf/book.json`（`paths.service_config("book")`，首启写缺省），现只有 `xochitlHost` / `uploadTimeoutSecs`（300）/ `nativeUploadLimitMb`（90）三键；xochitl 客户端 `timeout_connect` 10 秒（整体 300 秒只防大书误判）。
- koreader-serve（8791）现只从母版库 `POST /books/adopt` 落库；`GET /status`（installed / running〔扫 /proc cmdline〕/ version / 计数）、`/books`、`/fonts`、`/dicts` 仍在；此后新增只读 `GET /annotations`、`/vocabulary`（§03ar）与 `/config/{settings|defaults|gestures}[?dry_run=1]`。
- host CLI 整段已砍（附录 B）。

**教训**：为"上传→转换→优化→检查→注入"直投设计的 Strategy/Pipeline，在规则统一成"只落母版库"后没有调用方——架构模式服务于当时的规则，规则变了就整块删，别留着当"以后可能用"。

### 03r｜读书线重构：中间层（母版库）三层架构（2026-09-05，用户"逻辑很乱"提出改造）

> **状态**：三层架构与"三动作正交"是现行主线；Phase C 里的"母版库收任意格式""优化档位""投完清除""电脑 `shelf push`""微读占位"均已被后续推翻（见下"已被后续取代"）。

**问题**：读书线让用户一次背 3 维决策（读器×投递口×文件夹），"优化"与"落库"耦合；根因是把机器判不了的"用途分类"（侦探小说/漫画是闲书还是研读）塞进流程。**方案**：三动作正交——入库 / 优化 / 落库，中间夹**母版库**作交汇点；统一规则：**所有源一律原样直传母版库，"优化"是母版库独立动作，"落库"也独立**。附带决策：母版库可反复落库（一本落两读器对照）；指引只做决策辅助（不替用户分类，现为网页「三步走」）；微读只作内容源（后被砍，§03u）。

#### Phase A 母版库基础设施（✅ 真机通 2026-09-05）
- `paths::staging_dir()`（`state_dir()/books/staging`，/home 分区不丢、**不套 LRU 淘汰**）；koreader-serve `POST /books/adopt` 从共享 staging 读→`KoStore::install` 纯复制。
- **关键设计：落库 = 纯复制原字节、不优化**，保证一本母版落两读器是**同字节可对照**。
- 真机端到端：上传→母版库→优化(1400→1778 字节，标记 false→true)→投 xochitl（`visibleName` 坐实）→adopt KOReader（1778 同字节）→母版保留、两库并存。

#### Phase B/C 内容源汇入与 UI 三层重构（✅ 真机通 2026-09-05）
- **网文获取下沉**：抓取 + Readability + scraper 白名单核心从 `reading/device-rs` 下沉进 `bookconv::article`；`POST /api/books/staging/fetch-article {url}`。真机：scraper/html5ever **aarch64-musl 交叉编译通过**；设备联网抓 runoob / 阮一峰两篇成功入库。能力边界：静态 HTML 文章好；SPA / 付费墙 / 反爬抽不出；图片 best-effort；恒产单章。
- **UI 定稿**（首版把母版库列表复用进三个 tab、读器页仍留传书区，用户指出"书从哪进有三个答案"）：「传书」固定 tab 放第一位 = 唯一总入口，二级 tab「入库」｜「母版库」；按格式/安装状态门控按钮；读器页彻底不传书（xochitl 页只剩原生字体，KOReader 页只剩字体+词典）；**网页直传取消、电脑 `shelf push --direct` 也去掉**（用户："规则一致，所有书只允许落母版库"）。

**已被后续取代**：
- "母版库改收任意格式" → §03s 曾分三档，09-17/18 收成**只收 EPUB/PDF**（`formats.rs`，测试钉死 `BOOK_EXTS == NATIVE_EXTS`）。
- 「优化档位」「投完清除」→ 2026-09-19 已砍（`staging/optimizing.rs` 文档注释）；「微信读书占位」→ §03u；「电脑 shelf push」卡片 → 09-18 砍。
- 「清理已落库」批量按钮 → 由底部操作栏 + 服务端批量队列取代（§03bp）；⚠ "快满了"提示文案（`transfer.staging.lowWarn`）里还残留"清理已落库"字样。

#### 规则漏洞两处（用户问"逻辑是否清晰"时自查出）
- **inbox 绕过**：scp 进 `inbox/` 的书被 `process_inbox` 自动优化直投 native，违反统一规则 → 改为原样落母版库（现只收 EPUB/PDF，非书籍进 `failed/` 带原因），老 `POST /?target=` 标废。
- **"已优化"徽章说谎**：网文/格式转换产物走 `assemble_optimized`（无 wash）却写同一标记，母版库显示已优化并隐藏「优化」按钮 → `marker_value(full)`：含 wash 写版本号，无 wash 写 `<版本>-core`；`StagingEntry.level` = `full / core / old / none`（`staging/library.rs::probe_level`），UI 徽章「已优化 / 已优化·未清洗 / 旧版优化 / 未优化」，非 full 仍给「优化」按钮。
- 真机通（2026-09-05）：scp 进 inbox → 8 秒防抖后日志"已入母版库"、xochitl 无该书；抓网文 → `level:core`、点优化 → `full`。⚠ 验证时踩了自己的坑：watcher 是 8 秒防抖，5 秒就去查以为没触发。

#### 第二步"能用→好用"小迭代（✅ 真机通 2026-09-05）
- **落库记录** sidecar `.<书>.delivered`：deliver 自动记 native，KOReader adopt 由前端调 `POST /staging/mark` 记（各服务只写自己目录）；徽章"已投原生 / 已加入KO"，落库时间早于母版 mtime 标"旧"；删书连带删 sidecar（此后 sidecar 还扩了 `render` / `optimize` / `deliver` / `source` 字段，`sidecar.rs`）。
- `GET /staging` 带 **freeBytes**（`Staging::free_bytes` 解析 `df -k`，坑见上表），前端 <300MB 红字告警（`app.js`）；同名重复入库回执"已有同名，存为 1_x"。

#### Phase E PDF 结构化重排（host 旧管线，已随 host 砍除；教训保留）
真机《财新周刊》"整页大片空白"，解剖产物（不猜）定位三根因：**每页一章**（91 页→91 xhtml，读器每章末强制翻页）、**段落碎成行**（PyMuPDF 对杂志每行一个块）、**图近整屏高**推到下页留白。当时修法：整本平铺→文档级段落合并→按标题分章→图封顶宽≤954/高≤60% 屏；量化章 91→10、段落 1209→770、句中断开 48%→12%、中位段长 47→109 字，用户核 v3"空白没了，段落正常"。
- **二次坑**（dump 单页逐行数据才看出）：① 分栏按"块中点在中线左/右"，单栏页宽块被甩到"右栏"排最后 → 改为仅在存在贯穿页面竖向空白带时分栏；② 行尾零宽空格挡住句末标点判断 → 全局剥零宽字符；③ 旧分栏在双栏页**静默丢图块**（8 张→39 张才是全量），且 954 宽照片存 PNG 一张 1.3MB → 21MB，改 JPEG q80 → 3.6MB。
- 现役的 PDF 优化是完全不同的新实现，见 §03br。

### 03s｜代码质量核查二轮：母版库领域化 · 直投路删除 · 上传模板统一（2026-09-05，用户"审视 shelf/ 合理用设计模式、去重、解耦"）

> 本节是 2026-09-05 的重构记录；其后命名又变：`shelf-core` 现为顶层 `rmsvc-core`（`rmsvc_core::…`），`shelf-gateway` 现为顶层 `gateway`，`font-serve`/`wallpaper-serve` 已搬到 `enhance/`，host CLI 已砍。下文保留当时模块名，括注现名；所列项目均已对照当前代码确认仍在。

**动机**：§03r 落地后代码里留着两套并行的"收书"路（旧 `POST /?target=` Strategy+Pipeline 直投 vs 新 `/staging*`），三份手搓 multipart 循环，格式白名单在网页 JS / book-serve / font-serve / koreader-serve 四处各写一份，服务 `main` 里 `let (k1..k10) = (k.clone()…)` 克隆串，"缺 name"取参样板 ×7，单段文件名校验 ×4。全部按"单一事实源 + 模板方法 + 门面"收。

| 层 | 收编内容 |
|---|---|
| `rmsvc-core`（原 shelf-core） | `formats`（`BOOK_EXTS/FONT_EXTS/DICT_EXTS/IMAGE_EXTS` + `has_ext/ext_of/dotted`，网页 accept、上传门、inbox 追平同源）；`fs::plain_name`（单段文件名，拒 `/`、`..`、`.` 开头）+ `unique_path` / `move_unique`；`http::bind` / `Router::any` / `Request::json()→JsonBody{str/str_or/bool_or}` / `multipart_boundary()` / `q_flag()`；`asset::AssetUploadFlow::in_dir`（暂存在 `.work/` 与母版库同分区，入库 rename 零拷贝）、`reject_message` / `success_message` 缺省方法、`receipt()` 统一回执 |
| book-serve | 删 `target.rs` / `pipeline.rs`；母版库从 `spool.rs` 拆成独立领域模块 `staging`（现为 `staging/{mod,intake,library,optimizing,deliver}.rs`）；`api.rs` 回到"取参 + 调领域方法 + 回执"的适配层；`spool.rs` 只剩 inbox 队列；`BookConfig` 退役 `optimizeDirectEpub` / `comicMono`（旧 json 多余键被忽略，有测试）；`/targets` 删 |
| koreader-serve | 删直传 `POST /books` 与 `optimizeEpub`；书只从母版库 adopt；不再依赖 bookconv；`ConfigSync` 改持 `Arc<KoReader>` |
| font-serve / wallpaper-serve（现 `enhance/`） | `State{store, paths}` + `bind`；font-serve 注册 tab 改名「xochitl」（order 10），book-serve 不再挂 tab——网页 xochitl 字体页由 font-serve 存活与否驱动（原先键在 book-serve 上：它关了字体页没了，font-serve 关了页面还在但全 404） |
| 网关 | `ui::page()` 用 `OnceLock` 把 `formats` 白名单注入模板 `__EXTS__`；`upHtml()` + `uploader()` 认 `.up` 容器；`fillList` / `delBtn` / `cjkBadge` 与 `assetTab` 模板收编列表渲染；`proxy::forward` 自己查目录表 |

**副作用**：设备端 AZW3/MOBI/FB2/CBZ 转换不再从 book-serve 可达（bookconv 转换器本体保留给 reading 线）。**没动的**：`htmlproc` / `wash` 规则本体、`koreader/merge.lua`、systemd 单元、install/uninstall。

**验证**：离线 `cargo test --workspace` + host pytest 全绿、`node --check`、workspace 零警告（当时数字 bookconv 104 / shelf-core 37 / book-serve 10 等，此后早已增长，仅作存档）。**真机通（2026-09-05 13:41，USB 10.11.99.1，备份 `shelf-20260905-134052`）**：font-serve 挂 tab「xochitl」、book-serve 无 tab；一次传 jpg+epub → jpg 拒收（文案含整份白名单）、epub 落地回执 `name` = 落地名；优化回执带统计、等级 none→full；非 EPUB/PDF 落库被门拦；`/books/adopt` 进 KOReader 子目录同字节；旧 `POST /?target=` 404、`POST /koreader/books` 405；`.work/` 空。测试书已删。

**格式提示分档（2026-09-05 中间态，已被 09-17/18 收紧取代）**：当时核实 18 个扩展名全是设备 KOReader v2026.07.1 `documentregistry` 真注册的（crengine：txt/html/rtf/doc/docx/chm/mobi/azw/prc/fb2；mupdf：pdf/cbz/cbr/xps，`libarchive.so.13` 带 rar 所以 CBR 真能开；djvu 引擎：djvu），问题在**展示**——一口气列 18 个看不出谁能去哪，故分 `NATIVE_EXTS` / `HOST_CONVERTIBLE_EXTS` / `KOREADER_ONLY_EXTS` 三档。后两档现都已删，见 `formats.rs` 文档注释与测试 `retired_*_no_longer_accepted`。**教训**：格式收紧是用户策略，"KOReader 技术上能读"不构成收的理由。

### 03u｜砍微读线（2026-09-05，用户"不做了，直接砍微读线"；范围只限书架）

> **⚠ 历史记录 / 已砍**：书架内没有微读线；`weread-serve` 槽位、入库页占位、目录表 weread 行都已删。

**评估后砍**，依据 Phase D 探索（两个只读 agent + 真机核）：
1. 「微读 Skill」= 微信读书官方 Skills/Agent 网关（Bearer api_key，`/_list` 自描述 17 接口）**只读没正文**：书架/目录/搜书能拿，章节正文只能走 web 端点 + cookie `wr_skey`（约 90 分钟过期，靠 renewal 续）+ 分片 `codec`（经授权照 MiuRead 移植的 AGPL 派生，个人自用不分发）。
2. 下书栈在 `reading/device-rs`（≈1000 行）；接入书架要 `git mv` 成 `shelf/crates/weread-core`（AGPL-3.0）+ 旧 crate re-export，再立 `weread-serve` 8794（后台任务 + 进度轮询 + 55 分钟续期 + 封面代理）+ 网页扫码状态机。
3. 设备与 host 都没有微读凭证（重置机）；协议脆弱是已知常量（上游一改即断，"200 空 {}"曾卡数日）。投入约两天、收益是一条依赖第三方私有协议的内容源，用户决定不做。

**砍掉的**：管理页目录表 weread 行（`installable` 机制保留，`gateway/src/manage.rs` 现仍有此字段）、入库页占位与三处"待接"文案、`services/weread-serve/` 槽位 README、`weread-web/`（从未执行的 spike）。**不动的**：`reading/` 的 wr-* 下书栈（块③旧线，与书架无关）；`bookconv` 里为微读书做的脚注/远程图规则（第三方书同样受益）。书架内容源现为三条：网页上传 / 抓网文 / scp inbox。

**追记（2026-09-09，若将来想把"墨香管理"接进书架网页，先看这条）**：`reading/device-rs` 的 `wr-serve`（墨香面板 API，`127.0.0.1:8777`）内嵌一条上传页线程，**硬编码监听 `0.0.0.0:8778`**（`upload_server.rs`），注释说"以书架为准，`CANGJIE_UPLOAD_PAGE=0` 让出端口"，但没有任何 systemd/安装脚本真设过该变量。网关 2026-09-10 改绑 443（§03am，现行 `0.0.0.0:443`）后字面端口冲突消失，**只是巧合躲开**：两者从未一起跑过（wr-serve 未接入服务注册表，不受 `shelf.target` 管），"传文件到 inbox"与 book-serve 入库功能重复也依旧。真要接入，第一步仍是砍掉那条内嵌上传页。⚠ `reading/` 已整体搬出仓库（2026-09-11），本次审计无法复核 `upload_server.rs` 现状，按原记录保留，**未复核**。

### 03ao｜`shelf push --no-calibre`：纯 EPUB 优化、跳过 Calibre（2026-09-10）

> **⚠ 已被取代**：`shelf push`（含 `--no-calibre`）随电脑端命令行在 2026-09-18 整体砍除（附录 B），现在没有网页等价物。本节保留作历史与决策依据。

用户问"是不是缺一个单纯 EPUB 优化、不转格式的逻辑"。**先核实现状再动手**：核心能力本来就有——`bookconv::optimize::optimize_epub_with` 是唯一的纯 EPUB→EPUB 优化实现，已被网页「母版库→优化」按钮（`Staging::optimize`）和独立 CLI `epub-optimize`（`bookconv/src/bin/epub_optimize.rs`，仍在）调用；真正缺的是 host `shelf push` 没暴露"跳过 Calibre、只跑 epub-optimize"的入口（EPUB 输入只有"Calibre 深洗+叠加优化"捆死的一条路，没装 Calibre 时连自家优化都用不上）。用户要求新开关要含清洗层（对应网页「清洗＋优化」默认档，不是 `--no-wash` 那档）。

**当时实现（已随 host 砍除）**：`calibre_bridge.epub_optimize_bin()`（`WASH_OPTIMIZE_BIN` 环境变量 → PATH → 仓库内 `target/release/epub-optimize`）+ `optimize_only()`；`push.plan()` 第三条路 `optimize-only`；非 EPUB 退化 `raw`（不报错、不偷用 Calibre）；`--no-calibre` 判在漫画分支之前短路。验证：8 个新增单测 + 非 mock 真调用（真编二进制跑真实缓存 EPUB），纯 host 改动未上真机。

**教训**：用户说"缺一个功能"时，先查是"能力有、入口缺"还是真缺；新开关要先确认用户要的是哪一档语义，别拿相近的现成档凑。

### 03ap｜「抓网文」补「同步优化」复选框（2026-09-10，真机通）

**需求**：用户反馈"抓网文需要增加一个复选框（同步优化）……否则默认给设备渲染页面会有大量留空，但要给用户选择权"。

**根因（症状→根因）**：
- 网文正文在 xochitl 上按默认段距渲染，页面大片留空；
- `bookconv::article` 抽正文时属性白名单不留 `class` / `style`，产出 XHTML 没有任何排版样式；抓取落库只走核心遍（`level:"core"`），不做边距/段距归零、不注入外链缩进 css；
- xochitl 只认外链 `.css` 里的裸元素规则、无视内联 style（§03y 七条实测规则之一）；
- 母版库「优化」按钮其实能手动补一步——本次只是把"手动补一步"变成"抓取时可选自动做"。

**实现（现行代码，已核对）**：
- `Staging::fetch_article(url, optimize)`（`staging/intake.rs`）落库后若为 true，紧接着调**同一个** `self.optimize(&name, …)`（即列表「优化」按钮那个函数；当时写作 `OptimizeMode::Auto`，该枚举已随档位删除，现传空进度闭包）；返回 `FetchArticleOutcome{name, title, optimized, optimize_error}`。
- 同步优化失败**不阻断**抓取（`Result` 仍 `Ok`，`optimize_error` 带原因）；`api.rs` 按状态组"…并同步优化" / "…（同步优化失败：…，可在列表里手动点「优化」）" / 原文案。API 缺省 `optimize=false`，向后兼容旧客户端。
- 网页「抓网文」卡片加 `#artopt` 复选框，**前端缺省勾选**（`LS.get('artopt','1')`，本机 `localStorage` 记忆）；"给选择权"体现在"能关"而非"缺省关"。

**验证**：离线单测 + `node --check` + 网关测试（新增 2 个 `transfer.fetchArticle.*` i18n key，zh-CN/en-US 集合一致；`fetch_article` 依赖真实网络，项目一贯不进 CI）。host 侧非 mock：对 `runoob.com` 一篇教程真调用，`wash:None` 产物不含外链 `cangjie-wash.css`、`wash:Some` 含；`fetch_article(url,true)` 落库 `level=="full"`、`false` 落库 `"core"`。**真机（2026-09-10）**：`curl` 分别带 `optimize:true` / 不带字段各抓一次同一篇——前者 `level:"full"`、3761 字节、回执"…并同步优化"，后者 `level:"core"`、3488 字节；体积与 host 侧完全一致；首页 HTML 确认 `#artopt` 与新 i18n key 已下发；测试文章已删。

**追记（2026-09-10，§03aq）**：本节"默认解决『大量留空』"**过于乐观**——同日另一篇图片密集的网文复现，真机 A/B 逐页渲染证实：`optimize:true` 本身确实生效，但对"图片夹在正文中间导致的大片留白"**没有实质改善**，那类留白的真成因是分页引擎对图片块的处理（详见 §03aq）。本节修的是真实缺口（网文正文样式缺失），不是留白问题的完整解法。

### 03as｜`shelf notes pull` 补 `config.toml` 的 `notes_vault` 配置项（2026-09-16）

> **⚠ 已被取代**：`shelf notes pull` 随电脑端命令行在 2026-09-18 整体砍除（附录 B）。本节保留作历史记录。

真机验证 `shelf notes pull`（笔记线白皮书 §03ak / §03al）时用户指出：本机 Obsidian vault 落哪个目录不该只靠每次手敲 `--out`——vault 位置只有用户自己知道，CLI 不该替用户猜。当时做法：`config.toml` 加 `notes_vault` 字段（缺省空串），落地目录三级优先级 `--out` > `notes_vault` > `$XDG_DATA_HOME/shelf/notes-vault` 兜底；新增 1 个测试，host pytest 94 全绿；纯 host 改动未上真机。**保留下来的原则**：路径类配置项优先级 = 命令行 > 用户配置 > 兜底缺省，且兜底不该被当成"多数人真实位置"。笔记线导出 md 现在的入口见 `notes/README.md`。

### 03br｜入库 PDF 的「优化」：有文字层转 EPUB / 无文字层只裁边（2026-09-19）

> 主线摘要；用户的两处取舍与实现细节见 `bookconv优化白皮书.md` §18。

- 母版库「优化」按钮扩到 PDF（`bookconv::pdf_ingest`，现为 `pdf_ingest/{classify,headings,text,to_epub,trim,source}.rs` 子模块）：`classify_pdf` 把 PDF 分成 `Comic` / `NoTextLayer` / `TextLayer` 三类——
  - **有文字层**（`TextLayer`，每页平均可提取字符 ≥ `MIN_CHARS_PER_PAGE`=40）→ 转 EPUB：保留图片、保留公式（按行/区域裁剪成图，用纯 Rust 的 `hayro` 光栅化）、必须有 TOC（书签目录优先，没有就按字号识别标题，都识别不出才按固定页数分块）。转成功后**原 PDF 被删除**、以 `<书名>.epub` 落地（`Staging::optimize_pdf`）。
  - **漫画 / 无文字层的扫描件**（`Comic`：≥ 90% 的页被一张覆盖 ≥ 85% 页面面积的图占满；`NoTextLayer`）→ **只裁边**，格式不变（`optimize_pdf_trim_only`，有安全闸——只处理"零文字、每页恰好一张整页图"的 PDF，其它形状拒绝并保持原文件不动；保留书签，没有书签则按页分段兜底出目录）。分类判不准（打不开/解析失败）一律退到 `NoTextLayer`——转 EPUB 是破坏性格式变更，判不准时选更保守那条。
- PDF 转出的 EPUB 在列表里有来源徽章（`StagingEntry.pdf_source`，识别方式：OPF 里 `dc:identifier` 带 `pdf:` 前缀）；这类产物视为一次性完成，不再进 full/core/old/none 阶梯、也不再显示「优化」按钮。
- 验证状态：这一轮写代码时"不上真机"，合并后部署到设备，但功能本身**没有经过真机端到端验证**（部署健康不等于功能验证）。

## 第 B 章 优化管线

> **大白话导语**：这一章讲"一本 EPUB 放进母版库后，点「优化」到底改了什么、为什么这么改"。「优化」是设备端 `book-serve` 调用 `bookconv` 库完成的（bookconv=专门处理电子书内容的 Rust 库；母版库=书架里永久保存原书的暂存池）。之所以值得读，是因为 **xochitl（reMarkable 官方阅读器/UI 进程）的 EPUB 渲染器闭源、行为和常识不一致**，这章记录的多是"真机量出来的规矩"和踩过的坑。
> 阅读顺序：先看下面的现状结论和坑位表；要动排版规则读 §03y（xochitl CSS 规矩，含速查图）；想知道当时为什么做出某个决策再看 §03q / §03av；§03e、§03i 主要是历史。引擎的逐模块细节看 [`bookconv优化白皮书.md`](bookconv优化白皮书.md)。

> **现状结论**
> - 「优化」只有一档：完整清洗（wash）+ 优化器两遍 + 图片处理；`OPTIMIZE_VERSION`=15（v15=EPUB 漫画补白比例改 303:462.1，配合阅读器页边距 0）。母版库按产物内埋的版本标记显示 full（当前版本）/ core（只跑核心遍）/ old（旧版本）/ none，非当前版本的书可再点「优化」升级；旧漫画需**从原始文件**重新优化（对已优化产物再优化会多一代 JPEG 有损）。工程细节看 [`bookconv优化白皮书.md`](bookconv优化白皮书.md)。
> - **内存**：流式两阶段（先规划、再逐图处理），图片并行（2 个 worker，同时在处理的像素 ≤600 万），单图解码超过 900 万像素直接跳过（§03bi）。
> - **排版**：xochitl 的 CSS 引擎只认外链 `.css`，不认内联 `<style>`/`style=`/`!important`（类选择器认，但有"先出现者胜"等陷阱），规则速查见 §03y；脚注缺省 `Anchor`（注释移到章末、原生返回浮标兜底）；缺目录时自动建目录（纯图片书按每 20 页分段）。
> - **封面**：OPF 必须同时有 `<meta name="cover">` 和 `properties="cover-image"`（§03bo）。
> - **漫画**：自动识别，保画质、裁边（单边最多裁 35%）、单趟缩放；优化不改格式，漫画仍是 EPUB（2026-09-20 起，取代了 §03bk 的"改产 PDF"）。
> - 清洗层会删除指向书内不存在文件的 `<img>` 与死 `@font-face` 来源（§03bm 后续）。
> - **已砍**：电脑端 host CLI（`shelf push`、Calibre 洗书链、`check_output.py` 质量门）已于 2026-09-18 整体砍除，设备端 `book-serve` 是唯一线上路径；开发机上只剩 `epub-optimize` 小工具（同一份代码）。

#### 本章坑位表（一）

| 坑 | 症状/判据 | 根因 | 教训/规避 | 见§ |
|---|---|---|---|---|
| 内联排版规则不生效 | 首行缩进/边距归零"注入了却没变化" | xochitl 只认外链 `.css`，无视内联 `<style>` 与 `style=` | 排版规则一律写外链 `cangjie-wash.css`；一个真机活样本（《飘》）胜过七版凭空诊断 | 03q · 03y |
| CSS 声明缺尾分号被吞 | `p{text-indent:2em}` 不缩进，加 `;` 就好 | 规则里最后一个无分号的声明被丢 | 清洗层所有输出声明一律尾分号 | 03y |
| `text-indent:0` 无效/继承 | 顶格段仍带外层 div 的缩进 | `0` 被当"没设"，落回继承 | 顶格写 `0.01em` + `<div class="cj-flush">` 并剥掉书的类 | 03y |
| 旧版本书永远升不了级 | v7 之后的改进"反复测都看不到" | 幂等门只看标记存在、不看版本 | 版本号必须与当前比对；改动要告诉用户旧书需重传/重优化 | 03q |
| 内联图标脚注 marker 巨大 | 一页 7 个脚注 = 同一图标 7 次 | xochitl 行内图按固有像素尺寸渲染，不认 CSS | Inline 模式一律丢弃原 marker，只留纯文本 | 03q |
| 背景图平铺盖正文 | 分卷页整页风景图盖住文字 | xochitl 无视 `no-repeat`/`background-size` | 清洗层剥 `background`/`background-image`（章头 `<img>` 装饰保留） | 03q |
| 误诊：章头 banner 当"重复图"删 | 用户拍照判"对"的装饰图被删 | 真凶是 CSS 背景图不是 `<img>` | 看不到屏幕别猜渲染，让用户拍照；有活样本先解剖；已 `git revert` | 03q |
| 中文书零缩进 | 全书没有 `<p>`：每章一个 div、`<br/>` 分行、段首 U+3000 | xochitl 折叠 U+3000，`p{}` 无对象 | `cjk_paragraphize`：br 切成 `<p>`，剥段首全角空格，缩进统一走 css | 03y |
| 脚注想固定在"当前页最下面" | 段末块注释被用户否决 | EPUB 是流式重排，"第几页"是翻页时才算出的运行时结果，做书时不知道 | 页底定位是固定版式（PDF 线）的能力；`ParagraphEnd` 整个删除，回 `Anchor` | 03av |
| 修了 figure 边距，留白没变 | 网文图多，页尾大片空白 | 图片块在页尾放不下就整体推下一页，页尾不回填（渲染引擎分页决策） | 改前后逐页像素级一致才是证据；没有 CSS 层杠杆，别重排查 | 03aq |
| 裁边在设备上极慢 / 裁不净 | 25 页 2200×3400 漫画优化 2 分 19 秒；版权页留白 22–29% 裁不干净 | 逐行列扫描在弱 ARM 上被放大；旧上限 15% 太保守 | 后由图片并行 + 单趟管线提速；上限调到 35%（bookconv §19） | 03av |
| 放开 `background-color` 的风险 | （未验证）深底浅字块在彩色墨水屏上或更难读 | EPUB 线原则②不再剥颜色 | 需挑一本带彩色底纹的技术书真机验证 | 03av |
| 弹窗脚注做不成 | 竞品能弹，我们不能 | 闭源渲染器不实现弹窗，正文点击不通知可注入 QML 层 | 只能内联常显或"跳转+浮标"；KOReader 才有弹窗 | 03q |

（本章后半部分的坑见「本章坑位表（二）」；附录 §04 已收录的坑不重复。）

### 03e｜Phase 4 原生高质量门 · Phase 5 微读 spike 脚本（2026-09-03）

> **历史记录**：本节的 host 质量门与 spike 脚本都已不在仓库。现存的只有 `bookconv::check`（见 §03i），且设备端「优化」不跑门，仅 `epub-optimize --check` 可用。

- **P4（质量门）**：当时 host 产物必过 `check_output.py`（PDF：宽高比/outline/内链/字体子集/屏上字号列宽；EPUB：DRM 硬拦/nav+ncx 命中率/双 id/锚点），不过不推。整套随 host CLI 于 2026-09-18 砍除。
- **P5（微读 spike）**：`shelf/weread-web/spike.sh`（recon 侦查 → fetch 解包 + ldd 缺库 → run 看门狗 + 停 xochitl 前台跑 → restore 拉起并核 `NRestarts`），需用户在设备旁执行（涉及停 xochitl）。该目录已不在仓库。
- 网关传书 tab 曾只读展示 `reading-qol.json` 四开关（点击翻页/快速黑白/清残影/字体增强，书架不写它，改去设置→系统增强）——当时形态，现状以网关白皮书为准（未核）。

### 03i｜host 优化 vs 设备端优化：对标现状与移植计划（2026-09-03 傍晚，用户问"对标一致吗"）

> **⚠ 大部分已过时**：文中"host 优化"指电脑端 Calibre 洗书链路，已于 2026-09-18 砍除（附录 B）；设备端优化（`bookconv`）是现存的唯一路径。本节保留"当时为什么把 Calibre 那层规则移植进 Rust"这条脉络。

**当时结论：优化器同源一致，清洗层与质量门不对标。** host 路（Calibre `ebook-convert` + `wash_epub.sh` + `check_output.py`）比设备路多三层：Calibre 级 CSS 拍平（2026-09-02 按屏校准的杠杆：每页 +16% 行、全书 −15% 页）、EPUB/PDF 质量门、格式转换（HUFF/CDIC AZW3 只有 Calibre 能解）；优化器 `optimize_epub` 两边是同一个函数。

**用户拍板"先做清洗层+质量门移植"（2026-09-03 深夜）**：新规则进 `bookconv`，两边由构造保证一致。**现行规则以当前代码为准**（`shelf/crates/bookconv/src/wash/`，引擎细节见 bookconv 白皮书 §02）：

| 规则 | 当前行为 | 与 2026-09-03 原版的差异 |
|---|---|---|
| 伪 DRM（`wash/drm.rs`） | `encryption.xml` 只列 .css/.ttf/.otf/.woff/.woff2/.js → 丢文件 + encryption.xml + manifest 项；列了正文/图/导航 = 真 DRM，**报错停、原样不动** | 无 |
| CSS 锁（`wash/css.rs`） | .css / `<style>` / `style=""` 三处剥 `DEFAULT_FILTER_PROPS`=`font-family`/`font-size`/`font`/`background-image`/`background`（`@font-face`/`@import` 不动；实体 `&#39;` 内分号不截断） | 原版还剥 `color`/`background-color`/`text-align`（照抄 Calibre，2026-09-17 收窄，§03av） |
| 边距/段距 | body/html/@page 的 margin/padding 整个删；含 p/div 的选择器与内联：**上下** margin/padding 归零、左右保留；`keep_para_spacing` 只保留段距 | 原版还**注入** `html,body{…!important}` 内联块——那类注入在 xochitl 上从未生效，v10 起改写外链 `cangjie-wash.css`（§03q、§03y） |
| 空页（`wash/empty_pages.rs`） | spine 里无文字无图的页（`mbppagebreak` 独占页）→ 删 zip/manifest/itemref，目录改指下一篇；全空不动 | 无 |
| 自动目录（`wash/toc.rs`） | `IfMissing`（缺省）零条目时从 **h1–h6** 生成多级 `toc.ncx` + `nav.xhtml`；`Always` 强制重建 | 原版只 h1/h2（v7 扩，§03q） |
| 双 id | 每章先 `collapse_dup_id_attrs`（先修再拦） | 无 |
| 质量门（`check.rs`） | 真 DRM / 目录 href 命中率 <80% / 单标签双 id → **硬失败**；无目录（`require_toc` 升失败）、锚点丢失 → 告警 | **设备端不跑门**；门只被 `epub-optimize --check`（不过退出码 3）调用；PDF 门（pymupdf）从未移植 |

**当时的真机数据（2026-09-03 深夜，WiFi）**：造一本双 id + 无目录的坏书——门拦下未清洗的坏书、清洗后过门且自动目录 2 条；真书《飘·上册》（多看伪 DRM `dkagent.css`，10.6MB）设备端 `optimize=auto` **90s**，回执"已清洗+优化（35 章，10631321→3183269 字节，剥伪 DRM 1 项）；质量门通过（目录 34 条）"（host debug 版同书 33s、同结果）。⚠ 当时的 Pipeline / `optimize=off|plain|auto` / `check=` 直投接线早已随 §03s 删除；现行入口是母版库 `POST /staging/optimize {name}`（异步，进度见 §03bg）。

**当时未坐实的假设已有答案**：xochitl 不认 `!important`（§03q v10 / §03y），元素选择器与外链 css 才是有效路径。

**《镖人》三路对照（中断，无结论）**：AZW3 297MB；设备端上传到 54MB 时 book-serve RSS 2.5MB（流式落盘坐实）；host `wash_epub.sh` 2m41s 产出 282MB EPUB（156 章/2473 图，~900×1300 未触发降采样）；`comic2cbz.py` 8s 出 2473 页 CBZ。用户叫停：**漫画几乎全是图，清洗层对它无意义，对照应换文字书。**

### 03q｜书籍优化深层优化：做精做细做强（2026-09-04，用户"只做精做细做强"）

> 📖 优化引擎的机制细节（清洗层/优化遍/脚注四形态/图片降采样/**xochitl 渲染硬规则**/版本演进）见 **`bookconv优化白皮书.md`**。本节只记这几轮的诉求、决策与真机反馈。
> **状态**：其中 host 侧（Calibre、`shelf push`、`pdf_reflow_move.py`、KOReader 单独优化）已被 §03s / §03av / 2026-09-18 砍除取代，下文相应条目按"当时做法"读；xochitl CSS 结论以 §03y 为准。

四诉求：① 格式仅留 EPUB+PDF；② 做精=按 Move 屏参数/xochitl 裁切规则全面优化；③ 做强=图片美观、中英文各按习惯、不锁字体、不缺目录、**脚注自动呈现**、PDF 学术重排；④ 做细=host/端、EPUB/PDF 统一优化，xochitl≈KOReader 一致。分 6 阶段（每阶段独立构建 + 真机验证）。

**两条真机判死（决定可行边界，穷尽实测）**：
- **xochitl 弹窗脚注判死**——闭源渲染器不实现弹窗、正文点击不通知可注入的 QML 层；竞品「镇纸」的弹窗是另开 WebView 载微读网页版。∴ xochitl 脚注只能内联常显或"跳转 + 浮标"。
- **端上 PDF 重排不现实**——PDF 栅格化没有 musl-friendly 的纯 Rust 方案。

据此三决策：脚注 xochitl 内联常显 + KOReader 弹窗；PDF 走 born-digital 结构化重排→EPUB + 扫描件 k2pdfopt 兜底（不移植 k2pdfopt 位图引擎：28+43 个 C 文件，高投入低回报）；其它格式拒收、引导走 host Calibre。〔后两条随 host 砍除失效；PDF 现走 `bookconv::pdf_ingest`：有文字层转 EPUB / 无文字层仅裁边，见 bookconv 白皮书 §18。〕

| 阶段 | 当时做了什么 | 现状 |
|---|---|---|
| A 格式收敛 | 只收 EPUB/PDF，其它拒收 + 引导语 | `target.rs` 已删，母版库 `BOOK_EXTS` = EPUB/PDF（§03s、§03av） |
| B EPUB 做精做强（v7） | ① `wash::LangMode` 按全书 CJK/拉丁字符占比探测：中文 `text-indent:2em`、拉丁 `1.2em` + 标题后首段不缩进；② auto-TOC 从 h1/h2 扩到 h1–h6 并 dense-rank 多级嵌套；③ 不锁字体 | 沿用 |
| C 脚注目标感知 | `FootnoteMode{Inline,Anchor}`：native 默认 Inline（就地内联 `<span class="cj-fnote">〔…〕</span>`，**去标签成纯文本**，防块级标签塞进 `<p>` 致 xochitl 严格 XML 整章白屏）；`collect_footnote_notes` 支持扁平 `<div>` 注释 | 2026-09-17 缺省改 `Anchor`（§03av）；`Inline` 仅测试与历史用 |
| D KOReader 一致性 | koreader-serve 收 EPUB 走同一优化，脚注用 Anchor 让 `link_prefer_footnote` 触发底部弹窗 | 已撤：§03s 后落库=纯复制母版字节，KOReader 拿到的就是母版库里的产物 |
| E PDF 重排（host） | `pdf_reflow_move.py`：PyMuPDF 逐页文字覆盖率分流、结构化抽 blocks 重排成 EPUB | host 已砍；设备端 PDF 见 bookconv §18 |
| F 真机验证 | `!important`/line-height、内联脚注长注观感等 | 结论见下与 §03y |

**Phase F 真机（2026-09-04，用户拍照《飘·上册》逐页核）——三条 xochitl 渲染硬规则 + 一条误诊教训（`OPTIMIZE_VERSION`→8）**：
1. **内联脚注 marker 若是图标 `<img>` 必须丢弃**：《飘》marker=`<a noteref><span class="koboSpan"><img alt="note" 70×95/></span></a>`，xochitl 行内图按**固有尺寸**渲染 → 图标巨大，一页 7 个脚注重复 7 次。修：`Inline` 一律丢弃原 marker，就地只留 `〔注释纯文本〕`。
2. **EPUB 内嵌图卡竖向框（宽 ≤954）**：`class="logo"` 1696×630 的内联横幅按固有 1696px 宽渲染 → 溢出竖屏。修：EPUB 图走 `imgopt::downscale_for_epub`（竖向框 954×1696）；块级图适配列宽，行内图不再溢出。
3. **xochitl 无视 `no-repeat`/`background-size`，把背景图平铺**：分卷页 `body.fen{background:url() no-repeat bottom center;background-size:100% auto}` 被平铺满页盖正文。修：`filter_props` 加 `background`/`background-image`（`@font-face` 的 `src:url` 豁免；章头 `<img>` 装饰不受影响）。
- **⚠ 误诊教训**：先把"分卷页多幅图"当成"章头 banner 跨章重复"，加了"同图被 ≥5 章引用即删其 `<img>`"——**错**：章头 banner（每章一张村舍插画）是用户要保留的装饰（照片 IMG_0447 判"对"），真凶是 CSS 背景图。已 `git revert`。**看不到屏幕别猜渲染，让用户拍照。**真机核：分卷页背景 url 30→0、章头 banner 保留 30、内联脚注 32 全好；投递走 book-serve loopback，避 WiFi 云同步卡顿。

**第 2 轮用户反馈（2026-09-04，测「基本书」6 条 bug + 2 问）**：
- **★幂等门只看标记存在、不看版本 → 版本升级永远挡在门外（根因，波及全部 v7+ 改进）**：旧的 `is_optimized`=有标记就跳过，v6/v7 优化过的书 bump 到 v8 后重传被整步跳过，永远拿不到中英文缩进 / 脚注·背景修复——完美解释"反复测基本书都看不到优化"。修：按标记版本与当前版本比对，旧版本重优化升级；重优化安全性由 `double_optimize_inline_footnote_no_dup` + `reoptimize_relinked_footnote_no_dup` 坐实（注释不翻倍）。现状：Pipeline 已删，母版库按 full/core/old/none 显示、full 才隐藏「优化」按钮（`staging/library.rs` `probe_level`）。⚠ 设备上存量老书需重新优化才升级。
- **传书目标合并为文件夹选择**（用户拍板）：原"原生阅读/原生批注"二选一对 PDF 只差落地文件夹，改为单一「传书到 xochitl」+ 文件夹预设。
- **客户端格式预拦**：`uploader()` 加 `okExt`，选中即按扩展名判、不合格标红不上传。
- **文案去生硬 / PDF 端上不重排是设计如此**（端上无栅格化器）。
- **端/host 一致性（答用户）**：一致——两条路末步都调同一 `epub-optimize`（=`bookconv::optimize`），host 只多 Calibre 级 CSS 拍平与非 EPUB/PDF 转码。

**★首行缩进根因终结（2026-09-04 真机 7 版诊断书 + 《飘》活样本，`OPTIMIZE_VERSION`→10）**：bug「中英文没缩进」死磕出 xochitl 的 CSS 行为，纠正此前一路错判（细则见 §03y）：
- **xochitl 只认外链 `.css` 里的规则，完全无视内联 `<style>` 与 `style=`**。⟹ v6–v9 注入的内联 `<style class="cj-wash">`（边距归零/首行缩进）**从未生效**，之前"有效"的只是"删改源文件"类改动（剥字体锁/去背景图）。活样本：《飘》自带外链 `p{text-indent:2em}` 真机缩进正常。
- 当日结论"不认 `!important`、不认类选择器、只吃裸元素选择器"里，**`!important` 不认成立；"不认类选择器"后被 §03y 证伪**（是尾分号规则的误读）。
- **中途死路（记录以免重走）**：先误判"xochitl 无视 text-indent"，试段首 `&nbsp;`（v9）——宽度随字体变且**连续 nbsp 被折叠**，做不到精确 2 字；全角空格 U+3000 / em 空格 U+2003 被 xochitl 吞掉（前置空白折叠）；`inline-block` 占位被无视。全撤回。
- **根治（v10）**：排版规则写成外链 `cangjie-wash.css` + 每章 `<head>` 注 `<link>`（相对路径 `relative_to`）+ OPF manifest 补 `<item>`；中文 2em / 拉丁 1.2em，**em 单位=字体无关、精确 2 字**；`add_wash_css_entry` 幂等。真机《缩进v10无important验证》3 段精确 2 字缩进坐实。**方法论**：一个真机活样本（《飘》）＞ 七版凭空诊断。

### 03y｜xochitl CSS 引擎实测规则（2026-09-06，八轮"诊断 EPUB"量渲染缓存，3.28.0.172）

> **本节是这套规则的权威版**（比 bookconv 白皮书 §09④ 更新、更完整；§09④"只认裸 `p{}`、类选择器让整表失效"是 2026-09-04 的早期结论，已被本节第 3、7 条证伪，以本节为准）。

![xochitl 吃 CSS 的规矩](diagrams/xochitl-css-rules.svg)

**方法**：造只有一个变量的诊断 EPUB（每章一种写法 + 同章一个对照 `<p>`），scp 进 inbox → 设备上 `wget --post-data` 打 `127.0.0.1:8790/staging/deliver` 投原生（免密）→ xochitl 导入即渲染 `<uuid>.pdf`（也可 `GET /staging/render/{uuid}` 取）→ pymupdf 量每段首行相对下一行的 x 偏移（只量会换行的段）。**全程不需要肉眼、不需要用户点**；一轮 2 分钟；诊断 7–14 共 60 个变体。

**钉死的规则**（推翻/修正了"只认外链裸 `p{}`"的旧结论）：

| # | 规则 | 后果 / 清洗层怎么做 |
|---|---|---|
| 1 | **规则里最后一个没有分号收尾的声明被丢掉** | `p{text-indent:2em}` 整条无效，`p{text-indent:2em;}` 生效；书 css `.calibre_ {…;text-indent:1.2em;margin:0}` 的 `margin:0` 一直被吞。清洗层所有输出声明一律尾分号（`filter_decls`、`wash_css`）。keep-spacing 档位此前因此零缩进 |
| 2 | **`text-indent:0` 被当"没设"**，落回继承/其它规则；`0.01em`、`-0.01em` 生效 | 顶格规则写 `text-indent:0.01em` |
| 3 | **类选择器认**（`.x{text-indent:2em;}` 对 `div.x` 得 2em），且**类规则压过元素规则** | 此前"不认类选择器"是规则 1 的误读 |
| 4 | **同为类规则时先出现者胜**（书的表链接在前 → `.calibre_` 压住我们的 `.cj-flush`，无论我们的表链在前后、无论 class 属性顺序）；同选择器后写的 `p{}` 也压不过先写的 | 所以顶格段要**剥掉书的类** |
| 5 | **不认内联 `style=""`**；`div{}` 元素规则不生效（div 只吃类规则/继承） | — |
| 6 | **text-indent 继承**：`<div class="calibre1">`（书的 1.2em）里的子 div 若自己"没设"（含 0）就继承缩进 | Sheldon v5 顶格失败的根因 |
| 7 | 外链 css 里混类选择器不会"废整表"（诊断 11 V4/V5 对照 p 正常）；`!important` 仍不认 | 旧结论来自当年带 `!important` 的实验 |

**最终配方（真机量：章首/场景切换 0pt，续段 14.2pt）**：顶格段 → `<div class="cj-flush">`（**剥掉书的类与 style**，只留 cj-flush；id 等保留）+ `cangjie-wash.css` 加 `.cj-flush{text-indent:0.01em;margin-top:0;margin-bottom:0;}`。判定顶格的信号见 `wash/typeset.rs::flush_first_para_after_heading`：h 标签后 / 加粗或 `Chapter…` 开头的标题样段后 / 双 `<br>`·空段·`* * *` 后 / 章首。KOReader 走标准 CSS 同样顶格。

**中文书（同日闭环）**：用户经母版库「优化」后投原生的《人骨拼圖》仍零缩进——这本 2017 年旧 EPUB **全书没有 `<p>`**：每章一个 `<div>`、450 个 `<br/>` 分行、每段开头两个全角空格 U+3000。`p{text-indent:2em}` 没对象；xochitl 把 U+3000 折叠掉 → 0；KOReader 按字体宽度画 → 用户看到"中文换字体缩进跟着变"。清洗层中文模式新增 `wash/typeset.rs::cjk_paragraphize`：文件里 `<br` ≥4 且无 `<p>` 时按 br 切成 `<p>`（块级标签原样），所有 `<p>` 段首的全角空格/nbsp 剥掉，缩进统一走 css。真机：设备端重新「优化」→投原生，渲染缓存量到 278 个段首 24.1pt（=2em）、续行 0。

### 03aq｜真机排查"抓完优化还是大量留白"：wash 补 figure/figcaption 边距 + 证伪 + 找到真正成因（2026-09-10，真机 A/B 验证）

用户拿真实文章（aeon.co 一篇网文）反馈"抓完优化后还是有大量留白"，并点出"有图，夹在中间"。没有直接照猜测动手，先核实：非 mock 抓取（`bookconv::article::build_article_epub`）→ `optimize_epub_with` → 检查产出 XHTML。

**症状→假设→证伪→真因**：
1. **读源码发现的真实缺口**：`wash_css()` 只对 `<p>` 清零过 margin/padding/text-indent，从没管 `<figure>`/`<figcaption>`——网文管线的图恰好包成 `<figure><img/><figcaption>…</figcaption></figure>`。补上 `figure{margin:0;padding:0;}` 与 `figcaption{margin:0;padding:0;}`。**两条必须分开写，不写成 `figure,figcaption{}`**（xochitl 解析器脆，逗号/复合选择器整条失效；`wash::tests::lang_aware_indent` 早就断言 `!cjk.contains(',')`，新测试 `figure_and_figcaption_margin_zeroed_as_separate_bare_rules` 同样守住）。`keep_para_spacing` 档位不管图片边距，始终清零。
2. **真机 A/B 证伪**：同一篇文章分别用修复前/后的 `optimize_epub_with` 生成 EPUB → 传母版库 → 投原生 → 拉回 xochitl 自己的渲染缓存 PDF（`GET /staging/render/{uuid}`），25 页逐页对比：**修复前后像素级完全一致**。这条边距缺口不是留白成因，只是顺手堵上的真实但无关的缺口。
3. **真正成因**（同次诊断直接看出）：留白集中在两处——第 1 页正文结尾后半页空白、翻页整页是第一张图；一组两张图的画廊占满大半页后剩余空间空白。共同模式：**图片块（单张或连续画廊）在当前页剩余空间放不下时整体推到下一页，当前页余下空间不回填**——渲染引擎自己的分页决策，跟样式表无关（改 CSS 前后一致即证据），EPUB/CSS 层没有找到杠杆。与印刷排版"孤图不裂开、宁可留白"同类，没有"零改动/零留白/一张不少"的三全解。

**顺手发现、未修**：① aeon.co 原站用 JS 轮播只显示 1 张，`readability_rust` + `article.rs` 白名单抽取把轮播里全部 3–4 张**不同**图（逐张核对 `src`）连同 figcaption 展开——内容更全但页数/留白也涨，是"要内容全还是要页数少"的取舍，不是 bug。② 渲染结果里混入"1 of 4"/"1 of 3"轮播页码文本，**在原始抓取 HTML 里搜不到**，不是 `article.rs` 拼的，猜测来自 `readability_rust` 对无障碍属性/隐藏计数元素的转写，未验证；不影响阅读，留作已知问题。

**验证边界**：这次"真机验证"就是排查过程本身；诚实结论是"改了一处真实缺口，但它不是用户所报留白的成因"，**不是**"已解决用户的问题"。测试产物（母版库 + 原生书库各一份 "Continental divide"）已清理，原生库走回收站软删；核对时发现原生库里已有两份用户自己创建/打开过的同标题文档（`lastOpened`/`lastOpenedPage` 非零、时间早于测试），**只删了本次新建的两份**（uuid 精确匹配、`lastOpened:"0"` 确认从未打开），用户的两份原样保留。离线：`cargo test -p bookconv` 当时 110/110。

### 03av｜书籍优化架构调整：拆 EPUB 线/PDF 线，EPUB 线四原则落地（2026-09-17，⚠️ 只 host 单测验证，未上真机）

> **状态更新**：标题为当日初稿状态；同日随后补了**功能层真机验证**（见文末，产物字节层面通过），**视觉层仍未由用户屏幕确认**（同末尾清单）。原则③（注释位置）当天被用户真机反馈推翻，以本节原则③的"最终版"为准。

用户拍板把「书籍优化」拆成 **EPUB 线**和 **PDF 线**两条独立业务逻辑（PDF 线原则用户还没给，本节只记 EPUB 线），并提三个架构约束：① 设备端只收 EPUB/PDF，其余格式请用户自行转换后上传；② EPUB 优化全部在设备侧进行；③ EPUB 线四条优化原则（保留目录 + 拆分章节序号、解锁字号但保留原书颜色/加粗、注释移到引用段落之后、漫画自动识别不压画质）。

**架构收敛**：此前 EPUB 优化有两条并行路径（host CLI `epub-optimize` 由 `shelf push` 前置调用；设备 `book-serve` 由母版库「优化」触发），同一个 `bookconv::optimize::optimize_epub_with`。这次**彻底退役**"host 用 Calibre 把 AZW3/MOBI/AZW/PRC/FB2/TXT 自动转 EPUB 再推设备"（用户原话）。〔一天后 host CLI 整体砍除，§03av 之后只剩设备侧。〕

**四原则落地（全部在 `shelf/crates/bookconv`）**：

| 原则 | 落地 | 关键细节 / 风险 |
|---|---|---|
| ① 目录 | `wash/toc.rs`：`split_numbered_title`——标题"标题+编号"结尾（如「第一章 1」）拆成父级标题 + 缩进子级编号两条 TOC 条目，同指一个锚点（EPUB3 nav 嵌套 `<ol>`，不是 hack）；编号 >99（或 0）视为印刷页码残留，不拆。无 h1–h6 语义标题时兜底 `fallback_spine_toc`：按 spine 文件边界、取正文首段文本当标题；多数页面没有可提取文本（疑似漫画/画册）时不生成 | 拆分正则支持阿拉伯数字与汉字数字 |
| ② 字号解锁 | `DEFAULT_FILTER_PROPS` 从 8 项收窄成 `font-family`/`font-size`/`font`/`background-image`/`background`：`color`/`background-color`/`text-align` 不再剥 | 原名单是照抄 Calibre `--filter-css`，不是针对 xochitl 验证过的必要行为；`background*` 仍剥是因为坐实的平铺盖正文 bug（§03q）。**⚠ 风险未验证**：放开 `background-color` 后"深底浅字"块在彩色墨水屏上可能更难读；`boost_text_contrast()` 只处理文字颜色/字重、不处理背景对比度，需挑带彩色底纹的技术书真机测 |
| ③ 注释位置 | **最终 = `Anchor`**（章末 + 锚点跳转，原生返回浮标兜底）。见下方"原则③完整记录" | `book-serve::Staging::optimize()` 与 `epub-optimize` 缺省均为 `Anchor`，与 weread/pkm 线一致 |
| ④ 漫画 | 新增 `comic_detect`（移植自 host `comic.py::epub_image_stats`）：沿 OPF spine 统计 `<img>`/`<image>` 数与可见文字数，**图 ≥20 张且平均每张图配的文字 <40 字判漫画**（`MIN_IMAGES`/`TEXT_PER_IMAGE`）。命中后 `imgopt::trim_margins` 裁四边纯色/近纯色留白（整行/列颜色高度一致才裁，`TRIM_TOLERANCE`=8），JPEG quality 95（普通插图 85） | 单边裁边上限当日 15%，2026-09-19《镖人》版权页实测留白 22–29% 裁不净，现为 35%（`TRIM_MAX_FRACTION`）；EPUB 漫画后来改走单趟 `prepare_comic_page_for_epub`（bookconv §19）；与 CBZ 转换路径专用的 `dither_bilevel` 灰阶抖动方向相反，不共用 |

**原则③完整记录（同日两次决策）**：`FootnoteMode` 原只有 `Anchor`（跳章末，但 reMarkable 会吞互相引用的锚点对，**点了跳不回来**，真机坐实过）和 `Inline`（就地内联，**打断段内阅读**）。当天先加第三种 `ParagraphEnd`（不跳转，在"含该引用的整段"结束后插分割线 + 注释块，marker 改纯 `<sup>N</sup>`），设备侧切到它，构造测试书真机上传优化、解包核对产物字节按设计跑通。**用户随后拿真实转换的《赎罪》测试，反馈"注释并未在当前页最下面，而是在注释标记的段末"**——用户真正想要"翻到哪页，注释固定在那页最下面"。核实结论：这不是实现细节，EPUB 是流式重排文本，"这段内容落在第几页"是阅读器翻页时才算出的运行时结果，做书时无从得知；真正的页底定位需要固定版式（PDF 线的领地）。用户确认后拍板改回 `Anchor`，`ParagraphEnd` 连同实现和测试**整个删除**（已证实不符预期，不留死代码）。"功能按设计跑通"与"设计不是用户想要的"是两件事，分开看。

**格式收窄的连带影响（用户明确拍板，非默认选项）**：当时问过用户，AZW3/MOBI/PRC 除转 EPUB 洗书外还有一条独立用途（`shelf push` 解析 PalmDB 判漫画、转 CBZ 进 KOReader），退役要不要连带——用户选**连带禁止，一律拒收**，`comic.py` 的 `palmdb_image_ratio`/AZW3 分支一并删。⚠️ **这是主动放弃一条已验证可用的能力**：2026-09-05 格式分档调查实测坐实这些格式 KOReader 的 crengine 都能读，是用户为规则一致性主动收窄，不是"读不了才收窄"。现状：`rmsvc_core::formats::HOST_CONVERTIBLE_EXTS` 整档删除；2026-09-18 再收紧后 `BOOK_EXTS` = `NATIVE_EXTS` = `["epub","pdf"]`，母版库上传门（各服务共用）自动拒收，回执"不是书籍格式，母版库只收 .epub .pdf"（`staging/mod.rs`），不再有 `shelf push` 一侧的拒绝逻辑。

**待确认、未落地的一条建议**：host 不再预优化后，网页直接上传的 EPUB 在用户手动点「优化」前仍是未解锁字号/未拆注释的原始状态（改动前就存在，不是回归）。建议 `book-serve` 收到 EPUB/PDF 上传时自动优化（版本标记幂等，具备基础设施）——**未拍板、未实现**；当前只有网文抓取 `POST /staging/fetch-article {url, optimize?}` 带"同步优化"选项。

**验证状态（2026-09-17）**：`book-serve`/`gateway` 交叉编译部署真机（备份 + md5 核对 + `systemctl restart` + `ActiveState/MainPID/NRestarts` 健康检查），**功能层真机验证已过**：SSH 隧道到 `127.0.0.1:8790`，真实 HTTP multipart 上传四本构造 EPUB（四原则各一本）+ 一份 `.azw3` 假文件，`POST /staging/optimize` 后 scp 回产物解包核对：
- ① TOC：「第一章 1」在 `nav.xhtml`/`toc.ncx` 里拆成父级「第一章」+ 缩进子级「1」，「后记」没被误拆，两处指向同一锚点。
- ② `style="color:#0000ff;font-weight:bold;font-size:9px;"` → `style="color:#0000ff;font-weight:bold;"`，颜色/加粗保留、字号锁被剥。
- ③ 段末块注释：产物 marker 为纯 `<sup>1</sup>`，`</p>` 后插 `<hr/>` + `<div class="cj-fnote-para"><p>1. …</p></div>`——**历史记录**：`ParagraphEnd` 随后被撤回，此条只证明"按设计跑通"。
- ④ 25 页 2200×3400（四边留 250px 纯白）测试漫画优化后单页 954×1623，与"先裁四边再按屏幕框缩放"吻合（不裁应为 954×1474），确认 `trim_margins` 真在裁、并按漫画 quality 重编码。
- ⑤ 上传 `.azw3` 假文件收到干净回执，非裸错误堆栈（当日文案列了一长串格式，现为"只收 .epub .pdf"）。

**⚠️ 意外发现（真机性能）**：上述 25 页测试漫画在真机 `POST /staging/optimize` 跑了 **2 分 19 秒**（host 秒级）——`trim_margins` 逐行/逐列纯色扫描是 O(留白像素数)，在弱 ARM 芯片上被放大两个数量级；当时优化按钮同步阻塞、没有进度。**后续已处理**：优化改异步 + 真实进度（§03bg）、图片并行（乱马 01 设备 285s→148s）、EPUB 漫画走单趟管线，见 bookconv 白皮书 §19；真实大漫画耗时仍是分钟级。

**仍未验证（需用户在真机屏幕上肉眼确认，本环境无屏幕访问）**：以上只证明"产物 HTML/图片字节正确"，不等于"xochitl 渲染视觉正常"——① 彩色高亮块在 Paper Pro Move 上的实际可读性（原则②风险点，需带彩色底纹的真书）；② TOC 两级嵌套在原生目录 UI 里是否真有缩进层级观感；③ 注释在真实翻页阅读中的观感（现为 `Anchor`）；④ 一本长期在跑的普通文字书完整走一遍新流程做回归，确认没破坏正常渲染。

#### 本章坑位表（二）

> 第 B 章后半（§03aw–§03bo）：异步优化/落库的忙锁、《疯探》"没有 TOC"的多轮排查、优化进度、封面声明。前半的坑位表见"（一）"。术语：xochitl = reMarkable 官方阅读器/UI 进程；NCX = EPUB2 的目录文件 `toc.ncx`；manifest = `content.opf` 里的文件清单；`.epubindex` = xochitl 为每本 EPUB 自己生成的内部索引（章节路径 + 标题），原生目录面板显示与否取决于它里面有没有标题；sidecar = 母版库里挂在书旁边的小 JSON，记这本书的处理结果。

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见 § |
|---|---|---|---|---|
| 优化/落库是同步 HTTP | 大漫画优化 2 分 19 秒，请求卡死；处理中点删除会真删文件 | 同步阻塞 + 母版库没有"这条正被处理"的状态；`deliver` 起初只查忙不持锁 | 进程内忙锁 + `spawn_*` 异步；服务端权威 `busy` 字段，前端据此禁用按钮；忙锁不落盘，重启天然清零 | 03aw · 03ay |
| 前端 `onclick=async` 无防双击 | 连点触发重复请求 | 十几处 handler 没有禁用态 | 统一 `guardClick(el,fn)`（点击即禁用、`finally` 解禁） | 03aw |
| 书内 HTML 目录页从 spine 消失（《疯探》） | manifest 与文件都在，但翻页翻不到目录页；回执"删空页 2" | 项目最早期的启发式：页面链到 ≥10 个不同 html 就当"冗余目录页"摘出 spine，从没单测覆盖 | 目录页是作者放的正文，不是冗余物；整段代码已删，不留任何目录页特判 | 03ay |
| 原生目录入口整个不出现，但书能正常翻页 | `.epubindex` 每条记录只有路径、没有标题（不是标题错，是没写） | xochitl 二进制里**硬编码死查** manifest 里 `id="ncx"` 的 item，不认 `<spine toc="IDREF">` | NCX 条目 id 必须叫 `ncx`；自己插条目也别起别的名（曾写 `cj-ncx` 是活 bug）；见下方排查链 | 03bc |
| 对照两本书就下"根因"结论（`dtb:uid`） | 修完字节层面全对，入口仍不出现 | 两书差异不止一项，只挑了最显眼的 | 对照法必须逐项单变量隔离；"字节正确"≠"视觉出现"，要用户在设备上点开确认；方法论见排查链 | 03az · 03bb |
| 优化进度条不动 | 文字书/漫画优化阶段都没有进度 | `OptimizeCheck` 压根没有 `progress` 字段（不是"文字书不走流式"——所有 EPUB 都走流式） | 给流式阶段二加逐条目回调 + 每 5 条节流写盘/推 SSE；进度回调不改变内存占用，别指望它省内存 | 03bg |
| 封面缩略图取不到 | 日志 `failed extracting cover: got null cover image` | 封面条目 id 带点（如 `x00000001.jpg`）且只有 `<meta name="cover">`；或 meta 指向的不是图片（239 字节文本残片） | meta 与 `properties="cover-image"` 必须同时有；在清洗之前补声明 | 03bo |
| Anchor 模式脚注可能产出嵌套 `<p>` | 注释源自带内层 `<p>` 时得到 `<p id><p>…</p></p>` | `preserve_relink_footnotes` 的 Anchor 分支固定包一层 `<p id>` | ⚠ 未处理、xochitl 渲染未验证（读代码：`htmlproc/footnote.rs` 里 `<p id=…>` 仍是固定包裹，仅 duokan 形态有拍平） | 03aw |

#### 《疯探》"没有 TOC"排查链总览（§03ay → §03az → §03bb → §03bc）

同一本书（"番茄小说 EPUB Generator" 产物，94 章）连续四轮反馈，前三轮归因都不对。下表是压缩后的假设→证据→结论链，这是整条线最值得复用的部分。

| 轮次 | 假设 | 证据 / 动作 | 结果 |
|---|---|---|---|
| ① §03ay | 优化器把目录页摘出了 spine（"目录没了"） | 本地跑 `epub-optimize`，`toc.ncx` 字节不变，但 `content.opf` spine 里 `idref="toc-html"` 被删 → 定位到 `remove_toc_from_spine` | **成立但只是另一个 bug**：修掉后书内目录页保住，原生目录入口仍没有（用户明确指的是原生目录面板"三个横杆"入口） |
| ② §03az | `dtb:uid`（NCX）与 OPF `dc:identifier` 不一致：《疯探》两个不同 uuid，《雪人》一致且目录正常 | 拉两本真书对照字节；加 `fix_ncx_uid` 后真机重投 | **被证伪**：uid 已同步，入口仍不出现（修复保留，是 EPUB2 规范要求） |
| ③ §03ba（不在本块） | `toc.ncx` 的外部 DTD `<!DOCTYPE>` 让解析器联网卡住 | 加 `strip_ncx_doctype` 后真机复测 | **被证伪**（修复保留，无害） |
| ④ §03bb | 不再猜字段：逆向 `.epubindex` 拿判据，再"真书字节重打包 + 单点替换"二分 20+ 轮（细节见 §03bb 表） | 《疯探》94 条标题位全空；仅把重建版的 `content.opf` 换成真书字节即复现 | 定位到 `content.opf` 文件本身，**但没锁定字段**，暂停并请示是否反编译 |
| ⑤ §03bc | 反编译 xochitl（`epubcontext` 管线）看它到底怎么找 NCX | 二进制里有写死的 3 字符 `"ncx"`，用来查 manifest 的 id 表；真书 `id="toc"` + `<spine toc="toc">` 完全合规却查不到 | **根因坐实**：只改一行（`id="toc"`→`id="ncx"`，`spine toc` 同步）`.epubindex` 即 7188→15558 字节、94 条标题全出 |

**为什么二分测了 20+ 轮都没找到**：所有测试只变动 `content.opf` 的*内容*字段（描述文本、`linear`、声明顺序），没有一轮测*id 命名*；而《雪人》的夹具恰好叫 `id="ncx"`（Calibre 等主流工具的默认命名），所以"能用的书"长期掩盖了这条隐含约定。**方法论**：① 先造可量化判据（`.epubindex` 标题字段）；② 二分要覆盖"名字/格式/顺序/内容"所有维度；③ 二分穷尽而黑盒不透明时，反编译比继续猜更划算。

### 03aw｜真机反馈第二轮：脚注撤回复核+异步优化真机全链路验证+防双击（2026-09-18，真机通）

用户拿真实转换书（《赎罪》）测 §03av 后反馈 5 条：①「其他」页删字体后设备端仍显示存在（font-serve，另一模块，后续单独处理）；②注释没在页底、在段末；③书籍依然被锁字体；④优化过程中点删除这类并发操作要拦；⑤所有操作都该异步。②③④⑤当天处理。

**② 脚注复核**：完整决策（撤回 `ParagraphEnd`、改回 `Anchor`）见 §03av。重新部署 `Anchor` 版、同一份 `footnote-test.epub` 真机重跑：marker 变回可点的 `<a href="#fn1">1</a> <a href="#fn1">[1]</a>`，注释回到章末 `<div class="footnotes">`。
**顺手挖到的既有缺口**（非本次引入）：注释源 `<aside epub:type="footnote" id="fn1"><p>注释文字</p></aside>` 自带内层 `<p>`，`Anchor` 分支又固定包一层 `<p id="{frag}">`，产物是嵌套 `<p id="fn1"><p>…</p></p>`。XML 仍良构（不同于《消失的爱人》重复属性的整章白屏），xochitl 渲染未验证。⚠ 读当前代码仍未处理（`htmlproc/footnote.rs`，duokan 形态另有拍平），仍是待办。

**④⑤ 根因与修法**（症状→根因→修法→教训）：

| 项 | 内容 |
|---|---|
| 症状 | 25 页测试漫画优化真机耗时 2 分 19 秒（§03av），`POST /staging/optimize` 同步阻塞，前端等于卡死；优化没跑完时点删除会真删文件，优化线程再回写就写进不存在的文件（同名文件被顶替则有数据错乱风险，设计上存在、未实测触发） |
| 根因 | 同步调用 + 母版库没有"这条正被处理"的状态 |
| 修法 | ① 忙锁：进程内、不落盘，重启天然清零，不会"重启后永久卡忙"（首版是 `Arc<Mutex<HashSet<String>>>`，现已并入 `OpRegistry`，`book-serve/src/ops.rs`，同时承载 2026-09-20 加的取消协作与 `POST /staging/cancel`）。② `spawn_optimize()` 先同步做格式/存在性校验（错误立即回给调用方），通过才加锁、起后台线程，`catch_unwind` 兜底 panic 保证锁一定被清；`remove()`/`deliver()` 忙时直接拒绝。③ 异步结果写进 sidecar `delivered.optimize`（`OptimizeCheck{status,message,at,progress}`），完成后推 `books/staging` 事件——**复用现有 SSE 零轮询架构，没新增任何轮询**。④ `GET /staging` 每条带 `busy`，网关母版库据此禁用删除/落库/加入 KOReader/再次优化——服务端权威，跨设备/跨浏览器刷新都准 |
| 教训 | 忙判断必须在服务端；前端本地禁用只挡得住同一标签页的同一按钮 |

顺手补防双击：审计出十几处 `.onclick=async`（字体/壁纸删除、笔记回收站批量操作、模型管理、管理页模块启停/卸载等）完全没有禁用态，新增通用 `guardClick(el,fn)`（`gateway/ui/app.js`：点击即禁用、`finally` 解禁），与母版库列表原有 `btn()` 的防护统一成同一套心智模型。

**真机全链路验证**（测的是"异步 + 忙锁"架构本身，SSH 隧道直连 `book-serve`）：上传测试漫画调 `/staging/optimize` → HTTP 立即回 `"async":true`；处理期间 `GET /staging` 该条 `busy:true`；同时 `/staging/delete` 被拒（"《comic-test.epub》正在优化中，请等它跑完再删除"）；约 2 分钟后 `busy` 回 `false`，`delivered.optimize` 落 `{status:"ok", message:"已优化…（25 章，4585885→3592642 字节）"}`。忙锁 / 异步返回 / 拒绝并发 / 完成后落盘四环节全部真机验证。

### 03ay｜第四轮真机反馈：落库改异步（补齐"投书按钮"防双击）、真书《疯探》坐实并修复"目录页被优化器吃掉"的老 bug（2026-09-18，真机通）

用户报两条：①「投书」（落库到原生）按钮也要有跟「优化」一样的异步/防双击；②真书《疯探》"原书有目录，优化后反而没了"。

**① 落库改异步**：`Staging::deliver()` 原是同步方法，只在开头查一次 `is_busy`、自己**不持锁**——超限漫画要按卷拆分、逐份建包+上传，真机能拖到分钟级，期间点「删除」或再点一次「投入原生书库」都能在服务端真并发跑起来（UI 的 `btn()` 只挡同一标签页同一按钮）。改法完全照抄 `spawn_optimize`：新增 `Staging::spawn_deliver()`（`staging/deliver.rs`）——零耗时校验（格式/文件存在）通过后加忙锁、起后台线程跑 `deliver()`，HTTP 立即回"已开始"；`deliver()` 本身不查/不占忙锁（职责交给 `spawn_deliver`，同 `optimize()` 裸方法的设计）。渲染自检（`spawn_render_check`）挪进 `spawn_deliver` 的后台线程内部（结果本来就在线程里才出来）。新增 `sidecar::DeliverCheck`（形状同 `OptimizeCheck` 但独立成类型，字段名不借用"优化"影射"落库"）。前端徽章文案"优化中"→"处理中"（覆盖两种忙态）；完成状态跟优化一样靠「已投原生」时间戳徽章 / 「上次投递失败」徽章体现，不再弹窗。

> 当时还写了"`keep=false` 时清母版库改走 `remove_unlocked()`"：2026-09-19 起母版库永远保留、已无"投完自动删除"，`remove_unlocked` 现在只被 `remove()` 调用（`staging/library.rs`），忙锁本体见 §03aw。

真机 SSH 隧道直调 API 验证：`POST /staging/deliver` 立即回 `async:true`、`busy` 立刻变 `true`；忙着时再发 `/staging/deliver` 或 `/staging/delete` 都回"正在处理中，请稍候"；后台线程跑完后 `delivered.deliver.status=ok`、`delivered.native` 时间戳、`delivered.render.status=ok`（渲染自检确实从 `spawn_deliver` 内部起）三样都落盘。

**② 《疯探》目录页消失：根因是一段与"EPUB 线原则①保留目录页"直接矛盾的老代码**（症状→根因→修法→教训）：

- **症状**：staged 原书健全——`OEBPS/toc.html`（94 章链接的目录页）与 `toc.ncx`（94 条 navPoint）都在；本地跑 host CLI `epub-optimize`（与 book-serve 共用 `optimize_epub_with`）复现：`toc.ncx` 字节不变（`auto_toc` 的 `IfMissing` 门正确识别已有目录），但 `content.opf` spine 里 `idref="toc-html"` 那条 `<itemref>` 被删——manifest 与文件还在，只是翻页翻不到。
- **根因**：`count_distinct_html_links` / `TOC_LINK_THRESHOLD` / `remove_toc_from_spine`——启发式"一个 (x)html 页面链到 ≥10 个不同 html 文件，判为冗余书内目录页并从 spine 删掉，理由是 reMarkable 有原生 TOC"。来自最早期的 shelf Phase 0 骨架，从没单测覆盖（`optimize` 自己的测试从不构造真正的 `content.opf`），这次第一次被真书触发。**假设站不住**：①原生 TOC 面板对任意 EPUB2 `toc.ncx` 是否都好用，从未真机确认；②即便好用，书内目录页也是作者放的**正文内容**，不是可丢的冗余物——这条老代码从第一天起就在违背原则①。
- **修法**：整段删除（三个符号 + 两处调用点），不留目录页特判，与模块头部"不重组目录/spine，最大限度兼容各家 EPUB"重新一致。`OPTIMIZE_VERSION` 10→11（旧书需 `force:true` 重优化拿回目录页；当前值 15）。回归测试 `html_toc_page_kept_in_spine_not_stripped_as_redundant`（`optimize/tests.rs`：构造带真实 `content.opf` + 94 链目录页的书，断言 spine `idref` 不被删）。`convert/mobi.rs` 头部一段过时注释一并改成实话：MOBI 转换产物现在与原生 EPUB 一样，目录页原样留在 spine。
- **真机复现 + 验证**：先对拉下来的原始字节跑修复前代码，坐实 `idref="toc-html"` 会消失；修复后本地确认还在；再走真机 HTTP API 全链路——`POST /staging/optimize` 异步跑完、`GET /staging` 显示"清洗+优化，100 章，786388→787085 字节，删空页 1"（不再是"删空页 2"，第二个"空页"就是被误判的目录页），`ssh` 拉真实产物 `content.opf` 确认 `idref="toc-html"` 在。
- **⚠ 这一轮并没解决用户的真实问题**：用户要的是原生目录面板入口，见 §03az → §03bc。

**部署**：`book-serve`/`gateway` 交叉编译，按标准流程（备份 + md5 + restart + 健康检查）走过。

**验证副作用**：真机验证在设备上留下**两份「疯探」**（用户此前落的原始版 `a42ac671-…` 缺目录页；新的 `4de57414-…` 是修复后版）。原生书库删除不自动执行——重复留给用户处理，与 §03ax 的"14 份重复《火影忍者》"同类。

### 03az｜第五轮真机反馈：《疯探》"没有 TOC"根因是 dtb:uid 不匹配（真机对照《雪人》坐实），《雪人》暴露"已有目录不拆两级"的范围问题（2026-09-19，真机通/待决策）

> ⚠ **标题里的"根因是 dtb:uid"已被 §03bb/§03bc 证伪**：`dtb:uid` 修一致后《疯探》目录入口仍不出现，真根因是 manifest 里 NCX 条目的 id 不叫 `ncx`（§03bc）。`dtb:uid` 同步仍保留（EPUB2 规范要求两者一致）；本节的《雪人》两级目录问题独立成立（后续由 §03ba 的 `restructure_existing_toc_parts` 处理，`wash/toc.rs`、`wash/mod.rs` 调用）。排查链总览见本章坑位表（二）之后。

用户看完 §03ay 后追问两条真书：《雪人》"01 雪人 不在第二层级"、《疯探》"没有 TOC"。先问清指的是原生目录面板（点按钮弹出的章节列表），而非书内翻页目录页；《疯探》是"根本没有目录按钮/入口"，不是面板空。

**当时的（错误）归因**：拉两本书真实字节对照，`toc.ncx` 的 `navMap` 都完整（《疯探》94 条全部可达），差异在 `<meta name="dtb:uid">`——《雪人》的 `www.haodoo.net` 与 OPF `dc:identifier` 一致；《疯探》的 `dtb:uid`（`urn:uuid:5ddc1173-…`）与 `dc:identifier`（`urn:uuid:72b00c5a-…`）不一致（EPUB2 规范要求一致，是生成器自身 bug），当时据此判定"原生面板遇到不一致就不显示入口"。
**漏看的差异**：《雪人》的 manifest id 恰好是 `id="ncx"`、`toc.ncx` 也没有 DOCTYPE——对照两本书就有三处差异，只挑了最显眼的一项。

**留下来的有效修复**：`wash::opf_unique_identifier`（解析 `unique-identifier` 指向的 `dc:identifier`）+ `wash::fix_ncx_uid`（幂等，真不一致才改，`wash/ncx_fix.rs`），`wash_entries` 无条件跑；同时发现我们自己 `build_ncx()` 的 `dtb:uid` 硬编码 `"cj-wash"`，一并改成接真实标识符（`wash/toc.rs::auto_toc` 传入 uid）。`OPTIMIZE_VERSION` 11→12；回归测试 `existing_ncx_dtb_uid_synced_to_opf_identifier`（含"已一致不误报"）。真机把产物 `dtb:uid` 同步后，**字节正确但原生面板未出现**（旧结论至此已不成立）；为验证又投了第三份《疯探》（`dc6ebf2b-…`），重复副本留给清理待办。

**《雪人》"01 雪人 不在第二层级"是另一件事**：这本书（好读书櫃/haodoo.net 生成）自带 `toc.ncx`，`toc_entry_count()>0` 触发 `AutoToc::IfMissing` 的"已有目录不动"分支——保留原样是对的。问题是自带 ncx 本身**扁平**（`dtb:depth=1`），条目形如"第一部　01　雪人"（首条：部+编号+书名）、"　02　卵石眼"（后续条目只有编号+章名）；章节 HTML 也不规整：

| 章 | 章节 HTML 形态 | 对现有 `split_numbered_title` 的影响 |
|---|---|---|
| 第 1 章 | 唯一 `<h3>` 是书名；"第一部""01　雪人"只是普通 `<p>` | 无标题可拆 |
| 第 2–9 章 | 有干净 `<h3>02　卵石眼</h3>`，但编号在标题**前面** | 现有函数只认"标题 + 编号在后"（如"第一章 1"），顺序相反，不命中 |
| 第 10 章（第二部第一章） | `<h3>` 只有"第二部"，"10　粉筆"降级成普通 `<p>` | 部标题与章名分离 |

**结论**：即使做"已有目录也拆两级"，也要新写一套专门识别"部/编号/章名"三段式 + "部只在每部第一章出现"惯例的启发式，不能复用 `split_numbered_title`；这类排版惯例在不同书源间差异很大，通用化有对别的书拆错的风险。当时留给用户决定（见 §05 待办）。

### 03bb｜《疯探》目录入口深度排查：dtb:uid/DOCTYPE 都不是根本原因，反编译式二分定位到 content.opf，未能锁定最终触发点（2026-09-19，真机通，⚠未解决）

> ⚠ **已被 §03bc 取代**：本节停在"问题在 `content.opf` 但没锁定字段"，§03bc 反编译后坐实（manifest 里 NCX 的 id 必须叫 `ncx`）。本节保留的价值是**方法**：`.epubindex` 判据 + 单点替换二分，以及"二分为什么漏掉了真因"。

用户第三次反馈"三个横杆"（原生目录入口）仍没有，且已排除副本混淆（测的就是 DOCTYPE 修复后的最新那份）。dtb:uid（§03az）+ DOCTYPE（§03ba）都修完仍不出现——两处修复本身合理（都是真实的规范/健壮性问题），**但都不是《疯探》失败的原因**，这是本轮最重要的认知修正。

**方法论转向：不再猜"哪个字段有问题"，改对 xochitl 自己生成的内部索引 `<uuid>.epubindex` 做二进制逆向**。这是带 `rM epub index` 魔数头的私有格式，逆出的大致结构：`[UTF-16BE 长度前缀][UTF-16BE 字符串]` 重复的变长记录流，每个 spine 页一条"路径记录"，其后若紧跟"标题记录"说明该页提取到了标题。对照《疯探》（`07b33e06`）与《雪人》（真机目录正常）：**《疯探》94 个章节的标题位全部为空——不是内容错，是压根没写**；《雪人》每条后面都跟着从 `toc.ncx` 提取的标题。**这就是可测的判据**：xochitl 索引生成器在《疯探》上标题提取整体失败，与我们产物的 `toc.ncx`/`dtb:uid`/DOCTYPE 是否合规无关。

**真机二分**（每步造一本裁剪/修改过的书，走真实 `POST /staging/optimize` + `/staging/deliver` 投到设备，拉新文档的 `.epubindex` 看标题是否写入，20+ 轮）：

| 步 | 变量 | 结果 |
|---|---|---|
| 1 | 平铺路径 vs 子目录路径（`c1.xhtml` vs `text/c1.xhtml`） | 都能提取，排除 |
| 2 | 章节规模（3 章 / 100 章清洗内容） | 都能提取，排除"大书超时/截断" |
| 3 | spine 含不在 `navMap` 里的前置页 | 能提取，排除 |
| 4 | 真书 94 章原文 + 真前置页/封面/CSS，用 Python zipfile **自己重新打包**（脱离生成器，内容逐字节抄自真书，786KB） | **成功提取**——体积、章数、图片、CSS 都排除 |
| 5 | 把步骤 4 成功版的 `content.opf` 换成真书 `content.opf` 字节，其余不变 | **复现失败**（索引 7240 字节，与真机失败那份大小一致）→ **问题在 `content.opf` 本身** |
| 6 | 在成功版里逐个单独加入：真实长 `dc:description`、`linear="no"` 非线性 spine 项（`end-copyright`）、`toc.ncx` 在 manifest 里的声明顺序与 zip 物理位置不一致 | 三次全部仍成功，都不是单独触发点 |

**停在这里**：定位到 `content.opf` 但锁不定字段；继续下去要反编译 xochitl（无源码），当时判断投入产出比不合理，向用户汇报现状请其决定是继续深挖还是接受为个例。

**为什么漏了**：步骤 6 只改了 `content.opf` 的*内容*字段，没有测*id 命名*——见 §03bc。

**顺带清理**：排查产生的 20 份测试文档（`AB*`/`疯探*`）已用 `POST /trash/add` 清理（均为本会话自建、有完整 uuid 记录）；设备上只留 `07b33e06`。

### 03bc｜《疯探》目录入口真正根因：反编译 xochitl 二进制坐实——硬编码死查 manifest `id="ncx"`，不走 `<spine toc="IDREF">`（2026-09-19，真机通，✅已解决）

用户明确表态"继续投入去反编译"，遂对真机拉回的 `/usr/bin/xochitl`（ARM64、Qt6/C++、stripped）做反编译。

![xochitl 如何查找 NCX，以及 wash_entries 的对应修复](diagrams/xochitl-ncx-lookup.svg)

**工具链**（可复用于设备上其它闭源可执行文件）：Ghidra 12.1 已砍 Jython 支持，headless `-postScript` 要求 PyGhidra（需 `jep` 原生库，环境没有），装 Jython 扩展也救不回（`PyGhidraScriptProvider` 仍抢注 `.py`）。改用 PyGhidra 官方 Python API（`pyghidra.start()` 用 `JPype` 拉起 JVM，不依赖 `jep`）离线跑一遍完整自动分析（237 秒，AARCH64 ELF、23.6MB），再在同一 Python 会话里反复按地址反编译、查调用者/被调者。

**定位过程**：用 `strings`（**含 UTF-16LE**——Qt `QStringLiteral` 常量按 UTF-16 编译，默认 8-bit 扫描会漏）找到一批 EPUB 解析调试字符串（`Cannot find <navMap> element`、`Found metadata for Epub:`、`Invalid spine item at line` 等），顺藤摸到 xochitl 内部 EPUB→PDF 转换/索引管线（`epubcontext.cpp`，函数名已 strip，用 `FUN_xxxxxx` 占位）：`container.xml` 找 `content.opf` → 解析 OPF（metadata/manifest/spine/guide，全程宽松，单条 `<itemref>`/`<guide><reference>` 失败只打 warning）→ 生成 PDF 的同时**遍历 `toc.ncx` 的 navMap 构建标题索引**。给 navMap 找 `toc.ncx` 的反编译代码里直接挖出**硬编码的 3 字符字面量 `"ncx"`**，用它去哈希查 **manifest 的 `id` 表**；只有查不到才退回读 `<spine toc="IDREF">`（后备分支实测没能救回《疯探》，原因未继续深挖——已足够定位并验证根因）。

**根因**：《疯探》的 NCX 条目写的是完全合规的 `<item id="toc" href="toc.ncx" …/>` + `<spine toc="toc">`（EPUB2 只要求 `spine` 的 `toc` 属性指对，从没规定 manifest id 叫什么），但 id 不是字面量 `"ncx"`，xochitl 的硬编码查找找不到，navMap 标题提取整体失败，原生目录入口消失；书本身照常翻页（渲染走另一条不依赖这个 id 的路径，与索引构建是两条独立代码路径）。§03az 的《雪人》夹具 manifest id 恰好是 `ncx`（Calibre 等主流工具的默认命名），纯属撞对了约定俗成。

**验证**（从窄到宽）：
1. **最小精确验证**：真书 `content.opf` 原封不动，只改一行——`id="toc"`→`id="ncx"`、`<spine toc="toc">`→`<spine toc="ncx">`，真机投递：`.epubindex` 7188→15558 字节，94 条标题全部正确（"第二章 影音室""第三章 切入点"……逐条核对）。
2. **端到端**：`fix_ncx_manifest_id` 写进 `bookconv::wash`（`wash/ncx_fix.rs`）：找 manifest 里 `media-type="application/x-dtbncx+xml"` 的 `<item>`，id 不是 `ncx` 就改成 `ncx`，同步 `<spine toc="…">`；已叫 `ncx` 或改了会与别的条目 id 冲突时原样不动（幂等）。交叉编译部署真机 `book-serve`（备份旧二进制、md5 核对、`systemctl restart` 后 `is-active=active`/`NRestarts=0`），走真实 `POST /staging/optimize` + `/staging/deliver`（非手工 patch）重处理同一本《疯探》——回执带"修复目录条目标识符"，拉回的 `.epubindex` 同样 15558 字节、标题全对，与第 1 步逐字节一致。回归测试 `ncx_manifest_id_renamed_to_ncx_and_spine_toc_synced`（含"已叫 ncx 不误报"）。

**同源的第二处活 bug**：`wash::auto_toc`（书完全没有目录、从标题自动生成 `toc.ncx` 的分支）自己插入的 manifest 条目曾写 `id="cj-ncx"`（当初图个"标出是我们插的"，没想到这个 id 字符串正是 xochitl 硬编码查找的 key）——任何触发自动目录的书原生入口都会因此不出现，只是没被用户撞见。已改为 `id="ncx"`，不依赖 `fix_ncx_manifest_id` 二次修正（`wash/toc.rs`，注释已说明原因）。

**结论**：dtb:uid（§03az）与 DOCTYPE（§03ba）两处修复保留——都是真实的规范/健壮性问题，但不是《疯探》的根因；manifest id 硬编码查找才是。`OPTIMIZE_VERSION` 13→14（当时值；当前 15，v15 是 EPUB 漫画补白比例，与本链无关，`optimize/mod.rs:55`）。`wash_entries` 里现行顺序：`fix_ncx_manifest_id` → `restructure_existing_toc_parts` → `auto_toc` → `fix_ncx_uid` → `strip_ncx_doctype`。旧书需 `force:true` 重优化才能拿到修复。

### 03bg｜优化也补真实分步进度（不是漫画专属），顺带确认这不能"实质省内存"（2026-09-19，真机通，✅已解决）

用户追问"是不是只有判断为漫画的才流式处理、文字书不流式，所以进度条一直不动"——**前提不对，结论对**。

- **前提为何不对**：`Staging::optimize()` 对所有 EPUB 无条件走 `optimize_epub_file_streaming`（`staging/optimizing.rs`），漫画判定 `comic_detect::is_comic` 只是流式函数内部"图片按哪档画质处理"的开关，不是"要不要流式"的开关；流式是 §03ba 修 552MB《镖人》OOM 时改的，与进度条无关。
- **真正原因**：`OptimizeCheck` 压根没有 `progress` 字段——不管流不流式、是不是漫画，优化都是一次不透明调用，中途没有回调往外报进度。唯一有真实分步进度的是"超限漫画按卷拆分落库"（`DeliverCheck.progress`，§03be），那是**投递阶段**机制，与**优化阶段**是两回事。

**"能不能实质省内存"——不能**（置信度：高，基于读代码）：§03ba 的内存瓶颈是图片，峰值已压到"一张图 + 全书文字部分"，图片本就逐张读-处理-写-丢。进度回调只是已有逐条目循环里多一次通知，不改变任何数据存活时长/份数，与内存优化正交。真要再压，瓶颈会是阶段一整份读进内存的全书非图片条目（文字/CSS/字体），但体积通常很小，且不是 552MB OOM 真机实测坐实的瓶颈；为一个从未真机验证的理论瓶颈搭更复杂的真流水线架构，这次没做。

**实现**：
- `OptimizeCheck` 新增 `progress: Option<StepProgress>`（`DeliverProgress` 顺手改名 `StepProgress`，两者共用 `{done,total}`，`sidecar.rs`）。
- `optimize_epub_file_streaming` 新增 `on_progress(done, total)` 回调：阶段二（真正耗时的逐条目写出）每写完一条目回调一次，`total` = 这本书要写出的条目总数（`optimize/streaming.rs`）。
- `Staging::optimize()` 原样转发回调（不耦合 sidecar/EventBus，同步调用方传空闭包）。
- `spawn_optimize` 的后台线程里加节流：`OPTIMIZE_PROGRESS_STRIDE = 5`，每 5 条目才落一次盘/推一次 SSE，首尾两条永远落——大漫画阶段二可能几百个条目，每条目一次原子写（开临时文件+写+改名）+ SSE 广播在这台设备存储上是真实开销。
- 网页进度条从只认 `dc.progress` 扩成 `dc.progress`/`oc.progress` 都认。

**真机端到端**：用户原始《飘·上册》EPUB（10.6MB，35 章）走 `/staging/optimize`，轮询 `GET /staging` 观察到进度 `36/56` → `41/56` → `46/56` 推进，最终 `status:"ok"`——拿到真实中间态数字，证明节流 + SSE + sidecar 落盘全链路通。测试产物用完即删；`bookconv` 有流式进度单测（done 严格递增、最后一次 done==total）。

### 03bo｜封面声明规则与无损补封面工具（2026-09-20，真机对照实验）

> 完整实验记录见 `bookconv优化白皮书.md` 末尾「封面：真机对照实验」小节。

**现象**：设备上部分书没有封面缩略图，xochitl 日志有 `failed extracting cover: got null cover image`。

**根因实验**（同一张真封面图造 3 个最小 EPUB 上传设备，只改封面声明写法）：

| 封面声明写法 | 结果 |
|---|---|
| 封面条目 id 带点（如 `x00000001.jpg`）+ 仅 `<meta name="cover">` | 取不到封面 |
| 同 id + 再加 `properties="cover-image"` | 正常 |
| id 简单（`cover`）+ 仅 meta | 正常 |

**规则**（`wash::ensure_cover_declared`，`wash/cover.rs`）：
- OPF 里 **meta 和 `properties="cover-image"` 必须同时有**（只有 meta 也要补属性）。
- 必须在清洗**之前**调用（清洗会删只含 SVG 封面的 titlepage；调用点 `optimize/mod.rs::prepare_entries`）。
- 封面文件本身若不是图片（例如 239 字节文本残片），兜底取前 12 个 spine 页里第一张真实图。
- 占位 EPUB（§03bn）找封面时同样校验是图片（`placeholder.rs::find_cover`）。

**工具**：`cover-fix in.epub out.epub [cover.png]`（`bookconv` 的 bin，`bin/cover_fix.rs`）——只改 OPF，其余条目 raw copy（图片零重编码），并按 xochitl 规格（552×981 RGB PNG 白底居中）生成封面缩略图；已对设备上 3 本已投的书原地修补。已投的旧书不会自动补封面（重新优化会多一代 JPEG 有损，所以没自动做）。

## 第 C 章 落库、大文件与漫画

> **大白话导语**
> - **落库**＝把母版库里的书真正放进某个阅读器：「加入 xochitl」（reMarkable 官方阅读器/UI 进程）走它的网页 `/upload` 接口，「加入 KOReader」直接把文件复制进 KOReader 的书目录。**母版库**是书架里永久保存原书的暂存池，落库不会动它。
> - 这章回答四个问题：书怎么进 xochitl（含超过 100MB 的大文件）、文件夹怎么建、落完怎么知道"渲染没坏"、漫画有哪些特殊处理。
> - 建议先看下面的现状结论和决策树，再按需查具体小节；§03t、§03bk 是被推翻的历史，§03d 的命令行部分已砍，只读结论即可。
> - 名词：**qmd**＝对 xochitl QML 界面的补丁（注入运行中的 xochitl，能调它自己的内部函数）；**SSE**＝服务端事件推送（网页不轮询，服务端有变化就推）；**边车**＝书旁边的隐藏小文件，记这本书的附加状态。

> **现状结论**
> - **加入 xochitl**（`book-serve` 的 `Staging::deliver`）：≤ `nativeUploadLimitMb`（缺省 90，即 94,371,840 字节）走流式 `/upload` + 渲染自检；超限**优先走"占位 + 磁盘替换"**（先传几 KB 占位、再在设备上原子替换成真文件，不分卷，安全上限 1GiB，§03bn）；只有本机没有书库目录、超过 1GiB 或造不出占位时才回退"按卷拆分"（仅漫画 EPUB / 带书签漫画 PDF）或拒绝。
> - **为什么是 90**：2026-09-19 真机二分法测出 xochitl `/upload` 的硬上限是 **100,000,000 字节**（≥ 99,999,900 起 `HTTP 413`）；此前的 150MB 是从没验证过的猜测值。`0` = 不设体积门。
> - 文件夹留空＝书库根；填了不存在的名字，经 `MkdirQueue` 排队、由 qmd 代理调 `Library.createCollection` 真建出来（最多等 20 秒）；文件夹名带 `/` 合法（§03bf）。
> - **加入 KOReader**：`koreader-serve` 的 `POST /books/adopt` 把母版本地复制进 KOReader 书目录（先写 `.part` 再 rename），不经优化器、不限体积；同一份母版字节两边可对照。
> - 落库记录写在边车 `.<书名>.delivered`；渲染自检徽章两条路径都会记（PDF 直接 ok；大文件 EPUB 首次打开后页数一变自动显示真页数）。
> - 漫画：2026-09-19 一度改产 PDF（§03bk），2026-09-20 用户拍板换回 EPUB；PDF 相关代码仅服务"超限 PDF 分卷"。
> - 电脑端 `shelf` 命令行（`push`/`koreader pull|diff|sync`/`doctor --render`/`--wait`）2026-09-18 整体砍除，下文凡提到它们的都是历史记录；KOReader 配置同步现走 `koreader-serve` 的 `/config/*`。

![落库决策树：≤90MB 流式上传 / >90MB 占位替换 / 兜底按卷拆分 / KOReader 本地复制](diagrams/deliver-decision.svg)

#### 本章坑位表

| 坑 | 症状/判据 | 根因 | 教训/规避 | 见§ |
|---|---|---|---|---|
| 传书 WiFi"卡死"，传字体不卡 | 传 5 本 <1MB 书必卡、5 个 25MB 字体不卡 | xochitl 每导入一本书就把它和渲染缓存（约 2–3MB/本）同步到 reMarkable 云，出站突发占满弱热点上行 | 大书走 USB（`https://10.11.99.1`）；分清哪端掉：设备 ping host 不丢包＝设备无辜 | §03l |
| 整章渲染失败无人察觉 | 书只出 7 页，用户翻到才发现 | 同一标签双 id → 严格 XML 吞整章 | 落库后自动对比"实际页数/期望页数"，< 50% 报 warn | §03aa |
| 外部进程改 `.metadata` / 建文件夹 | 改了被运行中 xochitl 内存态覆写回来 | xochitl 没有对外的建夹/删除通道 | 只能走 xochitl 自己的代码路（qmd 注入调 `Library.createCollection` / `selectionMoveToTrash`） | §03aa · §03be |
| "静态调用链一致"≠真机通 | 反编译推出的调用链，没人真点过 | 只做了离线验证 | 结论止于"反编译一致"就如实标注，等真实业务场景验证 | §03aa · §03be |
| 注入代理后端消失 | 旧版 qmd 轮询已下线的接口 | 后端删了、真机 qmd 还在跑 | 设计成"非 200 就静默返回"，后端消失自然失活；砍功能前先想这一点 | §03aa · §03be |
| 进度条不动，要手动刷新 | 拆分落库时进度数字冻结 | 写了边车但没 `bus.publish`，SSE 零轮询机制不知道有变化 | 状态落盘后必须紧跟一次事件推送 | §03be |
| 带斜杠的文件夹名建不出 | 《乱马1/2》每次都落回书库根 | `MkdirQueue::add` 拦了含 `/` 的名字，`ensure_folder` 又对 Err 静默放弃 | 校验要有技术依据（这个名字只当 JSON 字符串走）；"机制没问题"≠"这次输入一定成功"，拿用户的具体失败样本复现 | §03bf |
| 上传 96MB 单卷反复断连 | `Connection reset by peer` / `Broken pipe` | 上限是 100,000,000 字节而非猜的 150MB，96MB 落在"看着安全实际会炸"的区间 | 上限要真机二分实测；缺省取 90MB 留余量 | §03bn |
| 连续传多份分卷冲垮 xochitl | 每份卷传完下一份上传被同一卷打断 | xochitl 忙着渲染+缩略图+建索引（每卷 15–30 秒） | 传完一份等它真渲染出页数再传下一份（每份限时 90 秒，超时不算失败） | §03ax |
| 拆出的漫画卷四周留白 | 原生阅读器里图片只占页面一部分，底部空 90pt | 拆分包没有 CSS：默认边距没清零、图片没撑满 | 外链 `comic.css`；`height` 类 CSS xochitl 一律不认，只能补白图片像素 | §03ax |
| 大文件占位替换后名字/封面不对 | 显示成占位的名字、没封面 | xochitl 用占位 EPUB 的 `dc:title` 当显示名、按占位封面生成缩略图，替换后不补生成 | 占位必须带真书名和真封面 | §03bn |
| 漫画灰阶抖动后体积不降反涨 | 283MB→687MB（镖人），171MB→229MB（阿拉蕾①） | Floyd–Steinberg 抖动后位图是高熵噪点，Flate 压不动；再叠加跨页拆分/放大增加总像素 | 降灰阶≠省体积（省的是刷新闪烁） | §03t · §03ad |
| 用文档缓存验目录 | 渲染缓存 `<uuid>.pdf` 里书签 0 条 | xochitl 渲染缓存从不带书签 | 目录要从 EPUB 的 ncx/nav 验，不能从缓存验 | §03aa |
| `xovi/start` 跑完看着成功其实没挂上 | `Job for xochitl.service canceled`，`xochitl.service.d/` 是空的 | vellum tmpfs 持久化偶发不稳 | 跑完核对新 PID 的 `LD_PRELOAD`/`XOVI_ROOT`，不能只看 `is-active`；⚠ xovi 已生效时不要再跑它（见工程纪律） | §03aa |

### 03d｜Phase 3 KOReader 配置即代码（2026-09-03，离线完成）

> **现状（2026-09-22 核对）**：设计与端点仍是现役；文中 `shelf koreader pull/diff/sync` 命令行已随 host CLI 砍除（附录 B），现在手工调 `koreader-serve` 的接口。`profile/` 三个补丁文件里的值后来已按 2026-09-03 真机快照校正过（`shelf/koreader/README.md`），"待 pull 核对"的说法过时。

**设计**：Lua 处理不在 host 造轮子——`shelf/koreader/merge.lua`（Lua 5.1，`include_str!` 进 `koreader-serve`）由 KOReader 自带 `luajit` 跑：标量覆盖、表递归、`"__DELETE__"` 删键；`--dry-run` 只出差异；写＝先 `.tmp` 再 rename，头行保留 KOReader 惯例注释；输出 JSON `{changes:[{path,old,new}],written}`。

**执行流程**（Template Method，`ConfigSync::apply`，`koreader-serve/src/config.rs`）：

| 步骤 | 做什么 |
|---|---|
| ensure_stopped | 扫 `/proc` 找 KOReader 进程，运行中直接 409（它退出时会回写配置覆盖你的改动）；`dry_run` 不受限 |
| backup | `~/.local/state/shelf/koreader-backups/<文件>.bak.pre-shelf-<时间戳>` |
| stage | 补丁与 `merge.lua` 落 `$XDG_RUNTIME_DIR/shelf/koreader/` |
| merge | `luajit merge.lua <目标> <补丁> [--dry-run]` |
| verify | `dofile` 回读；失败自动从备份还原 |

**端点**：`GET|POST /config/{settings|defaults|gestures}[?dry_run=1]`（GET 读设备原文；POST 的 body＝补丁 Lua 文本；三个名字对应 `settings.reader.lua`、`defaults.custom.lua`、`settings/gestures.lua`）、`GET|POST /dicts`（StarDict 多文件落 `data/dict/<name>/`，POST 要 `?name=`）、`GET|POST /fonts`；fonts/dicts 共用 `receive_into`。

**profile**（`shelf/koreader/profile/`）：`settings.reader.patch.lua`（脚注弹窗/滑动返回/放大链接命中区、`wf_level=1`、`full_refresh_count=16`、`avoid_flashing_ui`、`color_rendering=false`、悬挂标点、`footer` 收紧）、`defaults.custom.lua`（DTAP_ZONE 死区）、`gestures.patch.lua`（长按左上角退出）。

**验证**：merge.lua 本机 luajit：dry-run 不写、写入后回读、二次幂等零差异、删键/新表/嵌套均通过；`ConfigSync` 单测（host luajit）通过。**真机待验**（原文未见后续记录）：运行中 sync 被拒；退出后 diff→sync→回读→启 KOReader 逐项勾（脚注弹窗/滑动返回/无锯齿/死区/退出手势/footer）；二次 sync 零 diff；字体/词典同步可用。

### 03l｜"传书就卡、传字体不卡"根因＝reMarkable 云同步（2026-09-04，用户报 + 真机复现坐实）

**症状**：用户电脑浏览器传 5 个 25MB+ 字体一个不卡，传 5 本 <1MB 书必卡（原话"WiFi 断一会儿/卡死"）。字体更大反而不卡，直接否定"吞吐/网卡"。

**排除**：① 纯 CPU 打满两核（load 1.89）设备 ping host **0 丢包**——不是 CPU 饿死 WiFi；② 走 spool 直接喂 book-serve（不过 WiFi 传输）不卡；③ 1.2KB 探针书经 WiFi 灌 5 本每本 0.5s 返回不卡（探针太小、xochitl 几乎不渲染/同步，复现不出）。

**坐实**（用户真机复现 + 设备侧每秒读 `/sys/class/net/wlan0/statistics/{rx,tx}_bytes` 差值 + `/proc/net/tcp` 数远端 `:01BB`(443) ESTABLISHED）：每本书的模式＝**入站 rx ~100–460KB（上传书）→ 紧接出站 tx ~800–1900 KB/s 持续 2–3s + cloud443 由 1 变 2**。即 **xochitl 每导入一本书就立刻把它（连同渲染好的页面缓存，约 2–3MB/本）同步到 reMarkable 云**（设备 `xochitl.conf` 有 `UserToken`/`devicetoken`，scope 含 `sync:fox`）。这股**出站突发**在 host 弱热点（Intel AX 网卡跑 AP、2.4G ch6、干净传 40MB 需 67s≈5Mbit/s）的上行被占满几秒，期间网络"卡一会儿"；书更大/更多时云同步排队更久＝用户说的"卡死"。ping 基本不断、load<1——**设备自身 WiFi 与 shelf 都无辜**。

**为什么字体不卡**：字体不是文档，xochitl 不同步 → 只有入站、没有出站突发，哪怕 25MB 也顺。

**修法（都不改 shelf 代码）**：① **传书走 USB**（`https://10.11.99.1`，lo 别名常驻；2026-09-10 起网关绑标准 443，不用带端口号，见 §03am）——上传不过 WiFi，云同步在后台慢慢传不影响操作，**推荐**；② 换真路由器（上行宽，突发不明显）；③ 退出账号/关同步（影响所有云功能，不建议为此关）。

**验证（2026-09-04）**：用户改走 USB 传书顺畅、不卡——坐实"卡在 WiFi 无线路径 + 云同步出站"，非 shelf/设备 bug。**结论：大书上传优先 USB**；WiFi 传书本身能完成，只是每本书云同步那几秒会占满弱热点上行。**诊断法可复用**：设备 ping host 不丢＝设备无辜，再看 host AP 网卡 + 设备出站 tx 突发是否＝云同步。。

### 03t｜漫画通道：AZW3 漫画 → CBZ + 固定版式 PDF（2026-09-05，用户"AZW3 漫画该怎么转 / 镖人为何投不了原生"）

> **⚠ 已被取代**：本节"CBZ + 固定版式 PDF""漫画不投原生 / 只出 CBZ"是 2026-09-05 的结论；CBZ 通道随 host 整体砍除（2026-09-18，附录 B，BOOK_EXTS 现只剩 epub/pdf），漫画现在是 **EPUB**，超限时优先走大文件占位通道（§03bn）。下面只留决策脉络与仍有效的教训。

- **起因**：《镖人》AZW3 被当文字书走 Calibre 洗书路 → 282MB EPUB（2637 文件 / 2473 图）→ 点「投入原生书库」两次，xochitl 日志 `HttpRequest: expected multipart body is too large`——撞的是 xochitl `/upload` 体积上限（当时测得 60～285MB 间 413；精确值后来测出是 100,000,000 字节，见 §03bn），不是我们的格式门。
- **当时的三步**：① 探针判漫画（PalmDB 数 JPEG/PNG 魔数记录，图片字节 ≥60% 且 ≥20 张；EPUB 按 spine 数 `<img>` 与可见文字，图 ≥20 且每图配字 <40——这套判据后来移植成 `bookconv::comic_detect`，`MIN_IMAGES=20`、`TEXT_PER_IMAGE=40.0`，现役）；② 产 CBZ + 固定版式 PDF（每页 954×1696）；③ 当天晚些收口：**投原生的整条（`cbz2pdf` 产物、`to-pdf` 端点、网页按钮）全部删除**，漫画只做 CBZ → 「加入 KOReader」。收口理由：xochitl 没有固定页漫画体验、整本几百 MB 撞上限、分卷读漫画割裂。
- **仍有效的教训**：
  - 首版"整页只有一张图的页占比"判据在 Calibre 洗过的 EPUB 上失败（Calibre 把十几张图塞进一个 xhtml，146 页 spine 只 24 页单图）→ 改成"图/字比"后 AZW3 与洗后 EPUB 都命中（《镖人》AZW3 2498 图占 99.96%；洗过的 EPUB 2473 图 / 5527 字）。
  - **1-bit 抖动不省体积**：《镖人》`--mono` 只从 297MB 缩到 223MB（3/4 而非预想的 1/8）——Floyd–Steinberg 抖动后的位图是高熵噪点，Flate 压不动（927×1327 1-bit 裸 154KB，压后仍 ~90KB，只比 120KB 的 JPEG 小 1/4）。省体积要换 CCITT G4/JBIG2，没做。
  - `imgopt` 改成"只读头取尺寸、达标页零解码"，《镖人》2473 页 host 2m36s → 1m49s（产物字节相同），这一条提速保留至今。
- **§03t 收尾的文档事故**：文档脚本用 `**未闭环**：` 当切片锚点，它出现两次，切掉了 §00b 尾～§05 整段并随提交入库，靠 `898f2fd` 恢复。**教训：切片锚点必须先 `count()==1`。**
- 同批核查里定下的体积门（当时缺省 150MB）已被 §03bn 推翻（现缺省 90MB + 占位通道），`status` 仍带 `nativeUploadLimitBytes`，但网页已不再据此灰掉按钮。

### 03aa｜投原生后的渲染自检（2026-09-06，3.28.0.172 真机）

> **现状（2026-09-22 核对）**：渲染自检、`shelf-trash-agent.qmd`、`mkdir.rs` + `shelf-mkdir-agent.qmd` 都是现役。本节里的 `shelf doctor --render` / `shelf push --wait` / TXT 切章 / `--eink-gray` 属于已砍的 host CLI，只留结论与数据；"建文件夹代理 2026-09-15 被物理删除"已被 §03be 推翻（原样捞回复活）。`shelf-core` 现名 `rmsvc-core`（`rmsvc_core::xochitl`）。

#### 渲染自检

**问题**：整章渲染失败（同一标签双 id → 严格 XML 吞整章，《消失的爱人》只出 7 页；背景图盖正文）以前都是用户翻到才发现。

**依据（真机事实）**：xochitl 导入 EPUB 时**同步渲染**，`/upload` 返回时 `<uuid>.content` 已有 `pageCount`（人骨拼圖 523、Tell Me Your Dreams 352），旁边有 `<uuid>.pdf` 渲染缓存。所以自检只读一个 JSON 字段，设备端不用解析 PDF（设备也没有 pdfinfo/mutool/python3）。

**做法**（`book-serve/src/render_check.rs` + `rmsvc_core::xochitl::{find_documents_since,page_count}` + `bookconv::stats::text_profile_file`）：

1. 投书前流式算正文非空白字符数与 `dc:title`，期望页数 ＝ 字符数 ÷ 每页字符数（真机标定：中文 460 字/页、英文 960 字符/页，两本真书 0.99 吻合；常量 `CJK_CHARS_PER_PAGE`/`LATIN_CHARS_PER_PAGE`）。
2. `/upload` 不回 uuid、visibleName 取自 EPUB 元数据不等于文件名 → 按 `createdTime ≥ 投书时刻` 圈候选，书名（`dc:title` / 文件名 stem，忽略大小写）相符者优先，否则最新一本。
3. 立即探一次；没渲染完就 `fswatch::watch_until` **限时**监听书库目录（3 s 防抖 `DEBOUNCE`、最长 10 min `TIMEOUT`，结束即撤——不给书库留常驻 inotify，守 §03z 约束）。超时记 `timeout`（不是错误：设备上打开一次就渲染）。
4. 判定 `pages < expected × 50%`（`WARN_RATIO`）→ `warn`。阈值标定：自检发生在导入当下、xochitl 用缺省字号/边距，页数只随文字密度浮动；坏章在 xochitl 里各占 **1 页空白**，4 章坏 3 章的探针 10/29＝0.34，30% 抓不住，定 50%。
5. 结果写母版库边车 `.<name>.delivered` 的 `render`（事件有损、状态必须落盘）+ 推 `books/render {name,status,pages,expected}`；网页徽章「渲染 N 页」/「⚠ 只渲染 N 页」（红）/「渲染中…」/「未见渲染」，状态机 `pending → ok | warn | timeout`（大文件 EPUB 另有 `onopen`，见 §03bn）。

**真机**：Probe Good（4 章随机词）ok 25/29；Probe Bad（h1 双 id ×3 章）warn 10/29；事件四条当秒到达，边车落盘。探针书当时留在原生书库根目录由用户删。

**仍保留的取回接口**：`GET /staging/render/{uuid}` 返回 xochitl 渲染缓存 `<uuid>.pdf`（只认 uuid 形状、只读），用于"量排版"类诊断。

#### 已砍的 host 侧附带能力（只留结论）

| 能力（均已随 host CLI 砍除） | 留下的结论/数据 |
|---|---|
| `shelf doctor --render` 排版回归探针：造探针 EPUB → 落库 → 取回渲染缓存 → pymupdf 量首行缩进 | 3.28.0.172 缺省字号 12.05 pt：首段顶格 PFLUSH1/2/3＝0.0 pt，续段缩进 PINDENT1–4＝**14.27 pt＝1.184 em**（与 §03y 的 14.2 pt / 1.17 em 一致），7/7 PASS；CJK 段走回退字体、缩进同样由 css 给出。坑：pymupdf 把**同一视觉行拆成多段**（拉丁 EBGaramond 与 CJK 回退 KingHwaOldSongGJ 各成一 line，y0 差 2 pt），不按 y（<0.6 字号）合并碎片会量出 −62 pt |
| `shelf push --wait` | 设备离 USB 后几秒就自动休眠关 WiFi（§03w），最常见失败是"连不上"；探活用网关公开路由 `GET /health` |
| 中文 TXT 切章（`txt_to_epub.py`） | 样本《人骨拼圖》抽成 GB18030 TXT（38 章 24.8 万字）→ 投原生 531 页（自检期望 526，ok），正文缩进 24.1 pt＝2em。坑：TXT 开头常自带目录，会被切成一串空章 → 丢掉"无正文且标题在后面再次出现"的章。**xochitl 渲染缓存 PDF 从不带书签**（Tell Me Your Dreams 的也是 0 条），目录只能从 EPUB 的 ncx/nav 验。TXT 现已不收（格式只剩 EPUB/PDF） |
| 漫画省刷新档 `--eink-gray`（CBZ 16 级灰 + Floyd–Steinberg → 4-bit PNG，彩页按平均色度 ≥ 0.06 保色） | 依据"墨水屏波形按内容分档"（彩重 / 256 灰中 / ≤16 灰轻 / 1-bit 最轻）。《阿拉蕾（第 1 部）》171.2 MB → 108.2 MB（48 s）；用户目视 16 灰翻页明显少闪，曾定默认开。此 Python 实现已删；EPUB 侧的灰度/色度门槛在 `bookconv::imgopt` |

#### 原生回收站代理（`xovi/shelf-trash-agent.qmd` + `book-serve/src/trash.rs`）

**为什么要它**：外部进程直改 `.metadata` 的 `parent="trash"` 是判死路（运行中 xochitl 的内存文档模型会覆写回来）；唯一可靠的软删是 xochitl 自己的 `EntitySelection.selectionMoveToTrash()`。

**做法**：从 reading 线 `trash-agent.qmd` 剥离移植——注入 Sidebar 一个隐形 Item，挂当前文件夹文档模型 `entityListModel` 的 `rowsInserted`/`modelReset`（xochitl 自己在用的信号，纯事件驱动不轮询），4 s 防抖后 `GET 127.0.0.1:8790/trash/pending`，逐个 `selection.add` + `selectionMoveToTrash`；不在文件视图或用户正手动选中则跳过。`TrashQueue`（`$XDG_STATE_HOME/shelf/books/trash-pending.json`）：`POST /trash/add {uuid,name}` **入队时按 visibleName 核对 uuid**（错 uuid 就是错删别的书，拒绝），`GET /trash/pending` 顺手把已进回收站/已不存在的出队（QML 端无需 ack），`GET /trash` 列队列。

- **3.28 锚点核实**：从设备 `/usr/bin/xochitl` 解出 556 个 QML（`extract_qml.py`），Sidebar.qml 仍是 `#root > ColumnLayout#filterColumn`，`NavigationManager.activeContext.{explorer.entityListModel,selection}` 与 `selectionMoveToTrash` 都在；用设备 hashtab 离线 `apply-diffs` 一次通过。
- **真机**：xochitl 启动时不触发（进的是上次视图），投一本书 → `rowsInserted` → 5 s 内 `SHELF-TRASH: moved 2 to trash`，队列清空、`parent:"trash"` 由 xochitl 自己写入；名字守卫：同 uuid 配错名 400。
- ⚠ **当前无调用方**：唯一的入队方 `doctor --render` 已砍，网页没有触发点；队列与 qmd 保留待用（见"存疑"）。

#### 原生建文件夹代理（`xovi/shelf-mkdir-agent.qmd` + `book-serve/src/mkdir.rs`）

**缘起**：2026-09-07 为 note-serve 生成《书名》一章一本写的（`Xochitl::upload` 找不到目标文件夹只会 best-effort 落书库根，外部进程没有合法建夹通道）。2026-09-09 note-serve 改成复用书本自己的设备文件夹后没了消费方，2026-09-15 被当死代码物理删除，**2026-09-19 因网页「加入 xochitl → 文件夹」需求原样复活**，完整经过见 §03be。下面是当年反编译得出、至今仍是这套机制依据的设计记录。

**反编译得出的路径**（`extract_qml.py` 全量解出真机 QML，固件 md5 `952f1e28f…` 对拍）：

- `library-ui/window/create-collection` 对话框的确认按钮调 `root.library.createCollection(parentFolderId, name)`；`root.library` 在别处（如 `PageSelection{library:Library}`）被绑定为**裸全局单例 `Library`**（`import xofm.libs.library`；**跟 `LibraryController` 不是同一个对象，只有 `Library` 有 `createCollection`**）。
- 顶层（书库根）`parentFolderId` ＝ 空字符串（`currentFolderId` 声明处缺省值 `""`）。
- **锚点选 `MainView.qml`（`cardhw-notify.qmd` 用过的同一份）而不是 Sidebar**：Sidebar 没 `import xofm.libs.library`，要用得先加 `IMPORT`，而 qmldiff 的 `IMPORT` 强制要显式版本号、源文件是 Qt6 无版本 import，硬造版本号风险不可控；MainView 本来就有这两个 import，改动面更小。
- **触发只能轮询**：建夹必须先于 `/upload` 发生，没有 `rowsInserted` 那样的天然事件可等 → 8 s 一个 Timer 轮询 `GET 127.0.0.1:8790/mkdir/pending`（返回 `{names:[…]}`，量级同 cardhw 的 5 s/trash 的 4 s 防抖）。
- **后端**：`MkdirQueue`（跟 `trash.rs` 同款，共用 `pending_queue::PendingQueue<T>`）：`POST /mkdir/add {name}` 入队（已存在就不入队）、`GET /mkdir/pending` 拉取并顺手剔除已建出的名字、`GET /mkdir` 列队列。前端「＋新建文件夹」也直接调 `/api/books/mkdir/add`。

**真机（2026-09-07 当时）**：`journalctl` 见 qmd 被 `qmldiff` 加载、无解析错误；note-serve 生成新章节 → 队列即时出现《人骨拼圖》→ ~80 s 内 `SHELF-MKDIR: created 《人骨拼圖》`、书库真多出一个 `CollectionType` 文件夹；再生成同书，新文档 `parent` 正确指向该文件夹（建夹前落根目录的旧份原样留在根，设计内行为）；重复触发不产生重名文件夹；`NRestarts=0`。**当时没有点开新建文件夹对话框交叉验证**（会真建用户可见文件夹），结论止于"反编译 + 静态调用链一致 + 业务场景间接验证"，直到 §03be 才有第一次直接真机验证。

### 03ad｜漫画跨页拆分 + 白边裁切放大 + 小体积漫画可选投原生（2026-09-08）

> **⚠ 已被取代**：本节全部是 **host 侧 Python 管线**（`comic_gray.py`、`cbz2pdf` 薄 CLI、`push.py::comic_prepare`）的产物；host 整体已砍（2026-09-18），CBZ/`--comic-native` 路径不存在了，网页也不再产 CBZ。现役等价物只有 `bookconv::imgopt::trim_margins`（白边裁切）与 `bookconv::comic_split`（超限拆卷，§03ax）。跨页拆分与灰阶 CBZ 在 Rust 里**没有**对应实现。保留下来的是数据与判据。

**用户当时的两个问题**：① 漫画优化按屏幕缩放，"第 2 页内容裁到第 1 页"；② 想让不需要分卷的小体积漫画也能选择投原生。真机排查发现两件事分开修：东立扫描版《火影忍者》卷 1 的图片是**两页拼一张的扫描跨页图**（1674×1250，宽高比 1.34，中间有装订缝），旧管线整张缩放，症状就是"内容裁到别的页"；《阿拉蕾①》是正常一页一图，只是约 8% 白边没利用。

**当时的算法（判据留档）**：

| 环节 | 规则 |
|---|---|
| 跨页识别 | 宽高比 `w/h` > `SPREAD_RATIO_MIN=1.05` 才可能是跨页；在 35%–65% 宽度找内容密度最低的竖直窄带当装订缝（不假设正中间）；找不到干净缝时比例 ≥ `SPREAD_RATIO_CONFIDENT=1.3` 仍按猜的位置拆并标 `flagged`，比例不夸张就不拆；拆完两半仍偏宽（三联/扉页）放弃拆分、原图直出；默认从右往左排序（东亚漫画，`rtl=True`） |
| 白边裁切 | 四边向内扫内容包围盒（灰度 <245 算内容，行/列内容占比 >0.5% 才算有内容）；单轴裁掉 >`MAX_TRIM_FRAC=20%` 就放弃裁该轴，防满页留白分镜被误裁没；裁后允许放大回填屏幕，封顶 `MAX_UPSCALE=1.5` |
| 小体积投原生 | 灰阶 CBZ 体积 ×1.5 估算 PDF 体积（实测 PNG 页解码后走纯 zlib，约 1.45×），在体积门内才生成 PDF；超限只出 CBZ，绝不触发分卷 |

**真机样本（当时）**：《火影忍者》卷 1（96 页）全部识别成跨页、拆成 192 页；2 页无干净装订缝被标 `flagged`；封面/书脊/封底三联扫描（2819×1250）拆后仍偏横，是唯一不够干净的结果（未处理）。55.98MB 灰阶 CBZ → 80.97MB PDF，当时体积门（150MB）内直投原生成功，xochitl 解析出全部 192 页。

**"超限只出 CBZ"分支复验（2026-09-09 真机）**：

| 书 | 处理 | 体积变化 | 备注 |
|---|---|---|---|
| 阿拉蕾①（171.2MB AZW3，1092 页） | 跨页拆 0 页 / 16 灰 1085 页 / 保色 7 页 | 171.2 → **229.0MB** | 超过当时 150MB 门，只出 CBZ；全程 1m57s |
| 镖人（283.2MB AZW3，2473 页） | 跨页拆 **17 页** / 16 灰 2489 页 / 保色 1 页 | 283.2 → **686.9MB** | 体积不降反涨超一倍；17 处装订缝无干净切割点，按"最空一列"猜测拆分并标 `flagged`；全程 6m1s |

体积涨的根因与 §03t 相同：16 灰 Floyd–Steinberg 抖动位图是高熵噪点、Flate 压不动，叠加跨页拆分+放大后总像素数增加。母版库前后对照确认只多出两个 `.gray.cbz`，无 PDF、无分卷文件——"漫画路由跳过分卷循环"真机同样成立。

### 03ar｜koreader-serve 新增高亮/生词只读端点，绕开一个 SQLite 交叉编译坑（2026-09-16，host 侧真机通）

给笔记线（`notes/`）的 KOReader 高亮/生词回流功能（笔记线白皮书 §03al）打地基：本仓库只加两个**只读**端点，条目库的创建/合并逻辑不在这边。

| 端点 | 数据源 | 实现要点 |
|---|---|---|
| `GET /annotations` | `books/` 下每本书的 `<basename>.sdr/metadata.<ext>.lua`（KOReader 原生标注 sidecar，路径规则读它自己的 `docsettings.lua` 核实） | Rust 里不写 Lua 语法解析器，沿用 `config.rs` 同一策略：交给 KOReader 自带 `luajit` 跑 `shelf/koreader/annot.lua`（`dofile` 出真表、手写 JSON 序列化，逻辑抄自 `merge.lua` 的 `jval`） |
| `GET /vocabulary` | `settings/vocabulary_builder.sqlite3`（KOReader 内置生词本插件库；库不存在＝没用过该插件，返回空列表） | `sqlite_min.rs`：**手写零 C 依赖的纯 Rust 只读 SQLite 解析器**，只实现读表要的最小子集（文件头 + table b-tree interior/leaf + 溢出页 + record 变长编码） |

**为什么手写解析器（被判死的方案与死因）**：第一版用 `rusqlite`（bundled sqlite3 C 源码），host 编译/测试都过，但交叉编译到 `aarch64-unknown-linux-musl` 链接失败——`sqlite3.c` 调 `open64`/`stat64` 这类 glibc LFS64 符号，而本仓库交叉工具链是"`aarch64-linux-gnu-gcc`（glibc 头文件）编 C + `rust-lld` 链 musl"，这套组合对 `ring`（纯计算不碰 libc 文件 I/O）成立、对真要读文件的 SQLite 不成立；本机没有 musl 原生交叉 gcc。三个选项（`koreader-serve` 单独退到 gnu 动态链 / 手写只读解析器 / 装 musl 交叉工具链）与用户过后选手写。`rusqlite` 降级成**只在 host 测试用**的 dev-dependency，用它造真实 `.sqlite3` 当 fixture 差分测试手写解析器（单页小表 / `INTEGER PRIMARY KEY` 别名 / 3000 行强制 interior page / 长文本强制溢出页链 / 真实 `vocabulary`+`title` 两表 schema）；产物仍是全静态（`file` 确认 `statically linked`）。

**验证**：`annot`（3 测）+ `vocab`（2 测）+ `sqlite_min`（5 测）共 10 个新测试，当时 `cargo test -p koreader-serve` 14 个全绿；`shelf/build.sh`（host 构建+测试+aarch64-musl 交叉编译，CI 同路径）全过；host 侧真实端到端（真 `koreader-serve`、真 `luajit`、Python 现造的真 sqlite3 文件）通过。**当天设备恢复连接后补做真机验证**：拉真机 6 本书 `.sdr`（3 漫画 + 3 小说，`metadata.cbz.lua`/`metadata.epub.lua` 两种后缀）`annot.lua` 全部解析正确（含一条"纯书签无文字"标注被正确过滤）；**发现真 bug**：生词本路径读源码时想当然写成 `data/`，真机实测在 `settings/`（表结构/字段名本身是对的），已修；部署后真实划一条高亮 + 真实加一个生词，两次 `POST /koreader/import`（笔记线 `ink-serve` 的端点）都正确回流，现有条目库数据完好（细节见笔记线白皮书 §03al）。

### 03ax｜第三轮真机反馈：脚注返回浮标已有解、裁边真机核实无误、超限漫画按卷拆分投原生（2026-09-18，真机通）

> **⚠ 部分已被后续修改取代**：① 拆分规则"按 NCX 递归拆分"2026-09-19 已改为"**只切第一层**，单卷仍超预算再按页贪心切"（原因与 panic 排查见 `bookconv优化白皮书.md` §15）；② 网页「投入原生书库」按钮的体积门（"PDF 灰掉"）已不存在，`nativeUploadLimitBytes` 只留在 `status` 里；③ 超限时现在**优先走占位通道（§03bn）**，按卷拆分降为兜底。本节保留问题发现、CSS 根因和"先量再改"的方法。

用户看完 §03aw 回执后给了三条：

1. **脚注返回**：脚注跳转后"点了跳不回来"本来当成 `Anchor` 模式的已知限制记，用户指出**已经有解**——reMarkable 原生"返回"浮标（`displayLinkNotification`）在跟内容里任何内部链接交互后都会出现，延时早前已从 8s 延长到 20s（另一条独立线的成果）。**§03aw 把这条写成"未解决的已知限制"不准确，此处更正**：`Anchor` 模式的脚注跳转体验完整（有跳转 + 有原生返回路径）。
2. **《火影忍者》"未裁边"**：拉这本书真实页面（抓两页截图）核对——它本来就紧贴内容边缘、没有留白，`trim_margins` **正确识别出没有值得裁的东西、什么都不做**（`TRIM_TOLERANCE` / `TRIM_MAX_FRACTION` 两道防误判门槛的设计意图），不是漏裁。（`TRIM_MAX_FRACTION` 后来在 2026-09-19 从 0.15 调到 0.35：镖人每卷版权页留白单边能到 22%–29%。）
3. **超限漫画能否按卷拆分投原生**：用户拍板三点——只做 EPUB、一旦超限就全拆、复用原按钮不新增入口。当天设计+实现+真机验证完成。

**新增 `bookconv::comic_split` + `Staging::try_deliver_split`**：超限 EPUB 漫画按自带 `toc.ncx` 拆卷，各自独立组包上传，聚合成功/失败回执；拆出的份只存在于内存、上传即弃，母版库原书字节不动。真机用《火影忍者(第2部_卷8~卷14)》（281MB，7 卷合集）端到端：一次 `POST /staging/deliver` 拆出 8 份（总封面 + 7 卷，每卷约 38–40MB，都在当时 150MB 预算内）全部上传成功（约 1m22s），设备侧 `.metadata`/`.content` 核实 xochitl 已自动渲染打开其中一卷（`lastOpenedPage:10`）——是真能读的文档。顺手发现网页「投入原生书库」的成功回执一直被前端静默丢弃（`postJ` 只在失败时 `alert`），一并补上。

**当天追加：拆分卷四周留白**（用户拍照对比 KOReader 与原生发现）。没停在"看照片猜"——拉设备渲染缓存 `<uuid>.pdf`，pymupdf 精确测量《卷八》：页面 303×538pt，图片实际只占 (17.8,35.5)–(284.8,447.9)，底部空出 90pt。

- **根因**：`comic_split::build_piece()` 直接调 `epub::assemble()` 吐出的页面**没有任何 CSS**——默认文档边距没清零、`<img>` 没撑满容器；KOReader 对漫画有自己的贴边逻辑，原生阅读器走标准文档流就露馅。
- **修法**：`repack_with_comic_css()` 组包后补外链 `comic.css`（`body{margin:0;padding:0}` + `img{width:100%;height:auto}`，只用裸元素选择器，符合 §03y 七条实测规则），**每个含 `<img>` 的章节 `<link>` + `content.opf` manifest 两处都要补**（漏一处可能不生效）；纯文字页（如"后记"）不挂，免得被清零边距。不改共享的 `epub.rs`（fb2/mobi/kf8/article 都在用），改动收在 `comic_split.rs` 内。
- **后续结论**（2026-09-19，当前代码注释）：镖人真机逐像素对比 `max-width/height:100%`、`vw`/`vh`、`display:table` 居中等五种写法，**跟纯 `width:100%;height:auto` 完全一样——xochitl 对 `height`/`max-height` 类 CSS 一律不认**，"底部留白"靠 CSS 治不了，修法挪到图片像素本身：`imgopt::pad_to_device_aspect` 把图片补白成设备页面长宽比。

**这次测试的副作用（待用户处理）**：设备上因为这轮测试 + 用户自己点了一次「投入原生书库」，躺着 **14 份**《火影忍者》卷八～卷十四（7 卷各 2 份，都是修 CSS 之前的旧版、带留白问题）。清理"删多个文档"被权限分类器拦下，没有清——**是待用户自己处理或明确授权的操作，不是技术限制**。

### 03be｜落库进度条不刷新 + 「加入 xochitl → 文件夹」填名不建文件夹：两条独立反馈，一条补事件推送、一条复活死代码（2026-09-19，真机通，✅已解决）

用户对同一天上午刚做完的「进度条取代静态文案」追加两条反馈：① "进度条不会动，要自己刷新"；② "加入原生书库 → 文件夹里写了名字依然不会创建文件夹"。

**① 进度条冻结（症状 → 根因 → 修法）**：`try_deliver_split`（现为 `deliver_pieces`）每完成一份拆分卷确实把 `progress:{done,total}` 写进边车，但当时没拿到 `EventBus`，写完不 `bus.publish`；网页那套"SSE 推事件才刷新"的零轮询机制不知道记录变了，进度数字冻结在第一次渲染的值。修法：`deliver`/`try_deliver_split` 签名加 `bus: &EventBus`，每完成一份紧跟一次 `bus.publish("books","staging")`。真机订阅 `GET /events` 确认 SSE 流正常（真触发一次优化，收到入库/开始/完成三条事件）；**漫画拆分卷的逐份推送这次没有用真实超限漫画重新端到端验证**（结构性改动，风险很小，如实说明）。

**② 文件夹不会自动创建**：

- **先坐实事实**：`strings` 真机 xochitl 二进制，本地网页接口（`httpinterface`）只有 `/documents/`、`/upload`、`/download`、`/thumbnail` 四个路由，**没有建文件夹的 HTTP 接口**；`extract_qml.py` 解出内嵌 QML 全量比对，确认"新建文件夹"对话框走 `root.library.createCollection(parentFolderId, collectionName)`，`root.library` 是裸全局单例 `Library`（不是 `LibraryController`）。（`extract_qml.py` 现在只在本机回收站的 `oldbak/` 里，只读引用，没恢复/移动任何东西。）
- **巧合发现**：`git log` 挖出这条能力 2026-09-07 就为 note-serve 写过一整套（`mkdir.rs` + `shelf-mkdir-agent.qmd`，见 §03aa），2026-09-09 没了消费方、2026-09-15 被当死代码删除——但删除提交留了一句"已部署在真机上的旧版 qmd 不受影响"，于是这份 qmd **在真机上原封不动跑了一周多，一直安静轮询着一个早就 404 的接口**。`journalctl -u xochitl` 核实它从 2026-09-11 起被 `qmldiff` 反复成功加载、零解析错误——这条注入路径本身早已稳定的额外真机证据。
- **修法**：用 `git show <删除提交>^:路径` 把 `mkdir.rs` 与 `shelf-mkdir-agent.qmd` **原样捞回**（qmd 一个字没改）；`Staging::deliver` 新增 `ensure_folder`：目标文件夹不存在就往 mkdir 队列入队，用 `fswatch::watch_until`（3 秒防抖、**20 秒超时** `FOLDER_WAIT_TIMEOUT`）同步等代理真建出来再继续；等不到就原样走"找不到就落书库根"的 best-effort 兜底（不是新错误）。
- **离线验证**：用 `asivery/qmldiff` 对固件解出的真实 `MainView.qml` 跑 `apply-diffs`（固件 md5 与当年一致），`Timer{ id: shelfMkdirAgent … }` 正确插进 `FocusScope#rootItem` 之后，无需额外 `IMPORT`。
- **真机端到端（完整闭环，补上当年"没有真机点过对话框"的缺口）**：真实上传一本测试 EPUB，`POST /staging/deliver` 指定一个设备上原本不存在的文件夹名，8 秒内状态 `pending` → `ok`；真机 SSH 核对 `.metadata`——新文件夹真的建出来（`type:"CollectionType"`，`parent:""`），测试文档 `.metadata` 的 `parent` 精确指向它的 uuid。这是 `Library.createCollection` 这条调用链第一次有真机交叉验证。测试文档/文件夹排进 `POST /trash/add` 队列等回收站代理清掉（**如实记录：还没确认清掉**）。

**教训**：① 砍功能时，真机上已部署的旧版本"优雅降级"（这里是 404 静默不做事）不代表设计有问题，只是没等到真消费方——`git log --diff-filter=D` 找回一份反编译分析已完成的死代码，比从零重来快得多也更可信（`Library` vs `LibraryController` 这类细节不用再踩一遍）。② `~/.local/share/Trash/` 是本机回收站，`oldbak/` 被挪进去了，这次只读引用，`oldbak/` 去留仍是用户自己的决定。

### 03bf｜§03be 跟进：《乱马1/2》文件夹名带斜杠被误拒，`ensure_folder` 静默放弃（2026-09-19，真机通，✅已解决）

§03be 修完后用户追问"book-serve 是否过重、bookconv 要不要拆独立服务"（无代码改动，结论＝不拆），接着报告"还是没有建立文件夹，我文件夹名为乱马1/2"——同一功能的**第二个独立 bug**。

- **症状**：文件夹名 `乱马1/2` 每次都建不出来，书落回书库根，表现跟"没建文件夹"一模一样。
- **排查**：`/mkdir/add` 手测 + 真实 `/staging/deliver`（全新文件夹名）反复验证 §03be 机制本身没有可复现缺陷，只能先推测是一次性时序问题；**用户第二次反馈带来确定性线索：名字本身就是问题**。
- **根因**：`MkdirQueue::add()` 有一条"名字不能带路径分隔符"的校验，含 `/` 直接拒；`ensure_folder` 对这个 `Err` 静默放弃（"交给 upload 的兜底"），于是**每次**带斜杠都稳定触发。**这条校验没有技术依据**：`name` 全程只当 JSON `visibleName` 字符串走（`createCollection(parentId, name)` 收普通 JS 字符串；`find_folder_by_name`/`Xochitl::upload` 按 `visibleName` 整体比较；文件夹真实落点走 `.metadata` 的 uuid），当初只是"路径分隔符＝可疑"的直觉防御。
- **修法**：整条删掉 `/`、`\` 校验，只保留"trim 后不能为空"；新增回归测试 `mkdir::tests::add_accepts_names_with_slash`。
- **真机端到端**：用与用户报告完全一致的名字 `乱马1/2` 调 `/staging/deliver`，~8 秒建出文件夹（`SHELF-MKDIR: created 乱马1/2`），测试文档 `parent` 指向它；测试文档/文件夹已排进 `/trash/add` 队列。**用户随后在自己设备上用同一个名字重试，回复"已正常"**——比 §03be 那轮（没等到用户确认）闭环更扎实。
- **教训**："机制本身没问题"（压力测试通过）≠"用户这次操作一定成功"；用户带着具体失败样本回来时，先假设这个具体输入有特殊性，而不是再把上一轮"验证机制"重跑一遍。

### 03bk｜漫画 EPUB 优化改产出带书签 PDF，根治左右留白硬限制（2026-09-19，真机通，✅已解决）

> **⚠ 已被取代（2026-09-20）**：用户拍板漫画「优化」**不再转 PDF**，统一"优化不改格式"——转 PDF 虽能拿到 0% 左右留白，但一图一页会丢漫画里夹带的文字页；EPUB 的固定内边距是 xochitl 渲染引擎的硬限制，接受它换取"不变动书籍内容"。`Staging::optimize()` 里漫画→PDF 分发已删；`comic_pdf` 模块保留，仅服务超限 PDF 分卷投递（`deliver_split_pdf_streaming`）。决策记录见 `bookconv优化白皮书.md` §19 末「漫画换回 EPUB」小节。此后又找到另一条路让 EPUB 漫画左右留白≈0：阅读器内 qmd 代理调 `setMargins(1)`（`xovi/shelf-comic-margins.qmd` + `GET /margins/{uuid}`，实验室开关默认关，见 `bookconv优化白皮书.md` §20）。
>
> 下面只留当时（2026-09-19）的决策依据。

用户连续几轮反馈漫画留白（裁边比例、拆分丢文字页、上下留白堆一边）修完后追问"为什么文字书调页边距能到边、漫画不行"，一路查到根因：xochitl 的 EPUB 渲染走文字排版盒模型，内容区相对物理页面有个消不掉的固定内边距（`.content` 的 `margins` 字段，UI 只给 28/56/112 三档，改文件也没用），CSS 层面测过的绕开手段全部失败。真机对照发现 **PDF 直传左右留白能到 0.00%**（PDF 走独立于文字排版盒的直接光栅化路径，`.content` 里根本没有 `margins` 字段），用户当时拍板改产带书签 PDF。实现与四轮真机内存排查（optimize 595MB → 206MB、split 卡死 11 分钟 → 525MB → 199.6MB → 67.8MB 的 VmHWM 数据表）见 `bookconv优化白皮书.md` §16；分支 `feat/comic-pdf-optimize` 曾合并进 `feat/gateway-concurrency-budget`（§03bl）。

### 03bn｜大文件"占位 + 替换"通道与统一命名规则（2026-09-20，真机验证过 PDF 154MB / EPUB 153MB）

> 这一节是主线摘要；完整实验过程与踩坑见 `bookconv优化白皮书.md` §19 末尾几个小节。当前代码：`rmsvc_core::xochitl::Xochitl::upload_large_file`、`bookconv::placeholder`、`book-serve` 的 `Staging::try_deliver_direct`（`staging/deliver.rs`）。

**问题**：xochitl 的网页上传接口有约 100MB 硬限（**100,000,000 字节**；超了直接断连或 413，2026-09-19 真机二分法实测）。此前只能把大漫画按卷拆分（§03ax），PDF 和非漫画则只能拒绝。

**做法**：`book-serve` 跑在设备上、能直接写 xochitl 的书库目录，所以只让网页接口传一个**占位文档**（PDF：一页空白；EPUB：带真封面，几十到几百 KB）让 xochitl 建好条目，再把磁盘上那个文件原子替换成真文件。图见 [`upload-limit-bypass.svg`](../../docs/diagrams/upload-limit-bypass.svg)。

| 步骤 | 做什么 |
|---|---|
| ① 造占位 | EPUB：`epub_placeholder(真书, 规范名)`——**必须带真书名（`dc:title`）和真封面**（封面按 OPF `<meta name="cover">` → `properties="cover-image"` → 第一个 spine 页首图的顺序找，找不到则只有标题）；PDF：`pdf_placeholder()` |
| ② 上传占位 | 走普通 `/upload`（占位很小，不撞上限） |
| ③ 认新文档 | 每 200ms 轮询，最多 20 秒，按"创建时间 ≥ 上传前 2 秒 且 占位文件字节数吻合"认出刚出现的那份（`/upload` 不回 uuid） |
| ④ 复制真文件 | 复制成 `<uuid>.<ext>.new`，校验大小一致，权限 0600 |
| ⑤ 改元数据 | EPUB：删占位的渲染缓存 `<uuid>.pdf`/`.epubindex`；PDF：改写 `.content` 的 `pageCount`/`originalPageCount`/`pages`（逐页 UUID 表）/`redirectionPageMap`/`sizeInBytes` |
| ⑥ 原子 rename | `.new` 覆盖占位文件；不需要重启 xochitl |

**决策顺序**（`Staging::deliver`）：格式门 → `ensure_folder` → 体积 ≤ `nativeUploadLimitMb`（90MB）→ 普通流式上传；超限 → **优先走大文件通道**（EPUB/PDF、≤ **1GiB**（`MAX_DIRECT_BYTES`）、不分卷、不限漫画）；本机没有书库目录、超 1GiB 或造占位失败（`try_deliver_direct` 返回 `Ok(None)`）→ 才退回按卷拆分（漫画 EPUB / 带书签漫画 PDF）或拒绝。**占位已上传之后才出的错直接报错、不再退回分卷**（否则书库里会留重复内容）。

**渲染徽章**：两条路都记渲染记录——PDF 直接 `ok`（页数就是写进 `.content` 的真页数）；EPUB 记 `onopen`（xochitl 要用户**首次打开**才渲染，此刻 `.content` 里是占位的页数），之后每次列表读该文档 `.content` 的 `pageCount`，页数一变（用户打开、xochitl 渲染完）就自动升级成 `ok` 显示真页数。`backfill_render_records` 给大文件通道上线前直接投入、没有渲染徽章的老书补记（按书名 + 文件大小认领）。

**已验证 / 已知限制**：

- 真机验证过 PDF 154MB / 349 页、EPUB 153MB 都能打开；EPUB 146MB 首次打开约 25 秒重新渲染，之后走缓存。PDF 列表里的页数/大小要用户点开后才刷新（真机观察）。
- **未验证**：>153MB 的首次渲染内存与耗时（1GiB 上限只是保守上界，不是测出来的）。
- 占位上传后进程崩溃会在书库残留占位文档（回执提示手动删除，不做危险的回滚删除）。

**统一命名规则**（`bookconv::naming`）：EPUB 一律 `书名 - 卷/部/上/下`，**数字在前**：

| 输入 | 规范化结果 |
|---|---|
| `卷02` | `02卷` |
| `第二卷` | `二卷` |
| `Vol.3` | `3卷` |
| `镖人(卷二)` | `镖人 - 二卷` |
| `上` / `中` / `下`（及 `上册` 等） | 原样保留 |
| 下载站 ` -- 作者 -- … -- hash` 尾巴、`[完]` 标记 | 去掉 |
| 无卷标记（如 `疯探-空城` 的"空城"是副标题） | 原样保留 |

规则幂等。入库（`stage_new`/`stage_from_path`，仅 EPUB）直接用规范名；已有长名在「优化」完成时改名（边车一并移动，目标已存在则保持原名不覆盖）。优化时对**带卷标记**的书把 OPF 的 `dc:title` 也改成规范名，设备里显示名才与文件名一致；无卷标记的不改（避免把 `abc123.epub` 这种无意义文件名覆盖掉书里本来正确的书名）。

## 第 D 章 网关与网页 UI

> **大白话导语**
> 设备上跑着好几个各管一摊的小服务（书库、字体、KOReader、壁纸、笔记……），它们都只听设备本机端口。**网关（gateway）** 是它们对外唯一的门：你在手机或电脑浏览器打开 `https://shelf.local/`，看到的"秘密花园"网页、登录密码、证书全由网关提供，网页上每次点按钮再由网关转给对应的小服务。
> 本章记这扇门和这个网页**怎么一步步长成现在的样子**：HTTPS 与私有 CA、密码登录、`shelf.local` 伪域名、事件推送、中英文切换、首层四个 tab 怎么排。名词速查：**CA**=自己签发证书的"根证书"，装进手机一次后浏览器不再报"不安全"；**mDNS**=局域网里不靠 DNS 服务器、设备自己回答"shelf.local 是谁"的协议；**SSE**=服务端事件推送，服务主动通知网页"该刷新了"；**qmd**=对 xochitl（reMarkable 官方阅读器/UI 进程）QML 界面的补丁。
> 阅读顺序：先看下面"现状结论"和结构图；证书/登录细节看 §03j，事件推送看 §03z；§03g–§03n 是早期 UI 迭代流水账，已压成"保留的决策和坑"。网关**自身**的权威设计（请求转发、批量队列、并发闸门）在 `gateway/docs/reMarkable网关白皮书.md`，本章不重复。

> **现状结论**
> - 网关对外 `0.0.0.0:443`（`gateway/systemd/gateway.service` 的 `--bind`；2026-09-10 前是 `:8778`，见 §03am）：HTTPS（私有 CA，首启生成，`/ca.crt` 可下载）+ 登录页密码（无用户名，首次默认 `shelf`、登录后强制改；网页会话 30 天、CLI 走 Basic）+ mDNS `shelf.local`；网页标题"秘密花园"。
> - 首层四个固定 tab：**传书 / 笔记 / 其他 / 管理**（结构见下图）；正文全量中英文（`gateway/ui/locales/{zh-CN,en-US}.json` 各 460 个 key，缺 key 显示 key 本身）。
> - **网页零轮询**：服务在变更处发事件 → 网关 `GET /api/events` 汇聚 → 网页只刷对应 tab；批量队列与并发闸门进度也走同一条流（`events::notify_books`）。旧版写过"批量运行期间前端每 3 秒刷一次"，已被取代，`app.js` 里现在没有 `setInterval`。
> - **母版库页 2026-09-20 重做**：行内只显示书名/类型/大小/状态/进度，仅处理中/排队中才有一个"停止/取消排队"按钮；所有操作在勾选后的**底部批量栏**（优化 / 加入 xochitl / 加入 KOReader / 删除 / 清除）；"加入位置"是两个常驻下拉（xochitl 文件夹、KOReader 目录，均可"＋新建"）；搜索带下拉建议；真分页（25/50/100）；PC 与手机同一套单列布局。批量队列与并发闸门都在网关（`batch.rs`/`budget.rs`），细节见 §03bp 与网关白皮书。
> - **历史节里的旧名对照**：

| 历史节里的说法 | 现在 |
|---|---|
| `shelf-gateway`（`shelf/services/shelf-gateway/`） | 顶层 `gateway/`（2026-09-11 正名搬出，见网关白皮书 §01） |
| `shelf-core::{tls,auth,mdns,events,http,ttf}` | 顶层共享基座 `rmsvc-core` 的同名模块 |
| `ui.rs` 里的 `PAGE` 大字符串 | `gateway/ui/{index.html,style.css,app.js,auth.css}`，编译期 `include_str!`；`ui.rs` 负责拼装与登录/改密页 |
| `shelf-gateway passwd/reset-password/regen-tls` | `gateway passwd <新密码>` / `reset-password` / `regen-tls`（`gateway/src/main.rs`） |
| `shelf/deploy.sh` | `packaging/deploy.sh`（`shelf/build.sh` 编译，`shelf/install.sh` 设备端安装） |
| `shelf/services/{font,wallpaper}-serve` | `enhance/{font,wallpaper}-serve` |
| 电脑端 `shelf …` 命令行（`events`/`inbox`/`push`/`koreader font`/`passwd`） | 2026-09-18 整体砍除、无网页等价物；下文只作历史记录 |

![网页首层四个 tab 与二级 tab 结构](diagrams/web-ui-tab-structure.svg)

#### 本章坑位表（一）

> 收录 §03g–§03al 的坑；后半章（§03am 起）见"本章坑位表（二）"，跨章通用的坑见附录 §04。

| 坑 | 症状/判据 | 根因 | 教训/规避 | 见§ |
|---|---|---|---|---|
| busybox `ls` 中文名显示 `?` | 清理时目录列表全是问号 | 设备 busybox 显示假象，字节完好 | 验名一律 `find \| hexdump -C` | §03g |
| 清理时把用户原有目录当测试遗留 `rm -rf` | KOReader `漫画/`（镖人+阿拉蕾×3）连 `.sdr` 进度被删 | curl 传未编码中文 URL 失败后，误以为根列表里的目录是自己建的 | 清理前 `find` 核对是否本来存在；绝不对用户数据目录 `rm -rf`；手工 curl 中文 URL 要百分号编码 | §03h |
| `running()` 误判 KOReader 在跑 | 全量并行测试时非 dry-run apply 被拒 | 只匹配 "koreader"，cargo 测试二进制 `koreader_serve-xxx` 也含 | 判据改 `reader.lua`/`koreader.sh`/`luajit`+`/koreader/`；运行态由调用方注入，单测不碰真 `/proc` | §03h |
| 设备 busybox `wget` 不认 Basic/自签 | 部署脚本健康检查失败 | busybox wget 功能残缺 | 设备上只查 systemd+注册表，HTTPS 探测由 host `curl -k` 做 | §03h |
| 浏览器 HTTPS"不安全"提示 | 用伪域名/IP 都一样报 | 提示来自证书**信任链**，与地址形式无关 | 装私有 CA 一次才真零提示；叶证书有效期 ≤ 825 天（Apple 拒绝更长） | §03j |
| 安卓浏览器打不开 `shelf.local` | Chrome/安卓 `.local` 不通 | 安卓系统解析器不查 mDNS | 走 host 热点时给内置 dnsmasq 加别名 `shelf.rm`（已在证书 SAN） | §03j |
| `std::sync::Mutex` 重入 | 测试二进制静默卡死 4 分钟 | `change_password` 持锁期间又调 `must_change()` | 锁内先取值再用 | §03j |
| `pkill -f "<含自己命令行的模式>"` | 当前 shell 被杀（exit 144） | 模式匹配到执行 pkill 的 shell 自己 | 别匹配自己 | §03j |
| 盲改 qmd 上机 | 字体菜单只剩 4 项 | qmd 里打不中的 TRAVERSE 会让**整个 qmd 不生效** | 先解出真 QML，再用 qmldiff CLI 离线 `apply-diffs` 验选择器 | §03m |
| SSE 小帧发不出去 | 真机 curl 30 s 零字节，垫到 1.2 KB 也没用 | tiny_http 的 chunked 编码器攒满 8 KB 才发，外面再套 1 KB BufWriter | 流式回执用 `Request::upgrade` 接管裸 socket，一帧一 flush | §03z |
| 上传器失败后不能重传 | 某本失败后再点上传不重试 | 循环 `if(f.st)continue` 把失败项也跳过 | 只跳过 `st==='ok'` | §03n |
| 关键解释只在 `title` 属性 | 触屏看不到禁用原因/徽章说明 | 触屏没有 hover | 写成可见小字；徽章点一下弹完整说明（全局委托，现走 toast） | §03af |
| 三级嵌套子标签点哪个都错位 | 外层按钮 3 个、扫到面板 5 个 | `subtabs()` 的 `querySelectorAll('.subpanel')` 不限层级 | 改 `:scope > .subnav`/`:scope > .subpanel` | §03al |
| `hidden` 与 `.subpanel.on` 并存 | 按钮隐藏了、面板还在 | 作者样式表的 class 选择器压过 UA 给 `[hidden]` 的 `display:none` | 隐藏前先切回别的子标签；用 `hidden` 不从 DOM 移除（`subtabs()` 靠位置下标配对） | §03ak |
| 路由带后缀通配不匹配 | `/ui/locales/zh-CN.json` 404 | 路由只支持整段 `{param}` | 整段捕获 `{lang}` 再自己剥 `.json` | §03ae |
| 文档写"body 流式透传"与实现不符 | 代理响应其实整体缓冲 | 只有请求方向真流式 | 改注释不改行为 | §03ah |
| 反复点启停 battop | 理论上复现 cgroup/RCU 死锁触发条件 | 内核 RCU 问题没修，只是把触发频率从 144 次/天降到"手点几次" | 不能说"不会重现"；网页无防连点，文案写明边界 | §03ak |
| 搬功能时原样搬旧文案 | battop"未装"提示指向旧路径 `misc/battery-audit/battop/install.sh` | 搬走后没核对文案内容 | 搬文案顺手核对；改成 `enhance/battop/install.sh` | §03ak |

### 03g｜用户反馈补齐（2026-09-03 傍晚）

> 历史记录：CLI `shelf koreader font …` 已随电脑端命令行砍除；KOReader 页现在只有「字体 / 词典」两个子标签，书籍走母版库"加入 KOReader"。

- **KOReader 页缺"直接传字体"**：加上传区（只装进 `koreader/fonts/`，不进原生阅读器）+ 列表 + 删除；koreader-serve `DELETE /fonts/{file}`（现仍在 `koreader-serve/src/api.rs`）。真机：上传→列表→删除通。
- font-serve 删字体曾同步撤 KOReader 镜像——§03h 整个撤销（字体分开装、不再镜像）。
- 清理测试遗留物：测试字体经 API 删、`books/shelf-test/` 删、壁纸 `unbind` 还原原生（md5 30569afc）、测试书 `.metadata` 改 `parent:"trash"`。
- **去掉"内建字体"概念（用户纠正）**：原 font-serve 把旧字体菜单 qmd 硬编码的三项抄成"内建、不可删"——是对旧中文化套件 scp 字体的路径耦合。改为 font-serve = `$XDG_DATA_HOME/fonts/` 管理器：全部字体一视同仁、按 `fc-scan` 首家族名归组（KF Readerly 四字重=一项；`%{family}` 自带中文名作显示名）、`DELETE /{family}` 删整家族；被 `~/.config/fontconfig/fonts.conf` 引用的家族只标 `fontconfigRef` ⚠（界面中文回退）不拦。真机：6 家族、上传并入同家族、重启后索引自动剔除已删文件。

### 03h｜HTTPS + 密码 · 字体分开装 · KOReader 子目录（2026-09-03 晚，用户三条要求）

> 被 §03j 取代：本节的"Basic 弹窗密码 + 随机初始密码文件 + `show-password`"已换成登录页；自签证书已升级为私有 CA；哈希已从单轮 salted SHA-256 升级为 PBKDF2。留下的是当时的落地事实和事故教训。

- **网关 HTTPS + Basic（第一版）**：首启 rcgen 自签（SAN 含 shelf/localhost/remarkable/10.11.99.1 + 设备当前 IP，存 `~/.config/shelf/tls/`，key 0600）；失败 401 + `WWW-Authenticate`；loopback 领域服务不认证。**真机**：明文 HTTP 连不上、无/错密码 401、对密码 200。tiny_http `ssl-rustls`（rustls 0.20 + ring）musl 交叉编译无碍，当时网关 2.2MB。`install.sh --password` 沿用至今（现走 `gateway passwd`）。
- **字体分开装**：字体页只装原生（fontconfig 目录），KOReader 页只装 KOReader，不再镜像（font-serve 去掉 mirror 配置与 ureq 依赖）。
- **KOReader `books/` 多级目录**：`subdir()` 允许多段（拒 `..`/隐藏/空段/反斜杠）；`GET /books?folder=` 返 `{kind: dir|file, count}`（跳过 `.sdr`）。真机：根列出 小说/漫画，`../x` 被拒。（当时 UI 有面包屑进目录；该 UI 现已不在，"加入 KOReader"目录改由母版库页的下拉/`＋新建`承担，后端 `subdir()`/`/books/mkdir` 仍在。）
- **⚠ 事故（我的错）**：验证子目录时 curl 传未编码中文 URL 失败，却把根列表里**用户原有的** `漫画/`（镖人+阿拉蕾×3 AZW3）当成自己建的 `rm -rf` 了（含 `.sdr` 进度与阿拉蕾手调的 RTL/对比度）。源文件在 host `~/Documents/ereader/books/漫画/`；用户决定不恢复、改作 Calibre vs 设备端优化的验证素材。教训见坑位表。

### 03j｜登录页密码 · 私有 CA · mDNS 伪域名（2026-09-03 深夜，用户："能用伪域名吗？尽量不要有安全提示，不需要用户名，新增页面输入密码正确即登入，首次默认，登录后必须改"）

**前提**：浏览器"不安全"提示来自证书**信任链**，与地址是 IP 还是域名无关；伪域名本身不消除提示。零提示只有两条路：① 把自己的 CA 装进客户端信任库一次；② 真域名 + 公共 CA（Let's Encrypt DNS-01，需自有域名与续签外网）。本项目走 ①，并把 CA 做成一键下载。

**登录与改密（`gateway/src/auth.rs` 策略 + `ui.rs` 页面；HTTP 层只见 `rmsvc_core::http::Guard`）**
- 登录页只要密码。会话 Cookie `shelf_session`（HttpOnly · SameSite=Strict · HTTPS 时 Secure · 缺省 30 天，令牌只在内存，网关重启需重登）。未登录：浏览器（`Accept` 含 `text/html`）303→`/login?next=…`，API 401；密码错延时 500ms。公开路径只有 `/login` `/logout` `/ca.crt` `/health` `/favicon.ico`。CLI 用 Basic（用户名任意、只看密码）。
- **首次默认 `shelf`、登录后必改**：`gateway.json`（`$XDG_CONFIG_HOME/shelf/gateway.json`）无哈希时写默认哈希 + `mustChangePassword=true`；必改态下会话/Basic 都只能到 `/password`（网页 303、API 403"首次登录必须先改密码"）；新密码 ≥ 6 位（`MIN_PASSWORD_LEN`）且 ≠ 默认；改完踢掉其它设备会话。忘记：`gateway reset-password`。
- **2026-09-09 审计加固（现行）**：哈希是 `pbkdf2$600000$…`（PBKDF2-HMAC-SHA256、16 字节随机盐；旧 `sha256$` 单轮格式仍可验证）；60 秒内连错 5 次锁 60 秒（429 + `Retry-After`，锁定期不做 PBKDF2，避免被并发猜密码烧 CPU）；改密页 `minlength`/提示从 `MIN_PASSWORD_LEN` 插值，不再各写硬编码。

**私有 CA 与叶证书（`rmsvc_core::tls::ensure_ca_signed`，存 `~/.config/shelf/tls/`）**
- `ca.pem/ca.key`（10 年）+ 叶 `cert.pem`（叶+CA 链）/`key.pem`（**800 天**——Apple 拒绝 >825 天的 TLS 服务器证书；含 ServerAuth EKU、SAN 必备，满足 iOS 13+），`cert.meta` 记签发时间与 SAN；SAN 变化或满 700 天自动**只换叶、CA 不变**，已装 CA 的设备无感。`/ca.crt` 公开下载（登录页有链接）。旧自签目录自动升级（浏览器再提示一次）。
- 叶证书 SAN = 设备各 IPv4 + `<mdnsName>.local` + `gateway.json` 的 `extraSans`（缺省含 `shelf.rm`）。

**mDNS 伪域名 `shelf.local`（`rmsvc_core::mdns`）**：只答本名 A 查询，应答地址取**与提问者同子网**的本机 IPv4（USB 网段答 10.11.99.1、热点网段答 10.42.0.x）。接口重扫现为 60 秒（初版 30 秒，后为省电合并读超时；代价是 WiFi 后连的新地址最迟 60 秒才被应答）。**安卓系统解析器不查 mDNS**——走 host 热点时在 host 加 dnsmasq 别名（需 sudo，用户自己跑）：

```sh
# host（NetworkManager 热点 = 内置 dnsmasq）；shelf.rm 已在证书 SAN 里（gateway.json extraSans）
sudo tee /etc/NetworkManager/dnsmasq-shared.d/shelf.conf <<'EOF'
address=/shelf.rm/10.42.0.224
EOF
sudo nmcli connection down Hotspot && sudo nmcli connection up Hotspot
```

设备 DHCP 地址若变，别名要跟着改（或在同文件加 `dhcp-host=<设备MAC>,10.42.0.224` 固定）。

**配置与子命令**：`gateway.json` 的 `https`/`auth`/`mdnsName`（空=不起 mDNS）/`extraSans`/`sessionDays`；`passwd <pw>` · `reset-password` · `regen-tls`（删叶重签、CA 不变）。

**真机（2026-09-03 深夜，WiFi 10.42.0.224，当时端口 :8778）**：无登录 `GET /` 303→`/login?next=%2F`、API 401、错密码 401、默认密码登录 303→`/password`、必改态 API 403、改密 303→`/`、会话与 Basic（任意用户名）API 200；`/ca.crt` 下载后 `curl --cacert ca.crt https://shelf.local:8778/`（`--resolve` 到设备 IP）与 IP 都 200 **零告警**；从 host 热点网段发 mDNS 查询得 `10.42.0.224`；旧自签目录自动升级成 CA+叶。验完已复位为默认 `shelf`。

**踩坑**：`change_password` 持锁期间又调 `must_change()`；`pkill -f` 杀自己——见坑位表。

### 03m｜网页改版 + 传书页重构 + 字体菜单长名截断（2026-09-04，用户三条要求）

> 历史记录：①②里的"传书页/`book-serve` tab 改名 xochitl/投递目标对比表/EPUB 处理档位对比表"已被取代——「传书」现在是"入库｜母版库"（§03bp），「xochitl」是「其他」下的字体子标签，档位表随"优化档位"一起砍除。留下来的是视觉体系与字体菜单修法。

- **① 网页美化**（现在的样式在 `gateway/ui/style.css`）：深/浅色跟随系统（`prefers-color-scheme` + 完整 token）、卡片式布局、圆角胶囊 tab、吸顶头部、统一的拖放区/进度条/徽章。纯内嵌 CSS/JS、无 CDN（设备离线也渲染）。
- **② 传书页 → xochitl 标签**（用户："像 KOReader 标签页统一风格，字体上传并进来，改名 xochitl"）：当时把原生字体上传并入 book-serve tab、独立「字体」tab 去掉；这套结构后又整体重排（§03al、§03bp）。
- **③ 字体菜单长家族名溢出**（用户："字体名称太长超出可视范围"）：
  - **症状→根因**：3.27 `FormatFont.qml` 的 delegate `Text#fontSample` 只锚左边、无右界也无 elide，长名（如 `LXGW Neo ZhiSong Screen Full`）画出格子。
  - **修法**：该 Text 加 `anchors.right: parent.right` + `anchors.rightMargin: 8`（初值 `Values.documentViewFormatTitleMargin`=30 太狠，用户目测 8 合适）+ `elide: Text.ElideRight`；`text`/`font.family` 仍是完整 `modelData`，`setFontName` 不受影响。现存于 `shelf/xovi/font-menu-dynamic-3.27.qmd`。⚠ 3.28 线的 `shelf/xovi/font-menu-dynamic.qmd` 里**没有**这段 elide（核对未见；是否需要移植未核实）。
  - **方法（务必复用）**：`extract_qml.py` 从 xochitl 二进制解出真实 `FormatFont.qml`（blob `qml_00d42f92`），建 [`asivery/qmldiff`](https://github.com/asivery/qmldiff) CLI，`apply-diffs <root> <dest> qmd -f -c` **离线实跑**，确认 `Item[#fontContainer] > Grid[#fontGrid] > Repeater > Item > Text[#fontSample]` 选择器命中、产物是合法 QML。**别盲改 qmd 上机**。
  - qmd 改动需重启 xochitl 才生效。原文当时写"一律走 `/home/root/xovi/start`"；现行纪律：xovi 已在运行的 xochitl 上用 `systemctl restart xochitl`，绝不手动跑 `xovi/start`，只有 xovi 没生效才用它——已生效时再跑会让运行中的 xochitl SEGV、整机自动重启（2026-09-20 踩过）。

### 03n｜细节调整（2026-09-04，重构主体后的打磨，用户四项全做，分批部署验证）

Explore 走查出的粗糙点 + 一个真 bug，分 5 批，批 1–4 真机部署验证。仍有效的结论：

- ★真 bug：上传器 `for(f of files){if(f.st)continue}` 让失败项永远跳过——改成 `if(f.st==='ok')continue`（失败可重传）。
- 队列逐项「×」删除、各上传器自动补「清空」、顶部「N/M 完成·K 失败」总进度；顶层回执 `note` 不再丢；localStorage（键前缀 `shelf.`，`try/catch` 包）记住 target/质量门/文件夹/壁纸模式；非 USB 网段时传书区提示"大书走 USB"。
- **中文回退链展示**：xochitl 字体区顶部列"中文缺字回退：A → B …"（`cjkPct` ≥ 8% 降序，客户端从 `/api/fonts` 算，与服务端 `cjk_fallback_keys` 同判据）。
- **failed 留原因**：book-serve `spool.rs` `archive_failed(p, reason)` 写 `<名>.reason` sidecar，`SpoolEntry.reason` 读回，`retry`/`delete` 连带清；inbox 行显示失败原因（重试不再靠猜）。真机：坏书失败带 reason、重试通、清理不留 sidecar。
- **字体渲染**：`ttf`（家族名/魔数/CJK 覆盖率 cmap）下沉为共享 `rmsvc_core::ttf`；KOReader 字体 `GET /fonts` 也算 `cjkPct`、网页显示徽标（真机：FZFW 35% / 京華 100%）；**embolden**：font-serve `emboldenCjkFallback`（原默认关，§03o 目测更清楚后改默认开），为真时对每个回退字体加 `<match target="font"><edit name="embolden">true`；`PUT /api/fonts/config` 实时切（fontconfig 无需重启 xochitl，翻书即见）。真机：PUT true→fonts.conf 6 条 embolden、false→0。
- **云同步探针遗留**：8 个测试文档 `parent=trash` 但 UI 回收站不显示（与云同步状态错位）；**正确清法是 UI 里清空回收站**（云端一并删），不直接 rm 本地文件（metadata≠UI，见附录 §04）。
- 已取代：批 3 host CLI（`shelf status`/`shelf inbox`）随命令行砍除；批 4 install.sh 的 `--with-xovi-reenable` 已撤回（§03o：把 shelf 耦合回外层 reenable，违背原则）。

### 03z｜事件推送：网页零轮询即时刷新（2026-09-06，用户"不喜欢轮询，要事件通知，包括 host"）

**设计**（三条硬约束：不轮询、不监听全盘、日志写入不触发）：
1. **服务在变更发生处发事件**：上传/优化/落库/mark/抓网文/删除、inbox 追平、KOReader adopt/字体/词典/配置、原生字体装删、壁纸入池/激活/删除/唤醒轮换……每个领域服务各挂 `GET /events`。唯二 inotify：inbox（本就有）与网关的注册表目录（tmpfs，服务启停）。
2. **网关汇聚**（`gateway/src/events.rs::Hub`）：每个有事件流的模块一条 loopback 订阅线程（`rmsvc_core::events::follow`），事件补 `svc` 后转发到网关自己的总线；`GET /api/events` 受登录守卫，在 `/api/{svc}` 代理通配之前注册。`MODULES` 里 `events:false` 的 `mind-serve`（纯被动问答）不订阅。
3. **网页**：`EventSource('/api/events')`，事件只刷对应 tab（前台立刻刷，非前台记脏、切过去时刷；页面被隐藏时只记脏，`visibilitychange` 变可见补刷）；`manage` 事件若 tab 集合变了整页重载；页眉小圆点显示连接状态；断线（WiFi 掉/休眠醒来）自动重连。
4. ~~host `shelf events [--once] [--area …]`~~ 已随命令行砍除（当时用户确认：页眉圆点绿、不刷新列表自己变，网页/CLI 两端闭环）。

**流式回执为什么不能走 tiny_http 的 `respond`（症状→根因→修法）**
- **症状**：真机隧道 curl 30 s 零字节，垫到 1.2 KB 也没用。
- **根因**：tiny_http 的 chunked 编码器（`chunked_transfer::Encoder`）攒满 8 KB 才发、外面再套 1 KB `BufWriter`，小帧永远滞留。
- **修法**：`Request::upgrade` 接管裸 socket（先发只有头的 200 + `Connection: upgrade`，浏览器/curl 对 200 忽略它），一帧一 flush（现为 `rmsvc_core::http::respond_stream`，`SseStream` 在 `rmsvc_core::events`，20 s 无事件吐一行注释心跳）。每请求一线程，长连接不堵别人。真机：inbox 落库与母版库删除的三帧当秒到达。

**耗电**：空闲时 inotify/通道 recv 都是内核阻塞、零唤醒；浏览器 SSE 20 s 一次十几字节心跳（网关↔服务的 loopback 连接后来带 `?ka=120`，2 分钟一次）；设备深度休眠时进程不跑、连接断、醒来重连；订阅线程断线指数退避（3 s→60 s）、服务没起等注册表 inotify 唤醒，不再固定 3 s 重连。对比"每 5 s 一个完整 HTTPS 请求"的轮询少两个数量级。

### 03ac｜可插拔机制验证：接住一条独立仓库线（2026-09-06 晚）

笔记线（独立仓库 `notes/`）选择复用书架的网关/注册表/事件汇聚/部署链——这是**笔记线自己的架构决策**（见 `notes/docs/reMarkable笔记白皮书.md` §01）。本节只记**书架这边为接住它实际改了什么**：

- `manage::MODULES`（现 `gateway/src/manage.rs`）加几行映射；
- 构建/部署脚本在对方 `Cargo.toml` 存在时顺带编译打包（现 `shelf/build.sh` 编译、`packaging/deploy.sh` 部署）；
- `install.sh`/`uninstall.sh` 令牌同规则加（现 `shelf/manifest.sh` 的 `SHELF_ALL` 含 `ink transcribe mind note`）；
- 网页按同一事件总线接一个新 tab。

没有为笔记线开任何专门口子；book-serve 原有的原生回收站队列（`POST /trash/add`）也被笔记线的软删需求复用。**真机（当时）**：设备只在 WiFi（USB 网卡没起，局域网扫 8778 找到 `192.168.1.22`），一键部署后书架 5 服务 + 笔记线 3 服务共八项 `active`、注册表 8 项、HTTPS 探测 401 正常，书架侧行为零变化。（现状：笔记线 4 个服务 ink/transcribe/mind/note。）**遗留 gotcha**：`MODULES` 是跨两个仓库的单一事实源——任何独立线接进来都得先来这个文件登记一行，README 已写明。

### 03ae｜网页 UI i18n 架子：主界面外壳 + 顶层导航（2026-09-09，部分真机验证）

> 被 §03an 扩展：本节只搭了架子（约 12 个 key），2026-09-10 §03an 已把正文全量迁移（现 460 key）。登录页/改密页仍是 Rust 端 `format!` 拼的中文，不走这套 key 查表（`ui.rs::login_page/password_page`）。

用户要求"考虑增加 UI 页面 i18n，可配置各国语言文件"。当时调研 `app.js` 约 52% 的行是中文硬编码、且不少是"条件分支 + 变量插值"的复合文案，机械抽取风险高，先搭架子 + 只迁外壳与顶层导航。**保留下来的设计**：

| 要点 | 做法 |
|---|---|
| 语言包 | `gateway/ui/locales/{zh-CN,en-US}.json`（扁平 key→字符串），`include_str!` 编译进二进制，延续"单文件零外链" |
| 服务端 | `ui::locale_json(lang)` 按语言码分发，不认识的落中文（不是空白页）；`GET /ui/locales/{lang}` 整段捕获再剥 `.json`，在登录守卫保护内（只有登录后的主界面才需要） |
| 前端 | `T(key)`（缺 key 显示 key 本身，不留空白也不掩盖翻译缺口）+ `currentLang()`（`localStorage` 记选择，缺省按 `navigator.language`，猜不出落中文）；启动 IIFE 先 `fetch` 语言包再渲染导航；`index.html` 有 `<select id="langsel">`，选后存 `localStorage` 并整页刷新 |
| 不翻译 | 产品专名 `xochitl`/`KOReader` |
| 离线测试 | `locale_files_have_identical_key_sets`（防漏改一份 JSON）+ `locale_json_falls_back_to_chinese_for_unknown_lang` |

**当时没做**：登录/改密页、各模块正文、后端 `ServiceSpec.label`（会出现在 `/api/services` 注册表里，影响面更广）。**部分真机验证（2026-09-09）**：`curl` 确认 zh-CN/en-US 各返回对应语言、`fr-FR`（不认识）落回中文而非 404/空白；主页 HTML 已含 `id="langsel"`；部署后 `NRestarts=0`。**没做到**：浏览器里点开切换器、肉眼看文案切成英文——只做到"数据链路通"。

### 03af｜网页 UI 人性化/触屏可用性一批小修（2026-09-09，离线）

三维审计里 UI 一路核查 `gateway/ui/` 的人性化与手机/PC 便利性；最突出的问题是"关键解释只存在于 `title` 属性，触屏完全看不到"。按高/中优先级修了一批：

| 问题 | 修法 |
|---|---|
| 禁用按钮的原因只在 `title`（"投入原生书库"超限、"加入 KOReader"未安装） | `btn()` 补一行可见小字（`flex-basis:100%` 独占一行） |
| 徽章完整解释只在 `title` | 全局委托：任何带 `title` 的 `.badge` 点一下弹完整内容（当时 `alert`；2026-09-19 全站 alert 统一改 toast，现为 `toast(b.title,'info',6500)`），配 `cursor:help` |
| "重扫"文案暗示有代价但没拦截 | 补二次确认，说明数据风险其实不大 |
| fetch 遇非 JSON 响应报裸"HTTP 状态码" | 改成一句人话 + 括号里的状态码 |
| 空状态"（空）"没引导 | `fillList` 新增 `emptyMsg`，各页传"去哪里做什么" |
| 窄屏 | `table.cmp` 加窄屏媒体查询去掉 `min-width:34em`；"×"/`.subnav`/头部链接触控热区补 padding（`.subnav` 没顶到 44px——两层叠着会占太多屏幕，是密度和热区的折中） |
| 可访问性 | select/input 补 `label`/`aria-label`；壁纸缩略图 `alt` 改描述性；上传队列"×"补 `aria-label` |
| 母版库主次视觉 | `.btn-bad` 也用于"删除"（销毁性），与"优化"（编辑性）颜色区分 |

**离线**：`node --check app.js`、`cargo test -p shelf-gateway`、交叉编译零警告。**⚠️ 当时未做真机/浏览器渲染核对**，只确认了代码逻辑；本节这批改动未单独复验。

### 03ah｜`shelf-gateway::proxy` 模块文档修正："body 流式透传"跟实现不符（2026-09-09，离线）

三维审计发现模块注释写"body 流式透传"，实际只有**请求方向**真流式（`send(&mut *req.body)`），**响应方向**整体缓冲进内存（`read_to_end`，`gateway/src/proxy.rs` 现仍如此）。评估过改成真流式（`Reply::stream`），结论是**不改行为，只改注释**：那套流式通道底层是 `tiny_http` 的 `upgrade("sse", …)` 接管裸 socket，专为 SSE 设计，用于任意大小下载响应前要先确认 Content-Length/chunked 语义，收益不值得承担不确定性。评估详情见笔记线白皮书 §03aj。

### 03aj｜「管理」拆二级 tab + 系统增强开关上网页（2026-09-09，真机通）

> 状态：本节是"管理"页**第一版**；后续 §03ak（加实验室）→ §03al/§03am（电池刺客几次挪位）→ 2026-09-21（battop 开关移回「系统增强」）多次调整，现状见结构图。

用户对「管理」提四点：① `shelf push` 卡片挪去「传书」页（该卡片随命令行 2026-09-18 删除）；② 「模型管理」卡片拆成二级 tab（`renderManage()` 顶部加 `.subnav`，复用 `subtabs()`）；③ 加"系统增强工具开关"二级 tab；④ 统一视觉风格。**③ 是唯一有真实后端工作量的点**，三个开关当时地基差异很大：

| 开关 | 当时的地基 | 处理 |
|---|---|---|
| cjk 画线吸附 | `reading-qol.json` 的 `hlSnapCjk`（默认开，langhook C hook 消费，见系统增强白皮书 §04） | 真开关 |
| cjk 手写笔迹优化 | 用户澄清是"手写笔锋按 CJK 书写习惯渲染"，与 AI 识别（`cardhw`）**无关**（第一轮理解错了）；当时全仓库**完全不存在**（无配置键/hook/反编译记录）；沾边的"够到原生渲染层"先例（笔记页背景滤镜）判死（C++ `SceneView` tile 增量渲染够不到） | 「未上线」占位卡片；**§03ak 接上了真实功能** |
| 电池刺客 battop | 原生设置页只有只读展示；2026-08-29 出过 cgroup/RCU 死锁（`enhance/battop/FINDINGS.md`），已修为常驻 `Type=simple` | 真开关（边界见 §03ak 追问 1） |

**后端结构**：`reading-qol.json` 有全量写回铁律（系统增强白皮书 §08）——`gateway/src/enhance/qol.rs::patch()` 把整份文件当不透明 `serde_json::Map` 读入、只覆盖要改的键、其余原样写回；`enhance/battop.rs` 探测（unit 文件在不在 + `systemctl is-active`）+ 开关（复用 `manage::run` 这个 `pub(crate)` helper）。`enhance.rs` 做成 `enhance/{mod,qol,battop}.rs` 目录——用户问"以后系统增强能力多了要不要单独文件夹"，权衡后**没有**升到独立 service/crate（都是同机文件 I/O / `systemctl` 直调，没有独立进程边界，硬拆只多一层 IPC/注册/代理开销），目录分文件低成本，每加一个能力就是加一个文件 + `mod.rs` 挂路由。

**④ 命令说明块视觉语言**：根因不是背景色对比度（第一轮猜错了）——`.opt-note` 与「笔记」页 `.entry-quote`（引用摘录）几乎同款：小字号、`color:var(--mute)`、浅底、左侧色条。命令是要人读要人抄的可操作内容，借"安静的引用摘录"语言会显得不起眼。新增 `.cmdblock`：正文级字号、等宽字体、整块实线边框。

**真机验证（用户密码授权，curl 功能测试）**：备份→scp→md5→`systemctl restart`→健康检查（`active`、MainPID 变化、`NRestarts=0`）。`PUT /api/enhance/qol {hlSnapCjk:false}` → `reading-qol.json` 其余 6 个键（`tapPageTurn`/`fastMono`/`refresh`/`refreshByChapter`/`refreshEvery`/`fontEnhance`）原样保留，再 PUT true 复原；`POST /api/enhance/battop/start`（设备当时未装）返回干净 400；未登录 `/api/enhance/status` 干净 401（新路由已接进鉴权）。命令块字体清楚了（用户确认）。⚠ 用户顺手测划线吸附没观察到效果（`hlSnapCjk` 全程未变、文件 mtime 事发前后都是 2026-09-02，与本次改动无关），见系统增强白皮书 §04 的独立追记，需单独复验那个 C hook。

### 03ak｜「管理」加「实验室」二级 tab + 独立「电池刺客」标签页 + 「导入 md 文档」改文件上传（2026-09-10，真机通）

> 被取代的部分：独立顶层「电池刺客」标签页当天就撤销（追问 2、§03al）；battop 后来拆成"开关 + 「管理→电池刺客」二级 tab"（§03am），2026-09-21 开关卡片从实验室移回「系统增强」。留下来的是两个设计取舍与遗留教训。

§03aj 的「CJK 手写笔迹优化」还是占位——同一天另一会话在 `enhance/handwriting-stroke/` 真机验证出两个 hook 目标（`FUN_00f47530`+`FUN_00f4c8d0`，见系统增强白皮书 §03e–§03f）。用户五条：①「管理」新开「实验室」二级 tab 并把手写笔迹/电池刺客搬过去；②新增「导入 md 文档」开关，控制「笔记」tab「导入」子标签显示；③完成手写笔迹开关；④完成电池刺客开关+独立标签页；⑤「导入」改名并从打字改成上传文件。

**两个设计取舍**：
- **手写笔迹开关纯网页层实现，不碰设备端 C 代码**：`hw_stroke.c` 没有独立开关字段，`hwStrokeNibMinRatio`/`hwStrokeSpeedMinRatio` 两个强度阈值本身就是"1.0=关、<1.0=开"。`qol::hw_stroke_enabled()` 只读 `hwStrokeNibMinRatio` 派生；`mod.rs::set_qol()` 收到 `hwStrokeEnabled` 时同步写两个 ratio（开＝`0.6`，真机验证过的强度；关＝`1.0`），角度/宽度/速度精调字段不碰。**代价**：网页开关关了再开会把手动精调的值冲成固定 `0.6`——接受，换零设备端改动、当天上线。
- **「导入 md 文档」不做真正的 multipart 上传**：前端用 `FileReader.readAsText()` 在浏览器里读成字符串，**继续走原来的 JSON POST**（`{title,markdown}`），`note-serve` 路由/`import_markdown()` 一行没改——"选文件"与"打字"的体验差异已做到，没必要为用户看不见的传输差异碰已验证的端点。

**「实验室」**：`subnav`/`subpanel` 从三个扩到四个（同一套 `subtabs()`）。「系统增强」留 `hlSnapCjk`（真正系统级、默认开）；「实验室」放"还在打磨"的功能：手写笔迹开关、导入 md 文档开关（`notesImportMdEnabled`，默认关——新功能首次上线不想让用户点开笔记 tab 就撞见半成品）。此后又加了 `comicMinMargin` 漫画页边距开关，共五个网页开关，全走 `GET /api/enhance/status` / `PUT /api/enhance/qol`（`gateway/src/enhance/mod.rs`）。

**「导入」子标签显隐**：`<textarea>` 换成 `<input type=file accept=".md,.markdown">`，文件名当默认标题；显隐跟 `notesImportMdEnabled` 走，用 `hidden` 属性而不是移除 DOM（`subtabs()` 是位置下标配对），挂在笔记 tab 的 `refresh()`（切回时调，不用额外接 SSE）；开关被关时若正停在「导入」，先 `.click()` 切回「浏览」再设 `hidden`（`hidden`/`.subpanel.on` 并存的坑见坑位表）。

**真机验证（curl，用户口头授权+密码）**：`build.sh` → 部署 → 健康检查（`active`、MainPID 变化、`NRestarts=0`）。`PUT {hwStrokeEnabled:false}` → 两个 ratio 变 `1.0`，其余 6 个 hwStroke 阈值 + `hlSnapCjk` + 无关键原样保留，`true` 复原为 `0.6`；`{notesImportMdEnabled:true}` 写入新键成功；`POST /api/notes/books/{uuid}/import-md {title,markdown}` → 真机 xochitl 目录生成完整 `.metadata`/`.content`/`.rm`/`.thumbnails`，笔记本真实可用。用户真机写字确认 `journalctl -u xochitl` 有 `[hw-stroke:f4c8d0]` 日志、开关状态与 `reading-qol.json` 一致（那次笔画 `w≈3` 低于 `hwStrokeNibWidthLow=5.0`、比例仍 `1.0`——是既有"细笔画自动排除"设计，不是开关失效；需更粗的笔才看得到）。

**遗留 bug**：battop"未装"提示写着 `misc/battery-audit/battop/install.sh`——battop 早已 `git mv` 到 `enhance/battop/`（§03b），这次原样搬了旧文案没核对。改成 `enhance/battop/install.sh`（现见 `zh-CN.json` 的 `battop.toggle.notInstalled`）；全仓确认这是唯一一处"指导操作"的路径引用停在旧位置。

**用户当天追问两点（都对，回来改）**
1. **"反复启停 battop 会不会触发 cgroup 死锁死机"**——核对 `FINDINGS.md`：原文案"只是一次性启停，不会重现事故的触发条件"**没讲清边界**。真正触发崩溃的是 `cgroup_procs_write`（进程启动时 systemd 把它塞进 cgroup）撞上内核罕见的 RCU stall——**内核问题没修，是概率事件**；旧架构靠 `battop.timer` 每 10 分钟重启一次，把这动作干到 144 次/天，放大了 144 倍撞窗口概率；新架构只在启动那一刻迁一次 cgroup，**是降低触发频率，不是把风险归零**。偶尔点几下可忽略，但短时间反复连点（按钮**没有防连点/限流**）理论上就是在复现旧 timer 的高频条件。边界已写进 `battop.rs` 头注和网页卡片文案。
2. **"battop 启动才应该显示电池刺客标签"**——服务 tab 本来就"运行才出现"，独立顶层标签页常显与约定不一致。当时做法：`addTab` 前先拉 `/api/enhance/status`，`battop.running` 才加；`battop_toggle()` 成功后手动 `hub.bus.publish("manage","battop")`（battop 不在服务注册表，注册表 inotify 碰不到它）触发整页重载。⚠️ **同一天被推翻**：用户拍板"电池刺客降级移入管理"，独立顶层标签页与这套机制一起撤掉（§03al）。

**离线**：`qol.rs` 新增 3 测；`node --check app.js`；语言包 key 集合一致性测试确认（新顶层 tab 走 `T()`，其余卡片当时仍硬编码中文，§03an 后全量迁移）。

### 03al｜首层标签重排（传书/笔记/其他/管理）+「入库」拆三卡 + battop 真机装机验证 + 电池审计数据表接入（2026-09-10，真机通）

同一天用户又提四条，一次做完：

**① 首层标签重排为「传书/笔记/其他/管理」**：原来"传书 + 注册表动态生成的 xochitl/KOReader/笔记/壁纸 + 管理"改成固定四段——「笔记」占第二位，xochitl(font-serve)/KOReader(koreader-serve)/壁纸(wallpaper-serve) 降一级包进新「其他」tab 当二级子标签（`renderOther(sec,svcs)`，只列已装且在跑的，与原"没装就不出现"一致）。

**踩到的嵌套坑（症状→根因→修法）**
- **症状**：包进「其他」后点哪个子标签都错位。
- **根因**：KOReader 自己的 `render()` 内部本有一层字体/词典 `subnav`，变三级嵌套；`subtabs(sec)` 用不限层级的 `querySelectorAll('.subpanel')`，把 KOReader 内层两个 subpanel 也扫进来——外层按钮 3 个、扫到面板 5 个。
- **修法**：改 `:scope > .subnav`/`:scope > .subpanel`（只找直接子元素）。对所有非嵌套调用点无副作用，顺手堵死这类潜在坑。

**② 「传书 → 入库」拆成独立卡片**：原来"上传/抓网文/电脑端 shelf push"挤在一张 `.card` 里用 `<h3>` 分隔，看着像"上传"带两个附属步骤；用户要求各自成卡（三条互不依赖的入库路径），纯 HTML 结构调整。（2026-09-18 `shelf push` 卡片随命令行砍除，现在只剩「上传」「抓网文」两张。）

**③ battop 真机实装（完整闭环）**：交叉编译 `enhance/battop`（musl 全静态）→ scp 二进制 + `install.sh` → 真机 `install.sh`（`mount rw` → 写 `/usr/lib/systemd/system/battop.service` → `enable --now` → `mount ro`；这是 battop 既有安装方式）→ `is-active`=`active`。curl 验证 `/api/enhance/status` 的 `battop.installed/running` 从 `false` 变 `true`，`start`/`stop` 循环两轮状态正确翻转、`lastSampleAt` 每次是新时间戳。

**④ 电池审计数据接上**：用户反馈"应呈现跟以前设备端电池审计对标的网页版，不是文字描述开关功能"。`enhance/battop/src/main.rs::write_summary` 的 `summary.json` 本就是"4 个时间窗（今日/7天/30天/全部）× 应用/进程/唤醒源 top15"的预聚合（battop 自己算好）；新增 `battop::summary()` + `GET /api/enhance/battop/summary`（没数据时 `available:false`，网页显示"还没有数据"而非报错）；卡片加时间窗 `subnav`，展示放电%/mAh/均值 mA/采样数 + 按应用累计时长 top + 唤醒源计数。真机 curl 返回真实数据（`windows.{today,7d,30d,all}` 齐全，装上没多久已有 `xochitl`/`koreader-serve`/`memfaultd` 条目）。

**⑤ 独立顶层「电池刺客」撤销**：用户拍板"降级移入管理"——§03ak 追问 2 刚做的"运行才 addTab + `hub.bus.publish`"整个撤掉，`battop_toggle()` 恢复成不需要 `events::Hub` 的单一函数，`renderBattop` 与 `tab.battop` key 删除、新增 `tab.other`。此后 battop 落点又变两次（§03am；2026-09-21 开关回「系统增强」+ 电池刺客二级 tab 仅运行时出现）；当前 `renderManage` 共五个二级 tab：基石与模块 / 模型管理 / 系统增强 / 电池刺客 / 实验室。

**真机验证**：`cargo test -p shelf-gateway` 全绿、`node --check app.js`、交叉编译+部署+健康检查（`active`/`NRestarts=0`）；curl 确认 summary 端点有真实数据。⚠ 「其他」tab 三级嵌套**逻辑推演过、`:scope >` 收紧过、装置健康**，但"浏览器里点开其他/KOReader 子标签，字体/词典切换正常"这个纯前端交互没有拿到用户肉眼确认——理论上没问题，但按项目纪律不能替代真人点一遍。

#### 本章坑位表（二）

> 网关网页 UI 与批量队列/闸门这一半的踩坑。前半章（A–D 早期 UI 与母版库）的坑见「本章坑位表（一）」；网关权威踩坑清单见 `gateway/docs/reMarkable网关白皮书.md` §04。术语：**SSE**＝服务端事件推送（服务变更时主动通知网页刷新，代替轮询）；**闸门**＝网关里限制同时处理几本书的并发/内存预算；**母版库**＝书架里永久保存原书的暂存池。

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见§ |
|---|---|---|---|---|
| 状态提示"一闪而过"，把延时 1.5s→3s 毫无效果 | 「推送本章/重新转写/提问」点完，结果文案立刻消失；改延时后用户复验仍闪 | 冲掉文案的不是 `wait()` 到期后的主动重画，而是后端发的 `entries` 事件经 SSE 触发的 `sec.refresh()` 整段重画，比计时器快得多 | 先查"重画到底从哪条代码路径触发"再动手；用 `holdRefreshUntil` 时间戳让 SSE 那条路让位 | 03au |
| 模块顶层 `const` 里直接写 `T('key')`，永远显示 key 兜底文本 | 不报错，肉眼难发现 | `I18N` 语言包是启动时异步 `fetch` 填的，顶层常量在脚本解析时就求值一次被"烤死" | `T()` 只能在渲染/交互时才执行的函数体里调；顶层字典只存 key 名；`FMT_TIERS`/`GUIDE`/`OPTTABLE` 改零参函数 | 03an |
| 多层嵌套 tab 互相串台 | 内层 `.subpanel` 被外层 `subtabs()` 一并抓走切显隐 | `querySelectorAll('.subpanel')` 会递归到所有后代 | `subtabs()` 用 `:scope >` 只认直接子元素，层数再深也不互扰（管理页现有四层） | 03am |
| 下拉放进"会重画的容器"，切时间窗后选择被重置 | 切下拉后时间窗弹回"今日" | 下拉跟内容一起被重画；重画不记得原激活项 | 跨时间窗持续存在的控件放在重画区域外；重画函数带 `activeIdx` 参数、重画前先读当前激活下标 | 03am |
| 长串报错撑宽 flex 卡片，页面横向溢出 | 模型卡"最近错误"里的 URL/JSON 片段没有空格，默认不换行 | flex 子项 min-content 尺寸被撑开 | `overflow-wrap:anywhere`（比 `word-break:break-word` 彻底，算 min-content 时也生效） | 03an |
| 只改用户点到的那一处 | 用户说"删除确认还是 alert"，其实是原生 `confirm()`，同类问题全站 9 处 | 同一模式散落各处 | 先 `grep` 全文件、一次修完 | 03bj |
| 批量状态放在浏览器标签页 JS 内存里 | 关掉/换设备打开页面，看不见队列也停不掉；分支合并后"加入 xochitl"对超限 PDF 一律灰 | 状态在标签页；另一个兄弟分支没有对方的改动 | 状态挪到网关进程（`batch.json` 落盘 + `budget` 的 pending/active 快照）；分叉分支先合并再修前端判断 | 03bl、03bp |
| 网关中途崩溃后重启形成"崩溃循环" | 某本书稳定触发崩溃，systemd 每次拉起都先重放它再崩，后面的书永远轮不到 | 重启续跑不区分"偶发中断"与"这本书就是元凶" | `Job.attempts` 落盘，同一项最多重放一次（`MAX_ATTEMPTS = 2`），超过记失败并跳过 | 03bp |
| 开机时 `book-serve` 起得慢，未完成队列被丢 | 重启后队列消失 | 旧实现等不到 book-serve 就直接返回，随后任何入队的落盘把磁盘上的旧队列覆盖 | 先把读回的队列装进内存再等；等不到也保留队列（不清空），待下次入队或手动"全部中止" | 03bp |
| 落盘顺序错乱把队列回退到过期状态 | 重启后读到旧快照 | 序列化在状态锁内、写文件在锁外，两个线程的快照可能倒序写盘 | 序列化+写盘放在同一把 `persist_lock` 里 | 03bp |
| "全部中止"并非所有步骤都能立刻停 | 单文件上传、PDF 优化点了停止没反应 | 这两步没有安全中断点，未 `mark_cancellable` | 如实回 `cancelled:false`；EPUB 优化（每处理完一个条目）、按卷拆分投递（份与份之间）才可中断 | 03bp |

### 03am｜电池刺客再拆分（二级 tab + 按进程）+ 总标题改「秘密花园」+ 端口改 443（2026-09-10，真机通）

同一天用户提了四条调整，并在同日追加了三条小调整。

**① 电池刺客第三次调整落点。** §03al 那版把时间窗+应用+唤醒源直接放进实验室卡片。用户这次要求：实验室只留开关和说明，打开后「管理」多一个二级 tab「电池刺客」，下面两个三级 tab（耗电情况、唤醒源）。

- `mountBattopCard` 拆成两个函数：
  - `mountBattopToggleCard`：纯 `<label class="toggle">` 开关，`checked` 直接对应 systemd 的 running 状态，`onchange` 打 `/api/enhance/battop/{start|stop}`。
  - `renderBattopDetail`：管理页的二级 subpanel，只在 `battop.running` 时显示。显隐写法照抄「笔记」tab「导入 md 文档」子标签那套：`hidden` 属性 + "当前若正停在被隐藏的标签上，先点回默认标签再设 hidden"。
- 现状（2026-09-21 又改过）：开关卡片挪到了「管理→系统增强」（`#enhBattopCard`），不再在实验室；「管理」subnav 现为五项——基石与模块 / 模型管理 / 系统增强 / 电池刺客（默认 `hidden`，`manageNav.children[3]`）/ 实验室。
- **嵌套到了四层**：管理 subnav → 电池刺客 subpanel → 耗电情况/唤醒源 subnav → 对应 subpanel → `renderBattopWindowed` 自带的时间窗 subnav → 时间窗 subpanel。`subtabs()` 之前已改成 `:scope >` 限定直接子元素（§03al 为「其他」tab 三级嵌套做的），这次直接复用，没有新踩坑。

**② 耗电情况补"按进程"。** `summary.json` 本来就有 `proc`（按 comm 分组）和 `app`（按 systemd unit/友好名分组）两个数组，§03al 只展示了 `app`；这次用同一个 `battopTopList` 渲染 `d.proc`，零新增数据处理。

**③ 总标题「书架」改「秘密花园」。** 只改**用户可见字符串**，不动仓库目录/crate/二进制名。改动点：

| 位置 | 说明 |
|---|---|
| `gateway/ui/locales/{zh-CN,en-US}.json` 的 `app.title` | 中文"秘密花园"，英文"Secret Garden" |
| `ui/index.html` 静态 `<title>`/`#applogo` | JS 加载完会被 `T('app.title')` 覆盖；改这里是让 JS 跑起来之前的瞬间文案也对 |
| `src/main.rs` 的 `ServiceSpec.label` | 经 `/api/services` 出现在「管理→基石与模块」列表，是第二处用户可见处 |
| `src/ui.rs` 的 `login_page`/`password_page` | Rust `format!` 拼的字符串，不走 SPA 的 `T()`，一并手动改，避免"主界面叫秘密花园、登录页还叫书架" |
| `style.css` 的 `.logo::before` | `"📚"` 换 `"🌿"` |
| `index.html` 新增 `<link rel="icon">` | 之前浏览器标签页图标为空，属顺手补齐；emoji 内联 SVG data URI，零外部请求 |

**④ 网关端口 8778 改绑标准 443。**

- 真机 `ss -tln` 确认设备上没有进程占用 443（`10.11.99.1:80` 是 xochitl 自己的 `/upload`，不冲突）；网关的 systemd 单元没写 `User=`（默认 root），绑特权端口无需额外提权。
- 只改**部署单元**的 `ExecStart --bind 0.0.0.0:443`；`main.rs` 里 CLI 的 `default_bind` **刻意留在 `0.0.0.0:8778`**（`gateway/src/main.rs:29`）——本地 `cargo run` 不用 root 即可测，部署路径靠单元显式 `--bind` 覆盖，两者不冲突。
- 连带改了安装脚本的探测/提示地址、`mdns.rs`/`ui.rs` 文档注释、README 三处地址。
- 路径现状：当时叫 `shelf/systemd/shelf-gateway.service`，现为 `gateway/systemd/gateway.service`（2026-09-11 随目录搬迁改名）；`deploy.sh` 已不在 `shelf/`，安装提示地址在 `shelf/install.sh`（"标准 443 端口，不用带端口号"）。
- 真机验证：`curl https://10.11.99.1/api/enhance/status`（不带端口）返回真实数据；`curl https://10.11.99.1:8778/...` 连接被拒（旧端口确实关了，不是留着旧进程）。

**当时的整体验证**：`cargo build --workspace`、`cargo test -p shelf-gateway`（当时 16 个）全绿，`node --check app.js` 通过；交叉编译全 workspace + 完整备份→部署→健康检查（9 服务 active，网关 MainPID 变化、`NRestarts=0`）；curl 确认新标题/图标/443 均生效（`<title>秘密花园</title>`、`/api/services` 里网关 `port:443`）。⚠️ **没有拿到用户肉眼确认**：浏览器里"管理→电池刺客→耗电情况/唤醒源"多级 tab 切换是否顺畅（与 §03al「其他」tab 嵌套同一条缺口），只做了理论分析 + curl 数据层验证。

**同一天追加三条小调整：**

1. 「耗电情况」的"按应用/按进程"改成下拉 `<select>`，不再两份列表并排。下拉放在时间窗 subnav 外面（跨时间窗持续存在，不随切时间窗重建）；新增 `battopActiveWindowIdx()` 读当前激活的时间窗下标，`renderBattopWindowed()` 加 `activeIdx` 参数，重画时原样传回——切下拉不会把时间窗弹回"今日"。
2. 「电池刺客」二级 tab 挪到「实验室」前面（按钮和 subpanel 一起挪，下标 `children[3]` 同步改），纯 DOM 顺序调整。
3. 用户指出详情页风格与别处不统一（内容"裸"躺在 subpanel 里）：每个时间窗内容包一层 `.card`（与 KOReader 字体/词典子标签各一张卡同一规矩）；耗电情况/唤醒源开头各加一张说明卡（标题+一句话）；"按应用/按进程"下拉放进耗电情况说明卡里（下拉不能塞进每次切时间窗都可能重画的区域，这条边界这次理清）；"还没有采样数据"的空状态也包 `.card`。

### 03an｜网页正文全量 i18n（传书/笔记/其他/管理四个 tab）+ 两处小样式修复（2026-09-10）

> i18n＝界面多语言。现状：`ui/locales/{zh-CN,en-US}.json` 各 **460** 个 key（本节落地时 437，此后随功能增长；`locale_files_have_identical_key_sets` 测试钉住两份 key 集合一致）。登录页/改密码页至今**仍未** i18n（Rust `format!` 拼字符串，`lang="zh"` 写死）。

**两处小样式修复：**

- **「模型管理」子标签拆卡。** 原来说明卡（h2+lead）把 `#modelcards` 整个包在里面，模型卡（视觉/文字两张）嵌在说明卡内部——是「管理」页里唯一的卡中卡，跟「传书·入库」「引导·基石」"说明卡起头、各功能各自平铺一张卡"的既有语言不一致。改成说明卡补 `<h2>` 标题、与 `#modelcards` 平级；纯 DOM 调整，`mountModelPanel` 内部逻辑一字未动。
- **模型卡「最近错误」行溢出页面。** `data-stat` 显示上游 API 报错原文，可能是没有空格的长串（URL/JSON），默认换行规则遇不到断点，把 flex 卡片撑宽。补 `overflow-wrap:anywhere`。

**主体：正文全量 i18n。** 用户要求"完成 shelf/notes/enhance 涉及的各前端功能页面的 i18n"，§03ae 当时"先只覆盖主界面外壳+顶层导航"的边界这次补完。规模比预想大：`app.js` 1023 行中 526 行含中文，散布在 10 个渲染函数/区块（传书、笔记、其它、管理台、模型管理、电池刺客等），大量文案深嵌在三元表达式/模板插值里（渲染自检徽章 title、按钮禁用原因、confirm/alert 弹窗）。

**跟用户确认的范围**：① 全量精细翻译到 title 提示语/弹窗/三元分支，允许顺手优化措辞；② 登录页/改密码页这次不做——未登录态不跑 JS、读不到 `LS`（localStorage）里的语言选择，要做需要另一套"未登录态也能传语言"的机制（如写 cookie 给 Rust 端读），工程量与风险面跟纯前端抽取不是一回事。§03ae"正文文案暂不迁移"的半句作废，"登录页/改密码页暂不迁移"的半句继续成立。

**设计要点：**

- `T(key, vars)` 加可选 `vars` 做插值（`{name}` 占位符，`split/join` 替换），零参调用行为不变。
- **最大的隐藏坑（症状→根因→修法→教训）：**
  - 症状：把文案塞进模块顶层 `const 模板字符串`（`FMT_TIERS`/`GUIDE`/`OPTTABLE`）后，永远显示 key 兜底文本，不报错、肉眼难发现。
  - 根因：`I18N` 是启动 IIFE 里异步 `fetch` 填充的；顶层常量在脚本解析时就求值一次，结果被"烤死"。此前仅有的 10 处 `T()` 全在函数体内（延迟执行），所以从没踩过。
  - 修法：三个模板改成零参函数（`FMT_TIERS()` 等），调用点加括号；`STYLE_NAMES`/`STATUS_NAMES`/`PROVIDER_NAMES`/`BATTOP_WINDOWS` 四个顶层字典只存 key 名，`T()` 查找挪到调用点；`DEST_ICON`（混了 emoji/SVG+文字）改惰性函数。
  - 教训：这条规则已写进 `app.js` `T()` 定义处头注，作为以后扩充 i18n 的硬性前提。
- key 命名延续扁平点号约定（不引入嵌套 JSON），按源码区块分命名空间：`transfer.*`（含 `GUIDE`/`OPTTABLE`/`FMT_TIERS`）、`notes.*`、`assets.*`+`koreader.*`+`wallpaper.*`、`other.*`、`manage.*`、`models.*`（含 `PROVIDER_NAMES`）、`battop.*`、`common.*`（`j`/`postJ`/`uploader`/`delBtn` 等跨函数复用的原子文案）。
- **明确不翻**：代码注释（项目约定保持中文）；登录页/改密码页；专有名词/技术字面量（文件扩展名、shell 命令本身、URL、`xochitl`/`KOReader`/`Obsidian`/厂商名——厂商名后括注的中文说明要翻，专名不翻）；`fmtB()` 的 `B`/`KB`/`MB` 单位；`shelf push` 示例文件名（装饰性，非语义字符串）。（`shelf push` 命令行 2026-09-18 已砍，这类示例文案是历史残留，见 §04 的 CLI 退役说明。）

**执行方式**：单个 feature 分支内按区块拆 6 个提交（i18n 架子+`transfer.*` → `common.*` → `assets.*`/`koreader.*`/`wallpaper.*`/`other.*` → `notes.*` → `manage.*`/`models.*`/`battop.*`），每个提交各自过 `node --check` + `cargo test -p shelf-gateway`（当时 16/16，含 `locale_files_have_identical_key_sets`），不攒到最后验证。

**离线验证**：每个区块改完，跑脚本核对函数体范围内残留中文（排除注释），确认用户可见字符串已清空；全文件收尾扫描：526 行含中文收敛到只剩 `/* */`/`//` 注释、两处永远不会被渲染的历史死代码（`TABS['note-serve'/'wallpaper-serve']` 的 `.title`，`titleKey` 优先级更高）、shelf push 示例文件名。

**真机验证（比 §03ae 更进一步，仍有边界）**：`sh build.sh` + `SHELF_NO_BUILD=1 sh deploy.sh 10.11.99.1` 部署（当时的脚本名，现已无 `deploy.sh`）；`curl` 拉取设备上实际 served 的 `zh-CN.json`/`en-US.json`（各 437 key，集合一致，抽查翻译正确）+ 首页 HTML；脚本把首页里全部 416 处字面量 `T('key')` 引用与设备 served 的 `zh-CN.json` 交叉核对，**零缺失**。这确认的是"部署到设备的 app.js 里每个翻译引用都能在设备实际提供的语言包里查到"，排除了部署/`include_str!` 编译环节出岔子。**没做**：浏览器里真正打开语言切换器、人眼确认切成英文后的排版/换行/对齐——只做到"数据链路+内容对照 100%"，视觉渲染仍待人眼确认，这条缺口沿用 §03ae 的挂到 §05，不重复开。

### 03au｜「整理」区状态提示停留时间 1.5s→3s，⚠️ 只是标，真根因是 SSE 抢跑重画（2026-09-16/17，真机通）

> 本节含一次"误诊→复验→真根因"，现行修法是 `holdRefreshUntil`，不是延长计时器。

**现象**：用户真机反馈「推送本章」/「重新转写」/「提问」点完弹出的状态文字（如"✓ md 已导出"）"闪一下就没了"。

**第一轮（误诊，未解决）**：这三处点击处理函数共用同一模式——显示结果文案 → `wait(1500)` → 整页重画（`renderBook`/`reloadBook`）。以为是 1.5 秒太短，改成 3 秒（3 处 `wait(3000)`）；`node --check` 通过，部署后 curl 核对首页确为 `wait(3000)`。用户隔天复验：提示仍"一闪而过"。

**真根因**：冲掉文案的从来不是 `wait()` 到期后的主动重画，而是另一条完全独立的路径：

1. 三个操作在后端各让对应服务发一次 `entries` 事件（`ink-serve`/`transcribe-serve`/`mind-serve` 各自 `bus.publish`）。
2. 笔记 tab 开着时，网页 SSE 订阅（`es.onmessage`，`app.js` 末尾 IIFE）一收到就立刻调 `sec.refresh()` 整段重画。
3. 这次重画和点击处理函数自己的 `await wait(3000)` 不同步，快得多（一个网络来回，通常远小于 1 秒），文案实际是被它冲掉的。

**修法**：`renderNotes(sec)` 新增 `holdRefreshUntil` 时间戳变量。三处点击处理函数在**动作一开始**（发请求前）写一次（防御性 15s 上限，防请求期间被抢跑），**结果文案刚显示那一刻**再写一次（`Date.now()+3000`，即"文案至少停留 3 秒"的真正窗口）；失败分支（如提问失败）置 0。`refresh()`（SSE 触发的重画入口）起手先看这个时间戳，没过就直接跳过——不会漏更新，因为点击处理函数自己那句 `await wait(3000); await reloadBook(...)`/`renderBook(...)` 本来就会在窗口结束后主动刷一次。纯前端改动，不改后端事件发布逻辑（服务该发事件仍发，网页只是不再被"自己刚发起的动作"的事件打断自己正在展示的提示）。

**教训**：与 §03am 那次"用户反馈还没修好才发现真根因"同一类——第一轮只对着"延时够不够长"一个可能性动手，没验证"到底是不是这个计时器冲掉的"；一查重画从哪条代码路径触发，就会立刻看到 SSE 那条快得多，延时改多大都没用。

### 03bj｜母版库删除确认改自定义弹窗，全站 9 处原生 `confirm()` 一并换（2026-09-19，真机通，✅已解决）

用户反馈"母版库中删除确认还是 alert"。严格说用的是浏览器原生 `confirm()`，不是 `alert()`（`alert()` 这一天早些时候已全站禁用改 `toast()`）——但对用户来说是同一类"弹出个与站内风格脱节、还阻塞交互的原生对话框"。不纠结用词，按同一纪律处理：`grep` 全文件查还有几处没换，一次修完，而不是只改用户点到的那一处。

**实现**：`gateway/ui/app.js` 新增 `confirmDialog(msg)`，返回 `Promise<boolean>`。

- 样式复用 `toast()` 已定下的 `--surface`/`--fg`/`--line`/`--shadow` CSS 变量约定：遮罩 + 居中卡片 + "是/否"两个按钮（`common.yes`/`common.no` 现成 i18n key，不新增）。
- 交互对齐原生 `confirm()`：点遮罩空白处 / `Esc` = 取消，`Enter` = 确认。
- 全站 9 处调用点全部改成 `await confirmDialog(...)`：母版库删除单条（`delBtn`）、批量删除、笔记回收站恢复全部/清空/重新摄取/条目"不要了"、模型 key 删除、管理台模块卸载/全部关闭领域服务。调用点原本都在 `async` 箭头函数里（多数被 `guardClick` 包裹），改 `await` 不需要额外包一层。

**验证**：`node --check` + `gateway` 17 测试全绿（当时数字）；部署后认证态 `curl` 首页，确认嵌了新代码、且没有裸的 `confirm(` 调用残留（`grep` 只剩中文注释里提到这个词）；`GET /api/books/staging` 确认服务端行为没变（纯前端改动）。**已知缺口**：弹窗本身"点是/点否/点遮罩/按 Esc"的真实交互还没过浏览器人眼确认，本机没有浏览器自动化工具。

### 03bl｜网关并发闸门分支合并 comic-pdf-optimize + 母版库批量 UI 三轮真机反馈修复 + 漫画 PDF 徽章补齐（2026-09-19）

> **⚠ 大部分已被取代**：① “漫画优化产出 PDF”已在 2026-09-20 被取代（换回 EPUB，见 §03bk 开头说明）；② 本节的批量 UI（前端逐项顺序提交、`batchAbort` 停止、`tooBig` 判断）已被 §03bp 的服务端批量队列 + 母版库页重做整体替换，前端不再有这些变量。**保留下来的要点**：分叉分支的合并教训、批量运行期间锁按钮/可见当前项/可停止这三条需求，以及"漫画 PDF 徽章"这条仍在的前端分支。闸门与队列现状见 §03bp。

主要是网关侧改动，完整经过+代码细节见 `gateway/docs/reMarkable网关白皮书.md` §04，本节只记跟漫画 PDF 线相关的部分。

用户真机点开 `feat/gateway-concurrency-budget` 分支部署的批量优化 UI，报了三条反馈：

| # | 反馈 | 排查/修法（当时） |
|---|---|---|
| ① | 点了批量按钮，单条按钮/其余批量按钮照样能点 | 批量运行期间锁住单条与其余批量按钮；"取消选择"从锁定列表摘出来常驻可点，运行期间变身"停止"，真能中断排队中的项目 |
| ② | 批量运行期间看不出在处理哪本 | 批量提交改逐项顺序处理，行内进度条即"当前在处理哪本"的指示 |
| ③ | 「加入 xochitl」对超限 PDF 一律灰掉 | 见下 |

**③ 的根因是"分叉的兄弟分支"**：该分支与 `feat/comic-pdf-optimize`（§03bk，PDF 分卷投递已真机验证）是同一个 master 提交分出去的兄弟，互相没有对方的改动。设备上实际跑的 `book-serve` 早就支持 PDF 分卷，但这条分支前端还按旧假设灰按钮。修法：合并 `feat/comic-pdf-optimize` 进这条分支（自动合并无冲突，workspace 测试全绿），再修前端 `tooBig` 判断（PDF 跟 EPUB 一样不提前灰按钮）。顺带修了一个真 bug：漫画转 PDF 后旧文件名残留在前端选中集合里变成"幽灵计数"。

**同一轮：漫画 PDF 徽章。** 用户追问"优化后直接变 PDF，已优化/未优化状态还有什么意义"。纠正前提（只有漫画 EPUB 才转 PDF，文字 EPUB 的优化徽章链没变）后确认一个真 gap：漫画转出的 PDF 与用户自己上传的原生 PDF 在列表里视觉上分不出来。后端 `list()` 早就用 PDF 头部/尾部标记正确区分（`it.level`/`it.optimized`），只是前端没用——补一条"已优化(漫画)"徽章分支，用户确认"别的 PDF 不动"，只改 PDF 格式且 `it.optimized` 为真时的展示。

- 现状：该分支仍在 `stgBadges`（`app.js:217`），文案 `transfer.staging.badge.comicPdf`＝"已优化(漫画)"，提示"漫画 EPUB 优化管线产出的 PDF…"；后端判据函数已泛化改名为 `pdfwrite::looks_like_own_bookconv_pdf`（原 `looks_like_own_comic_pdf`；2026-09-19 PDF 裁边产线接入后，裁边输出也会被判为"自产 PDF"，故此徽章现在可能标在非漫画的裁边 PDF 上，见文末存疑）。

**真机验证现状**：book-serve+网关（合并后那次）、网关（徽章那次）都已交叉编译部署，md5 校验通过、`systemctl restart` 后均 `active`/`NRestarts=0`。但停止按钮是否真能中断、当前处理项指示是否清楚、PDF 分卷按钮点了是否真能成功投递、"已优化(漫画)"徽章渲染是否正常，**都还没有用户独立复核过**；闸门本身"两本大书是否真被网关串行化 + `VmHWM` 不叠加"这条最核心的验证仍是老缺口，没有新进展。

### 03bp｜批量队列、并发/内存闸门与母版库页重做（2026-09-20）

> 主线摘要；实现与三轮真机反馈的完整记录在 `gateway/docs/reMarkable网关白皮书.md` §03b / §04，此处不重复。图：[`batch-queue.svg`](../../docs/diagrams/batch-queue.svg)（批量队列状态机）、[`budget-gate.svg`](../../docs/diagrams/budget-gate.svg)（闸门档位与三种结局）。

**为什么放网关**：`book-serve`（优化/加入 xochitl）、`koreader-serve`（加入 KOReader）、`gateway` 是三个独立进程，`book-serve` 的忙锁只按书名加、点不同的书互不阻塞；而网关是所有跨服务请求唯一的转发关口，进程内的锁就够，不需要跨进程锁/共享内存。

**关键常量（已对照当前源码核实）：**

| 项 | 值 | 位置 |
|---|---|---|
| 大/小档分界 | 体积 > 90MB 为大档（`LARGE_THRESHOLD_BYTES`，与 book-serve 的 `nativeUploadLimitMb` 缺省 90 只是数字巧合、语义不同：那边是"传不传得上 xochitl"，这边是"值不值得单独占大档名额"，不做跨进程配置同步） | `gateway/src/budget.rs` |
| 并发名额 | 大档同时 1 个；小档同时 3 个（`MAX_SMALL_CONCURRENT`），两档各自计数、互不占用 | `budget.rs` |
| 排队上限 | 30 分钟（`ADMIT_WAIT_TIMEOUT`），超时返回 503；重复书名 409；可取消 | `budget.rs` |
| 队列落盘 | `state/batch.json`（`paths.state_dir()`，设备上即 `~/.local/state/shelf/batch.json`），每次变化写一次 | `gateway/src/batch.rs` |
| 单书重放上限 | `MAX_ATTEMPTS = 2`：第一次中断放回队首重放，第二次仍没走完则记失败跳过 | `batch.rs` |
| 恢复时等 book-serve | 上限 30 分钟（`RESUME_WAIT_MAX`）：前 30 秒每 2 秒试一次，之后每 30 秒；等不到则队列保留在内存和磁盘、不再有人主动跑 | `batch.rs` |
| 轮询"一本书处理完"上限 | 1 小时（`SETTLE_POLL_TIMEOUT`） | `gateway/src/proxy.rs` |
| 网关接口 | `POST /api/batch {action, names\|all, folder}`、`GET /api/batch/status`、`POST /api/batch/stop`、`GET /api/budget/status`（`{pending, active}`）、`POST /api/budget/cancel {name}` | `gateway/src/main.rs` |

**服务端批量队列**（`batch.rs`）：

- `POST /api/batch` 入队，`action` 取 `optimize`/`deliver`/`koreader`；同一队列可混合三种动作。网关后台线程**顺序**逐本执行（设备双核，优化内部已并行处理图片）。
- 入队时校验资格：已优化的不再优化、非 EPUB/PDF 不加入 xochitl、没装 KOReader 不加入 KOReader，点名的不适用计入 `skipped`，同一本同一动作重复入队也跳过。
- 每一本先过闸门，再调对应服务；异步的优化/加入 xochitl 轮询到不忙才算完成，并读母版库里 `delivered.<kind>` 判成败；加入 KOReader 同步，成功后代记一笔 `staging/mark`（落库记录归 book-serve）。
- 状态落盘；网关重启后 `resume` 续跑：上次进行中的那本放回队首（受 `MAX_ATTEMPTS` 约束），并按最新母版库状态重新校验（已优化的不重做、不存在的剔除）。落盘序列化与写盘同锁，保证落盘顺序等于状态变化顺序。

**中途停止**（`POST /api/batch/stop`，界面叫"全部中止"）：清空队列；正在等闸门的取消排队；已在 `book-serve` 里的发 `POST /staging/cancel`——EPUB 优化每处理完一个条目检查、按卷拆分投递每份之间检查，终态 `cancelled`（不是 failed）；单文件上传与 PDF 优化没有安全中断点，如实回 `cancelled:false`（"无法中途停止"）。

**并发/内存预算闸门**（`budget.rs`）：每一本先 `admit(档位, 书名)`，拿不到名额就在条件变量上等，被唤醒后重新判断；拿到名额得到一个 RAII `Slot`，`Drop` 时自动释放。起因是真机测出优化/超限分卷投递的内存峰值约等于要处理的文件体积本身（如 245MB 源书峰值 206MB），而设备只有约 2GB 内存、systemd `MemoryMax` 没有真正生效，多本大书同时处理会线性叠加，有真实的 OOM（内存耗尽被杀）风险。刻意不做"连续字节预算求和"的精细模型——没有足够数据给出各操作/书籍类型的内存倍率，强行量化是假精确，直接做成两档。

**母版库页重做**：见第 D 章「现状结论」。用户汇总要求的取舍：行内不放按钮（单条"加入"与"勾选后加入"重复）；PC 与手机同一套单列；"已完成"筛选替代原"清理已落库"。

**没验证**（原样保留）：批量"加入 xochitl / 加入 KOReader"两条在设备上没有端到端实测（代码路径与优化相同，只是端点不同）；新界面的真实触屏交互；暗色模式截图对照。网关白皮书 §03b 另记：闸门"两本大书是否真被串行化且 `VmHWM` 不叠加"这条最核心验证从未独立做过。

## 第 E 章 稳定性、内存与耗电

> **大白话导语**
> 这一章讲"书架服务怎么在一台只有约 2GB 内存、电池有限的设备上稳定跑"。设备上真出过整机内存耗尽（OOM，内存用光后内核随机杀进程，可能殃及 xochitl 阅读器本体）和耗电异常，所以本章大部分内容是"某次事故 → 根因 → 修法 → 教训"。
> 几个术语：**book-serve** = 管母版库（书架里永久保存原书的暂存池）的后台服务；**边车** = 每本书旁的隐藏 JSON `.<书名>.delivered`，记录这本书最近一次优化/落库/渲染自检的结果；**VmHWM** = 进程从启动起的内存峰值（`/proc/<pid>/status`），比瞬时采样更可信。
> 建议读法：先看下面的"现状结论"和示意图，再看 §03ba、§03bh、§03bi（三次内存事故，一环扣一环），然后 §03bm（耗电）、§03bq（可靠性）；§03p、§03ab、§03ag、§03ai 是代码去重重构的记录，只想知道"现在代码怎么组织"的话读它们的"现有结构"即可。

> **现状结论**
> - 设备只有约 2GB 内存，`systemd` 的 `MemoryMax` 实测不生效（memory 控制器没进 `system.slice`，`book-serve.service` 里的 `MemoryMax=192M` 至今形同虚设），所以内存要靠代码约束：**流式优化**（`optimize_epub_file_streaming`）、**流式落库**（`upload_file`/`text_profile_file` 不整本进内存）、**单图 900 万像素上限**（`imgopt::MAX_DECODE_PIXELS`）、**图片并行的 600 万像素预算**（`imgpool::PIXEL_BUDGET`，至多 2 个 worker）、**网关并发闸门**（`gateway/src/budget.rs`：>90MB 大档同时 1 个、小档同时 3 个，见 §03bp）。
> - book-serve 的后台任务用 `catch_unwind` 兜住 panic，前提是 release 用 `panic="unwind"`（原先是 `abort`，兜底完全无效，2026-09-19《镖人》踩过）；忙锁与取消标记合并为 `OpRegistry`；边车更新加全局锁防丢更新；启动时把被中断的 `pending` 修正为失败/超时并清 `.optimizing.tmp` 半成品（§03bq）。
> - 耗电：`Staging::list()` 的"已优化/已加入"判定按（大小,mtime）缓存 + `BufReader` 读 zip 中央目录，CPU 约降 70 倍；wifi-watch 链路正常时零 fork（§03bm）。
> - 孤儿边车：新书不继承同名旧边车，启动时清理孤儿边车。
> - 定内存阈值的铁律：**先真机/本地实测 VmHWM 再定，不靠"字节数乘法"估算**（§03bi 第一版阈值就是这么翻车的）。

![内存防线：从上传到落库每一段怎么压住峰值，以及单图像素阈值的实测依据](diagrams/e-oom-guards.svg)

#### 本章坑位表

| 坑 | 症状/判据 | 根因 | 教训/规避 | 见§ |
|---|---|---|---|---|
| 内存版优化整本读入 | 552MB《镖人》优化 VmRSS 冲到 1.4GB+，可用内存探底 ~25MB | 全部条目（含图片）进内存 | 两阶段流式，峰值 = 一张图 + 全书文字 | §03ba |
| `MemoryMax=192M` 不生效 | 上面 1.4GB 没被任何机制拦住 | `cgroup.subtree_control` 为空，memory 控制器没启用 | 内存约束必须写在代码里，不指望 cgroup | §03ba |
| 落库不拆分路径叠 3 份 | 估算峰值 ~180–270MB | `fs::read` 整本 + 自检解压全部条目 + `upload` 内部再克隆 | `upload_file` + `text_profile_file` 流式 | §03bh |
| 单图解码无像素上限 | 《乱马》VmHWM 271MB；第一版阈值后《火影》仍 262MB | 解码前只读宽高；阈值按"3 字节/像素"估，真实开销 3–5 倍 | 阈值按实测定：900 万像素 | §03bi |
| 临时产物没带点前缀 | `....epub.optimizing.tmp` 被列成 `format:"other"` 条目 | 流式优化跑分钟级，暴露过滤缺口 | 半成品一律点前缀 | §03ba |
| 列表每次开 zip | 3 秒轮询，10 本×60MB 单次 0.2s CPU | 裸 `File` 小读风暴 + 无缓存 | `BufReader` + （大小,mtime）缓存；新增轮询字段必缓存 | §03bm |
| wifi-watch 高频 fork | 一小时子进程 16s CPU，是 book-serve 空闲 3 倍 | 每 15s fork 一串命令 | 链路正常只读 sysfs `carrier`（零 fork） | §03bm |
| powersave 判据永不成立 | 每次开机白改并 `con up` 断线重连 | `nmcli -g` 回 `disable`，脚本比的是 `"2"` | 接受 `2\|disable\|"2 (disable)"` | §03bm |
| 同名新书继承旧边车 | 新书一入库就显示"已加入" | 书被外部清掉后边车成孤儿 | 落地前清旧边车 + 启动 GC | §03bm/§03bq |
| `panic="abort"` 下 `catch_unwind` 无效 | 一次 panic 摔掉整个进程 | release profile 是 abort | 改 `panic="unwind"` | §03bq |
| 重启后界面永远"处理中" | 边车停在 `pending` 但没线程在跑 | 进程被 OOM/崩溃/断电打断 | 启动 `recover_interrupted` | §03bq |
| `cargo fmt --all` | 64 个文件被重排进提交 | 仓库 Rust 从不走 rustfmt | 不跑 fmt，手写对齐 | §03ab |
| 分卷静默失效 | 母版库是 297MB/188MB 整本 PDF，投原生被断连 | 旧 host 分卷缺依赖时静默 `return [path]` | 失败必须显式报错，不静默回退成整本 | §03p |
| "字节对了"≠xochitl 认 | 《疯探》修了 dtb:uid、DOCTYPE 仍无目录入口 | 真根因在别处（§03bc） | 闭源组件每版修复都要真机点开看 | §03ba |

### 03p｜代码质量核查一轮：去重 / 复用 / 解耦（2026-09-04，用户"合理用设计式避免重复、复用、解耦"）

> 这是重构记录。基座 crate 后来正名为 `rmsvc-core`（2026-09-11 搬到仓库顶层，原 `shelf-core`）；本节里的电脑端 CLI（`host/` Python）已在 2026-09-18 整体砍掉，相关小节仅作历史。

三个只读审查 agent 分片扫（6 服务 / 基座 crate / host CLI），主线自核 shell/systemd/Cargo。**结论**：基座是干净的 port/adapter（把"做什么"和"怎么做"分开的接口/适配器结构），抽象真在复用（同一 HTTP 栈、单一密码哈希、单一路径表/语言），债务集中在几处明确复制 + 一处被绕过的抽象。行为保持不变，离线门槛全过后真机部署冒烟坐实。

#### 现有结构（当前代码仍在）

| 模块（`rmsvc-core/src/`） | 收编了什么 |
|---|---|
| `config.rs` | `load_or_default` / `load_or_seed`（首启写缺省、损坏文件不覆盖）/ `save`（原子 + 可选 0600）；替代 book/font/gateway/wallpaper 四处各写一份的 config 读写，顺带把非原子的"原地 write"统一成原子写 |
| `fs.rs` | `write_atomic`（`<path>.tmp`→rename）+ `set_mode`；registry / fonts.json / 所有 config-save 共用 |
| `multipart::receive_part_to` | 单点化"建文件 + io::copy"，`AssetUploadFlow` 用它落盘（各自再定空文件/归档语义） |
| `asset::all_ok` | 统一 `!empty && all(ok)` 回执判据；扩展名门由各 `AssetStore::allowed_ext()` 给出，**空列表 = 接受任意扩展名**（`koreader-serve` 的 `KO_ANY`，对齐 KOReader books「原样」） |

**koreader-serve 收编被绕过的抽象**：`KoStore`（`services/koreader-serve/src/koreader.rs`）实现 `AssetStore`，books/fonts/dicts 三条上传路统一走 `AssetUploadFlow`，删掉两个近乎相同的手搓 multipart 循环。`install` 走 `dest/.<name>.part`→rename，"KOReader 扫目录不见半成品"的保证扩展到字体/词典。取舍：`AssetUploadFlow` 先落中央暂存再 install，跨文件系统时对大书是两次拷贝，接受此微增 I/O（书经 KOReader 多为中小文件）。**客户端回执契约不变**（仍 `{file,ok,message}`）。

**死代码清除**（生产零调用者）：`auth::random_password`、`tls::ensure_self_signed`、`xochitl::content_type_of`（book-serve 走 `bookconv` 派生 MIME）、`paths::cache_dir`。均已不在当前代码里。

**评估后跳过（避免过度抽象）**：各服务 `status()` 字段本就领域各异不强统一；三种语言的路径表有意各写一份（host 不链接 Rust）；systemd 4 个 loopback 单元近全同但声明式模板化收益低；大范围 `pub`→`pub(crate)` churn 大收益小。

**验收**：`cargo test` 全绿 + shellcheck 0 + aarch64 交叉编译干净；真机（固件 3.27.3.0）5 服务重启 0 NRestarts、日志无 panic，`gateway.json` 落盘 0600 无 `.tmp` 残留，koreader-serve 真上传落盘正确、回执契约不变、无 `.part` 残留。

#### 分卷静默失效（2026-09-05，历史，教训仍有效）

**症状**：用户报"上传失败 … Connection reset by peer"。母版库里的 PDF 是 297MB / 188MB **整本**，投原生时 xochitl 日志 `multipart body is too large`（14:32:44 / 14:33:15）并直接断连。
**根因**：旧 host 的 `pdfsplit.split` 缺 pymupdf 时 `except ImportError: return [path]` 静默不分卷。
**修（当时）**：分卷挪到 `host/calibre/pdf_split.py`，切不了抛 `CalibreError` 不推，回执写明原因。补救：host 用修好的 splitter 切 5+4 卷经 API 入母版库。
**被取代**：host CLI 已砍，现在按卷拆分由 `bookconv::comic_split` + `book-serve` 的 `try_deliver_split` / `try_deliver_split_pdf` 完成；"xochitl 上传上限 <188MB（60MB 稳）"是当时的粗略结论，**已被精确实测取代：100,000,000 字节（十进制 100MB）起 HTTP 413**，`nativeUploadLimitMb` 缺省 90（见 §03bn 与 `services/book-serve/src/config.rs`）。
**教训**：兜底 fallback 不能静默地把"失败"变成"原样继续"。

### 03ab｜代码体检与重构（2026-09-06，六项闭环后）

> 重构记录。原文里 host CLI、`install.sh` 迁移块两类改动对应的代码已在 2026-09-18 之后整体砍掉（见 §03p 开头说明），下面只保留仍成立的结构结论。

用户："如果没有值得新增的，就优化代码：设计模式、解耦、删失效代码"。先全量扫（死 pub 项 = 非测试代码零引用；重复小助手；过期注释），结论是**结构本身已经对**——端口/适配器、`AssetStore` 模板、`ServiceSpec` 模板（`rmsvc-core/src/service.rs`）、事件总线汇聚都在；当时能改的是四类零碎，各开一支 `--no-ff` 并入（其中 host、install.sh 两类已随 CLI 砍除）：

1. **基座 / 服务**：新 `clock` 模块（`now_secs`/`now_ms`/`now_nanos`/`secs_of`）收编 8 处手写的 `SystemTime::now().duration_since(UNIX_EPOCH)`；删死项（`paths::data_root`、`htmlproc::has_internal_anchor`、`optimize::is_current_version` 等，均已不存在）；`xochitl.rs` 找文件夹/找文档两处各扫一遍 `.metadata`，合成 `metadata_entries` + `is_live`；book-serve 落库记录边车（`Delivered`/`RenderCheck`/读改删）从 500 行的 `staging.rs` 拆成 `sidecar.rs`（Repository，仓储模式：读写落盘只经它），`staging` 与 `render_check` 只通过它落盘。现在 `staging/` 已是目录（`mod.rs`/`intake.rs`/`optimizing.rs`/`deliver.rs`/`library.rs`）。
2. **网关 UI**：484 行 Rust 原始字符串里嵌 CSS+JS → 真文件 `gateway/ui/{index.html,style.css,app.js,auth.css}`（另有 `locales/`），编译期 `include_str!` 拼成单页（网页仍零外链），CI 加 `node --check gateway/ui/app.js`（`.github/workflows/ci.yml`）。
3. **host / install.sh**：已随 host CLI 砍除，略。

**没动的**（评估后认为不值得或有风险）：`bookconv::convert` 整族；四个服务各自一行 `GET /events` 路由（塞进 `service::run` 反而把总线所有权搞乱）；`multipart.rs`/`http.rs` 体量大但职责单一。**踩坑**：中途 `cargo fmt --all` 把 64 个文件重排进了提交，回滚重放——本仓库 Rust 从不走 rustfmt，别跑 fmt。

### 03ag｜消掉 book-serve 两个队列的重复：新增 `PendingQueue<T>`（2026-09-09，离线）

三维审计发现 `trash.rs`（原生回收站队列）和 `mkdir.rs`（原生建文件夹队列）逐行重复：同样的 `{file, lib_dir, lock}`、字节级相同的 `load()/save()`（读 JSON、原子写）、"校验→查重→push→save"的 `add()`、"按 `.metadata` 状态过滤→剔除已完成项→save→返回(剩余,剔除数)"的 `pending()`。与笔记线 `ChapterStore<T>` 是同类重复。

**现有结构**：`services/book-serve/src/pending_queue.rs` 的 `PendingQueue<T>` 只抽持久化 + 入队去重 + 剔除这层通用外壳（`new(file)` / `list()` / `add(exists, make)` / `prune(keep)`）；领域校验留在各自包装里。**现在有三个使用者**：`TrashQueue`（`trash-pending.json`）、`MkdirQueue`（`mkdir-pending.json`）、`ComicMargins`（`comic-margins.json`，后来新增的漫画页边距待办，见 bookconv 优化白皮书 §20）。

**保留原则**：
- 只抽"形状相同"的外壳，领域校验（uuid 形状/文件夹名合法性/是否已存在）不合并；
- `add()` 的 `exists`/`make` 两个闭包共用同一个参数：保持 `&str`（`Copy`），先转 `String` 再共享会撞借用检查器（一借一移），已写进 `trash.rs` 注释；
- 查重与写入在同一把锁内完成（`make` 拿到锁之后才调用），避免两个并发请求都通过查重各插一条。

**离线**：新增 2 测（入队去重 + 跨实例持久化、剔除后无变化不重写），原有测试全绿；`cargo clippy -p book-serve` 无新增警告；`sh build.sh` 交叉编译 aarch64-musl 零警告。纯内部重构，无真机行为变化。

### 03ai｜`shelf-core::registry` 新增 `SvcClient`/`enc`（2026-09-09，离线，代码改在这边、消费方全在 notes 那条线）

> 命名更新：本节写作时基座叫 `shelf-core`，现为 `rmsvc-core`（`rmsvc-core/src/registry.rs`）。

三维审计发现 notes 三个服务（`mind-serve`/`note-serve`/`transcribe-serve`）各自的 `ink.rs`（访问 ink-serve 的 HTTP 客户端）、`note-serve::trash.rs`（访问 book-serve 回收站队列的客户端）——四处 `struct{paths,agent}`、`new()`、`base()`、`get_json()`、`enc()` 几乎逐字节相同。`registry` 本来就管服务发现，加一层"按发现结果建客户端"是同一职责的自然延伸。

**现有结构**：`SvcClient::new(paths, service, timeout_secs)` + `base()`（未运行时统一报"`<服务名>` 未运行"）/ `get_json()` / `post_json()` + 逃生舱 `agent()`（要下载原始字节不是 JSON 时用，如 `transcribe-serve::crop()`）；后来又加了 `post_json_value()` / `try_post_json()`（返回结构化的 `SvcError{status,message}`，非 2xx 时读出对方错误体里的人话原因）。`registry::enc()` 是 `multipart::percent_encode` 的薄封装。

**使用者现状**（原文"shelf 自身零处调用"已过时）：notes 线的 `mind-serve`/`note-serve`/`transcribe-serve`/`ink-serve`，以及网关的 `gateway/src/proxy.rs`、`gateway/src/batch.rs`（批量队列轮询 book-serve 状态）。`shelf/` 目录自己的服务（book-serve/koreader-serve）仍不调用。

**保留原则**：只抽传输样板，各服务自己的业务 trait（`EntryStore`/`TrashSink` 等）、方法签名、错误文案不变——合并它们会把不同服务的语义耦合在一起。跨线原则：笔记线的架构决策记在笔记白皮书（`notes/docs/reMarkable笔记白皮书.md` §03ai，消费方视角），基座为接住它们改了什么记在这里，两边字母巧合相同但不是同一节。

**离线**：新增 2 测（`svc_client_base_url_uses_registry_and_errors_with_service_name_when_not_running` / `enc_percent_encodes_path_segments`），零回归、clippy 无新增警告、交叉编译零警告。

### 03ba｜真机内存危机+《疯探》目录入口最终根因：流式优化 + 剥 toc.ncx 外部 DTD 引用（2026-09-19，真机通）

> **状态说明**：本节的**流式优化**结论至今有效（并在 2026-09-20 加了图片并行，见下"现状"）。但《疯探》目录入口这条线（dtb:uid、外部 DTD 两处修复）**不是最终根因**——后续 §03bb 证明这两处都不是根本原因，§03bc 反编译 xochitl 才坐实真根因（xochitl 硬编码查 manifest `id="ncx"`，不走 `<spine toc>`）。`wash::strip_ncx_doctype` 保留为无害的去外部依赖清洗（`wash/ncx_fix.rs`）。

用户看完 §03az 后提两件事：① 疯探目录还是没有；② 超限书籍能否"拆一章优化一章"，怕整本一起优化会 OOM。

#### 内存事故：症状 → 根因 → 修法

**症状**：用真机母版库里一本 552MB 的 EPUB《镖人（套装共11卷）》触发一次真实优化，`VmRSS` 几十秒内冲到 **1.4GB+**，系统可用内存从 ~950MB 探底到 **~25MB**；抢在真 OOM 之前手动重启 book-serve 叫停（原文件走 `write_atomic` 临时文件模式，没被写坏）。

**根因**：
1. 内存版 `optimize_epub_with` 把整本 zip 的全部条目（含占绝大部分体积的图片）读进内存。
2. 顺带查出 `book-serve.service` 里 `MemoryMax=192M` **从来没生效过**：设备 systemd 没把 memory 控制器代理进 `system.slice` 子树，`cgroup.subtree_control` 为空。当时那 1.4GB 完全没被任何机制拦住，继续涨大概率触发**全系统级** OOM（内核会挑内存最大的进程杀，可能殃及 xochitl 本体），不是"book-serve 自己被杀、干净重启"这种可控失败。

**"拆一章优化一章"字面做不到**：跨文件脚注回链、自动目录、目录分部重建、`dtb:uid` 同步、空页清理都要先看完整本书结构。但内存大头几乎全是图片字节，文字结构信息本来就小。

**修法**：新增 `optimize_epub_file_streaming`（路径进路径出，`optimize/streaming.rs`，现为构建器 `StreamingOptimize` + 旧签名薄封装）：

| 阶段 | 做什么 | 内存里有什么 |
|---|---|---|
| 阶段一 | 只把非图片条目（html/css/opf/ncx/字体）整份读入并做清洗；图片条目只记名字、字节留空占位（清洗层与漫画识别只看 html 文字和 `<img>` 标签引用，从不需要图片真实字节） | 全书文字部分 |
| 阶段二 | 按处理好的顺序重新遍历写出：非图片条目直接用阶段一结果；图片条目才从源文件按需读回、处理、立刻写进直接落盘的目标文件 | 当前在处理的图片 |

峰值 ≈ "一张图 + 全书文字"，不随书体积线性涨。共享函数 `first_pass_html` / `transform_html_chapter` / `transform_image_bytes`（`optimize/html_pass.rs`）让内存版和流式版共用同一份业务逻辑，对拍测试证明逐字节一致。`Staging::optimize()` 与 CLI `epub-optimize` 都走流式；内存版保留给测试/小书。

**真机复测**：同一本 552MB《镖人》，`VmRSS` 全程 **8–53MB**，173 章全部处理完，579MB → 785MB（漫画路径高质量重编码，体积涨是预期）——从触发 OOM 到解决同一天内。

**现状（2026-09-20 起）**：阶段二加了图片并行——至多 2 个 worker（设备双核 A55）、提前读 `workers+2` 张原图、在处理图片像素总和受 `imgpool::PIXEL_BUDGET`（600 万像素）限制，输出按原条目顺序写、与逐张顺序处理逐字节相同（`imgpool.rs` 文档：单线程处理一本 350 页漫画约 285 秒）。所以"峰值 = 一张图"现在应读作"至多几张图、且总像素有上限"。

**附带 bug**：临时产物命名一开始没带点前缀（`....epub.optimizing.tmp`），真机复现过它被 `GET /staging` 当成一条 `format:"other"` 的母版库条目——流式优化要跑分钟级，才暴露了这个过滤缺口。补点前缀（复用边车的隐藏命名约定），真机验证过。

#### 《疯探》目录入口（历史，已被 §03bb/§03bc 取代）

用户测新投的《疯探》仍没有阅读器右上角的目录入口。当时先发现设备上攒了 3 份同名文档（用户测的极可能不是最新那份），经用户明确授权后按具体 uuid 删掉两份旧的（这是首次真正执行"删设备文档"）；清理后仍无入口，对照正常的《雪人》发现 `toc.ncx` 带外部 DTD 的 DOCTYPE（"番茄小说 EPUB Generator"产物特有），假设 xochitl 联网取 DTD 卡住，新增 `wash::strip_ncx_doctype` 剥掉。**事后证明 dtb:uid 与 DOCTYPE 都不是根因**（见上方状态说明）。

**教训**：**"字节层面看着对"不等于"xochitl 真的认"**。对没有源码可查的闭源组件，每一版修复都得真机点开看一眼才能收尾，不能靠猜第二个假设就向用户宣称"应该好了"。

### 03bh｜OOM 排查：`Staging::deliver()` 落库不拆分路径是唯一未修的真实风险；顺带 Rust/前端代码质量去重（2026-09-19，真机通，✅已解决）

用户要求"核查目前最有可能发生 OOM 的服务，确认是否立即优化或拆分"。三路并行审计（OOM 风险面 / Rust 后端重复代码 / 前端 `app.js` 重复代码）覆盖 `book-serve`、`koreader-serve`、`bookconv` 全部上传/优化/落库路径。

**结论**：
- **`book-serve` 是唯一有实质 OOM 历史和现存风险的服务**；`koreader-serve` 投递全程 `fs::copy`，纯 OS 层流式，不占用户态内存。
- 按卷拆分投递（`try_deliver_split` / `comic_split::deliver_split_streaming`，§03ba 已改流式）与 `/staging/upload` multipart 落盘（`rmsvc_core::multipart` 本来就流式落盘）都确认不是风险。
- **不拆服务**：`bookconv` 拆独立服务上一轮已定案不拆；这次 OOM 是函数级内存管理问题（同一份数据在内存里叠好几份），拆服务解决不了，反而多一层 IPC 序列化成本。

#### 唯一未修风险：症状 → 根因 → 修法

**根因**：`Staging::deliver()`（`services/book-serve/src/staging/deliver.rs`）落库"不拆分"这条路（≤90MB 的 EPUB/PDF，`native_upload_limit_mb` 缺省 90）同一时刻三份数据在内存：
1. `std::fs::read(&p)` 整本读进 `Vec<u8>`；
2. `bookconv::stats::text_profile(&data)` 内部 `read_entries` 把 zip **全部条目（含图片）**解压进 `Vec<Entry>`（自检只用 OPF/HTML 文本，图片解压出来即弃，纯浪费）；
3. `Xochitl::upload` 内部再克隆一份拼 multipart body。

估算峰值 ~180–270MB，与 §03ba 的 552MB 事故同类；而 `MemoryMax=192M` 从未生效，不能指望它兜底。

**修法**（三处，行为不变）：

| 文件 | 改动 |
|---|---|
| `rmsvc-core/src/xochitl.rs` | 私有 `send_multipart` 接 `impl Read + body_len`，用 `Cursor(头).chain(body).chain(Cursor(尾))` 流式发送，显式设 `Content-Length`（`ureq` 设了就不退化成 chunked，线上字节与改动前逐字节相同）；公开 `upload(&[u8],…)` 签名不变（内部包 `Cursor`）；新增 `upload_file(path,…)` 流式开文件直传。三个既有调用点（book-serve 拆分份 / note-serve 推笔记本 zip）零改动，各省一次内部克隆 |
| `bookconv/src/stats.rs` | 新增 `text_profile_file(path)`：直接开文件当 zip 按条目遍历，`wants_entry` 先看条目名，图片等非 OPF/HTML 条目连解压都不做；逐字符统计抽成私有 `accumulate()`，内存版 `text_profile(&[u8])` 与流式版共用，差分测试断言两入口结果完全相等 |
| `staging/deliver.rs` | `deliver()` 非拆分路径删掉 `std::fs::read`，自检走 `text_profile_file(&p)`，上传走 `upload_file(&p,…)` |

**真机验证（VmHWM，不是估算）**：host 用 Python 合成一本 80MB 测试 EPUB（20 张 4MB 随机字节图片，模拟图片主导的插画书，接近 90MB 上限但走不拆分路径），scp 进设备母版库，真机 `POST /staging/deliver` 投递成功（`status:"ok"`，`render` 自检 `expected:46/pages:48` 吻合，证明 `text_profile_file` 端到端正常）。`book-serve` 的 `VmHWM` **投递前后全程 3.2–3.5KB 量级**（自 3484 kB 起就没变过）——不是"峰值更低"，是这条路径对 80MB 文件几乎零内存开销。
**已知验证缺口**：`note-serve` 共用了改动的 `xochitl.rs`，重启后 `/status`/`/books` 基础功能正常，但设备上没有真实笔记本数据可做完整推送链路复测——如实记录，不是"验证过"。

**顺带发现、当时判"暂不处理"**：`imgopt.rs` 单张图片解码没有像素数上限（中置信度的边缘风险，无真实触发样本）。**几小时后就撞上了真实样本，见 §03bi。**

#### 代码质量去重（同一轮顺手做，行为不变，`cargo test --workspace` 全绿）

后端：
- `staging/mod.rs` 的 `spawn_bg()`：`spawn_optimize`/`spawn_deliver` 原来各自手写"起后台线程 + `catch_unwind` + `end_busy` + `bus.publish`"（约 20–25 行同构），现只保留这层外壳；业务体（调 `optimize`/`deliver`、写哪个边车字段）留在各自 `body` 闭包，业务级 `catch_unwind`（转成带具体原因的 `Err`）也保留在闭包里，因为两处 panic 文案不同。不引入 `rmsvc_core` 通用泛型任务框架——`koreader-serve` 没有同类异步操作，为假设中的未来需求设计属于过度抽象。
- `busy_err(name, extra)` 合并三处（优化/落库/删除）逐字重复的"《{name}》正在处理中，请稍候"忙锁提示。

评估后**不做**：`OptimizeCheck`/`DeliverCheck` 合并成泛型（故意分开）；bookconv 各处 zip 打开模式（重复的只是测试样板）；`try_deliver_split` 的逐份进度写入（与终态写入语义不同，硬抽更难读）。

前端 `gateway/ui/app.js`（现路径）：
- 新增 `el(tag,attrs,children)` DOM 构建 helper 与 `renderStepProgress(container,{label,prog,msg})`（`{done,total}` 真百分比 / 无数据不确定态滚动条），后者也用于笔记「整理」页"推送本章"按钮；`stagingList` 局部 `btn()` 的禁用/复位逻辑委托给全局 `guardClick`。
- **范围限定**：不对其余手写 DOM 做机械替换；`uploader()` 的字节级真实进度（连续字节 vs 服务端步数是两种语义）不强行统一；瞬时操作的启停按钮不套进度组件。

**前端验证**：`node --check app.js` + 静态审读 + `cargo test --workspace`（`gateway` 17 测试，含 `include_str!` 骨架测试）；用户给了网页登录密码后，经网关代理（`POST /login` 拿 `shelf_session` cookie）原样走一遍网页按钮背后的请求序列——入库 → 优化 → 轮询到 `optimize.status:"ok"` → 落库（不拆分路径）→ `render` 自检数字吻合 → 加入 KOReader（`adopt` + `mark`）→ 删除清理，响应 JSON 形状与 `app.js` 读取字段一一对上；测试产物均已清理，四服务 `NRestarts=0`。**仍没做**：本机没有浏览器自动化工具，**没验证 DOM 渲染出来的像素/交互观感**（按钮是否真变灰、进度条动画是否真跑），这一层只是静态审读的结论。

### 03bi｜§03bh 顺带发现"没有真实触发样本"的 `imgopt` 无像素上限，几小时内真机撞上（2026-09-19，真机通，✅已解决）

> 一句话：单图解码要设像素硬上限；**上限必须按实测定**。第一版 2500 万像素靠"3 字节/像素"估算，形同虚设；改按实测 **900 万像素**（`imgopt::MAX_DECODE_PIXELS`）。

#### 第一轮：撞上真实样本（2500 万像素版）

**症状**：用户随手投递一套《乱马1/2》漫画（第 9～16 卷，走按卷拆分流式路径）。翻真机 journal + `.metadata` 时间戳（不是听用户转述），投递期间 `book-serve` 的 `VmHWM` 冲到 **271MB**，比 §03bh 那条路径（几 KB）高好几个数量级，也明显高于拆分投递设计目标"峰值 ≈ 单份体积（≤ 90MB）"。

**根因**：先排除嫌疑——`deliver_split_streaming` 只"读原始压缩字节→组包→上传→丢"，不调用任何 `imgopt` 解码。真正的元凶在用户前一步"优化"：`optimize` 的 `transform_image_bytes` 对漫画页调 `trim_margins` + `downscale_for_epub_comic`（内部 `downscale_into_q`），解码前只用 `header_dims` 读宽高判断"要不要处理"，**没有"这张图本身大到不该整个解出来"的硬上限**。

**修法（第一版）**：`imgopt.rs` 新增 `MAX_DECODE_PIXELS = 25_000_000`（约 5000×5000，估算"真实扫描页极少超 4000px 长边，留 1.5 倍余量"）+ `within_decode_budget(w,h)`，在解码前加 guard，超限返回 `None`——三个函数对调用方本来就是"`None` = 原样保留"的语义，天然兜底，不是新错误路径。真机验证：25 页合成漫画（满足 `comic_detect::is_comic`），第 12 页造 6500×8000（5200 万像素）：`VmHWM` 2844 kB → 12164 kB，该页原封不动、其余 24 页正常缩到 630×910。

#### 第二轮（同一天几十分钟后）：第一版阈值本身估错了

**症状**：用户又优化上传一本真实漫画（《火影忍者》第 17～21 卷），`VmHWM` 冲到 **262MB**——与修复前 271MB 几乎同量级，第一版修复没有真正压住峰值。

**根因**：阈值按理论估算"2500 万像素 × 3 字节 ≈ 75MB"定，完全没抓住真实开销。本地 release 编译、忠实复刻 `optimize` 真实调用链（`trim_margins(bytes)` → `downscale_for_epub_comic(&trimmed)`），实测 `VmHWM`：

| 像素数 | 400 万 | 870 万（A4 300dpi） | 1600 万 | 2500 万 |
|---|---|---|---|---|
| 实测峰值 | 62MB | 97–109MB | 164MB | 230–236MB |

实际约 **9–16MB/百万像素**，是"3 字节/像素"估算的 3–5 倍（`image` 库解码 + `to_rgb8()` + resize 中间缓冲多份同时存活）。2500 万像素本身的真实峰值（230MB+）和"不设上限"的事故峰值（271MB）几乎一样高——**新阈值等于没起作用**。

**修法（定稿）**：`MAX_DECODE_PIXELS` 改 **900 万像素**（约 3000×3000，覆盖 A4 300dpi 及绝大多数真实漫画/书籍扫描页），真机峰值约 100–110MB，比 262MB 低一个数量级。当前代码 `within_decode_budget` 在 6 个解码入口统一 guard（`imgopt.rs`：`downscale_into_q` / `pad_to_device_aspect` / `trim_margins` / `prepare_comic_page_for_pdf` / `decode_trim_comic` / `dither_bilevel`；第一版只有前述三处，后续新增入口也补了）。真机复现：合成 25 页漫画，一页 4000×4000（1600 万像素，旧阈值下会撞 ~164MB 级峰值），走真实 `/staging/optimize`：`VmHWM` 2908 kB → 12260 kB，无尖峰，超限页原封不动、其余页正常处理。回归测试 `imgopt::tests::oversized_image_skipped_by_all_decode_entries` + `within_decode_budget_boundary`（900 万像素边界）。已重新部署（备份 `book-serve.bak.pre-pixel-cap-recalibrate`），四服务健康。

**后续演进**：2026-09-20 加图片并行后，`imgpool::PIXEL_BUDGET = 600 万像素` 限制"同时在处理的图片总像素"（按头部声明像素申请额度，单张超过总预算独占全部额度；`imgpool.rs` 文档按 ~12 字节/像素估，600 万像素 ≈ 70MB），最坏峰值不高于原先单线程处理一张 900 万像素图。

#### 教训

1. **"没有真实触发样本"的保质期很短**：§03bh 记下的"理论边缘风险"几小时内就被用户随手一次真实操作撞上。下次翻日志核查，要主动带着这类"暂不处理"的记录去对照，别等用户第二次反馈。
2. **定内存阈值前必须先实测**（哪怕只是在不同规模下跑一遍看 `VmHWM`），不能靠"字节数乘法"拍脑袋——`image` 这类做大量内部缓冲/中间态转换的库，真实开销可能是理论值的好几倍。第一版"加了上限"表面完成、实际没压住峰值，差点让用户撞回原问题。
3. 同类的"阈值要按实测数据定"还有裁边比例：`imgopt::TRIM_MAX_FRACTION` 由 0.15 改 **0.35**（真机《镖人》版权页实际单边留白 22%–29%，抽样 43 张真实页最大约 28.6%，旧阈值在 15% 强行停手裁不干净；两边独立累加最多 0.7×边长，仍留 30% 给内容）。

### 03bm｜耗电与日志全面核查：列表轮询、wifi-watch、孤儿记录（2026-09-20，真机通，✅已解决）

**起因**：用户反馈"book-serve 耗电远超其他"，并要求核查全部日志。

**方法**：battop 数据按小时拆（`enhance/battop` 的 `samples-*.tsv`，口径 = 各服务累计 CPU 毫秒）；设备 `/proc/<pid>/stat` 的 utime+stime 前后差量测稳态；`journalctl` 按单元/来源去数字聚合。

**结论与修复**（症状 → 根因 → 修法）

1. **book-serve 空闲不耗电**：夜间小时 0.2–0.5 s/h；9/19 全天 15211 s 是反复跑优化测试。图片解码/缩放/编码的 CPU 是正当负载。
2. **真问题一：列表判定每次开 zip**。`Staging::list()` 对每本 EPUB 开两次 zip 读中央目录，`optimized_version_file` 用裸 `File`（每条目十几次几字节 read 系统调用），批量优化时前端每 3 秒轮询。设备实测 10 本×60MB 单次列表 0.2 s CPU。**修**：套 `BufReader`（`optimize/marker.rs`）+ 按（大小,mtime）缓存 `probe_level` 结果（`staging/library.rs` 的 `ProbeCache`，列表时顺带清掉已不存在条目的缓存）→ 冷 <0.01 s、热 ~0.003 s（约 70 倍）。
3. **真问题二：wifi-watch 每 15 秒 fork 一串命令**（rfkill/nmcli/grep/head/cut），开机一小时子进程累计 16 s CPU（≈0.46%），是 book-serve 空闲的 3 倍。**修**：链路正常时只读 `/sys/class/net/wlan0/carrier`（shell 内建 `read`，零 fork），只有 carrier≠1 才走原慢路径；固化频段/省电只在 carrier 0→1 跳变与每 `RECHECK`（40×15s = 10 分钟）兜底时做。稳态 120 s 由约 37 ticks 降到 4 ticks（≈9 倍），主机用桩 `nmcli`/`rfkill` 覆盖常态/假死/接口 down/跳变四场景。脚本与单元在 `packaging/wifi-watch/`（`wifi-watch.sh` + `wifi-watch.service`）。
4. **顺带抓到的 bug：powersave 判据永不成立**。`nmcli -g 802-11-wireless.powersave` 回文字 `disable`，脚本拿它跟 `"2"` 比 → 永不相等 → 每次开机/服务启动都白改一遍并 `con up` 断线重连 WiFi（日志 9/17–9/20 每次都有）。**修**：接受 `2|disable|"2 (disable)"`，重启服务后不再重连。
5. **孤儿边车**：`.<书名>.delivered` 在书被外部清掉后残留（设备上 28 个），同名新书会**继承旧的"已加入/渲染"记录**。**修**：`stage_new`/`stage_from_path` 落地前 `sidecar::remove` 目标名旧边车；启动时 `gc_orphan_sidecars()` 清孤儿（`staging/intake.rs`、`staging/library.rs`）。
6. **渲染自检噪声**：投完就删书时自检线程写结果失败，刷"母版库里没有这本书"日志。**修**：书已不在则静默跳过（`render_check.rs`）。

**核查后确认无需处理**
- 内核 warn（iw61x CMD_CANCEL、regulator of_node 等）是这颗 SoC 常见硬件噪声；OOM 只出现在 9/17（OOM 架构修复之前）。
- xochitl 日志 `Opening and ending tag mismatch`、`Images/cover.jpg` 找不到：《绝叫》每章 `<head>` 里有 `<img src="../Images/cover.jpg"/></div>*/` 残留，是**原书自带的损坏模板**（原书 `cover.jpg` 本就不存在，真封面是 `cover00224.jpeg`），不是优化器引入；xochitl 容错，页数/翻页正常。DuoKan 字体 `@font-face` 指向不存在的 ttf 同理。
- `rm-sync: Local immutable file changed`：直投/封面无损修补替换了磁盘上的 EPUB，云同步察觉；设备未走云同步，不影响。
- `cangjie-backups` 2.0 GB（170 份历次部署二进制备份），/home 剩 40 GB，不处理。

**后续（同日）：清洗层去掉无效 `<img>` 和 `@font-face`**（`wash::drop_dead_refs`，`wash/dead_refs.rs`，`WashReport.dead_refs_removed`）
- 规则：`<img src>` 指向书内不存在的文件 → 删该标签；有实际替代文字（非空且不是我们封面转换写的 `cover`）的留着；远程/`data:`/大小写差异一律保留。`@font-face` 里死 `url()`（书内缺失或 `res:///sdcard|opt/...` 设备路径）：全死且无 `local()` → 整条删；有 `local()`/活 url → 只剔除死 `url()`（连 `format()` 和逗号），`local()` 候选保留。
- 顺序：清洗在远程图内联之前，内联出来的新本地图不会被误判；流式路径的图片是空占位但条目名齐全，判定只看条目名，安全。
- 真书验证：《绝叫》33 处无效封面 `<img>` 清零、唯一有效封面保留；《罗杰疑案》DuoKan 死 `url()` 清零、88 个 `local()` 候选保留；两本正文（204281 / 134772 字）**逐字一致**；设备上 book-serve 优化产物字节数与主机一致。
- 未提升 `OPTIMIZE_VERSION`：这两类只是 xochitl 日志噪声，已优化的书不必因此被标"旧版"；下次重新优化时自然生效。（当前版本号 `"15"`，见 `optimize/mod.rs`。）

### 03bq｜book-serve 可靠性：panic=unwind、OpRegistry、启动恢复（2026-09-20）

现有结构一览：

| 机制 | 位置 | 作用 |
|---|---|---|
| `panic = "unwind"` | `shelf/Cargo.toml` 的 `[profile.release]` | 后台线程 `catch_unwind` 兜住 panic（损坏书触发的越界等），失败只影响这一本；原先 `abort` 下 `catch_unwind` 完全无效，一次 panic 摔掉整个进程、卡住所有在途操作（2026-09-19《镖人》真机踩过）。代价只是二进制略大 |
| `OpRegistry` | `book-serve/src/ops.rs` | 把原先三把互相独立的 `Mutex<HashSet>`（忙锁/可取消/已请求取消）合成一张 `HashMap<条目, OpState{cancellable,cancel}>`：每个操作一次持锁；"结束 = 整条移除"不会漏清标记；`try_start` 已忙则拒绝（不排队不覆盖）；`request_cancel` 三态（`Ok(true)` 已登记 / `Ok(false)` 这一步无法中途停止 / `Err` 没在处理）；取锁容忍 poison（`ops::lock`）。同一条目的「优化」与「落库」互斥 |
| 取消协作 | `OpRegistry` + 优化/拆分投递的检查点 | 只有 EPUB 优化（每处理完一个条目）和按卷拆分投递（每份之间）登记 `mark_cancellable`；单文件上传、PDF 优化没有安全的中断点，不登记 |
| 边车全局锁 | `sidecar::update` 里的 `static WRITE` | 优化进度回调、渲染自检线程、HTTP 线程会并发改同一份边车，`write_atomic` 只保证不写一半、不保证不丢更新（A 读→B 读→A 写→B 写），一把全局锁足够（边车都很小） |
| 启动恢复 | `Staging::recover_interrupted`（`staging/library.rs`，由 `State::ensure_dirs` 调用） | 进程被打断（崩溃/OOM/systemd 杀/断电）后：边车里 `pending` 的优化/落库记录改 `failed`（否则界面永远"处理中"）；渲染自检 `pending` 改 `timeout`；`.<书名>.optimizing.tmp` 半成品（可达数百 MB，点前缀列表看不见）删除 |
| 孤儿边车 | `stage_new`/`stage_from_path` + `gc_orphan_sidecars()` | 新书落地前清目标名的旧边车；启动时清没有对应书的边车 |
| 列表探测缓存 | `Staging::list()` 的 `probe_level` | 按（大小,mtime）缓存 + `BufReader` 读 zip 中央目录（§03bm） |
| 渲染记录补记 | `backfill_render_records()`（启动时） | 给"已加入 xochitl 但没有渲染记录"的书按书名 + 大小认领文档补记 |

忙锁是**进程内存态、不落盘**：重启 = 没有任何操作还在跑，"忙"天然清零；边车里的 `status` 只管"上次结果展示"，不参与忙判断。

## 第 F 章 设备、字体壁纸与固件

> **大白话导语**
> - 这一章讲"书架这套软件怎么和 reMarkable 设备本身打交道"：上传字体、换休眠壁纸、固件（系统）升级后怎么恢复、WiFi 为什么老掉线，以及踩过的几个和设备系统有关的大坑。
> - 几个名词：**xochitl** = reMarkable 官方的阅读器/界面主进程；**xovi** = 第三方"扩展加载器"，让我们能往 xochitl 里挂扩展；**qmd** = 一份对 xochitl 界面文件（QML）的补丁，由 `qt-resource-rebuilder`（qrr）在 xochitl 启动时注入；**OTA** = 设备在线升级固件；**font-serve / wallpaper-serve** = 书架里管字体、管壁纸的两个后台服务（源码在 `enhance/`）。
> - 建议阅读顺序：先看下面的"现状结论"和"xochitl 怎么重启（速查）"，遇到具体症状查"本章坑位表"，需要来龙去脉再读各 §。§03c/§03f 是字体壁纸的首版与首轮真机；§03k/§03bd 是字体的两次事故；§03o/§03v 是 xovi 持久化与固件升级；§03w/§03x 是休眠屏与 WiFi；§03at 是单元并存事故。

> **现状结论**
> - 只支持 reMarkable Paper Pro Move、固件 **3.28.0.172**；换固件后功能需要重新安装一遍（`/home` 数据保留，`/usr` 与 `/etc` 会被冲掉）——安装/恢复流程见 `docs/INSTALL.md`（OTA 恢复的**权威说明**在那里，本章 §03v 只留实录）。
> - 字体（`font-serve`，8792）与壁纸（`wallpaper-serve`，8793）"上传即可用"；壁纸写 xochitl 的 `SleepScreenPath` 隐藏键，bind-mount 方案已退役；字体菜单靠 qmd 注入（`shelf/xovi/font-menu-dynamic.qmd`），删字体后菜单不刷新曾是坑（§03bd，已修）。
> - WiFi 连上恰好 60 秒必掉的真凶是 cfg80211 regdomain 宽限（精简 regdb 的 CN 无 5150–5350，路由 5G 信道 36 被判非法）→ 连接锁 2.4G；`wifi-watch` 常驻看护脚本与单元在 `packaging/wifi-watch/`（2026-09-20 起零 fork 快路径，是 `install-all.sh` 的一个步骤）。
> - 两个隐蔽事实：`shelf` 安装器已内建"清旧命名遗留单元"（`shelf-gateway.service`，§03at 的根治）；设备上 appload 现要求 ≥ 0.6.0（KOReader 入口现状见 `shelf/koreader/README.md`）。

#### xochitl 怎么重启（速查）

改了 qmd / 扩展 / 壁纸键之后常要重启 xochitl。**先判断 xovi 是否已在运行中的 xochitl 里生效**（`LD_PRELOAD` 含 `xovi.so`；脚本里是 `devlib.sh` 的 `cj_xochitl_has_xovi`）：

| 情况 | 做法 | 为什么 |
|---|---|---|
| xovi 已生效 | `systemctl restart xochitl` | drop-in 保持，xovi 不丢 |
| xovi 没生效（刚开机、OTA 后、`/etc` tmpfs 被清） | `/home/root/xovi/start` | 它现场写 `/etc` 的 drop-in 再重启 xochitl |
| xovi 已生效却跑了 `xovi/start` | **禁止** | 它会 umount 重挂 drop-in 目录，运行中的 xochitl SEGV，`OnFailure` 触发整机自动重启（2026-09-20 真机事故） |

重启前先告知用户（会打断阅读），短时间别连续重启（`StartLimitBurst=4`/10 分钟，见 `docs/INSTALL.md` 风险③）。`shelf/install.sh` 只落盘 qmd、不自动重启，末尾按上表打印该用哪条命令。§03f 与 §03bd 里"restart 会丢 xovi、要用 xovi/start"的旧表述，只在"xovi 当时本就没生效"的场景成立，以本表为准。

#### 本章坑位表

| 坑 | 症状/判据 | 根因 | 教训/规避 | 见§ |
|---|---|---|---|---|
| 字体删了菜单里还在 / "换字体不生效" | 删字体后菜单仍列出，选它没反应；日志 `SHELF-FONT: … removed=0` | 3.28 版 qmd 用真 `ListModel`，只 `append` 从不 `remove` | `cjSync()` 先删后加；新增删除路径要单独测 | 03bd |
| 中文方框 + 选的字体不生效 | `fc-match sans-serif:lang=zh` 落到 Noto Sans；所选字体被盖 | 旧中文化套件写死的 fontconfig：回退指向已删字体 + `strong` prepend 抢在所选字体前 | font-serve 接管 fontconfig，全 `weak`、按覆盖率排回退链 | 03k |
| xochitl 重启后 xovi 没了 / 反过来 SEGV | 新进程 maps 里 `xovi.so` 为 0；或 `xovi/start` 后整机重启 | `/etc` tmpfs 重启即清；已生效时再跑 `xovi/start` 会摧毁运行中进程 | 按上表先判状态再选命令 | 03f · 03bd · 03o |
| 安装器选错固件版本 qmd | 3.27 机装了 3.28 锚点 | `/etc/version` 是 build 号（如 20260612…），不是语义版本 | 读 `/usr/lib/os-release` 的 `IMG_VERSION` | 03f |
| qmd 整份不应用 | 日志 `expected item assignment value token, got Some(Symbol('('))` | qmldiff 解析不了 `({})` 对象字面量与不带花括号的 `if (` handler | 用 `[]` 与 `{ … }` 块；改前用 asivery/qmldiff CLI 离线 `apply-diffs` 实跑 | 03v · §04 |
| 休眠壁纸"不轮换" | 按了多次电源键，journal 无 `PM: suspend entry` | 充电/USB 连着时按电源键只画休眠屏、内核不 suspend，systemd-sleep 钩子不跑 | 轮换改由 wallpaper-serve 读 `journalctl -f -u xochitl` 的 `DeepSleep to Normal` 触发 | 03f |
| 休眠键改了不生效 | 写了 `SleepScreenPath` 仍显示原图 | `isettings.sleepScreenPath` 只在 xochitl 启动时读 | 首次写键要重启一次；之后换图只覆盖文件 | 03x |
| WiFi 连上恰 60 秒掉线 | `iw event` 见 `disconnected (local request)`；wpa/NM 无断开事件 | cfg80211 `REG_ENFORCE_GRACE_MS`=60 s：精简 regdb 的 CN 不含 5150–5350，路由 5G 信道 36 被判非法 | 连接锁 2.4G（`band=bg`），或路由 5G 改 149–165；regdb 有签名换不了 | 03w |
| "时不时连不上" | 不插 USB 时空闲几秒就断 | 内核 autosleep 深度休眠，WiFi 随之断，摸屏才恢复 | 要长时间可达就插 USB；排查先 `journalctl -b \| grep "suspend entry"` | 03w |
| 时钟从不同步 | `timedatectl` 一直 `synchronized: no` | chrony 默认 4 个 `time*.google.com` 国内不通 | 改底层 `/etc/chrony.conf`（`packaging/chrony-cn.sh`）；OTA 后被冲回 | 03w |
| 两个网关单元并存，真机跑旧二进制 | `shelf-gateway.service` NRestarts=4517，报 `Address in use` | 网关改名后旧单元的 `.wants` 链接没清，两个单元抢 443 | `install.sh` 内建 `SHELF_LEGACY_UNITS` 清理；改名类重构必须同时写迁移清理 | 03at |
| busybox 里中文文件名显示 `?` | `ls`/`grep` 看中文名全是问号 | busybox 终端 locale | 验证中文名用 `find \| hexdump` + md5，别信 `ls` | 03f |
| 改 `xochitl.conf` 泄露凭证 | 文件含 DeveloperPassword/UserToken | — | 只动目标行、tmp+rename、首次留 `.shelf-bak`；任何路径不打印行内容 | 03x |
| 备份留在 `extensions.d/` | 崩溃循环 | xovi 把目录下任意文件当扩展加载 | 备份放 `~/cangjie-backups/`（项目铁律） | 见 `docs/INSTALL.md` |

### 03c｜Phase 2 字体/壁纸上传即可用（2026-09-03，离线完成）

> 本节描述首版设计；**下方"现状"表以当前代码为准**——首版里的 KOReader 镜像、"内建三项"、`source` 字段、bind-mount 子命令都已删除（§03x、§03k）。

**两个服务的现状**（`enhance/font-serve`、`enhance/wallpaper-serve`，均挂网关、注册表驱动网页 tab）：

| | font-serve（8792，tab「xochitl」） | wallpaper-serve（8793，tab「壁纸」） |
|---|---|---|
| 存储 | `FontStore: AssetStore`；落 `$XDG_DATA_HOME/fonts/`（fontconfig 用户字体目录） | `WallpaperStore`；池 `~/.local/share/shelf/wallpapers/pool/`，`current.png` 同级；状态 `~/.local/state/shelf/wallpaper-state.json` |
| 上传校验 | 扩展名 ttf/otf/ttc + 魔数（拒非字体） | jpg/jpeg/png，≤15MB，魔数 |
| 入库动作 | 落盘 → `fc-cache -f` → `fc-scan --format '%{family}'` 取家族名（无 fc-scan 自解析 `name` 表）→ 重写 `~/.local/share/shelf/fonts.json` 与 fontconfig | `fit_to_screen`（cover/contain）缩成 **954×1696** RGBA PNG 入池；首张上传自动激活 |
| 数据形状 | `fonts.json` = `{version:1, fonts:[{key, files[], names{cn,tw,en}, cjkPct, fontconfigRef}]}`，**一个家族一项**，无 `source`/"内建" | 状态 `{mode: sequential\|random\|fixed, current}` |
| 删除 | `DELETE /{family}` 删整个家族全部文件，**所有字体都可删** | 删当前拒绝；`PUT /current`、`PUT /mode` |
| 其它 | `PUT /config {emboldenCjkFallback}`、`GET /events`(SSE)；**不碰 KOReader**（KOReader 字体由 koreader-serve `POST /fonts` 单独管，用户 2026-09-03 定两边各自装） | `roll`：按 mode 轮换；`activate` 用 `truncate(true)` 原地写 `current.png` **保 inode**（单测断言） |
| 单元 | `MemoryMax=192M`，`CPUWeight=20`，`Nice=5`；网关 `ExecStartPre` 补跑 `fc-cache`（fontconfig 索引在 tmpfs、真重启后丢） | 同左 |

屏幕常量 954×1696 现在是 wallpaper-serve 自己的 `W/H`（2026-09-11 起从 `bookconv::imgopt` 复制而来，不再路径依赖；屏幕规格若变要两处分改）。

**字体菜单 qmd**：`shelf/xovi/font-menu-dynamic.qmd`（3.28 锚点 `Control#root > ColumnLayout > ListModel#fontModel`，真 `ListModel` 逐项 `append`）与 `font-menu-dynamic-3.27.qmd`（3.27 锚点，`Repeater` 内联数组 model，整体重赋值并按"内容签名"`cjLastSig` 判是否刷新）。两版都读 `fonts.json`，且**受 `reading-qol.json` 的 `fontEnhance`（设置页「阅读字体增强」）门控**；`fonts.json` 不存在则不追加（没有"内建三项"回退）。菜单每次可见（`onVisibleChanged`）做差量同步。`shelf/install.sh` 读 `IMG_VERSION` 主次号选版本、落到 `qt-resource-rebuilder` 目录、被替换的旧文件备份进 `~/cangjie-backups/shelf-<时间>/`，**不自动重启 xochitl**（见上"速查"）。

**本机冒烟（首版）**：真 TTF 上传→家族名 `LXGW WenKai`→fonts.json→删除 ✓；两图→954×1696 ✓；首张自动激活、设当前、模式、删当前拒 ✓；`roll` ✓。

**当时待真机验证的三个假设**（结论见 §03f）：S-A 传字体后不重启，`epub.setFontName(新家族名)` 渲染是否认；S-B `onVisibleChanged` 是否每次开菜单触发；S-C `FontLoader` 备选（未用）。

### 03f｜真机首轮（2026-09-03，固件 3.27.3.0 build 20260612，全程 WiFi 10.42.0.224）

**部署**：`rm-ssh-over-wlan on` 后 WiFi SSH 通；`shelf/deploy.sh` 一次装齐当时的五服务（dm-verity 未激活，单元写 /usr）。

**验证结果**
- **P1 通**：KOReader 投递中文名+子目录，`find|hexdump` 字节与 md5 一致（⚠ busybox `ls`/`grep` 看中文名会显示 `?`，别信）；native 投递 4s，"已优化（3 章，27734→5077 字节）；已导入"，书库根出现《书架真机测试》（xochitl 用 EPUB `dc:title` 作 visibleName，不是上传文件名；`library` 文件夹不存在按设计回落根）；`uploadReachable=true`。
- **P2 字体通**：11MB 真 TTF 上传 20s，`fc-list` 认出 `LXGW Neo XiHei Screen Full`，新 qmd 加载后 `SHELF-FONT327: onCompleted model=8 rows=4`（4 原生 + 内建 3 + 上传 1，首版设计）。**S-A 通**：菜单选新字体 → `.content` fontName 变 → scp 回 `<uuid>.pdf`，`pdffonts` 嵌入 `LXGWNeoXiHeiScreenFull CID TrueType`——**上传→菜单→渲染全程免重启 xochitl**（只 fc-cache），`restartNeeded=false` 定案。**S-B 通**：再传第二个字体后重开菜单，`SHELF-FONT327: visible model=9 rows=5`，`onVisibleChanged` 差量追加成立，"菜单每进程只建一次"的限制被绕开。3.28 版 qmd 同机制，3.28 机验证见 §03v/§03bd。
- **P2 壁纸（bind 时代）通**：两图缩成 954×1696，首张自动激活，`roll` 保 inode(2184) 且 `suspended.png` 真身 md5 随之变。此方案已被 §03x 取代。
- **P3（KOReader 配置同步）通**：`pull` 拉三份配置，校正 profile 后 `diff` 零差异。⚠ 该 host CLI 2026-09-18 已砍（现走 koreader-serve `/config/*`，见 `shelf/koreader/README.md`）。

**真机纠出的 4 个 bug（已修）**：① `/health` 被 `GET /{name}` 通配抢先（`service::run` 改前置注册 + `Router::merge`）；② 安装器按 `/etc/version` 判固件版本，那是 build 号，选成 3.28 锚点——改读 `/usr/lib/os-release` 的 `IMG_VERSION`；③ 旧 `add-reading-fonts-3.27.qmd` 与新 qmd 并存重复追加——旧文件移备份；④ KOReader profile 三处臆测值按快照修正（`floating_punctuation=1`→`true` 等）。

**⚠ 事故与恢复**：为验字体菜单跑 `systemctl restart xochitl`，新进程 maps 里 xovi.so/qrr/appload 全 0——这台重置机之前重启过、`/etc` tmpfs 里的 xovi drop-in 已清，没装 `xovi-reenable.service`。恢复 = `/home/root/xovi/start`（自带 restart），15s 后 xovi 4 段/qrr 5 段/appload 4 段、NRestarts 不增。教训：**重启 xochitl 前先核 `grep -c xovi.so /proc/<pid>/maps`（或 `LD_PRELOAD`）**，按"速查"表选命令。**当时用户决定（2026-09-03）暂不装 `xovi-reenable.service`**——此决定后被取代：现 `install-all.sh` 有 `xovi-persist` 步（§03o）。

**壁纸唤醒轮换根因（用户反馈"没有轮换"）**：14:50–14:53 按了 7 次电源键，journal 只有 xochitl `Changing display state from Normal to DeepSleep / DeepSleep to Normal`，**零条 `PM: suspend entry`**——设备充电/USB 连着时按电源键只画休眠屏、内核不挂起，systemd-sleep 钩子永远不跑（之前"真 suspend"的记录是拔线状态）。**定案**：轮换由 wallpaper-serve 内 `journalctl -f -u xochitl -n 0 -o cat` 阻塞读（`wake.rs`），匹配 `Changing display state from DeepSleep to Normal` 触发，充电与否都通；放在唤醒而非入睡时轮换，是为避开和 xochitl 画休眠屏抢时序；空闲零唤醒。真机通（14:55）：按电源键唤醒瞬间 `唤醒 → 轮换到 wp-tall.png`。**同时修的小坑**：调试误起第二个 `--bind 127.0.0.1:1` 实例顶掉活服务的注册 → `register()` 改为同名活 pid 拒绝覆盖。

**qmldiff 观察**：主进程只在启动时 "Iterating/Loading/Processing"；此后 `rm.worker.unix` 渲染 worker 每次渲染文档会再 "Loading file" 一遍（日志前缀是 `<uuid>.pdf`），不代表主 UI 重新注入。

### 03k｜字体两 bug 根因与修复（2026-09-04 凌晨，用户报"第二次上传字体不生效 + 中文字体在书里是方框"）

**症状 → 根因**（两个 bug 同一根因）：shelf 服务端本身没问题（两次上传都落盘、fonts.json 与 fontconfig 都认得）。设备 `~/.config/fontconfig/fonts.conf` 是**中文化套件**（块②，2026-08-23）留下的，把中文回退**写死**成 `LXGW Neo ZhiSong Screen Full` + `HanaMinB`，且对所有 zh 文本 `mode="prepend" binding="strong"`。
- **方框（bug2）**：用户把那两个写死的字体删了，回退链指向不存在的家族 → `fc-match sans-serif:lang=zh` 落到 **Noto Sans**（无 CJK 字形）。临时把 LXGW 装回，方框立刻消失（反证）。
- **不生效（bug1）**：`strong` 的 zh-prepend 把写死字体名**抢在用户所选字体前面**，所以选新字体后中文仍走回退。
- 附带：用户留的「方正FW筑紫明朝」只覆盖中文基本区 **35%**（7374/20902），美术字体，当正文必缺字。

**修法（方案 A，用户拍板"shelf 接管回退"）**：font-serve 每次上传/删除（`write_index`）**动态重写** `~/.config/fontconfig/fonts.conf`（首行带标记"shelf font-serve 自动生成"，据此判断能否安全重写）：
- 把界面/书籍的中文回退指向**当前已装**的中文字体（覆盖率降序），全部 **`binding="weak"`**：阅读器 `setFontName(所选字体)` 是 strong 家族、永远排最前 → 选谁是谁（修 bug1）；只有所选字体缺的字才字形级回退（修 bug2，只要装了任意中文字体就不豆腐）。`sans-serif/serif/monospace` 三个 generic 也 prefer-weak 指向它们（界面/笔记）。
- 首次接管前把原配置备份到 `~/.config/shelf/fontconfig-fonts.conf.pre-shelf.bak`（只备一次）。
- 覆盖率：`ttf::han_coverage_pct` 解析 cmap（format 4/12）数 U+4E00..=U+9FFF；**< 8% 不入回退链**（`CJK_MIN_PCT`，滤纯拉丁字体），**< 80% 上传回执警告**（`CJK_LOW_PCT`，"正文会缺字/方框"）。fonts.json/list/status 带 `cjkPct`，回执带 `fallback` 回退链。
- 3.27 版 qmd 菜单刷新判据从"按 model 长度"改为"按内容签名"（`cjLastSig`），删+加净零也刷新。
- **embolden**：见 §03o——现已内建并默认开（首版曾"未保留"chinese-ime 的 e-ink 加粗补偿，后补回）。

**真机（2026-09-04，WiFi）**：只剩 FZFW 时 `fc-match sans-serif:charset=4e2d`（含「中」）→ FZFW（逐字回退），`FZFW:lang=zh` → FZFW（所选不被盖）；装回 LXGW/京華（各 100%）后回退链 `[100% 字体…, FZFW]`，FZFW 上传回执报"覆盖率仅 35%"警告。**注意**：`fc-match sans-serif:lang=zh`（不带字符集）仍显示 Noto Sans——那是 pattern 顶配、非渲染真相；带字符集才是逐字渲染落点。qmd 判据修复需重启 xochitl 生效，fontconfig 修复实时生效。

### 03o｜xovi 持久化架构修正 + embolden 默认开 + 管理台愿景（2026-09-04，用户驳回 batch4 耦合）

**用户批评**：batch4 让 `shelf/install.sh --with-xovi-reenable` 装、`deploy.sh`/`package.sh` stage 外层的 `cangjie-xovi-reenable.service`——**shelf 直接耦合回外层代码/旧路径**，违背"网关+领域服务、独立可插拔、借鉴外层但不引用"；且 reenable 会重注入**整个 xovi 栈**，是 **xovi 层**通用持久化，不是 shelf/中文化专属。

**技术前提（Explore 坐实）**：xovi 是**进程级全量加载**（启动时 preload 扫 `extensions.d` dlopen 全部 .so，qrr 读全部 qmd，一次性）。**没有"只重载单个扩展/qmd"的机制**，任何增删都要全量重启 xochitl。所以"每服务各自 xxx-xovi-reenable"技术上不成立：N 个 = 开机重启 N 次，xochitl 有 watchdog+StartLimit，变砖面变大。

**改动与现状**
1. **撤回耦合**：`shelf/install.sh` 删 `--with-xovi-reenable`，只留诊断提示——缺 reenable 时指路 `packaging/deploy-xovi-persist.sh`（现行 install.sh 3d 节）。
2. **reenable 归位基石层**：`cangjie-xovi-reenable.service` → `packaging/xovi-reenable.service`（层中立，开机 `After=home.mount xochitl.service` 跑一次 `/home/root/xovi/start`，`ConditionPathExists` 保证没装 xovi 时"跳过"而非"失败"；**绝不给 xochitl.service 本体加依赖**——2026-08-15 变砖红线）。装/卸/OTA 恢复处做旧名清理。现在是 `install-all.sh` 的 `xovi-persist` 步（`deploy-xovi-persist.sh`，OTA 后要重装，`/usr` 单元会被冲）。⚠ 写 `/usr` 的只是这个独立 oneshot 单元，**不是** `xochitl.service.d` 的 drop-in（那条路 2026-08-16 放弃，写 `/usr` 触发过两次 dm-verity A/B 回滚变砖，见工程纪律）；xovi 本身的 drop-in 由 `xovi/start` 写进 `/etc` tmpfs，重启即清。
3. **可插拔靠两层**：`install.sh --only` 决定磁盘上放不放 qmd/.so 与起不起 daemon；注册表驱动网页 tab 自动显隐。⚠ 当时说"shelf 唯一的 xovi 足迹是 font-serve 一个 qmd"已过时：现 `book` 服务另装 3 个 qmd（`shelf-trash-agent`/`shelf-mkdir-agent`/`shelf-comic-margins`），清单见 `shelf/manifest.sh`。
4. **embolden 默认开**：`FontConfig::default().embolden_cjk_fallback = true`（用户目测更清楚；配置里显式 false 才关）；每个中文回退字体加 `<match target="font"> embolden=true`（对标旧中文化套件的墨水屏细笔画补偿）；`PUT /config {emboldenCjkFallback}` 运行时切换，fontconfig 实时生效。
5. **实测：常开不卡不费电**（开机 23.4h，当时 5 服务）：共 ~9MB RSS、累计 ~1.8 CPU 秒（~0.002%）、负载 0.31。→ "关服务"定位为"隐藏功能/减暴露面"，**不卖省电**。（后来 book-serve 因列表轮询每次开 zip 被查出耗电，已单独治理，见第 E 章；不影响此处"空闲开销极小"的量级结论。）

**管理台（已落地，以 `gateway/src/manage.rs` 为准）**：单一模块目录表 `MODULES`（seg↔service↔`--only` 令牌↔label；现 8 个领域模块：books/fonts/koreader/wallpapers + 笔记线 ink/transcribe/mind/notes，网关自身不在表内、不可关）。`GET /api/manage` 三态（未装：二进制不在→给引导命令；已装未开；已开）；`GET /api/foundation` 只读探测 xovi/appload/qrr/KOReader/WeRead 红绿；`POST /api/manage/{seg}/{start|stop|uninstall}`——开关 = `systemctl`，卸载 = 调设备上 `shelf-uninstall --only <令牌>`（与 `uninstall.sh` 共用 `manifest.sh`）；**安装不走网页**（不让网页 remount /usr），未装只给命令。全部受登录守卫。

**网页易用性打磨（2026-09-04）**：① 二级 tab 复用主 tab 显隐思路（xochitl 拆「传书 / 原生字体」，KOReader 拆「书库 / 字体 / 词典」），根治"一屏堆多个长列表"，选 tab 而非折叠是因为折叠仍要展开再滑；② 管理台三态/开关说明写成默认收起的 `<details>`，含上面的性能实测；③ 基石引导补 reManager（`github.com/rmitchellscott/reManager`，桌面端 vellum 生态管理器）链接。

### 03v｜固件升级 3.27.3.0 → 3.28.0.172 实录（2026-09-05，真机）

> **当前 OTA 恢复流程以 `docs/INSTALL.md`「固件升级（OTA）之后」为准**；本节是首次跨固件升级的实录，附带保留"哪些被冲、哪些活着"的事实。下文 ⑤ 步及 appload 部分已被后续变化取代（见各处标注）。

**升级机制（真机核）**：OTA 走 Memfault（`memfaultd` swupdate 特性，`.swu`），写进**备用槽** root_a（`mmcblk0p2`），30 秒 SUCCESS，等重启切槽；旧槽 root_b 的 3.27 完整保留（`/proc/cmdline root=` 看在哪个槽）。3.28.0.172 的内部版本串 `3.28.1.1666`（`strings /usr/bin/xochitl`），os-release `VERSION=5.8.203`。

**冲掉 / 活着**：新槽 `/usr` 是全新 rootfs——书架 systemd 单元、壁纸 bind 单元 + sleep 钩子全没；`/home` 原样（母版库、KOReader、xovi、`~/.local/bin` 二进制、配置、证书）；`/etc` overlay 照旧 tmpfs。dm-verity **未激活**（`dmsetup ls --target verity` 空），`mount -o remount,rw /` 可写，install.sh 的 verity 门照常放行。

**appload 要不要停**：appload.so **没有 `_xovi_shouldLoad` 自检**，不会"自己停"；但 xovi 的 LD_PRELOAD 只在 `/etc` tmpfs 里，重启即清，所以**首次进 3.28 是纯原厂 xochitl，任何扩展都不载入**——循环死机只可能发生在之后手动 `xovi/start` 时；即便循环，StartLimitAction 整机重启后 `/etc` 又清空、自愈，**最坏是一次被迫重启，不会砖**。实际操作：重启前把 `appload.so` + 两份 qmd 挪到 `/home/root/xovi-disabled/pre-3.28-<时间>/`（extensions.d 外；带 md5 清单），`exthome/appload/koreader/` 数据不动。

**进 3.28 后的顺序（实测约 8 分钟，2026-09-05 版）**：① `xovi/rebuild_hashtable`（设备旁输密码；hashtab 20231 条重建）→ ② `xovi/start` 只带 qrr（`[qmldiff]: Set system version to 3.28.0.172`，NRestarts=0）→ ③ 重装书架（`SHELF_NO_BUILD=1 deploy.sh`；install.sh 按 `IMG_VERSION` 选 3.28 锚点 qmd，未激活，下次重启才生效）→ ④ `chrony-cn.sh` 恢复国内 NTP（OTA 冲 rootfs 后 chrony 回到 Google 服务器，§03w）→ ⑤ 装回 wifi-watch。**现行做法**：③④⑤ 已并入 `packaging/install-all.sh`（`shelf`/`chrony-cn`/`wifi-watch` 步，`deploy.sh` 现在 `packaging/deploy.sh`）；①依旧要人在设备旁做。

**appload 的变化（时效性）**：当时 0.5.3 的 qmd 钩 3.28 已删的 `SidebarFilterItem`，KOReader 无侧栏入口，后来靠 PR #59 的 qmd 等长回填补丁暂时复活；**2026-09-21 起要求 appload ≥ 0.6.0（官方版含 3.28，该补丁工具链已删）**，现状与升级注意（换文件后别 `systemctl restart xochitl`，直接整机重启）见 `shelf/koreader/README.md` 与 `docs/INSTALL.md` 风险①。（上游 v0.6.0 release 存在已查到；发布日期与"含 3.28"的表述取自仓库文档，未独立核实。）

**可升级性的准确说法（2026-09-05 用户问"是否 99% 可用"）**：*升级零风险、数据零丢失、随时可升；升完要手工重装，不是"升了就能用"*。风险按层分、不合成百分比：书架服务/壁纸/WiFi/chrony/休眠屏键只用 `/upload` 接口和系统标准件，重装即回；qmldiff 注入（字体菜单）依赖 xochitl QML，大版本常要重适配（3.27→3.28 已两版 qmd）；KOReader 本体独立，侧栏入口靠第三方 appload。

**顺带核实**：`/home/root/.bashrc` 没有登录触发的 xovi 恢复钩子（A2 登录恢复在这台重置机上没装）；`root` 的 shell 是 `/usr/sbin/rmdevlogin`；vellum 的 `post-os-upgrade` 钩子只打印"先跑 rebuild_hashtable"，不自动做。KOReader 的 `exthome/appload/koreader/` 与 appload 是否加载无关，koreader-serve 照常工作。

### 03w｜原生自定义休眠屏 `SleepScreenPath` + WiFi 60 秒掉链根因（2026-09-05，3.28 真机）

**原生休眠屏（隐藏键，真机通）**：`xochitl.conf [General]` 加 `SleepScreenPath=<png 绝对路径>`（无需 `file://`）。3.28 `SleepScreenView.qml` 的机制：`logo.source = isettings.sleepScreenPath`（默认 `/usr/share/remarkable/suspended.png`）；`isCustomSleepScreenPath` 为真时 **logo 铺满整屏 PreserveAspectFit、插画卡自动隐藏**；图加载失败显示占位文字 "reMarkable is sleeping"。`ShowSleepScreenCarousel` 不是隐藏功能，就是 3.28 设置里的「休眠屏幕插图」开关。真机：停 xochitl → sed 写键 → 重启，键不被 xochitl 抹掉；休眠屏显示池图（954×1696 满屏）；指向 `~/.local/share/shelf/wallpapers/current.png`，**用户两次休眠对照确认轮换生效**（2026-09-05 17:10）——xochitl 每次休眠按路径重读，不吃 Qt Image 缓存。结论：bind-mount 整套可退役（落地见 §03x）。

**WiFi "升级后连不上"**

> 首判（省电模式）后被推翻，真凶见下"真凶（2026-09-06 定案）"。

- **现象**：连上 AP 后约 60 秒 `systemd-networkd: wlan0: Lost carrier` + `wpa_supplicant: REGDOM-CHANGE init=CORE type=WORLD`；**无** wpa DISCONNECTED、dmesg 无字；NM 仍标 connected、路由 dead、永不自愈；`nmcli con up <SSID>` 立刻回来再掉。3.27 那次 2 天 uptime 里 `Lost carrier` **95 次**——一直如此，以前主要走 USB 没察觉。
- **首判（省电，次要）**：`iw dev wlan0 set power_save off` 后 7 分钟零掉（对照：开着时 60/61/61/101/139 秒必掉）。做了 `nmcli con modify <SSID> 802-11-wireless.powersave 2`（按连接持久，NM 库在 `/var/lib/NetworkManager` bind 自 /home；重建该 WiFi 会丢，换新 SSID 要再 modify）。事后证明省电只是叠加因素。
- **真凶（定案，八步排查后）**：`iw event` 抓到掉链是 `disconnected (local request)`；wpa 无断开事件、NM 无 deactivating、nm-metrics 停掉照掉、有 ping 流量照掉——都不是。时序：连上 AP → wpa `REGDOM-CHANGE init=COUNTRY_IE alpha2=CN` → **恰好 60 s** 后 `Lost carrier` + `REGDOM-CHANGE init=CORE type=WORLD`。60 s = 内核 cfg80211 `REG_ENFORCE_GRACE_MS`：regdomain 变化后宽限 60 s，然后主动断开落在"新规则不允许的信道"上的连接。设备自带 `/lib/firmware/regulatory.db` 是 reMarkable 精简版（2 KB），`iw reg get` 里 **CN 只有 2402–2472 和 5735–5835，没有 5150–5350**；路由器 5G 用信道 36（5180 MHz）→ 断开 → regdom 回滚 world（5180 又合法）→ 重连 → 国家码 IE 再设 CN → 60 s 再断，无限循环。
- **验证**：`nmcli con modify <SSID> 802-11-wireless.band bg` 锁 2.4 GHz → 3 分 20 秒 40/40 ping、零掉链、零 regdom 事件。（原文曾写"关省电后昨天 7 分钟不掉"，但那次并未真正验证 5G 下长时间稳定。）
- **两条出路**：① 连接锁 2.4G（已设，较慢）；② 路由器 5G 信道改 149–165（在允许段），再把 band 改回 `a` 或清限制。`regulatory.db` 带签名（`.p7s`，内核编入证书校验），换不了。

**常驻看护 + 固化（2026-09-06，用户拍板"所有 WiFi 走 2.4G"）**：`packaging/wifi-watch/`（`wifi-watch.service` `/usr` 单元 + `~/.local/bin/wifi-watch.sh`，`install-all.sh` 的 `wifi-watch` 步安装）：① 每 15s 看一次，NM 说连着而 wlan0 连续两次 NO-CARRIER 才 `nmcli con up`；② 当前活动连接缺 `802-11-wireless.band=bg` 或 `powersave=2` 就补上并重新激活（每连接只查一次；`BAND=` 置空关掉频段固化）——因为省电关掉后 **slumber 醒来仍会**出现 `Lost carrier`+`REGDOM init=CORE` 的假死（09:51 实例：醒来 37s 后掉、插 USB 也不自愈）。**2026-09-20 省电改造**：carrier=1 走快路径（只读一个 sysfs 文件，零 fork），仅疑似假死才走 rfkill/nmcli 慢路径——原版每 15s fork 外部命令，开机一小时累计 16 s CPU（≈0.46%）。⚠ `nmcli con up` 对已激活连接会先断再连，所以判据必须是真 NO-CARRIER；`nmcli -g …powersave` 回的是文字 `disable` 不是数字 2（旧版拿它跟 "2" 比，每次启动白白改一遍）。日志 `journalctl -u wifi-watch`。

**已落地的兜底（历史）**：曾有 `xovi/scripts/post-start/wifi-reconnect.sh`（`xovi/start` 后台看护 60 秒，NO-CARRIER 就 `nmcli con up`，`journalctl -t xovi-wifi`，真机 `t+5s NO-CARRIER → 已激活` 通）。该源文件已不在仓库 `packaging/` 里，能力由 wifi-watch 取代；设备上若还残留该脚本，未核实。

**时钟从未同步的根因与修法**：`chronyd` 4.5 配置的 4 个 `time{1-4}.google.com` 在国内不通（ping 100% 丢），启动以来没有一条 `Selected source`，`timedatectl` 一直 `synchronized: no`（RTC 本身准，差 1 秒）。设备没有 `chronyc`、busybox 没有 `ntpd`。修法：改 **rootfs 底层** `/etc/chrony.conf`（换成 `ntp.aliyun.com / ntp.tencent.com / cn.pool.ntp.org / time.cloudflare.com`；现由 `packaging/chrony-cn.sh` 幂等完成），重启不丢、OTA 会冲。改后重启 chronyd 8 秒 `Selected source ntp.tencent.com`，`synchronized: yes`。
- **改底层 /etc 的姿势**：`/etc` 是 overlay（lower=/etc 本体，upper=/var/volatile tmpfs）。必须 `mount -o remount,rw /` **先于** `mount --bind / /tmp/rootbind`（bind 继承挂载时的 ro 标志，后 remount 不传播），改 `/tmp/rootbind/etc/…`，umount，再 remount ro；**overlay 缓存 lower**——改完当场 `/etc/` 看到的仍是旧内容，重启才变，要立即生效就再 `cp` 一份进 overlay（落 upper tmpfs，重启自然消失、底层接管）。`remount,ro` 偶发 `mount point is busy`，隔几秒再试。
- **"时不时连不上"的另一半真相（2026-09-06）**：不插 USB 时设备空闲几秒就进内核深度休眠（journal `PM: suspend entry (deep)` / `rm_sleep_monitor: Enter autosleep`，屏幕内容不变，`Woke up with reason=Ignored`），WiFi 随之断，摸屏才回来并重新 DHCP（还会漫游到另一 BSSID）。这不是 WiFi 故障：**要长时间可达就插 USB 供电**；排查前先 `journalctl -b | grep "suspend entry"`。

### 03x｜退役 bind-mount 壁纸整套，改写 `SleepScreenPath`（2026-09-06，3.28.0.172 真机）

**动机**：§03w 证明原生隐藏键满屏、隐藏插画卡、每次休眠重读——bind-mount 覆盖 `suspended.png` + 三张透明插画卡 + 开机单元 + sleep 钩子那整套（§03g P2 时代）从此多余，且是书架唯一还在写 `/usr` 且要 root 挂载的地方。**本节是壁纸现状。**

**现状代码**
- `rmsvc_core::xochitl_conf`（`rmsvc-core/src/xochitl_conf.rs`；原名 `shelf_core::xochitl_conf`）：`xochitl.conf [General]` 单键 get/set/remove，只动目标行、tmp+rename 原子写、首次改前留 `xochitl.conf.shelf-bak`；**文件含 DeveloperPassword/UserToken/devicetoken，模块任何路径都不返回/打印行内容**（错误只带键名）；无 `[General]` 段时在文件头补一段；单测锁"插在段头后、其它行逐字节不变、删键后与原件相同"。
- `wallpaper-serve/native.rs`：`enable`（写键指向 `current.png`，幂等）/ `disable`（删键）/ `restart_pending`（记住写键时 xochitl MainPID，PID 没变即"还没生效"）；激活首张时自动 `enable`；`GET /status` 出 `native:{enabled,path,restartPending}`，上传回执首次提示"跑一次 `/home/root/xovi/start` 后下次休眠即显示"（xovi 已生效时按"速查"表改用 `systemctl restart xochitl`）。子命令：`serve|enable|disable|roll|activate`。
- 删除：`blank776.png`、`CAROUSEL/SUSPENDED_PNG` 常量、入睡补 bind 分支、`shelf-wallpaper-bind.service`、sleep 钩子。脚本本体留档在 `enhance/wallpaper-serve/legacy-bind-mount/`。`install.sh` 3b 现在只做：建池目录 → 有 `current.png` 就 `wallpaper-serve enable`（原先的"迁旧池 + 清旧 bind"迁移块已于 2026-09-06 随体检删除，真机零残留）；`uninstall.sh` 调 `wallpaper-serve disable` 删键还原原生休眠屏。网页壁纸页状态显示「原生休眠屏 已启用/未启用 · 需重启 xochitl 生效」。

**真机**：WiFi `192.168.1.22` 部署（USB 当时不通；known_hosts 里该地址是别的机器旧键，按 USB 已知指纹核对一致后替换）。装前：4 个 bind 在挂、旧单元 active、键已在；装后：bind 0、旧单元/钩子全没（`reset-failed` 清掉 systemd 残留态）、键仍在、`native.enabled=true restartPending=false`、rootfs 回 ro、xochitl 未动（NRestarts=0）。**用户确认（2026-09-06）**：bind 全卸后休眠 3 次换了 3 张池图——只靠原生键 + 唤醒轮换成立，闭环。

**QSettings 运行中改键的边界**：xochitl 在 sync 时按 mtime 重读再合并，外部加的键不被抹（装机时 xochitl 在跑、键保住）；但 `isettings.sleepScreenPath` 只在启动时读，所以首次写键必须重启 xochitl 一次；之后换图不再碰 conf。

### 03at｜真机重大发现+已修复：`gateway`/`shelf-gateway` 两个 systemd 单元并存，真机一直在跑 2026-09-10 的旧二进制（2026-09-16，真机通）

**症状**：部署笔记「整理」区提示的小改动时，例行查 `shelf-gateway.service` 健康，发现 `NRestarts=4517`、`ActiveState=activating`（卡死重启循环），日志 `绑定 0.0.0.0:443（TLS）失败: Address in use`。

**根因**：网关 2026-09-11 正名（`shelf-gateway`→`gateway`）之后，`/usr/lib/systemd/system/` 下同时存在两份单元——旧 `shelf-gateway.service`（`ExecStart=…/shelf-gateway`）和新 `gateway.service`，**两个都被 `shelf.target` 静态 `Wants`**（`shelf.target.wants/` 下两个符号链接）。设备上正名后没人清旧单元。两个单元抢 443，抢不到的那个每 5 秒（`RestartSec=5`）失败重启一次；`NRestarts` 不自动清零，真实持续时长不可知。

**一次操作失误（教训）**：`/home/root/.local/bin/gateway`（新名字）其实一直正常在跑——PID 886 自 2026-09-14 11:58:12 起。当时误判成"占端口的孤儿进程"直接 kill，导致 443 被旧单元抢到，真机从"9-14 之后的新二进制"**倒退**成"2026-09-10 17:18 的旧二进制"。很快查出并纠正（`systemctl stop shelf-gateway.service` + `start gateway.service`，只动运行态）。**教训：端口冲突时先 `systemctl` 看是哪个单元占着，不要凭进程名 kill。**

**修复（分两步，第二步用户看完风险说明后明确要求才做）**
1. 运行态修复（不碰 `/usr`）。
2. 先**核查**：扫 `shelf.target.wants/` 下全部 10 个单元的符号链接时间戳与 `ExecStart` 二进制是否存在——只有 `shelf-gateway.service` 是 2026-09-10（正名前）的旧链接，其余 9 个（book/font/ink/koreader/mind/note/transcribe/wallpaper/gateway）都是 2026-09-11 14:28 重装的，二进制路径都在。**只有 gateway 这一处踩坑**（它是唯一"连目录带二进制名一起搬"的服务）。然后只删最小必要文件：`mount -o remount,rw /` → `rm …/shelf.target.wants/shelf-gateway.service`（只删 `.wants` 链接，不动单元文件本体和旧二进制，留痕）→ `systemctl daemon-reload` → remount ro。
3. **验证**：`systemctl list-dependencies shelf.target` 里只剩 `gateway.service`；`gateway.service` 全程 active/NRestarts=0/PID 没变，零停机。这次 `/usr` 写入（删一个符号链接、不写新内容、不碰 xochitl 相关路径）与项目记录的两次变砖事故（新建 `xochitl.service.d/` drop-in）不同类；`/` 是真实 ext4，普通重启不冲回，只有 OTA 覆盖。

**已根治（代码层）**：这次是手工清的；之后 `shelf/manifest.sh` 加了 `SHELF_LEGACY_UNITS="shelf-gateway.service"`、`SHELF_LEGACY_BINS="shelf-gateway cangjie-lo-alias.sh"`，`install.sh` 每次安装都 `disable --now` 并删旧单元与 `.wants` 链接、备份并删旧二进制，`uninstall.sh` 也清（2026-09-20 提交 cb9e60b，"manifest.sh 共用清单"）。**教训：任何"改名"类重构必须同时写迁移清理，并核对设备上的真实单元列表。**

**次生怀疑（未证实）**：9-13 几次部署（`gateway.bak.pre-dedupe-*`）落的都是新路径；若当时新单元还没起来、旧单元占着端口，那几次验证是否验到了"当前对外服务的进程"存疑。但 886 从 9-14 起稳定在跑、时间线对得上，风险低，只是无法百分之百倒推。

### 03bd｜font-serve 字体菜单"换字体不生效/删除后仍显示存在"：同一根因，QML 列表只增不删（2026-09-19，真机通，✅已解决）

**症状**：用户自己先诊断出"换字体不生效"+"删除后仍显示存在"是同一类 bug；本节是代码定位 + 真机修复验证。

**根因**：`shelf/xovi/font-menu-dynamic.qmd`（3.28 线，`FormatFont.qml` 的 `fontModel` 是真 `ListModel`）的 `cjSync()` 只往 `fontModel` `append()`，用 `cjAdded[key]` 去重，**从来没有 `fontModel.remove()`**——字体在 font-serve 删了、`fonts.json` 正确重写了，菜单条目却只增不减，直到 xochitl 重启。对照 `font-menu-dynamic-3.27.qmd`（model 是普通数组，每次 `onVisibleChanged` 整体重赋值）天然没有此问题——bug 是 3.28 版改用 `ListModel.append` 才引入的，此前没人测过"删除"路径。

**"换字体不生效"的真相**：最初怀疑 Qt 的 `QFontDatabase` 进程启动时建好、不重扫 fontconfig，新装字体渲染 worker 认不到（qmd 注释里早标为"从没验证过的假设"，upload 接口也一直硬编码 `restartNeeded=false`）。真机验证：上传一个全新家族（仓耳今楷03，风格差异极大）→ 菜单选中 → **正文立即变成新字体、不需重启**——该假设被推翻，`restartNeeded:false` 是对的。"换字体不生效"的真实触发场景是：用户选中了一个早已删除、却因列表不摘除而一直挂着的旧条目——文件不在了，当然没反应。

**修法**：`cjSync()` 加"先删后加"——用当前 `fonts.json` 的 key 集合比对 `fontModel`，把 `cjAdded` 记过（=我们自己 append 的，不含原生 4 项）但已不在 `fonts.json` 里的条目**从后往前**`fontModel.remove()`，再走原追加逻辑。日志打 `SHELF-FONT: <reason> lang=… appended=N removed=M count=K`。

**离线验证**：本机没有现成 xovi clone，当场 `git clone asivery/qmldiff`（独立仓库，不在 `xovi` 本体里）+ `cargo build --release` 出 CLI；按 `AFFECT` 锚点手搭最小 QML 夹具（不是设备提出的真实 `FormatFont.qml`——没有现成抽取工具），先跑改前 qmd 确认夹具兼容，再跑改后版本确认解析无误。**只能证明"语法合法、锚点对得上"，不能代替真机。**

**真机验证**（SSH 隧道直调 font-serve API + 用户设备操作 + `journalctl` 盯 `SHELF-FONT:` 三方对照）：
1. 备份现网 qmd（`.bak.pre-list-remove-fix`），scp 新版，`systemctl restart xochitl` 后发现 xovi 扩展链没重新挂上（`_xovi_shouldLoad: 找不到 xochitl 映射 → 拒绝加载`）——改跑 `/home/root/xovi/start`，qrr 正确加载、`Processing file …/FormatFont.qml` 无解析错误。（xovi 当时为何没在运行进程里生效未在记录中说清；此后规则以本章"速查"为准：**先判 xovi 是否生效再选命令**，绝不在已生效时跑 `xovi/start`。）
2. 上传全新家族 → 用户开字体菜单，日志 `SHELF-FONT: visible lang=en appended=1 removed=0 count=8` → 选中 → **正文肉眼确认立即变成毛笔楷体**。
3. 删除该家族 → 用户退出重开菜单，日志 `appended=0 removed=1 count=7` → **用户肉眼确认条目已消失，全程未重启 xochitl**。（日志里同时有两条无害 `TypeError`：删的正是当前选中字体，某个显示"当前字体名"的控件找不到已删条目；不是本次引入，换个没在用的字体删就不触发。）

**结论**：两条症状同一根因（`ListModel` 只增不删），3.28 线已修并真机验证；3.27 线无此问题，不用改。

## 附录

> 这些是跨主题的合集或历史资料：踩坑合集（§04）、旧版现状总览（§00b）、真机待办（§05）、演进记录表、已移除的能力。

### 04｜踩坑

> **本节是全白皮书的「坑位总表」**：先看下面的分类表找坑，需要来龙去脉再跳到「见§」列指向的章节（各章末尾的「本章坑位表」只列本章独有的坑，跨章的以本节为准）。
> **状态标记**：**有效**＝规避方法今天仍适用；**部分过时**＝根因/教训仍成立、具体做法已变；**已过时**＝对应功能已被砍或结论被推翻，只保留教训（详情压缩在本节末尾「已过时条目的细节」）。
> 名词：xochitl＝reMarkable 官方阅读器/UI 进程；qmd＝对 xochitl QML 界面代码的补丁文件；SSE＝服务端事件推送；母版库＝书架里永久保存原书的暂存池；OOM＝内存耗尽被系统杀进程。

#### 一、设备 / 固件 / xochitl 行为

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见§ | 状态 |
|---|---|---|---|---|---|
| xochitl CSS 引擎的怪癖 | 缩进/边距"写了不生效"或全书零缩进 | 七条实测规则：尾部声明无分号被丢；`text-indent:0` 当没设；类规则认且压过元素规则；同类先出现者胜；不认内联 `style`；`text-indent` 会继承；`!important` 不认 | 改排版规则前先用**诊断 EPUB 量渲染缓存**（每章一个变量，投原生后量 xochitl 渲染出的 `<uuid>.pdf`），别靠肉眼；所有输出声明一律带尾分号 | §03y | 有效 |
| 磁盘 `.metadata` 不等于 UI 实际状态（2026-09-04 用户纠正） | 直接 `sed` 把 `parent=trash` 写进 8 个探针的 metadata，UI 回收站却只见真实书 | xochitl 运行时在内存里缓存，写回时会覆盖磁盘；云同步也可能还原 | 书库状态以设备 UI / xochitl 实际为准；清测试文档走正常删除流程，或先停 xochitl 再操作，别边跑边改 | 本节；§03n（批 5：云同步探针清法）；原生回收站代理 §03aa | 有效 |
| qmd 语法比 QML 窄，解析错误＝整份不应用且不报错（2026-09-05） | 菜单静默缺项（用户"选不到已安装的字体"）；journal 里只有一行 `[qmldiff]: Error while processing file tree` | `({})`、裸 `if (` handler 等写法让 qmldiff 报 `expected item assignment value token`；出错时 xochitl 不崩不告警 | 改 qmd 先克隆 `asivery/qmldiff` 编译，`qmldiff apply-diffs <root> <dest> x.qmd -f -c` 对**从固件解出的真 QML**（root 按资源路径摆）离线实跑，过了再上机；上机后必 `journalctl -u xochitl \| grep qmldiff`。解 QML 的 `extract_qml.py` 属历史工具，不在 git 仓库（现存放位置见 §03bf） | §03v、§03bd、§03bf | 有效 |
| 字体菜单"换字体不生效 / 删除后仍显示存在"（同一根因） | 网页删了字体，设备菜单里条目还在；选中这个已删条目当然"没反应" | 3.28 的 `font-menu-dynamic.qmd` 用真 `ListModel`，`cjSync()` 只 `append()`、从不 `remove()`，删掉的字体留到 xochitl 重启才消失（3.27 版整体重赋值数组，没有此问题）。怀疑过的"`QFontDatabase` 不重扫、新字体要重启"被真机推翻：新装字体家族无需重启即生效 | `cjSync()` 改"先删后加"；别把"选中没生效"直接归因给 Qt 缓存，先看菜单里是不是残留了死条目 | §03bd | 有效 |
| 中文字体在书里是方框 / 第二次上传字体不生效 | 中文显示豆腐块 | 中文化套件早期在设备 `fonts.conf` 里写死了中文回退（且 `mode="prepend" binding="strong"`），删字体后回退缺失 | 见对应节；`font-serve` 现动态重写回退 | §03k | 有效 |
| 目录入口消失（《疯探》） | 书里有 TOC，xochitl 不显示目录入口 | 反编译坐实：xochitl **硬编码按 manifest `id="ncx"` 查目录**，不走 `<spine toc="IDREF">`；另有 `toc.ncx` 的 `dtb:uid` 不匹配、外部 DOCTYPE 引用两条独立诱因 | 优化器要保证 manifest 里的 ncx 项 `id="ncx"`、剥 DOCTYPE、对齐 `dtb:uid` | §03bc（结论）、§03az、§03ba | 有效 |
| 两个 systemd 单元抢同一个端口（网关正名后） | `NRestarts=4517`、日志 `绑定 0.0.0.0:443 失败: Address in use`；真机悄悄在跑旧二进制 | 网关正名（`shelf-gateway`→`gateway`）后旧单元的 `shelf.target.wants/` 符号链接没清，两个单元都被 `shelf.target` 拉起 | 改名/搬目录的服务，部署后核对 `systemctl list-dependencies shelf.target` 只剩新单元；判断"谁占着端口"前先看 `ps` 起始时间，别直接 kill（一次误杀让真机倒退到旧二进制） | §03at | 有效 |
| `MemoryMax` 软限从没生效 | 优化 552MB EPUB 时 `VmRSS` 冲到 1.4GB+，系统可用内存探底 ~25MB | 设备 systemd 没把 memory 控制器代理进 `system.slice`（`cgroup.subtree_control` 为空） | 别指望 systemd 内存限额保护 xochitl，必须在代码里做流式/内存预算 | §03ba、§03bh | 有效 |
| busybox 命令残缺 | `head 5` 报错、无 `timeout`/`base64`/`od -A`/`pkill`、`ls` 中文名显示 `?` | 设备是精简 busybox | `head -n`；验中文名用 `find \| hexdump -C`；需要超时/编码在 host 侧做 | 本节 | 有效 |

#### 二、网络 / WiFi / USB

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见§ | 状态 |
|---|---|---|---|---|---|
| "传书就卡、传字体不卡" | 电脑经 WiFi 传 5 本小书必卡，传 5 个 25MB+ 字体反而不卡 | xochitl 每导入一本书就把它连同渲染缓存（约 2–3MB/本）同步到 reMarkable 云，出站突发占满弱热点上行；字体不是文档，不同步 | 大书**优先走 USB**（`https://10.11.99.1`）；先判断"哪端掉了"：设备 ping host 不丢包＝设备无辜；host 的 Intel 网卡 AP 模式吞吐低也是同类问题 | §03l | 有效 |
| WiFi 连上恰 60 秒必掉 | 连上后 60 s 左右 `Lost carrier`，NM 仍标 connected，无 wpa 断开事件 | 内核 cfg80211 的 regdomain 宽限（60 s）：设备精简版 `regulatory.db` 的 CN 只有 2402–2472 与 5735–5835，路由器 5G 用信道 36（5180MHz）被判非法 → 断开 → 回滚 → 重连，无限循环；省电只是次要因素 | 设备连接锁 2.4GHz，或路由器 5G 信道改 149–165；`wifi-watch` 常驻看护 | §03w | 有效 |
| 不插 USB 空闲几秒就"连不上" | 屏幕内容不变但 WiFi 断，摸屏就恢复并重新 DHCP | 内核深度休眠（`PM: suspend entry (deep)`），不是 WiFi 故障 | 要长时间可达就插 USB 供电；排查先 `journalctl -b \| grep "suspend entry"` | §03w | 有效 |
| 改一个常量只改了服务端，客户端默认值仍指向旧端口 | 电脑 CLI 报"设备不可达"，实际设备正常（`curl :8778/health` 失败、`curl https://10.11.99.1/health` 200） | 网关端口 8778→443（§03am）时，host CLI 自己的默认端口是另一份硬编码，被漏改，三天无人发现 | **`grep` 全仓库搜数字本身**，不要只沿"哪些文件输出这个 URL"排查；纯装饰性的测试假数据不会因逻辑测试失败而暴露过期 | §03am；CLI 已砍，教训通用 | 已过时（教训保留） |
| 网页 SSE 长连接在设备上无法用 busybox 验证 | `wget` 缓冲到结束、`nc` 拿不到响应 | busybox 工具不支持流式读取 | host 侧 `ssh -L` 隧道 + `curl -sN`；后台进程用 `setsid … < /dev/null` 否则 ssh 会话挂住；`pkill -f` 的匹配串别写进自己的命令行（会杀掉自己，exit 144） | 本节；SSE 见 §03z | 有效 |

#### 三、格式与渲染

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见§ | 状态 |
|---|---|---|---|---|---|
| `figure`/`figcaption` 边距、图片"留白" | 图多的网文留大量空白 | ① 边距确实没清零（已补，但对该文零改善）；② **真因**：块级图片在页尾放不下就整体挪下页，属分页引擎固有行为，CSS 无杠杆；③ `figure,figcaption{}` 逗号选择器会让 xochitl 整条规则失效 | 别把"顺手堵上的缺口"当成用户投诉的成因，A/B 对照再下结论；选择器分开写 | §03aq；bookconv 白皮书 §12 | 有效 |
| 漫画 EPUB 左右留白 / 页边距 | 图片左右各约 20pt 留白 | xochitl 的 EPUB 渲染走文字排版盒；外部改文件无效，必须在 xochitl 进程内调用它自己的 API | 现行方案见 bookconv 白皮书 §20（页边距设 1 + 补白比例，实验室开关）；2026-09-19 曾改产 PDF 绕开（§03bk），2026-09-20 用户拍板换回 EPUB（bookconv §19） | §03bk（已被取代）；bookconv §16、§19、§20 | 部分过时 |
| 封面声明写法导致取不到封面 | 书库没有封面缩略图，xochitl 日志 `failed extracting cover: got null cover image`；大文件占位替换后封面/显示名不更新 | 三个最小 EPUB 对照实验：封面条目 id 带点（如 `x00000001.jpg`）且只有 `<meta name="cover">` → 取不到；同 id 再加 `properties="cover-image"` → 正常；id 简单（`cover`）仅 meta → 正常 | OPF 里 meta 与 `properties="cover-image"` 同时写（`wash::ensure_cover_declared`，须在清洗前调用）；占位必须带真书名（`dc:title`）与真封面；补封面用无损工具 `cover-fix`（不重编码图片） | §03bo、§03bn | 有效 |
| 扫描漫画 PDF 处理失败 | 旧 CLI 报"扫描件重排失败（无 k2pdfopt 且裁边未产出）" | 分类器 `is_comic()` 当年完全不认 `.pdf`；重排依赖可选外部工具，扫描型又被裁边脚本主动拒绝 | 分类器要按**每种输入格式**逐一覆盖，别让某个格式落进兜底 `False`；两层"遇到扫描件就拒绝"叠加会包装成一个含糊的 rc | §03br；bookconv §18 | 已过时（现行：无文字层的扫描件与 PDF 漫画在母版库只裁边） |
| 网页 `hidden` 属性与 class 同时控制显隐 | 关掉一个曾被点开的子标签，`hidden=true` 却没隐藏 | 页面自己的 `.subpanel.on{display:block}`（author 样式表）无条件压过浏览器内置 `[hidden]{display:none}`（UA 样式表），与选择器优先级、书写顺序无关；未真机复现，读代码推出 | `hidden` 之前先把 `.on` 挪走（做法：先 `.click()` 切到默认子标签）；以后凡"class 控显示 + hidden 叠加"直接照此顺序（同类的 `subtabs()` 嵌套坑见 §03al、§03am） | §03ak、§03al、§03am | 有效 |

#### 四、内存、稳定性与耗电

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见§ | 状态 |
|---|---|---|---|---|---|
| multipart 流式解析变慢 | 测试 50 s | `fill()` 每次 `Vec::resize(+64KB)` 清零，在逐字节到达的流上变成 memset 风暴 | 改读进栈上临时块再 `extend_from_slice` → 0.8 s；现行代码 `rmsvc-core/src/multipart.rs` 的 `fill()` 已是此写法 | 本节 | 有效 |
| xochitl `/upload` 体积上限 | 大文件 `multipart body is too large` / HTTP 413 / `Connection reset` | 上限精确为 **100,000,000 字节**（2026-09-19 二分法实测；此前 150 是猜测值，一卷约 96MB 的漫画落在"看着安全"的区间反复炸） | book-serve 配置键 `nativeUploadLimitMb` 缺省 **90**；超限走「占位 + 磁盘替换」大文件通道（真机验证 PDF 154MB / EPUB 153MB），本机没有书库目录才退回按卷拆分/拒绝；原文"EPUB 不能分卷、漫画别走 EPUB"已被推翻 | §03bn、§03ax、§03t（旧结论） | 部分过时 |
| 整本 EPUB 优化冲爆内存 | 552MB《镖人》套装优化，`VmRSS` 1.4GB+ | 整本读入内存；落库不拆分路径叠 3 份数据 | 流式优化；`Staging::deliver` 落库改流式；内存阈值必须**真机实测**（`imgopt` 单图像素上限先按理论估算定 2500 万，真机又撞 262MB 级，实测 `VmHWM` 后改 900 万才真压下去） | §03ba、§03bh、§03bi | 有效 |
| 后台线程 panic 摔掉整个 book-serve | 损坏书触发越界，进程死、在途操作全卡（《镖人》真机踩过） | release profile 原为 `panic = "abort"`，`catch_unwind` 无效 | 改 `unwind`；启动时把停在 `pending` 的边车改 `failed`，删 `.optimizing.tmp` 半成品 | §03bq | 有效 |
| 列表轮询耗电 | 空闲 book-serve 耗电远超其他 | `Staging::list()` 每次给每本 EPUB 开两次 zip，裸 `File` 每条目十几次小 read；前端每 3 秒轮询 | `BufReader` + 按（大小,mtime）缓存，约 70 倍；新增轮询字段必须缓存；`wifi-watch` 的 powersave 判据拿 `disable` 文本比 `"2"` 永不成立（每次开机白白断线重连） | §03bm | 有效 |
| 前端状态提示"一闪而过" | 「推送本章」等点完后提示不到 1 秒消失 | 不是 `wait(1500)` 到期，而是后端 `entries` 事件经 SSE 触发的 `refresh()` 抢跑重画，把提示连 DOM 冲掉；把延时改成 3 秒毫无用处 | 症状"闪一下"先查**重画是哪条路径触发的**再动手；用 `holdRefreshUntil` 时间戳让 SSE 路径让路 | §03au | 有效 |
| 上传去重：重传/重复拖同一文件产生 `1_x`/`2_x` 副本 | 母版库出现同名带编号的重复条目 | 母版库落地用 `rmsvc-core::fs::unique_path`：**只按文件名判重、不比内容、绝不覆盖**（有意设计，防不同书撞名互吞）；上传端又没有跨次记忆 | 网页 `uploader()` 加可选第 6 参 `dedupeApi`：上传前 `GET /api/books/staging` 取 `name\|bytes` 快照，命中就跳过，**且每次上传成功后必须把该文件加回快照**（否则同批次内两份同名同大小仍会上传两遍）；只有母版库上传口传此参数 | 本节（细节见下） | 有效 |

#### 五、工程流程 / 部署 / 测试

| 坑 | 症状 / 判据 | 根因 | 教训 / 规避 | 见§ | 状态 |
|---|---|---|---|---|---|
| 挪代码时顺手带走的文案不代表内容还准（2026-09-10 用户真机逮到） | 「实验室」里 battop"未装"提示的路径 `misc/battery-audit/battop/install.sh` 早已 `git mv` 到 `enhance/battop/`（现路径 `enhance/battop/install.sh`，已核实存在） | §03aj 写文案时没核路径；§03ak 又原样搬进新卡片，两轮都没查 | 移动/复用含具体路径/命令/版本号的文案时**当场核对还准不准**；"原样复制"只保证格式没错 | §03aj、§03ak | 有效 |
| 跨语言移植逻辑漏了一行 | 网页上传去重上线后，同一批里两份同名文件仍各传一次，产生 `1_x`/`2_x` | CLI 版每次上传成功后 `staged.update(...)`，移植到 JS 时这一步漏掉；本机测试只覆盖"刷新页面重传"一种触发 | 移植时追问"当时为什么要有这一行"；各触发分支（跨会话 / 同批次）**各写一条测试**，不能一条当整体验证过 | 本节 | 有效 |
| 本机冒烟污染真实用户目录 | 测试数据写进宿主机真实 `~/.local/state/shelf` | 桌面环境自带 `XDG_STATE_HOME`/`XDG_CONFIG_HOME`，盖过 `HOME` 覆盖 | 本机起服务/测试一律加 `env -i`（2026-09-13 起隔离实例时又踩了一次，及时发现清理，未造成实际污染） | 本节 | 有效 |
| shell 工作目录漂移 | `curl -F` 落地文件混进仓库（2026-09-05 误提交两个临时文件，已删） | 会话里 cwd 在 `cang-jie/` 与 `shelf/` 之间漂移 | 落地文件一律写绝对路径到 scratchpad | 本节 | 有效 |
| `shelf/build.sh` 与 pytest 的运行目录 | 找不到脚本 / 测试挂 | 原写法"build.sh 必须在 `shelf/` 跑""pytest 要从仓库根跑" | 现 `build.sh` 开头自带 `cd "$(dirname "$0")"`，任意目录可跑（仓库根仍没有 `build.sh`）；`shelf/host/tests` 已随 host 一起砍，CI 现只跑 `uv run pytest packaging/tests`，仍从仓库根跑 | — | 部分过时 |
| 多个测试文件对同一个 `http.server` Handler 类 monkeypatch | pytest 挂死（patch 链互相覆盖递归） | module fixture 共用服务器线程 | 每个文件用自己的 Handler **子类** + 自己的 fixture；对应 host 测试已砍，教训通用 | — | 已过时（教训保留） |
| 部署对象是否真是"当前对外服务的进程" | 部署验证的可能不是真正响应请求的二进制 | 见 §03at 双单元 | 部署后核对 `systemctl is-active`/`NRestarts`/进程起始时间；网关 UI 是 `include_str!` 编译进二进制的，光改 JS/JSON 不重新编译不生效 | §03at | 有效 |

#### 已过时条目的细节（电脑端 `shelf` CLI 已在 2026-09-18 砍除，见附录 B）

以下几条当年是电脑端 `shelf push` 的坑，功能已整体消失、没有网页等价物；只保留数据与教训，防止有人重蹈。

**1. 批量部分失败后重跑不幂等 → 网页上传口同款缺口（2026-09-13，读代码坐实）**

- 症状→根因：`shelf push a.epub b.epub` 中 a 成功、b 失败，重跑整条命令会让 a 在母版库落成 `1_a.epub`——**成功的静默重复**，回执文案"已有同名，存为 1_a.epub"很容易被一眼带过。CLI 顺序遍历、逐文件 POST、失败只置 `rc=1` 继续，没有任何跨次记忆；staging 端 `unique_path` 只按文件名判重。
- 修法（当年）：CLI 首次探活后查一次 `GET /api/books/staging`，按（文件名, 字节数）快照跳过同名同大小文件；不比 hash，已知取舍（同名同大小内容不同会被误跳过，改文件名即可绕过）；查询失败退回旧行为不阻断。
- **网页端同款缺口（仍现役）**：同一页面会话内点"重传"是安全的（`f.st==='ok'` 的项跳过），但**刷新页面后重拖同一批**或**手滑拖两次同一文件**会撞上同一个 staging 根因。修法即上表「上传去重」一行（`gateway/ui/app.js` 的 `uploader(...,dedupeApi)`，i18n key `common.alreadyStaged`）。
- 验证经过：本机起隔离 `book-serve`+`gateway`（`env -i`）+ puppeteer 无头浏览器：登录→改密→上传→刷新→重传，第二次提示"已在母版库…跳过重传"、staging 里只有一条；部署真机后用户用一本真漫画 PDF 实测，出现"已入母版库（已有同名，存为 1_x）+ 存为 2_x"，暴露"同批次内快照没更新"漏网分支，补 `existing.add(...)` 后用 `<input multiple>` 塞两份同一文件复现并验证只入库一条，重新交叉编译部署（备份旧二进制、`NRestarts=0`）。
- 未清理状态：当次测试在真机母版库留下的 `1_x`/`2_x` 两份重复文件需用户自己在网页删除（不替用户做删除决定）；现在是否仍在，未核实。

**2. 扫描版漫画 PDF 走 `shelf push` 必然失败（2026-09-13，用户真机踩到）**

- 症状：一本 Anna's Archive 的纯扫描图片 PDF（《照明商店》第 11–20 话，81MB；同系列第 1–10 话走网页原样上传成功，因为网页不碰"洗书重排"）报 `pdf_reflow_move.py 失败（rc=2）：扫描件重排失败（无 k2pdfopt 且裁边未产出）`。
- 根因：两层"遇到扫描件就拒绝"叠加——重排先找可选外部工具 `k2pdfopt`（项目有意不内嵌位图重排引擎，`doctor` 只提示"缺（可选）"且全仓库没有安装指引）；没有就退回裁边脚本，而它对扫描/混合型按设计主动拒绝产出（`return 3`）。真正的架构缺口：`comic.is_comic()` 只判 `.cbz`/PalmDB 家族/`.epub`，**`.pdf` 从设计上不在判断范围内**。
- 当年修法（2026-09-14）：新增 `.pdf` 分支——抽样"有图且几乎无文字"的页占比，图片页 ≥20 且占比 ≥60% 判漫画（独立子进程探针，缺 pymupdf 时静默退回 False）。用用户原始文件实测：探针判出 2375 页、抽样 ratio=1.0；`ebook-convert` 3.3 秒转出 76MB 中转 EPUB；抽出 2376 张页图肉眼核对画面完整；全流程（16 灰+跨页拆分）155 秒出 247MB 灰阶 CBZ。**未验证**：没有做最后一步真网络上传落地。
- 现状：以上 CLI/Calibre/CBZ 路径全部已砍。现行 PDF 入库见 §03br（无文字层的扫描件与 PDF 漫画在母版库只裁边、格式不变）与 bookconv 白皮书 §18。

**3. "跳过已存在文件"只省流量、没省 host 处理时间（2026-09-13 发现，2026-09-14 修）**

- 局限：判重放在"处理完之后、上传之前"，重跑大部头仍要完整重跑洗书/重排；不能简单前移，因为 staging 里存的是**处理后**字节数，与原始输入对不上。
- 当年方案：`sidecar::Delivered` 新增 `source: Option<SourceRef>`（`{name, bytes}`，`serde default` 兼容旧记录）；`POST /staging` 接受可选 `?srcName=&srcBytes=`，入库后 `Staging::set_source()` 写进边车；CLI 在处理任何一本书前先查（原始文件名, 原始字节数）是否命中 `sources`，命中就整本跳过。测试：Rust 侧 18 个 book-serve 测试通过，Python 侧 90 个 host 测试通过；用独立 `book-serve` 实例（`env -i`）实测线路通。**未验证**：没有经过真实 `gateway`+HTTPS+密码的三跳，也没在真机上重跑一批验证。
- 现状：服务端 `?srcName=&srcBytes=` 与边车 `source` 字段仍在（`book-serve/src/api.rs`、`sidecar.rs`、`staging/deliver.rs`），但唯一的调用方 CLI 已砍，网页 UI 没有传这两个参数——目前是没有调用方的休眠能力。

**4. 修 `shelf_cli/config.py` 默认端口**：见上文「网络」表最后一行（8778→443；`uv run pytest shelf/host/tests` 87 项全绿、`shelf status` 从连接失败变成正常连上但报密码问题，没有真机密码无法验证到底）。

#### 其它章节的关键坑索引（详情以各章为准）

| 主题 | 一句话 | 见§ |
|---|---|---|
| 脚注返回浮标 | 原生已有解；§03aw 的不准确记录已在 §03ax 更正 | §03ax |
| 大漫画拆分 | 多层嵌套 NCX 边界计算 panic、分卷文件名超 255 字节被误诊为"文件系统错误" | bookconv §15、§17 |
| 文件夹名带斜杠被误拒 | `MkdirQueue::add` 的校验没有技术依据（`Library.createCollection` 只把它当 `visibleName` 字符串）；"机制本身没问题"≠"用户这次操作没问题" | §03bf |
| 落库进度条不刷新 | `try_deliver_split` 写完边车没有 `bus.publish` | §03be |
| 重优化跨版本脚注不得翻倍、读条目失败必须整体报错、非 ASCII 字符边界 panic | 优化器内部的几条硬约束 | bookconv §12 |
| SQLite 交叉编译坑 | koreader-serve 高亮/生词端点为绕开它另做了只读实现 | §03ar |
| xochitl 原生 `SleepScreenPath`、bind-mount 退役 | 现行休眠屏方案 | §03w、§03x |

### 00b｜现状总览（2026-09-10 刷新，读本文其余历史节前先看这里）

> **⚠ 已被取代**：这是 2026-09-10 的旧版现状总览，**现状以文首「现状总览（2026-09-20 刷新）」为准**。本节已压缩成"旧版要点 + 与现状的差异表"，只为查历史；不要把下面"旧说法"一列当现役。
>
> **2026-09-18 现状更正**：用户表态以后不再使用 PC 端，`shelf/host/`（Python 命令行 `shelf` + Calibre 转换管线）整个砍掉，源码留档本机 `oldbak/cang-jie/shelf-host/`（不随仓库走、不再维护）。同一天格式白名单再收紧："入库只入 PDF 和 EPUB，不论格式是否支持"——原三档（原生 / 电脑可转 / 仅 KOReader）收成一档，`rmsvc_core::formats::KOREADER_ONLY_EXTS` 已删，`BOOK_EXTS` 等于 `NATIVE_EXTS`（`rmsvc-core/src/formats.rs:7,15`，只有 epub/pdf）。**这是策略收紧，不是技术判断**；已在库里的旧格式条目仍可加入 KOReader，只是不再有新的。**被砍掉、且没有网页等价物的旧能力**（真实的功能减法，不是搬家）：Calibre 深洗、PDF 结构化重排、TXT 切章建目录、漫画→CBZ（跨页拆分/白边裁切/16 灰省刷新档）、`doctor --render` 排版回归探针、`shelf notes pull`（笔记线 md 导出拉本机 Obsidian vault）、`shelf font/wallpaper/koreader/events/inbox/passwd` 等全部子命令。完整决策与影响范围见附录 B、第 A 章与附录 A；笔记线自己受影响的部分由 `notes/` 的白皮书记录。

#### 旧版要点（架构与读书线）

**架构**：一个网关 + 四个只听本机的领域服务 + 注册表驱动的网页 tab + 事件总线。

- 网关 `gateway`：`0.0.0.0:443`（2026-09-10 起绑标准端口，历史上是 `:8778`，§03am），HTTPS 私有 CA + 登录页密码 + mDNS `shelf.local`；单页 UI 源码在 `gateway/ui/`，编译期 `include_str!` 进二进制（`gateway/src/ui.rs:9-11`）；用户可见标题/图标是「秘密花园」（§03am）。
- 领域服务（loopback）：book 8790 / koreader 8791 / font 8792 / wallpaper 8793；tab 由运行时注册表驱动。
- 事件总线：各服务 `GET /events` → 网关 `Hub` 汇聚成 `GET /api/events`（SSE，服务端事件推送），网页零轮询（§03z）。
- 设备当时是固件 3.28.0.172（2026-09-05 从 3.27.3.0 升级，§03v）；KOReader v2026.07.1。

![shelf 架构：网关 + 领域服务](diagrams/architecture.svg)

**读书线 = 三层 · 三动作正交**（§03r 定、§03s 收口）：内容源（网页上传 / 抓网文〔可选「同步优化」，§03ap〕/ scp inbox；微读线已砍，§03u）→ **母版库**（`~/.local/state/shelf/books/staging/`，原样入库、永久保留、不淘汰）→ 落库（人选：投 xochitl 或加入 KOReader）。「优化」是母版库里对 EPUB 的独立动作，落库＝纯复制母版字节；**所有书只落母版库，没有任何直投读器的路径**。投原生后自动渲染自检（§03aa：xochitl 导入即渲染并写 `pageCount`，与正文字符数期望比，<50% 判 warn，结果进边车 `.<书>.delivered.render` + `books/render` 事件 + 网页徽章）。

![shelf 数据流：三层·三动作正交](diagrams/data-flow.svg)

#### 旧说法 vs 现状

| 项 | 2026-09-10 的说法（已过时） | 现状 | 见 |
|---|---|---|---|
| 电脑端 CLI `shelf` | `push`（Calibre 洗书/PDF 重排/TXT 切章/漫画→CBZ→16 灰，`--wait`、`--no-calibre`）、`doctor --render`、`events`、font/wallpaper/koreader/inbox/status/passwd | 2026-09-18 整体砍除，无网页等价物 | 附录 B |
| 格式 | 三档：原生 epub/pdf · 电脑可转 azw3/mobi/azw/prc/fb2/txt · 仅 KOReader 其余 10 个 | 一档，只收 EPUB/PDF | 第 A 章 |
| 投原生体积门 | `nativeUploadLimitMb` 缺省 150 | 缺省 **90**（`services/book-serve/src/config.rs:24`）；>90MB 优先"占位+磁盘替换"（不分卷，上限 1GiB），不可用才回退按卷拆分 | §03bn、第 C 章 |
| 优化 | 档位 auto / keep-spacing / plain，产物标记 full/core/old | 只有「完整清洗 + 优化」一档；`OPTIMIZE_VERSION`=**15**（`shelf/crates/bookconv/src/optimize/mod.rs:55`） | 第 B 章 |
| 漫画 | host 转 CBZ（再过 16 灰），默认只加入 KOReader，小体积可选投原生；超限只出 CBZ、绝不分卷（§03ad） | CBZ 与 host 转换已不存在；漫画走 EPUB 入库，自动识别、保画质、优化不改格式；超限走占位替换或按卷拆分 | §03bk、§03bn |
| 共用基座 | `shelf_core`（`clock`/`xochitl`/`fswatch`/`events`/`registry`〔含 `SvcClient`/`enc`〕） | 顶层 `rmsvc-core`（`rmsvc-core/src/` 同名模块） | §03ai |
| 网关 | `services/shelf-gateway/`（含 `src/enhance/` 的 `mod.rs`/`qol.rs`/`battop.rs`） | 顶层 `gateway/`，`gateway/src/enhance/` 三文件仍在 | 第 D 章 |
| font / wallpaper 服务 | 与 book/koreader 同属 shelf | 源码搬到 `enhance/`（`font-serve`、`wallpaper-serve`） | 第 F 章 |
| book-serve 领域代码 | `staging.rs` 一个文件 | 已拆成 `staging/` 目录（`deliver.rs` 等）；新增 `ops.rs`（忙锁/取消 `OpRegistry`）、`service_state.rs`、`comic_margins.rs` | §03bq |
| appload | 0.5.3 经 qmd 回填补丁在 3.28 复活 | 要求 appload ≥ 0.6.0（官方已含 3.28），补丁工具链已删 | §03v、第 F 章 |
| 建文件夹队列 `mkdir.rs` | "note-serve 已不再调用，当前消费方存疑" | **2026-09-19 复活**：真消费方是母版库「加入 xochitl → 文件夹」，`Staging::deliver` 落库前 `ensure_folder`（`staging/deliver.rs:174`）入队并同步等 `shelf-mkdir-agent.qmd` 建出 | §03be、§03bf |

**仍然有效的旧要点**（细节在各章）：

- **代码落点**：book-serve 的 `sidecar.rs`（落库记录边车）/ `render_check.rs`（渲染自检）/ `pending_queue.rs`（§03ag：`PendingQueue<T>` 持久化+入队去重+剔除的共用骨架）/ `trash.rs`（原生回收站队列，执行方 `shelf/xovi/shelf-trash-agent.qmd`）/ `mkdir.rs`（原生建文件夹队列，执行方 `shelf-mkdir-agent.qmd`）/ `spool.rs`（inbox 队列）/ `api.rs`（纯适配）；koreader-serve 只做"从母版库 adopt"+ 字体/词典/配置同步（不依赖 bookconv）；bookconv CLI `epub-optimize` 与设备是同一个函数。
- **网页 tab**（2026-09-10 §03al 定为固定四段）：传书（入库｜母版库，固定第一）· 笔记（`notes/` 的 note-serve 注册；「导入 md 文档」子标签由 `notesImportMdEnabled` 控制，缺省隐藏，§03ak）· 其他（xochitl(font-serve)/KOReader/壁纸降一级当二级子标签，只列真装了的）· 管理（二级：基石与模块｜模型管理｜系统增强｜实验室｜电池刺客〔`battop.running` 时才出现，§03am〕）。§03ak 那版做过的"独立电池刺客顶层页"已撤回。四条 tab 排布的最新形态以第 D 章为准。
- **笔记线（`notes/`）是独立仓库线，只是挂在同一网关上**：§03ac 记的是"书架这套可插拔机制接得住独立线"，不是笔记线的设计；笔记线自己的架构/真机记录/踩坑在 `notes/docs/reMarkable笔记白皮书.md`，本文不代管。
- **设备杂项（§03v/§03w，全真机通）**：3.28 字体菜单 qmd 通（qmldiff 语法坑见 §04）；原生休眠屏键 `SleepScreenPath=current.png` 满屏且随轮换，bind-mount 壁纸整套退役（§03x，`xochitl_conf`）；**WiFi 连上恰 60 秒必掉**的真凶＝cfg80211 regdomain 宽限（精简 regdb 的 CN 无 5150–5350，路由 5G 信道 36 被判非法）→ 连接锁 2.4G + `powersave 2`，`packaging/wifi-watch` 常驻固化（§03w）；离 USB 数秒自动休眠关 WiFi 是设备正常行为；chrony 国内 NTP `packaging/chrony-cn.sh`；OTA 后恢复以 `docs/INSTALL.md`「固件升级（OTA）之后」为准。
- **已删的东西**（别再找）：漫画 CBZ→PDF **分卷**投原生整条（`POST /staging/to-pdf`、`push --mono`，§03t 末，被否决）——⚠ `cbz2pdf` bin 本身 2026-09-08 曾以新形态（体积门控、不分卷）复活（§03ad），别当"已删"；book-serve 的 `POST /?target=native|annot` 直投路与 `target.rs`/`pipeline.rs`、`/targets`、`done/` LRU；koreader-serve 直传 `POST /books` 与 `optimizeEpub`；网页读器页的传书区与 KOReader 书库浏览；壁纸 bind-mount 整套（§03x、§03ab）；`paths::data_root`、`htmlproc::has_internal_anchor`、`optimize::is_current_version`（§03ab）。

**2026-09-10 追加四轮（§03an–§03aq）**：§03an 网页正文全量 i18n（437 个 key；§03ae 架子当时只覆盖外壳+顶层导航，这次补完四个 tab 的全部正文）· §03ao `shelf push --no-calibre`（已随 CLI 砍除）· §03ap 抓网文「同步优化」复选框（`fetch_article` 加 `optimize` 参数）· §03aq `wash_css` 补 `figure`/`figcaption` 边距归零 + 真机排查"抓完优化还是大量留白"。**§03aq 的结论是"改了一处真实缺口，但没解决投诉"**：留白的真成因是分页引擎对放不下的图片块整体挪页，没有 CSS/EPUB 层杠杆，问题仍在，别把它误当"留白已修"。

**当时的"未闭环"清单**（i18n 只缺人眼确认、§03af 只缺截图、§03aq 留白未解）已并入 §05「仍开放的待办」，那里是权威。CJK 手写笔迹优化在 §03aj 时还是纯占位卡片，2026-09-10 已是真开关（`enhance/handwriting-stroke/`，见系统增强白皮书 §03e–§03f）；`hlSnapCjk` 荧光笔吸附不生效的根因是 `cangjie-langhook.so` 整个从设备上消失，不是书架线的事，见系统增强白皮书 §04 追记。

### 05｜真机待办（滚动更新）

> **⚠ 本节停在 2026-09-06 ～ 09-19（更新日志见下），其中"OTA 后固定五步"已过时**：文中的 `packaging/wifi-watch/install.sh` 并不存在（`packaging/wifi-watch/` 下只有 `wifi-watch.service` 与 `wifi-watch.sh`，部署走 `packaging/deploy-wifi-watch.sh`/`install-all.sh`），`shelf doctor --render` 已砍除。OTA 后的恢复流程以 `docs/INSTALL.md`「固件升级（OTA）之后」为准（三份 OTA 表已合并成那一份）。
>
> **本节结构**：一、仍开放/未验证的待办（权威）→ 二、已闭环的旧待办（划掉，只留指针）→ 三、更新日志（原来塞在标题里）→ 四、已闭环（真机）清单 → 五、Phase E 实录 → 六、已放弃。

#### 一、仍开放 / 未验证的待办

| # | 事项 | 现状（为什么还没闭） | 需要谁做什么 | 见 |
|---|---|---|---|---|
| 1 | **图片密集网文大片留白** | 真机 A/B 已证伪 `figure`/`figcaption` 边距是成因；真成因是分页引擎对放不下的图片块整体挪页、当前页剩余空间不回填，**没有已知修法**。不是"没人看过"，是"看过了、问题还在" | 无（需要新思路） | §03aq |
| 2 | 网页 UI i18n 的浏览器人眼确认 | 数据链路+内容对照已真机验证（`curl` 交叉核对真机 served 语言包与部署 `app.js` 里全部 416 处 `T()` 引用零缺失；语言包 437 个 key）；**没在真实浏览器里点切换器看英文排版/换行/对齐**。这条缺口从 §03ae 延续至今 | 用户在浏览器里切一次语言 | §03ae、§03an |
| 3 | 网页 UI 人性化/触屏（§03af）的实际渲染 | 纯前端改动，只确认过新标记已 served；禁用按钮说明文字是否显示、徽章点击 `alert` 是否弹出、`.btn-bad` 配色是否合预期都没人眼确认过 | 同上 | §03af |
| 4 | **Anchor 脚注模式的嵌套 `<p>`** | `preserve_relink_footnotes` 的 Anchor 分支固定 `<p id="{frag}">{text}</p>` 包注释（`htmlproc/footnote.rs:391`）；注释块若来自 `aside`/`div`/`li`，内层可能自带 `<p>`，产出 `<p id="fn1"><p>…</p></p>`——XML 良构但内容模型非法。**xochitl 实际渲染表现未验证**（没找到真实带该模式的书）。既有缺口，非当轮引入；代码里也没有做防护 | 找一本真带块级注释的书测 | §03aw |
| 5 | 「书籍依然被锁字体」 | 《赎罪》CSS 里根本没有 `font-family`/`font-size`（无从剥起），排除了 `DEFAULT_FILTER_PROPS` 改动是成因；症状是"阅读器里改字体，正文没变"，机制没查清。需要对照实验（同书：原样上传 vs 点优化后，字体切换是否都失败）判断是回归还是 xochitl 固有行为 | 用户在设备上做对照实验 | §03aw |
| 6 | 火影忍者拆分卷 `comic.css` 修复的真机视觉复验 | 外链 `comic.css` 修复已部署，**只验证了组包正确**（zip 结构 + manifest + link），没重投验证渲染；要拉渲染缓存量页面/图片矩形，确认不再是 17.8/35.5/18.2/90.1 那组留白数字。之后漫画管线又经几次大改（§03bk 一度改产 PDF、2026-09-20 换回 EPUB，再到 §20 的页边距/补白），这条旧复验是否还有意义以 §20 为准（未见后续复验记录） | 需要时重投一卷量一遍 | §03ax、§20 |
| 7 | 设备上的重复/旧版副本 | 14 份《火影忍者》卷八～十四（7 卷各 2 份，CSS 修复前的旧版）：批量清理曾被 安全检查拦下，**文中无后续清理记录，设备当前状态未核实**。重复《疯探》已清理到只剩一份（§03ba 时为 `07b33e06`） | 用户在设备上清，或明确授权 | §03ax、§03ay、§03az |
| 8 | 网关并发闸门核心验证 | "两本大书是否真被网关串行化、`VmHWM` 不叠加"这条最核心的验证一直没独立做过；批量「加入 xochitl / 加入 KOReader」两条在设备上没端到端实测；新界面的触屏交互没人眼确认；§03bl 的三条反馈修复已部署但用户尚未独立复核 | 用户/设备联调 | §03bl、§03bp、`gateway/docs/reMarkable网关白皮书.md` |
| 9 | >153MB 文件走占位通道的首次渲染内存/耗时 | 154MB PDF / 153MB EPUB 已真机验证能开，更大的没验证 | 需要时投一本更大的 | §03bn |
| 10 | 入库 PDF 转 EPUB（有文字层）合并部署后的功能本身 | 已并入并部署，但功能本身**未经真机验证** | 拿真实有文字层 PDF 投一次 | §03br |

#### 二、已闭环的旧待办（划掉）

| 旧待办 | 结论 |
|---|---|
| ~~《疯探》目录入口出现没有~~（曾多轮反馈"依然没有三个横杆"） | 已解决：dtb:uid 不一致（§03az）与 toc.ncx 外部 DTD（§03ba）都不是根本原因，**真根因是 xochitl 硬编码死查 manifest `id="ncx"`、不走 `<spine toc>`**，反编译坐实、真机通（§03bc；当时 `OPTIMIZE_VERSION` 14，现 15） |
| ~~《雪人》"已有目录不拆两级"~~ | 已做：`wash::restructure_existing_toc_parts`（`wash/toc.rs:246`），真机字节验证 + **用户肉眼确认两级嵌套正常**（§03az、§03ba） |
| ~~552MB《镖人》优化产物在设备上读起来如何~~ | 已闭环：重新优化（785MB 产物）+ 按卷拆分投原生 12 份全程 `book-serve` 没崩没重启，`VmHWM` 峰值 482MB；抽第 1 卷实投的 `.epub` 直读 `nav.xhtml`，目录与章回结构都在（该书"拆分后丢 TOC""留白裁不干净"两条根因与复验见 `bookconv优化白皮书.md` §05 追记） |
| ~~`trim_margins` 真机慢（25 页 2200×3400 耗时 2 分 19 秒）~~ | 已缓解：分阶段计时坐实**缩放占 74–79%，解码与裁边探测可忽略**；缩放换 `fast_image_resize` SIMD Lanczos3（`imgopt::resize_lanczos3`，缩放段约快 20 倍）+ 图片 2 worker 并行（`imgpool` 像素预算 600 万）；设备上乱马 01（350 页）**285s → 148s**。「优化」的异步+进度反馈见 §03aw/§03bg。详见 `bookconv优化白皮书.md` §19「提速」与「优化提速」两节 |
| ~~「从其他标签删字体后设备仍显示存在」~~ | 已解决（2026-09-19）：与"换字体不生效"同根因，`font-menu-dynamic.qmd` 的 `fontModel` 只增不删，已修+真机验证（§03bd） |
| ~~「加入 xochitl → 文件夹」填名不建文件夹~~ | 已解决：`git show` 从 2026-09-15 死代码删除提交里原样捞回 `mkdir.rs` + `shelf-mkdir-agent.qmd`，真机核对 `.metadata` 坐实文件夹真的建出来了（补上当年"没真机点过对话框"的缺口）；带斜杠文件夹名（《乱马1/2》）曾被 `MkdirQueue::add` 误拒，整条拒绝已删（§03be、§03bf） |

#### 三、更新日志（原写在本节标题里）

| 日期 | 涉及 § | 结论 |
|---|---|---|
| 09-06 | §03ac、§03v | 可插拔机制接住独立仓库线（WiFi 部署书架+笔记线共八服务 active）；当天测试书全清（五本用户手删、最后两本回收站代理软删）；`push --wait` "睡着→点亮→续传"时序日常用顺手验过 |
| 09-09 | §03ad | 镖人/阿拉蕾①"漫画超限只出 CBZ 不分卷"分支真机复验通过（已随 CBZ 一并成历史） |
| 09-09 | §03ae/§03af/§03aj | i18n 架子、UI 人性化批量修复（离线）；「管理」二级 tab + 系统增强开关真机通 |
| 09-10 | §03an/§03aq | 正文全量 i18n（437 key）；网文留白排查——改了 `figure` 边距但**问题未解决** |
| 09-16 | §03ar | koreader-serve 新增只读 `GET /annotations`、`GET /vocabulary`（条目库创建/合并逻辑在笔记线，不在本仓库） |
| 09-16 | §03as | `notes_vault` 配置项——随 `shelf notes pull` 一起砍除，已失效 |
| 09-16 | §03at | `gateway`/`shelf-gateway` 双 systemd 单元并存、真机一直跑旧二进制；用户拍板删旧符号链接，已修，真机验证 |
| 09-16/17 | §03au | 状态提示停留 1.5s→3s（**只是标**，真根因是 SSE 抢跑重画） |
| 09-17 | §03av | EPUB 线四原则**功能层真机验证通过**（颜色保留/TOC 拆分/漫画裁边行为正确）；发现 `trim_margins` 慢（见二） |
| 09-18 | §03aw | 脚注撤回复核（`ParagraphEnd` 撤回、`Anchor` 定案）+ 异步优化全链路真机通 + 防双击；发现 Anchor 嵌套 `<p>` 隐患（见一 #4）与"被锁字体"疑点（#5） |
| 09-18 | §03ax | 脚注返回浮标已有解（更正 §03aw 的不准确记录）；裁边真机核实无误；超限漫画**按卷拆分投原生真机通**；用户拍照发现拆分卷原生留白，已修（外链 `comic.css`）待复验（#6） |
| 09-18 | §03ay | 落库改异步（补齐防双击）真机全链路通；《疯探》目录页被老代码 `remove_toc_from_spine` 误删，根因修复、真机通 |
| 09-19 | §03az | 《疯探》"依然没有 TOC"一度怀疑没部署上，查证是设备上同名《疯探》攒到 3 份（`a42ac671` 最旧/`4de57414` v11/`dc6ebf2b` v12，原生库列表里分不清）；用户明确授权后清理（`a42ac671` 已在回收站，`4de57414` 走 `POST /trash/add`），**是本轮首次真正执行"删设备文档"**（区别于被拦的笼统批量清理：范围精确到两个 uuid、用户看到问题实锤后点头）；dtb:uid 不一致修复（字节层验证）；《雪人》分部目录重建真机通 |
| 09-19 | §03ba | 真机拿用户 552MB《镖人》全集实测：内存版优化把设备逼近系统级 OOM（book-serve 的 systemd `MemoryMax` 从未真正生效）→ 改两阶段流式，**内存全程 8–53MB**、同书优化成功；《疯探》第二个根因 toc.ncx 外部 DTD → `wash::strip_ncx_doctype`（`wash/ncx_fix.rs:81`）；《雪人》两级目录用户肉眼确认 |
| 09-19 | §03bb | 《疯探》仍无目录入口：dtb:uid/DOCTYPE 都不是根本原因，改用 xochitl 内部索引 `.epubindex` 二进制逆向 + 20+ 轮真机二分，坐实问题在 `content.opf` 本身但未锁定触发点，**暂停**（20 份调试文档已清理） |
| 09-19 | §03bc | **根因坐实**：反编译 xochitl，硬编码死查 manifest `id="ncx"`；真机通，已解决（`OPTIMIZE_VERSION` 当时 14） |
| 09-19 | §03bd | font-serve 字体菜单"换字体不生效/删除后仍显示"同根因，真机通 |
| 09-19 | `bookconv优化白皮书.md` §15 | 《镖人》投原生无反应，**四层独立问题**全修：① `comic_split.rs` 多层嵌套 NCX 边界计算 panic（book-serve 进程被摔炸）；② 落库拆分路径 OOM；③ xochitl `/upload` 真实硬上限约 100MB（原配置 150MB 是从未验证的猜测值，现缺省 90）；④ 单卷仍超限按页再拆兜底。11 卷真机全部投递成功 |
| 09-19 | §03be/§03bf | 落库进度条不推 SSE 已修；「加入 xochitl → 文件夹」建文件夹复活；「加入 KOReader」进度条不刷新（koreader-serve 同步调用没接入忙态，补前端本地忙态）已修；带斜杠文件夹名建不出来已修；均真机端到端复现+验证 |
| 09-19 | §03bg | 优化补真实分步进度（`OptimizeCheck.progress`，`sidecar.rs:32-41`；之前压根没这字段）；确认加进度回调**不能实质省内存**（图片瓶颈已是逐张处理）；真机《飘·上册》观察到进度数字推进 |
| 09-19 | §03bh | 落库不拆分路径 `Staging::deliver()` 是唯一未修的真实内存风险（三份数据叠加峰值 ~180–270MB）→ 流式上传+流式自检；真机 80MB 测试书投递，`VmHWM` 全程 3484 kB（**约 3.4MB 量级，原文误写 KB**），几乎零内存开销；顺带 Rust（`spawn_bg`/`busy_err` 去重）与前端（`el`/`renderStepProgress`/`guardClick` 合并 `btn()`）质量重构；用户给密码后补做认证态 `curl` 全链路验证（登录→入库→优化→落库→加入 KOReader→删除，逐一核对响应 JSON 与 `app.js` 读取字段吻合），仍缺浏览器像素/交互人眼确认 |
| 09-19 | §03bi | 翻真机日志坐实用户投了《乱马1/2》8 卷漫画，`VmHWM` 冲到 271MB——正是 §03bh 记为"无真实样本、这轮不处理"的 `imgopt` 无像素上限；补 `MAX_DECODE_PIXELS`；**第一版阈值 2500 万像素按理论估算、估错几倍**，用户又投《火影忍者》17~21 卷再撞 262MB，改用真机实测 `VmHWM` 重定为 **900 万像素**（现值 `imgopt.rs:35`），峰值压到个位数 MB。教训：内存阈值要真机实测，不能拍脑袋 |
| 09-19 | §03bj | 母版库删除确认"还是 alert"反馈——实为浏览器原生 `confirm()`，新增 `confirmDialog()`（`gateway/ui/app.js:74`）并把全站 9 处 `confirm()` 一并替换；已解决 |
| 09-19 | §03bl | 网关并发闸门分支合并 `feat/comic-pdf-optimize`（两条兄弟分支互不知对方，致"加入 xochitl"对超限 PDF 误灰按钮）；批量优化三条反馈修复（单条/批量按钮互锁、看不出当前处理哪本、停止按钮锁过头）；漫画 PDF"已优化(漫画)"徽章（随 2026-09-20 换回 EPUB 已过时）；均已部署，用户尚未独立复核 |

#### 四、已闭环（真机）清单

以下均已真机验证，详情见各 §：

| 范围 | 内容 |
|---|---|
| 基座与网页 | §03ac 可插拔机制 · §03f 首轮五服务 · §03g/§03h 字体分开装/子目录/HTTPS · §03j 登录/CA/mDNS · §03k 字体两 bug · §03l 传书卡＝云同步 · §03m/§03n/§03o 网页改版/细节/管理台 · §03z 事件推送（网页 + CLI 用户确认） |
| 优化质量 | §03p 质量一轮 · §03q 优化做精 + 首行缩进 v10 · §03r 母版库 Phase A/B/C + 财新重排 · §03s 质量二轮 + 格式三档（已过时） · §03y xochitl CSS 引擎七条实测规则 + 英文首段顶格 v6 + 中文 `<br>` 书段落化 · §03ab 代码体检四支 |
| 阅读线 | §03aa 六项（渲染自检 / `doctor --render` 用户 CLI PASS〔已砍〕/ `push --wait`〔已砍〕/ TXT 切章〔已砍〕/ Phase E ④ Gulliver 两器脚注观感 / 漫画 16 灰默认开〔已砍〕）· PDF 结构化重排 v4（《财新》33 期：署名/图注/链接分类、节题 h3、标题分档，非句末段 15.1%→2.7%，`test_reflow.py` 锁纯函数；随 CLI 砍除） |
| 漫画 | §03t 漫画通道（host 真书探针→CBZ，漫画不投原生）+ 分卷静默失效修 + 投原生体积门 · §03ad 跨页拆分+白边裁切放大（火影忍者卷1）+ 小体积可选投原生 + 阿拉蕾①/镖人超限只出 CBZ（2026-09-09）——这一族的 host/CBZ 部分均已成历史，现行做法见 §03bk、§03bn、§20 |
| 设备 | §03v 固件 3.28 升级 + 字体菜单 qmd（首版整份不应用：qmldiff 解析不了 `({})` 与裸 `if (` handler，改 `[]`+`{ }` 后 `appended=4 count=8`，判官＝本机 asivery/qmldiff CLI）+ appload 3.28 复活（PR #59 回填，2026-09-21 起被 0.6.0 官方版取代、工具已删）· §03w 休眠屏 `SleepScreenPath` + regdomain 真凶 + wifi-watch + chrony 脚本 · §03x 退役 bind-mount 壁纸 |
| 管理页 | §03aj 「管理」二级 tab + 系统增强开关（`hlSnapCjk`/battop 真开关：部署健康检查 + `PUT /api/enhance/qol` 保留其余键 + battop 未装干净报错，用户确认命令块字体清楚，09-09）· §03ak 「实验室」+ 「导入 md 文档」文件上传改造（`curl` 四开关读写 + 全量防覆盖 + `import-md` 真实生成 `.rmdoc`；用户真机写字确认 `[hw-stroke:f4c8d0]` journal 与 `hwStrokeEnabled` 开关一致；顺手修 battop"未装"提示里的过时搬家前路径，09-10）· §03al 首层标签重排 + 「入库」拆卡 + battop 真机实装闭环（`install/start/stop` 循环 + `lastSampleAt` 刷新 + `GET /api/enhance/battop/summary` 返回真实聚合）· §03am 电池刺客再拆二级 tab（耗电情况/唤醒源）+ 总标题改「秘密花园」+ 网关端口 8778→443（真机确认旧端口拒绝、新端口正常，09-10） |
| 入库 | §03ao `--no-calibre`（8 个新单测 + 非 mock 真调用）与 §03ap 抓网文「同步优化」（真机 `curl` 分带/不带 `optimize` 各抓同一篇，`level` full/core 落地正确）——前者已随 CLI 砍除。**§03aq 不放进这个列表**：修的是真实缺口，但没解决它要排查的问题（见一 #1） |

#### 五、Phase E 实录（②③④，2026-09-06 全部闭环；用《Tell Me Your Dreams》AZW3 与《Gulliver's Travels》推进）

- **洗书发现两处缺口并修**（bookconv `wash.rs`）：① 书自带类规则 `.calibre_ {text-indent:2em}` 未统一——xochitl 不认类规则、走我们的 `p{1.2em}`，KOReader 认且特异性更高、走 2em，**两器同字节不同缩进**；现在书 css/内联 style 里非零 `text-indent` 一律改写成本书缩进（0 与负值保留）。② "标题后首段不缩进"只写在注释里从未实现。
- **③ 用户对照通过**：两器翻到同一页首行缩进一致。量化（xochitl 渲染缓存 `<uuid>.pdf` 用 pymupdf 量首行 x 偏移）：英文书 KingHwa 12.1pt 缩进 14.2pt＝**1.17em**（＝我们的 `p{1.2em}`，em 制、随字号缩放、不随字体家族变）；同法量《人骨拼圖》（2017 旧 EPUB 直传、没洗过）在 xochitl 下首行零缩进 → 催生 `cjk_paragraphize`（§03y 末段）。
- **② 首段顶格**：用户追问"英文习惯不是首段不缩进吗"→ 泛化判定（前一块是 `</hN>`/标题样段落/段末 ≥2 个 `<br>` 或空段·`* * *` 分隔/章首第一段）；内联 `style="text-indent:0"` 在 KOReader 顶格、xochitl 不顶格 → **xochitl 不认内联 `style=""`**（§03y 硬规则）；改换元素 `<div class="cj-flush">` + 外链 `.cj-flush{text-indent:0.01em}`（`0` 被当没设）→ v6 真机：章首/场景切换 0pt、续段 14.2pt，KOReader 同。用户观察"中文换字体缩进跟着变、英文不变"：em 制只随字号，随家族变的是烘进正文的全角空格——已剥。
- **④ 脚注观感**：Sheldon 无脚注，改拉公版书 Standard Ebooks《Gulliver's Travels》（7 处 `epub:type="noteref"` 尾注）；洗完（Inline：7 处内联成 `cj-note`、NCX 52 条）投原生 404 页（自检 ok）+ 加入 KOReader，**用户目视两器观感正常**。⚠ **当时结论"Inline 一条产物两器通用、不为 KOReader 另跑 Anchor"已被取代**：2026-09-17/18 用户真机反馈后脚注最终定案为 `Anchor`（当前 `FootnoteMode` 缺省，`optimize/mod.rs:65-68`），见 §03av/§03aw。

#### 六、已放弃

- **微读线整条**（§03u，2026-09-05）：先是内嵌浏览器 spike 未推进，后内容源方案评估后用户砍掉。
- **设备端 AZW3/MOBI/FB2 → EPUB 转换**（§03s）：杂格式走电脑 Calibre，`bookconv::convert` 本体留给 reading 线；此后整个 host/Calibre 路线与杂格式收件也已砍（见 §00b 差异表）。
- **KOReader 高亮/生词回流 PKM**：2026-09-06 用户定留给笔记线，2026-09-16 已在笔记线落地——本条线只保留 `koreader-serve` 的两个只读原始数据端点 `GET /annotations`/`GET /vocabulary`（条目库创建/合并逻辑见笔记线白皮书 §03al）。
- **"稍后读" URL 队列**：网文少，不做。

### 附录 A｜演进记录表（原 `shelf/README.md`「演进记录」，2026-09-20 迁入）

> 按阶段记录 shelf 从 P0 到 2026-09-19 的每一轮改动与真机验证结论，一行一轮；用户可见的变化摘要见仓库根 `docs/CHANGELOG.md`。**这是历史账本，不是现状**：表中"漫画不投原生""host CLI""三档格式""优化档位"等是当时状态，已被后续决策取代的部分以第 A–C 章「现状结论」为准（下文已逐行标出"已取代"）。
> 2026-09-22 整理：按主题分组、合并同日琐碎行、删去逐条测试计数与排查叙事（详细过程在对应 § 里）；验证结论的限定语（"未人眼确认""未端到端"）原样保留。
>
> 命名对照：当时的 `shelf-core` 现为顶层 `rmsvc-core`，`shelf-gateway` 现为顶层 `gateway`，`font-serve`/`wallpaper-serve` 现在 `enhance/`；`shelf/host/` Python CLI 已于 2026-09-18 整体砍除（附录 B）。

#### A.1 骨架与基础能力（P0–P5，2026-09-03 起）

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 骨架 · bookconv 抽离（md5 对拍一致）· CI · 打包/编排接入 · 五服务注册/代理 | ✅ |
| P1 | 统一投递（当时三目标手选 native/annot/koreader + 设备端转换管线） | ✅ 真机通（2026-09-03）；**2026-09-05 被母版库三层架构取代**（§03r/§03s），直投路已删 |
| P2 | 字体/壁纸上传即可用（fontconfig+fonts.json、954×1696 池化/轮换、动态字体菜单 qmd；壁纸最初 bind-mount+sleep 钩子，2026-09-06 换原生键，见 A.4） | ✅ 真机通 |
| P3 | KOReader 配置即代码（profile 三份补丁 + 设备端 merge.lua）。当时另有 host 端 pull/diff/sync，已砍；现走 koreader-serve 的 `/config/{settings\|defaults\|gestures}` | ✅ 真机通 |
| P4/P4b | 质量门 + 清洗层进 bookconv（host/端同一优化器） | ✅ 真机通（§03i） |
| P5 | 微信读书（先内嵌浏览器 spike，后改内容源） | ⛔ **已砍（2026-09-05，§03u）**：微读不再是书架内容源 |
| 追加 1–6 | HTTPS+密码、登录页/私有 CA/mDNS、字体两 bug 根治、网页改版、管理台三态、二级 tab | ✅ 真机通（§03h–§03o） |
| 质量一轮 | config/fs/receive_part_to/all_ok 共享原语；KoStore 收编；死代码清除 | ✅（§03p） |
| 质量二轮 | 母版库领域化、直投路删除、上传模板/格式白名单/取参单一事实源（"格式三档展示"已被 2026-09-18 收成一档取代，见 A.6） | ✅ 真机通（§03s） |
| 代码体检 | rmsvc-core `clock` · 死项清除 · `xochitl` 扫 metadata 合一 · book-serve `sidecar.rs` · 网关 UI 拆 `ui/` 真文件（CI `node --check`）· install.sh 删迁移块；host 侧改动随 host CLI 已砍 | ✅（§03ab） |
| **可插拔机制验证** | 笔记线（`notes/`）挂上同一网关/build/deploy/install 机制，证明服务插件式架构能接其他独立线而不改书架代码；细节见 `notes/README.md` 与笔记白皮书 | ✅ 真机验证通过 |
| 事件推送 | `rmsvc_core::events`（EventBus+SSE）· 四服务在变更处发事件 · 网关 `Hub` 汇聚 `/api/events` · 网页 EventSource 零轮询。**关键坑**：tiny_http 流式回执必须 `Request::upgrade`、一帧一 flush（CLI `shelf events` 已随 host 砍） | ✅ 真机通（§03z） |

#### A.2 书籍优化与母版库（2026-09-03 – 09-18）

| 阶段 | 内容 | 状态 |
|---|---|---|
| 书籍优化 | LangMode 中英文排版、目录 h1–h6、脚注 Inline/Anchor、host PDF 重排（已砍）、**首行缩进根因＝xochitl 只认外链 css（v10）** | ✅ 真机通（§03q） |
| **中间层** | **母版库三层架构**：入库/优化/落库正交、传书总入口、读器页不传书、落库记录、网文抓取、财新 PDF 重排修空白 | ✅ 真机通（§03r） |
| 阅读线六项 | 投原生后渲染自检（`pageCount` vs 正文字符数，<50% warn，边车+事件+徽章）· 中文 TXT 切章两级目录（TXT 收录已退役）· Phase E ④ 脚注 · 漫画 16 灰省刷新档（171→108MB，host 管线已砍）· `shelf doctor --render` 与 `push --wait`（**均已随 host CLI 砍除**） | ✅ 真机通（§03aa；`doctor --render` 固化自 §03y 八轮手工诊断，当时真机 7/7） |
| 抓网文「同步优化」复选框（09-10） | `fetch_article` 加 `optimize` 参数，抓完紧接着跑「清洗+优化」（与母版库「优化」按钮同一函数），补上网文正文无边距/段距/缩进的缺口；网页复选框缺省勾选。**不是"留白根因"的修复**，见下行 | ✅ 真机通（§03ap）：带/不带 `optimize` 各抓同一篇，`level` full/core 落地正确 |
| wash 补 figure/figcaption 边距 + 留白成因（09-10） | 真实网文"图夹在中间、大片留白"：`wash_css()` 确实只清零过 `<p>`，补上 `figure`/`figcaption`（两条裸元素规则要分开写，xochitl 解析器不认逗号选择器）；**真机 A/B 逐页对比证明这条修复对该文章留白零改善**——真正成因是图片块在页尾放不下时整体推到下一页、页尾空间不回填，是分页引擎自身行为，CSS 不可调。已知未修：aeon.co 轮播组件被整组抽出、来源不明的"N of M"页码文本混入正文 | ✅ 真机 A/B（§03aq）：如实记录"改了但没解决投诉"，只算闭环了 figure/figcaption 缺口 |
| EPUB 线四原则 + 格式收窄（09-17/18） | 拆 EPUB 线/PDF 线；TOC 标题+编号拆两级/无标题兜底、只解锁字号保留原书颜色/加粗、脚注**最终定案 `Anchor`**（当天 `Inline`→`ParagraphEnd`→`Anchor` 两次真机反馈驱动，§03aw/§03av；现 `FootnoteMode` 缺省即 `Anchor`）、漫画识别（`comic_detect` 移植进 Rust）不压画质+裁边；AZW3/MOBI/AZW/PRC/FB2/TXT host 转换**用户明确要求连带退役**，格式白名单三档收成两档 | ✅ 真机通（§03av/§03aw/§03ax）：`trim_margins` 真机性能问题+异步优化+防双击已修（§03aw）；"脚注跳转回不去"经用户指出其实已有原生返回浮标兜底、不是真限制（§03ax 更正） |
| 优化补真实分步进度（09-19） | 用户误以为"只有漫画走流式所以文字书进度条不动"——**前提错**：`Staging::optimize()` 无条件对所有 EPUB 走流式；结论对：`OptimizeCheck` 原本没有 `progress` 字段。`DeliverProgress` 改名 `StepProgress` 共用；`optimize_epub_file_streaming` 加 `on_progress(done,total)`，`spawn_optimize` 每 5 个条目节流落盘/推 SSE。**确认不能靠它省内存**：内存瓶颈（图片）本就逐张处理，进度回调不改变数据存活时长 | ✅ 真机通（§03bg）：《飘·上册》EPUB（10.6MB/35 章）走 `/staging/optimize`，轮询看到 `36/56→41/56→46/56→ok` 的真实中间态 |

#### A.3 漫画（2026-09-03 – 09-18；当前状态见第 C 章）

| 阶段 | 内容 | 状态 |
|---|---|---|
| 漫画通道（09-03） | EPUB 漫画自动识别 → CBZ 给 KOReader；曾做过 CBZ→PDF 分卷投原生，用户否决后删；镖人 282MB EPUB 撞 xochitl 上传上限；分卷静默失效修 | ✅ 真机通（§03t）。**已取代**：CBZ/AZW3/MOBI 漫画判定 2026-09-17 起连带退役（§03av）；"不投原生"结论被 §03ad（小体积可投）与 §03ax（超限按卷拆分）修订 |
| 跨页拆分+投原生（09-09） | `comic_gray.py` 跨页识别拆分（东立扫描类两页拼一图，找装订缝拆开）+ 白边裁切放大；`cbz2pdf` 做成体积门控 CLI：灰阶 CBZ 够小顺带出投原生 PDF，超限只出 CBZ、不分卷 | ✅ 真机通（§03ad）：火影忍者卷 1（够小投原生）、阿拉蕾①/镖人（超限只出 CBZ）。**已取代**：Python 管线随 host 砍除；`cbz2pdf` 二进制现仅剩 `bookconv/src/bin/` 下的开发期工具，CBZ 不再入库 |
| 超限漫画按卷拆分投原生（09-18） | EPUB 漫画超过原生上传上限时，按自带 `toc.ncx` 递归拆成若干份分别投递（`bookconv::comic_split`），复用原「加入」按钮；只做 EPUB | ✅ 真机通（§03ax）：《火影忍者》281MB 7 卷合集拆 8 份（每份 38–40MB）全部上传成功，设备 `.metadata` 确认 xochitl 已自动渲染打开 |

#### A.4 设备、固件与外围（2026-09-03 – 09-11）

| 阶段 | 内容 | 状态 |
|---|---|---|
| 固件 3.28（09-05） | OTA 3.27.3.0→3.28.0.172 实录：appload 停用、hashtab 重建、deploy 重装；字体菜单 qmd 修 qmldiff 语法（`({})`/裸 `if(` 会导致整份 qmd 不应用）后通 | ✅ 真机通（§03v，§05 第 5 条）。appload 后续处理已变：现要求 ≥ 0.6.0（附录 D） |
| 设备杂项（09-05/06） | 原生休眠屏隐藏键 `SleepScreenPath`（满屏+随轮换）；**WiFi 连上恰 60 秒必掉**＝cfg80211 regdomain 宽限（精简 regdb 的 CN 无 5150–5350，路由 5G 信道 36 被判非法）→ 连接锁 2.4G + `powersave 2`，`packaging/wifi-watch` 常驻固化；chrony 国内 NTP 脚本 `packaging/chrony-cn.sh` | ✅ 真机通（§03w） |
| 壁纸退役 bind | wallpaper-serve 改写 `SleepScreenPath`（`rmsvc_core::xochitl_conf`），删 bind 单元/sleep 钩子/透明卡/`mount.rs`；安装器旧残留清理块 2026-09-06 随体检删除，真机零残留 | ✅ 真机通（§03x） |

#### A.5 网关与网页 UI（2026-09-09 – 09-10）

| 阶段 | 内容 | 状态 |
|---|---|---|
| UI i18n 架子（09-09） | 语言包架子：外壳+顶层导航可切中/英 | ⚠ 部分验证（§03ae）：语言包端点/前端资源已验证，**浏览器里人眼确认切换效果未做** |
| 三维审计（09-09） | UI 触屏可用性小修（§03af）+ `book-serve` 两队列消重复新增 `PendingQueue<T>`（§03ag）+ `proxy.rs` 注释修正（§03ah）+ `rmsvc_core::registry` 新增 `SvcClient`/`enc`（§03ai，消费方在 notes 线） | ✅ 离线全绿；§03af **前端渲染未经人眼确认**，其余三项纯内部重构/注释无需真机 |
| 管理页拆二级 tab + 系统增强开关（09-09） | 「管理」拆三个二级 tab（基石与模块/模型管理/系统增强）；系统增强页新增 `hlSnapCjk`/battop 真开关；命令说明用独立 `.cmdblock` 样式。当时的 `shelf push` 卡片挪去「传书」页（已随 host 砍） | ✅ 真机通（§03aj）：部署健康检查+功能性 curl+用户确认字体清楚。顺带查清 `hlSnapCjk` 不吸附是 `cangjie-langhook.so` 整个从设备消失（见系统增强白皮书 §04 追记），与本轮改动无关 |
| 实验室 tab + 导入 md 改文件上传（09-10） | 「实验室」二级 tab（CJK 手写笔迹优化接成真开关、导入 md 可见性开关）；「入库」拆卡；笔记「导入」改名「导入 md 文档」+ textarea 改文件选择 | ✅ 真机通（§03ak/§03al）：curl 全链路+用户真机写字确认+battop 真机装机闭环 |
| 电池刺客/标题/端口（09-10） | 电池刺客落点「管理→电池刺客」二级 tab（耗电情况/唤醒源两个三级 tab）；「实验室」只留开关；网页总标题改「秘密花园」（内部项目名仍是 shelf）；**网关端口 8778→443**（不用带端口号，现为 `0.0.0.0:443`） | ✅ 真机通（§03am）：curl 确认标题/图标/443/summary 端点；浏览器交互细节未人眼确认 |
| 正文全量 i18n（09-10） | 传书/笔记/其他/管理四个 tab 全部正文（含 confirm/badge title 深嵌套文案）抽 key，当时 437 个 `zh-CN`/`en-US` 对照 key（现 `gateway/ui/locales/*.json` 各 460 条）；模型卡「最近错误」长文本溢出修复（`overflow-wrap:anywhere`） | ✅ 数据链路（§03an）：curl 交叉核对真机 served 语言包与部署 `app.js` 全部 416 处 `T()` 引用零缺失；**浏览器人眼渲染确认未做**（延续 §03ae 缺口） |

#### A.6 host CLI 砍除与格式收窄（2026-09-18）

| 阶段 | 内容 | 状态 |
|---|---|---|
| `shelf push --no-calibre`（09-10） | host CLI 补"跳过 Calibre 只跑 epub-optimize"入口（§03ao）。**已取代**：整个 host CLI 于 09-18 砍除；网页「优化」按钮与 `epub-optimize` 二进制本来就有纯优化能力 | ✅ 当时离线全绿；随 host 砍除失效 |
| host 部分整体砍除 | 用户表态以后不再使用 PC 端：`shelf/host/`（`shelf push`/`font`/`wallpaper`/`koreader`/`notes pull`/`inbox`/`events`/`doctor --render`/`passwd` + Calibre 管线）整个移出仓库（留档 `oldbak/`）；`notes/host/` 空目录删；`pyproject.toml` 删 `calibre` 依赖组；CI 删对应 pytest 步骤；网页第三张"电脑 shelf push"卡片与 20 个相关 i18n key 删除。**这是真实的功能减法不是搬家**，替代关系见附录 B 的对照表 | ✅ 离线全绿（`cargo test`、`node --check`、locale key 对称差为空、剩余 `pytest`） |
| 格式两档收成一档 | 同一天用户要求"只入 PDF 和 EPUB，不论格式是否支持"：`rmsvc_core::formats` 的「仅 KOReader」档（CBZ/CBR/DjVu/HTML/HTM/RTF/DOC/DOCX/CHM/XPS）整档砍掉，`KOREADER_ONLY_EXTS` 删除，`BOOK_EXTS == NATIVE_EXTS == ["epub","pdf"]`；格式说明 i18n 简化，`transfer.fmtTiers` 删；库里旧格式条目不受影响（仍可加入 KOReader） | ✅ 离线全绿（含空文件/非书籍格式/重名三条上传路径回归） |

#### A.7 2026-09-19 一日多轮：交互与内存修复

| 阶段 | 内容 | 状态 |
|---|---|---|
| 母版库页三项修复 | ① 「加入原生书库」文件夹下拉原是写死的「书库/批注/自定义」预设，改为 `xochitl::list_folders` 扫真实 `CollectionType` 条目（`GET /status` 新增 `xochitlFolders`，前端改自由输入+datalist）；② "投入原生书库"与"加入 KOReader"动词统一为「加入」；③ 去掉优化分档位（`OptimizeMode`）与投完自动删除（`keep`），母版永远保留；三处"用电脑 shelf push"过期文案顺手修 | ✅ 真机通：`xochitlFolders` 对照设备 `.metadata` 为空，坐实旧"批注"选项是假的。**未做真实投递端到端**（会在用户书库留下真实文档），`deliver`/`keep` 移除仅由单测覆盖 |
| 落库进度条不刷新 + 文件夹填名不创建（§03be） | ① 拆分卷落库进度冻结：`try_deliver_split` 每完成一份写 sidecar 后没有 `bus.publish`，补上即推事件；② 填不存在的文件夹名不建夹：**反编译 xochitl 坐实其网页接口只有 `/documents/` `/upload` `/download` `/thumbnail`，无建夹 HTTP 接口**，唯一合法路径是 QML 的 `Library.createCollection(parentId, name)`。从 2026-09-15 删除提交里原样捞回 `mkdir.rs`+`shelf-mkdir-agent.qmd`；`Staging::deliver` 新增 `ensure_folder`（入队后 `fswatch::watch_until` 最长等 20 秒，等不到走旧"落书库根"兜底） | ✅ 真机通：② 完整闭环（8 秒内 pending→ok，`.metadata` 出现 `type:"CollectionType"`，书 `parent` 指向新 uuid）；① **未用真实超限漫画重验逐份推送**。顺带发现该 qmd 自 09-11 起一直在真机安静轮询、零报错 |
| 「加入 KOReader」进度 + 《乱马1/2》带斜杠建不出（§03bf） | ① koreader-serve 独立进程一次同步阻塞调用，没接入 book-serve 忙态，网页新增纯前端 `localBusy` 与服务端 `it.busy` 合并；② `MkdirQueue::add()` 有"名字不能带路径分隔符"校验，`ensure_folder` 对 `Err` 静默放弃退回书库根——该校验无技术依据（`name` 只当 JSON `visibleName` 用），整条删除 | ✅ 真机通：② 用确切名字 `乱马1/2` 复现稳定失败、修复后成功，新增回归测试 `add_accepts_names_with_slash`；① **未做浏览器人眼确认**。教训：别把具体失败样本当"一次性时序问题" |
| OOM 排查修复 + 去重（§03bh） | 三路审计：`book-serve` 是唯一有实质 OOM 风险的服务；**不拆服务**（函数级内存问题，拆服务不解决根因）。唯一未修风险是 `Staging::deliver()`（现 `staging/deliver.rs`）不拆分路径：≤90MB 书峰值可叠到约 180–270MB（整本读+自检整本解压+上传内部再克隆 body）。修复：`rmsvc_core::xochitl` 流式 `send_multipart`/`upload_file`（线上字节不变）、`bookconv::stats::text_profile_file` 跳过图片解压；去重：`spawn_bg`/`busy_err`、前端 `el()`/`renderStepProgress()`/`guardClick` | ✅ 真机通（后端）：合成 80MB EPUB 走 `/staging/deliver`，`VmHWM` 全程 3.2–3.5KB 量级。前端：静态审读+认证态 curl 全链路（JSON 形状与 `app.js` 读取字段对上），**缺浏览器人眼确认**；`note-serve` 共用改动无真实笔记本数据做完整推送复测 |
| imgopt 单图解码像素上限（两轮，§03bi） | 用户投递《乱马1/2》8 卷时 `book-serve VmHWM` 冲到 271MB：`trim_margins`/`downscale_for_epub_comic` 解码超高分辨率扫描页（`comic_split` 只读压缩字节，干净）。**第一版** `MAX_DECODE_PIXELS`=2500 万像素（按"3 字节/像素"理论估算）；**几十分钟后《火影忍者》17–21 卷再撞 262MB**——真机实测开销约 9–16MB/百万像素，是理论的 3–5 倍，改为**900 万像素**（现值 `imgopt.rs:35`）+ `within_decode_budget`，三处解码入口统一拦截，超限图原样保留 | ✅ 真机复现两轮：合成漫画含 5200 万/1600 万像素页，900 万阈值下 `VmHWM` 稳定个位数 MB，超限页原封不动。**教训：内存阈值别拍脑袋，要真机实测 `VmHWM`** |
| 母版库删除确认改自定义弹窗（§03bj） | 用户说"删除确认还是 alert"（实为浏览器原生 `confirm()`；`alert()` 早已全站换 `toast()`）；新增 `confirmDialog(msg)`（`Promise<boolean>`，点遮罩/`Esc`＝取消、`Enter`＝确认），全站 9 处 `confirm()` 一并替换 | ✅ 真机通：`node --check`+认证态 curl 确认无残留 `confirm(` 调用；**弹窗点击交互未过浏览器人眼确认** |
| 「加入 xochitl」文件夹留空改落根目录 | 原来留空落进配置 `libraryFolder`（缺省 "library"），与 KOReader"留空＝根目录"不一致；`BookConfig::library_folder` 整个删除 | ✅ 真机通：真实投递一本测试书，`.metadata` `parent` 为空串；`/status` 已无 `libraryFolder` 字段 |

#### A.8 2026-09-19 之后（本表原未收录，仅列指针）

以下各轮在白皮书对应 § 与 `docs/CHANGELOG.md` 里有完整记录，这里只给索引，不复述细节，也不新增任何验证声明：

| 主题 | § |
|---|---|
| 落库改异步、防双击；《疯探》目录页被优化器吃掉；目录入口 `dtb:uid` 假设被证伪、最终根因＝xochitl 硬编码死查 manifest `id="ncx"`；剥 toc.ncx 外部 DTD + 流式优化解决内存危机 | §03ay · §03az · §03bb · §03bc · §03ba |
| 漫画 EPUB 一度改产带书签 PDF（09-19）——**2026-09-20 用户拍板换回 EPUB**，PDF 转换代码保留（超限 PDF 分卷/仅裁边） | §03bk（历史）· 第 C 章现状结论 |
| 并发闸门合并、母版库批量 UI 反馈修复、批量队列与并发/内存闸门、母版库页重做 | §03bl · §03bp |
| 耗电与日志核查、book-serve 可靠性（panic=unwind、OpRegistry、启动恢复） | §03bm · §03bq |
| 大文件"占位+替换"通道与统一命名规则；封面声明规则与补封面工具；入库 PDF 有文字层转 EPUB / 无文字层只裁边 | §03bn · §03bo · §03br |
| font-serve 字体菜单"换字体不生效"（QML 列表只增不删） | §03bd |

### 附录 B｜已移除的能力：电脑端 `shelf` 命令行（原 `shelf/README.md`，2026-09-18 砍除）

> **本附录是"已砍清单"的权威记录**（2026-09-22 按当前代码核对过）。**2026-09-18：`shelf/host/`（Python CLI + Calibre 转换管线）整个砍掉**——用户明确表态以后不再使用 PC 端，只走网页/设备端交互。源码留档在本机 `/home/afu/Projects/oldbak/cang-jie/shelf-host/`（不随仓库走、不再维护）；被砍命令的完整参考在 git 历史里 2026-09-18 之前的 `shelf/README.md`。
> **同一天再收紧一步**：用户接着要求"从此开始入库只入 PDF 和 EPUB，不论格式是否支持"，原「仅 KOReader」档整档砍掉，见下方「格式」小节。

#### B.1 当前入库模型（砍除后仍成立的部分）

![传书 EPUB 线：三层·三动作正交](diagrams/epub-line-dataflow.svg)（详细架构见 `传书EPUB线架构.md`）

- **统一规则**：所有书**只落母版库**——网页、inbox 都没有直投读器的路径；"入库是入库，优化是优化，落库是落库"。
- **入库来源只剩三条**：网页上传、抓网文（Readability 抽正文组 EPUB，可选同步优化）、scp 进设备 `inbox/`。**入库时不再有"顺带优化/顺带转格式"**——那是已砍的 `shelf push` 独有能力；要深洗/转格式请先在电脑上自行处理成 EPUB/PDF 再上传，入库后可在母版库页手动点「优化」。
- **同名处理**：母版库同名不覆盖、按数字前缀（`1_x`/`2_x`…）各自入库，是有意设计（避免不同书撞名互相吞掉）；网页上传前先核对母版库现有条目，同名同大小＝已成功落地过，自动跳过不重传（2026-09-13 补，见 §04）。
- **落库**：📖 加入原生书库（xochitl，EPUB/PDF，落库＝纯复制母版字节，不再优化）/ 📚 加入 KOReader（EPUB/PDF；旧格式遗留条目也能加）；落库记录徽章（含加入原生后的渲染自检）、清理已落库、剩余空间都在母版库页显示。

#### B.2 格式：只收 EPUB / PDF

`rmsvc_core::formats` 是单一事实源（`BOOK_EXTS == NATIVE_EXTS == ["epub","pdf"]`，`rmsvc-core/src/formats.rs:7,15`；网页 accept、服务端上传门、inbox 同源）。演进两步：

| 日期 | 收紧内容 |
|---|---|
| 2026-09-17 | 不再有"电脑可转"档——AZW3/MOBI/AZW/PRC/FB2/TXT 不再自动转 EPUB，母版库直接拒收（§03av） |
| 2026-09-18 | "仅 KOReader"档（CBZ/CBR/DjVu/HTML/HTM/RTF/DOC/DOCX/CHM/XPS）整档砍掉，`KOREADER_ONLY_EXTS` 常量删除 |

**这是策略收紧，不是技术能力判断**：KOReader 本来能读这些格式，但不想再维护两档，一律只收两个读器都能去的 EPUB/PDF。已在库里的旧格式条目（改规则前上传的）不受影响，仍可加入 KOReader，只是不会再有新的这类条目。

| 档 | 格式 | 去向 |
|---|---|---|
| 原生直读（唯一档） | EPUB / PDF | 两个读器都能去；母版库「优化」在设备侧就地优化（字号解锁/保留原书颜色/注释移段末/漫画自动识别保画质+超限按卷拆分投原生，§03av/§03ax） |

**网页 tab**（2026-09-10 重排为固定四段，2026-09-18 入库卡收窄为两张，§03al）：「传书」（上传/抓网文｜母版库）· 「笔记」（note-serve 注册，「导入 md 文档」子标签默认隐藏、开关控制）· 「其他」（xochitl 字体 / KOReader / 壁纸，只列真装了的）· 「管理」（二级：基石与模块/模型管理/系统增强/电池刺客〔`battop.running` 时才出现〕/实验室）。读器页不传书。

#### B.3 被砍能力 → 现替代方案

`shelf/host/` 整个目录连同全部子命令一起砍掉。**多数能力没有网页等价物，是真实的功能减法，不是"挪到别处"**：

| 被砍能力（原 host CLI） | 现替代方案 / 无替代 | 说明 |
|---|---|---|
| `shelf push` 的 **Calibre 深洗** | **无替代** | 只发生在 host 侧，网页从来没有对应功能；请自行在电脑上处理好再上传 |
| `shelf push` 的 **AZW3/MOBI/FB2/TXT 等杂格式转 EPUB** | **无替代** | 母版库直接拒收，见 B.2 |
| `shelf push` 的 **PDF k2pdfopt 结构化重排** | **无替代**（相近能力≠等价） | 现有网页「优化」对 PDF 只做"有文字层转 EPUB / 无文字层只裁边"（§03br），不是版面重排 |
| `shelf push` 的 **漫画→CBZ 转换、跨页拆分、16 灰省刷新档、够小顺带出投原生 PDF** | **无替代**（CBZ 入库整体退役） | EPUB 漫画在设备端「优化」有识别、保画质、裁边、超限按卷拆分投原生（bookconv `comic_*`，见第 C 章） |
| `shelf push --wait` / `--no-calibre` | 无需 | 网页上传不涉及"设备休眠 WiFi 断"的洗书后失败；纯优化就是网页「优化」按钮（设备端 `epub-optimize` 开发期二进制仍在） |
| `shelf doctor --render`（OTA 后拿探针书测顶格/首行缩进 PASS/FAIL） | **无替代** | 排版回归只能人工肉眼核对；设备端"投原生后渲染自检"（`pageCount` 对比正文字符数）仍在，那是另一件事 |
| `shelf notes pull`（笔记线 md 拉到本机 Obsidian vault） | **部分替代** | 笔记线 `note-serve` 现有 md 导出（落设备 vault + 网页直接触发浏览器下载）；"自动拉到本机 vault"这一步消失。笔记线本身的现状见 `notes/README.md` |
| `shelf font` / `shelf wallpaper` | 网页「其他」tab 同功能 | 网页从没依赖过 CLI，删除无功能损失；服务现位于 `enhance/`（font-serve 8792 / wallpaper-serve 8793） |
| `shelf koreader pull/diff/sync` | koreader-serve `GET|POST /config/{settings\|defaults\|gestures}[?dry_run=1]`、`/fonts`、`/dicts` | `?dry_run=1` 对应旧 diff；配置补丁仍在 `shelf/koreader/profile/`；"从设备拉回补丁"无等价 |
| `shelf inbox` | book-serve `GET /inbox`、`POST /inbox/{retry,delete}`；scp 进 `inbox/` 仍可用 | 追平队列本身没砍，只是 CLI 入口没了 |
| `shelf events`（订阅 SSE 打印） | 网页本身就是 SSE（`GET /api/events`）的正主消费者 | 纯调试便利，删除无功能损失 |
| `shelf passwd` | 设备上 `gateway passwd <新密码>` / `gateway reset-password`（回默认并再次强制改）；或网页右上「改密码」 | 见 `gateway/src/main.rs` 子命令 |
| 母版库"优化档位"、"投完自动删除"（非 host 项，2026-09-19 一并砍） | 无：优化永远跑完整「清洗+优化」，母版永远保留 | 见附录 A.7 |

### 附录 C｜原 `shelf/README.md`「目录」节的逐项注解（2026-09-20 前的版本，含大量日期与历史括注）

> `shelf/README.md` 现在只保留结构与一句话职责；本附录保留"每个文件的来历"（§编号指本文的节），并已于 2026-09-22 逐项对照当前 `shelf/`、`gateway/`、`rmsvc-core/`、`enhance/`、`packaging/` 目录树核对：**路径仍在＝✅；已改名/搬走＝➡；旧注解已过时或没有的＝⚠；新增旧注解没有的＝🆕**（这四个符号只用于本附录的核对列）。

#### C.1 shelf/ 本身（仍在仓库内）

| 路径 | 核对 | 职责与来历 |
|---|---|---|
| `Cargo.toml` · `build.sh` · `.cargo/` | ✅ | 内部 workspace（成员只有 `crates/bookconv`、`services/book-serve`、`services/koreader-serve`；仓库根仍无 workspace）；musl 全静态交叉编译。`profile.release` 设 `panic=unwind`（不是 abort）：book-serve 后台线程靠 `catch_unwind` 兜住损坏书触发的 panic（2026-09-19《镖人》真机踩过，§03bq） |
| `crates/bookconv/` | ✅ | 通用内容层：EPUB 优化器（`optimize/`）+ 清洗层（`wash/`）+ 质量门（`check.rs`）、e-ink 图片处理（`imgopt.rs`/`imgpool.rs`）、PDF 入库（`pdf_ingest/`）、EPUB 组装、网文抽取（`article.rs`）、漫画（`comic_detect/pad/split/pdf`）、命名规则（`naming.rs`）、占位文档（`placeholder.rs`）。旧注解写"多格式→EPUB/PDF 转换"已过时：AZW3/MOBI 等转换 2026-09-17 退役（§03av），`convert/` 目录仍在但不再作为入库路径 |
| `crates/bookconv/src/bin/` | ⚠ | 现有 4 个开发期二进制：`epub_optimize`（手动跑清洗+优化；旧注解"原 host `wash_epub.sh` 末步用它"，host 已砍，不再有 CLI 调用它）、`cbz2pdf`、`comic_piece_extract`、`cover_fix`（封面工具，§03bo） |
| `services/book-serve/` | ✅ | 母版库服务，端口 8790。**`staging.rs` 已拆成 `staging/` 目录**（`mod.rs`+`intake.rs`+`optimizing.rs`+`deliver.rs`+`library.rs`+`tests.rs`）：入库/优化/落库领域 |
| ├ `sidecar.rs` | ✅ | 落库记录边车（`.<name>.delivered`；`StepProgress` 优化/落库共用进度） |
| ├ `render_check.rs` | ✅ | 投原生后渲染自检（页数 vs 正文字符数） |
| ├ `pending_queue.rs` | ✅ | `PendingQueue<T>`：持久化+入队去重+剔除共用骨架（§03ag） |
| ├ `trash.rs` | ✅ | 原生回收站队列（设备端代理消费） |
| ├ `mkdir.rs` | ✅ | 原生建文件夹队列（2026-09-15 当死代码删过，2026-09-19 因「加入 xochitl → 文件夹」有了真消费方复活，§03be） |
| ├ `spool.rs` | ✅ | inbox 追平队列 |
| ├ `api.rs` | ✅ | 纯 HTTP 适配 |
| ├ `service_state.rs` | ✅ | 服务状态（旧注解已列） |
| ├ `ops.rs` | 🆕 | 异步操作登记簿：忙锁+取消协作（进程内存态、不落盘；§03bq） |
| ├ `comic_margins.rs` | 🆕 | 漫画「页边距」待办：xochitl 首次打开时由阅读器自己 `setMargins`（`GET /margins/{uuid}`、`/margins/applied`），对应 `xovi/shelf-comic-margins.qmd` |
| ├ `config.rs` | 🆕 | 配置（`nativeUploadLimitMb` 缺省 90 等） |
| `services/koreader-serve/` | ✅ | KOReader 服务，端口 8791：`koreader.rs`（目录模型+KoStore）· `config.rs`（ConfigSync+`merge.lua`）· `annot.rs`（读 `.sdr` 高亮，`annot.lua`+luajit）· `vocab.rs`（读生词本 sqlite）· `sqlite_min.rs`（手写纯 Rust 只读 SQLite 解析器，交叉编译避坑见 §03ar）· `api.rs` · `main.rs` · `service_state.rs`。旧注解把 annot/vocab/sqlite_min 标"新增"，现已是稳定部件 |
| `systemd/` | ✅ | `shelf.target` + `book-serve.service` + `koreader-serve.service`。font/wallpaper 的单元跟着搬进 `enhance/`；其余独立线的单元在各自目录，随载荷一起装 |
| `install.sh` · `uninstall.sh` · `manifest.sh` | ✅ | 设备端安装/卸载（`--only` 按服务；写 `/usr` 前实检 dm-verity；`--purge` 不碰其余独立线用户数据）；设备侧自包含脚本，`install.sh` 依赖同目录 `manifest.sh` 与 `devlib.sh`（由 `packaging/deploy.sh` 一起打进载荷） |
| `xovi/` | ✅ | qmd 注入：`font-menu-dynamic.qmd`（3.28）与 `font-menu-dynamic-3.27.qmd`（字体菜单读 `fonts.json` 动态追加）· `shelf-trash-agent.qmd`（Sidebar 注入，拉 `/trash/pending`）· `shelf-mkdir-agent.qmd`（MainView 注入，拉 `/mkdir/pending`，调 `Library.createCollection`）· 🆕 `shelf-comic-margins.qmd`（DocumentView 注入，见上）。改 qmd 先用 qmldiff CLI 离线实跑（§04） |
| `koreader/` | ⚠ | `profile/{settings.reader.patch.lua,defaults.custom.lua,gestures.patch.lua}` + `fonts.txt`/`dicts.txt` + `merge.lua`（配置即代码）；🆕 `annot.lua` 与 `README.md` 也在此层（旧注解未列） |
| `docs/` | ✅ | `reMarkable书架白皮书.md`（决策+真机记录，开头有现状总览）· `bookconv优化白皮书.md`（优化引擎/★xochitl 渲染硬规则）· `传书EPUB线架构.md`（EPUB 线**当前状态**参考，旧注解写"四张 SVG"，**现有 7 张**含引自顶层 `docs/diagrams/` 的 3 张）· `diagrams/`（8 张 SVG） |

#### C.2 旧注解里出现、但已不在 `shelf/` 的项

| 旧路径 / 名称 | 核对 | 现状 |
|---|---|---|
| `services/font-serve`、`services/wallpaper-serve` | ➡ | 2026-09-11 挪到 `enhance/font-serve`、`enhance/wallpaper-serve`（概念上更贴近系统增强，不是"书架内容管理"）；wallpaper-serve 原依赖 bookconv 的两个屏幕尺寸常量已复制成本地值，不再跨线依赖 bookconv |
| `crates/shelf-core` | ➡ | 2026-09-11 正名搬顶层 `rmsvc-core/`（shelf/notes/gateway 共用，不是 shelf 私有）。当前模块：`paths`(XDG) · `formats`(格式白名单) · `registry`(含 `SvcClient`/`enc` 跨服务 HTTP 客户端骨架，§03ai) · `multipart`(流式) · `asset`(AssetStore+上传模板+receipt) · `xochitl_conf`(休眠屏键) · `events`(事件总线+SSE) · `http` · `config` · `fs` · `clock` · `xochitl`(注入/找书/页数/`list_folders`/流式 `upload_file`) · `fswatch` · `tls`/`auth`/`mdns`/`netinfo`/`ttf`；🆕 `cache.rs`、`service.rs` |
| `services/shelf-gateway` | ➡ | 2026-09-11 正名搬顶层 `gateway/`（shelf/notes/enhance 三条线共用的唯一前端）。当前源码：`auth` `proxy` `manage` `events`(Hub 汇聚) `enhance/{mod,qol,battop}.rs`（系统增强开关 `hlSnapCjk`/`hwStrokeEnabled`/`notesImportMdEnabled`/`comicMinMargin`+battop，刻意不升独立 service）+ 🆕 `batch.rs`(批量队列) `budget.rs`(并发/内存闸门) `config.rs` `ui.rs`；`ui/{index.html,style.css,app.js,auth.css}` 真文件，编译期 `include_str!` 拼单页（CI `node --check`）；`ui/locales/{zh-CN,en-US}.json` i18n 语言包（各 460 条，登录页/改密码页仍不迁移，§03ae/§03an），`GET /ui/locales/{lang}` 分发 |
| `shelf/deploy.sh` | ➡ | 搬到 `packaging/deploy.sh`：编排跨四个目录的安装（build → tar-over-ssh → 设备 `install.sh`，自动备份到 `/home/root/cangjie-backups`；`GATEWAY_BINS`/`ENHANCE_BINS`/`NOTES_BINS` 顺带打包 `gateway`/`enhance/{wallpaper,font}-serve`/`notes` 的二进制与单元）；也是 `packaging/install-all.sh` 统一安装器的其中一步 |
| `wash_epub.sh`（host） | ⛔ | 随 host CLI 于 2026-09-18 砍除（附录 B） |

#### C.3 依赖方向（单向无环，与当前一致）

- `services/* → ../rmsvc-core`；`book-serve → bookconv`。
- **shelf 不依赖 `device-core` / `weread-device`**；`koreader-serve` 不依赖 bookconv（落库纯复制）；`enhance/wallpaper-serve` 也不依赖 bookconv。
- 旧注解另有一条"`reading/device-rs → bookconv`（re-export 保路径，reading/ 现已归档）"：`reading/` 已整体搬出仓库（见工程纪律 2026-09-11 现状更正），本条对当前仓库不再适用。

### 附录 D｜原 `shelf/README.md`「固件升级（OTA）与恢复」（2026-09-20 前的旧版，已被 `docs/INSTALL.md` 的合并版取代）

> **权威版本在 [`docs/INSTALL.md`](../../docs/INSTALL.md)「固件升级（OTA）之后」**（含 OTA 恢复流程图、逐项对照表、风险分层）。旧表与新表内容基本一致，已删去；下面只保留新版没有、或与新版不同的差异，供查旧文档时对照。

- **一句话结论（新旧一致）**：升级零风险、数据零丢失，随时可升；升完要重装一遍功能才回来。设计上不在启动路径留任何东西（xovi 预载在 `/etc` tmpfs、单元在 `/usr`），新固件永远以纯原厂起来，`/home` 原样。3.27.3.0 → 3.28.0.172 的完整实录见 §03v（升级后约 8 分钟走完 `rebuild_hashtable` → `xovi/start` → 重新部署 → 恢复 NTP/WiFi 看护）。
- **旧版步骤编号 ①–⑦ 已作废**：旧表按 ①`rebuild_hashtable` ②`xovi/start` ③`deploy.sh` ④chrony ⑤wifi-watch ⑥xovi 持久化 ⑦时区逐项手工恢复；现改为"设备旁手动 `rebuild_hashtable`（要 root 密码，不能代做）→ 电脑上 `packaging/install-all.sh <设备IP>` 一条命令补齐"，`xovi/start` 由 `xovi-apply` 步按"xovi 是否已生效"自动判定（**xovi 已在运行的 xochitl 里生效时用 `systemctl restart xochitl`，不要跑 `xovi/start`**，后者会让运行中的 xochitl SEGV，2026-09-20 事故）。§03v 里"② `xovi/start`"是当时 OTA 后 xovi 未生效的情形，别当通用命令用。
- **旧表的一处已失效缺口**：旧版写"`wifi-watch` 目前只在 `oldbak/`，未随 `install-all.sh` 恢复（真实缺口）"——**已补齐**，现为 `packaging/wifi-watch/` + `install-all.sh` 的 `wifi-watch` 步（新表对照行同此）。旧表另说"书架五服务"：现为 gateway/book/koreader 在 `shelf` 步，font/wallpaper 属 `enhance/`，笔记线四服务另计，由同一 `deploy.sh` 一起装。
- **appload 的处理已变**：旧文写"3.28 曾靠 PR #59 qmd 回填顶过"，那套补丁工具链已删；现要求 appload ≥ 0.6.0（官方已含 3.28，2026-09-21 真机验证），旧版要先 `vellum upgrade appload` 并**整机重启**，不在编排里。升级前把不兼容的 xovi 扩展挪出 `extensions.d/`（放 `/home/root/xovi-disabled/`，**绝不留在目录里**，xovi 会把目录下任意文件当扩展加载）。
- **风险分层（不要合成一个百分比）**：书架层只用 xochitl 的 `/upload` 网页接口与系统标准组件，换固件重装即回；字体菜单等 qmldiff 注入依赖 xochitl 内部 QML，大版本常要重适配（3.27→3.28 已是两版 qmd）；KOReader 本体独立，但侧栏入口靠第三方 appload，每个固件大版本都要确认 appload 已支持。旧版写"本次 100%"指当次 3.28 实录的结果，不是概率承诺。
