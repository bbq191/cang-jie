# 传书模块 EPUB 线架构文档

> 这是一份**当前状态参考文档**，不是按时间顺序记录"做过什么"的会话日志——那类历史叙事在
> `reMarkable书架白皮书.md`（书架整体，含每一轮真机反馈的排查过程）和 `bookconv优化白皮书.md`
> （bookconv 内部技术细节的演进史）里，两份加起来四十多万字。本文档只回答一个问题：**传书模块的
> EPUB 线现在长什么样**，给需要快速建立/刷新心智模型的场景用（自己接手、给别人讲、隔一段时间
> 回来忘了细节）。写作时点：2026-09-19，随代码演进本文档需要跟着更新，别当成一次性写完就不变的
> 东西。

## 0｜这是什么

"传书"是书架（shelf）项目里"把书从各种来源弄到设备上、并且弄得质量过得去"这件事的统称。EPUB 线是
传书模块里专门处理 EPUB 格式的完整链路——EPUB 是唯一会被**深度处理**（清洗排版、目录重建、图片
降采样、漫画识别与拆分）的格式；PDF 走同一套入库/落库流程，但只原样透传，不进 `bookconv` 的任何
处理管线。

**核心设计哲学**：三层架构（内容源 → 母版库 → 读器），三个正交动作（入库 / 优化 / 落库）。"正交"
指这三个动作互不耦合——入库不需要优化，优化后可以选择不落库，落库前不强制要求先优化。母版库是
唯一的书籍容器，网页没有任何"跳过母版库直接投读器"的路径。

![传书模块 EPUB 线：系统架构](diagrams/epub-line-architecture.svg)

## 1｜整体架构

四个进程参与，边界清楚：

| 组件 | 角色 | 端口/连接方式 |
|---|---|---|
| `gateway` | 唯一 Web 前端，托管网页 UI + 反向代理 | `:443`（外部），登录墙保护 |
| `book-serve` | 母版库领域服务：入库/优化/落库 | `:8790`（loopback，只经网关代理访问） |
| `bookconv` | **库，不是服务**，被 `book-serve` 进程内直接调用 | 无网络面 |
| `koreader-serve` | 落 KOReader 需要的领域服务 | `:8791`（loopback） |

**`bookconv` 是进程内库，不是独立服务**——这是一个明确评估过、否决过拆分的架构决策：`bookconv`
已经是独立 crate，库边界本来就干净；拆成独立 HTTP 服务唯一实质好处是进程级隔离，代价是大文件
（几百 MB EPUB）要跨进程序列化，峰值内存可能不降反升；这台设备的 cgroup `MemoryMax` 从未真正生效
（systemd 没把 memory 控制器代理进 `system.slice` 子树），进程隔离想要的内存兜底本来就是假的，拆
服务换来的隔离收益进一步打折。

**网关代理**（`gateway/src/proxy.rs`）：`/api/{svc}/*` 按 URL 段查服务注册表转发到对应 loopback
端口，`/api/books/*` → `book-serve:8790`。**请求体是真流式转发**（大文件上传不额外占网关内存）；
**响应体目前整体缓冲进内存再回发**——代码里有一处注释与实现不一致的已知偏差（注释写"流式"，实现
是 `read_to_end`），风险/收益比不高，没有动它，这里如实记录不是漏掉。

**`book-serve` → `xochitl` 是直连，不经过网关**——`Staging::deliver()` 内部直接用
`rmsvc_core::xochitl::Xochitl` 连 `10.11.99.1`（设备 USB 网口），跟"浏览器 → book-serve"这条走
网关代理的链路是完全独立的两条边，画数据流图容易把它们混成一条，要分清楚。

## 2｜三层数据流

![传书 EPUB 线：三层 · 三动作正交](diagrams/epub-line-dataflow.svg)

### 2.1 入库（内容源 → 母版库）

两条入口：
- **网页上传**：`uploader()`（`gateway/ui/app.js`）逐文件 `XMLHttpRequest` + `upload.onprogress`
  真实字节进度；母版库上传口额外传 `dedupeApi` 参数，先查一次现有条目按 `name|bytes` 去重，避免
  同名同大小重复上传落成 `xxx_1.epub`。服务端 `rmsvc_core::multipart` 流式解析、边读边落盘，
  不会因为大文件把整个请求体读进内存。
