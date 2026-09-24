# reMarkable 设备端网关（gateway）白皮书

> **读者与用途**：维护网关、或要弄清“批量队列 / 并发闸门 / 母版库页”怎么设计、为什么这样的人。网关是什么、对外接口、目录结构见 [`../README.md`](../README.md)；整体位置见 [`../../docs/OVERVIEW.md`](../../docs/OVERVIEW.md)。
> 本文记“怎么决定、为什么正名搬顶层、踩了什么坑”。**读现状先看“5 分钟读懂”和 §00b；批量队列 / 闸门 / 母版库页的现行设计看 §03b；踩坑看 §04；命名遗留与待办看 §05。**
> 反向代理 / 注册表 / 事件汇聚这套架构本身的决策过程，是在它还叫 `shelf-gateway`、挂在 `shelf/services/` 下时定的，记在 `shelf/docs/reMarkable书架白皮书.md` §01（“三种拆法”）、§03z（事件推送）、§03aa/§03ah（渲染自检 / `proxy` 注释修正）等历史节，本文不重复。

## 5 分钟读懂

**一句话**：网关是设备上所有网页服务的“前台”——浏览器只认它一个（`https://shelf.local/`），登录一次，它把请求分给后面各个只听本机的小服务，同时替它们管好“别把 2GB 内存挤爆”。

**术语速查**

| 词 | 含义 |
|---|---|
| 反向代理 | 网关替浏览器去访问后端服务，再把结果原样带回来 |
| 注册表 | 每个服务启动时在 `$XDG_RUNTIME_DIR/shelf/services/<名>.json` 写下自己的端口；网关读目录就知道谁活着 |
| seg（URL 段） | `/api/books/...` 里的 `books`；`manage.rs::MODULES` 把它对应到服务名 `book-serve` |
| SSE | 服务器单向推送的长连接；网页靠它“被通知”刷新，而不是定时去问 |
| 闸门（budget） | 按文件体积给“吃内存的操作”排队：大档 1 个、小档 3 个 |
| 批量队列（batch） | 网关自己的后台队列，一次一本顺序执行，关掉浏览器照跑 |

![请求怎么走](diagrams/request-flow.svg)

**四条要记住的事实**

1. 部署固定 `0.0.0.0:443`，HTTPS（私有 CA，`/ca.crt` 装一次信任）+ 登录页密码（无用户名，默认 `shelf` 首登必改）。
2. 所有业务服务只听 `127.0.0.1`；`/api/<seg>/*` 由网关剥掉 `<seg>` 转发，服务没起 → 404，网页自动隐藏对应 tab。
3. 会占内存的三个 POST（`优化` / `加入 xochitl` / `加入 KOReader`）在网关统一过闸；批量队列同样每本过闸（§03b）。
4. 密码试错太多被锁时：网页登录口回 **429**，带 Basic 头的请求回 **401**（图见 §00b）。

## 00｜定位与原则

`gateway` 是 `shelf/`、`notes/`、`enhance/` 三条项目线共用的**唯一对外入口**：一个 HTTPS 反向代理 + 单页 UI 托管 + 服务注册表 + 事件汇聚器。它自己不做“书 / 笔记 / 字体 / 壁纸”这类业务，只做：① 托管单页前端；② `/api/services` 列注册表；③ `/api/<service>/*` 反向代理；④（2026-09-20 起）**批量队列**与**并发/内存预算闸门**——因为它是三个独立服务之间唯一的转发关口，跨服务的编排与限流只能放这里（§03b）。

**2026-09-11 正名搬顶层**：原名 `shelf-gateway`、位置 `shelf/services/shelf-gateway`。用户判断“壁纸/字体该不该挪进 enhance”时指出这个名字已不准确——它托管的网页早不只是“书架”（`notes/` 的「笔记」tab、`enhance/` 的系统增强 / 实验室 / 电池刺客开关都挂在它下面，网页标题 2026-09-10 也已改成“秘密花园”）。用户拍板：crate/二进制改名 `gateway`，目录搬到仓库顶层，三条业务线通过路径依赖 / 协同构建脚本接入它，而不是它属于其中某一条线。

原则（延续自 shelf-gateway 时代）：

