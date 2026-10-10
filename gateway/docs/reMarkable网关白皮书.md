# reMarkable 设备端网关（gateway）白皮书

> **读者**：要维护网关，或想弄清"登录怎么判、请求怎么转、批量怎么排队续跑、网页什么时候取数"的人。
> **相关文档**：入口与速查见 [`../README.md`](../README.md)；网关用到的共用基座（HTTP、TLS、鉴权原语）见 [rmsvc-core 白皮书](../../rmsvc-core/docs/reMarkable设备端Web服务基座白皮书.md)；整个项目的位置见 [`../../docs/OVERVIEW.md`](../../docs/OVERVIEW.md)。
> **以 2026-10-10 的代码为准**（`gateway/src`、`gateway/ui`、`rmsvc-core/src`）。2026-10-10 重组过章节，旧章节号（§01～§09、§5.1、§06b 等）在文末[附录 C](#附录-c旧章节号对照表) 有对照。

## 目录

1. [一句话与术语](#1-一句话与术语)
2. [五分钟读懂](#2-五分钟读懂)
3. [怎么用](#3-怎么用)
4. [怎么工作](#4-怎么工作)（请求的路径 · 登录与安全 · 反向代理 · 事件推送 · 批量队列 · 网页前端 · 系统增强开关 · 设备健康）
5. [接口与配置参考](#5-接口与配置参考)
6. [开发、构建与测试](#6-开发构建与测试)
7. [已知限制与踩坑](#7-已知限制与踩坑)
8. [验证现状](#8-验证现状)
9. 附录：[A 设计决策与历史](#附录-a设计决策与历史演进) · [B 已移除的功能](#附录-b已移除的功能) · [C 旧章节号对照表](#附录-c旧章节号对照表)

## 1 一句话与术语

**一句话**：设备上有好几个各管一摊的小网页服务（书架、字体、壁纸、笔记四件套），它们都只听本机 `127.0.0.1`。**网关**是站在最前面的"前台"：浏览器只认它一个地址（`https://shelf.local/`），登录一次；它把请求转给后面的服务，批量「加入 xochitl」也由它排队、一本处理完再下一本。它自己不做"书 / 笔记 / 字体"的业务。

| 词 | 意思 |
|---|---|
| xochitl | reMarkable 自带的书库 / 阅读 / 笔记程序。本项目不改它本体，只往它的书库里投文件、用补丁调界面 |
| xovi / qmd | xovi 是让 xochitl 加载第三方扩展（`.so`）的框架；qmd 是改 xochitl 界面（QML）的补丁文件，由 xovi 扩展 qt-resource-rebuilder 在 xochitl 启动时读入 |
| rmsvc-core | 所有设备端 Web 服务共用的 Rust 基座库（HTTP、注册表、事件、TLS…），见[基座白皮书](../../rmsvc-core/docs/reMarkable设备端Web服务基座白皮书.md) |
| 反向代理 | 网关替浏览器去访问后端服务，再把结果带回来 |
| 注册表 | 每个服务启动时在 `$XDG_RUNTIME_DIR/shelf/services/<名>.json` 写下自己的端口和 pid；网关读这个目录就知道谁在跑 |
| seg（URL 段） | `/api/books/...` 里的 `books`。`manage.rs` 的 `MODULES` 表把它对应到服务名 `book-serve` |
| SSE | 服务器单向推送的长连接；网页靠它"被通知"刷新，而不是定时去问 |
| 母版库 | 书架服务 book-serve 里存原书的地方；网页「传书」页从这里把书「加入 xochitl」 |
| 批量队列 | 网关自己的后台队列，一次处理一本，关掉浏览器照跑 |
| 私有 CA | 设备首次启动时自己生成的根证书；装进手机/电脑一次，浏览器就不再报"不安全" |
| 整机重启 | 让 xovi 扩展、qmd 补丁生效的唯一方式。单独重启 xochitl 有概率在它退出时崩溃，所以网页里的提示一律写"整机重启" |

## 2 五分钟读懂

### 2.1 能做什么（用户视角）

| 能力 | 一句话 | 详见 |
|---|---|---|
| HTTPS + 登录 | 私有 CA 签证书；只要密码；首登必改；输错按来源 IP 限速 | [4.2](#42-登录与安全) |
| 服务发现 + 反向代理 | 注册表知道谁在跑；`/api/<seg>/*` 剥掉 `<seg>` 转发；大上传、大下载边读边转 | [4.3](#43-反向代理服务表与管理台) |
| 事件汇聚 | 各服务的 `GET /events` 汇成一条 SSE，网页不轮询 | [4.4](#44-事件推送) |
| 批量「加入 xochitl」 | 勾选多本，网关后台逐本做、落盘续跑、可全部中止 | [4.5](#45-批量队列) |
| 网页 UI | 单文件页面编译进二进制，中英文；传书 / 笔记 / 其他 / 管理四个标签页 | [4.6](#46-网页前端) |
| 系统增强开关 | 荧光笔汉字吸附、单击翻页、导入 md；显示扩展是否真的加载进 xochitl | [4.7](#47-系统增强开关) |
| 设备健康 / OTA 横幅 / 清理 | 打开时才采集的体检页；OTA 后页头提示重装；清理早期遗留 | [4.8](#48-设备健康ota-横幅与遗留清理) |

### 2.2 组成部分

| 部分 | 代码 | 一句话 |
|---|---|---|
| 入口与路由 | `src/main.rs` | 子命令、组装路由；路由器按"最具体优先"分发 |
| 登录 | `src/auth.rs`、`src/config.rs` | 登录守卫（Cookie / Basic）、首登必改、按 IP 限速；`gateway.json` 读写 |
| 服务表与管理台 | `src/manage.rs` | `MODULES`（唯一事实源）、三态启停卸载、基石探测 |
| 反向代理 | `src/proxy.rs` | `/api/<seg>/*` → 对应服务 |
| 事件汇聚 | `src/events.rs` | 订阅各服务、补 `svc`/`tab`、网关自发事件常量 |
| 批量队列 | `src/batch.rs` | 逐本「加入 xochitl」、落盘续跑、全部中止 |
| 页面 | `src/ui.rs`、`ui/` | 把 8 个脚本、样式、语言包编译期拼成单页 |
| 系统增强 | `src/enhance/` | 开关表 `TOGGLES`、`reading-qol.json` 读写、扩展加载检测 |
| 设备健康 | `src/device/` | 健康采集、OTA 横幅判据、遗留清理 |
| 共用片段 | `src/failed.rs` | 失败项 `{name, message}` |

### 2.3 架构图

![网关与各服务](diagrams/request-flow.svg)

网关只依赖 [`rmsvc-core`](../../rmsvc-core/README.md)（打开它的 `gateway` feature），不依赖 `shelf/`、`notes/`、`enhance/` 的任何 crate；后端服务是独立进程，网关只通过 HTTP 和注册表认识它们。

## 3 怎么用

### 3.1 第一次打开

1. 设备装好后（`packaging/install-all.sh`，见 [`docs/INSTALL.md`](../../docs/INSTALL.md)），手机或电脑浏览器打开 `https://shelf.local/`；连 USB 时也可以用 `https://10.11.99.1/`，WiFi 下用 `https://<设备 WiFi IP>/`。安卓系统解析不了 `.local`，走热点 dnsmasq 别名 `shelf.rm`。
2. 浏览器会提示"不安全"：点登录页上的「下载 CA 证书」（`/ca.crt`，不用登录），装进系统信任库（iOS 装完还要在"证书信任设置"里打开完全信任），之后不再提示。
3. 用默认密码 `shelf` 登录，按提示改密码（至少 6 位，不能是 `shelf`）。

### 3.2 网页上有什么

| 顶层标签 | 内容 | 谁提供数据 |
|---|---|---|
| 传书 | 入库（上传）· 母版库（勾选后底部栏「加入 xochitl」/ 下载原件 / 改名 / 删除） | book-serve + 网关批量队列 |
| 笔记 | 浏览 · 整理 · 回收站 · 导入 md（实验室开关打开才显示） | ink / transcribe / mind / note-serve |
| 其他 | xochitl 字体 · 壁纸（哪个服务在跑才出现哪块） | font-serve、wallpaper-serve |
| 管理 | 基石与模块 · 设备健康 · 模型管理 · 系统增强 · 实验室 | 网关自己 + 笔记线 |

页头还有三种横幅：OTA 后"需要重新安装"、"这个 WiFi 上不了外网"、设备端代理连试多次仍没做成（agent-failed）。页头圆点是事件流状态（绿 = 已连上）。

### 3.3 命令行

| 命令 | 作用 |
|---|---|
| `gateway serve [--bind 地址]` | 起服务。部署固定 `--bind 0.0.0.0:443`（`systemd/gateway.service`）；不带参数时的 `0.0.0.0:8778` 只是本地手动跑的兜底 |
| `gateway passwd <新密码>` | 直接改密码（同样 ≥6 位、不能是 `shelf`） |
| `gateway reset-password` | 回到默认密码 `shelf` 并强制下次登录改 |
| `gateway regen-tls` | 删掉服务器证书（`cert.pem`/`key.pem`/`cert.meta`），下次启动用同一个 CA 重签 |

改密码类命令要重启网关才生效（会话在内存里）。脚本访问用 Basic：`curl -u x:<密码> https://shelf.local/api/batch/status`（用户名随便填）。

### 3.4 接入一个新服务

1. 服务用 `rmsvc_core::service::run_or_exit` 启动（自动登记注册表、自带 `GET /health`）。
2. 在 `src/manage.rs` 的 `MODULES` 加一行：URL 段 `seg`、服务名、安装令牌 `only`、中文兜底名 `label`、有没有 `/events`、事件要唤醒谁（`wake`）、事件归哪个顶层标签（`tab`）。
3. 网页文案：语言包加 `manage.modules.label.<seg>`（`ui.rs` 的测试会核对两份语言包都有）。

代理只认服务名，不关心源码在哪条线的目录里。

## 4 怎么工作

### 4.1 一个请求要过的关

![一个请求要过的关](diagrams/request-lifecycle.svg)

网页打开时先取 `/api/services` 知道哪些服务在跑（每项带 `seg`），据此决定显示哪些标签页、拼 `/api/<seg>/…` 地址。之后每个请求依次过：并发名额 → 登录守卫 → 路由（网关自有路由优先）→ 反向代理。下面几节按这个顺序展开。

### 4.2 登录与安全

![登录鉴权](diagrams/auth-flow.svg)

#### HTTPS 与私有 CA

- 首次启动在 `~/.config/shelf/tls/` 生成私有 CA（`ca.pem`/`ca.key`，10 年）和服务器证书（`cert.pem`/`key.pem`，800 天；Apple 平台拒绝超过 825 天的证书）。证书和私钥都原子写，私钥创建时就是 0600（实现在基座 `tls`）。HTTPS 起不来时网关打日志并回落 HTTP。
- **什么时候换服务器证书**：证书里的名字列表变了（换了 WiFi、IP 变了），或签发满 700 天。CA 不变，已装 CA 的终端不受影响。
- **名称约束**：CA 只准签 `shelf`、`shelf.local`、`shelf.rm`、`localhost`、`remarkable`、`remarkable.local`（及子域）和 `10/8`、`172.16/12`、`192.168/16`、`127/8`，且不能派生下级 CA。原因：CA 装进了手机/电脑的系统信任库，不加约束的话，设备上的 `ca.key` 一旦泄露就能给任何网站签出被信任的证书。约束外的名字/IP 在签发时剔除并打日志（否则浏览器会拒绝整张证书）。
- **旧 CA 迁移**：启动时发现 `ca.pem` 没有约束（2026-09-24 前生成的），就把 5 个文件改名为 `<名>.bak-<秒>`（不删）、重新生成。**每台终端要做的**：重新下载安装新 CA，**并删掉旧的那张**（两张同名"shelf 书架私有 CA"，按签发日期区分；旧 CA 没约束、私钥还在 `ca.key.bak-*` 里，只装新不删旧，风险没有消除）。全部终端确认后再删设备上的 `*.bak-*`。

![私有 CA 检查与迁移](diagrams/ca-migration.svg)

#### 密码、会话与首登必改

- **登录方式**：网页登录页只要密码，成功后拿 Cookie `shelf_session`（HttpOnly；HTTPS 下 Secure；SameSite=Strict）；脚本用 `Authorization: Basic`，用户名随便填。
- **公开路径**（不用登录）：`GET/POST /login`、`POST /logout`、`GET /ca.crt`、`GET /health`、`GET /favicon.ico`。其余请求没认出身份时：浏览器（`Accept` 含 `text/html`）303 到 `/login?next=<原路径>`，其它 401。
- **会话**：32 字节随机令牌、只存内存（重启网关全部要重新登录）、最多 64 个（满了淘汰最早到期的）、有效期 `sessionDays`（缺省 30 天）。改密码后踢掉其它设备的会话，本会话保留。
- **首登必改**：首次启动写入默认密码 `shelf` 的哈希并置 `mustChangePassword`。改之前只放行 `/password` 和 `GET /api/session`：网页 303 到 `/password`，API 回 403。Basic 可以直接 `POST /password`（已证明持有当前密码，不用再填 `current`）。新密码至少 6 位（`config::MIN_PASSWORD_LEN`，改密页的提示也从这个常量插值）且不能是 `shelf`。
- **哈希**：`pbkdf2$<轮数>$<盐>$<摘要>`，PBKDF2-HMAC-SHA256 60 万轮、16 字节随机盐；旧版单轮 SHA-256 哈希仍能校验，改密后自动升级。PBKDF2 一律在配置锁外面算，一次慢校验不会卡住其它请求。
- **Basic 认证缓存**：带 Basic 头的脚本每个请求都要校验一次密码。基座的 `VerifyCache` 记住最近验过的密码：10 分钟、最多 8 条、**只缓存"通过"**（猜错照样现算、照样被限速），键是 `SHA-256(进程随机密钥‖存储的哈希‖密码)`，内存里不留明文；改密码时清空。
- **登录后跳转**：`next` 只接受本站路径——以单个 `/` 开头，不能是 `//`，不能含 `\` 或控制字符。
- **配置锁容忍 poison**：密码哈希与"首登必改"标记在一把进程内锁里，统一走 `rmsvc_core::sync::lock`。某个处理函数持锁时 panic 后照常能取到数据（单测 `poisoned_cfg_lock_keeps_auth_working`）。
- **忘记密码**：在设备上跑 `gateway reset-password` 或 `gateway passwd <新密码>`，再重启网关。

#### 失败限速（按来源 IP）

- 同一个 IP 在 60 秒内输错 5 次就锁这个 IP，直到最早那次失败满 60 秒（最多锁 60 秒）。锁定期内不做 PBKDF2（防并发猜密码烧 CPU），也不再累计（不会被无限续锁）。每次输错另外固定延时 500ms。成功一次清零该 IP；重启网关全部清零。
- 被锁时：`POST /login`、`POST /password` 回 **429** + `Retry-After`；带 Basic 头的普通请求回 **401**（守卫在锁定期直接当"没认出身份"）。已登录的会话不受影响。
- 按 IP 而不是全局计数：别人输错只锁他自己的地址，主人照样能登录。
- 表最多记 256 个 IP。满了先清过期的，再淘汰"没被锁的里最久没出错的"，全都被锁时才淘汰最旧的——否则攻击者换一批地址各错一次，就能把自己被锁的 IP 挤出表外。
- 不豁免任何网段：USB 网段 `10.11.99.0/24` 和 `127.0.0.1` 也限速，因为设备自己的 lo 别名就是 `10.11.99.1`。
- 对端 IP 由基座的 HTTP 服务器取 TCP 对端地址、写进内部头 `X-Rmsvc-Remote-Ip`；客户端自带的同名头在头白名单那一步就被丢掉，伪造不了（见基座白皮书 4.1「http」）。

### 4.3 反向代理、服务表与管理台

#### 服务表 `MODULES`

`manage.rs::MODULES` 是 URL 段 ↔ 服务名 ↔ 安装令牌 ↔ 显示名 ↔ 事件归属的**唯一事实源**：代理、管理台三态、事件订阅与标签路由都从它派生，不许别处再写一份映射。

| seg | 服务 | 缺省端口 | 有 `/events` | 事件归哪个标签 | 所属线 |
|---|---|---|---|---|---|
| `books` | book-serve | 8790 | 是（还唤醒批量队列） | 传书 | shelf |
| `fonts` | font-serve | 8792 | 是 | 其他 | enhance |
| `wallpapers` | wallpaper-serve | 8793 | 是 | 其他 | enhance |
| `ink` | ink-serve | 8795 | 是 | 笔记 | notes（条目库） |
| `transcribe` | transcribe-serve | 8796 | 是 | 笔记 | notes（手写转文字） |
| `mind` | mind-serve | 8797 | **否**（纯被动问答） | 笔记 | notes（问 AI） |
| `notes` | note-serve | 8798 | 是 | 笔记 | notes（笔记本 / 导出） |

端口是各服务的缺省值，代理实际按注册表里写的转发。`label` 只是语言包缺键时的中文兜底，网页优先用 `manage.modules.label.<seg>`。

#### 路由与代理行为

- **路由优先级**：基座的 `Router` 按"最具体优先"分发（字面段多的胜、精确匹配胜尾部通配），所以 `/api/batch`、`/api/enhance/...` 这些网关自有路由不会被 `/api/{svc}/*` 代理通配抢走，跟注册顺序无关。GET/POST/PUT/DELETE 以外的方法在 `/api/*` 下回 400。
- **转发**：`/api/<seg>/<rest>` → 剥掉 `<seg>` → `http://127.0.0.1:<端口>/<rest>`，查询串原样带上（`/api/<seg>` 不带后缀时转到后端 `/`）。后端直连（SSH 调试）和经网关走的是同一套路由。
- **出错**：seg 不认识 → 404"未知服务"；服务没起 → 404"`<服务>` 未安装或未运行"（网页据 `/api/services` 隐藏对应标签）；后端连不上 → 502。
- **请求体**：边读边转发、一律不解析（上传大文件不占网关内存）。GET/DELETE 不转发请求体，也就**不带客户端的 `Content-Length`**——否则带 body 的 DELETE 会让后端一直等那几个永远不来的字节。
- **响应体**：规则只有一条——**有长度的大应答边读边发，其余读完再回**。状态 200、后端给了 `Content-Length`，并且是下载（带 `Content-Disposition`）或体积超过 256KB（`STREAM_MIN_BYTES`）→ 按定长边读边发，浏览器能显示进度；其余（JSON 等小应答、错误应答、没有长度的应答）读完再回。host 实测代理一个 60MB 应答，网关峰值内存 62.4MB → 5.0MB。
- **只带回三个头**：`Content-Type`、`Content-Disposition`，以及状态 200 时的 `Cache-Control`（给 ink-serve 带哈希名的裁图长期缓存）。其余一律丢弃，不给后端夹带 `Set-Cookie` 之类的口子。
- **超时与连接**：进程内共用一个 ureq Agent（连接池复用 loopback 连接）。只设空闲超时——连接 3 秒、读 900 秒（后端有分钟级的同步接口，如转写、生成笔记本）、写 120 秒；**不设总时长**，慢网下传几百 MB 的书不会被拦腰截断。

#### 管理台与基石探测

- **`GET /api/services`**：注册表里在跑的服务（注册文件字段原样），每项补上 `MODULES` 里的 `seg`（网关自己不在表里，为 `null`）。
- **`GET /api/manage`**：每个模块回 `seg`、`service`、`only`、`label`、`installed`、`running`，外加 `gateway: {running: true}`。
- **三态**：未装（`~/.local/bin/<服务>` 不在）/ 已装未开 / 已开（注册表里有）。开关 = `systemctl start/stop`（只省后台占用，不是省电）；卸载 = 调设备上的 `shelf-uninstall --only <令牌>`。**安装不走网页**（不让网页去 remount `/usr` 装系统单元），未装的模块只给引导。网关自己不能从网页关或卸。
- **外部命令有超时**：`systemctl` 30 秒、卸载脚本 180 秒（基座 `proc::run_timeout`：超时 kill，stdout/stderr 各自排空）。
- **基石探测** `GET /api/foundation` → `{xovi, qrr}`：只读检查 xovi（`~/xovi/xovi.so` 或 `~/xovi/start`）与 qt-resource-rebuilder（`~/xovi/exthome/qt-resource-rebuilder/`）装没装。

### 4.4 事件推送

![事件汇聚](diagrams/events-fanin.svg)

- **原则**（2026-09-06 定）：不轮询、不监听全盘、日志写入不触发。事件只来自服务代码里的变更点，加上注册表目录的 inotify。事件只说"哪块该刷新了"，不带状态。
- **汇聚**：`events::spawn` 为 `MODULES` 里有 `/events` 的每个服务起一条线程，跑基座的 `events::follow`（服务没起就等注册表 inotify；连上用 `?ka=120`，两分钟一次心跳；断线 3 秒→60 秒指数退避；对方回 404 就长等 10 分钟），收到的事件补上 `"svc":"<seg>"` 和 `"tab":"<MODULES 声明的 tab>"`（覆盖服务自带的同名字段）后发进网关总线（`events::tag`）。行写坏了也照转，原文放进 `raw`。
- **服务上下线**：另一条线程阻塞在基座的 `events::registry_wake` 上（订阅线程等服务上线用的同一条 inotify，防抖 300ms），注册表变化时发 `{"area":"manage","kind":"services","tab":"manage"}`。
- **网关自己也发**：`batch.rs` 每次落盘经 `events::notify_books` 发 `{"area":"books","kind":"batch","tab":"books"}`（不带 `svc`，以此和 book-serve 自己的事件区分）。
- **反过来驱动批量队列**：`MODULES` 里带 `wake` 的服务（目前只有 book-serve）发来事件时，唤醒批量 worker 里"等这本处理完"的等待（`books_wake`，见 4.5）。
- **浏览器侧**：`liveStream` 连 `/api/events?ka=60`；按事件的 `tab` 找顶层标签，再由各标签自己决定刷多少（见 4.6）。

#### 事件 area/kind 总表

网页所有事件分支只认 `ui/core.js` 的 `EV` 常量表。服务侧：网关（`events.rs`）、book-serve（`src/events.rs`）、笔记线三个服务（各自 `main.rs` 的 `EVENT_*` 常量）都是常量，网关 `ui.rs` 与 book-serve 的测试核对 `EV` 表认得它们发的每个取值；font-serve、wallpaper-serve 仍写字面量。改名时两边都要动。

| area | kind | 谁发 | 什么时候 | 网页怎么处理 |
|---|---|---|---|---|
| `books` | `staging` | book-serve（`events::STAGING`，`api.rs`、`service_state.rs`、`staging/deliver.rs`） | 母版库条目变了（入库、删除、改名、忙态开始/结束、落库结果） | 传书：`LEVEL.LIST`（母版库列表 + 批量状态） |
| `books` | `render` | book-serve `render_check.rs`（带 `name`/`status`/`pages`） | 渲染自检出结果 | 传书：`LEVEL.LIST` |
| `books` | `mkdir` / `trash` | book-serve `api.rs` | 建文件夹队列、回收站队列变化 | 传书：`LEVEL.FULL` |
| `books` | `inbox` / `import` | book-serve `service_state.rs` / `api.rs` | inbox 处理了文件 / 直接导入做完一本 | 传书：`LEVEL.FULL` |
| `books` | `agent-failed` | book-serve `agent_failures.rs` | 设备端代理连试多次仍没做成 | 全站：重取 `/api/books/agent-failures` 更新横幅（与标签无关） |
| `books` | `batch`（不带 `svc`） | 网关 `batch.rs::persist` | 批量队列状态每变一次 | 传书：`LEVEL.QUEUE`（只取 `/api/batch/status`） |
| `manage` | `services`（不带 `svc`） | 网关 `events.rs::spawn` | 服务上下线 | 查 `/api/services`，标签集合变了整页重载，否则刷管理台 |
| `notes` | `entries` | ink-serve | 条目库变了（摄取、改字、浏览态动作、恢复、清空回收站） | 笔记：整页刷新；自己刚重取过（`refreshGate` 的 quiet）则不理 |
| `notes` | `transcribe` | transcribe-serve | 一轮转写结果变了 / 改了配置 | 笔记：整页刷新 |
| `notes` | `notebooks` | note-serve | 生成笔记本 / 导出 md 后同步状态变了 | 笔记：只重取同步状态 |
| `fonts` | `fonts` / `ui` / `config` | font-serve `main.rs` | 阅读字体池 / 界面字体 / 加粗开关变化 | 其他：只刷字体子面板 |
| `wallpapers` | `pool` | wallpaper-serve `main.rs`、`wake.rs` | 壁纸池或当前壁纸变化（含每次休眠轮换） | 其他：只刷壁纸子面板 |

`tab` 取值 `books`/`notes`/`other`/`manage`（传书/笔记/其他/管理）。「其他」再按 `svc` 分到字体/壁纸子面板；`area` 只用来判 `manage` 与传书的刷新档位。

### 4.5 批量队列

![批量队列状态机](diagrams/batch-queue.svg)

**为什么放在网关**：各服务是独立进程，进程内的锁天然不跨进程；网关是网页操作**唯一必经**的转发关口，在这里用一个进程内队列就能保证"同一时刻最多一本在投"，不需要跨进程锁或 IPC。状态放网关进程而不是浏览器里，关掉页面、换设备打开都能看、能中止。

- **接口**：`POST /api/batch {action: "deliver", names?: [...], all?: true, folder?}` → `{queued, skipped}`；`GET /api/batch/status` → `{running, waitingService, action, total, done, current, queued, failed}`（`queued` 只给前 200 个，`failed` 是 `[{name, message}]`）；`POST /api/batch/stop` → `{cleared}`。`action` 只认 `deliver`，别的回 400"action 只能是 deliver"；`names` 与 `all` 都没给回 400。入队时取不到 book-serve 的母版库列表：没运行 / 连不上回 503，它自己回的 4xx 原样透传。
- **资格**（和界面底部栏按钮同一套）：`rmsvc_core::formats::NATIVE_EXTS`（EPUB/PDF）。点名了但不合格的、已在队列或处理中的同名书计入 `skipped`（`all:true` 时不合格的是预期筛选，不计）。
- **一轮的计数**：从空闲开始的一次入队开新一轮，`done`/`failed` 清零，`total` 按队列里**实际有多少本**算（含上一次恢复后没跑的遗留项）；已经在跑时再入队，累加进同一轮。`action` 由当前那本或队首推出，都没有但有上一轮结果时是 `deliver`，从没跑过是 `null`。
- **执行**：后台 worker 线程**一次一本**。每本直连 book-serve 的 `POST /staging/deliver {name, folder}`，再 `poll_until_settled` 等到这本 `busy=false`（或已不在列表）：事件驱动（`books_wake` 唤醒，至多 30 秒兜底查一次）；两次 `GET /staging` 至少隔 5 秒（`MIN_REQUERY`）；**连续** 6 次查询失败才放弃等待（`MAX_POLL_FAILURES`，失败之间隔 5 秒）；最长 1 小时。然后从判定处理完的那份列表里读这本的 `delivered.deliver`。
- **成败只认 `ok`**：`delivered.deliver.status`（基座 `wire::DeliverStatus`）是 `ok` 才成功；`failed` 记失败、带 book-serve 写的原因；其余——等满 1 小时、连续 6 次查不到、条目途中消失、仍是 `pending`、认不出的旧值（`Unknown`）——一律记失败，原因写"没等到结果……请到 xochitl 书库里核对"。代价：书其实加进去了、只是没看到结果时会多一条"失败"，所以提示让人去书库核对而不是重加。worker 用 `catch_unwind` 兜住 panic，只让这一本失败（release 是 `panic="unwind"`）。
- **落盘与续跑**：每次状态变化原子写 `~/.local/state/shelf/batch.json`（序列化与写盘在同一把锁里，旧快照不会后写）。网关启动时 `resume`：被打断的那本**放回队首**；先把队列装进内存，后台等 book-serve 注册（最长 30 分钟，等注册表变化、30 秒没动静兜底探一次），这期间 `waitingService=true`、网页显示"等 book-serve 就绪"；再按最新母版库剔除已不在的；等不到也**保留**队列，下次入队一起跑。文件损坏或格式不对 = 当作没有未完成的队列。
- **全部中止**：清空还没开始的（总数同步扣掉）；已经交给 book-serve 的那本会跑完（整本上传、大文件通道都没有安全的中断点）；已出队但还没提交的那本——`stop` 同时打"中止当前"标记，worker 提交前看到就不提交，记失败"已全部中止"。

### 4.6 网页前端

#### 打包与源文件

- `ui/index.html`、`style.css`、脚本、`auth.css` 在编译期 `include_str!` 进二进制，拼成**一个零外链的单文件页面**；格式白名单从 `rmsvc_core::formats` 注入（`__EXTS__`），网页 `accept`、格式徽章、格式筛选项和服务端上传门同源。
- **脚本是 8 个源文件**，`ui.rs` 的 `APP_JS` 用 `concat!(include_str!(…))` 按固定顺序拼回**同一个** `<script>`（页面仍是单文件、不多一个请求）。都是经典脚本、共用一个全局作用域，不是 ES 模块：

  | 顺序 | 文件 | 内容 | 加载时 |
  |---|---|---|---|
  | 1 | `core.js` | 纯逻辑：`esc`/格式化、`T()` 与语言包、`EXT`（`__EXTS__` 占位）、`j()`/`jsend()`/`sendT()` 取数、`coalesce`、事件常量表 `EV`、笔记页刷新闸门 `refreshGate`、`LS`（localStorage 封装，键前缀 `shelf.`） | 只定义；调用时也不碰 DOM，node 测试整文件加载 |
  | 2 | `dom.js` | `el`/`btn`/toast/对话框/上传器/二级标签 `subtabs`、事件流连接 `liveStream` | 只定义 |
  | 3–7 | `transfer.js` `notes.js` `assets.js` `manage.js` `health.js` | 传书 / 笔记 / 其他（字体、壁纸）/ 管理（含模型卡片、系统增强开关）/ 设备健康与页头横幅 | 只定义 |
  | 8 | `app.js` | 启动代码：取语言包、建标签、开 SSE | **唯一有副作用的文件**，必须最后 |

  约束：顶层 `const` 只能引用排在前面的文件里的名字（函数声明会提升，不受限）。`ui/test/scripts.test.mjs` 兜住"拼接产物整体能解析""前 7 个文件在没有 `document` 的环境里加载不抛错"；`ui.rs` 的测试核对每个顶层名字在拼接产物里恰好定义一次。
- **i18n**：`ui/locales/{zh-CN,en-US}.json` 各 483 个键，`GET /ui/locales/{lang}` 下发，不认识的语言落中文。英文界面不混全角标点（括号、"名称：值"、列表分隔符走 `common.paren`/`common.labelValue`/`common.listSep`）；`<html lang>` 跟着界面语言切换。后端直接吐给前端的字符串（如 `MODULES.label`）绕过了翻译管线，前端优先查语言包（`T()` 缺键时返回键本身，不能写成 `T(k)||兜底`）。登录页与改密页是服务端生成的中文页面（未登录时读不到网页的语言选择）。
- **安全**：外部数据（文件名、书名、转写、AI 回答、服务端错误）插入 `innerHTML` 前统一经 `esc()` 转义；`xss.test.mjs` 扫全部 `ui/*.js` 里插进 HTML 的 `${…}` 是否都过了 `esc()`。
- **断网兜底**：`fetch` 在设备休眠、WiFi 断开、网关重启时会直接抛错；统一的 `j()` 把它转成"网络连接失败"提示；401 带 `next` 跳登录、403 跳改密（`authRedirect()`，`j()` 与上传器共用）。

#### 什么时候取数

![网页什么时候向网关要数据](diagrams/ui-refresh.svg)

- 各标签**第一次切过去才渲染**（渲染本身取一次数据），之后切回来只刷新；后台标签不取数。每块（顶层标签、「其他」里的子面板）的刷新都经 `refreshSec` 各自 `coalesce`，同一块同一时刻最多一个刷新在飞。
- 管理页一次刷新并行取 `/api/foundation`、`/api/enhance/status`、`/api/manage`；`/api/enhance/status` 只取一次，结果同时给「基石与模块」「系统增强」「实验室」。
- **事件到了取多少**（前台标签自己的 `onEvent`）：

  | 标签 | 事件 | 重取 |
  |---|---|---|
  | 传书 | 网关的 `batch`（不带 `svc`） | `LEVEL.QUEUE`：只取 `/api/batch/status` |
  | 传书 | book-serve 的 `staging` / `render` | `LEVEL.LIST`：母版库列表 + 批量状态 |
  | 传书 | 其余 `books` 事件、切标签、重连、操作后 | `LEVEL.FULL`：再加 `/api/books/status`（取 xochitl 文件夹列表） |
  | 其他 | 字体 / 壁纸任一服务 | 只刷发事件那个服务的子面板；认不出来源退回整块刷新 |
  | 笔记 | 笔记四服务的事件 | 经 `refreshGate` 决定（见下）；切回本标签时才查「导入 md」开关 |

  传书三档走同一个 `coalesce`，`need` 记"下一轮至少取到哪一档"、取最大值，不会出现旧的全量结果盖掉新的排队状态。
- **笔记页刷新闸门 `refreshGate`**（`core.js`）：① **hold**——推送本章 / 重新转写 / 提问的请求在跑（挡 10 分钟兜底）或结果提示停留中（再挡 3 秒），SSE 触发的刷新一律挡掉；用户自己换书、清空回收站是 force 不挡；被挡的那轮 force 消耗、`list`/`checkImport` 留给下一轮。② **quiet**——自己的动作刚重取过整本书，重取期间与之后 1 秒内的 `entries` 事件不理。③ **editing**——焦点在本标签的输入框里时事件只记一笔，失焦补一次。`notebooks` 事件只刷同步状态。`gate.st` 记着各道闸的状态与最近一次判定（`last`，如 `hold:busy`），排查时在浏览器控制台看它。时长常量在 `NOTE_GATE`。
- **事件流 `liveStream`**（`dom.js`）：页面隐藏超过 60 秒主动断开 SSE（锁屏的手机不再让设备为它保活），重新可见时重连；重连成功后补查 `/api/services`、补刷当前标签，断开期间的事件不会漏掉效果。浏览器放弃重连时（连上时回 401——网关重启后会话全没了——或 503 并发满，EventSource 直接进 `CLOSED`、不再重试），先查一次 `/api/session`（401 由 `j()` 带去登录页），其余情况 5 秒起、翻倍、封顶 5 分钟退避重开；页面隐藏时不重试。页面隐藏时 `manage` 事件也不去取 `/api/services`，只记一笔，可见或重连后补查。

#### 母版库页

`transfer.js` 的 `renderTransfer`/`stgRow`，2026-09-20 按用户汇总的要求定下，之后别加回单条按钮：

1. **加入位置**：一个常驻的 xochitl 文件夹下拉，带"＋ 新建文件夹…"。选了新建却没点创建就点加入会先提示，不悄悄落到书库根。新建走 book-serve 的建文件夹队列（xochitl 里的 `shelf-mkdir-agent.qmd` 长轮询 `GET /mkdir/pending?wait=290`），界面**不等它建好**：名字先补进下拉，加入时 book-serve 自己会等文件夹出现，建好后的 `mkdir` 事件把它换成真实项。选中的文件夹记在浏览器里（`shelf.folder`）。
2. **书名与搜索**：行内显示 book-serve 下发的清爽书名 `title`（去掉扩展名与下载站 `-- 作者 -- hash` 尾巴，完整名在提示里）；搜索框的建议按 `series`（`title` 再取第一个 `-` 之前）分组。
3. **行内只显示**书名、格式、大小、状态、进度，**没有按钮**。格式徽章：白名单格式按 `EXT.book` 显示；格式收窄前留下的旧文件 book-serve 报 `format:"other"`，徽章显示文件真实扩展名。另有落库记录（晚于母版修改时间标"旧"）、渲染自检页数、失败或"被重启打断"徽章；处理中只有不确定态进度条。
4. **所有操作在勾选后的底部栏**：第一行"已选 N 本 / 清除选择"；第二行主操作「加入 xochitl」（角标是可处理数，0 就置灰）；第三行"下载原件 / 改名 / 删除"（多选时只剩删除）。窄屏（≤34em）去掉"加入"前缀（动词与宾语是两个语言键 `stg.bar.deliverVerb`/`stg.bar.deliverObject`），≤22.5em 再缩字号。批量运行时底部栏显示进度 + 全部中止，同时照样能对勾选的书操作；`waitingService` 时显示"等 book-serve 就绪"；上一轮结果点「收起」后记在本机浏览器（`shelf.stgDismissed`）。选中的书全在处理中时删除按钮置灰并说明原因。
5. **PC 和手机同一套单列布局**；真分页（每页 25/50/100）；"全选当前筛选"。筛选：全部 / 未加入 / 已加入，各带数量，**默认「未加入」**；「已加入」看 book-serve 下发的 `done`（不在处理中、最近一次加入没失败、`delivered.native` 有值），「未加入」是它的补集（处理中、加入失败的也归这里），三个计数加得拢；另有格式筛选（选项由 `EXT.book` 生成，即 EPUB / PDF）。
6. **剩余空间告急**：优先读 book-serve 给的 `lowSpace`（剩余 <300MiB，阈值由后端定），旧后端没有这个字段时退回前端按 `freeBytes` 判（`lowspace.test.mjs`）。

原件下载、改名、笔记全文搜索的业务逻辑在 book-serve / note-serve / ink-serve，网关只是页面宿主，细节见书架与笔记白皮书。

#### 界面规则

2026-09-24 截图走查后定下，改样式时别破坏：

- 触屏设备（`@media (pointer:coarse)`）上按钮、页头链接、语言下拉、开关的可点区域撑到约 40px；只撑热区不放大字号，桌面鼠标不受影响。
- 标签栏吸顶位置跟着页头实际高度走：`app.js` 用 `ResizeObserver` 把页头高度写进 CSS 变量 `--hdr-h`（英文界面在手机上页头会折两行）。只在尺寸变化时回调，不轮询。
- `search`/`number`/`password` 输入框也套站内输入框样式。
- 列表行右侧按钮组放不下时整组换行靠右，不把名字挤成竖排；回收站条目的徽章单独一行。
- 新建 DOM 统一用 `el()`；按钮用 `btn()`、"发请求 + 失败提示"用 `sendT()`、页头横幅用 `banner()`。
- 确认 / 输入 / 单选三种对话框共用 `modal(cancelValue, build, onKey)` 骨架：点遮罩或 Esc = 取消，Enter 确认/提交——但焦点在某个按钮上时 Enter 只按那个按钮；关闭后摘掉键盘监听。
- 防连点：母版库底部栏的批量按钮与「全部中止」走 `guardClick`；上传进行中禁用删行、新文件只追加行（不整表重画，进度不会挂在旧节点上）。
- 笔记页：失焦保存不等回应，重取前先等在途的保存落地；用户正在输入时推迟事件重画。

#### 前后端契约

- **网关自有接口一律 DTO**：`batch::Status`/`Enqueued`、`manage` 的 `ManageStatus`/`Services`/`Foundation`/`ActionDone`、`enhance::Status`、`device::health::Health`、`device` 的 `CleanupList`/`CleanupDeleted`、`ota::Check`、`auth` 的 `Session`/`LoggedIn`/`PasswordChanged` 都是 `#[derive(Serialize)]` 结构体，各有线上格式快照测试（`wire_snapshot_*`，6 个）。仍用 `json!` 的只有不是网关应答的地方：转发给 book-serve 的请求体、`/api/device/wifi`（原样转发 wifi-watch 写的文件）、事件补字段失败时的兜底、页面注入的格式白名单。
- **失败项形状统一** `{name, message}`（`failed::Failed`）：批量队列 `failed[]` 与清理接口 `failed[]`。清理接口 200 应答不带 `ok`，部分失败（含全部失败）标 `partial:true`；`ok:false` 只来自基座错误信封（请求本身被拒）。
- **派生字段由后端下发，前端不镜像规则**：母版库条目的 `title`/`series`/`done`（book-serve）、笔记条目的 `live`（ink-serve，只在 HTTP 应答里、不写进条目库文件）、系统增强开关的 `toggles[].loaded`（网关）。不写兼容旧后端的回退，**各服务要和网关一起部署**。
- **动态 i18n 键有测试**：网页里拼出来的键（`ota.reason.<码>`、`ota.recovery.<值>`、`stg.batch.<动作>`、`wifi.banner.<状态>`、`agentfail.<种类>`、`manage.modules.label.<seg>`）由 `ui.rs` 的 `dynamic_i18n_keys_cover_every_backend_value` 遍历后端取值集合（`ota::Reason::ALL`/`Recovery::SHOWN`、`batch::Action::ALL`、`rmsvc_core::wire::FailureKind::ALL`、`MODULES`）断言两份语言包都有；WiFi 横幅状态 `portal`/`none` 来自 `packaging/wifi-watch/wifi-watch.sh`，在测试里就地列出。同一个测试还核对 `TOGGLE_UI` 给 `TOGGLES` 的每个键都配了文案、`EV` 表含网关自发事件的取值。
- **笔记条目字段读蛇形、写驼峰（有意不改）**：`notecore::model::Entry` 既是 HTTP 应答也是条目库的落盘格式，字段是蛇形（`page_index`、`chapter_title`），改名会让设备上已有的条目库读不回来；网页写条目走 ink-serve 手解的请求体，收驼峰（`askAi`、`destination`、`text`）；全文搜索的 `Hit` 是纯应答 DTO，用驼峰（`pageIndex`）。新增条目字段沿用蛇形，新增纯应答结构用驼峰。

### 4.7 系统增强开关

网关自己的固定能力（`src/enhance/`），不经注册表、不代理。刻意不升成独立服务：它们只是"网页开关薄薄一层"（同机文件读写），没有独立进程边界的必要。

| 接口 | 作用 |
|---|---|
| `GET /api/enhance/status` | `toggles:[{key, on, kind, loaded}]`；另保留三个顶层布尔 `hlSnapCjk`、`notesImportMdEnabled`（笔记页还在读）、`tapPageTurn`，以及 `loaded`（扩展映射与 qmd 载入的原始扫描结果，「基石与模块」列"生效的扩展"用） |
| `PUT /api/enhance/qol` | body 里出现 `TOGGLES` 里哪个键的布尔值就改哪个；一个都没有回 400（已移除的 `hwStrokeEnabled`、`comicMinMargin`、`rtlPageTurn` 单独传也回 400）。应答同 status |

- **开关表 `TOGGLES`**（`enhance/mod.rs`）：每行 = `reading-qol.json` 里的键名、缺省值、承载产物。读、写、`toggles` 都从它派生；加一个开关 = 表里一行 + 网页 `manage.js` 的 `TOGGLE_UI` 一行文案。网页不认识任何 `.so`/`.qmd` 文件名。

  | key | 缺省 | 产物 | `kind` | 页面位置 |
  |---|---|---|---|---|
  | `hlSnapCjk` | 开 | `hl-snap.so`（xovi 扩展，每次划线时读） | `extension` | 管理 → 系统增强 |
  | `tapPageTurn` | 关（= xochitl 原生行为） | `reader-page-turn.qmd`（每次打开书时读，切换后下次打开书生效） | `patch` | 管理 → 系统增强 |
  | `notesImportMdEnabled` | 关 | 无（只影响网页「导入 md」子标签） | `web` | 管理 → 实验室 |

- **`loaded` 怎么判**（`enhance::load_state`）：找不到 xochitl 主进程 → `unknown`；扩展映射了 → `on`，否则 `off`；补丁已载入 → `on`，文件比进程新 → `pending`（待整机重启），否则 `off`；`web` 为 `null`。网页按 `kind` 选徽章说明、按 `loaded` 选徽章。
- **扩展加载检测**（`enhance/loaded.rs`）：直接读 xochitl 主进程（`comm==xochitl` 且父进程是 1，排除渲染用的同名子进程）的 `/proc/<pid>/maps`，映射了哪个 `extensions.d/*.so` 就是真加载了；maps 解析与设备健康共用 `device::health::parse_maps`，行尾带 ` (deleted)` 的映射照样算已加载。qmd 不是 `.so`，按"qt-resource-rebuilder 在进程里 + 补丁文件早于 xochitl 启动"推断为已载入——这是按加载机制推断，看不到 qmd 里的定位是否全部命中。
- **缓存**：这个接口很常被调（管理页每次刷新、每个 manage 事件）。`reading-qol.json` 一次请求只读一次；xochitl 扩展扫描按 **(pid, 进程启动时刻)** 缓存，同一个 xochitl 进程只全量扫一次，之后每次只读一次 `/proc/<pid>/stat` 核对（host 合成数据 253µs → 1.7µs）。启动不到 30 秒的 xochitl 不缓存（xovi 还在逐个加载扩展）；qmd 状态看文件修改时间，每次现算。
- **开关存哪**：`~/.local/share/cangjie-ime/reading-qol.json`（与设备原生设置页、C 扩展共用）。写法是"整份读进来、只覆盖要改的键、其余原样写回"，进程内串行化，所以不认识的键不会丢（旧设备上的 `hwStroke*`、`comicMinMargin`、`rtlPageTurn` 原样留着，无人再读）。各开关在 xochitl 侧怎么生效见[系统增强线白皮书](../../enhance/docs/reMarkable系统增强线白皮书.md)。

### 4.8 设备健康、OTA 横幅与遗留清理

同样是网关自身固定能力（`src/device/`）。全都**不轮询**：没有定时器、没有后台采样，只有网关启动时那一次固件哈希。

![设备健康与 OTA 横幅](diagrams/device-health.svg)

| 接口 | 作用 |
|---|---|
| `GET /api/device/health[?fresh=1]` | 「管理 → 设备健康」的数据；缓存 15 秒，刷新按钮带 `fresh=1` 现采 |
| `GET /api/device/ota[?fresh=1]` | 页头横幅：`{needsReinstall, reasons, missingUnits, recovery, firmware}`；缓存 30 秒 |
| `GET /api/device/wifi` | 页头"WiFi 上不了外网"横幅：原样读 `wifi-watch` 写的 `~/.local/state/shelf/wifi-connectivity.json`（`{ssid, state: ok\|portal\|none, code, at}`）；文件不在 → `{state:"unknown"}`。不缓存 |
| `GET /api/device/cleanup` | 可清理的遗留文件 + xochitl 书库里的 EPUB/PDF + 回收站代理是否载入（只读） |
| `POST /api/device/cleanup/delete {area, names}` | 逐个删除遗留文件 → `{deleted, failed:[{name,message}], partial}` |

**页面**：「管理 → 设备健康」顶部一张卡（标题、刷新按钮、采集时刻），下面五个二级标签：概览（开机时长、xochitl、xovi、`/home` 空间、固件、OTA 判定）/ 服务（各单元状态、重启次数、内存、启动耗时）/ 扩展（映射的扩展、`(deleted)`、待换入区）/ 日志（上次开机 journal 尾部）/ 清理。进页或刷新时一次取 `health`+`ota` 分给前四个标签，切标签不重取；`cleanup` 只在清理标签在前台时取。上次停留的二级标签记在浏览器里（`shelf.healthSub`）。

**健康采集（`health.rs`）**：一次只 fork **两个**子进程——一个 `systemctl show -p … <全部单元>`（xochitl、`xovi-reenable`、gateway 加 `MODULES` 里的 7 个服务），一个 `journalctl -b -1 -n 20 --no-pager`（超时 10 秒；journal 没持久化时整块不显示）——其余都是读 `/proc` 和 `stat`：开机时长；各单元 `ActiveState`/`NRestarts`/`MainPID` 与 MainPID 的 `VmRSS`/`VmHWM`；启动耗时（`ActiveEnterTimestampMonotonic` 减 `InactiveExitTimestampMonotonic`）；xochitl 主进程（取 systemd 的 MainPID）的 `maps`——**每次现读**，因为 `(deleted)` 会在同一进程生命期里变化；待换入区 `~/.cangjie-stage/so-pending/`（换入途中的半成品 `.<名>.new.<pid>` 不列）；`/home` 剩余/总空间（基座 `fs::fs_space`，即 `statvfs`）。飞行记录仪日志在宿主机上，网关读不到。

**OTA 横幅（`ota.rs`）**：OTA 冲掉 `/usr/lib/systemd/system/` 下的单元和 `/etc` 里的 xovi 加载配置，`/home` 保留，所以"网关能打开"不代表装好了。

| 判据（`ota::Reason`） | 怎么查 | 触发横幅 |
|---|---|---|
| 单元文件缺失 | `gateway.service`、`shelf.target`，加上二进制已装的每个服务的单元，在 `/usr/lib/systemd/system/` 里不在 | 是 |
| xovi 未生效 | xochitl 主进程在跑、`maps` 里没有 `xovi.so`（复用 4.7 的扫描缓存）；xochitl 没在跑就不下结论 | 是 |
| 固件不在白名单 | `/usr/bin/xochitl` 的 sha256 不在构建时编进来的 `packaging/firmware-allowlist.txt` | 否，只在横幅已显示时提示"安装要加 `--force`" |

- 固件哈希单独不触发：`install-all.sh --force` 装在新固件上后，新哈希只追加进 host 的 `firmware-allowlist.local.txt`，网关构建时看不到，否则会永久误报。
- 哈希由启动后的后台线程**只算一次**（先等 30 秒避开开机高峰，流式读）；另两条打开页面时现查（缓存 30 秒），不在启动时定死——OTA 后照横幅重装时网关多半不重启，定死的结果会让恢复完横幅还挂着。
- 恢复方式（`ota::Recovery`，按 [`docs/INSTALL.md`](../../docs/INSTALL.md)「固件升级（OTA）之后」）：缺单元 → 设备旁 `xovi/rebuild_hashtable`，电脑上 `cd packaging && sh install-all.sh <设备>`（固件也不在白名单时带 `--force`）；只是 xovi 没生效 → `sh deploy-xovi-apply.sh <设备>`。横幅不给 `xovi/start`：xovi 已生效时跑它会让 xochitl 崩溃、整机重启。
- 可以点 × 在本次会话里关掉；不轮询。

**遗留清理（`cleanup.rs`）**：

- **`~/.local/state/shelf/books/done/`**（清理区代码 `books-done`）：早期直投流程的遗留目录，全仓库已没有代码读写它。网关列出里面的普通文件（不递归，符号链接和子目录不列），用户勾选、二次确认后逐个 `remove_file`。
- **删除的安全规则**（仓库出过清理时 `rm -rf` 掉用户目录的事故）：前端只能传清理区代码和文件名，不能传路径；名字必须是单段（拒绝空、`.`、`..`、含 `/` `\` NUL；有意不用基座的 `fs::plain_name`——它拒绝 `.` 开头的名字，而这里要清的就包括边车 `.<书名>.delivered`）；清理目录本身和目标都不能是符号链接、目标必须是普通文件；两边 `canonicalize` 后目标的父目录必须恰好是清理目录；从不整目录删。测试覆盖 `..`、兄弟文件、子目录、绝对路径、外指符号链接、被换成符号链接的清理目录、未知清理区，并把 HOME 与全部 XDG 指到临时目录。
- **xochitl 书库里的重复副本**：早年按卷拆分投进去的分卷，书名跟新版整本对不上，**没有精确识别规则**，所以只读列出书库里活的 EPUB/PDF，标"同名 ×N"供人工核对，不预先勾。勾选后前端逐本调 book-serve 的 `POST /api/books/trash/add {uuid,name}`；xochitl 里常驻的回收站代理 `shelf-trash-agent.qmd` 取走后调 xochitl 自己的 `moveEntriesToTrash`，几秒内进回收站、可恢复。**网关不直接删、不改 xochitl 目录里的任何文件**。回收站代理没载入或 xochitl 没在跑时，页面提示"排进队列要等它生效"。

## 5 接口与配置参考

### 5.1 接口

**不用登录**：`GET /login` · `POST /login` · `POST /logout` · `GET /ca.crt` · `GET /health` · `GET /favicon.ico`

**登录后（Cookie 或 Basic）**：

| 类别 | 接口 | 见 |
|---|---|---|
| 页面 | `GET /` · `GET /ui/locales/{lang}` · `GET/POST /password` · `GET /api/session` | 4.2、4.6 |
| 服务发现 / 管理 | `GET /api/services` · `GET /api/manage` · `GET /api/foundation` · `POST /api/manage/{seg}/{start\|stop\|uninstall}` | 4.3 |
| 事件 | `GET /api/events`（SSE，`?ka=<秒>` 改心跳间隔） | 4.4 |
| 批量队列 | `POST /api/batch` · `GET /api/batch/status` · `POST /api/batch/stop` | 4.5 |
| 系统增强 | `GET /api/enhance/status` · `PUT /api/enhance/qol` | 4.7 |
| 设备健康 | `GET /api/device/health` · `GET /api/device/ota` · `GET /api/device/wifi` · `GET /api/device/cleanup` · `POST /api/device/cleanup/delete` | 4.8 |
| 反向代理 | `GET/POST/PUT/DELETE /api/<seg>/*`，`<seg>` ∈ `books` `fonts` `wallpapers` `ink` `transcribe` `mind` `notes` | 4.3 |

错误应答统一是基座的错误信封 `{ok:false, message}`。

### 5.2 配置与文件位置（设备上，HOME = `/home/root`）

| 项 | 位置 / 值 |
|---|---|
| 二进制 | `~/.local/bin/gateway` |
| 配置 | `~/.config/shelf/gateway.json`（0600）：`https`（缺省 true）、`auth`（true）、`passwordHash`、`mustChangePassword`、`mdnsName`（`shelf`，空 = 不起 mDNS）、`extraSans`（`["shelf.rm"]`）、`sessionDays`（30） |
| 证书 | `~/.config/shelf/tls/`：`ca.pem` `ca.key` `cert.pem` `key.pem` `cert.meta` |
| 批量队列 | `~/.local/state/shelf/batch.json` |
| 系统增强开关 | `~/.local/share/cangjie-ime/reading-qol.json` |
| 注册表 | `$XDG_RUNTIME_DIR/shelf/services/<名>.json` |
| systemd | `systemd/gateway.service`：`ExecStart=… serve --bind 0.0.0.0:443`；`PartOf=shelf.target`；`After=home.mount NetworkManager.service xovi-reenable.service`（只排顺序、不拉起，**不牵连 xochitl 本体**；排在 `xovi-reenable` 后是让 xochitl 开机先把界面拉起来；不依赖 `network-online.target`，否则没 WiFi 时开机要等超时）；`ExecStartPre=-/bin/sh ~/.local/bin/lo-alias.sh`（让 `10.11.99.1` 常驻可达，失败不阻断）；`Restart=on-failure`；`CPUWeight=20`、`MemoryMax=192M`、`Nice=5` |

### 5.3 关键数字

| 项 | 值 | 出处 |
|---|---|---|
| 同时处理的请求上限 | 64（含 SSE 长连接），超了回 503 + `Retry-After: 2` | `rmsvc-core/src/http/server.rs` |
| 每连接读空闲超时 | 60 秒 | 同上（vendored tiny_http 补丁） |
| 登录限速 | 同一 IP 60 秒内错 5 次锁该 IP（最多 60 秒）；表最多 256 个 IP；每次错延时 500ms | `auth.rs` |
| Basic 认证缓存 | 10 分钟、最多 8 条、只缓存"通过" | `auth.rs`（`VerifyCache`） |
| 会话 | 30 天、只存内存、最多 64 个 | `main.rs`、`config.rs` |
| 代理超时 | 连接 3 秒、读 900 秒、写 120 秒，无总时长 | `proxy.rs::agent` |
| 代理流式阈值 | 200 + 有长度 +（下载或 > 256KB） | `proxy.rs::STREAM_MIN_BYTES` |
| 批量等待 | 事件唤醒，30 秒兜底；查询间隔 ≥5 秒；连续 6 次失败放弃；单本最长 1 小时；重启后等 book-serve 最长 30 分钟 | `batch.rs` |
| 设备健康缓存 | health 15 秒、ota 30 秒；固件哈希启动 30 秒后算一次 | `device/mod.rs`、`device/ota.rs` |
| 语言包 | 中英各 483 个键 | `ui/locales/` |

## 6 开发、构建与测试

- **依赖**：只依赖顶层 `../rmsvc-core`（`features = ["gateway"]`）；独立 Cargo 项目，不在任何 workspace 里，自带一份 `.cargo/config.toml`（交叉编译的 CC/AR 覆盖，原因见基座白皮书第 7 章）。
- **构建/部署由别人代管**：没有自己的 `build.sh`/`deploy.sh`。`shelf/build.sh` 顺手编译 `../gateway`，`packaging/deploy.sh` 把二进制和 `systemd/gateway.service` 打进同一个部署包。单独交叉编译：`cargo build --release --target aarch64-unknown-linux-musl`。**网关要和各服务一起部署**（网页依赖服务下发的 `title`/`series`/`done`/`live` 等字段）。
- **release profile**：`opt-level="z"`、LTO、strip、`panic="unwind"`（abort 下 `catch_unwind` 完全无效，一次 panic 就摔掉整个进程；代价是 aarch64 二进制大约 8%）。dev 构建给依赖开 `opt-level=2`（登录测试要算 60 万轮 PBKDF2）。
- **测试**（在仓库根目录跑；数字是 2026-10-10 文档更新时实跑）：

  | 测试 | 命令 | 数量 |
  |---|---|---|
  | Rust 单测 | `cargo test --locked --manifest-path gateway/Cargo.toml` | 77 个，含 6 个线上格式快照（`wire_snapshot_*`）、动态 i18n 键覆盖、拼接脚本顶层名字唯一 |
  | 前端单测 | `node --test gateway/ui/test/*.test.mjs` | 6 个文件共 19 项：`core` 4（格式徽章、`refreshGate`）/ `locales` 4（中英键集合与占位符一致、无死键）/ `lowspace` 2 / `net` 4（断网兜底、401/403 去向）/ `scripts` 2（拼接产物可解析、非启动文件无加载期副作用）/ `xss` 3 |
  | 语法检查 | `for f in gateway/ui/*.js; do node --check "$f"; done` | 8 个脚本 |
  | 浏览器冒烟（手动） | `PUPPETEER_NODE_MODULES=<含 puppeteer 的 node_modules> node gateway/ui/test/smoke.puppeteer.mjs` | 覆盖 SSE 断开重连、按事件来源重取、笔记输入中不重画、对话框键盘、`format:"other"` 徽章、`toggles` 渲染、清理部分失败提示等；不进 CI，本次文档更新未重跑 |

  node 测试经 `ui/test/load.mjs` 按 `ui.rs` 的拼接顺序整文件加载源文件（在 vm 上下文里跑），零 npm 依赖。CI（`.github/workflows/ci.yml`）跑前三项与 `cargo audit`；人眼走查工具见 [`../tools/screenshot-walkthrough/README.md`](../tools/screenshot-walkthrough/README.md)。

## 7 已知限制与踩坑

**已知限制**

- 改了 `mdnsName`/`extraSans`，或设备拿到 CGNAT（`100.64/10`）、链路本地、公网 IP 时，这些名字/地址不进证书，用它们访问会报证书错误；约束内容以后再改，迁移逻辑只看"有没有约束"，不会自动重签。
- OTA 横幅的误报面：dm-verity 激活、单元从没装进 `/usr` 的设备（`deploy-usr-unit.sh` 会跳过写入）会一直显示"缺单元"；在 host 上跑网关也会显示。
- 批量结果不明（超时、查不到）一律记失败，会多出"其实已经加进去"的失败项——按提示去 xochitl 书库核对，别直接重加。
- 会话只存内存：网关重启后所有终端都要重新登录（开着的网页会自己跳到登录页）。
- **命名遗留（刻意不动）**：XDG 命名空间仍叫 `shelf`（`~/.config/shelf/`、`~/.local/state/shelf/`、`$XDG_RUNTIME_DIR/shelf/services/`）、systemd `shelf.target`、默认密码 `shelf`、mDNS 名 `shelf.local`、Cookie 名 `shelf_session`。它们牵连已部署设备的真实路径和已装的证书/书签，改名需要专门的迁移方案，没有排期。

**踩坑与教训**（每条的来龙去脉在 git 历史与 [CHANGELOG](../../docs/CHANGELOG.md)）

| 坑 | 根因 | 教训 / 修法 |
|---|---|---|
| 原件下载永远收不完（09-24） | 文件下载复用了 SSE 的"读到连接关闭"通道，后端读完并不关连接 | 只有带长度的应答才流式、走定长响应；两种流语义不同，不能混用（基座 10-10 起用 `Body` 枚举从类型上分开） |
| 慢网下传大书被截断（10-07） | 代理给 ureq 设的 900 秒是整请求时长 | 大文件转发只设连接/读/写空闲超时，不设总时长 |
| 带 body 的 DELETE 让后端干等（09-25） | 代理不转发请求体，却照抄了 `Content-Length` | 只在真转发请求体时带长度 |
| 等异步任务时一次查询失败就放行（09-24） | 大书处理时查询最容易超时 | 连续 6 次失败才放弃；"侦测失败"和"任务结束"分开对待 |
| 批量把"没等到结果"记成成功（10-07） | 结论只认 `failed` | 只认明确的 `ok`；结果不明一律记失败并让人去书库核对 |
| 开机时批量队列被覆盖（09-20） | `resume` 等不到 book-serve 就返回，随后入队把磁盘上的旧队列覆盖 | 先装进内存再等；超时也保留 |
| 同名书并存互相抹记录（09-20） | 排队表以书名为键 | 同名书已在队列或处理中时跳过 |
| 批量进度显示"5/2"（09-25） | 遗留队列项仍会跑，新一轮 `total` 只算这次入队的 | 重置时 `total` 取队列实际长度 |
| 「全部中止」拦不住当前这本（09-30） | 取消落在"已出队、未提交"窗口 | 中止标记 + 提交前检查；"取消"要覆盖每一个交接窗口 |
| 旧 `batch.json` 里一项已撤的动作毁掉整个队列（09-30） | 动作枚举删了一项，整份反序列化失败 | 删枚举值时要想到落盘的旧数据 |
| 网关重启后网页不再实时刷新（09-30） | 连上时回 401/503，`EventSource` 进 `CLOSED` 永不重试 | `CLOSED` 时先查会话，其余退避重开；别以为 EventSource 会无条件重连 |
| 大书处理时母版库每秒全量重取 6 个接口（09-25） | 进度事件被当成"一切都可能变了" | 按事件来源分档 |
| 上传进度条停住 / 笔记改字被冲掉（09-25） | 异步操作还挂在旧 DOM 节点上时整段重画 | 进行中只追加不重画；重取前等在途保存落地；输入中推迟重画 |
| 确认框里 Tab 到「否」按 Enter，删除照样执行（10-09） | 文档级 keydown 把 Enter 一律当"确认" | 焦点在按钮上时 Enter 交给按钮 |
| 一次 panic 之后谁都登不进（10-09） | 认证配置锁被毒化后取到空哈希、`must_change` 恒假 | 长期持有的进程内锁用容忍 poison 的取法，并写"先毒化再校验"的单测 |
| 换了 `xovi.so` 后误报 OTA 横幅（10-07） | 扩展加载判定自己解析 maps，不认 ` (deleted)` | 同一种解析只留一份（`parse_maps`） |
| ink/transcribe 的事件能到笔记页纯属巧合（10-10 审计 FE-6） | 网页按 `area` 找标签，note-serve 的 seg 恰好也叫 `notes` | 由 `MODULES.tab` 显式声明归属，网关补 `tab` 字段 |
| 「管理」页 8 个服务名漏翻译（09-11） | `MODULES.label` 是后端硬编码中文，绕过 `T()` | i18n 审计要扫"后端直接吐字符串"的路径，并真切语言看每页 |
| 独立顶层 crate 交叉编译失败（09-11） | 不继承调用方目录的 `.cargo/config.toml` | 每个独立顶层项目各带一份 |
| 网页提示让用户"重启 xochitl"或"跑 `xovi/start`"（09-25 前） | 停止 xochitl 有概率在退出途中崩溃；xovi 已生效时跑 `xovi/start` 必崩 | 网页、回执、文档里一律写"整机重启" |
| 隐藏原生入口后第三方 app 进不去（09-13，相关功能已退役） | 侧边栏补丁隐藏了原生"AppLoad"入口 | 下结论前要反向确认"隐藏了原生入口后，新装的第三方 app 还有没有路进去" |
| 想用 `/usr/bin/screenshot` 截图 | 它给 xochitl 发 `SIGUSR2`，这版固件没接这个信号，等于杀进程 | **别再走这条路** |

## 8 验证现状

写法：「真机通」= 在设备上实际操作过这个功能；「已部署」= 代码装上设备、部署自检（`packaging/verify-on-device.sh`、服务状态、接口健康检查）通过，但功能本身还没人手测；「host」= 只有单元测试或开发机上的测试。

| 批次 | 状态 |
|---|---|
| 网关部署运行（`active`、`NRestarts=0`） | **真机通**：09-24 只读核对；09-25 整轮卸载后重装 43✓ 0✗；09-30、10-07 几次部署后 `verify-on-device.sh` 全部 0✗，9 个常驻服务 `NRestarts` 0 |
| 批量「加入 xochitl」 | **真机通**（2026-09-25 用户在网页操作：两本 >90MB《乱马》，队列 `done 2 / failed 0`）；批量中途停止、网关重启后续跑 09-20 真机通 |
| xochitl"新建文件夹" | **真机通**（09-19 端到端；09-24 验证 290 秒长轮询） |
| 09-25 真实登录只读走查 | **真机**：无头 Chromium 登录真机，只点一至三级标签、不点会改数据的按钮；中文亮/暗 390、中文 1280、英文 390 共 108 屏，无横向溢出、无控制台报错、无失败请求，并据此修了设备健康页四处显示 |
| 私有 CA 自动迁移到带名称约束的新 CA | 设备端已发生（2026-09-24 09:38，日志与 `.bak-*` 文件可见）；**各手机/电脑是否已重装新 CA、删掉旧 CA：未核实** |
| 09-24 第三轮审计（代理大应答流式、`/api/enhance/status` 缓存、按 IP 限速、`next` 校验、按标签懒加载、断网兜底） | 已部署；安全改动没在真实手机/电脑上专门试过 |
| 09-25 设备健康 / OTA 横幅 / 清理与第四轮审计（按事件来源减少重取、上传器/笔记保存修复、对话框骨架） | 已部署；页面经真实登录只读走查。**真 OTA 后横幅、两类清理的删除按钮没在真机上做过**（采集依赖的设备格式 09-25 已只读核过：多单元 `systemctl show` 按空行分块、journal 已持久化、`/usr/bin/xochitl` 算 sha256 用 0.54 秒、书库 `createdTime` 是字符串形式的毫秒） |
| 09-30 第五轮审计（撤 KOReader/WeRead/appload、Basic 认证缓存、全部中止补漏、事件流断线退避） | 已部署（09-30），部署自检通过；功能待手测 |
| 10-07 书架不再优化书、删闸门、代码审查修复（代理空闲超时、批量只认 `ok`、`waitingService`、`registry_wake`、`parse_maps` 共用） | 已部署（10-07 15:54 并整机重启，部署自检 36✓ 1⚠ 0✗）；功能未真机手测 |
| 10-09 第六轮审计（确认框 Enter、配置锁 poison、`/api/services` 带 `seg`、`lowSpace`、页面隐藏不取数等）及同日审计后续（批量 `names` 改用 `opt_str_list`、网页发单价用 `inputPer1k`/`outputPer1k`） | 已部署（第六轮 10-09 13:48 `install-all`，整机重启后部署自检 38✓ 1⚠ 0✗；审计后续随之后的部署上机）；**功能未在真机手测** |
| **10-10 重构**（网页脚本拆 8 个源文件；接口 DTO 化 + 线上格式快照；`TOGGLES` 与 `toggles`；`MODULES.tab` 路由事件；后端下发 `title`/`series`/`done`/`live`；清理失败项 `{name,message}` + `partial`；`format:"other"` 徽章；`EV`、`refreshGate`、`liveStream`；`FailureKind`） | **host**：`cargo test` 77 个、node 19 项通过（本次实跑），浏览器冒烟与 clippy 在 10-10 重构时通过；**未部署**，须与 book-serve、ink-serve 同批上机 |

**待手测清单**（部署后逐批看；书架那边对应条目在书架白皮书附录 §05）

- **10-10 重构**（须与 book-serve 的 `title`/`series`/`done`、ink-serve 的 `live` 同批上机，否则母版库书名会显示成完整文件名、搜索建议为空、「已加入」计数为 0，笔记「整理」为空）：母版库书名、搜索建议、三个筛选计数与部署前一致；残留的非 EPUB/PDF 文件徽章显示真实扩展名；勾一本书后底栏按钮在手机窄屏去掉"加入"；「系统增强/实验室」三张开关卡片齐全、勾选状态与"已加载 / 待重启 / 网页功能"徽章在整机重启前后正确；「设备健康 → 清理」部分失败时提示列出原因；笔记页结果提示停留约 3 秒不被冲掉、打字时后台事件不打断、失焦后补刷；页面切到后台 60 秒以上再切回，页头圆点转绿；`GET /api/enhance/status` 带 `toggles`；`GET /api/events` 的事件带 `tab`，笔记、其他标签照常即时刷新。
- **10-09 第六轮审计**：确认框里 Tab 到「否」按 Enter 不执行；清空回收站后笔记页正常；「模型管理」改单价保存后刷新数值不变；页面在后台时装/卸一个服务，切回后标签集合跟着变；管理台卸载失败时错误提示留得住；母版库剩余空间告急时标红；设备健康"待换入"不列半成品；`GET /api/services` 每项带 `seg`。
- **10-07 三批**：母版库只剩「加入 xochitl」、行内没有按钮、筛选计数加得拢、格式筛选只有 EPUB/PDF；批量多本照常一本本跑、结果不明记失败并提示去书库核对、网关重启后底部栏显示"等 book-serve 就绪"并续跑；`/api/budget/*` 回 404；单独 `PUT {"rtlPageTurn":true}` 回 400；笔记页事件刷新不跳章、编辑中不丢字、「推送本章」链接能打开；慢网下大文件超过 15 分钟不被截断；换 `xovi.so` 后不误报 OTA 横幅（书架白皮书附录 §05 #22、#23）。
- **09-30 第五轮**：网关重启后开着的网页能自己回登录页或恢复实时刷新；「全部中止」点在一本刚开始时这本确实不跑；带 Basic 头的脚本连续请求时网关 CPU 不再每次尖峰；母版库全量刷新为 3 个请求。
- **09-25 第四轮**：「其他」标签壁纸轮换只刷壁纸块；笔记输入框里打字时后台事件不打断；上传中删行按钮置灰。
- **09-24 的安全改动**：在真实手机/电脑上走一遍按 IP 限速、`next` 校验、断网提示；确认每台终端已装新 CA 并删掉旧 CA，之后删设备上的 `tls/*.bak-*`。
- **第三轮审计剩余**：下载大原件时网关 `VmHWM`；"已加载"徽章在整机重启前后是否正确刷新；真实手机上的触屏热区与英文页头吸顶。
- **设备健康 / 清理**：真 OTA 后横幅是否出现、恢复后是否消失；两类清理的删除按钮在真机上点一次。

## 附录 A　设计决策与历史演进

只留结论和"为什么"；完整经过在 git 历史与 [CHANGELOG](../../docs/CHANGELOG.md)。

- **2026-09-11 正名搬顶层**：原名 `shelf-gateway`，在 `shelf/services/` 下。它托管的网页早不只是"书架"（笔记、系统增强都挂在这里，网页标题 09-10 已改"秘密花园"）。名字选 `gateway` 而不是 `hub`/`panel`：直白，也和 `*-serve` 领域服务区分开。"谁托管网页 UI"不等于"UI 里的功能归哪条线"。反向代理 / 注册表 / 事件汇聚的最初决策是在 `shelf-gateway` 时代定的，记在书架白皮书 §01（三种拆法）、§03z（事件推送）。
- **路由优先级**（09-20）：此前"先注册先匹配"，通配路由会抢走字面路由；改成最具体优先，与注册顺序无关。
- **批量队列**（09-20）：此前批量是浏览器 `for…await` 逐个提交，关掉页面剩下的就丢了；改为网关后台队列、落盘续跑。
- **并发/内存闸门**（`budget.rs`，2026-09-19～10-07，已删除）：当年设备上优化大书时内存峰值约等于文件体积（245MB 的书峰值 206MB），设备只有约 2GB 内存，同时点几本大部头有 OOM 风险，于是按书的体积限并发（>90MB 同时 1 本、其余 3 本，排队最长 30 分钟，`/api/budget/*` 可看可取消）。2026-10-07 书架不再在设备上优化书，「加入 xochitl」是流式上传、book-serve 只多占几 MB，批量队列本来就一本一本来，闸门拦不到任何东西，整个删除；其中"等这本书处理完"的逻辑（连续 6 次失败才放弃）原样挪进了 `batch.rs`。闸门那条待办"两本大书真的串行"从未真机验证，随删除作废。完整代码用 `git log -- gateway/src/budget.rs` 找。
- **被打断重放的次数上限**（10-07 删）：此前每项记 `attempts`，连续两次没走完就记失败，防"某本书稳定触发崩溃 → 无限重放"。网关不读书的内容，崩溃元凶不会在网关，计数删掉。
- **代理流式**（09-24）：第一版流式下载借了 SSE 通道，真机上下载收不完；改成只流式有长度的应答，并把"大图片"也纳入。
- **按事件来源取数**（09-25）：此前任何 `books` 事件都全量重取；分档后大书处理时母版库每次事件的请求数从 6 个降到 3 个，闸门删除后再降到 2 个（统计口径是请求数，真机没数过）。
- **按标签懒加载**（09-24）：打开页面的请求从 33 个降到 7 个，逛完四个标签从 69 降到 35（当时的统计）。
- **事件按 `tab` 路由**（10-10）：此前按 `area` 找标签，靠巧合成立（见第 7 章 FE-6）。
- **10-10 重构**：网页脚本从 1516 行的单个 `app.js` 拆成 8 个源文件、编译期拼回（拆分提交核过拼接产物与原文件的非空行多重集合相同，只多了两处为"加载时不碰 DOM"做的改动）；接口 DTO 化前先写线上格式快照，改完逐字段相同（唯一差异是键序）；系统增强开关从散在 6 处的键名收成 `TOGGLES` 表；`refreshGate`、`liveStream` 把散落的时序标志收成对象，机制不变；母版库/笔记的派生规则（`stgClean`/`stgTitle`/`isBookDone`、"活条目"名单）改由后端下发；node 测试从"正则抠函数"改为整文件加载。将来要改成真正的 ES 模块（`<script type="module">`），文件已按"只定义不执行"写好，代价是首开多 7 个请求，暂不做。
- **i18n**：语言包 09-09 起、09-10 覆盖主界面全部正文；键数随功能增删变化（09-30 513、10-07 468、10-09 480、10-10 483）。

## 附录 B　已移除的功能

别再找，也别加回：

- **KOReader / WeRead / appload 相关**（设备 2026-09-29 卸载，网关 09-30 撤）：`koreader` 代理段（`/api/koreader/*` 现在回 404"未知服务"）、"加入 KOReader"批量动作、「其他」里的 KOReader 子标签、基石探测里的三项。
- **电池刺客（battop）与手写笔迹优化**（09-30）：`/api/enhance/battop/*`、`hwStrokeEnabled` 开关与对应子标签。
- **设备端「优化」、抓网文、原 PDF 备份**（2026-10-07，书架不再优化书，书在电脑上用 [sheng-ren](https://github.com/bbq191/sheng-ren) 优化好再传）：批量 `optimize` 动作（再传回 400）、`POST /staging/cancel`、优化等级与"PDF 来源"徽章、进度百分比。
- **并发/内存闸门**（10-07）：`budget.rs`、`GET /api/budget/status`、`POST /api/budget/cancel`、"取消排队"按钮与页头"排队 N 本"。
- **漫画页边距开关 `comicMinMargin`、日漫翻页规则 `rtlPageTurn`**（10-07）。
- **`/api/manage` 里没人读的 `installable`/`hasWeb`**（10-09）。

## 附录 C　旧章节号对照表

其他文档和代码注释里还写着 2026-10-10 重组前的章节号，按下表找新位置。

| 旧编号 | 旧标题 | 新位置 |
|---|---|---|
| 给新读者 / 现状 | 导读、术语、关键数字、真机验证状态 | 1、2、5.3、8 |
| §01 | 登录与安全 | 4.2 |
| §1.1 | HTTPS 与私有 CA | 4.2「HTTPS 与私有 CA」 |
| §1.2 | 密码、会话与首登必改 | 4.2「密码、会话与首登必改」 |
| §1.3 | 失败限速 | 4.2「失败限速（按来源 IP）」 |
| §02 | 反向代理与服务发现 | 4.3 |
| §2.1 | 服务表 `MODULES` | 4.3「服务表 `MODULES`」 |
| §2.2 | 路由与代理行为 | 4.3「路由与代理行为」 |
| §2.3 | 管理台与基石探测 | 4.3「管理台与基石探测」 |
| §03 | 事件推送（含「事件 area/kind 总表」） | 4.4（总表标题不变） |
| §04、§4.1、§4.3 | 批量队列 | 4.5 |
| §4.2 | 闸门（已删除） | 附录 A「并发/内存闸门」 |
| §05、§5.1 | 网页 UI：打包、取数时机、「界面规则」、前端修复 | 4.6（「界面规则」标题不变）；逐批修复记录压缩进第 7 章与附录 A |
| §5.2 | 母版库页 | 4.6「母版库页」 |
| §5.3 | 前后端契约 | 4.6「前后端契约」 |
| §06 | 系统增强接口 | 4.7 |
| §06b | 设备健康、OTA 横幅与遗留清理 | 4.8 |
| §07 | 构建、部署与 systemd | 6、5.2 |
| §08 | 踩坑与教训 | 7 |
| §09 | 命名遗留与待办 | 7「已知限制」（命名遗留）、8「待手测清单」 |
| 附｜来历 | 正名搬顶层 | 附录 A |
| 更早的 §00b / §03b / §03c / 旧 §04 / 旧 §05 | 2026-09 初的编号 | §00b → 2、8；§03b → 4.5、4.6「母版库页」；§03c → 4.2、4.3、4.6、4.7；旧 §04 踩坑 → 7；旧 §05 待办 → 8 |
