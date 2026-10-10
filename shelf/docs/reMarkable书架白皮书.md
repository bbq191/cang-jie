# reMarkable 书架（shelf）白皮书

> **读者与用途**：写给要使用、维护或排查书架（shelf）的人。本文只讲**现在**：它是什么、怎么用、怎么工作、验证到了哪一步。按时间写下的来龙去脉（原来的 §03xx 各节）在[历史附录](reMarkable书架白皮书-历史附录.md)；接口字段、锁表这类实现细节在[传书链路技术细节](传书EPUB线架构.md)。
> - **按 2026-10-10 的代码写**（master，含 10-10 重构与安装脚本加固）。**10-10 的改动还没部署到设备**；设备上跑的是 10-09 部署的版本。两者不同的地方，正文会标"10-10"，部署与真机验证状态统一在第 8 节「验证现状」。
> - **"已部署"不等于"真机验证过"**：已部署＝装上设备、部署自检（`packaging/verify-on-device.sh`）通过、服务健康；真机验证＝那项功能在设备上实际走过一遍。
> - **旧章节号**：2026-10-10 前本文按时间编号（§00、§03b … §03cc、§04、§05 #N、第 0/A–F 章、附录 A–D），代码注释和别的文档里还引用着这些号——查第 9.3 节「旧章节号对照表」。

## 目录

1. 这是什么
2. 五分钟读懂：能做什么、由什么组成
3. 怎么用
4. 怎么工作
5. 接口与配置参考
6. 开发与维护
7. 已知限制与排错
8. 验证现状
9. 附录：设计决策、已移除的能力、旧章节号对照表

## 1｜这是什么

shelf（书架）是跑在 reMarkable Paper Pro Move 上的一组网页服务。你用手机或电脑浏览器把书（EPUB / PDF）传进设备的**母版库**（永久保存原书的地方），再点「加入 xochitl」把它放进设备自带的阅读器。字体和休眠壁纸也做成"上传即可用"。

它解决的问题：xochitl 自带的网页上传接口单个文件不能超过 100,000,000 字节，也没有建文件夹的接口。书架在它之上补了：多文件上传与 scp 投递、永久保存原书的母版库、批量加入、超过上限的大书、按文件夹放（不存在就建）、原地换新版本后找回阅读位置。