1. **Facade（网关代理）**：`manage::MODULES` 是 URL 段 ↔ 服务名的单一事实源，管理台三态 / 代理路由 / status 都从它派生，不许各处各写一份映射。
2. **薄客户端消费 `enhance/` 产出**：`src/enhance/battop.rs` 只做 `systemctl start/stop` + 读 `summary.json`，不关心 `battop` 源码在哪；`wallpaper-serve`/`font-serve` 挪进 `enhance/` 后 `manage.rs` 完全不用改——代理只认服务名字符串。
3. **不做业务、只做转发 + 托管**：任何领域逻辑都不该长在这里；唯一例外 `src/enhance/{mod,qol,battop}.rs`——刻意不升成独立 service，因为它们只是“网页开关薄薄一层”，没有独立进程边界的必要（决策在 `shelf/docs/reMarkable书架白皮书.md` §03aj）。

## 00b｜现状总览（以当前代码为准；2026-09-22 按代码复核）

**对外**：`0.0.0.0:443`（2026-09-10 前是 `:8778`，见书架白皮书 §03am；`main.rs` 里的 `default_bind: 0.0.0.0:8778` 只是本地手动跑不带参数时的兜底）；HTTPS 由私有 CA 签发（首启生成，叶证书有效期 800 天、过期前 30 天或 SAN 变化时自动换叶，CA 不变）；`mDNS shelf.local`（iOS/macOS/Windows/Linux 可直接用，安卓不解析 `.local`，走热点 dnsmasq 别名 `shelf.rm`）。

**接口与目录结构**：完整清单见 [`../README.md`](../README.md)，这里不重复。

**登录与限速（依据 `src/auth.rs` + `rmsvc-core/src/auth.rs`）**：

![登录鉴权](diagrams/auth-flow.svg)

- 会话令牌 32 字节随机、只存内存（重启网关全部重新登录），容量 64 个，超出淘汰最早的；有效期取 `gateway.json` 的 `sessionDays`（缺省 30）。
- 首登必改：默认密码 `shelf` 首启时写入哈希并置 `mustChangePassword`；期间会话只能访问 `/password`（网页 303、API 403）；Basic 可直接 `POST /password`（已证明持有当前密码，不用再填 `current`）。新密码 ≥6 位（`config::MIN_PASSWORD_LEN`，登录页 `minlength` 与提示文案都从这个常量插值）且不能是默认值。
- **限速**：`FailLimiter` 全局计数（不分 IP——HTTP 层 `Request` 不带对端地址；局域网单用户，代价是攻击者最多让主人被锁一个窗口），60 秒窗口内 5 次失败锁 60 秒；锁定期内根本不做 PBKDF2 校验，也不再累计。**回应码因入口而异**：`POST /login`、会话态 `POST /password` 回 429 + `Retry-After`；`identify()` 里的 Basic 校验在锁定期直接判 Nobody，所以带 Basic 头的普通 API 请求回 **401**（不是 429）。
- 哈希：新密码一律 `pbkdf2$<轮数>$<盐>$<摘要>`（PBKDF2-HMAC-SHA256，60 万轮，16 字节随机盐）；旧版 `sha256$…`（单轮加盐）仍能校验，改密后自动升级（2026-09-09 审计修）。
- 忘记密码：设备上 `gateway reset-password`（回默认并强制改）或 `gateway passwd <新密码>`，均需重启网关生效。

**事件汇聚（依据 `src/events.rs` + `rmsvc-core/src/events.rs`）**：

![事件汇聚](diagrams/events-fanin.svg)

`Hub::spawn` 为 `MODULES` 里 `events:true` 的每个服务起一条线程跑 `events::follow`（服务没起等注册表目录 inotify、404 长等、断线 3s→60s 退避、loopback 心跳 `?ka=120`）；`mind-serve` 没有 `/events`（`events:false`），网关不订阅它——此前对它每 3 秒白打一个 404。另有一条线程监听注册表目录（防抖 500ms），服务上下线发 `{"area":"manage"}`。`batch.rs`/`budget.rs` 通过 `events::notify_books()` 把状态变化发到同一条总线。约束（用户 2026-09-06）：不轮询、不监听全盘、日志写入不触发。

**代理实现要点（依据 `src/proxy.rs`）**：**请求方向流式**（上传大文件不额外占内存），**响应方向整体缓冲**（2026-09-09 审计发现旧文档写的“body 流式透传”与实现不符，已如实改；`Reply::stream` 现有通道是给 SSE 用的，直接复用给任意大小下载的语义风险与收益不成比例，未改）；只回传 `Content-Type`/`Content-Disposition` 两个响应头，其余一律丢弃，不给后端夹带 `Set-Cookie` 之类敏感头的口子（历史踩坑见书架白皮书 §03ah）。超时 900 秒。