- **抓网文**：`Staging::fetch_article`，Readability 提取正文后现场用 `bookconv::epub` 组装一本
  最小合规 EPUB3，可选"同步优化"。

原样入库，不做任何格式转换——书架已经不支持除 EPUB/PDF 外的其它格式（AZW3/MOBI/CBZ 等格式转换
连带 host 端 CLI 一并在更早的一轮改动里退役）。

### 2.2 母版库（中间层暂存池）

目录：`$XDG_STATE_HOME/shelf/books/`，下面几个子目录各司其职：
- `staging/`：母版库本体，条目**永久保留**，落库不会自动删除（用户可以反复投给两个读器对照、
  换设备重投，要删由用户自己在列表里点删除）。
- `inbox/`/`.work/`/`failed/`：`spool.rs` 管理的三段式追平队列（历史上给某条已退役的入口用，
  现状：inbox 待处理→`.work` 已认领处理中（也兼上传暂存目录）→成功进 `staging/`、失败进
  `failed/`）。

`Staging` 结构体（`book-serve/src/staging.rs`）核心字段：
- `dir`：母版库目录
- `xochitl: Arc<Xochitl>`：连设备的客户端
- `native_limit: u64`：投原生体积门（字节，见 §5）
- `busy: Arc<Mutex<HashSet<String>>>`：**进程内存态**忙锁，不落盘

**忙锁 vs sidecar 状态：两套独立职责**——`busy` 回答"现在是否有线程正在跑"，进程重启即清零（有意
设计，不是 bug：重启后没有任何操作真的还在跑，"忙"状态天然该清零，比落盘更简单也不会出现"重启后
永久卡忙、谁都清不掉"的死锁）；sidecar 里 `OptimizeCheck`/`DeliverCheck` 的 `status` 回答"上次
异步操作跑到哪一步/结果如何"，两者不混在一起判断。同一条目「优化」「落库」互斥（都要读/写同一份
母版库文件），互斥判据是同一把 `busy` 锁。

**sidecar**：`.<name>.delivered`（隐藏文件，跟母版库文件同目录），JSON 结构（`sidecar.rs`）：

```
Delivered {
  optimize: Option<OptimizeCheck { status, message, at, progress: Option<StepProgress{done,total}> }>,
  deliver:  Option<DeliverCheck  { status, message, at, progress: Option<StepProgress{done,total}> }>,
  native:   Option<u64>,   // 最近一次投原生的时间戳
  koreader: Option<u64>,   // 最近一次加入 KOReader 的时间戳
  render:   Option<RenderCheck>,  // 渲染自检结果，见 §2.3
  source:   Option<SourceRef>,
}
```

`OptimizeCheck`/`DeliverCheck` 字段形状故意一致但**不合并成一个泛型结构**——注释里明写不想靠字段
名去影射区分两种状态，语义上"优化"和"落库"是两件独立的事。`StepProgress{done,total}` 是两者共用
的进度形状。

### 2.3 落库（母版库 → 读器）

两个读器互不依赖，同一份母版字节可以分别投给两边对照：

- **加入 xochitl（原生）**：`Staging::deliver()`，纯复制字节（不再优化）。`folder` 留空＝书库根
  （2026-09-19 起——之前会落进配置里的默认文件夹，已去掉这条隐藏行为，跟 KOReader 那边"留空＝
  根目录"的语义对齐）；文件夹不存在会经 `MkdirQueue` 排队等设备端 QML 代理建出来（见 §4）。
  超过体积门（默认 90MB）触发漫画按卷拆分（见 §6）。xochitl 只读 EPUB/PDF，CBZ 不投原生。
  投完 EPUB 会另起一条**渲染自检**线程（`render_check::run`，不阻塞落库请求）：xochitl 导入时
  会同步渲染出页数，自检线程限时（最长 10 分钟）轮询书库目录，等到页数后跟优化时统计的"期望
  页数"比——低于一半判 `warn`（真机标定：好书页数比 0.86-0.99，整章渲染失败的坏书能低到 0.34，
  见 `render_check::WARN_RATIO`），结果写回 sidecar 的 `render` 字段+推 SSE 事件。`/upload`
  接口不直接回 uuid、书名也可能跟文件名不一致，认书靠"投书时刻之后新出现的文档 + visibleName
  跟书名/文件名相符者优先，否则取最新一本"这套启发式（`render_check::pick`）。
