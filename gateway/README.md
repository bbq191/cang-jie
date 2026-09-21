# gateway —— 设备端 Web 网关

> **读者与用途**：想弄清“我在浏览器里看到的那个网页是谁提供的、请求怎么走到各个服务、批量和并发是谁管的”的人。
> 它是 `shelf/`、`notes/`、`enhance/` 三条线共用的**唯一对外入口**。整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)；
> 决策记录与踩坑见 [`docs/reMarkable网关白皮书.md`](docs/reMarkable网关白皮书.md)。

## 一句话：它是什么

设备上有好几个各管一摊的小服务（书架、KOReader、字体、壁纸、笔记四件套……），它们都只听本机 `127.0.0.1`，外面直接碰不到。
网关是站在最前面的那一个“前台”：**你的浏览器只需要认识它——一个网址（`https://shelf.local/`）、一次登录**，剩下的转发、排队、限流它来管。
它自己不做任何“书/笔记/字体”业务，只做下面几件事。

## 它能做什么

| # | 职责 | 在哪 | 一句话 |
|---|---|---|---|
| 1 | 托管单页 UI | `ui/`、`ui.rs` | `index.html/style.css/app.js/auth.css` 编译期 `include_str!` 进二进制（零外链单文件）；`ui/locales/{zh-CN,en-US}.json` 是中英文语言包 |
| 2 | 登录与 HTTPS | `auth.rs`、`config.rs` | 私有 CA 签的证书 + 登录页密码（无用户名）+ mDNS 名 `shelf.local`；CLI 走 Basic |
| 3 | 服务发现 + 反向代理 | `manage.rs`、`proxy.rs` | `/api/<seg>/*` 剥掉 `<seg>` 转给对应服务的本机端口；服务没起 → 404，网页据 `/api/services` 隐藏对应 tab |
| 4 | 事件汇聚 | `events.rs` | 各服务的 `GET /events` 汇成一条 SSE `/api/events`，网页零轮询 |
| 5 | 并发/内存预算闸门 | `budget.rs` | 设备只有约 2GB 内存：>90MB 大文件同时只 1 个、≤90MB 同时 3 个，排队最长 30 分钟，可取消 |
| 6 | 批量队列 | `batch.rs` | 勾选多本书后网关后台线程顺序逐本处理，状态落盘、重启续跑、可全部中止 |
| 7 | 管理台 + 基石探测 | `manage.rs` | 各服务“未装 / 已装未开 / 已开”三态、启停、卸载；探测 xovi/appload/KOReader/WeRead 装没装 |
| 8 | 系统增强开关的网页面板 | `enhance/` | 荧光笔汉字吸附、手写笔迹优化、导入 md 入口、漫画页边距最小化、电池刺客启停——都是“薄客户端” |

子命令：`serve [--bind]`（部署固定 `0.0.0.0:443`）· `passwd <新密码>` · `reset-password`（回默认密码 `shelf` 并强制改）· `regen-tls`（删叶证书，下次启动用同一 CA 重签）。

## 怎么被三条产品线复用

```
shelf/      book-serve   koreader-serve                 ┐
enhance/    font-serve   wallpaper-serve                ├─ 各自只听 127.0.0.1:<端口>，启动时向注册表登记
notes/      ink-serve  transcribe-serve  mind-serve  note-serve ┘
                     ▲ 全部经 gateway 的 /api/<seg>/* 对外；接入 = 在 manage.rs 的 MODULES 加一行
```

新服务接入只需两步：服务用 `rmsvc-core::service::run` 启动（自动注册、自带 `/health`），再在 `manage.rs::MODULES` 加一行（URL 段、服务名、安装令牌、显示名、有没有 `/events`）。
代理只认服务名字符串，不关心源码在哪条产品线的目录里。

![请求怎么走](docs/diagrams/request-flow.svg)

## 对外接口一览

**公开（不需要登录）**：`GET /login` · `POST /login` · `POST /logout` · `GET /ca.crt`（下载私有 CA 装信任）· `GET /health`

**登录后（Cookie 或 Basic）**：

| 类别 | 接口 |
|---|---|
| 页面 | `GET /`（单页 UI）· `GET /ui/locales/{lang}`（不认识的语言落中文）· `GET/POST /password` · `GET /api/session` |
| 服务发现/管理 | `GET /api/services`（注册表）· `GET /api/manage`（管理台三态）· `GET /api/foundation`（基石探测）· `POST /api/manage/{seg}/{start\|stop\|uninstall}` |
| 事件 | `GET /api/events`（SSE） |
| 系统增强 | `GET /api/enhance/status` · `PUT /api/enhance/qol`（body 给 `hlSnapCjk`/`hwStrokeEnabled`/`notesImportMdEnabled`/`comicMinMargin` 之一）· `POST /api/enhance/battop/{start\|stop}` · `GET /api/enhance/battop/summary` |
| 批量队列 | `POST /api/batch {action: optimize\|deliver\|koreader, names?\|all:true, folder?}` → `{queued, skipped}` · `GET /api/batch/status` · `POST /api/batch/stop` |
| 并发闸门 | `GET /api/budget/status`（排队中 / 处理中的书名）· `POST /api/budget/cancel {name}`（只对还在排队的生效） |
| 兜底代理 | `GET/POST/PUT/DELETE /api/<seg>/*`，`<seg>` 见 `manage.rs::MODULES`：`books` `koreader` `fonts` `wallpapers` `ink` `transcribe` `mind` `notes` |