**测试**：约 34 个 `#[test]`（`budget.rs` 11、`batch.rs` 6、`manage.rs` 5、`auth.rs` 4、`ui.rs` 4、`proxy.rs` 2、`events.rs` 1、`config.rs` 1；数字随代码增长），`cargo test --manifest-path gateway/Cargo.toml` 独立跑；前端 `ui/test/`（冒烟与 XSS 测试）。语言包 `zh-CN.json`/`en-US.json` 当前各 460 个 key，`ui.rs` 有测试钉住两份 key 集合一致。

**依赖**：只依赖顶层 `../rmsvc-core`；不依赖 `shelf/crates/bookconv`、不依赖 `notes/` 任何 crate；不在任何 workspace 内。**谁在托管谁**：本目录只放网关自己的代码；三条业务线通过 `manage::MODULES` 的几行映射“挂上来”，各自的架构决策在各自白皮书，本文不代管。

## 01｜架构决策：为什么正名，而不是继续留在 shelf 底下

跟 `rmsvc-core` 正名同一次连锁判断（见 `rmsvc-core/docs/reMarkable设备端Web服务基座白皮书.md` §01），对网关这一半的关键点：**“谁托管网页 UI”不等于“UI 里的功能归哪条业务线”**。`enhance/` 的 `battop`/`hl-snap` 开关早就挂在这个网关的网页里，但没人因此觉得 `battop` 源码该待在 `shelf/` 底下——网关本来就是三线共用的展示层。现在把“网关本身”也按同样逻辑对待：它服务三条线，就不该继续算“shelf 的一个服务”。

**crate/二进制名**：候选过保留 `shelf-gateway`（只挪目录）、`hub`、`panel`；用户已把“改名”放进范围，最终选 `gateway`——简单直接，不引入需要额外解释的隐喻，也跟仓库里 `*-serve` 这套领域服务命名区分开（网关不是一个“领域”，是转发层）。

**没有改的东西**（详见 §05）：`SPEC.name`、日志前缀、CLI 帮助文本都改成了 `gateway`；但 XDG 运行时命名空间（`$XDG_RUNTIME_DIR/shelf/services/`）、`shelf.target`、默认密码字面量 `shelf`、mDNS 域名 `shelf.local` **没动**——它们牵连已部署设备的真实路径/配置，需要专门的迁移方案，本次范围明确排除。

## 02｜systemd

单元文件 `systemd/gateway.service`（原 `shelf/systemd/shelf-gateway.service`）：`PartOf=shelf.target`、`After=home.mount network-online.target`、`ExecStartPre=-/bin/sh /home/root/.local/bin/lo-alias.sh`（脚本源 `enhance/lo-alias/`，让 `10.11.99.1` 常驻可达，xochitl 的 web `/upload` 只绑 USB 网口）、`ExecStartPre=-/usr/bin/fc-cache`、`ExecStart=…/gateway serve --bind 0.0.0.0:443`；`Restart=on-failure`、`CPUWeight=20`、`MemoryMax=192M`、`Nice=5`（软限：交互式重活需要突发，只降权不硬顶）。依赖 `home.mount` 是本服务自己的依赖，**不牵连 xochitl 本体**（红线）。

`shelf.target` 名字**没有跟着改**——它是 `shelf/`、`notes/`、`gateway/`、`enhance/{wallpaper,font}-serve` 全部服务共用的开机分组目标，改名同样牵连已部署设备，见 §05。

## 03｜构建与部署：没有自己的 build.sh/deploy.sh

`gateway/` 没有独立安装/部署脚本——历史上它就跟着书架整包装，正名只挪了源码位置。实际构建/打包由 `shelf/build.sh`（`cd ../gateway && cargo build`）和 `shelf/deploy.sh`（把 `../gateway/target/.../gateway` 二进制 + `../gateway/systemd/gateway.service` 打进同一个部署包）代管，跟它代管 `../notes` 是同一套跨目录协同模式。

`gateway/.cargo/config.toml`（CC/AR 覆盖）是独立成顶层项目后必须补的东西，坑的详情见 `rmsvc-core` 白皮书 §04（网关与 `wallpaper-serve`/`font-serve` 一起踩的同一个坑）。