- **加入 KOReader**：`koreader-serve` 的 `adopt`，本地同分区文件拷贝，不经过 `bookconv`。这个
  操作没有服务端忙态可查（跟 book-serve 是两个独立进程），网页补了一个纯前端的 `localBusy` 集合
  凑齐"点了有反应"的体验，不是真的服务端异步状态。

两个操作在 HTTP 层都是**异步**的（`spawn_optimize`/`spawn_deliver`）：请求立即回"已开始"，真正
结果通过 sidecar 状态 + SSE 事件呈现，不是让 HTTP 请求挂着等。`spawn_bg`（`staging.rs`，
2026-09-19 代码质量审计新增）是两者共用的"起线程 + `catch_unwind` 兜底 + 解忙锁 +
`bus.publish`"外壳，业务内容（调 `optimize`/`deliver`、写哪个 sidecar 类型）留在各自闭包里不
下沉。

## 3｜EPUB 优化管线

![EPUB 优化：两阶段流式管线](diagrams/epub-optimize-pipeline.svg)

`bookconv::optimize` 有两个入口，业务逻辑共用（`first_pass_html`/`transform_html_chapter`/
`transform_image_bytes` 三个函数），产出逐字节对拍一致：

- **`optimize_epub_with`**（内存版）：整本读入、一次性产出。给测试/CLI 小书场景用，签名不变。
- **`optimize_epub_file_streaming`**（流式版，`book-serve` 生产入口）：两阶段设计，2026-09-19
  因真实 552MB 书触发 `VmRSS` 冲 1.4GB+ 的真机 OOM 事故而生。**阶段一**（规划，轻量）：逐条目
  遍历，非图片条目（html/css/opf/ncx）整读进内存，图片条目留空占位；`wash_entries` 十步清洗、
  漫画判定、TOC 处理都只看文字/`<img>` 标签引用，占位不影响判断。**阶段二**（真正耗内存的部分）：
  逐图片流式处理，每张图独立作用域——按需重开文件 seek 取真实字节 → 像素上限 guard（见下）→
  裁边/降采样 → 直接写进输出 zip → 出循环即释放。峰值内存＝"一张图 + 全书文字部分"，不随书体积
  线性涨。

幂等标记：产物埋 `META-INF/com.cangjie.optimized`，内容是 `OPTIMIZE_VERSION`（当前 **14**），
区分 `"14"`（完整优化含清洗）/`"14-core"`（只跑核心遍无清洗）。版本号每次改动优化行为都要涨，
靠它判断"这本书是不是已经用当前逻辑优化过，不用重来"。

### 3.1 `wash_entries`：十步清洗（`bookconv::wash`，按代码顺序）

1. `strip_pseudo_drm`——剥伪 DRM，真 DRM 报错拦截
2. `remove_empty_pages`——清空页
3. 语言探测（`LangMode::Auto` 时按字符占比判 CJK/Latin）
4. 逐 html `wash_html`（排版：CJK 首行缩进 2em / Latin 1.2em，标题后首段不缩进）+
   `inject_css_link`；逐 css `filter_css`（剥 `DEFAULT_FILTER_PROPS`＝
   `font-family/font-size/font/background-image/background`，**不剥 color/background-color/
   text-align**——2026-09-17 起明确保留原书颜色/加粗，只解锁字号不解锁配色）
5. 写外链 `cangjie-wash.css`（**xochitl 只认外链 css，完全无视内联 `<style>`**——这条是排版
   规则必须写外链样式表的根本原因）
6. `fix_ncx_manifest_id`——NCX 在 manifest 里的 `id` 必须叫 `"ncx"`，xochitl 定位目录硬编码死查
   这个字符串字面量（反编译 xochitl 二进制坐实，不是走 EPUB 规范的 `<spine toc="IDREF">`）
