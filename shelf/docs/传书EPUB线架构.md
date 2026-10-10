# 传书模块架构文档

> **这份文档管什么**：书在设备上**怎么流动、每个服务管什么**——数据流、母版库状态、落库通道、内存设计、异步与进度、并发与锁、全部 API 与配置。它是"现在怎么运转"的参考，不是开发流水账。（文件名里的"EPUB 线"是历史叫法：当年 EPUB 走深度优化、PDF 另走一条转换线；现在两种格式都只是原样投递；文件名被多处引用，没改。只有 §11 是历史，其余都是现行参考。）
>
> **2026-10-07 起书架不再优化书**（用户定）：书的优化全部在电脑上用 **sheng-ren** 做（[github.com/bbq191/sheng-ren](https://github.com/bbq191/sheng-ren)，电脑端工具 booklib，`xochitl` 阅读模式出 EPUB），书架只负责把**已经优化好的书原样投进 xochitl**。book-serve 随之删掉了「优化」、入库 PDF 转换、原 PDF 备份、抓网文、联网补封面、旧产物兼容预处理、中途取消，见 §11；同日稍后的清理与代码审查修复（书架白皮书 §03bx、§03by）也已写进本文。
>
> **本文按 2026-10-09 的代码写**，设备上跑的也是 10-09 部署的版本（部署自检通过）。文中标"10-09"的是第六轮审计（EPUB 解压上限、回收站代理、建文件夹等待、`lowSpace`、投到已删文件夹落根、大文件通道认领改 inotify 等，书架白皮书 §03bz，**功能未在真机逐项手测**）和原地替换后找回阅读位置（§9，书架白皮书 §03ca，**真机验证过一次**）。
>
> **书籍相关文档的分工**（涉及别处的内容，本文只写一句并指过去）：
>
> | 想知道 | 看哪份 |
> |---|---|
> | 书该被优化成什么样（排版、注释、漫画） | **sheng-ren 仓库** `docs/typesetting.md`；xochitl 阅读器的实测踩坑（只认外链 CSS、书内跳转规则、目录查找等）在 `docs/xochitl.md` |
> | 书在服务间怎么流动、API 与配置 | 本文 |
> | 真机排查经过与历史决策（含已删除的设备端优化） | [`reMarkable书架白皮书.md`](reMarkable书架白皮书.md) |
>
> 本仓库原有的《EPUB 优化规范白皮书》《bookconv 优化白皮书》2026-10-07 删除（规则在 sheng-ren，xochitl 踩坑已搬到 sheng-ren `docs/xochitl.md`）；要看原文，用 `git log --oneline -- shelf/docs/EPUB优化规范白皮书.md` 找到删除它的提交，再 `git show <该提交>^:shelf/docs/EPUB优化规范白皮书.md`（另一份同理）。
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
> **核对时点**：2026-09-19 首写，每轮按源码逐字段复核；最近一次 **2026-10-09**（第六轮审计后）。文中"真机验证"沿用当时的记录；部署记录见书架白皮书「现状总览 → 部署状态」与附录 §05。


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
| `gateway` | 唯一 Web 前端：网页 UI + 反向代理 + **批量队列（`batch.rs`）** + SSE 汇聚（并发闸门 `budget.rs` 2026-10-07 删除，§7.2） | `0.0.0.0:443`（HTTPS，登录墙） |
| `book-serve` | 母版库领域服务：入库 / 改名 / 删除 / 下载原件 / 加入 xochitl，外加三个设备端代理队列（§4）；直接导入与原地替换后的阅读位置快照（§9） | `127.0.0.1:8790`（只经网关访问） |
| `shelf-conv` | **库，不是服务**，被 `book-serve` 进程内调用，**只读不改书**：`epub`（读 container.xml/OPF、书名作者语言、封面图、sheng-ren 的漫画页边距标记，写占位 EPUB 的小 zip 写入器；container.xml/OPF 解析与 href 解码用与笔记线共用的 `rmsvc-core/epubpkg`）、`pdfmeta`（第三方 PDF 页数）、`placeholder`（大文件通道的占位文档；10-07 稍后前还读 EPUB 翻页方向 `epub_is_rtl`，随日漫翻页删除，§2.5）、`naming`（文件名规范化） | 无网络面 |

（2026-10-07 稍后起**不再依赖 sheng-ren `bookconv`**：此前书架经 git 依赖借用它的公开接口读 EPUB，连带拉进图片处理、网页正文抽取、HTTP 客户端等用不上的依赖；改成 `shelf-conv::epub` 的最小实现后 `shelf/Cargo.lock` 从 309 个包降到 214 个，`cargo update -p bookconv` 也不再有。2026-10-09 这份最小实现里跟笔记线 `epubmap` 重复的部分〔container.xml → OPF、manifest/spine/Dublin Core、href 解 XML 实体与百分号、标签扫描、有上限地读条目〕合进 `rmsvc-core/epubpkg`，两边都改用它；它只依赖 regex/zip，不连带 rmsvc-core 的 HTTP/TLS。两处是从 sheng-ren 抄过来、要跟着它改的：页边距标记名 `READER_MARGINS_MARKER`＝`META-INF/eink-reader-margins`，以及书名规范化 `canonical_book_name`，§2.1、§3。渲染自检的期望页数模块 `stats` 同日删除，§2.3。）

**网关代理**（`gateway/src/proxy.rs`）：`/api/{svc}/*` 按 URL 段（`books`→`book-serve`、`fonts`→`font-serve`、`wallpapers`→`wallpaper-serve` 等，唯一的表是 `gateway/src/manage.rs` 的 `MODULES`；`koreader` 段 2026-09-29 撤掉）转发到 loopback。**请求体真流式**（大文件上传不占网关内存）；**响应**：后端给了 `Content-Length` 的 200 应答，若是下载（带 `Content-Disposition`）或体积超过 256KB（`STREAM_MIN_BYTES`），网关按定长边读边发；其余小 JSON 与没有长度的应答读完再回（2026-09-24；没有长度的流只能走 SSE 那种"读到连接关闭"的通道，拿来做下载会让浏览器等不到结束）。2026-10-07 稍后起网关对 `POST staging/deliver` 也不再特殊处理，原样转发（此前要额外读一次小 JSON 并过并发闸门，闸门同日删除，§7.2）。**超时**只有空闲超时（连接 3 秒、读 900 秒、写 120 秒），不设总时长，进程内共用一个连接池（2026-10-07 审查修复；此前 900 秒是整请求时长，慢网下传几百 MB 的书超过 15 分钟会被截断）。

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

**书名规范化**（`shelf_conv::naming::canonical_file_name`，内部用同模块的 `canonical_book_name`）：入库直接用规范名 `书名 - 02卷`（去掉下载站 `-- 作者 -- … -- hash` 尾巴与 `[完]`；无卷标记原样；幂等）；撞名加数字前缀不覆盖（`unique_path`）。`canonical_book_name` 2026-10-07 从 sheng-ren `bookconv::naming` 连同测试复制过来（此前是直接调用它）；**sheng-ren 改了规则，这里要跟着改**。**只改文件名、不改书里的 `dc:title`**——xochitl 显示的书名取书内元数据，由 sheng-ren 决定（大文件通道的占位也用书里的 `dc:title`，§6.1）。

**长书名**：中文 80 来个字的书名加 `.epub` 已接近单段文件名 255 字节上限。2026-09-30 起不再因此失败：`rmsvc_core::fs::write_atomic` 的临时名只保留目标名前 200 字节（`TMP_BASE_MAX`，按字符边界截），母版库自己的临时文件改成与书名无关的 `ScratchFile`（§2.2），边车名超长时改用短名（§2.2「边车文件名」）。

### 2.2 母版库（中间层暂存池）

目录 `$XDG_STATE_HOME/shelf/books/`（设备上 `~/.local/state/shelf/books/`，/home 分区，重启/OTA 不丢）：

| 路径 | 作用 |
|---|---|
| `staging/` | 母版库本体，条目**永久保留**，落库不自动删、不套 LRU（可反复加入） |
| `inbox/` → `.work/` → `staging/` 或 `failed/` | `spool.rs` 追平队列（同一时刻只跑一轮，spool 锁，§7.3）：`inbox/` 待处理 → `.work/` 处理中（兼上传暂存）；`failed/` 封顶 50MB，带 `<name>.reason`（10-09 起原子写）；重试＝人工拷回 `inbox/`（无 HTTP 接口） |
| `mkdir-pending.json` / `trash-pending.json` / `comic-margins.json` | 设备端代理队列（§4） |
| `staging/.pdf-originals/` | 2026-10-07 前「PDF 转 EPUB」留的原 PDF 备份（7 天）。功能已删，book-serve 不再读写；旧设备上可能还剩几份，到期也不会自动清，可手动删 |
| `done/` | 2026-09-03 早期直投流程的遗留目录，代码已不再读写；设备上仍有几份旧文件，未清理 |

`Staging`（`book-serve/src/staging/mod.rs`；动作分在 `intake.rs`/`deliver.rs`/`library.rs` 各自的 `impl Staging`）核心字段：`dir`；`xochitl: Arc<Xochitl>`；`native_limit: u64`（体积门，字节，§5）；`ops: OpRegistry`（`ops.rs`，**进程内存态**忙锁登记簿，按书名，不落盘，容忍 poison）；`caches: ListCaches`（列表用的两份按文件戳失效的缓存，见下）；`land`（落名临界区的锁，§7.3）；`comic_margins`（页边距待办队列，§3）。

**忙锁 vs sidecar 是两套独立职责**：`ops` 答"现在有没有线程在跑"，重启即清零（有意，落盘会"永久卡忙"）；sidecar 答"上次加入跑到哪/结果如何"。同一条目「加入 xochitl」「改名」「删除」互斥（同一张 `OpRegistry`；删除 09-25 起在删的那一刻也占着忙锁），冲突回 400"《…》正在处理中，请稍候"。加锁统一用 `OpRegistry::try_guard`，返回的守卫离开作用域（含 panic 展开）时自动解锁，可以移进后台线程（2026-10-07 审查修复；此前各处手写加锁解锁，改名要配对解两把）。全部锁一览见 §7.3。

**跨分区入库**（`stage_from_path` 的 rename 失败时，2026-09-25）：先在落名临界区**外**把字节拷进母版库目录下的点前缀临时文件，再回临界区挑名、同目录 rename；此前直接往最终名上拷，拷贝期间列表里就有一本半截书。**临时文件**：母版库目录里的中转文件一律是 `ScratchFile`（`book-serve/src/scratch.rs`；2026-10-07 审查修复起与直接导入的请求体临时文件共用同一份守卫和取名），名字 `.<pid>.<序号>.landing.tmp`，**与书名无关**，Drop 时自动删；母版库目录里现在只用于跨分区入库中转（2026-10-07 前还有优化产物 `optimizing`、补封面副本 `cover`、兼容预处理 `legacy` 几种，名字里带种类；只剩一种后同日稍后去掉了种类参数）。

**重启修正**：崩溃/OOM/断电可能留 `pending`，启动时 `recover_interrupted` 把落库记录改为 `failed`（"服务重启，上次加入被中断，可重新加入"）、渲染自检改为 `timeout`，并删掉母版库目录下**所有"点前缀 + `.tmp` 结尾"的普通文件**（跨分区中转、旧版留下的优化半成品、边车原子写没来得及改名的残留）；`gc_orphan_sidecars` 清孤儿边车，新书落地前也清目标名旧边车。

**边车文件名**（`sidecar::file_name_for`，2026-09-30）：书名 → 边车名只有这一个函数，读、写、改名、删书、列表缓存的文件戳、孤儿清理、启动修复全走它。`.<书名>.delivered` 不超过 255 字节时就是这个名字；超过时（书名约 244 字节以上）改成 `.<书名按字符边界截到 200 字节>.<书名 sha256 前 16 位 hex>.delivered`。短名没法从文件名反推书名，所以孤儿清理与启动修复先用 `sidecar::owners` 列出目录里每本书的边车名建反查表，再按表认主。

**列表缓存**（`staging/library.rs::ListCaches`）：网页每收到一条母版库事件就重拉一次 `GET /staging`。两份 `rmsvc_core::cache::StampCache`（上限 `LIST_CACHE_CAP`＝4096 条）按文件戳缓存：`sidecars`（边车内容，按边车的戳）、`pages`（`onopen` 书的 `.content` 页数，按 `.content` 的戳）。戳＝（长度, mtime, inode）。文件没变时一次列表每本书只剩几次 `stat`。（2026-10-07 前还有第三份 `probes`，开 zip 判优化等级与是否 PDF 转来，随优化删掉。）

![母版库里一本书的一生](diagrams/sh-staging-item-life.svg)

**sidecar**（`sidecar.rs`，字段全可缺省，旧记录照读）＝`Delivered { native: Option<u64>（最近加入 xochitl 的时间戳）, render: Option<RenderCheck{uuid,pages,status,at}>, deliver: Option<DeliverCheck{status,message,at}> }`。旧边车里可能还有已退役的字段——`koreader`（加入 KOReader 的时间，2026-10-07 审查修复从结构体删掉）、`optimize`（2026-10-07 前的优化结果）、`render.expected`（2026-10-07 前按字数估的期望页数）、`source`、`direction`、`deliver.progress`——serde 当未知字段忽略，下次改写边车时自然消失。

| 字段 | `status` 取值 |
|---|---|
| `deliver` | `pending` → `ok` / `failed`（落库没有安全中断点，不可取消） |
| `render` | `pending` → `ok` / `timeout`（没等到，不算错）；大文件通道的 EPUB 另有 `onopen`（首次打开才渲染；列表发现 `.content` 页数变了就升成 `ok` 并写回边车）。2026-10-07 前还有 `warn`（页数远低于按字数估的期望），旧边车里的 `warn` 记录网页照样当页数显示 |

列表条目（`GET /staging` 的 `items[]`）：`name`、`bytes`、`format`（`epub` / `pdf`，其余一律 `other`；2026-10-07 审查修复前 `cbz` 单列一种）、`mtime`、`delivered`、`busy`，以及 2026-10-10 起后端算好的 `title`（显示名：去 `.epub`/`.pdf`、取第一个 ` -- ` 之前）、`series`（`title` 第一个 `-` 之前，搜索建议分组）、`done`（不忙、上次加入没失败、加入过），三者逐字移植网页原来的 `stgClean`/`stgTitle`/`isBookDone`；外层 `freeBytes` 用 `rmsvc_core::fs::fs_space`（statvfs，与网关设备健康同一份，book-serve 随之去掉 `libc` 直接依赖），`lowSpace`（10-09）＝剩余严格小于 300MiB（`LOW_SPACE_BYTES`；查不到空间为 false），网页优先读它；2026-10-07 前的 `optimized`/`level`/`pdfSource` 与外层的 `originals` 已删。

### 2.3 落库（母版库 → xochitl）

同一份母版可以反复加入。决策树（体积门、占位通道、拒绝的先后顺序）：

![落库决策树：一本书怎样进 xochitl](diagrams/deliver-decision.svg)

- **加入 xochitl**：`Staging::deliver()`，纯复制字节；投进 xochitl 那一层（建文件夹、上传、大文件通道、认领、登记页边距）在 `delivery.rs` 的 `XochitlDelivery`，与直接导入共用一份（2026-10-10）。`folder` 留空＝书库根；非空＝**书库根下**这个名字的文件夹（`XochitlDelivery::ensure_folder`：根下按名字找，没有经 `MkdirQueue` 让设备端 QML 代理在根下建，§4；按 uuid 上传。2026-10-10 前按名字在全库任意层找，同名子文件夹会被选中）；上传前"设当前文件夹"失败（比如那个文件夹刚在 xochitl 里删了）时落书库根（10-09；此前会落进上一次设过的文件夹）。超体积门（`nativeUploadLimitMb`，默认 90MB）→ 走"占位 + 磁盘替换"大文件通道（§6.1）；超过 1GiB、本机无书库目录或造占位失败就整本拒绝（回执末尾"没有加入"）。≤ 体积门走普通上传，`Xochitl::upload_file_into` 流式发送（§5）。
- 走普通上传的 EPUB 另起**渲染自检**线程（`render_check::run`，不阻塞；大文件通道直接写渲染记录）：限时 10 分钟（`TIMEOUT`）监听书库目录，等 xochitl 渲染出页数，认出这本书就记下 uuid 和页数（`ok`），等不到记 `timeout`，写 sidecar `render` + 推 SSE。它留着主要是因为 `/upload` 不回 uuid，普通上传只能在这里认书，而登记漫画页边距（§3）要 uuid。认书（2026-10-10 改）：上传前拍书库快照（`delivery::Claim`），认"快照里没有、`<uuid>.epub` 与母版逐字节相同"的那份，与直接导入同一判据；母版在自检期间被删 / 改名才退回按 `dc:title` / 文件名 stem 认；**认不出就不认**（记 `timeout`、不登记页边距）。此前"都不符时取最新一本"，并发投递会认错书。2026-10-07 稍后起**不再估期望页数、不再报 `warn`**：以前上传前按正文字数估期望页数（中文每页 460 字、英文 960 字），实际页数低于期望一半判 `warn`，那主要抓的是设备上优化出错的症状；书改在电脑上用 sheng-ren 优化、有它的质量门把关后删掉，`shelf_conv::stats` 模块一并删除。
- 漫画符合条件时登记首次打开设页边距（§3）。
- 书名、封面、页边距标记都从同一个 `shelf_conv::epub::Book` 取，一本书只开一次 zip（2026-10-07 审查修复；此前普通落库开 2 次、大文件通道开 3 次）。
- （历史）「加入 KOReader」2026-09-29 撤掉。留下的痕迹 `POST /staging/mark`（`target` 只剩 `native`，没有调用方）与边车 `koreader` 字段 2026-10-07 审查修复一并删除；旧边车里的 `koreader` 当未知字段忽略。

HTTP 层是**异步**的（`spawn_deliver`：加忙锁、边车记 `pending`，排进与直接导入共用的串行作业队列 `jobs.rs`〔2026-10-10 前每次新起线程〕，作业里 `catch_unwind` + 写结果 + 解忙锁 + `bus.publish`）：立即回"已开始"，结果经 sidecar + SSE 呈现；排队期间这本书显示"处理中"。

### 2.4 改名与下载原件（2026-09-24）

- **下载原件** `GET /staging/file?name=`：book-serve 边读边发（`Reply::sized_stream`，带 `Content-Length`），网关见到 `Content-Disposition` 也流式转发，整条链不把书读进内存。
- **改名** `POST /staging/rename {name,newName}`：只改文件名，扩展名不变（不带扩展名沿用原扩展名；带了别的扩展名只当名字的一部分）；新名已存在、或新旧任一名字正在处理中则拒绝；边车跟着改名。**不改书内书名/作者**。边车里渲染自检还是 `pending` 时，改名把它直接收成 `timeout`（2026-10-07 审查修复）：自检线程按旧名找书，改名后永远找不到，以前会一直显示「渲染中」直到重启。

（同期的"原 PDF 备份"——PDF 转 EPUB 后原 PDF 留 7 天、可恢复——2026-10-07 随 PDF 转换删除。）

### 2.5 阅读方向（2026-10-07 稍后删除）

**书架不再管翻页方向**（用户定，2026-10-07）：书架只管入库，方向交给书本身。book-serve 的 `GET /reading-direction/{uuid}`（`reading_direction.rs`）、shelf-conv 的 `epub_is_rtl` / `spine_is_rtl`、`reader-page-turn.qmd` 里的日漫分支、网页「日漫翻页规则」开关（`rtlPageTurn`）都已删除（10-07 15:54 已部署）。**后果（用户已知悉）**：xochitl 自己不看 OPF 的 `page-progression-direction`，所以日漫在 xochitl 里一律从左往右翻。单击翻页（`tapPageTurn`）保留，见系统增强线白皮书 §03i。

删之前核对过设备上的旧手动清单 `books/rtl-overrides.json`：里面 15 个 uuid 都已不在 xochitl 书库里，删掉清单没有影响任何现有的书；设备上的这个文件 10-07 当天也已删除。

（历史：此前判从右往左＝书库里那份 EPUB 的 spine 写着 `rtl`，或 uuid 在只读的手动清单里；2026-09-25～09-29 母版库还可以按书设方向、优化时写进 OPF，2026-09-30 用户定移除，当时的接口 `POST /staging/direction`、列表的 `direction`/`directionStale` 都已删。旧边车里的 `direction` 字段照读、忽略。设计与真机记录见书架白皮书、系统增强线白皮书 §03i 与 git 历史。）

## 3｜漫画页边距（带 sheng-ren 标记的漫画一律登记）

问题：xochitl 的图片框由"栏宽（303pt − 2×页边距，缺省 56）"与"高度上限 462.1pt"先到者决定，漫画页左右留白约 20/23pt；改 `.content` 会被运行中的 xochitl 盖回（5 次仅成功 1 次），须让 xochitl 自己调 `EpubProperties.setMargins`（真机诊断记录见 sheng-ren `docs/xochitl.md` 与书架白皮书 §03bk 之后的历史）。漫画按页边距 1 排版是 sheng-ren 的事；书架只负责**认标记、登记、让 xochitl 设页边距**。**2026-10-07 起没有开关**（用户定）：原来「管理→实验室→漫画页边距」（`comicMinMargin`，默认关）删除，带标记的漫画加入后一律登记；`reading-qol.json` 里旧的 `comicMinMargin` 键无人再读（网关写回时照常原样保留未知键）。

| 环节 | 代码 | 行为 |
|---|---|---|
| 认标记 | `shelf_conv::epub::Book::reader_margins` → `Option<u32>`（落库在 `Staging::deliver` 里与书名同一次打开 zip 时读，大文件通道在 `XochitlDelivery::upload_large` 里读；标记名常量 `READER_MARGINS_MARKER` 必须与 sheng-ren 的同名常量一致，两边各写一份）：书里有 `META-INF/eink-reader-margins`（sheng-ren 按 `xochitl` 模式优化漫画时写，值是页边距，现为 `1`） | 只认 sheng-ren 的标记；不带标记的书（文字书、PDF、旧漫画）完全不碰。**书架 2026-10-07 前自己优化的漫画不再认**（旧标记的兼容判断随 `shelf_conv::legacy` 删掉），要用 sheng-ren 重新优化后再加入 |
| 登记 | `XochitlDelivery::register_comic_margins(uuid, 页边距)` 写 `comic-margins.json` | 普通上传在渲染自检按字节认到 uuid 时登记（认不出就不登记，2026-10-10 起不再"取最新一本"）；直接导入认领后立即登记；大文件通道替换后立即登记 |
| 执行 | `shelf/xovi/shelf-comic-margins.qmd`（注入 DocumentView）：开书 1.5 秒后 `GET /margins/<uuid>`（404 不动；200 调 `setMargins(m)`），成功后 `POST /margins/applied` 销账 | **每本只设一次**：想要回缺省页边距 56 的，首次打开后在阅读器「文字设置」里自己调回，之后不再干预；qmd 只在 xochitl 启动时加载，装/改后要**整机重启**（2026-09-25 起不再 `systemctl restart xochitl`，停 xochitl 本身会概率性崩） |

## 4｜设备端代理队列：为什么不能直接建文件夹/删文档

外部进程不能直接写 xochitl 的 `.metadata`（运行中的 xochitl 会覆写回来），唯一合法路径是让 xochitl 自己的 QML 去做（`Library.createCollection` 建文件夹、`LibraryController.moveEntriesToTrash` 删文档、`EpubProperties.setMargins` 设页边距）。`book-serve` 因此维护三个代理队列（`mkdir.rs`/`trash.rs`/`comic_margins.rs`，三者共用 `pending_queue::PendingQueue<T>`；前两个长轮询的再共用 `AgentQueue<T>`——剔除已完成、交出、交满放弃一整套，2026-10-10），由设备端注入的 qmd 拉取执行：

| 队列 | qmd（`shelf/xovi/`） | 触发 | 用途 |
|---|---|---|---|
| `mkdir-pending.json`：要建的文件夹（名字 + 上级文件夹 uuid） | `shelf-mkdir-agent.qmd`（注入 MainView） | 长轮询 `GET /mkdir/pending?wait=290`（服务端阻塞到入队或到期，上限 300 秒；遇 30 秒客户端超时自动退回 25 秒）；每次唤醒书库文件夹只扫一遍、队列空就不扫；按 `items` 调 `Library.createCollection(parent, name)` | 「加入 xochitl → 文件夹」填了不存在的名字（建在根）；直接导入逐级建多级文件夹（建在上一级里）；也可 `POST /mkdir/add`（建在根） |
| `trash-pending.json`：要删的文档 uuid+name | `shelf-trash-agent.qmd`（注入 MainView，09-25 起） | 长轮询 `GET /trash/pending?wait=290`（同一 uuid 交出后 30 秒内不重复交；遇 30 秒客户端超时自动退回 25 秒，2026-10-10 补，与建夹代理同构），先 `Library.entryForId(uuid)` 取条目、再取 `.id`，按 id 调 `LibraryController.moveEntriesToTrash(ids)`，与当前在看哪个文件夹无关 | 入队时按 visibleName 核对 uuid，已进回收站或已删的拒绝；拉取时只把"`.metadata` 真没了、或已进回收站"的项移出队列，读不了 `.metadata`（xochitl 正在改写）这次先留着（10-09；此前读失败当成已完成，静默丢待办）；调用方是网关「设备健康→清理」和笔记线 `note-serve` 旧版本软删 |
| `comic-margins.json`：待设页边距的 uuid | `shelf-comic-margins.qmd`（注入 DocumentView） | 开书 1.5 秒后 `GET /margins/<uuid>` | §3 |

另有第四个 qmd `shelf-keep-progress.qmd`（注入 DocumentView，2026-10-09）不走队列：只在 sheng-ren 原地替换过的书重开时问 `GET /progress/{uuid}`，让 xochitl 自己 `goToPage` 跳回原来读到的地方，见 §9「直接导入」与书架白皮书 §03ca（含流程图）。

（2026-10-07 稍后前表里还有一行：`reader-page-turn.qmd` 开了「日漫翻页规则」时开书问 `GET /reading-direction/<uuid>`；随日漫翻页删除，§2.5。单击翻页不经 book-serve。）

**建文件夹队列按（上级, 名字）记**（2026-10-07，直接导入要按层建多级文件夹）：队列项是 `{name, parent, at}`，`parent` 是上级文件夹 uuid、空串＝书库根；此前的队列文件没有 `parent`，按根读。入队去重和"已经建出来就剔除"都按（parent, name）判——只有那个上级正下方有同名活文件夹（`CollectionType`、没删、不在回收站）才算存在，所以「漫画/卷01」和「小说/卷01」各是各的。`GET /mkdir/pending` 回 `{"items": [{"name", "parent"}, ...]}`，代理按它调 `Library.createCollection(item.parent, item.name)`（2026-10-10 删掉给 10-07 前旧代理的 `names` 和代理里的退回逻辑：两者总由 `install.sh` 一起装）。交出记录的键就是（parent, name），不再拼成 `"<父>/<名>"` 再拆。`POST /mkdir/add {name}`（网页用）不变：建在根，名字里的 `/` 当普通字符。**`createCollection` 传非空 parent 还没真机验证**（10-07 约 16:32 已部署，部署自检通过，但还没实际建过子文件夹）：依据是反编译里对话框传 `currentFolderId`，以及 10-07 在 Move 上用界面手工建的三层文件夹（Folder → Folder 2 → Folder 3）的 `.metadata`：parent 依次是 ""、上一级的 uuid（只读看到的）。

`XochitlDelivery::ensure_folder_path`（`delivery.rs`；网页落库的单段文件夹走它的单段情形 `ensure_folder`）每一级最多等 20 秒（`FOLDER_WAIT_TIMEOUT`）让文件夹建出来，等不到不算错，书落在已经有的那一级（单段就是书库根）。入队时发现文件夹刚好已经出现（`add_in` 回 0）就不再等（2026-10-07 审查修复）；等待改用 `fswatch::wait_for`：**先挂目录监听再查一次**，入队到挂上监听之间代理已经建好的也不会白等满 20 秒（10-09）。

**两条长轮询共用的交付规则**（`pending_queue::Handout<K>`，键是回收站的 uuid / 建文件夹的（上级, 名字），2026-09-25 第四轮审计，当天部署；放弃与退避路径平时难触发，没在真机专门走过）：

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
| 落库普通上传路径 | `rmsvc_core::xochitl::upload_file` 流式上传（10-07 前还有 `stats::text_profile_file` 流式算期望页数，已删）；2026-09-19 前整本读 + 自检整本解压 + 上传克隆叠 3 份，峰值 ~180-270MB | 80MB 测试书投递，`VmHWM` 全程约 3.4MB（09-19） |
| 大文件通道 | xochitl 上传 100MB 硬限 → 占位 + 磁盘替换（§6.1），真文件本机 `fs::copy` | 不占 HTTP 内存（真机数据见书架白皮书 §03bn） |
| 第三方大 PDF 读页数 | `shelf_conv::pdfmeta` 有界解析：只读用到的几小段，最坏峰值约 48MB（§6.1） | host 测试；真机待手测 |
| 下载原件 | 两端都流式（§2.4） | 未专门量 `VmHWM` |
| 读 EPUB 小条目 | `shelf_conv::epub` 只读 container.xml、OPF、封面要用的少数几个条目，从不解压整本；单条目解压上限按用途分开：container.xml / OPF / 正文 16MB（`MAX_TEXT_BYTES`）、封面图 64MB（`MAX_IMAGE_BYTES`），超了当成读不到、不 OOM（10-09；此前统一 256MB，超过 book-serve 的 `MemoryMax=192M`）；页边距标记只读前 16 字节（`read_capped`） | host 单测 |
| 多本书同时加入 | 网页的「加入 xochitl」只走网关批量队列，一本加完才取下一本（§7.2）；2026-10-07 前另有并发闸门（大档 1 个、小档 3 个），同日删除 | — |

**`nativeUploadLimitMb` 缺省 90MB**（94,371,840 字节，§9）：xochitl `/upload` 硬上限经真机 `curl` 二分测得＝100,000,000 字节。超过走占位通道（§6.1），走不了就整本拒绝。

**验证方法论**：读 `VmHWM`（比瞬时采样权威），真机合成接近临界值的文件、走完整 API 序列。**内存阈值别拍脑袋算，要真机实测**（09-19 设备端优化时期反复踩过）。当年设备端优化的内存设计（两阶段流式优化、图片像素上限、PDF 解析限深与解压封顶等）随优化删除，记录见书架白皮书第 E 章（历史图 [`diagrams/e-oom-guards.svg`](diagrams/e-oom-guards.svg)）。

## 6｜超限书籍的落库：大文件通道

> **大白话**：xochitl 网页上传最多约 100MB。更大的书先传一个几 KB 的"占位"书建好条目，再把真文件直接换进去；换不了就不加入。本节是这条通道**机制的权威描述**；真机数据与决策来由在书架白皮书 §03bn。

xochitl `/upload` 约 100MB 硬限（超了断连）。超过体积门时 `Staging::deliver`（决策树见 §2.3）：① **大文件占位通道**（§6.1，整本、EPUB/PDF 都行）→ 走不了（超过 1GiB、本机无书库目录、造不出占位）→ ② **整本拒绝**，回执"《书名》N MB 超过 xochitl 上传上限（90 MB），也走不了大文件通道（超过 1GB，或造不出占位文档），没有加入"。（2026-09-30 前 ① 与 ② 之间还有一步按卷拆分，已移除，见书架白皮书 §03ax。）

### 6.1 大文件"占位 + 磁盘替换"通道（2026-09-20）

![绕开 xochitl 上传体积上限](../../docs/diagrams/upload-limit-bypass.svg)

`Staging::try_deliver_direct`（→ `rmsvc_core::xochitl::Xochitl::upload_large_file`）：造带真书名和真封面的占位（`shelf_conv::placeholder`；EPUB 几十到几百 KB，显示名一律用书自己的 `dc:title`（2026-10-07 稍后起；此前带卷标记时改用规范名，sheng-ren 已经写好规范书名，这层判断删掉）；PDF 是手写的一页最小 PDF，单测断言 <4000 字节）→ 上传 → 等最多 20 秒（书库目录 inotify，10-09 起；此前 200ms 轮询）、按**占位字节数**在书库认出新文档 → 母版真文件复制为 `<uuid>.<ext>.new`（0600，校验大小）→ EPUB 删渲染缓存（`.pdf`/`.epubindex`，首次打开约 25 秒重渲，146MB 实测）/PDF 改写 `.content`（`pageCount`/`originalPageCount`/`pages`/`redirectionPageMap`/`sizeInBytes`）→ 原子 rename 覆盖占位（无需重启 xochitl）→ `mark_delivered`；渲染记录 PDF 直接 `ok`、EPUB 记 `onopen`（之后读 `.content` 的 `pageCount` 显示真页数）；漫画符合 §3 时登记页边距。真机上 154MB PDF、153MB EPUB（09-20）与 156.5MB EPUB 首次打开约 74 秒渲染出 349 页（09-25）都走通了，数据见书架白皮书 §03bn。

**整段串行**（2026-09-25）：认领只凭"刚进库 + 大小等于占位"，而 PDF 占位是同一份固定字节——两本大 PDF 同时投会认领到同一个 uuid。现在 `upload_large_file` 在进程内用一把锁从"传占位"一直串到"替换完成"；回归测试用"回应后才落盘"的假 xochitl 复现，去掉锁必挂。经网关批量队列走时本来就一本一本来（§7.2），这把锁是 book-serve 进程内的保证，管得到绕过网关直连的调用（如 §9 的直接导入）。真机没有并发投过两本大 PDF。

**适用条件**：EPUB/PDF、≤ 1GiB（`MAX_DIRECT_BYTES`）、本机有书库目录、造占位成功；否则返回 `None`，调用方整本拒绝。PDF 还要先读出真页数（写进 `.content`）：`shelf_conv::pdfmeta::page_count` 按 PDF 规范从文件尾 `startxref` 沿 `/Prev` 链登记每一节交叉引用（传统表、交叉引用流〔PDF 1.5+〕、混合式 `/XRefStm` 都认），取最新 trailer 的 `/Root` → Catalog 的 `/Pages` → `/Count`；对象在对象流（`/ObjStm`）里就只解压那一个流（FlateDecode + PNG 预测器）。**内存有硬上限**：字典对象窗口 ≤4MB、单个流压缩数据 ≤16MB / 解压后 ≤32MB、`/Prev` ≤64 节、传统表子段 ≤10 万、页数 ≤20 万，最坏峰值约 48MB；格式不认识（加密的对象流、LZW 等别的过滤器、偏移错乱）一律 `Err` → 整本拒绝，不 panic。**占位已上传后才出的错直接报错**（书库里可能留下半成品占位）。**占位必须带真书名和真封面**：xochitl 用占位 `dc:title` 当显示名、导入时生成 `cover.png`，替换后不改名不补封面（真机踩过）。
⚠ 已知限制见 §10（占位上传后崩溃会残留占位文档，回执提示手动删，不做危险的回滚删除）。

## 7｜异步任务与进度上报

> **大白话**：点按钮立即返回"已开始"，真正的活在后台跑，结果通过服务端推送实时刷到网页上，网页从不定时轮询。

`spawn_deliver` 模板：先做零耗时同步校验（格式/文件存在/忙锁），失败立即回 400 + 原因；通过才写初始 `pending`、排进串行作业队列（`jobs.rs`，与直接导入共用；2026-10-10 前是每次新起线程）、返回"已开始"。落库只有整本上传与大文件通道两条路，都**没有分步进度**，网页对处理中的书画不确定态滚动条。

**刷新机制**：`rmsvc_core::events::EventBus`，`book-serve` 在状态变更点 `bus.publish("books", <kind>)`（`kind`：`staging`/`inbox`/`render`/`trash`/`mkdir`/`agent-failed`）；网关 `GET /api/events`（SSE；缺省 20s 心跳，网页带 `?ka=60` 改为 60s，少唤醒设备）汇聚各服务事件并补 `svc` 字段，网关自己的批量队列变化也发 `books`（`batch`）事件（2026-10-07 前闸门还发 `budget`）；浏览器按 `area` 找 tab，非前台记脏、切过去再刷，页面隐藏不刷、可见/重连后补刷。前台的母版库按事件来源决定重取多少：网关自己的 `batch` 事件只取批量状态 1 个；book-serve 的 `staging` 事件（入库、忙态开始/结束、落库结果）与 `render` 事件（渲染自检结果；2026-10-07 审查修复前走全量）再加母版库列表共 2 个；其余事件、切 tab、重连、操作后才全量取 3 个（再加 `GET /api/books/status` 取文件夹列表）。（2026-10-07 稍后前是 2/3/4 个，多一个闸门状态接口。）三档走同一个 `coalesce`，取档位最大值。全站取数时机见网关白皮书 §5.1 的图。**前端没有任何定时轮询**。**事件流断了怎么办**（2026-09-30）：`EventSource` 进 CLOSED（网关重启后会话没了→401、并发满→503）就先查一次 `/api/session`（401 跳登录页），否则 5 秒起、翻倍、封顶 5 分钟退避重开；页面隐藏时不重试。新建 xochitl 文件夹后不轮询，等 `mkdir` 事件。

### 7.1 不能中途停

剩下的操作（整本上传、大文件通道）都没有安全的中断点，**没有取消**。批量「全部中止」清掉还没开始的，已经交给 book-serve 的那本会跑完；网页行内没有任何按钮（2026-10-07 稍后删了"取消排队"，它只对闸门里排队的书有效，闸门同日删除）。（2026-10-07 前有 `POST /staging/cancel`：EPUB 优化每处理完一个条目检查一次取消标记、终态 `cancelled`；随优化删掉，`OpRegistry` 也简化成只有忙锁。）

### 7.2 批量队列（在网关，2026-09-20；并发闸门 2026-10-07 删除）

网关是后端服务与浏览器间的唯一转发关口，故批量队列放这里（`batch.rs`），机制见 [`gateway/docs/reMarkable网关白皮书.md`](../../gateway/docs/reMarkable网关白皮书.md) §04：

- **批量队列**：`POST /api/batch {action: "deliver", names | all:true, folder}`，后台 worker **顺序逐本**：一本交给 book-serve 后，等它在 `/staging` 不再 `busy`（`poll_until_settled`：事件驱动，靠 `books_wake` 唤醒、至多 30 秒兜底查一次；两次 `GET /staging` 至少隔 5 秒〔`MIN_REQUERY`〕；连续失败 6 次才放弃等待；上限 1 小时）才取下一本。**成败**从等待结束时最后取到的那份列表里读这本的 `delivered.deliver.status`：只有 `ok` 算成功；`failed` 带 book-serve 的原因记失败；等满 1 小时、连续查不到、条目途中消失、仍是 `pending` 也一律记失败，提示"请到 xochitl 书库里核对"（2026-10-07 审查修复；此前只认 `failed`，这几种都记成成功）。网页的「加入 xochitl」只走这条队列，所以同一时刻最多一本书在投。入队按"与界面按钮同一套资格条件"（EPUB/PDF）过滤，点名了但不适用的计入 `skipped`。状态落盘 `state/batch.json`，网关重启 `resume` 续跑（最多等 `book-serve` 就绪 30 分钟，等的是注册表变化、30 秒兜底，这期间 `GET /api/batch/status` 的 `waitingService=true`、网页显示"等 book-serve 就绪"；进行中那本**一律放回队首重放**；文件损坏或不是当前格式＝当作没有未完成的队列）；`POST /api/batch/stop`＝全部中止；`GET /api/batch/status` 任何会话可看。`action` 只认 `deliver`（`koreader` 09-30、`optimize` 2026-10-07 撤掉，再传回 400）。
  - **全部中止的时间窗**（2026-09-30 修）：当前这本已经出队、但还没交给 book-serve 时点「全部中止」，`stop` 同时置 `abort_current`，worker 在提交前看到就放弃（记失败"已全部中止"）。
  - **2026-10-07 稍后删掉的旧规则**：同一本连续两次开始都没走完就记失败、不再重放（`attempts`/`MAX_ATTEMPTS`，当年防"优化某本书把 book-serve 搞崩 → 重启重放 → 再崩"的循环；网关不读书的内容，现在加入也不在设备上解书，崩溃元凶不会在这边）；读回旧 `batch.json` 时剔除 optimize/koreader 任务的迁移（`parse_saved`，现在按当前格式严格读）；已不会出现的 `cancelled` 状态分支。
- **并发闸门（2026-10-07 稍后删除）**：此前每一本「加入 xochitl」先过网关 `budget.rs` 的 `admit`——>90MB 大档同时 1 个、≤90MB 小档同时 3 个，排队最长 30 分钟、可取消（`/api/budget/status`、`/api/budget/cancel`），网页行内有"取消排队"。删掉的理由：网页的「加入 xochitl」只走批量队列，队列本来就一本处理完才取下一本；加入是流式上传，book-serve 只多占几 MB；闸门"内存峰值≈文件体积"的前提来自设备上优化大书，书架不再优化后闸门已经拦不到任何东西。"等这本书处理完"的轮询从 `proxy.rs` 挪进了 `batch.rs`（上面的 `poll_until_settled`）。

![批量队列状态机](../../gateway/docs/diagrams/batch-queue.svg)

### 7.3 进程内的锁：谁和谁不能同时做

![书架服务的并发控制：哪条路径进哪把锁](diagrams/sh-concurrency-locks.svg)

原则：**锁只包住真正会撞的那一小段**。网页发起的加入靠网关批量队列一本一本来（§7.2；2026-10-07 前还有网关闸门），其余都是单个进程内的锁：

| 锁 | 代码 | 谁进 | 锁多久 | 防什么 |
|---|---|---|---|---|
| 忙锁 | `ops.rs::OpRegistry`（按书名） | 加入 xochitl、改名（新旧两名）、删除 | 整个操作 | 同一本同时被加入/删除/改名 |
| 落名临界区 | `Staging::land`（`land_guard()`） | 网页上传、inbox 追平（`stage_from_path`）、改名 | 只包"`unique_path` 挑名 + rename/写入"，毫秒级；跨分区入库的拷贝在锁外做 | 两本同名书挑到同一个名、后到的覆盖先到的 |
| spool 锁 | `Spool::guard` | 只有 `process_inbox` 一轮追平 | 一轮 | 两轮追平抢同一文件 |
| 边车写锁 | `sidecar::update`（全局 static） | 所有边车读-改-写（落库结果、渲染记录、落库标记） | 一次读改写 | 交错写丢字段 |
| 投递作业队列 | `jobs.rs::Jobs`（一个工作线程，2026-10-10） | 网页落库（`spawn_deliver`）、直接导入（`POST /import`） | 一件作业（建文件夹、上传、认领；渲染自检另起线程不占） | 落库与直接导入各自一套任务模型、同时往 xochitl 传 |
| xochitl 上传锁 | `rmsvc_core::xochitl`（进程内 static） | 每次 `GET /documents/<文件夹>` + `POST /upload` | 一次上传 | 并发投递落进别人的文件夹 |
| 认领锁 | `XochitlDelivery::upload_and_claim`（进程内 static） | 直接导入的普通上传 + 等认领 | 上传 → 认出 uuid | 两次导入同一份字节时后一次认走前一次的文档 |
| 大文件通道锁 | `Xochitl::upload_large_file`（进程内 static） | 超体积门走"占位 + 磁盘替换"的投递 | 传占位 → 认领 → 换成真文件 | 两本大 PDF 认领到同一个条目（§6.1） |

这些锁的来由（09-24 网页上传攥着 spool 锁、xochitl"当前文件夹"是全局状态导致落错文件夹；09-25 删除占忙锁、大文件通道整段串行、跨分区锁外拷贝）都有 host 回归测试（`slow_upload_does_not_block_inbox_processing`、`concurrent_landing_of_same_name_never_clobbers`、`concurrent_uploads_land_in_their_own_folders`、`remove_holds_busy_lock_and_releases_it`、`concurrent_large_pdf_uploads_claim_distinct_entries`、`stage_from_path_across_filesystems_lands_atomically`），已部署，并发场景没在真机专门触发过。note-serve 是另一个进程，和 book-serve 同时投 xochitl 时上传锁管不到。（2026-10-07 前「优化」「恢复原 PDF」「抓网文的同步优化」也进忙锁和落名临界区，随功能删掉。）

## 8｜前端 UI 层（`gateway/ui/app.js`）

`renderTransfer` 渲染"传书"页：两个 subpanel——**入库**（上传卡；提示"书请先在电脑上用 sheng-ren 的 xochitl 模式优化好再传"）、**母版库**。

**母版库页**（2026-09-20 重做，取舍见书架白皮书 §03bp；2026-10-07 随书架不再优化书精简）：
- **行内只显示**书名、类型、大小、状态徽章、进度，**没有任何按钮**（2026-10-07 稍后删了只对闸门排队有效的"取消排队"）。徽章：格式 / 大小 / 落库记录（晚于母版 mtime 标"旧"，比如重新上传了 sheng-ren 新优化的版本）/ 渲染自检（只显示页数，旧的 `warn` 记录也按页数显示；10-07 前有"⚠ 只渲染 N 页 / 预期≈M"）/ 处理中、失败或"被重启打断"（卡 `pending` 但 `busy=false`）。进度条只有不确定态滚动条（`renderBusy`）。
- **操作统一在勾选后的底部操作栏**（按钮排成三行、每行一排绝不折行）：第二行「加入 xochitl」（带可处理数量角标，0 置灰）；第三行是**只勾一本时才有的「下载原件」「改名」**和删除（`confirmDialog()`）；批量运行时显示进度条 + 当前书 + 失败数 + **全部中止**，同时照样能对勾选的书操作（2026-10-07 审查修复）；跑完显示小结，点「收起」记在本机浏览器；选中的书全在处理中时删除置灰并说明原因。
- **加入位置**：xochitl 文件夹**常驻下拉**（可"＋新建"，走 §4 mkdir 队列）；搜书名带下拉建议（多卷合一条）；筛选 **全部 / 未加入 / 已加入**（各带数量，**默认「未加入」**；"已加入"＝加入过 xochitl，"未加入"是它的补集——正在处理、加入失败的书也算未加入，三个数加得拢〔2026-10-07 审查修复〕；选了"＋新建"没点创建就加入会先提示；浏览器记着的旧"已优化"筛选回落到「未加入」）+ 格式过滤（只有 EPUB / PDF，10-07 稍后删了永远为空的「其它」）；真分页（25/50/100）；PC 与手机同一套单列，不横向溢出。
- **状态在服务器**：批量取自 `GET /api/batch/status`，前端不自己记"谁在忙"（10-07 稍后前还取闸门 `GET /api/budget/status`，页头显示"⏳ 排队 N 本 · 处理中 M 本"，随闸门删除）。
- 母版库页头说明：带 sheng-ren 标记的漫画加入后首次打开会自动把页边距设到最小（10-07 稍后修正，此前还写着"要开「实验室→漫画页边距」"，那个开关同日早些时候已删）。
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
GET  /status                         {ok, xochitlFolders}（xochitl 书库文件夹列表；3 秒缓存，加入 / 直接导入后失效）
GET  /staging                        母版库列表 {items, freeBytes, lowSpace}
POST /staging                        multipart 入库（逐文件）
GET  /staging/file?name=             下载原件（流式，带 Content-Disposition）
POST /staging/rename {name, newName} 改名（扩展名不变、边车跟随；冲突或忙时 400）
POST /staging/deliver {name, folder?} 异步加入 xochitl
POST /staging/delete {name}          删除条目（忙时 400）
GET  /margins/{uuid} · POST /margins/applied {uuid}   漫画页边距待办（qmd 用；没登记的 uuid GET 回 404）
GET  /progress/{uuid}[?pages=N]      原地替换后的阅读位置（shelf-keep-progress.qmd 用，2026-10-09）：没有快照 → 404；新排版（新 .epubindex）还没出来 → 202 {pending:true}；出来了 → 200 {page}（文档页序，0 起）。pages＝阅读器当前总页数，可省（书架白皮书 §03ca）
POST /progress/applied {uuid, reason?}  删阅读位置快照（跳完 reason=applied；qmd 等约 60 秒新排版还没出来就放弃，reason=timeout）→ {ok, removed}
GET  /events                         SSE 事件流
POST /trash/add · GET /trash/pending · GET /trash      原生回收站代理队列（GET /trash 只供调试：ssh 上 curl 看队列）
POST /mkdir/add {name}（建在根）· GET /mkdir/pending[?wait=秒] → {items:[{name, parent}]}（10-10 删 names）· GET /mkdir   原生建文件夹代理队列（pending 支持长轮询；GET /mkdir 只供调试）
GET  /agent-failures · POST /agent-failures/clear      两个代理交满次数放弃的记录（网页页头横幅，§4）
POST /import?name=&folder=           直接导入 xochitl、不进母版库（体＝EPUB 原始字节；folder 是 / 分隔的多级路径）→ 202 {job}（异步，结果查下一行）
POST /import?uuid=&name=             原地替换已有文档内容、uuid 不变 → 202 {job}；不在/已删/回收站/非 EPUB → 404；同一 uuid 正在替换 → 409（2026-10-09 起，以前是 400；客户端等下一行的 replacing 变假再交）（都在回 202 之前判）
GET  /import/jobs/{id}               → {job, state: running|done|failed, stage, uuid?, name?, folder?, message?}（done 带 uuid/name/folder，folder＝实际落进的完整路径；failed 带 message）；不存在 → 404
GET  /import/{uuid}                  → {uuid, name, folder, deleted, replacing}（folder＝从根起的完整路径；replacing＝正在原地替换，2026-10-09）；不存在 → 404
POST /import/states {uuids: [...]}   → {docs: {<uuid>: {name, folder, deleted, replacing}}}（一次查一批，口径同上，不存在的不出现；最多 10000 个，2026-10-09）
```

**直接导入**（`import.rs`，2026-10-07）：给电脑上的 sheng-ren（`booklib sync`）用——经 SSH 端口转发（`ssh -L` 到设备 `127.0.0.1:8790`）直连 book-serve，不经网关、不带 `/api/books` 前缀，书**不进母版库**。
- **异步**（2026-10-07 17:27 提交，部署记录里没有这一项；同网页「加入 xochitl」的 `spawn_deliver`）：`POST /import` 只做收体和快速校验（下面两条里"回 202 之前"的部分），校验过了就把剩下的活交给后台作业队列（`jobs.rs`，2026-10-10 前叫 `import_jobs.rs`；网页落库也排这个队列），立即回 `202 {"job": "<任务 id>"}`；客户端轮询 `GET /import/jobs/{id}`：`state` 为 `running`（`stage` 是给人看的当前阶段：「排队」「开始」「建文件夹」「上传给 xochitl」「上传给 xochitl（大文件通道）」「等 xochitl 排版」「登记页边距」「替换文件」）、`done`（带 `uuid`、`name`〔visibleName〕、`folder`〔实际落进的完整路径〕，`stage`「完成」）或 `failed`（带 `message`，`stage`「失败」）。任务**串行**：只有一个工作线程，按提交顺序一次做一个，排队中的是 `running` + 「排队」。任务记录**只在内存里**：做完的至少留 1 小时（`JOB_KEEP`，之后在下一次提交 / 查询时清掉）；book-serve 重启就没了，查询回 404（任务 id 带进程启动时刻，重启后不会撞上旧 id）。任务里 panic 由 `catch_unwind` 转成失败，工作线程照常做下一个。成功时推 `books/import` 事件、让 `/status` 缓存失效；失败记进日志（`journalctl -u book-serve`）。原来同步回 `{uuid, name, folder}` 的行为去掉了。**来由**：此前同步——HTTP 一直挂到加入完成，而 xochitl 收 `/upload` 时当场排版，2026-10-07 真机《阿加莎全集》（30MB）排了 8 分多钟；客户端 10 分钟超时、服务端认领只等 120 秒，结果书加进去了却回了失败，电脑上没有记录、下次同步重复传。现在认领超时（`/upload` 超时判"很可能已送达"时）放宽到 30 分钟，反正在后台线程里等。**未真机验证**。
- 新导入：请求体（`Content-Type: application/epub+zip`，带 `Content-Length`）按块写到 `$XDG_STATE_HOME/shelf/books/import-tmp/` 的临时文件（不整本进内存；任务做完/出错都删，启动时清上次的残留），回 202 之前校验扩展名 `.epub`、非空、≤ 1GB（`MAX_DIRECT_BYTES`）、收到的字节数等于 `Content-Length`、开头是 zip 头 `PK\3\4`（不对 → 400）。后台任务里：目标文件夹逐级确保（见下一条）；**幂等**：目标文件夹里已经有一份内容与这次逐字节相同的活文档（先比大小）就直接认它、不再上传（上一次其实成功了、客户端没拿到结果再来一次，不会重复加入）；≤ 体积门走普通 `/upload`，再按"上传前没有、上传后新出现、`<uuid>.epub` 与上传字节逐字节相同"认出 uuid（进程内串行，回执后等 20 秒；`/upload` 超时判"很可能已送达"时等 30 分钟〔10-07 前 120 秒〕；等的是书库目录的 inotify 事件〔防抖 500ms〕，2026-10-07 审查修复前是每 200ms 扫一遍书库），> 体积门走大文件通道（§5，它自己返回 uuid）。漫画带 sheng-ren 页边距标记的照常登记（§3）。
- **多级文件夹**（2026-10-07 用户要求）：`folder` 是原件在用户书目录里的相对子目录，`/` 分隔（`漫画/死亡筆記(愛藏版)`），按 `/` 拆、去掉各段首尾空白和空段（单段就是原来的行为，空＝书库根）。`XochitlDelivery::ensure_folder_path`（2026-10-10 前挂在 `Staging` 上）从根往下逐级：在上一级正下方按名字找（`rmsvc_core::xochitl::find_child_folder`）→ 没有就 `MkdirQueue::add_in(上一级 uuid, 名字)` 入队 → 等它真建出来（书库目录 inotify，防抖 3 秒，每级最多 20 秒），拿到这一级的 uuid 再往下。最后按 **uuid** 上传进最里层（`Xochitl::upload_file_into` / `upload_large_file_into`，不按名字在全库找——不同上级下可能有同名子文件夹）。某一级等不到就停在已经有的那一级（书落在那里，不会落到别处），回执的 `folder` 照实写实际路径。回执和 `GET /import/{uuid}` 的 `folder` 都是从文档的 parent 往上找到根拼出的完整路径（`rmsvc_core::xochitl::folder_path_of`；名字本身带 `/` 的文件夹拼出来有歧义，只供显示和比对）。此前整串当一个名字，建出叫「漫画/死亡筆記(愛藏版)」的顶层文件夹；设备上已有的两个这种顶层文件夹不自动处理，按（上级, 名字）逐级找也不会误认它们。**未真机验证**（依赖 `createCollection` 传非空 parent，见 §4）。〔10-07 约 16:32 已部署（`deploy.sh --only book` + 整机重启），部署自检 36✓ 1⚠ 0✗；功能仍未真机验证，手测项见书架白皮书附录 §05 #24〕
- 原地替换：`<uuid>.metadata` 不在、`deleted`、在回收站、没有 `<uuid>.epub` → 404（回 202 之前判，收完体再判一次；客户端据此改成新导入；任务排队期间书被删了，任务开头再判，失败进 `message`）；请求体先写到 `import-tmp/` 的临时文件（0600；与书库同在 /home 分区）→ 删 `<uuid>.pdf`、`<uuid>.epubindex` → rename 成 `<uuid>.epub`；删 `.epubindex` 之前先给阅读位置拍快照（2026-10-09，下次打开时由 qmd 让 xochitl 跳回去，书架白皮书 §03ca；拍不成不影响替换）；`.content`/`.metadata` 不动（保留 `lastOpenedPage`、页边距、所在文件夹），**漫画页边距也不重新登记**（每本只设一次，用户在阅读器里调过的不被 sheng-ren 每次同步覆盖）。2026-10-07 审查修复前体直接写在书库里的 `<uuid>.epub.new`，进程被杀时会在书库留下最大 1GB 的半成品，而启动清理只管 `import-tmp/`。同一 uuid 同时只允许一个替换（第二个回 409，2026-10-09 前是 400；忙锁从收体一直持到后台任务做完）。**真机**：10-09 在《绍宋》上走过一次——一本已经打开过、已排版的书被替换后，xochitl 重开时正确重排（新 `.epubindex` 打开当场写出），找回阅读位置跳回原页（书架白皮书 §03ca）；那次替换前后排版没变，内容改动导致页数变化的情形还没测。缩略图不动。
- 删除不另设接口：`POST /trash/add {uuid, name}`（`name` 用导入任务结果里的 `name`，即 visibleName）。
- 超时：rmsvc-core 只有"读请求体时 60 秒收不到一个字节就断"的空闲超时（`READ_IDLE_TIMEOUT`），没有请求体大小上限和总时长上限。改成异步后 `POST /import` 收完体、写完盘就回，客户端读应答不用再按分钟设；等结果靠轮询任务。

2026-10-07 已删（回 404）：`POST /staging/mark`（审查修复删，无调用方）、`GET /reading-direction/{uuid}`（稍后随日漫翻页删，§2.5）、`POST /staging/optimize`、`POST /staging/cancel`、`POST /staging/fetch-article`、`POST /staging/originals/restore|delete`。更早已删：`POST /staging/direction`（09-30）、`GET /inbox`、`POST /inbox/retry|delete`、`GET /staging/render/{uuid}`（09-22）；`/api/koreader/*` 2026-09-29 起不再代理。

**`/api/fonts/*`**（font-serve:8792）：`GET /` · `POST /` · `DELETE /{family}` · `PUT /config {emboldenCjkFallback}` · `GET /status`。
**`/api/wallpapers/*`**（wallpaper-serve:8793）：`GET /` · `POST /[?activate=1]` · `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}` · `GET /status`。

**网关自有端点**：见 [`gateway/README.md`](../../gateway/README.md)「对外接口」。笔记线 API 见 `../../notes/README.md`。

上传回执统一 `{ok, items:[{name, ok, message, item?}], …}`（`rmsvc_core::asset::receipt`），成功项的 `name` 是落地名。上传暂存位置：书进 `books/.work/`（与母版库同分区）；xochitl 字体、壁纸进 `$XDG_STATE_HOME/shelf/upload/`。都在服务启动时清掉上次中途被杀留下的 `.part`。

**请求约定**（`rmsvc-core`，2026-09-24 起）：JSON 请求体上限 1MB，超了回 400"请求体超过 1024 KB 上限"；路径参数与上传的 `filename*=` 解码时 `+` 就是 `+`，只有查询串按表单语义把 `+` 当空格；投 xochitl 时文件名里的 `"` 换成 `'`、换行换成空格。multipart 头参数按引号切分（09-25）：`filename="甲; 乙.epub"` 不再被切成 `甲`。

## 10｜已知限制（如实记录，不是遗漏）

- **书架不检查书优化过没有**：没经过 sheng-ren 的书照样能入库、加入，在 xochitl 上排版好不好全看书本身。2026-10-07 稍后起渲染自检只记页数、不再报 `warn`，整章渲染失败这类问题书架不再自动发现，只能打开书看或看页数是否离谱。
- **书内显示名**：普通上传和大文件通道的占位都显示书里的 `dc:title`，书架只规范文件名、不改书；书名由 sheng-ren 定（2026-10-07 前「优化」会把带卷标记的书 `dc:title` 改成规范名；10-07 稍后前占位对带卷标记的书也用规范名）。
- **从 sheng-ren 抄来的两处**：页边距标记名 `READER_MARGINS_MARKER` 与书名规范化 `canonical_book_name`（§1）。sheng-ren 改了任一处，书架要手动跟着改，否则漫画不再自动设页边距、或文件名规则两边不一致；没有自动检查。
- **旧版优化的漫画**：书架 2026-10-07 前自己优化的漫画不带 sheng-ren 的页边距标记，加入后不会自动设页边距；要用 sheng-ren 重新优化再上传。
- **原地替换后的阅读位置**（书架白皮书 §03ca，2026-10-09，真机验证一次）：只按"所在 spine 文件 + 文件内比例"找回，文件内内容增删多时偏几页（页数变化的情形未真机测）；书里插过笔记页时靠 `.content` 页表换算，重排后 xochitl 怎么安置笔记页没核实过；新排版约 60 秒内没写出 `.epubindex` 就放弃（真机上是打开当场写出，这个时限够用）。
- **漫画页边距没有开关**：带标记的漫画首次打开一律设成 1；不想要的只能在阅读器里每本手动调回。母版库里设备上优化过的其它书照样能加入。
- **旧设备残留**：`staging/.pdf-originals/` 里的旧备份不会再被清理，需要时手动删。
- 网关代理对 ≤256KB 且不是下载的应答、以及没有 `Content-Length` 的应答仍整体缓冲（§1）；它们都是小 JSON，不改。
- 并发控制（§7.3）：note-serve 与 book-serve 同时投 xochitl 时上传锁管不到。
- `shelf-mkdir-agent.qmd` 长轮询 290 秒（依据：设备 Qt 6.10 的 QML XHR 不设传输超时，源码核实）；部署后确认 journal 里没有 `SHELF-MKDIR: transfer timeout`（有就说明退回了 25 秒）。
- inbox 只判"还在不在写"（修改时间静止 5 秒），不校验内容完整：scp 中途断开留下的半截文件静止后仍可能被收进母版库（§2.1）。
- 大文件占位通道：占位上传后进程崩溃会残留占位文档（回执提示手动删）。
- **翻页方向**（§2.5）：书架不再管；xochitl 不看 OPF 方向标记，日漫在 xochitl 里一律从左往右翻（2026-10-07 稍后，用户接受）。
- **超限书**：超过 1GiB、或造不出占位文档时整本拒绝，只能自行把书分成多份再上传。加密到对象流、用 FlateDecode 以外过滤器压交叉引用的少见大 PDF 读不出页数，整本拒绝。
- book-serve 的领域错误一律回 400（第五轮审计记录在案、没改）。（同列的"大文件通道等占位 200ms 轮询"10-09 改成 inotify；目录监听被删或挪走后 10-09 起按退避 1 秒→5 分钟重新挂上。）
- **部署与验证**：10-07 的三批改动与 10-09 第六轮审计都已部署、部署自检通过，功能未真机逐项手测；10-09 找回阅读位置已部署、真机验证一次。要抽查的地方见书架白皮书附录 §05 #22–#26。

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

代码与文档原文都在 git 历史：`git log -- shelf/crates/bookconv` 找自带 bookconv v16 删除前的版本；本文 2026-10-07 前的版本（含当时的 §2.4 PDF 入库、§3 EPUB 优化管线、§3.4 兼容预处理、§5 优化内存表）用 `git log -- shelf/docs/传书EPUB线架构.md` 按日期找。决策经过见书架白皮书 §03bw；当年优化管线的真机记录（原第 B 章各节、§03bv）在[书架白皮书历史附录](reMarkable书架白皮书-历史附录.md)。