## 03b｜批量队列、并发闸门与母版库页（2026-09-19 / 2026-09-20，现行设计）

> **现状结论**
> - 批量由**网关**执行，不在浏览器里：`POST /api/batch` 入队，后台线程顺序逐本处理，状态落盘 `state/batch.json`（即 `~/.local/state/shelf/batch.json`），网关重启后 `resume` 续跑，`POST /api/batch/stop` 全部中止。
> - 每一本仍先过**并发/内存预算闸门**（大档 1 / 小档 3 / 排队 30 分钟 / 可取消）。两件事都放网关，是因为它是 `book-serve`、`koreader-serve` 之间唯一的转发关口。
> - 母版库页是“行内只有状态和停止钮 + 勾选后底部批量栏 + 两个常驻加入位置下拉”的单列布局。
> - **未验证**：并发闸门“两本大书是否真被串行化且 `VmHWM` 不叠加”这条最核心的验证一直没有独立做过；批量“加入 xochitl / KOReader”在设备上没跑；新界面的真实触屏交互。

### 闸门（`budget.rs`）

**问题**：漫画 EPUB→PDF 改动做完后真机测出“optimize / 超限分卷投递的内存峰值 ≈ 要处理的文件体积本身”（245MB 源书 optimize 峰值 206MB）。`book-serve`/`koreader-serve` 的忙锁都按书名分别加，点不同的书互不阻塞；设备只有约 2GB 内存、`systemd MemoryMax` 又没真正生效，同时点几本大部头会线性叠加，是真实的 OOM 风险。用户要求：按内存用量限流而不是按操作个数——小书可多点几个，大书基本只能点一个。

**为什么放网关**：三者是独立进程（独立 `[[bin]]`、独立 systemd unit、独立端口），进程内的 `Mutex`/`HashSet` 忙锁天然不跨进程；而网关是这三个操作**物理上唯一必经**的转发关口，进程内一把锁就够，不需要跨进程锁 / 共享内存 / IPC——这是本项目第一次遇到“三个独立进程需要协调”，网关天然是解法。

**实现**：不做“连续字节预算求和”（没有足够数据给不同操作 / 书籍类型精确倍率，量化是假精确），直接两档：体积 **> 90MB（严格大于）** 为大档，同时最多 1 个；否则小档，同时最多 3 个。`LARGE_THRESHOLD_BYTES` 与 book-serve 的 `native_limit`（xochitl 上传硬限）恰好都是 90MB，语义不同，不做跨进程配置同步。`admit(档位, 书名)` 阻塞等名额，最长 30 分钟，三种结局：放行（拿到 RAII `Slot`，`Drop` 自动归还并 `notify_all`）/ 超时（503）/ 被取消（503）；同名书已在 pending/active → 409。`pending`/`active` 两个书名集合只做展示与取消用，不参与准入判断（`large`/`small` 计数才是判据）。`proxy::forward` 只拦截三个命中路由（`book-serve staging/optimize`、`staging/deliver`、`koreader-serve books/adopt`），读一次小 JSON 拿书名、`stat` 母版库文件体积、过闸再转发，其余请求（含大文件上传）仍是纯流式转发。

**名额释放时机是最容易踩的点**：`优化`/`加入 xochitl` 在 book-serve 是**异步**的（HTTP 立即回“已开始”，真正处理在后台线程），不能靠响应返回释放——网关把 `Slot` 移进监控线程，查 `GET /staging` 直到该书 `busy==false`（或条目已不在列表里，如 PDF→EPUB 改名），上限 60 分钟。查询靠 book-serve 的事件唤醒，另有 30 秒兜底。查询失败要**连续 6 次**（每次至多隔 5 秒）才放名额；2026-09-24 前是一次失败就放，而大书优化时 book-serve 正忙、10 秒查询超时最容易撞上，第二本大书因此被提前放进来，闸门在最该起作用的时候失效；`加入 KOReader` 在 koreader-serve 是**同步**（`fs::copy`+`rename`），响应返回即做完，名额随请求释放。

**跨会话状态（2026-09-19 用户反馈驱动）**：最初的“取消/锁定”是纯浏览器标签页 JS 内存状态，关掉标签页就没了，而网关里排队的书照样傻等，新页面对此一无所知（列表里的书既看不出在排队也点不了停止，还能被当闲置条目删除或重复提交）。修法是把“谁在排队 / 谁在跑”搬到网关进程：`GET /api/budget/status → {pending, active}`、`POST /api/budget/cancel {name}`（只对还在排队的生效，已在跑的救不回来——诚实边界，不是没做全）。`pending/active` 变化还会经 `events::notify_books()` 推给网页（此前记的“不推 SSE、多标签页不同步”的范围边界已过时）。