7. `restructure_existing_toc_parts`——检测"第 X 部 编号 章名"这类扁平目录惯例，重建成两级
8. `auto_toc`——缺目录才建（`AutoToc::Off/IfMissing/Always`），按 h1-h6 收集标题，为空则按
   spine 兜底；多数页无文本则放弃（避免误伤漫画）；数字≤99 的"标题+编号"拆两级
9. `fix_ncx_uid`——NCX 的 `dtb:uid` 同步 OPF `dc:identifier`，不一致时目录面板整个不显示入口
10. `strip_ncx_doctype`——剥外部 DTD 引用，避免联网取 DTD 卡住

### 3.2 脚注

`FootnoteMode` 现在只剩两档：`Anchor`（默认，注释移到章末 + 锚点跳转 + 原生返回浮标延时 20s）、
`Inline`（就地内联、不跳转）。历史上的 `ParagraphEnd` 已经整个删除（真机反馈驱动改了两轮又撤回）。

### 3.3 图片处理（`bookconv::imgopt`）

三个解码入口（`downscale_into_q`/`trim_margins`/`dither_bilevel`）统一有像素上限 guard：
**`MAX_DECODE_PIXELS = 9,000,000`**（约 3000×3000）——2026-09-19 校准过一次的取值：第一版按
"3 字节/像素 RGB8"理论估算给了 2500 万，真机撞到两次 230-270MB 级内存尖峰（`image` 库内部
解码+`to_rgb8()`+resize 中间缓冲多份同时存活，实测开销是理论估算的 3-5 倍），改用真机实测数据
重新定的阈值，超限图直接原样保留不处理。

- `downscale_for_epub`：EPUB 内嵌图，竖向框 954×1696（宽绝不超 954，防行内横幅溢出竖屏）
- `downscale_for_device`：CBZ 整页，朝向框（横图 1696×954 / 竖图 954×1696）
- `downscale_for_epub_comic`：漫画专用，quality 95 不允许压画质（EPUB 线原则④）
- `trim_margins`：四边纯色/近纯色留白裁边，容差 8，单边最多裁 15% 防误判裁没内容
- `dither_bilevel`：**不在 EPUB 优化管线里**，只被 CBZ→Mono 省刷新路径调用，是完全独立的一条线

漫画判定：`comic_detect::is_comic`——图 ≥20 张（`MIN_IMAGES`）且平均每图配的可见文字 <40 字
（`TEXT_PER_IMAGE`）。

### 3.4 质量门（`bookconv::check`）

硬失败（拦下落库/推送）：真 DRM、目录 href 命中率 <80%、单标签双 `id` 属性（非法 XHTML，xochitl
严格 XML 解析会整章白屏）。告警（不拦）：无 TOC、锚点丢失。

## 4｜设备端代理队列：为什么不能直接建文件夹/删文档

外部进程不能直接写 xochitl 的 `.metadata`——运行中的 xochitl 会把改动覆写回来。唯一合法路径是
xochitl 自己的 QML 代码（`Library.createCollection`/`selectionMoveToTrash`）。`book-serve` 因此
维护两个代理队列（`mkdir.rs`/`trash.rs`，共用同一套 `pending_queue::PendingQueue<T>` 持久化+
去重+剔除基础设施）：入队一个"要建的文件夹名"/"要删的文档 uuid+name"，设备端注入的
`shelf-mkdir-agent.qmd`/`shelf-trash-agent.qmd`（8 秒一次 Timer 轮询）真正拉队列执行。
`ensure_folder`（`staging.rs`）落库前会同步等最多 20 秒让文件夹真的建出来，等不到不算错误，退回
`Xochitl::upload_file` 自带的"找不到就落根"兜底。

## 5｜内存安全设计

这是 EPUB 线里被真机事故驱动最多的一块，值得单独拎出来看全貌而不是散在各处：

