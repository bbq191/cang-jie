# gateway —— 设备端 Web 网关

> **读者与用途**：想弄清"我在浏览器里看到的那个网页是谁提供的、请求怎么走到各个服务、批量和并发是谁管的"的人。
> 它是 `shelf/`、`notes/`、`enhance/` 三条线共用的**唯一对外入口**。整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)；
> 决策记录与踩坑见 [`docs/reMarkable网关白皮书.md`](docs/reMarkable网关白皮书.md)。

## 它是什么

设备上有好几个各管一摊的小服务（书架、KOReader、字体、壁纸、笔记四件套……），它们都只听本机 `127.0.0.1`。网关是站在最前面的那一个：
你的浏览器只需要认识它——**一个网址、一次登录**，剩下的事它来转发。

## 职责

1. **托管单页 UI**：`ui/` 下 `index.html`/`style.css`/`app.js`/`auth.css` 是真文件，编译期 `include_str!` 拼进二进制；`ui/locales/{zh-CN,en-US}.json` 是 i18n 语言包。
2. **服务发现与反向代理**：`/api/services` 列服务注册表；`/api/<service>/*` 反向代理到该服务的 loopback 端口（请求体流式转发、剥掉 `<service>` 段）。服务缺席 → 404「未安装」，UI 据 `/api/services` 隐藏对应 tab。
3. **批量队列**（`batch.rs`，2026-09-20）：勾选多本书后，网关后台线程顺序逐本执行「优化 / 加入 xochitl / 加入 KOReader」，状态落盘、重启续跑、可全部中止。
4. **并发/内存预算闸门**（`budget.rs`）：设备约 2GB 内存，所有"会占内存的操作"先过这一道闸——>90MB 大档同时 1 个、≤90MB 小档同时 3 个，排队最长 30 分钟、可取消。
5. **事件汇聚**（`events.rs`）：各服务 `GET /events` 汇聚成 `/api/events`（SSE），网页据此即时刷新。
6. **系统增强开关的网页控制面**（`enhance/`）：荧光笔吸附、手写笔锋、电池刺客等的薄客户端。

对外 **HTTPS**（私有 CA 签发，首启生成，`/ca.crt` 可下载装信任）+ **登录页密码**（无用户名；首次默认 `shelf`，登录后必改；CLI 用 Basic）+ **mDNS `shelf.local`** 伪域名。部署固定 `0.0.0.0:443`。
子命令：`serve [--bind]` · `passwd <新密码>` · `reset-password`（回默认并强制改）· `regen-tls`（重签叶证书）。

![批量队列状态机](../docs/diagrams/batch-queue.svg)

![并发/内存预算闸门](../docs/diagrams/budget-gate.svg)

## 网关自身的端点

`GET /api/services` · `GET /api/manage`（管理台三态）· `GET /api/foundation`（基石探测）· `POST /api/manage/{seg}/{start|stop|uninstall}` · `GET /api/events`（SSE）· `GET /ui/locales/{lang}` · `/login` `/logout` `/password` `/ca.crt` `/api/session`；
系统增强：`GET /api/enhance/status` · `PUT /api/enhance/qol` · `POST /api/enhance/battop/{start|stop}` · `GET /api/enhance/battop/summary`；
**批量**：`POST /api/batch {action: optimize|deliver|koreader, names?|all:true, folder?}` · `GET /api/batch/status` · `POST /api/batch/stop`；
**闸门**：`GET /api/budget/status`（排队中/处理中的书名）· `POST /api/budget/cancel {name}`（只对还在排队的生效）。
其余 `/api/<seg>/*` 一律按 `manage.rs` 的 `MODULES` 转发到对应服务。

## 目录

```
src/
  main.rs      入口 + 子命令 + 路由注册 + ServiceSpec（name: "gateway"）
  auth.rs      密码策略 + 会话 + Basic 认证
  config.rs    gateway.json：HTTPS 开关、密码哈希、首登必改、mDNS 名、额外 SAN
  proxy.rs     /api/<seg>/* 反向代理
  manage.rs    URL 段 ↔ 服务的 MODULES 单一事实源（管理台三态/代理/status 都从它派生）
  events.rs    Hub：汇聚各服务事件，/api/events 对网页 SSE
  batch.rs     服务端批量队列（顺序执行、状态落盘、resume 续跑、stop）
  budget.rs    并发/内存预算闸门（大档 1 / 小档 3，排队超时 30 分钟，可取消）
  ui.rs        单页 UI 与登录/改密页的拼装（include_str! + 格式白名单注入）
  enhance/     系统增强开关后端（mod/qol/battop.rs）——薄客户端，实际工具源码分别在 enhance/{hl-snap,handwriting-stroke,battop}/、notes/
ui/            单页前端（index.html/style.css/app.js/auth.css + locales/）
systemd/gateway.service   开机单元（PartOf=shelf.target；shelf.target 名字暂未跟着改，见下）
```

## 依赖

只依赖顶层 `../rmsvc-core`（共享基座）——不依赖 `shelf/crates/bookconv`、不依赖 `notes/` 任何 crate。
`shelf/Cargo.toml` 的内部 workspace **不再包含本目录**，是完全独立的顶层 Cargo 项目。

## 构建 / 部署

自己没有独立的 `build.sh`/`deploy.sh`——跟 `notes/` 一样，被 `shelf/build.sh`（顺手 `cd ../gateway &&
cargo build`）和 `shelf/deploy.sh`（打包 `../gateway/target/.../gateway` 二进制 + `../gateway/systemd/
gateway.service`）代管，因为它历史上就是跟着书架整包一起装的，这次正名只是挪了源码位置，没有另起
一套独立的安装流程。真要单独重编：`cargo build --release --target aarch64-unknown-linux-musl`（需要
本目录 `.cargo/config.toml` 的 CC/AR 覆盖，跟 `shelf/`、`notes/` 同一份）。

## 工具

`tools/screenshot-walkthrough/`——前端可视渲染走查（2026-09-16 新增）：起 `book-serve`/
`ink-serve`/`note-serve`/`gateway` 四个服务 + 灌 fixture 数据 + Playwright 登录中英文各切一遍
tab、全页截图，补"改完前端反复迭代却从没人眼看过浏览器实际渲染"这个长期缺口（不是自动化断言
测试，截图仍要人看；第一次真跑就抓到一个真实前端 bug，见该目录 README「已知局限」末尾）。

## 命名遗留

XDG 运行时注册表路径、`shelf.target` systemd 目标、登录默认密码字面量 `shelf`、mDNS 域名
`shelf.local` 这几处**仍然叫 "shelf"**——这次重构只改了 crate/二进制/systemd 单元这一层的名字，
没有动这些会牵连已部署设备真实路径/配置的更深层命名，是刻意留白，不是遗漏。

## 文档

决策记录/踩坑/正名搬迁的完整过程见 [`docs/reMarkable网关白皮书.md`](docs/reMarkable网关白皮书.md)。

## 历史：为什么它在顶层

2026-09-11 从 `shelf/services/shelf-gateway` 正名搬到顶层——它托管的网页早就不只是"书架"了（笔记 tab、系统增强/实验室/电池刺客开关都挂在这一个网关下面，网页总标题也已经是"秘密花园"而不是"书架"），继续叫 `shelf-gateway`、放在 `shelf/` 里，跟它的实际定位不符。