![并发/内存预算闸门](../../docs/diagrams/budget-gate.svg)

### 批量队列（`batch.rs`，2026-09-20）

**用户反馈**：① 列表里 100 项要优化得点 100 次；② 批量是浏览器逐个提交，关掉页面剩下的就不跑了；③ 母版库页布局要兼顾 PC 和手机；④ 文件名太长（下载站 `-- 作者 -- hash` 尾巴）。

**设计**：`POST /api/batch {action, names?|all:true, folder?}` 入队 → 网关后台线程**顺序**逐本执行（设备双核、优化内部已并行处理图片，见 `bookconv::imgpool`）→ `GET /api/batch/status` / `POST /api/batch/stop`。任务自带动作，一个队列里可混合优化 / 加入 xochitl / 加入 KOReader。资格校验与界面按钮同一套（优化=EPUB/PDF 且未优化；加入 xochitl=EPUB/PDF；加入 KOReader=已装）；不适用的与重复入队的计入 `skipped`（`all:true` 时不适用的是预期筛选，不计）。异步的优化 / 加入 xochitl 轮询到 `busy=false` 才算完成，再读母版库条目的 `delivered.<kind>` 终态判成败（`failed`/`cancelled` 记入 `failed[]`）；加入 KOReader 同步，成功后代记一笔 `staging/mark`。worker 用 `catch_unwind` 兜住 panic，只让这一本失败（release profile 是 `panic="unwind"`，不是 abort）。

![批量队列状态机](diagrams/batch-queue.svg)

**状态落盘与续跑**：每次变化原子写 `state/batch.json`（序列化与写盘放在同一把 `persist_lock` 里，避免旧快照后写盘把队列回退）；网关启动 `batch::resume` 读回未完成队列，后台线程等 `book-serve` 就绪（最长 30 分钟，前 30 秒每 2 秒探测、之后每 30 秒；超时则队列保留在内存与磁盘，等下次入队继续，而不是丢弃）后按最新母版库状态重新校验每一本（已优化的不重做）。**崩溃循环防护**：每项记 `attempts`，中断时最多重放 1 次，`attempts≥2` 仍落在 `current` 里则记为失败跳过，避免某本书稳定触发崩溃时网关被 systemd 拉起后无限重放。

**中途停止**：“全部中止” = 清空未开始的（总数同步扣减）+ 正在处理的那本：还在等闸门 → 取消排队；已在 `book-serve` → `POST /staging/cancel`（EPUB 优化每处理完一个条目检查一次、按卷拆分投递每份之间检查，终态 `cancelled` 而非 `failed`；单文件上传、PDF 优化没有安全中断点，如实回 `cancelled:false`）。

### 03b-2｜母版库页按用户汇总重做（2026-09-20，同日第二轮，已部署设备；`app.js` 的 `stgRow`/`renderTransfer`，`style.css` 的 `.stg-*`）

第一版曾是“每行一个主按钮 + ⋯ 菜单、PC 两列”，用户看过后给出汇总，**推翻重做**为现行版：

1. **加入位置**：xochitl / KOReader 两个**常驻下拉**（“标签 + 下拉”一行，不折叠）+“＋ 新建文件夹…”。xochitl 新建走 `book-serve` 的 mkdir 队列（xochitl 里 QML 代理约 8 秒轮询建出来，界面等它出现再选中），KOReader 新增 `POST /books/mkdir`（幂等）。
2. **搜书名**：输入框带 `<datalist>` 建议（系列名 + 每本书名）。
3. **行内只显示**书名、类型、大小、状态、当前进度；**只有处理中/排队时才有一个“停止/取消排队”按钮**，其余按钮全去掉。
4. **所有操作统一在勾选后的底部操作栏**：优化 / 加入 xochitl / 加入 KOReader / 删除（批量删除带确认）/ 清除；按钮标“可处理数”，0 则置灰（回答“已优化被选上后再点优化会如何”：服务端本来就跳过，现在界面提前说清楚）；运行中变成进度 + 全部中止；新增“已完成”筛选。
5. **PC/手机同一套单列布局**，并用无头浏览器逐状态量 `scrollWidth`，390px/1280px 均无横向溢出。另有真分页（25/50/100 每页；PC 页码，手机“上一页/下一页”）、“全选当前筛选”与“优化全部待优化（N）”（`all:true`）。
6. 清爽书名去掉 `-- 作者 -- hash` 尾巴，完整名在提示里。