| 风险点 | 修复前 | 修复方式 | 真机验证 |
|---|---|---|---|
| 整本优化（旧版） | 552MB 书 `VmRSS` 冲 1.4GB+ | 两阶段流式（§3），峰值＝一张图+全书文字 | 同一本书优化成功，`VmRSS` 全程 8-53MB |
| 超限漫画拆分投递（旧版） | 785MB 书把 book-serve 逼近系统内存上限 | `comic_split::deliver_split_streaming`（§6），峰值≈单份体积 | 11 卷《镖人》真机全部投递成功 |
| 落库不拆分路径 | ≤90MB 书仍叠 3 份数据（整本读+自检整本解压+上传内部克隆），峰值 ~180-270MB | `rmsvc_core::xochitl::upload_file` 流式上传 + `bookconv::stats::text_profile_file` 流式自检（跳过图片） | 80MB 测试书投递，`VmHWM` 全程 3.2-3.5KB |
| 单图解码无像素上限 | 真实漫画页解码成未压缩位图，`VmHWM` 冲 262-271MB | `imgopt::MAX_DECODE_PIXELS`（§3.3），实测数据校准（不是理论估算） | 25 页测试漫画一页故意 1600 万像素，`VmHWM` 全程个位数 MB |

**`native_upload_limit_mb` 默认 90MB**：xochitl `/upload` 有真实硬上限（真机 `curl` 二分法精确
测出边界＝整数 100,000,000 字节，此前配置的 150 是从未验证过的猜测值），90MB 留够安全余量，超过
就走漫画拆分或直接拒绝，不发（会直接断连）。

**验证方法论**：`VmHWM`（`/proc/<pid>/status`，进程生涯内存峰值，比瞬时轮询采样更权威）是这几轮
排查共用的验证手段——真机合成接近临界值的测试文件、真实走一遍完整 API 请求序列、读峰值数字，
不是只看"功能测试通过"就当内存问题解决了。**这条教训是踩出来的**：第一版像素上限阈值功能测试
全过（超限图确实被跳过），但因为没有真的去量 `VmHWM`，实际峰值跟没修之前几乎一样高，靠用户第二次
真实操作才暴露出来。

## 6｜超限漫画按卷拆分投递

![超限漫画按卷拆分投递](diagrams/comic-split-deliver.svg)

`bookconv::comic_split::deliver_split_streaming`：

1. **判定**：是漫画 + 有 `toc.ncx` 才走这条路，否则退回整本拒绝（老行为不变）。
2. **阶段一 · 规划**（`plan_pieces_sized`）：按第一层 NCX 结构切（2026-09-19 简化为只切第一层，
   不再递归更深层——旧的递归边界计算有过真机 panic，book-serve 进程被摔炸）；某卷切完仍超预算，
   退化到 `fixed_page_chunks_range_sized` 按页贪心再切一刀。
3. **阶段二 · 逐份循环**：`build_piece` 按需重收这份图片、简化页面结构（一张图一页）、补外链
   `comic.css`（贴边满屏，修过一次组包没带 CSS 导致原生阅读器四周留白的 bug）→ 上传 →
   `render_check::probe` 等 xochitl 真渲染出页数才传下一份（超时不算失败，继续传，不为等不到的
   确认阻塞整本书）→ 每份成功后回调，写 sidecar 结构化进度 `{done:idx,total}` +
   `bus.publish`（漏推事件是真机踩过的坑，不发事件网页不知道要刷新，进度条冻结在第一份）。
4. 原书母版字节全程一份不动，拆分份只存在于内存里，上传即弃。
5. 渲染自检对拆分份跳过——`RenderPlan` 按母版库条目名找书，拆分份没有对应条目，硬接只会认错书，
   这是已知的范围限制，不是遗漏。

## 7｜异步任务与进度上报

`spawn_optimize`/`spawn_deliver` 都是"零耗时同步校验（格式/文件存在/忙锁）先做，通过了才起后台
线程"的模板：校验失败立即返回给 HTTP 层，成功则起线程、写初始 pending 状态、返回"已开始"。真正
耗时的部分在线程里跑，结果通过 sidecar + SSE 呈现。

**进度节流**：优化阶段二可能有几百个条目（大漫画），每条目都做一次原子写 sidecar + SSE 广播是
真实的磁盘 I/O 开销，`OPTIMIZE_PROGRESS_STRIDE = 5`——每 5 条目才落一次盘/推一次事件，首尾两条
（第 1 条、最后一条）永远落，保证 UI 能看到"刚开始动"和"到 100% 了"。漫画拆分卷落库按份数天然
稀疏（几卷到十几卷），不需要额外节流。

