# 传书模块架构文档

> **这份文档管什么**：书在设备上**怎么流动、每个服务管什么**——数据流、母版库状态、落库通道、内存设计、异步与进度、并发与锁、全部 API 与配置。它是"现在怎么运转"的参考，不是会话日志。（文件名里的"EPUB 线"是历史叫法：当年 EPUB 走深度优化、PDF 另走一条转换线；现在两种格式都只是原样投递。）
>
> **2026-10-07 起书架不再优化书**（用户定）：书的优化全部在电脑上用 **sheng-ren** 做（[github.com/bbq191/sheng-ren](https://github.com/bbq191/sheng-ren)，电脑端工具 booklib，`xochitl` 阅读模式出 EPUB），书架只负责把**已经优化好的书原样投进 xochitl**。book-serve 随之删掉了「优化」、入库 PDF 转换、原 PDF 备份、抓网文、联网补封面、旧产物兼容预处理、中途取消，见 §11。**本文按分支 `feat/drop-book-optimize` 的代码写：已合 master（15b4069），10-07 已部署、部署自检通过，功能未真机手测。**
>
> **书籍相关文档的分工**（涉及别处的内容，本文只写一句并指过去）：
>
> | 想知道 | 看哪份 |
> |---|---|
> | 书该被优化成什么样（排版、注释、漫画） | **sheng-ren 仓库** `docs/typesetting.md`；xochitl 阅读器的实测踩坑（只认外链 CSS、书内跳转规则、目录查找等）在 `docs/xochitl.md` |
> | 书在服务间怎么流动、API 与配置 | 本文 |
> | 真机排查经过与历史决策（含已删除的设备端优化） | [`reMarkable书架白皮书.md`](reMarkable书架白皮书.md) |
>
> 本仓库原有的《EPUB 优化规范白皮书》《bookconv 优化白皮书》2026-10-07 删除（规则在 sheng-ren，xochitl 踩坑已搬到 sheng-ren `docs/xochitl.md`）；要看原文：`git show 76fd031:shelf/docs/EPUB优化规范白皮书.md`（`bookconv优化白皮书.md` 同理）。
>
> **给谁看、先看哪几节**
>
> | 你是 | 先看 |
> |---|---|
> | 第一次接触书架 | [`../../docs/OVERVIEW.md`](../../docs/OVERVIEW.md) → 本文 §0、§1 架构图、§2 数据流 |
> | 要改 book-serve 的某个动作 | §2 对应小节 → §7.3 锁表（先确认会不会和别的动作撞）→ §9 接口 |
> | 查"书为什么没加入 / 卡住了" | §2.3 决策树 → §6 大文件通道 → §10 已知限制 |
> | 查内存 / 耗电问题 | §5 内存表 → §7 刷新机制 |
>
> **核对时点**：2026-09-19 首写，每轮按源码逐字段复核；最近一次 **2026-10-07**（按书架不再优化书的代码重写；已合 master 15b4069，10-07 已部署、自检通过）。文中"真机验证"沿用当时的记录；2026-09-30 及以前的部署记录见书架白皮书「现状总览」与附录 §05。

## 0｜这是什么

"传书"是书架（shelf）里"把书弄到 xochitl 里"的统称。用户视角只有三步：

1. **电脑上优化**：用 sheng-ren 的 booklib 按 `xochitl` 阅读模式把书优化成 EPUB（PDF 不处理也能直接传）。这一步不在本仓库。
2. **上传**：网页「传书 → 入库」上传，或 scp 进设备的 `inbox/`。书原样进**母版库**。
3. **加入 xochitl**：在「传书 → 母版库」勾选，点「加入 xochitl」。

![一本书的旅程：电脑上优化 → 上传 → 母版库 → 加入 xochitl](diagrams/book-journey.svg)

书架只收 EPUB/PDF，其它格式一律拒收（`rmsvc_core::formats`）。**书架不改书的内容**：入库、加入 xochitl 都是复制字节；唯一会动的是**文件名**（规范成 `书名 - 02卷`，§2.1）。母版库是唯一容器，网页没有"跳过母版库直接投读器"的路径。

### 0.1 术语小抄

| 词 | 一句话 |
|---|---|
| xochitl | reMarkable 官方阅读器/UI 进程；本项目不改它本体 |
| sheng-ren / booklib | 姊妹项目，电脑端的书籍优化工具；书架上的书应该先经过它 |
| 母版库 | 设备上永久保存原书的暂存池（`staging/`）；落库＝把书**复制**给 xochitl，母版字节不动、可反复加入 |
| sidecar（边车） | 母版库每本书旁的隐藏 JSON `.<书名>.delivered`（书名太长时是短名，§2.2），记落库与渲染自检结果 |
| qmd | 对 xochitl QML 的补丁（qmldiff 格式），让"xochitl 自己"做外部进程做不了的事 |
| SSE / `VmHWM` | 服务端事件推送（网页不轮询）/ 进程内存峰值（`/proc/<pid>/status`，判断内存问题的权威数字） |

## 1｜整体架构

![传书模块：系统架构](diagrams/epub-line-architecture.svg)

网关和 book-serve **都跑在设备上**（systemd 单元见 `shelf/systemd/`、`gateway/systemd/`，二进制在 `/home/root/.local/bin`），所以 `book-serve` 能直接读写 xochitl 书库目录：

| 组件 | 角色 | 端口/连接 |
|---|---|---|
| `gateway` | 唯一 Web 前端：网页 UI + 反向代理 + **批量队列（`batch.rs`）+ 并发闸门（`budget.rs`）** + SSE 汇聚 | `0.0.0.0:443`（HTTPS，登录墙） |
| `book-serve` | 母版库领域服务：入库 / 改名 / 删除 / 下载原件 / 加入 xochitl，外加三个设备端代理队列（§4） | `127.0.0.1:8790`（只经网关访问） |
| `shelf-conv` | **库，不是服务**，被 `book-serve` 进程内调用，**只读不改书**：`pdfmeta`（第三方 PDF 页数）、`placeholder`（大文件通道的占位文档、读 EPUB 翻页方向）、`stats`（渲染自检的期望页数）、`naming`（文件名规范化） | 无网络面 |
| sheng-ren `bookconv` | git 依赖（`shelf/Cargo.lock` 记提交，`cd shelf && cargo update -p bookconv` 跟进）。书架**只用它读 EPUB 的公开接口**（zip、OPF、封面、正文文字、页边距标记名），不调用优化器 | — |

**网关代理**（`gateway/src/proxy.rs`）：`/api/{svc}/*` 按 URL 段（`books`→`book-serve`、`fonts`→`font-serve`、`wallpapers`→`wallpaper-serve` 等，唯一的表是 `gateway/src/manage.rs` 的 `MODULES`；`koreader` 段 2026-09-29 撤掉）转发到 loopback。**请求体真流式**（大文件上传不占网关内存）；**响应**：后端给了 `Content-Length` 的 200 应答，若是下载（带 `Content-Disposition`）或体积超过 256KB（`STREAM_MIN_BYTES`），网关按定长边读边发；其余小 JSON 与没有长度的应答读完再回（2026-09-24；没有长度的流只能走 SSE 那种"读到连接关闭"的通道，拿来做下载会让浏览器等不到结束）。只有「加入 xochitl」（`POST staging/deliver`）会额外读一次小 JSON 并过并发闸门（§7.2；2026-10-07 前「优化」与勾了「同步优化」的抓网文也过）。

**`book-serve` → `xochitl` 直连，不经网关**：`Staging::deliver()` 用 `rmsvc_core::xochitl::Xochitl` 连 `10.11.99.1:80`（USB 网口地址，配置键 `xochitlHost`）的 `/upload`，大文件通道还直接读写书库目录。与"浏览器 → 网关 → book-serve"是两条独立的边，别混成一条。

## 2｜数据流

> **大白话**：书"原样"进母版库（永久保存的原书仓库），之后「加入 xochitl」是在母版上另外点的动作，可以反复点。

### 2.1 入库（内容源 → 母版库）

两条入口，都是**原样入库，不做任何格式转换或内容改写**：
- **网页上传**：`uploader()`（`gateway/ui/app.js`）逐文件 XHR 真实进度；传 `dedupeApi` 按 `name|bytes` 去重（免落成 `xxx_1.epub`）。服务端 `rmsvc_core::multipart` 流式解析边读边落盘（暂存 `.work/` 下随机名 `.<uuid>.book.part`，rename 入库；只在"挑名 + 改名"那一瞬进落名临界区，§7.3）。
- **scp 进设备 `inbox/`**：`fswatch` 监听（8 秒防抖）自动追平（§2.2）。**只收写完的文件**（2026-09-25）：scp 直接往最终文件名里写，追平时修改时间离现在不足 5 秒（`INBOX_SETTLE`）的文件先跳过，写完那次 `CLOSE_WRITE` 事件触发的下一轮再收；此前 WiFi 传大书超过 8 秒，还在写的文件会被改名进母版库，写入方的文件句柄跟着 inode 继续写，母版库里出现一本半截书。启动时先起监听、再扫一遍启动前就在的文件，扫描遇到"暂缓"的隔 5 秒重扫，最多 12 轮（约 1 分钟），之后交给事件。判据只看"还在不在写"，scp 中途断开留下的半截文件静止 5 秒后同样可能被收进。

  ![scp 往 inbox 传大书：什么时候才入库](diagrams/sh-inbox-settle.svg)

非 EPUB/PDF 回执"不是书籍格式，母版库只收 .epub .pdf"。（2026-10-07 前还有第三条入口「抓网文」，网页链接现在请用 sheng-ren 收书、优化好再传；电脑端命令行 2026-09-18 已砍。）

**同内容去重**（2026-09-28）：落名前若母版库里已有同（规范）名、且内容逐字节相同的书，直接认那一本，不再落一份 `1_书名`；同名不同内容（另一个版本、或 sheng-ren 重新优化过的）仍加数字前缀，不覆盖、不丢。

**书名规范化**（`shelf_conv::naming::canonical_file_name`，内部用 sheng-ren `bookconv::naming::canonical_book_name`）：入库直接用规范名 `书名 - 02卷`（去掉下载站 `-- 作者 -- … -- hash` 尾巴与 `[完]`；无卷标记原样；幂等）；撞名加数字前缀不覆盖（`unique_path`）。**只改文件名、不改书里的 `dc:title`**——xochitl 显示的书名取书内元数据，由 sheng-ren 决定（大文件通道的占位例外，§6.1）。

**长书名**：中文 80 来个字的书名加 `.epub` 已接近单段文件名 255 字节上限。2026-09-30 起不再因此失败：`rmsvc_core::fs::write_atomic` 的临时名只保留目标名前 200 字节（`TMP_BASE_MAX`，按字符边界截），母版库自己的临时文件改成与书名无关的 `ScratchFile`（§2.2），边车名超长时改用短名（§2.2「边车文件名」）。

### 2.2 母版库（中间层暂存池）

目录 `$XDG_STATE_HOME/shelf/books/`（设备上 `~/.local/state/shelf/books/`，/home 分区，重启/OTA 不丢）：

| 路径 | 作用 |
|---|---|
| `staging/` | 母版库本体，条目**永久保留**，落库不自动删、不套 LRU（可反复加入） |
| `inbox/` → `.work/` → `staging/` 或 `failed/` | `spool.rs` 追平队列（同一时刻只跑一轮，spool 锁，§7.3）：`inbox/` 待处理 → `.work/` 处理中（兼上传暂存）；`failed/` 封顶 50MB，带 `<name>.reason`；重试＝人工拷回 `inbox/`（无 HTTP 接口） |
| `mkdir-pending.json` / `trash-pending.json` / `comic-margins.json` | 设备端代理队列（§4） |
| `staging/.pdf-originals/` | 2026-10-07 前「PDF 转 EPUB」留的原 PDF 备份（7 天）。功能已删，book-serve 不再读写；旧设备上可能还剩几份，到期也不会自动清，可手动删 |
| `done/` | 2026-09-03 早期直投流程的遗留目录，代码已不再读写；设备上仍有几份旧文件，未清理 |

`Staging`（`book-serve/src/staging/mod.rs`；动作分在 `intake.rs`/`deliver.rs`/`library.rs` 各自的 `impl Staging`）核心字段：`dir`；`xochitl: Arc<Xochitl>`；`native_limit: u64`（体积门，字节，§5）；`ops: OpRegistry`（`ops.rs`，**进程内存态**忙锁登记簿，按书名，不落盘，容忍 poison）；`caches: ListCaches`（列表用的两份按文件戳失效的缓存，见下）；`land`（落名临界区的锁，§7.3）；`comic_margins`（页边距待办队列，§3）。

**忙锁 vs sidecar 是两套独立职责**：`ops` 答"现在有没有线程在跑"，重启即清零（有意，落盘会"永久卡忙"）；sidecar 答"上次加入跑到哪/结果如何"。同一条目「加入 xochitl」「改名」「删除」互斥（同一张 `OpRegistry`；删除 09-25 起在删的那一刻也占着忙锁），冲突回 400"《…》正在处理中，请稍候"。全部锁一览见 §7.3。

**跨分区入库**（`stage_from_path` 的 rename 失败时，2026-09-25）：先在落名临界区**外**把字节拷进母版库目录下的点前缀临时文件，再回临界区挑名、同目录 rename；此前直接往最终名上拷，拷贝期间列表里就有一本半截书。**临时文件**：母版库目录里的中转文件一律是 `ScratchFile`，名字 `.<pid>.<序号>.<种类>.tmp`，**与书名无关**，Drop 时自动删；现在只剩一种 `landing`（跨分区入库中转，`SCRATCH_KINDS`；2026-10-07 前还有优化产物 `optimizing`、补封面副本 `cover`、兼容预处理 `legacy`）。

**重启修正**：崩溃/OOM/断电可能留 `pending`，启动时 `recover_interrupted` 把落库记录改为 `failed`（"服务重启，上次加入被中断，可重新加入"）、渲染自检改为 `timeout`，并删掉母版库目录下**所有"点前缀 + `.tmp` 结尾"的普通文件**（跨分区中转、旧版留下的优化半成品、边车原子写没来得及改名的残留）；`gc_orphan_sidecars` 清孤儿边车，新书落地前也清目标名旧边车。

**边车文件名**（`sidecar::file_name_for`，2026-09-30）：书名 → 边车名只有这一个函数，读、写、改名、删书、列表缓存的文件戳、孤儿清理、启动修复全走它。`.<书名>.delivered` 不超过 255 字节时就是这个名字；超过时（书名约 244 字节以上）改成 `.<书名按字符边界截到 200 字节>.<书名 sha256 前 16 位 hex>.delivered`。短名没法从文件名反推书名，所以孤儿清理与启动修复先用 `sidecar::owners` 列出目录里每本书的边车名建反查表，再按表认主。

**列表缓存**（`staging/library.rs::ListCaches`）：网页每收到一条母版库事件就重拉一次 `GET /staging`。两份 `rmsvc_core::cache::StampCache`（上限 `LIST_CACHE_CAP`＝4096 条）按文件戳缓存：`sidecars`（边车内容，按边车的戳）、`pages`（`onopen` 书的 `.content` 页数，按 `.content` 的戳）。戳＝（长度, mtime, inode）。文件没变时一次列表每本书只剩几次 `stat`。（2026-10-07 前还有第三份 `probes`，开 zip 判优化等级与是否 PDF 转来，随优化删掉。）

![母版库里一本书的一生](diagrams/sh-staging-item-life.svg)

**sidecar**（`sidecar.rs`，字段全可缺省，旧记录照读）＝`Delivered { native: Option<u64>（最近加入 xochitl 的时间戳）, koreader: Option<u64>（历史：加入 KOReader 的时间，不再有新写入）, render: Option<RenderCheck{uuid,pages,expected,status,at}>, deliver: Option<DeliverCheck{status,message,at}> }`。旧边车里可能还有已退役的字段——`optimize`（2026-10-07 前的优化结果）、`source`、`direction`、`deliver.progress`——serde 当未知字段忽略，下次改写边车时自然消失。

| 字段 | `status` 取值 |
|---|---|
| `deliver` | `pending` → `ok` / `failed`（落库没有安全中断点，不可取消） |
| `render` | `pending` → `ok` / `warn`（页数远低于期望）/ `timeout`（没等到，不算错）；大文件通道的 EPUB 另有 `onopen`（首次打开才渲染；列表发现 `.content` 页数变了就升成 `ok` 并写回边车） |

列表条目（`GET /staging` 的 `items[]`）只剩 `name`、`bytes`、`format`、`mtime`、`delivered`、`busy`；2026-10-07 前的 `optimized`/`level`/`pdfSource` 与外层的 `originals` 已删。

### 2.3 落库（母版库 → xochitl）

同一份母版可以反复加入。决策树（体积门、占位通道、拒绝的先后顺序）：

![落库决策树：一本书怎样进 xochitl](diagrams/deliver-decision.svg)

- **加入 xochitl**：`Staging::deliver()`，纯复制字节。`folder` 留空＝书库根；文件夹不存在经 `MkdirQueue` 让设备端 QML 代理建（§4）。超体积门（`nativeUploadLimitMb`，默认 90MB）→ 走"占位 + 磁盘替换"大文件通道（§6.1）；超过 1GiB、本机无书库目录或造占位失败就整本拒绝（回执末尾"没有加入"）。≤ 体积门走普通上传，`Xochitl::upload_file` 流式发送（§5）。
- 走普通上传的 EPUB 另起**渲染自检**线程（`render_check::run`，不阻塞；大文件通道直接写渲染记录）：上传前先用 `shelf_conv::stats::text_profile_file`（流式、跳过图片）按正文量算"期望页数"，再限时 10 分钟（`TIMEOUT`）轮询书库目录等 xochitl 渲染出页数，低于期望一半判 `warn`（`WARN_RATIO`＝0.5；真机标定：好书 0.86-0.99，整章渲染失败的坏书低至 0.34），写 sidecar `render` + 推 SSE。`/upload` 不回 uuid，认书靠"投书时刻后新出现的文档 + visibleName 相符者优先，否则取最新"（`render_check::pick`）。渲染出问题的书，网页提示"用 sheng-ren 重新优化后再上传、重新加入"。
- 漫画符合条件时登记首次打开设页边距（§3）。
- （历史）「加入 KOReader」2026-09-29 撤掉。留下的痕迹只有：`POST /staging/mark` 的 `target` 只剩 `native`（可省略，传 `koreader` 回 400）；旧边车的 `koreader` 字段照读。

HTTP 层是**异步**的（`spawn_deliver`：起线程 + `catch_unwind` + 解忙锁 + `bus.publish`）：立即回"已开始"，结果经 sidecar + SSE 呈现。

### 2.4 改名与下载原件（2026-09-24）

- **下载原件** `GET /staging/file?name=`：book-serve 边读边发（`Reply::sized_stream`，带 `Content-Length`），网关见到 `Content-Disposition` 也流式转发，整条链不把书读进内存。
- **改名** `POST /staging/rename {name,newName}`：只改文件名，扩展名不变（不带扩展名沿用原扩展名；带了别的扩展名只当名字的一部分）；新名已存在、或新旧任一名字正在处理中则拒绝；边车跟着改名。**不改书内书名/作者**。

（同期的"原 PDF 备份"——PDF 转 EPUB 后原 PDF 留 7 天、可恢复——2026-10-07 随 PDF 转换删除。）

### 2.5 阅读方向

翻页方向只看书里自带的 OPF `<spine page-progression-direction>`。书架不改书，方向由 sheng-ren 产物决定（它的 `xochitl` 阅读模式保留原书的方向）。`GET /reading-direction/{uuid}`（`reading_direction.rs`，reader-page-turn.qmd 用）判从右往左＝书库里那份 EPUB 的 spine 写着 `rtl`（`shelf_conv::placeholder::epub_is_rtl`），**或** uuid 在旧手动清单 `books/rtl-overrides.json` 里；这份清单只读，book-serve 不再写，要撤就手动删条目。

（历史：2026-09-25～09-29 母版库可以按书设方向、优化时写进 OPF，2026-09-30 用户定移除；当时的接口 `POST /staging/direction`、列表的 `direction`/`directionStale` 都已删，设计与真机记录见书架白皮书与 git 历史。旧边车里的 `direction` 字段照读、忽略。）

## 3｜漫画页边距（带 sheng-ren 标记的漫画一律登记）

问题：xochitl 的图片框由"栏宽（303pt − 2×页边距，缺省 56）"与"高度上限 462.1pt"先到者决定，漫画页左右留白约 20/23pt；改 `.content` 会被运行中的 xochitl 盖回（5 次仅成功 1 次），须让 xochitl 自己调 `EpubProperties.setMargins`（真机诊断记录见 sheng-ren `docs/xochitl.md` 与书架白皮书 §03bk 之后的历史）。漫画按页边距 1 排版是 sheng-ren 的事；书架只负责**认标记、登记、让 xochitl 设页边距**。**2026-10-07 起没有开关**（用户定）：原来「管理→实验室→漫画页边距」（`comicMinMargin`，默认关）删除，带标记的漫画加入后一律登记；`reading-qol.json` 里旧的 `comicMinMargin` 键无人再读（网关写回时照常原样保留未知键）。

| 环节 | 代码 | 行为 |
|---|---|---|
| 认标记 | `Staging::comic_margin_eligible` → `Option<u32>`：书里有 `META-INF/eink-reader-margins`（sheng-ren 按 `xochitl` 模式优化漫画时写，值是页边距，现为 `1`） | 只认 sheng-ren 的标记；不带标记的书（文字书、PDF、旧漫画）完全不碰。**书架 2026-10-07 前自己优化的漫画不再认**（旧标记的兼容判断随 `shelf_conv::legacy` 删掉），要用 sheng-ren 重新优化后再加入 |
| 登记 | `register_comic_margins(uuid, 页边距)` 写 `comic-margins.json` | 普通上传在渲染自检认到 uuid 时登记；大文件通道替换后立即登记 |
| 执行 | `shelf/xovi/shelf-comic-margins.qmd`（注入 DocumentView）：开书 1.5 秒后 `GET /margins/<uuid>`（404 不动；200 调 `setMargins(m)`），成功后 `POST /margins/applied` 销账 | **每本只设一次**：想要回缺省页边距 56 的，首次打开后在阅读器「文字设置」里自己调回，之后不再干预；qmd 只在 xochitl 启动时加载，装/改后要**整机重启**（2026-09-25 起不再 `systemctl restart xochitl`，停 xochitl 本身会概率性崩） |

## 4｜设备端代理队列：为什么不能直接建文件夹/删文档

外部进程不能直接写 xochitl 的 `.metadata`（运行中的 xochitl 会覆写回来），唯一合法路径是让 xochitl 自己的 QML 去做（`Library.createCollection` 建文件夹、`LibraryController.moveEntriesToTrash` 删文档、`EpubProperties.setMargins` 设页边距）。`book-serve` 因此维护三个代理队列（`mkdir.rs`/`trash.rs`/`comic_margins.rs`，三者共用 `pending_queue::PendingQueue<T>`），由设备端注入的 qmd 拉取执行：

| 队列 | qmd（`shelf/xovi/`） | 触发 | 用途 |
|---|---|---|---|
| `mkdir-pending.json`：要建的文件夹名 | `shelf-mkdir-agent.qmd`（注入 MainView） | 长轮询 `GET /mkdir/pending?wait=290`（服务端阻塞到入队或到期，上限 300 秒；遇 30 秒客户端超时自动退回 25 秒）；每次唤醒书库文件夹名只扫一遍、队列空就不扫 | 「加入 xochitl → 文件夹」填了不存在的名字；也可 `POST /mkdir/add` |
| `trash-pending.json`：要删的文档 uuid+name | `shelf-trash-agent.qmd`（注入 MainView，09-25 起） | 长轮询 `GET /trash/pending?wait=290`（同一 uuid 交出后 30 秒内不重复交），先 `Library.entryForId(uuid)` 取条目、再取 `.id`，按 id 调 `LibraryController.moveEntriesToTrash(ids)`，与当前在看哪个文件夹无关 | 入队时按 visibleName 核对 uuid；调用方是网关「设备健康→清理」和笔记线 `note-serve` 旧版本软删 |
| `comic-margins.json`：待设页边距的 uuid | `shelf-comic-margins.qmd`（注入 DocumentView） | 开书 1.5 秒后 `GET /margins/<uuid>` | §3 |
| （无队列，只读查询） | `reader-page-turn.qmd`（注入 DocumentView / DeviceSceneView / SceneViewGestures） | 开书 300 ms 后，开了「日漫翻页规则」才 `GET /reading-direction/<uuid>`（book-serve 按文件戳缓存结果，解 zip 时不持缓存锁） | xochitl 阅读器单击翻页与日漫翻页方向，见系统增强线白皮书 §03i |

`ensure_folder`（`staging/deliver.rs`）落库前最多等 20 秒（`FOLDER_WAIT_TIMEOUT`）让文件夹建出来，等不到不算错，退回 `upload_file` 的"找不到就落根"兜底。

**两条长轮询共用的交付规则**（`pending_queue::Handout`，2026-09-25 第四轮审计，当天部署；放弃与退避路径平时难触发，没在真机专门走过）：

| 规则 | 现状 | 为什么改 |
|---|---|---|
| 什么时候回应 | 有该交的项立即回；否则睡到有人入队、`wait` 到期，**或最早一个"交出后静默期"到期** | 此前只等入队或 `wait` 到期：交出后没做成的那一项要等满 290 秒才重交，而落库只等建文件夹 20 秒 |
| 静默期 | 同一项交出后，建文件夹 15 秒、回收站 30 秒内不重复交 | 代理执行需要时间，防止重复执行 |
| 重试上限 | 同一项最多交 5 次（`HANDOUT_MAX_ATTEMPTS`，约 1–2.5 分钟）；交满、静默期过了还没真实发生就放弃：移出持久队列、记一行日志、记进 `agent-failures.json` 并发 `agent-failed` 事件（网页页头横幅）；重新入队从零计 | 此前没有终止条件，拿不到条目的一项会被永远重交 |
| 代理出错退避 | 两个 qmd 请求出错（book-serve 停了、重启中）时 15 → 30 → 60 → 120 秒封顶，成功一次复位 | 此前固定 15 秒，book-serve 长时间不在时每个代理每小时白醒 240 次 |

放弃记录在 `$XDG_STATE_HOME/shelf/books/agent-failures.json`（只留最近 20 条，`GET /agent-failures`）；网页页头横幅列出哪本书没能进回收站、哪个文件夹没建出来，点「知道了」（`POST /agent-failures/clear`）清空（09-25 部署；放弃横幅本身难触发、没真机见过）。`shelf-comic-margins.qmd` 出 `EpubProperties` 后立刻排好 5 秒后销毁，出错时写一行 `CJ-COMIC-MARGIN: failed` 日志。

## 5｜内存安全设计

> **大白话**：设备只有约 2GB 内存。书架不再优化书之后，设备上最吃内存的那一类活（解压、解码、重编码整本书的图片）已经没有了；剩下的都是"边读边发 / 本机拷贝"，原则是任何一步都不把整本书读进内存。

| 风险点 | 做法 | 真机结果 |
|---|---|---|
| 落库普通上传路径 | `rmsvc_core::xochitl::upload_file` 流式上传 + `stats::text_profile_file` 流式算期望页数（跳过图片条目，连解压都不做）；2026-09-19 前整本读 + 自检整本解压 + 上传克隆叠 3 份，峰值 ~180-270MB | 80MB 测试书投递，`VmHWM` 全程约 3.4MB（09-19） |
| 大文件通道 | xochitl 上传 100MB 硬限 → 占位 + 磁盘替换（§6.1），真文件本机 `fs::copy` | 不占 HTTP 内存（真机数据见书架白皮书 §03bn） |
| 第三方大 PDF 读页数 | `shelf_conv::pdfmeta` 有界解析：只读用到的几小段，最坏峰值约 48MB（§6.1） | host 测试；真机待手测 |
| 下载原件 | 两端都流式（§2.4） | 未专门量 `VmHWM` |
| 读 EPUB 小条目 | 页边距标记只读前 16 字节（`read_capped`）；OPF、正文文字等走 sheng-ren `bookconv` 的公开接口，流式开 zip、不整本读 | — |
| 多本书同时加入 | 网关并发闸门（§7.2）：>90MB 大档同时 1 个、小档 3 个 | 见网关白皮书（闸门核心串行化未独立验证） |

**`nativeUploadLimitMb` 缺省 90MB**（94,371,840 字节，§9）：xochitl `/upload` 硬上限经真机 `curl` 二分测得＝100,000,000 字节。超过走占位通道（§6.1），走不了就整本拒绝。

**验证方法论**：读 `VmHWM`（比瞬时采样权威），真机合成接近临界值的文件、走完整 API 序列。**内存阈值别拍脑袋算，要真机实测**（09-19 设备端优化时期反复踩过）。当年设备端优化的内存设计（两阶段流式优化、图片像素上限、PDF 解析限深与解压封顶等）随优化删除，记录见书架白皮书第 E 章（历史图 [`diagrams/e-oom-guards.svg`](diagrams/e-oom-guards.svg)）。

## 6｜超限书籍的落库：大文件通道

> **大白话**：xochitl 网页上传最多约 100MB。更大的书先传一个几 KB 的"占位"书建好条目，再把真文件直接换进去；换不了就不加入。本节是这条通道**机制的权威描述**；真机数据与决策来由在书架白皮书 §03bn。

xochitl `/upload` 约 100MB 硬限（超了断连）。超过体积门时 `Staging::deliver`（决策树见 §2.3）：① **大文件占位通道**（§6.1，整本、EPUB/PDF 都行）→ 走不了（超过 1GiB、本机无书库目录、造不出占位）→ ② **整本拒绝**，回执"《书名》N MB 超过 xochitl 上传上限（90 MB），也走不了大文件通道（超过 1GB，或造不出占位文档），没有加入"。（2026-09-30 前 ① 与 ② 之间还有一步按卷拆分，已移除，见书架白皮书 §03ax。）

### 6.1 大文件"占位 + 磁盘替换"通道（2026-09-20）

![绕开 xochitl 上传体积上限](../../docs/diagrams/upload-limit-bypass.svg)

`Staging::try_deliver_direct`（→ `rmsvc_core::xochitl::Xochitl::upload_large_file`）：造带真书名和真封面的占位（`shelf_conv::placeholder`；EPUB 几十到几百 KB，显示名带卷标记时用规范名、否则沿用书自己的 `dc:title`；PDF 是手写的一页最小 PDF，单测断言 <4000 字节）→ 上传 → 等最多 20 秒、按**占位字节数**在书库认出新文档 → 母版真文件复制为 `<uuid>.<ext>.new`（0600，校验大小）→ EPUB 删渲染缓存（`.pdf`/`.epubindex`，首次打开约 25 秒重渲，146MB 实测）/PDF 改写 `.content`（`pageCount`/`originalPageCount`/`pages`/`redirectionPageMap`/`sizeInBytes`）→ 原子 rename 覆盖占位（无需重启 xochitl）→ `mark_delivered`；渲染记录 PDF 直接 `ok`、EPUB 记 `onopen`（之后读 `.content` 的 `pageCount` 显示真页数）；漫画符合 §3 时登记页边距。真机上 154MB PDF、153MB EPUB（09-20）与 156.5MB EPUB 首次打开约 74 秒渲染出 349 页（09-25）都走通了，数据见书架白皮书 §03bn。

**整段串行**（2026-09-25）：认领只凭"刚进库 + 大小等于占位"，而 PDF 占位是同一份固定字节——两本大 PDF 同时投会认领到同一个 uuid。现在 `upload_large_file` 在进程内用一把锁从"传占位"一直串到"替换完成"；回归测试用"回应后才落盘"的假 xochitl 复现，去掉锁必挂。经网关走时闸门本来就只放 1 本 >90MB 的书（§7.2），这把锁是 book-serve 进程内的第二道保证。真机没有并发投过两本大 PDF。

**适用条件**：EPUB/PDF、≤ 1GiB（`MAX_DIRECT_BYTES`）、本机有书库目录、造占位成功；否则返回 `None`，调用方整本拒绝。PDF 还要先读出真页数（写进 `.content`）：`shelf_conv::pdfmeta::page_count` 按 PDF 规范从文件尾 `startxref` 沿 `/Prev` 链登记每一节交叉引用（传统表、交叉引用流〔PDF 1.5+〕、混合式 `/XRefStm` 都认），取最新 trailer 的 `/Root` → Catalog 的 `/Pages` → `/Count`；对象在对象流（`/ObjStm`）里就只解压那一个流（FlateDecode + PNG 预测器）。**内存有硬上限**：字典对象窗口 ≤4MB、单个流压缩数据 ≤16MB / 解压后 ≤32MB、`/Prev` ≤64 节、传统表子段 ≤10 万、页数 ≤20 万，最坏峰值约 48MB；格式不认识（加密的对象流、LZW 等别的过滤器、偏移错乱）一律 `Err` → 整本拒绝，不 panic。**占位已上传后才出的错直接报错**（书库里可能留下半成品占位）。**占位必须带真书名和真封面**：xochitl 用占位 `dc:title` 当显示名、导入时生成 `cover.png`，替换后不改名不补封面（真机踩过）。
⚠ 已知限制见 §10（占位上传后崩溃会残留占位文档，回执提示手动删，不做危险的回滚删除）。

## 7｜异步任务与进度上报

> **大白话**：点按钮立即返回"已开始"，真正的活在后台跑，结果通过服务端推送实时刷到网页上，网页从不定时轮询。

`spawn_deliver` 模板：先做零耗时同步校验（格式/文件存在/忙锁），失败立即回 400 + 原因；通过才起后台线程、写初始 `pending`、返回"已开始"。落库只有整本上传与大文件通道两条路，都**没有分步进度**，网页对处理中的书画不确定态滚动条。

**刷新机制**：`rmsvc_core::events::EventBus`，`book-serve` 在状态变更点 `bus.publish("books", <kind>)`（`kind`：`staging`/`inbox`/`render`/`trash`/`mkdir`/`agent-failed`）；网关 `GET /api/events`（SSE；缺省 20s 心跳，网页带 `?ka=60` 改为 60s，少唤醒设备）汇聚各服务事件并补 `svc` 字段，网关自己的批量/闸门变化也发 `books`（`batch`/`budget`）事件；浏览器按 `area` 找 tab，非前台记脏、切过去再刷，页面隐藏不刷、可见/重连后补刷。前台的母版库按事件来源决定重取多少：网关自己的 `batch`/`budget` 事件只取两个状态接口；book-serve 的 `staging` 事件（入库、忙态开始/结束、落库结果）再加母版库列表共 3 个；其余事件、切 tab、重连、操作后才全量取 4 个（再加 `GET /api/books/status` 取文件夹列表）。三档走同一个 `coalesce`，取档位最大值。全站取数时机见网关白皮书 §5.1 的图。**前端没有任何定时轮询**。**事件流断了怎么办**（2026-09-30）：`EventSource` 进 CLOSED（网关重启后会话没了→401、并发满→503）就先查一次 `/api/session`（401 跳登录页），否则 5 秒起、翻倍、封顶 5 分钟退避重开；页面隐藏时不重试。新建 xochitl 文件夹后不轮询，等 `mkdir` 事件。

### 7.1 不能中途停

剩下的操作（整本上传、大文件通道）都没有安全的中断点，**没有取消**。网页行内只有"取消排队"（书还在网关闸门排队时，§7.2）；批量「全部中止」清掉还没开始的，已经交给 book-serve 的那本会跑完。（2026-10-07 前有 `POST /staging/cancel`：EPUB 优化每处理完一个条目检查一次取消标记、终态 `cancelled`；随优化删掉，`OpRegistry` 也简化成只有忙锁。）

### 7.2 批量队列与并发闸门（都在网关，2026-09-20）

网关是后端服务与浏览器间的唯一转发关口，故这两件事放这里（`batch.rs`、`budget.rs`），机制见 [`gateway/docs/reMarkable网关白皮书.md`](../../gateway/docs/reMarkable网关白皮书.md) §04：

- **批量队列**：`POST /api/batch {action: "deliver", names | all:true, folder}`，后台 worker **顺序逐本**；入队按"与界面按钮同一套资格条件"（EPUB/PDF）过滤，不适用计入 `skipped`。状态落盘 `state/batch.json`，网关重启 `resume` 续跑（最多等 `book-serve` 就绪 30 分钟；进行中那本放回队首重放，**同一本连续两次开始都没走完→记失败不再重放**，防崩溃循环）；`POST /api/batch/stop`＝全部中止；`GET /api/batch/status` 任何会话可看。`action` 只认 `deliver`（`koreader` 09-30、`optimize` 2026-10-07 撤掉，再传回 400）；旧 `batch.json` 里残留这两种任务时读回只剔除这几项（`parse_saved`），其余照常续跑。
  - **全部中止的时间窗**（2026-09-30 修）：当前这本已经出队、但还没交给 book-serve 时点「全部中止」，`stop` 同时置 `abort_current`，worker 在提交前看到就放弃（记失败"已全部中止"）。
- **并发闸门**：每一本「加入 xochitl」（单点与批量共用）先 `admit`——>90MB 大档同时 1 个、≤90MB 小档同时 3 个，排队最长 30 分钟、可取消（`POST /api/budget/cancel`）；同名书已在排队/处理中回 409。名额待该书在 `/staging` 不再 `busy` 才释放（上限 1 小时）；两次 `GET /staging` 至少隔 5 秒（`MIN_REQUERY`）。两档数字是当年按设备端优化定的保守经验值；现在加入本身内存很小，闸门主要起"同一时刻别往 xochitl 塞太多本、大书一本一本来"的作用。

![批量队列状态机](../../gateway/docs/diagrams/batch-queue.svg)

![并发闸门](../../docs/diagrams/budget-gate.svg)

### 7.3 进程内的锁：谁和谁不能同时做

![书架服务的并发控制：哪条路径进哪把锁](diagrams/sh-concurrency-locks.svg)

原则：**锁只包住真正会撞的那一小段**。跨服务的并发靠网关闸门（§7.2），其余都是单个进程内的锁：

| 锁 | 代码 | 谁进 | 锁多久 | 防什么 |
|---|---|---|---|---|
| 忙锁 | `ops.rs::OpRegistry`（按书名） | 加入 xochitl、改名（新旧两名）、删除 | 整个操作 | 同一本同时被加入/删除/改名 |
| 落名临界区 | `Staging::land`（`land_guard()`） | 网页上传、inbox 追平（`stage_from_path`）、改名 | 只包"`unique_path` 挑名 + rename/写入"，毫秒级；跨分区入库的拷贝在锁外做 | 两本同名书挑到同一个名、后到的覆盖先到的 |
| spool 锁 | `Spool::guard` | 只有 `process_inbox` 一轮追平 | 一轮 | 两轮追平抢同一文件 |
| 边车写锁 | `sidecar::update`（全局 static） | 所有边车读-改-写（落库结果、渲染记录、落库标记） | 一次读改写 | 交错写丢字段 |
| xochitl 上传锁 | `rmsvc_core::xochitl`（进程内 static） | 每次 `GET /documents/<文件夹>` + `POST /upload` | 一次上传 | 并发投递落进别人的文件夹 |
| 大文件通道锁 | `Xochitl::upload_large_file`（进程内 static） | 超体积门走"占位 + 磁盘替换"的投递 | 传占位 → 认领 → 换成真文件 | 两本大 PDF 认领到同一个条目（§6.1） |

这些锁的来由（09-24 网页上传攥着 spool 锁、xochitl"当前文件夹"是全局状态导致落错文件夹；09-25 删除占忙锁、大文件通道整段串行、跨分区锁外拷贝）都有 host 回归测试（`slow_upload_does_not_block_inbox_processing`、`concurrent_landing_of_same_name_never_clobbers`、`concurrent_uploads_land_in_their_own_folders`、`remove_holds_busy_lock_and_releases_it`、`concurrent_large_pdf_uploads_claim_distinct_entries`、`stage_from_path_across_filesystems_lands_atomically`），已部署，并发场景没在真机专门触发过。note-serve 是另一个进程，和 book-serve 同时投 xochitl 时上传锁管不到。（2026-10-07 前「优化」「恢复原 PDF」「抓网文的同步优化」也进忙锁和落名临界区，随功能删掉。）

## 8｜前端 UI 层（`gateway/ui/app.js`）

`renderTransfer` 渲染"传书"页：两个 subpanel——**入库**（上传卡；提示"书请先在电脑上用 sheng-ren 的 xochitl 模式优化好再传"）、**母版库**。

**母版库页**（2026-09-20 重做，取舍见书架白皮书 §03bp；2026-10-07 随书架不再优化书精简）：
- **行内只显示**书名、类型、大小、状态徽章、进度；**只有还在闸门排队时才有"取消排队"按钮**。徽章：格式 / 大小 / 落库记录（晚于母版 mtime 标"旧"，比如重新上传了 sheng-ren 新优化的版本）/ 渲染自检 / 处理中、失败或"被重启打断"（卡 `pending` 但 `busy=false`）。
- **操作统一在勾选后的底部操作栏**（按钮排成三行、每行一排绝不折行）：第二行「加入 xochitl」（带可处理数量角标，0 置灰）；第三行是**只勾一本时才有的「下载原件」「改名」**和删除（`confirmDialog()`）；运行中变进度条 + 当前书 + 失败数 + **全部中止**；跑完显示小结。
- **加入位置**：xochitl 文件夹**常驻下拉**（可"＋新建"，走 §4 mkdir 队列）；搜书名带下拉建议（多卷合一条）；筛选 **全部 / 未加入 / 已加入**（各带数量，**默认「未加入」**；"已加入"＝加入过 xochitl；浏览器记着的旧"已优化"筛选回落到「未加入」）+ 格式过滤；真分页（25/50/100）；PC 与手机同一套单列，不横向溢出。
- **状态在服务器**：批量取自 `GET /api/batch/status`、闸门取自 `GET /api/budget/status`，前端不自己记"谁在忙"。
- （2026-10-07 删除：「优化」按钮与优化等级/PDF 来源徽章、「抓网文」卡片、母版库底部的「原 PDF 备份」面板、行内「停止」、"优化全部待优化"；与「未加入」重复的"隐藏已完成"开关也删了。）

## 9｜配置与 API 一览

**`BookConfig`**（`$XDG_CONFIG_HOME/shelf/book.json`，camelCase；缺省可用，首启写出缺省文件；已退役的键如 `libraryFolder`/`annotFolder` 静默忽略）：

| JSON 键 | 缺省 | 说明 |
|---|---|---|
| `xochitlHost` | `10.11.99.1` | xochitl web 主机（`/upload`） |
| `uploadTimeoutSecs` | 300 | `/upload` 超时（超时但已送达判 `LikelyDelivered`，绝不重试） |
| `nativeUploadLimitMb` | 90 | 投原生体积门：≤ 普通上传，> 大文件占位通道（§5、§6.1）；0＝不拦 |

（2026-10-07 前还有跨进程开关 `reading-qol.json` 的 `comicMinMargin`，随实验室卡片删除，book-serve 不再读。）

**`/api/books/*`**（网关代理到 `book-serve:8790`，代理层剥掉 `books` 段；直连后端去掉 `/api/books` 前缀）：

```
GET  /status                         状态（xochitl 可达性、体积门、inbox 计数、文件夹列表；3 秒缓存）
GET  /staging                        母版库列表 {items, freeBytes}
POST /staging                        multipart 入库（逐文件）
GET  /staging/file?name=             下载原件（流式，带 Content-Disposition）
POST /staging/rename {name, newName} 改名（扩展名不变、边车跟随；冲突或忙时 400）
POST /staging/deliver {name, folder?} 异步加入 xochitl
POST /staging/mark {name, target?}   标记已加入 xochitl（target 只剩 native、可省略；koreader 回 400；现无调用方）
POST /staging/delete {name}          删除条目（忙时 400）
GET  /margins/{uuid} · POST /margins/applied {uuid}   漫画页边距待办（qmd 用；没登记的 uuid GET 回 404）
GET  /reading-direction/{uuid}       → {rtl}（reader-page-turn.qmd 用，§2.5）
GET  /events                         SSE 事件流
POST /trash/add · GET /trash/pending · GET /trash      原生回收站代理队列
POST /mkdir/add · GET /mkdir/pending[?wait=秒] · GET /mkdir   原生建文件夹代理队列（pending 支持长轮询）
GET  /agent-failures · POST /agent-failures/clear      两个代理交满次数放弃的记录（网页页头横幅，§4）
```

2026-10-07 已删（回 404）：`POST /staging/optimize`、`POST /staging/cancel`、`POST /staging/fetch-article`、`POST /staging/originals/restore|delete`。更早已删：`POST /staging/direction`（09-30）、`GET /inbox`、`POST /inbox/retry|delete`、`GET /staging/render/{uuid}`（09-22）；`/api/koreader/*` 2026-09-29 起不再代理。

**`/api/fonts/*`**（font-serve:8792）：`GET /` · `POST /` · `DELETE /{family}` · `PUT /config {emboldenCjkFallback}` · `GET /status`。
**`/api/wallpapers/*`**（wallpaper-serve:8793）：`GET /` · `POST /[?activate=1]` · `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}` · `GET /status`。

**网关自有端点**：见 [`gateway/README.md`](../../gateway/README.md)「对外接口」。笔记线 API 见 `../../notes/README.md`。

上传回执统一 `{ok, items:[{name, ok, message, item?}], …}`（`rmsvc_core::asset::receipt`），成功项的 `name` 是落地名。上传暂存位置：书进 `books/.work/`（与母版库同分区）；xochitl 字体、壁纸进 `$XDG_STATE_HOME/shelf/upload/`。都在服务启动时清掉上次中途被杀留下的 `.part`。

**请求约定**（`rmsvc-core`，2026-09-24 起）：JSON 请求体上限 1MB，超了回 400"请求体超过 1024 KB 上限"；路径参数与上传的 `filename*=` 解码时 `+` 就是 `+`，只有查询串按表单语义把 `+` 当空格；投 xochitl 时文件名里的 `"` 换成 `'`、换行换成空格。multipart 头参数按引号切分（09-25）：`filename="甲; 乙.epub"` 不再被切成 `甲`。

## 10｜已知限制（如实记录，不是遗漏）

- **书架不检查书优化过没有**：没经过 sheng-ren 的书照样能入库、加入，在 xochitl 上排版好不好全看书本身。渲染自检报 `warn` 时网页提示用 sheng-ren 重新优化。
- **书内显示名**：普通上传时 xochitl 显示书里的 `dc:title`，书架只规范文件名、不改书；大文件通道的占位对带卷标记的书用规范名。两条路显示名可能不一致，要一致就在 sheng-ren 那边定书名（2026-10-07 前「优化」会把带卷标记的书 `dc:title` 改成规范名）。
- **旧版优化的漫画**：书架 2026-10-07 前自己优化的漫画不带 sheng-ren 的页边距标记，加入后不会自动设页边距；要用 sheng-ren 重新优化再上传。
- **漫画页边距没有开关**：带标记的漫画首次打开一律设成 1；不想要的只能在阅读器里每本手动调回。母版库里设备上优化过的其它书照样能加入。
- **旧设备残留**：`staging/.pdf-originals/` 里的旧备份不会再被清理，需要时手动删。
- 网关代理对 ≤256KB 且不是下载的应答、以及没有 `Content-Length` 的应答仍整体缓冲（§1）；它们都是小 JSON，不改。
- 并发控制（§7.3）：note-serve 与 book-serve 同时投 xochitl 时上传锁管不到。
- `shelf-mkdir-agent.qmd` 长轮询 290 秒（依据：设备 Qt 6.10 的 QML XHR 不设传输超时，源码核实）；部署后确认 journal 里没有 `SHELF-MKDIR: transfer timeout`（有就说明退回了 25 秒）。
- inbox 只判"还在不在写"（修改时间静止 5 秒），不校验内容完整：scp 中途断开留下的半截文件静止后仍可能被收进母版库（§2.1）。
- 大文件占位通道：占位上传后进程崩溃会残留占位文档（回执提示手动删）。
- **翻页方向**（§2.5）：只认书里自带的 OPF 标记和旧 `rtl-overrides.json`；书里没写 `rtl` 的日漫，要么在 sheng-ren 那边处理，要么手改清单。
- **超限书**：超过 1GiB、或造不出占位文档时整本拒绝，只能自行把书分成多份再上传。加密到对象流、用 FlateDecode 以外过滤器压交叉引用的少见大 PDF 读不出页数，整本拒绝。
- 等大文件通道占位落盘是 200ms 一次的轮询（最多 20 秒）；book-serve 的领域错误一律回 400；inbox 监听出错时每 5 秒重试——都是第五轮审计记录在案、没改的点。
- **本次改动（2026-10-07）未部署、未真机验证**：要抽查的地方见书架白皮书附录 §05。

## 11｜历史：设备端优化（2026-10-07 删除）

2026-09-03 到 2026-10-06，书架在设备上自己优化书：EPUB 走两阶段流式清洗 + 优化 + 质量门（引擎先是本仓库的 `shelf/crates/bookconv`，最后版本 v16；2026-10-07 当天短暂换成 sheng-ren 的 bookconv）；有文字层的 PDF 转成 EPUB、无文字层的裁白边，原 PDF 备份 7 天；抓网文用 Readability 组 EPUB；没封面的书联网补封面（豆瓣 / Wikidata / 生成）；漫画识别、裁边、补白；母版库显示优化等级、可中途停止。2026-10-07 用户决定书的优化全部交给 sheng-ren，这些一并删除：

| 删掉的 | 原来在哪 | 现在 |
|---|---|---|
| 「优化」（EPUB / PDF） | `POST /staging/optimize`、`staging/optimizing.rs`、`shelf/crates/bookconv` | 电脑上用 sheng-ren |
| 入库 PDF 转 EPUB / 裁边 | `pdf_ingest`、`pdf-extract-cj`、pdfwrite / pdfimg / pdf_epub | 删；第三方 PDF 原样加入 |
| 原 PDF 备份 7 天 | `/staging/originals/*`、`.pdf-originals/` | 删 |
| 抓网文 | `POST /staging/fetch-article` | 网页链接用 sheng-ren 收书 |
| 联网补封面 | `cover_fetch`（豆瓣 / Wikidata / 生成封面） | sheng-ren 负责封面 |
| 旧产物兼容预处理 | `shelf_conv::legacy` | 删；旧版优化的漫画要用 sheng-ren 重新优化 |
| 中途取消 | `POST /staging/cancel`、`OpRegistry` 的取消标记 | 剩下的投递没有能中途停的步骤 |
| 优化等级 / PDF 来源 | 列表的 `optimized`/`level`/`pdfSource`、边车 `optimize` | 删；旧边车字段读时忽略 |

代码与文档原文都在 git 历史：自带 bookconv v16 那版看 master `76fd031`；换成 sheng-ren 引擎、还在设备上优化的最后一版看本分支 `ba359aa`；本文 2026-10-07 前的版本（含当时的 §2.4 PDF 入库、§3 EPUB 优化管线、§3.4 兼容预处理、§5 优化内存表）用 `git show ba359aa:shelf/docs/传书EPUB线架构.md` 查。决策经过与真机记录见书架白皮书第 B 章与 §03bv、§03bw。