**验证情况（如实）**：

| 项 | 状态 |
|---|---|
| 批量优化（本机临时网关经 SSH 隧道指向设备真实 `book-serve`）：`all:true` 排 3 本自动跳过已优化、顺序完成；点名 6 个含 1 已优化 + 1 重复 → 排 4 跳 2；立即停止清掉 3 本 | 真机通（第一轮） |
| 中途停止：设备上优化 19MB 的《镖人》，15 秒后取消，1.2 秒停下、状态 `cancelled`、原文件字节不变、无 `.optimizing.tmp` | 真机通 |
| 续跑：排 5 本、进行中杀掉网关进程，重启后读回队列，已被优化完的那本不重做，其余续跑完成 | 真机通 |
| 批量“加入 xochitl / KOReader”（代码路径与优化相同，仅端点不同） | **未在设备上跑** |
| 并发闸门“两本大书被串行化、`VmHWM` 不叠加” | **从未独立验证** |
| 新界面的真实触屏交互；暗色模式对照；xochitl「新建文件夹」端到端 | **未验证**（只有无头浏览器点击） |
| 单测（约 34 个）、aarch64-musl 交叉编译零告警、部署后 `active`/`NRestarts=0` | 通过 |

## 04｜踩坑

按主题归并的坑位表（原按日期堆叠的长文已压缩，历史细节在 git 历史）：

| 坑 | 发现 / 根因 | 教训 / 修法 |
|---|---|---|
| 改 `ServiceSpec.name` 会不会连累别的服务 | 排查确认无人按这个字符串反查网关（网关只是注册表的读者+代理者） | 改名比预想安全；改前先 grep 反向依赖 |
| 测试假数据是孤儿 | `shelf/host/tests` 的 `FakeGateway` 硬编码了早已过期的 `"shelf-gateway"`/`"书架"`，纯字符串常量不会因逻辑测试失败暴露 | 改一处现实值，顺手 grep 测试 fixture 有没有抄旧值（该 host 测试随 host CLI 已砍，教训仍在） |
| 交叉编译坑（`.cargo/config.toml`、`version.workspace`） | 独立顶层 crate 不继承调用方目录的 cargo 配置 | 每个独立顶层项目各带一份物理副本；详见 `rmsvc-core` 白皮书 §04 |
| 「管理」页 8 个服务名漏了 i18n（2026-09-11） | `MODULES.label` 是硬编码中文，前端原样渲染，绕过 `T()`；靠读代码/grep 查不出，是真编译网关、无头浏览器登录后切英文才看见 | i18n 审计要扫“后端直接吐字符串”这类绕过管线的路径，并真切一次语言看每个页面。修法：`app.js` 优先查 `manage.modules.label.<seg>`，`m.label` 降为兜底 |
| 上传口重复文件（2026-09-13） | 网页 `uploader()` 缺 CLI 同款“跳过已存在”；真机又逮到漏网分支——同一批里排两份同名同大小文件时 `existing` 快照上传成功后没更新，造出 `1_x`/`2_x` 真实重复 | 修法与验证记在 `shelf/docs/reMarkable书架白皮书.md` §04（母版库概念归 shelf 线）；两份测试造的重复留给用户自己删 |
| 闸门批量第一版三个 UI 缺陷（2026-09-19） | ① 运行中单条按钮/其它批量按钮仍可点；② 看不出在处理哪本；③ 超限 PDF 一律灰掉——真根因是本分支与 `feat/comic-pdf-optimize` 是同一 master 的**兄弟分支**，前端还停在“PDF 不能拆”的旧假设 | 合并兄弟分支后 `tooBig` 对 EPUB/PDF 都例外；批量改逐项顺序提交；运行期间“取消选择”不该被统一禁用；漫画 optimize 会把 EPUB 改名 PDF，`picked` 集合里留下幽灵计数——每次 `refresh()` 按当前列表清理。**这些 UI 行为后被 §03b 的服务端队列整体取代** |
| “已优化(漫画)”徽章 | 漫画转出的 PDF 在列表里与用户原生 PDF 分不出；后端 `looks_like_own_comic_pdf`（有自己写的书签目录）早已算对，前端没用 | 只有漫画 EPUB 优化后才转 PDF（普通文字 EPUB 仍是 EPUB）；`format==='pdf'` 且 `optimized` 显示“已优化(漫画)”，别的 PDF 不动 |
| 取消按钮关浏览器后丢失（2026-09-19） | 锁定/停止是纯标签页 JS 状态 | 状态搬到网关（见 §03b 跨会话状态） |
| `/api/foundation` 新增 WeRead 探测（2026-09-13） | 用户装了外部第三方发行包 `remarkable-weread`（与早年自建、2026-09-05 已砍的微读管线无关，自带 `install.sh` 直装到 `~/.local/opt/remarkable-weread/`）；要求“显示在 KOReader 下方” | `Paths::weread_root()`（缺省该路径，`SHELF_WEREAD_ROOT` 可覆盖）；`foundation()` 探测 `bin/start-remarkable-weread.sh`；`app.js` 基石 kv 紧跟 KOReader 加一对——不进 `MODULES`（无 adopt 需求）。单测 `foundation_probes_weread_alongside_koreader` 覆盖 |