## 登录与限速（要点）

- 网页登录后拿 Cookie `shelf_session`（HttpOnly，HTTPS 下 Secure，SameSite=Strict；令牌只存内存，重启网关全部下线；有效期 30 天）；CLI 用 `Authorization: Basic`，用户名随意、只认密码。
- 首次启动默认密码 `shelf` 且**必须先改**（≥6 位、不能是 `shelf`）；改完之前只放行 `/password`，API 回 403。
- 密码哈希是 PBKDF2-HMAC-SHA256 60 万轮；60 秒内连续输错 5 次锁 60 秒（全局计数）。**锁定期表现不同**：`POST /login`、`POST /password` 回 **429** + `Retry-After`；带 Basic 头的普通请求锁定期回 **401**（等同没认出身份）。
- 详细判定流程见白皮书 §00b 的图，或直接看 `src/auth.rs`。

![登录鉴权](docs/diagrams/auth-flow.svg)

## 批量队列与并发闸门（要点）

- **闸门**：`优化` / `加入 xochitl` / `加入 KOReader` 三个吃内存的 POST，不管是单条点击还是批量，都在网关先按文件体积过闸（`budget::admit`）。
- **批量**：`POST /api/batch` 入队后由网关后台线程**一次一本**顺序执行，每本也过闸；状态写 `~/.local/state/shelf/batch.json`，网关重启后 `resume` 续跑；处理途中崩溃的那本最多重放一次。

![批量队列状态机](docs/diagrams/batch-queue.svg)

![并发/内存预算闸门](../docs/diagrams/budget-gate.svg)

## 目录

```
src/
  main.rs      入口 + 子命令 + 路由注册 + ServiceSpec（name: "gateway"）
  auth.rs      登录策略（Cookie/Basic）+ 首登必改 + 失败限速
  config.rs    gateway.json：HTTPS 开关、密码哈希、首登必改、mDNS 名、额外 SAN、会话天数
  proxy.rs     /api/<seg>/* 反向代理 + 三个吃内存操作的闸门拦截
  manage.rs    MODULES：URL 段 ↔ 服务名 ↔ 安装令牌 的单一事实源（管理台/代理/status 都从它派生）
  events.rs    Hub：汇聚各服务事件，对网页 SSE
  batch.rs     服务端批量队列
  budget.rs    并发/内存预算闸门
  ui.rs        单页 UI 与登录/改密页的拼装
  enhance/     系统增强开关后端（mod/qol/battop.rs）
ui/            单页前端（index.html/style.css/app.js/auth.css + locales/ + test/ 冒烟与 XSS 测试）
systemd/gateway.service   开机单元（PartOf=shelf.target；CPUWeight=20/MemoryMax=192M/Nice=5）
tools/screenshot-walkthrough/   前端可视渲染走查（起服务+灌 fixture+Playwright 截图，不是断言测试）
```

配置在 `~/.config/shelf/gateway.json`、TLS 材料在 `~/.config/shelf/tls/`（路径仍叫 `shelf`，见下方“命名遗留”）。

## 依赖 / 构建 / 测试

- 只依赖 [`../rmsvc-core`](../rmsvc-core/README.md)（共享基座）——不依赖 `shelf/crates/bookconv`，不依赖 `notes/` 任何 crate；不在任何 workspace 内，是独立顶层 Cargo 项目。
- 没有独立 `build.sh`/`deploy.sh`：由 `shelf/build.sh`（顺手 `cd ../gateway && cargo build`）和 `shelf/deploy.sh`（打包二进制 + `systemd/gateway.service`）代管。单独重编：`cargo build --release --target aarch64-unknown-linux-musl`（需要本目录 `.cargo/config.toml` 的 CC/AR 覆盖）。
- 测试：`cargo test --manifest-path gateway/Cargo.toml`（当前 34 个 `#[test]`，随代码增长）；前端 `node --check ui/app.js`。
- 前端走查工具见 [`tools/screenshot-walkthrough/README.md`](tools/screenshot-walkthrough/README.md)。

## 命名遗留（刻意不动）

XDG 运行时/配置路径（`~/.config/shelf/`、`$XDG_RUNTIME_DIR/shelf/`）、`shelf.target` systemd 目标、登录默认密码字面量 `shelf`、mDNS 域名 `shelf.local` 这几处**仍然叫 “shelf”**——它们牵连已部署设备的真实路径/配置，改名需要专门的迁移方案，是刻意留白，不是遗漏。

## 历史

2026-09-11 从 `shelf/services/shelf-gateway` 正名搬到顶层：它托管的网页早就不只是“书架”了（笔记 tab、系统增强开关都挂在这一个网关下，网页标题也已是“秘密花园”）。决策记录见白皮书 §01。