它**不做**的事：
- **不改书**。书的优化（排版、注释、目录、漫画处理）2026-10-07 起全部在电脑上用 [sheng-ren](https://github.com/bbq191/sheng-ren) 的 booklib 做（`xochitl` 阅读模式），书架只把优化好的书原样投进去。
- **不改 xochitl 本体**。只用 xochitl 自带的网页上传接口、直接读写它的书库目录，以及注入几个 qmd 界面补丁（见下面的名词表）。

## 2｜五分钟读懂：能做什么、由什么组成

### 2.1 能做什么

![一本书的旅程](diagrams/book-journey.svg)

用户视角只有三步：**电脑上用 sheng-ren 优化 → 网页上传（或 scp 进 inbox）→ 母版库里勾选「加入 xochitl」**。

| 能力 | 一句话 | 详见 |
|---|---|---|
| 入库 | 网页多文件上传带进度；scp 丢进 `inbox/` 写完自动收；只收 EPUB / PDF；字节原样；文件名规范成 `书名 - 02卷`；同名同内容不重复存 | 3.2 |
| 加入 xochitl | ≤90MB 普通上传；更大的走"占位 + 磁盘替换"，上限 1GiB；可选书库根下的文件夹（没有就请 xochitl 自己建）；记下渲染页数 | 3.3、4.2 |
| 批量 | 勾选多本，网关后台一本一本加，网关重启后续跑，可全部中止 | 3.3、4.7 |
| 母版库管理 | 只勾一本时下载原件、改名；删除；筛选 全部 / 未加入 / 已加入；剩余空间 <300MiB 标红 | 3.3 |
| 漫画页边距 | sheng-ren 优化的漫画带页边距标记，加入后首次打开自动把页边距设到最小（没有开关） | 4.4 |
| 直接导入 | 给电脑上的 sheng-ren 用：不进母版库，直接按多级文件夹放进 xochitl；也能按 uuid 原地替换一本已有的书 | 3.4 |
| 替换后找回阅读位置 | 原地替换一本在读的 EPUB 后，重开时跳回原来读到的地方 | 4.5 |
| 字体 / 壁纸 | xochitl 字体上传即装（改写中文回退链）；休眠壁纸池（写 xochitl 的 `SleepScreenPath` 键） | 3.5 |

### 2.2 由什么组成

![shelf 架构：组成部分与连接](diagrams/architecture.svg)

| 组件 | 端口 / 位置 | 职责 | 源码 |
|---|---|---|---|
| gateway（网关） | `0.0.0.0:443`，唯一对外 | HTTPS + 登录密码、单页网页、按 `/api/<段>/*` 反向代理、批量队列、事件汇聚 | `gateway/` |
| book-serve | `127.0.0.1:8790` | 母版库（入库、改名、删除、下载原件）、加入 xochitl、直接导入 / 原地替换、四个 qmd 代理的待办队列 | `shelf/services/book-serve` |
| shelf-conv | 库，链进 book-serve | **只读不改书**：读 EPUB 的书名 / 封面 / 漫画页边距标记、第三方 PDF 页数、大文件通道的占位文档、文件名规范化 | `shelf/crates/shelf-conv` |
| font-serve | `127.0.0.1:8792` | xochitl 字体上传即装 | `enhance/font-serve` |
| wallpaper-serve | `127.0.0.1:8793` | 休眠壁纸上传即用 | `enhance/wallpaper-serve` |
| 笔记线四个服务 | `8795–8798` | 挂在同一个网关上，见 [`notes/README.md`](../../notes/README.md) | `notes/` |
| rmsvc-core（基座） | 库 | 书架、笔记、系统增强、网关共用的 Web 服务底座：HTTP、事件、上传、路径表、往 xochitl 上传并认领，见 [`rmsvc-core/README.md`](../../rmsvc-core/README.md) | `rmsvc-core/` |

服务启动时往 `$XDG_RUNTIME_DIR/shelf/services/` 写一份注册信息，网关据此出页签、转发（段与服务的对应只写在 `gateway/src/manage.rs` 的 `MODULES` 表里：`books`→book-serve、`fonts`→font-serve、`wallpapers`→wallpaper-serve）。**装 / 卸一个服务 = 一个二进制 + 一个 systemd 单元**。systemd：`shelf.target` + 各服务 `PartOf=shelf.target`；**绝不给 xochitl 加启动依赖**（曾因此变砖）。

### 2.3 名词小词典

| 词 | 意思 |
|---|---|
| xochitl | reMarkable 官方的界面 + 阅读器进程；书架不改它，只和它"握手" |
| xovi / qmd / qmldiff | xovi 是给 xochitl 加载扩展与界面补丁的第三方机制；qmd 是对 xochitl 界面文件（QML）的补丁；qmldiff 是生成 / 应用这类补丁的工具 |
| 母版库 | `~/.local/state/shelf/books/staging/`，原书永久保存处；加入 xochitl 都从它出发，母版不会因此删除 |
| 落库 / 加入 xochitl | 把母版的字节复制进 xochitl 书库 |
| 边车（sidecar） | 每本书旁的隐藏小 JSON `.<书名>.delivered`，记"最近一次加入的时间、结果、渲染页数" |
| 投递层 | book-serve 里的 `XochitlDelivery`：建文件夹 → 上传 → 认出新文档 → 登记页边距，落库与直接导入共用（10-10） |
| 认领 | xochitl 的上传接口不告诉你新文档的 uuid，只能事后在书库里认：按"上传前不在、字节与母版相同"认出来；认不出就不认 |
| 占位 + 磁盘替换 | 绕开 xochitl 上传 100MB 上限：先传几 KB 占位让它建条目，再把磁盘上的文件换成真书 |
| qmd 代理 | 注入 xochitl 的 qmd，向 book-serve 取待办、调 xochitl 自己的函数（外部进程改书库文件会被 xochitl 盖回去） |
| 渲染自检 | 加入 EPUB 后等 xochitl 排版完，读出页数记进边车 |
| SSE | 服务端事件推送；网页据此即时刷新，不轮询 |
| sheng-ren / booklib | 姊妹项目，电脑端的书籍优化工具，不在本仓库；书架上的书应该先经过它 |

## 3｜怎么用

### 3.1 访问与登录

- 地址 `https://shelf.local/`（网关自带 mDNS）或 `https://<设备IP>/`。**安卓不解析 `.local`**：用 IP，或在电脑热点的 dnsmasq 加别名（示例见网关白皮书）。**传大书走 USB 的 `https://10.11.99.1/`**：WiFi 热点上行弱，xochitl 每导入一本书还会同步到云，占满弱热点的上行（历史附录 §03l）。
- 登录只要密码：首次默认 `shelf`，登录后强制改（≥6 位）。网页右上「改密码」，或设备上 `gateway passwd <新密码>`；忘了用 `gateway reset-password`。
- 证书由网关的私有 CA 签发：登录页「下载 CA 证书」装进手机 / 电脑信任库一次，之后不再有"不安全"提示。

### 3.2 入库：把书放进母版库

两条入口，都**原样入库**：

| 入口 | 怎么做 | 规则 |
|---|---|---|
| 网页上传 | 「传书 → 入库」，可多文件，有进度 | 同名同大小的视为已落地、跳过 |
| scp | 丢进设备 `~/.local/state/shelf/books/inbox/` | 文件静止 5 秒后才收（防止收进写到一半的书）；失败的挪进 `failed/` 并带 `.reason` 说明，重试＝拷回 `inbox/` |

![scp 往 inbox 传大书：什么时候才入库](diagrams/sh-inbox-settle.svg)

共同规则：
- **只收 EPUB / PDF**（策略，不是技术限制）；其它格式先在电脑上转好。
- 文件名规范成 `书名 - 02卷`（去掉下载站尾巴，卷号规整）；**只改文件名，不改书里的书名**——xochitl 显示的书名取书内 `dc:title`，由 sheng-ren 决定。
- 同名、内容逐字节相同：认已有那本，不再存一份；同名不同内容：加数字前缀，不覆盖。

### 3.3 母版库：加入 xochitl 与管理

「传书 → 母版库」：行内只显示状态（格式、大小、加入记录、渲染页数、处理中 / 失败），**所有操作在勾选后的底部栏**：

- **加入 xochitl**：可勾多本，网关排队一本一本加，网关重启后续跑；「全部中止」只清还没开始的（已经交给 book-serve 的那本会做完，没有安全的中断点）。只有 book-serve 回报成功才算成功，等不到、查不到、途中消失都记失败，并提示"到 xochitl 书库里核对"（免得重加出两本）。
- **加入位置**：常驻下拉，列的是 xochitl **书库根下**的文件夹；可"＋新建"。选"卷01"就是根下的「卷01」，不存在就请 xochitl 自己在根下建（最多等 20 秒，等不到书落书库根）。留空＝书库根。（10-10 前会按名字在全库任意层找，见 4.2。）
- **只勾一本时**：「下载原件」（全程流式）、「改名」（只改文件名，扩展名不变）。
- **删除**：正在处理的书删不了（提示"正在处理中"）；已经加入 xochitl 的副本不受影响。
- **筛选**：全部 / 未加入 / 已加入（默认「未加入」；处理中、加入失败的都算未加入）+ 格式（EPUB / PDF）；搜书名；分页 25/50/100。
- 母版库所在分区剩余不到 300MiB 时标红。

**体积与结果**：≤90MB 普通上传；>90MB 走"占位 + 磁盘替换"（≤1GiB），首次打开时 xochitl 才排版（156.5MB 漫画首次打开约 74 秒）；超过 1GiB 或造不出占位就整本拒绝，回执末尾写"没有加入"。

**漫画页边距**：sheng-ren 优化的漫画带 `META-INF/eink-reader-margins` 标记，加入后首次打开自动把页边距设成 1（左右留白≈0）；之后你在阅读器里调回，不会再被改。不带标记的书（文字书、PDF、书架 2026-10-07 前自己优化的漫画）完全不碰。

**翻页方向**：书架不管。xochitl 不看书里的方向标记，日漫在 xochitl 里一律从左往右翻（用户已接受）。单击翻页是系统增强线的功能。

### 3.4 给 sheng-ren 用：直接导入与原地替换

电脑上的 sheng-ren（`booklib sync`）经 **SSH 端口转发**（`ssh -L` 到设备 `127.0.0.1:8790`）直连 book-serve，**不经网关、不进母版库**：

- `POST /import?name=&folder=`：直接导入一本 EPUB。`folder` 是 `/` 分隔的多级路径，逐级找或建；目标文件夹里已有逐字节相同的书就认它，不重复加入。
- `POST /import?uuid=&name=`：原地替换一本已有书的内容，uuid 不变、所在文件夹与阅读进度都保留；**重开时会自动跳回原来读到的地方**（4.5）。
- 都是异步任务：先回 `202 {job}`，再查 `GET /import/jobs/{id}`。与网页「加入 xochitl」排进同一个作业队列，一本一本做。

接口细节见[传书链路技术细节](传书EPUB线架构.md) §9「直接导入」。

### 3.5 字体与壁纸

- **字体**（「其他 → xochitl」）：上传 TTF / OTF / TTC 即装，字体菜单里立即出现、选中立即生效，不用重启；中文字体会被写进 fontconfig 的中文回退链（缺字时逐字回退）。界面字体令牌 `ui-font-tokens.qmd` 随 font-serve 一起装（只在 3.28 固件）。
- **壁纸**（「其他 → 壁纸」）：上传后缩成 954×1696 PNG 进壁纸池，可顺序 / 随机 / 固定轮换；写的是 xochitl 的隐藏键 `SleepScreenPath`，**第一次写键后要整机重启一次**，之后每次休眠都重读。
- 这两个服务的源码与细节在系统增强线（[`enhance/README.md`](../../enhance/README.md)）；书架负责安装与网页入口。

### 3.6 网页的布局

首层四个固定页签：**传书**（入库、母版库）· **笔记**（笔记线）· **其他**（xochitl 字体、壁纸，只列已装且在跑的服务）· **管理**（基石与模块、设备健康、模型、系统增强、实验室）。网页不定时轮询，靠事件推送即时刷新。网页本身的设计见 [`gateway/docs/reMarkable网关白皮书.md`](../../gateway/docs/reMarkable网关白皮书.md)。

## 4｜怎么工作

### 4.1 母版库里一本书的一生

![母版库里一本书的一生](diagrams/sh-staging-item-life.svg)

两套状态各管一件事：
- **忙锁**（`OpRegistry`，内存）：答"现在有没有线程在做这本书"。按书名；加入、改名、删除互斥，冲突回 409；排队等加入的书也占着忙锁。**不落盘**：book-serve 重启即清零（落盘反而会"永久卡忙"）。
- **边车**（`.<书名>.delivered`，磁盘）：答"上次做到哪、结果如何"。`deliver`：`pending → ok / failed`；`render`：`pending → ok（记页数）/ timeout`，大文件通道的 EPUB 先记 `onopen`，首次打开排版后列表自动升成 `ok`。线上值是基座 `wire` 里的枚举（10-10），旧边车里的历史值读成 `unknown`。
- **重启修正**：停在 `pending` 的加入记为失败（"服务重启，上次加入被中断，可重新加入"）、渲染记 `timeout`；母版库目录里点前缀、`.tmp` 结尾的半成品全删；孤儿边车清掉。

母版库列表每项带后端算好的 `title`（显示名）、`series`（搜索分组）、`done`（已加入、不在处理、没失败）——10-10 起由后端下发，网页不再自己算。字段表见传书链路技术细节 §2.2。

### 4.2 加入 xochitl：投递层

![加入 xochitl：一本书怎样进书库](diagrams/deliver-decision.svg)

1. **HTTP 层只做零耗时检查**：格式（EPUB/PDF，否则 400）、书在不在（否则 404）、忙不忙（否则 409）；通过就加忙锁、边车记 `pending`、排进**作业队列**，立即回"已开始"。作业队列是 book-serve 里唯一的工作线程（`jobs.rs`），网页落库与直接导入共用、一次一件（10-10；之前每次落库新起线程，几本书能同时往 xochitl 传）。
2. **解析文件夹**（`XochitlDelivery::ensure_folder`）：留空＝书库根；填了名字＝书库**根正下方**按名字找，没有就请建文件夹代理在根下建，最多等 20 秒，等不到落书库根。名字里的 `/` 是普通字符（《乱马1/2》）。上传前"设当前文件夹"失败（比如那个文件夹刚在 xochitl 里删了）也落书库根。
3. **≤ 体积门**（缺省 90MB）普通上传，流式发送、不把整本读进内存：
   - **PDF** 只上传，不需要 uuid（没有渲染自检、不登记页边距）。
   - **EPUB** 上传并**当场认领**（基座 `Xochitl::upload_and_claim`）：上传前拍书库快照 → 上传 → 等书库目录变化 → 在快照之外的新文档里找 `<uuid>.epub` 与母版**逐字节相同**的那份。xochitl 回了 2xx 最多等 20 秒；读超时 / 408（"很可能已送达"，大书还在排版）最多等 30 分钟。**认不出就不认**：落库照算成功，渲染记 `timeout`、不登记页边距——绝不把别人刚投的书当成自己的（10-10 前"都不符时取最新一本"，并发投递会认错）。
   - 认出后另起**渲染自检**线程：先登记漫画页边距（书里带标记时），再限时 10 分钟监听书库目录，等 xochitl 写出页数。
4. **> 体积门**走大文件通道（4.3）。
5. 作业结束：边车写结果、解忙锁、推 `books/staging` 事件，网页刷新。

为什么 90MB：真机二分测出 xochitl `/upload` 的硬上限是 **100,000,000 字节**（超了直接断连），90MB（94,371,840 字节）留安全余量。`nativeUploadLimitMb` 可改，0＝不拦。

### 4.3 大文件通道：占位 + 磁盘替换

book-serve 跑在设备上，能直接写 xochitl 书库目录。所以超过体积门的书：造一个带**真书名和真封面**的占位（EPUB 几十到几百 KB；PDF 是手写的一页最小文件）→ 普通上传占位 → 按占位大小认出新文档的 uuid → 把母版拷成 `<uuid>.<ext>.new` → EPUB 删渲染缓存（`.pdf`、`.epubindex`），PDF 改写 `.content` 里的页数 → 原子改名覆盖占位。不用重启 xochitl。

- 条件：EPUB / PDF、≤1GiB、本机有书库目录、造得出占位。PDF 要先读出真页数（`shelf_conv::pdfmeta` 有界解析，最坏约 48MB 内存；加密对象流等少见写法读不出就整本拒绝）。
- 占位必须带真书名、真封面：xochitl 用占位的书名和封面，替换后不补生成（真机踩过）。
- 整段串行：PDF 的占位字节都一样，两本同时投会认到同一个条目，所以从传占位到替换完成在基座里用一把锁串起来。
- 占位已上传之后才出的错直接报错，书库里可能留下半成品占位（回执提示手动删，不做危险的回滚删除）。

流程图见 [`docs/diagrams/upload-limit-bypass.svg`](../../docs/diagrams/upload-limit-bypass.svg)，认领细节见基座的 [`large-file-claim.svg`](../../rmsvc-core/docs/diagrams/upload-claim.svg)。

### 4.4 为什么要 qmd 代理

![让 xochitl 自己动手：四个 qmd 代理](diagrams/sh-xochitl-agents.svg)

外部进程改 xochitl 书库里的 `.metadata` / `.content` 会被运行中的 xochitl 用内存里的状态盖回去（真机：外部改 `.content` 设页边距 5 次只成功 1 次）。所以要 xochitl "动手"的事，book-serve 只排队，由注入 xochitl 的 qmd 来取、调 xochitl 自己的函数：

| 代理 | 做什么 | 怎么取活 | 调什么 |
|---|---|---|---|
| `shelf-mkdir-agent.qmd`（MainView） | 建文件夹（按上级 + 名字，支持多级） | 长轮询 `GET /mkdir/pending?wait=290` | `Library.createCollection(上级, 名字)` |
| `shelf-trash-agent.qmd`（MainView） | 把文档移进回收站（网页「设备健康 → 清理」、笔记线挪走旧版笔记本） | 长轮询 `GET /trash/pending?wait=290` | `LibraryController.moveEntriesToTrash([Library.entryForId(uuid).id])` |
| `shelf-comic-margins.qmd`（DocumentView） | 带标记的漫画首次打开设页边距 1 | 开书 1.5 秒后问 `GET /margins/{uuid}` | `EpubProperties.setMargins(1)` |
| `shelf-keep-progress.qmd`（DocumentView） | 原地替换后跳回原阅读位置（4.5） | 开书后问 `GET /progress/{uuid}`，最多约 60 秒 | `sceneView.goToPage(n)` |

长轮询的交付规则：同一项交出后 15 秒（建文件夹）/ 30 秒（回收站）内不重交；同一项最多交 5 次，还做不成就放弃、记进 `agent-failures.json`，网页页头横幅提示（点「知道了」清空）；qmd 请求出错时退避 15 → 30 → 60 → 120 秒。回收站入队时按书名核对 uuid（防错删），读不了 `.metadata`（xochitl 正在改写）时先留着下次再看。

**qmd 只在 xochitl 启动时注入**：装了 / 改了要整机重启才生效（6.4）。

### 4.5 原地替换后找回阅读位置

![原地替换后找回阅读位置](diagrams/sh-keep-progress.svg)

sheng-ren 原地替换一本在读的 EPUB 后，xochitl 重开时整本重排、页号全变，自己只会按比例估一页，书改动大时偏很远。书架的做法：

1. 替换前（`finish_replace` 删旧 `.epubindex` 之前）book-serve 拍快照：`lastOpenedPage` → 按旧 `.content` 页表换成 PDF 页 → 用旧 `.epubindex`（xochitl 记的"每个章节文件从第几页开始"）找到所在章节文件和文件内比例，另存全书比例兜底；写 `~/.local/state/shelf/books/progress/<uuid>.json`。拍不成只记日志，不影响替换。
2. 重开时 `shelf-keep-progress.qmd` 在开书、页数变化、目录到达三个时机各起一个 2 秒的计时器去问 `GET /progress/{uuid}`：没有快照 404 收工；新 `.epubindex` 还没写出 202，每 5 秒再问，约 60 秒放弃；出来了 200 `{page}`，qmd 调 `sceneView.goToPage(page)`，15 秒内被 xochitl 自己跳走就跳回（最多 3 次），然后 `POST /progress/applied` 销账。
3. 换算：新页 ＝ 新 `.epubindex` 里同名章节文件的起始页 + round(文件内比例 × 该文件页数)；文件改名或拆分找不到时用全书比例。

只对 EPUB，没有开关。真机验证过一次（8.2），已知限制见 7.1。

### 4.6 并发与锁

![书架的并发控制：哪条路径进哪把锁](diagrams/sh-concurrency-locks.svg)

原则：**锁只包住真正会撞的那一小段**。网页的加入靠网关批量队列一本一本来；book-serve 进程内有忙锁、落名临界区、spool 锁、边车写锁、作业队列；"设当前文件夹 → 上传"这一对用锁文件的 `flock` 管到别的进程（note-serve 投笔记本也排进来，10-10）。完整锁表与来由见传书链路技术细节 §7.3。

### 4.7 事件推送与批量队列

- **事件**：book-serve 在状态变化处发事件（`area=books`，`kind`：`staging`/`inbox`/`render`/`trash`/`mkdir`/`agent-failed`/`import`，常量在 `events.rs`）→ 网关汇聚成受登录保护的 `GET /api/events`（SSE）→ 网页只刷对应页签，页面隐藏时只记一笔、可见时补刷。前端没有任何定时轮询。
- **批量队列**在网关（`gateway/src/batch.rs`）：只有「加入 xochitl」一种动作；顺序逐本、状态落盘续跑、可全部中止。见传书链路技术细节 §7.2 与网关白皮书。

### 4.8 内存、耗电与可靠性

- 设备内存约 2GB，**systemd 的 `MemoryMax` 在设备上实测不生效**，内存全靠代码约束：上传与下载都流式；大文件通道本机拷贝；读 EPUB 只读 container.xml、OPF、几页正文和封面，单条目解压上限文本 16MB、封面 64MB；PDF 页数有界解析。2026-10-07 书架不再优化书后，设备上最吃内存的活（解码整本书的图片）已经没有了。
- 定内存阈值铁律：**先在真机实测 `VmHWM`，不靠"字节数乘法"估算**（历史附录 §03bi）。
- 耗电：空闲不轮询；母版库列表按文件戳缓存（长度 + mtime + inode）；qmd 代理空闲约每 5 分钟往返一次。
- 可靠性：`panic = "unwind"` + 后台线程 `catch_unwind`，一本坏书不会摔掉整个服务；accept 循环遇到暂时性错误跳过继续，真结束时以错误退出交给 systemd 拉起。

### 4.9 字体、壁纸与设备

- font-serve 每次上传 / 删除都**动态重写** fontconfig：中文回退指向当前已装的中文字体（覆盖率降序）、全部 `weak`（所选字体永远在最前，缺字才逐字回退）；覆盖率 <8% 不入回退链。旧中文化套件写死回退曾导致"方框 + 选了不生效"（历史附录 §03k）。
- 字体菜单 qmd：3.28 版先删后加（只删自己加的条目）。
- 只支持 Paper Pro Move、固件 3.28.0.172；换固件后要重装（`/home` 保留，`/usr` 与 `/etc` 被冲，见 6.5）。
- WiFi 连上恰好 60 秒就掉的真凶是精简 `regulatory.db` 里 CN 没有 5150–5350 MHz；看护脚本在 `packaging/wifi-watch/`（历史附录 §03w）。

## 5｜接口与配置参考

### 5.1 接口一览

book-serve 的全部接口（含请求 / 应答字段、状态码）在[传书链路技术细节](传书EPUB线架构.md) §9。经网关时加前缀 `/api/books`。主要的：

| 接口 | 用途 |
|---|---|
| `GET /staging` · `POST /staging` · `GET /staging/file?name=` · `POST /staging/rename` · `POST /staging/delete` | 母版库列表、上传、下载原件、改名、删除 |
| `POST /staging/deliver {name, folder?}` | 加入 xochitl（异步） |
| `GET /status` | `{ok, xochitlFolders}`：书库根下的文件夹 |
| `POST /import` · `GET /import/jobs/{id}` · `GET /import/{uuid}` · `POST /import/states` | 直接导入与查询（sheng-ren 用） |
| `GET /mkdir/pending` · `GET /trash/pending` · `GET /margins/{uuid}` · `GET /progress/{uuid}` 及各自的 `applied` / `add` | qmd 代理用 |
| `GET /events` | SSE |

**错误状态码**（10-10）：请求不对 400、不在 404、冲突（正在处理中、名字已占用）409、设备出错 500。网页和 sheng-ren 只看是否成功和 `message`。

### 5.2 配置

`~/.config/shelf/book.json`（缺省可用，首启写出缺省文件）：

| 键 | 缺省 | 说明 |
|---|---|---|
| `xochitlHost` | `10.11.99.1` | xochitl 网页上传接口所在主机 |
| `uploadTimeoutSecs` | 300 | `/upload` 超时；超时但很可能已送达时不重试 |
| `nativeUploadLimitMb` | 90 | 体积门：≤ 普通上传，> 大文件通道；0＝不拦 |

配置文件坏了照常按缺省运行，并另存一份 `.corrupt` 留证（10-10，基座 `load_or_seed`）。

### 5.3 设备上的路径

设备 HOME=/home/root；路径表的唯一事实源是 `rmsvc_core::paths`。

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{gateway,*-serve,shelf-uninstall,lo-alias.sh}` |
| 卸载脚本用的库 | `~/.local/lib/shelf/{manifest.sh,devlib.sh}`（每次安装都刷新，含 `--only`） |
| 安装备份 | `~/cangjie-backups/shelf-<时间戳>/`（保留最近 5 份） |
| 配置 | `~/.config/shelf/<服务>.json` · `~/.config/shelf/tls/`（CA + 叶证书） |
| 数据 | `~/.local/share/shelf/`（`fonts.json`、壁纸池）· `~/.local/share/fonts/`（用户字体）· 系统增强开关 `~/.local/share/cangjie-ime/reading-qol.json` |
| 母版库 | `~/.local/state/shelf/books/staging/`（永久保留；临时文件只有 `.<pid>.<序号>.landing.tmp`） |
| 入库队列 | `~/.local/state/shelf/books/{inbox,.work,failed}/`（`.work/` 兼网页上传暂存） |
| 代理队列 | `books/{mkdir,trash}-pending.json`、`books/comic-margins.json`、`books/agent-failures.json`（最近 20 条） |
| 阅读位置快照 | `books/progress/<uuid>.json`（超过 30 天启动时清） |
| 直接导入暂存 | `books/import-tmp/` |
| 网关 / 壁纸状态 | `~/.local/state/shelf/batch.json`（网关批量队列）· `~/.local/state/shelf/wallpaper-state.json`（壁纸轮换） |
| 字体 / 壁纸上传暂存 | `~/.local/state/shelf/upload/`（在 /home，不占内存） |
| 运行时 | `$XDG_RUNTIME_DIR/shelf/`（设备上缺省 `/tmp/shelf-0/shelf/`）：`services/` 注册表、`xochitl-upload.lock` 跨进程上传锁；重启即清 |
| xochitl 书库 | `~/.local/share/remarkable/xochitl/` |
| 旧设备残留 | `staging/.pdf-originals/`（10-07 前的 PDF 备份，不再读写、不再自动清）、`books/done/`（09-03 遗留）、`koreader-backups/`、`koreader-upload/`；需要时手动删，`shelf-uninstall --purge` 会随 `~/.local/state/shelf` 一起删 |

### 5.4 qmd 清单

装在 `~/xovi/exthome/qt-resource-rebuilder/`，清单的唯一事实源是 `shelf/manifest.sh` 的 `shelf_svc_qmds`：

| 随哪个服务装 | qmd |
|---|---|
| book | `shelf-trash-agent.qmd`、`shelf-mkdir-agent.qmd`、`shelf-comic-margins.qmd`、`shelf-keep-progress.qmd`、`reader-page-turn.qmd`（单击翻页，归系统增强线） |
| font | `font-menu-dynamic.qmd`（按固件选 3.28 / 3.27 版）、`ui-font-tokens.qmd`（只在 3.28） |

## 6｜开发与维护

### 6.1 目录与依赖

```
shelf/
├── Cargo.toml · build.sh · .cargo/   workspace；aarch64 musl 全静态交叉编译
├── crates/shelf-conv/                只读不改书：epub（书名、封面、页边距标记）、pdfmeta、placeholder、naming
├── services/book-serve/              母版库服务（staging/ 领域、delivery.rs 投递层、jobs.rs 作业队列、api.rs 路由……）
├── systemd/                          shelf.target + book-serve 单元
├── xovi/                             qmd（见 5.4）
├── install.sh · uninstall.sh · manifest.sh   设备端安装 / 卸载与共用清单
└── docs/                             本文、历史附录、传书链路技术细节 + diagrams/
```

依赖单向无环：`book-serve → shelf-conv → rmsvc-core/epubpkg`；`book-serve → rmsvc-core`。`epubpkg` 是与笔记线共用的 EPUB 容器 / OPF 解析小 crate，不依赖 rmsvc-core 本体。书架**不依赖 sheng-ren 的代码**（6.7）。

### 6.2 构建与测试

一次性准备：`rustup target add aarch64-unknown-linux-musl`，再装 aarch64 交叉 gcc（Arch：`pacman -S aarch64-linux-gnu-gcc`，只用来编 `ring` 的 C 部分）。

```sh
cd shelf && sh build.sh            # host 测试 + aarch64 交叉编译（网关、enhance、笔记线的目录在就一起编），cargo 一律 --locked
cargo test --workspace --locked    # 只跑测试：2026-10-10 实跑 book-serve 123 + shelf-conv 24 个通过
```

门槛：clippy 零告警；所有 `.sh` 过 `shellcheck --severity=warning`；安装脚本另有本机模拟测试（`packaging/tests/`，见 `packaging/README.md`）。本仓库 Rust 不走 rustfmt，**别跑 `cargo fmt --all`**。本机起服务 / 跑测试要用 `Paths::sandbox` 或 `env -i`，别让 XDG 目录落到开发机真实路径。

### 6.3 部署与卸载

![书架怎么装到设备上](diagrams/sh-install-flow.svg)

```sh
packaging/install-all.sh <设备IP>                    # 整套设备增强（固件门、xovi、enhance 扩展、书架、笔记线），见 docs/INSTALL.md
cd packaging && sh deploy.sh 10.11.99.1              # 只更新书架这一组服务；只有 WiFi 时给 WiFi IP
sh deploy.sh 10.11.99.1 --only font,wallpaper        # 只装部分服务（网关总会装）；SHELF_NO_BUILD=1 跳过编译
sh deploy.sh 10.11.99.1 --password '新密码'           # 顺便设网关密码（经 ssh 标准输入传，不上命令行）
ssh root@10.11.99.1 '~/.local/bin/shelf-uninstall' [--only font] [--purge] [--dry-run]
```

- `--only` 令牌：`gateway book font wallpaper ink transcribe mind note`（唯一事实源是 `manifest.sh` 的 `SHELF_ALL`；未知令牌退出码 2）。
- `install.sh` 依赖同目录的 `manifest.sh` 与 `devlib.sh`，`deploy.sh` 会一起打包；**只拷一个 install.sh 到设备不够**。
- 安装幂等、失败不留半成品：先校验载荷；旧文件备份；二进制 / qmd 原子替换；写 `/usr` 前实检 dm-verity；只重启"变了、没在跑、还在跑被替换掉的旧二进制、或只改了密码的网关"的服务；qmd 换上一份就当场记"待生效"标记（10-10 加固）。旧命名遗留（`shelf-gateway`、`koreader-serve` 等，见 `manifest.sh` 的 `SHELF_LEGACY_*`）每次安装顺手清掉。
- 载荷确定性打包，设备上已是同一份就不重传（10-10）。主机端编排的其余细节见 [`packaging/README.md`](../../packaging/README.md)。

### 6.4 让 qmd 改动生效（速查）

qmd 和 xovi 扩展只在 xochitl **启动时**注入。**一律整机重启**，别单独重启 xochitl：

| 情况 | 做法 | 为什么 |
|---|---|---|
| 装了 / 改了 qmd 或扩展 | 电脑上 `packaging/deploy-xovi-apply.sh <设备>`，或设备上 `reboot`；`install-all` 最后一步有待生效改动才自动重启 | 整机重启约 20–60 秒回来；开机时 `xovi-reenable.service` 自动恢复 xovi |
| `systemctl restart xochitl` | **别用**（2026-09-25 起） | xochitl 退出时自身有概率崩溃，崩了走应急路径整机重启 |
| xovi 已生效时跑 `xovi/start` | **禁止** | 运行中的 xochitl 崩溃，触发整机重启（2026-09-20 真机事故） |
| 没装 xovi 持久化、刚开机 xovi 没生效 | 设备上 `/home/root/xovi/start` | 先查 `/proc/<xochitl pid>/environ` 的 `LD_PRELOAD` 含不含 `xovi.so` |

重启前先告知用户（会打断阅读）。权威说明在 [`docs/INSTALL.md`](../../docs/INSTALL.md)。`shelf-uninstall` 删了 qmd 也会记"待生效"标记并提示整机重启。

### 6.5 固件升级（OTA）之后

`/home` 数据不丢，但 `/usr` 里的 systemd 单元和 `/etc` 没了，要重装功能：设备旁手动 `xovi/rebuild_hashtable`，再在电脑上重跑 `packaging/install-all.sh <设备IP>`。权威步骤在 [`docs/INSTALL.md`](../../docs/INSTALL.md)「固件升级（OTA）之后」。qmd 依赖 xochitl 内部 QML，大版本升级常要重新适配。

### 6.6 工程原则

四条硬原则（2026-09-03 用户两轮驳回后定，至今有效）：
1. **XDG 基目录规范**：路径表单一事实源是 `rmsvc_core::paths`；shell / qmd 用同一张表的缺省展开值，env 可注入测试。
2. **设计模式去重解耦**：领域模块 + 纯适配层（`staging/` 是领域、`api.rs` 只取参回执）；资产上传模板 `UploadTarget` / `AssetUploadFlow`；服务启动模板 `ServiceSpec`；单一事实源（`formats`、`manifest.sh`、`MODULES`）。只抽行为 / 传输样板，不合并各服务的业务语义。
3. **专项专用可插拔**：按领域拆服务，不做单体。
4. **不引用旧项目 crate、不对接旧路径**（能力只许剥离移植）。

其它约定：改名类重构必须同时写迁移清理（`manifest.sh` 的遗留清单）；qmd 改动先用 `qmldiff apply-diffs` 对从固件解出的真实 QML 离线验证（打不中的选择器会让整个 qmd 不生效，方法见系统增强线白皮书）。

### 6.7 从 sheng-ren 复制的两处（要手动跟改）

书架 2026-10-07 起不依赖 sheng-ren 的代码，但有两处是从它复制来的，**sheng-ren 改了这边要手动跟着改**，没有自动检查：

| 复制的东西 | 位置 | 不跟改的后果 |
|---|---|---|
| 漫画页边距标记名 `READER_MARGINS_MARKER`＝`META-INF/eink-reader-margins` | `shelf/crates/shelf-conv/src/epub.rs` | 书架不再认漫画、不自动设页边距 |
| 书名规范化 `canonical_book_name` | `shelf/crates/shelf-conv/src/naming.rs` | 入库文件名规则两边不一致 |

取舍：少 95 个依赖包、不随 sheng-ren 加依赖而变；代价是两边会漂（历史附录 §03bx）。

## 7｜已知限制与排错

### 7.1 已知限制

- **书架不检查书优化过没有**：没经过 sheng-ren 的书照样能入库、加入；渲染自检只记页数，整章渲染失败这类问题书架发现不了，只能打开书看或看页数是否离谱。
- **超限书**：超过 1GiB、造不出占位就整本拒绝，只能自己分成多份再上传。加密到对象流、非 FlateDecode 压缩交叉引用的少见大 PDF 读不出页数，也整本拒绝。大文件通道占位上传后进程崩溃会残留占位文档。
- **没有中途停止**：剩下的投递都没有安全中断点；批量「全部中止」只清还没开始的。
- **认不出就不认**（10-10）：落库的 EPUB 等不到认领时只记 `timeout`、不登记漫画页边距；认领期间这本书占着作业队列（xochitl 回 2xx 时书多半已落盘，通常当场认出；未真机验证）。
- **找回阅读位置**：粒度是"章节文件内比例"，同一文件里内容增删多时偏几页；页数变化、书里插过笔记页的情形没在真机上测；快照比较依赖设备时钟不倒退。
- **漫画页边距没有开关**：带标记的漫画首次打开一律设成 1；sheng-ren 改了标记值，已有的书要删了重新导入。书架 2026-10-07 前自己优化的漫画不带新标记，要用 sheng-ren 重新优化。
- **翻页方向**：书架不管；日漫在 xochitl 里从左往右翻。
- **inbox 只判"还在不在写"**：scp 中途断开留下的半截文件静止 5 秒后仍可能被收进母版库。
- **从 sheng-ren 复制的两处**没有自动检查（6.7）。
- **建文件夹代理的长轮询 290 秒**依赖 Qt 6.10 的 QML XHR 不设传输超时；journal 里出现 `SHELF-MKDIR: transfer timeout` 说明退回了 25 秒。
- **直接导入的多级文件夹**依赖 `Library.createCollection` 传非空上级，部署了但还没在真机上建过子文件夹（8.3 #24）。
- **旧设备残留**：`.pdf-originals/`、`books/done/` 等不会再被自动清理（5.3）。

### 7.2 按症状排错

| 症状 | 先看什么 | 原因 / 办法 |
|---|---|---|
| 传书就卡、传字体不卡 | 是否走 WiFi 热点 | xochitl 每导入一本就同步到云，占满弱热点上行 → 传书走 USB `https://10.11.99.1` |
| 大于 100MB 的书加不进去 | 回执末尾 | 应自动走大文件通道；回执"没有加入"＝超 1GiB 或造不出占位 |
| 「加入」一直显示处理中 | `journalctl -u book-serve` | 排在作业队列里，或在等认领（`/upload` 超时时最多 30 分钟）；重启 book-serve 后会记为失败 |
| 加入成功但渲染显示「未见渲染」（timeout） | 打开一次那本书 | xochitl 可能延后排版；10-10 起认不出新文档也记 timeout，到 xochitl 书库核对有没有这本 |
| 带斜杠的文件夹名 | — | 合法，按普通字符处理 |
| 选的文件夹建不出来、书落在根下 | journal 的 `SHELF-MKDIR`、网页页头横幅 | 代理 20 秒内没建出来；看代理是否加载（整机重启过没有） |
| 移进回收站没执行 | journal 的 `SHELF-TRASH` | `ids` 必须是 `entryForId(uuid).id`；交满 5 次会在页头横幅提示 |
| 漫画左右留白没变小 | 书里有没有 `META-INF/eink-reader-margins`；`GET /api/books/margins/<uuid>` | 只有 sheng-ren 标记的漫画会设；每本只设一次 |
| 替换后重开不在原来的位置 | journal 有没有 `CJ-KEEP-PROGRESS`；`books/progress/` 里快照还在不在 | 见 4.5 与 7.1 |
| 书内链接点了不跳 / 只渲染 1 页 / 缩进不对 / 目录入口没了 | — | 书本身的问题，归 sheng-ren（`docs/xochitl.md` 记了 xochitl 只认同文件锚点、只认外链 CSS、硬编码查 `id="ncx"` 等怪癖） |
| 字体删了菜单还在 / 中文是方框 | 字体菜单 qmd 是否最新、fontconfig | 历史附录 §03bd、§03k |
| 写了壁纸仍显示原图 | 是否整机重启过 | `SleepScreenPath` 只在 xochitl 启动时读，首次写键后整机重启一次 |
| WiFi 连上 60 秒就掉 / 不插 USB 连不上 | `iw reg get`、路由信道 | 锁 2.4G 或路由改 149+ 信道；不插 USB 会进深度休眠 |
| 内存冲高 / book-serve 被杀 | `NRestarts`、`/proc/<pid>/status` 的 VmHWM | 读 EPUB 已有单条目上限；先实测再定阈值 |
| 网关起不来、端口被占 | `systemctl` 看哪个单元占着 443 | 别凭进程名 kill；旧命名单元由安装器清理（历史附录 §03at） |
| 改了 qmd 不生效 | 整机重启过没有；journal 有没有 `Error while processing file tree` | 见 6.4；qmd 语法比 QML 窄（`({})`、裸 `if (` 会让整份不应用） |

### 7.3 设备与工程通用坑

| 坑 | 规避 |
|---|---|
| 磁盘 `.metadata` ≠ 界面实际状态（运行中的 xochitl 与云同步会还原） | 以界面为准；清测试文档走正常删除流程 |
| busybox 命令残缺（`head 5` 报错、无 `timeout`/`pkill`、中文名显示 `?`） | `head -n`；验中文名用 `find \| hexdump -C` |
| 设备上没法用 busybox 验 SSE（`wget` 缓冲到结束） | 电脑侧 `ssh -L` 隧道 + `curl -sN` |
| 备份留在 `extensions.d/` 会被 xovi 当扩展加载 → 崩溃循环 | 备份放 `~/cangjie-backups/` |
| 部署后验证的其实是旧二进制 | 核 `is-active`、`NRestarts`、起始时间；网关网页编译进二进制，改 JS 要重编译 |
| 清理时误删用户数据 | 只删自己建的唯一名文件，删前 `find` 核对；中文 URL 必须百分号编码（历史附录 §03h） |
| 改端口这类常量漏改 | `grep` 全仓库搜数字本身 |

## 8｜验证现状

> 这一节是书架线"部署到哪、真机验证到哪"的唯一清单。

### 8.1 部署批次

| 批次 | 部署 | 部署自检 | 功能手测 |
|---|---|---|---|
| 09-25 下午第四轮审计 | 09-25 `install-all` | 43✓ 0✗ | 未逐项（#17） |
| 09-30 三项减法、第五轮审计、KOReader 源码删除、大 PDF 有界读页数、长书名边车短名 | 09-30 14:10 / 15:23，各整机重启一次 | 38✓ / 36✓，1⚠（刚开机）0✗ | 未（#18、#19） |
| 10-07 书架不再优化书、稍后的清理、代码审查修复、傍晚的直接导入多级文件夹 | 10-07 13:02、15:54、约 16:32（后两次整机重启） | 37✓；36✓ 1⚠ 0✗ | 未（#22–#24） |
| 10-09 第六轮审计 | 10-09 13:48 | 38✓ 1⚠（刚开机） | 未逐项（#25） |
| 10-09 原地替换后找回阅读位置（与审计后续修补一起） | 10-09 15:53 | 39✓ 1⚠（刚开机） | 真机验证一次（#26） |
| **10-10 重构第一、二阶段**（投递层统一、认领交给基座、错误分种类、跨进程上传锁） | **未部署** | — | 只有开发机测试（#27、#28） |
| **10-10 安装 / 卸载脚本加固** | **未部署** | — | 只有本机模拟（`packaging/README.md`「没有真机验证」） |

10-10 的服务与网关**要一起部署**（网页依赖服务新下发的字段）；book-serve 的两份 qmd 也换了，部署后要整机重启一次。

### 8.2 真机验证过的

| 日期 | 事项 | 结论 |
|---|---|---|
| 09-03 | 字体上传 → 菜单 → 渲染 | 全程免重启 xochitl（固件 3.27） |
| 09-03 | 私有 CA | 装 CA 后访问 `shelf.local` 与 IP 都零告警 |
| 09-04 | 传书卡、传字体不卡 | 坐实是 xochitl 云同步占满热点上行 |
| 09-05 | 固件 3.27.3.0 → 3.28.0.172 | OTA 后重装约 8 分钟，数据零丢失 |
| 09-05 | `SleepScreenPath` 原生休眠屏；WiFi 锁 2.4G | 两次休眠对照轮换生效；锁 2.4G 后 40/40 ping 零掉线 |
| 09-06 | 退役 bind-mount 壁纸 | 休眠 3 次换 3 张池图 |
| 09-16 | 双网关单元并存 | 安装器清理旧单元后只剩新网关 |
| 09-19 | xochitl `/upload` 上限 | 二分测得 100,000,000 字节 |
| 09-19 | 落库流式上传 | 80MB 书 VmHWM 全程约 3.4MB |
| 09-19 | 建文件夹代理（`createCollection`）；带斜杠文件夹名 | 8 秒内建出、文档 `parent` 指向它；《乱马1/2》重试正常 |
| 09-19 | 字体菜单先删后加 | 上传→选中立即生效→删除→条目消失，全程未重启 |
| 09-20 | 大文件通道 | PDF 154MB、EPUB 153MB 加入可开 |
| 09-20 | book-serve 空闲耗电 | 夜间 0.2–0.5 秒 CPU/小时 |
| 09-24 | 建文件夹长轮询放宽到 290 秒 | 约 280 秒无 `transfer timeout` |
| 09-25 | 回收站代理重写（MainView + 长轮询 + `entryForId`） | 真机通 |
| 09-25 | 批量加入两本 >90MB《乱马》（156.5 / 157.1MB） | 网关队列 `done 2 / failed 0`，落进同一文件夹、字节与母版一致 |
| 09-25 | >153MB 占位首次渲染 | 156.5MB 首次打开约 74 秒排出 349 页，xochitl 主进程 VmHWM 410MB、没有重启 |
| 09-28 | wifi-watch 省电 | 2.4G 空闲 20 分钟：关 168 mA → 开 106 mA，零掉线 |
| 10-09 | sheng-ren 直接导入批量查询（`POST /import/states`） | sheng-ren 同步在真机上跑通 |
| 10-09 | 原地替换后找回阅读位置 | 《绍宋》替换前读到第 15 页，重开约 2 秒后跳回第 15 页、快照销账；新 `.epubindex` 打开当场写出，`.content` 是 v2 带 `redir`。那次替换前后该章排版没变 |

### 8.3 待手测清单

编号沿用原 §05，别处引用"§05 #N"指的就是这里的 #N。

| # | 事项 | 现状与缺口 |
|---|---|---|
| 2 | 网页 i18n / 触屏 / 弹窗人眼确认 | 只验了数据链路；09-25 无头浏览器走查不算人眼 |
| 8 | 设备上的旧版重复副本 | 「管理 → 设备健康」底部有清理入口（`books/done/` 逐个勾选删；xochitl 书库同名多份走回收站软删，不自动识别旧拆分卷）；清理入口本身未真机点验 |
| 10 | 原件下载的内存峰值 | 两端都流式，没量 VmHWM |
| 12 | 09-24 第三轮审计 | 已部署、服务健康；并发锁与内存上限靠的场景没在真机专门触发 |
| 17 | 09-25 第四轮审计 | 已部署（43✓）。没专门看过：scp 大书进 inbox 不出半截书；书架服务停掉时两个代理不再每 15 秒重试；带 `&` 的书名不再显示 `&amp;` |
| 18 | 09-30 三项改动 | 超限书走大文件通道，超 1GiB / 造不出占位时整本拒绝、回执干净 |
| 19 | 09-30 第五轮审计与补修 | 长书名能入库、加入；母版库目录不留 `.*.tmp`；「全部中止」在一本刚开始时确实不跑；网关重启后网页自动恢复实时刷新；只加入过 KOReader 的旧书回到「未加入」；带 Basic 头的脚本连发请求时网关 CPU 不持续满载；>90MB 第三方 PDF（交叉引用流）加入后页数正确；书名 >244 字节的书在网页上显示加入状态 |
| 22 | 10-07 书架不再优化书（已部署 37✓） | 「传书」只剩上传卡、底部只剩「加入 xochitl」、筛选三档默认「未加入」；旧 `batch.json` 里的 optimize 任务被剔除；设备上优化过的书照样能加入；sheng-ren 漫画首次打开设页边距 1、其它书不设；sheng-ren 产的 >90MB 书走大文件通道；最小占位 PDF 让 >90MB 的 PDF 照常加入；已删接口回 404；补跑浏览器冒烟 |
| 23 | 10-07 稍后的清理与代码审查修复（已部署 36✓） | 行内没有按钮；渲染徽章只显示页数；格式筛选只有 EPUB / PDF；批量多本一本一本跑、网关重启后续跑；占位显示书里的书名；「系统增强」只剩「单击翻页」；日漫从左往右翻（预期）；`/reading-direction`、`/staging/mark`、`/api/budget/*` 回 404；筛选计数加得拢；结果不明记失败；加入后马上改名显示「未见渲染」；原地替换后书库没有 `.epub.new`、页边距没被重设 |
| 24 | 10-07 傍晚直接导入多级文件夹（已部署 36✓） | sheng-ren 推 `folder=一级/二级` 的书，书库出现「一级」里套「二级」（关键是 `createCollection` 传非空上级，从没在真机上走过）；「一级」已存在时不重复建；不同上级下的同名子文件夹分开；journal 无 `SHELF-MKDIR` 报错 |
| 25 | 10-09 第六轮审计（已部署 38✓） | 坏 EPUB 入库、加入时 book-serve 不被 OOM 杀；xochitl 正在改写 `.metadata` 时回收站待办不丢；「新建文件夹」加入后不再白等 20 秒；投到刚删的文件夹落书库根；剩余 <300MiB 网页标红；>90MB 的书照常；`shelf-uninstall --only book` 删 qmd 后提示整机重启；`.reason` 正常 |
| 26 | 10-09 找回阅读位置（已部署 39✓，真机验证一次） | 还要测：sheng-ren 改动内容、页数变了之后重开是否落在同一章附近；书里插过笔记页时跳到的页对不对；新 `.content` 的 `pageCount` 是否与新 `.epubindex` 同时更新；没替换过的书打开时 journal 没有 `CJ-KEEP-PROGRESS` |
| 27 | **10-10 投递层统一（未部署）** | 部署后：选根下已有 / 不存在的文件夹各加入一本，书落根下那个文件夹；书库已有「X/卷01」而根下没有「卷01」时选"卷01"，根下新建「卷01」；带标记的漫画页边距登记在它自己的 uuid 上；连续加入几本时一本一本处理、排着的显示处理中；sheng-ren 直接导入与网页加入同时进行都成功；回收站入队后很快进回收站；建文件夹代理照常（含子文件夹） |
| 28 | **10-10 迁到基座新接口（未部署）** | 部署后：加入一本 EPUB，几秒内渲染状态出现页数、边车里有 uuid；带标记的漫画 `GET /api/books/margins/<uuid>` 有登记、首次打开页边距变成 1；PDF 照常、没有渲染徽章；note-serve「推送本章」与网页加入同时进行时各落各的文件夹（锁文件 `/tmp/shelf-0/shelf/xochitl-upload.lock` 存在）；删除处理中的书提示"正在处理中"（409）；sheng-ren 直接导入照常（阶段显示「上传给 xochitl、等它排版」） |

### 8.4 已关闭 / 作废

编号保留，便于别处引用：

- **#1** 图片密集网文大片留白：真成因是分页引擎，无已知修法；抓网文 10-07 删除，作废。
- **#3** 「书籍依然被锁字体」：没有样本，关闭。
- **#4** 批量加入：✅ 09-25 真机通过（8.2）。
- **#5** >153MB 占位通道首次渲染：✅ 09-25 真机通过（8.2）。
- **#6** PDF 转 EPUB 公式裁图、竖排 / 多栏：作废（PDF 转换 10-07 删除）。
- **#7** 书内目录页跨文件链接：决定不做（xochitl 只认同文件锚点；归 sheng-ren）。
- **#9** KOReader 运行中拒写配置：作废（KOReader 已卸载）。
- **#11** `pdf-extract-cj` 嵌套表单防护：作废（10-07 删除）。
- **#13** 并发控制缺口（抓网文同步优化、网关闸门）：两样都已删除，作废。
- **#14** 建文件夹代理长轮询：✅ 09-24 真机验证（8.2）。
- **#15** 按书设阅读方向：作废（09-30 移除）。
- **#16** calibre 书的空 `<a id>` 注释目标：转给 sheng-ren。
- **#20** 优化规则 v16：作废（设备端优化删除）。
- **#21** 换成 sheng-ren 的 bookconv：作废（没部署就被 §03bw 取代）。

## 9｜附录

### 9.1 设计决策要点（为什么是现在这样）

| 决策 | 为什么 | 详见历史附录 |
|---|---|---|
| 网关 + 只听本机端口的领域服务 + 运行时注册表 | 装 / 卸一个服务只动一个二进制与一个单元；不做单体 | §01 |
| 母版库居中，入库与加入分开 | 一次只做一个决定；母版可反复加入、换设备重投 | §03r |
| 只收 EPUB / PDF | 用户的策略收紧，不是技术判断 | §03s、第九部分 |
| 书架不优化书，交给 sheng-ren | 优化规则一处维护；设备上的内存事故整类消失；book-serve 少了最复杂的代码 | §03bw |
| 不依赖 sheng-ren 的代码，复制两处常量 | 少 95 个依赖；代价是两边会漂 | §03bx |
| 要 xochitl 动手的事都走 qmd 代理 | 外部改书库文件会被运行中的 xochitl 盖回去 | §03aa、§03be、§03bk |
| 超 100MB 走占位 + 磁盘替换，不再按卷拆分 | xochitl 上传硬上限；拆分会丢页、冲垮渲染 | §03bn、§03ax |
| 网页零轮询，靠事件推送 | 空闲零唤醒，省电 | §03z |
| 批量队列放网关 | 网关是唯一的转发关口 | §03bp |
| 投递层统一、按字节认书、认不出就不认 | 两套判据曾导致认错书、落错文件夹 | §03cb、§03cc |
| 改动生效一律整机重启 | 单独重启 xochitl 有概率崩溃 | 第十部分、§03f |
| 忙锁不落盘 | 落盘会"永久卡忙"；重启 ＝ 没有操作在跑 | §03bq |
| 内存阈值先实测再定 | 估算曾差 3–5 倍 | §03bi |

### 9.2 已移除的能力

| 能力 | 移除时间 | 现在怎么办 |
|---|---|---|
| 设备上的「优化」、入库 PDF 转换（有文字层转 EPUB、无文字层裁边）、原 PDF 7 天备份、抓网文、联网补封面、旧产物兼容、中途取消、优化徽章与筛选 | 2026-10-07 | 书在电脑上用 sheng-ren 优化好再传；网页链接也用 sheng-ren 收书；PDF 原样加入 |
| 网关并发闸门与行内"取消排队"、渲染自检的期望页数与 `warn` | 2026-10-07 稍后 | 网页加入本来就一本一本来；渲染好坏由 sheng-ren 的质量门把关 |
| 日漫翻页（按书里的方向标记对调滑动方向） | 2026-10-07 稍后 | xochitl 里日漫一律从左往右翻；单击翻页保留 |
| 按卷拆分投递（EPUB 按目录、PDF 按书签） | 2026-09-30 | 超限走大文件通道，超 1GiB 整本拒收 |
| 母版库按书设阅读方向 | 2026-09-30 | 书架不管方向 |
| 漫画 EPUB→PDF 转换器（`comic_pdf.rs`） | 09-20 停用，09-30 删除 | 漫画由 sheng-ren 出 EPUB |
| KOReader 一切（加入 KOReader、字体/词典/配置同步、高亮与生词回流）与 koreader-serve | 2026-09-29（源码 09-30 删除） | 只用 xochitl；旧设备上的单元与二进制由安装器清掉 |
| 电脑端命令行 `shelf`（push / Calibre 管线 / doctor 等） | 2026-09-18 | 没有网页替代；其它格式先在电脑上转好 |
| `/inbox*` 与 `/staging/render/*` HTTP 接口 | 2026-09-22 | scp 进 `inbox/` 仍可用，失败看 `failed/*.reason` |
| bind-mount 壁纸 | 2026-09-06 | 写 `SleepScreenPath` |
| 微信读书内容源 | 2026-09-05 | 不做 |

本仓库原有的《EPUB 优化规范白皮书》《bookconv 优化白皮书》2026-10-07 删除（规则在 sheng-ren）；要看原文：`git log --oneline -- shelf/docs/EPUB优化规范白皮书.md` 找到删除它的提交，再 `git show <该提交>^:shelf/docs/EPUB优化规范白皮书.md`。

### 9.3 旧章节号对照表

2026-10-10 前本文按时间编号。下表覆盖旧版本文与历史附录里出现过的每一个编号；"现状"指本文的节，"历史"指[历史附录](reMarkable书架白皮书-历史附录.md)里同号的节（历史附录各节标题保留原编号）。

**按章、按附录**

| 旧号 | 原标题 | 现状在本文 | 历史在附录 |
|---|---|---|---|
| 「给新读者」 | 5 分钟读懂 | 1、2 | — |
| 「现状总览」 | 当前有效的关键规则、部署状态 | 2–7；部署状态 8.1 | — |
| 第 0 章 | 定位、原则与基础 | 1、2.2、6.6 | 二 |
| 第 A 章 | 入库与母版库 | 3.2、3.3、4.1 | 三 |
| 第 B 章 | 书架不再优化书 | 1、9.2 | 四 |
| 第 C 章 | 落库、大文件、xochitl 代理、找回阅读位置 | 4.2–4.5 | 五 |
| 第 D 章 | 网关与网页 UI | 3.1、3.6、4.7 | 六 |
| 第 E 章 | 稳定性、内存与耗电 | 4.6、4.8 | 七 |
| 第 F 章 | 设备、字体壁纸与固件 | 3.5、4.9、6.4、6.5 | 八 |
| 第 F 章「速查」 | 怎么让 qmd / 扩展改动生效 | 6.4 | — |
| 各章「坑位表」 | — | 7.2、7.3 | 各章开头的历史坑位表 |
| 各章「已挪走的节」 | — | 本表 | — |
| 附录 A | 演进记录表 | — | 一 |
| 附录 B | 已移除的能力（电脑端命令行等） | 9.2 | 九 |
| 附录 C | 目录注解与设备路径表 | 5.3、6.1 | — |
| 附录 D | 旧版 OTA 恢复 | 6.5 | 十 |

**§00 – §05**

| 旧号 | 原标题（简） | 现状在本文 | 历史在附录 |
|---|---|---|---|
| §00 | 定位与原则 | 1、6.6 | §00 |
| §00b | 现状总览（09-10 版） | 2 | §00b |
| §01 | 架构决策 | 2.2、9.1 | §01 |
| §02 | XDG 路径表 | 5.3 | §02 |
| §03 | systemd | 2.2、6.3 | §03 |
| §03b | Phase 1 统一投递 | 3.2 | §03b |
| §03c | 字体/壁纸上传即可用 | 3.5、4.9 | §03c |
| §03d | KOReader 配置即代码（已退役） | 9.2 | §03d |
| §03e | 原生高质量门（已删） | — | §03e |
| §03f | 真机首轮（3.27） | 6.4 | §03f |
| §03g | 用户反馈补齐（内建字体） | — | §03g |
| §03h | HTTPS + 密码 · 误删用户目录事故 | 7.3 | §03h |
| §03i | host 优化 vs 设备端优化（已删） | — | §03i |
| §03j | 登录页密码 · 私有 CA · mDNS | 3.1 | §03j |
| §03k | 字体两 bug（方框、选了不生效） | 4.9、7.2 | §03k |
| §03l | 传书卡＝云同步 | 3.1、7.2 | §03l |
| §03m | 网页改版 · qmldiff 离线验证法 | 6.6 | §03m |
| §03n | 细节调整 | — | §03n |
| §03o | xovi 持久化归位 · 管理台 | 6.3、6.4 | §03o |
| §03p | 代码质量核查一轮（并入 §03ab） | 6.6 | §03ab |
| §03q | 书籍优化深层优化（已删） | — | §03q |
| §03r | 母版库三层架构 | 4.1、9.1 | §03r |
| §03s | 母版库领域化 · 直投路删除 | 6.6 | §03s |
| §03t | 漫画通道 CBZ（已删） | — | §03t |
| §03u | 砍微读线 | 9.2 | §03u |
| §03v | 固件升级到 3.28 实录 | 6.5 | §03v |
| §03w | `SleepScreenPath` · WiFi 60 秒掉线 | 3.5、4.9、7.2 | §03w |
| §03x | 退役 bind-mount 壁纸 | 3.5 | §03x |
| §03y | xochitl CSS 引擎实测规则 | 7.2（归 sheng-ren `docs/xochitl.md`） | §03y |
| §03z | 事件推送零轮询 | 4.7 | §03z |
| §03aa | 渲染自检 · 回收站 / 建文件夹代理 | 4.2、4.4 | §03aa |
| §03ab | 代码结构重构记录 | 6.6 | §03ab |
| §03ac | 可插拔接住笔记线 | 2.2 | §03ac |
| §03ad | 漫画跨页拆分（已删） | — | §03ad |
| §03ae | 网页 i18n 架子（并入 §03an） | 3.6 | §03an |
| §03af | 网页触屏小修（并入 §03an） | 3.6 | §03an |
| §03ag | `PendingQueue<T>`（并入 §03ab） | 4.4 | §03ab |
| §03ah | 网关 proxy 流式应答 | 4.7 | §03ah |
| §03ai | `SvcClient`（并入 §03ab） | — | §03ab |
| §03aj | 「管理」页与首层标签 | 3.6 | §03aj |
| §03ak | 实验室 · 导入 md（并入 §03aj） | 3.6 | §03aj |
| §03al | 首层标签重排（并入 §03aj） | 3.6 | §03aj |
| §03am | 总标题 · 端口 443（并入 §03aj） | 2.2 | §03aj |
| §03an | 网页 i18n 与触屏 | 3.6 | §03an |
| §03ao | `shelf push --no-calibre`（CLI 已砍） | 9.2 | 第九部分 |
| §03ap | 抓网文同步优化（已删） | 9.2 | §03ap |
| §03aq | 网文留白证伪（已删） | — | §03aq |
| §03ar | koreader-serve 高亮/生词端点（已退役） | 9.2 | §03ar |
| §03as | `notes pull` 配置（CLI 已砍） | 9.2 | 第九部分 |
| §03at | 两个网关单元并存 | 7.2 | §03at |
| §03au | 状态提示被 SSE 抢跑重画 | — | §03au |
| §03av | 拆 EPUB 线 / PDF 线（已删） | — | §03av |
| §03aw | 异步优化 · 防双击（已删） | — | §03aw |
| §03ax | 脚注返回 · 超限按卷拆分（拆分已删） | 4.3 | §03ax |
| §03ay | 落库改异步 · 《疯探》目录页 | 4.2 | §03ay |
| §03az | 《疯探》dtb:uid（已证伪） | — | §03az |
| §03ba | 真机内存危机：流式优化 | 4.8 | §03ba |
| §03bb | 《疯探》二分排查 | — | §03bb |
| §03bc | xochitl 硬编码查 `id="ncx"` | 7.2（归 sheng-ren） | §03bc |
| §03bd | 字体菜单只增不删 | 4.9、7.2 | §03bd |
| §03be | 进度不刷新 · 文件夹不建 | 4.4 | §03be |
| §03bf | 带斜杠的文件夹名 | 4.2、7.2 | §03bf |
| §03bg | 优化分步进度（已删） | — | §03bg |
| §03bh | 落库路径叠 3 份数据 | 4.8 | §03bh |
| §03bi | 像素上限：内存阈值要实测 | 4.8 | §03bi |
| §03bj | 自定义确认弹窗 | — | §03bj |
| §03bk | 漫画转 PDF（已删）· 页边距必须 xochitl 自己设 | 4.4 | §03bk |
| §03bl | 闸门分支合并（闸门已删） | — | §03bl |
| §03bm | 耗电与日志核查 | 4.8 | §03bm |
| §03bn | 大文件占位 + 替换 · 命名规则 | 3.2、4.3 | §03bn |
| §03bo | 封面声明规则（已删） | — | §03bo |
| §03bp | 批量队列 · 闸门 · 母版库页重做 | 3.3、4.7 | §03bp |
| §03bq | panic=unwind · OpRegistry · 启动恢复 | 4.1、4.8 | §03bq |
| §03br | 入库 PDF 转换（已删） | 9.2 | §03br |
| §03bs | EPUB 优化线真机修复（已删） | — | §03bs |
| §03bt | KOReader 两套方案（已退役） | — | §03bt |
| §03bu | 第五轮审计 | 8.3 #19 | §03bu |
| §03bv | 换 sheng-ren 引擎（未部署，已取代） | — | §03bv |
| §03bw | 书架不再优化书 | 1、9.1、9.2 | §03bw |
| §03bx | 不再优化后的清理（含删日漫翻页、复制两处） | 6.7、9.2 | §03bx |
| §03by | 代码审查修复 | 4.1 | §03by |
| §03bz | 第六轮审计 | 4.8、8.3 #25 | §03bz |
| §03ca | 原地替换后找回阅读位置 | 3.4、4.5、8.2 | §03ca |
| §03cb | 投递层统一（10-10 第一阶段） | 4.2、4.6、8.3 #27 | §03cb |
| §03cc | 迁到基座新接口（10-10 第二阶段 2a） | 4.1、4.2、5.1、8.3 #28 | §03cc |
| §04 | 踩坑（通用坑、按症状查章） | 7.2、7.3 | — |
| §05 | 真机待办 | 8 | — |
| §05 #N | 待办第 N 项 | 8.3（未完成）/ 8.4（已关闭、作废），编号不变 | — |