**WeRead 侧边栏入口的误判链（2026-09-13，可复用的教训）**：装完后设备侧边栏没入口。① 第一次判断“随包带了 AppLoad 应用包，手动拷到 `~/xovi/exthome/appload/weread/` 即可”——**错**：只解决“AppLoad 认不认识这个 app”，不解决“用户怎么点到它”，因为本项目的 `koreader-sidebar-entry.qmd` 运行时把原生“AppLoad”一级菜单项**隐藏**了（`visible=false`，为把 KOReader 提到侧边栏一级直达）；② 真正修法：在同一个 QMLDiff INSERT 块里紧跟 `cjKoreaderEntry` 后加一个同构 `ArkControls.SidebarItem`（`id: cjWereadEntry`，`launchApplication("external::weread", …)`），图标最终定为“We”（对齐 KOReader 的“Ko”视觉重量）。**教训**：结论前要反向确认“隐藏了原生入口后，新装的第三方 app 还有没有别的路进得去”；其源码不在本仓库（在 `oldbak/xovi-extensions/reading-qol/`，是该设备的现役 payload）。真机日志已核对 `CJ-SIDEBAR[8]: KOReader` → `[9]: WeRead` 顺序；用户肉眼确认“Ko/We 两个入口图标正常”；扫码登录 → 读书 → 干净退出整条链路真机通。
附带事实：退出 WeRead 不需要重跑 `xovi/start`（它的退出钩子只是普通 `systemctl stop/start xochitl`，tmpfs 里的 `00-xovi.conf` 不受影响；只有设备真断电重启才需要）；退出时的“重新加载读条”是因为它 `qtfb:false` 走独占 framebuffer 模式、xochitl 被完整重启，KOReader 是 `qtfb:true`，xochitl 全程不停。
**踩到的雷**：想用设备内置 `/usr/bin/screenshot`（给 xochitl 发 `SIGUSR2`）留截图——这个固件的 xochitl 没接该信号，发信号=杀进程，连崩两次（`status=12/USR2`，`NRestarts` 顶到 3，逼近 `StartLimitBurst=4`/10 分钟），systemd 自动拉回未造成损坏；**别再走这条路**。

## 05｜命名遗留 + 待办

**命名遗留（有意不动，范围外）**——理由都是“牵连已部署设备真实路径/配置，需要专门迁移方案”：XDG 运行时命名空间仍是 `shelf`（`$XDG_RUNTIME_DIR/shelf/services/`、`~/.config/shelf/`）· systemd `shelf.target` · 登录默认密码字面量 `"shelf"` · mDNS 伪域名 `shelf.local`。

**待办**：
- 上面几处要不要处理、什么时候处理，没有排期。
- 并发闸门的核心验证（两本大书真串行化、`VmHWM` 不叠加）仍缺（§03b 验证表）。
- ~~“正名 + wallpaper/font 迁移只做了 host 侧验证”~~ **已不成立（2026-09-11）**：`packaging/install-all.sh` 真机跑通，`shelf` 步骤（含网关）在设备上部署 + 启动，健康检查里 `gateway` 从 `activating` 变 `active`，用户确认“已成功安装”。