**零轮询刷新**：`rmsvc_core::events::EventBus`，`book-serve` 在所有状态变更点 `bus.publish`；
网关 `GET /api/events`（SSE，20s 心跳）汇聚各服务事件；浏览器 `EventSource` 收到事件后按 `area`
找到对应 tab，在前台就立即 `refresh()`，不在前台记脏、切过去再刷。整个母版库列表没有任何
`setInterval` 轮询代码。

## 8｜前端 UI 层（`gateway/ui/app.js`）

`renderTransfer` 渲染"传书"页面：一个 subnav 两个 subpanel——**入库**（上传卡+抓网文卡）、
**母版库**（原生/KOReader 落库文件夹输入框+datalist 真实候选、搜索/格式/状态筛选、条目列表）。

`stagingList` 是这条线里最密集的渲染函数，每条目：
- 徽章：格式 / 优化等级 / 落库记录（原生+KOReader，落库晚于母版 mtime 标"旧"）/ 渲染自检
  （ok/warn/pending）/ 处理中或失败
- 三个操作按钮：「优化」→`POST .../staging/optimize`，「加入 xochitl」→`.../deliver`，
  「加入 KOReader」→`/api/koreader/books/adopt`+`.../staging/mark`；删除→`.../staging/delete`
  （`confirmDialog()` 二次确认，非浏览器原生 `confirm()`）
- 忙态判据：`busy = it.busy || localBusy`（服务端忙锁 || 纯前端本地忙态，后者专治 KOReader 无
  服务端忙态可查的情况）
- 进度：`renderStepProgress()` 统一渲染——`{done,total}` 有数据画真百分比（漫画拆分卷落库、
  EPUB 优化），没有画不确定态滚动条（普通整本落库），比一句不会变的静态文字更能传达"真的在动"

小组件：`el()`（轻量 DOM 构建 helper）、`guardClick()`（防双击，禁用态自动管理）、`toast()`
（替代 `alert()`）、`confirmDialog()`（替代 `confirm()`）——都是复用的横切关注点，不是每处各写
一遍。

## 9｜配置与 API 一览

**`BookConfig`**（`$XDG_CONFIG_HOME/shelf/book.json`）：

| 字段 | 默认值 | 说明 |
|---|---|---|
| `xochitl_host` | `10.11.99.1` | xochitl web 主机 |
| `upload_timeout_secs` | 300 | `/upload` 超时 |
| `native_upload_limit_mb` | 90 | 投原生体积门（见 §5） |

**`/api/books/*`**（经网关代理到 `book-serve:8790`）：

```
GET  /status                         状态+可用空间+xochitl 文件夹列表
GET  /staging                        母版库列表
POST /staging                        multipart 入库
POST /staging/optimize                异步优化
POST /staging/deliver                 异步落库（原生）
POST /staging/mark                    标记已加入某读器（给 KOReader adopt 流程配合用）
POST /staging/fetch-article           抓网文
GET  /staging/render/{uuid}           渲染缓存 PDF
POST /staging/delete                  删除母版库条目
GET  /events                          SSE 事件流
POST /trash/add · GET /trash/pending · GET /trash     原生回收站代理队列
POST /mkdir/add · GET /mkdir/pending · GET /mkdir      原生建文件夹代理队列
GET  /inbox · POST /inbox/retry · POST /inbox/delete   追平队列（历史入口，现状见 §2.2）
```

## 10｜已知限制（如实记录，不是遗漏）

- 网关代理响应体非流式（整体缓冲），跟部分注释描述不一致，低优先级未修。
- 渲染自检对漫画拆分份不生效（§6 第 5 点）。
- `imgopt` 的裁边/降采样在超大分辨率（>900 万像素）原图上直接跳过不处理，不是压画质，是完全
  不处理——这类图会原样出现在优化后的书里，体积/加载速度收益拿不到。
- `bookconv` 拆独立服务已评估否决（§1），不是"还没做"，是"做了评估、结论是不拆"。
