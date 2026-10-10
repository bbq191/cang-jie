# gateway —— 设备端 Web 网关

**一句话**：设备上那个"秘密花园"网页（`https://shelf.local/`）就是它提供的。它是书架、笔记、系统增强三条线共用的唯一对外入口：一个网址、一次登录，后面只听本机的小服务都躲在它身后。

- **给谁看**：想知道网页是谁提供的、请求怎么走到各个服务、怎么构建和测试的人。
- **怎么开始**：不用单独装，`packaging/install-all.sh` 或 `packaging/deploy.sh` 会连它一起装（见 [`docs/INSTALL.md`](../docs/INSTALL.md)）。设计细节看[白皮书](docs/reMarkable网关白皮书.md)，整体位置看 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

![网关与各服务](docs/diagrams/request-flow.svg)

## 它做什么

设备上的书架、字体、壁纸、笔记四件套等服务都只听 `127.0.0.1`，外面碰不到。网关站在最前面：浏览器只认识它，转发、批量排队、登录限速由它管；它自己不做"书 / 笔记 / 字体"的业务。

| 职责 | 代码 | 一句话 |
|---|---|---|
| 托管网页 | `ui/`、`src/ui.rs` | 8 个脚本、样式、中英文语言包编译期拼成一个零外链的单文件页面 |
| HTTPS 与登录 | `src/auth.rs`、`src/config.rs` | 私有 CA（带名称约束）签的证书；只要密码；首登必改；输错按来源 IP 限速；脚本用 Basic |
| 服务发现 + 反向代理 | `src/manage.rs`、`src/proxy.rs` | `MODULES` 表是唯一事实源；`/api/<seg>/*` 剥掉 `<seg>` 转给对应服务；大上传、大下载边读边转 |
| 事件汇聚 | `src/events.rs` | 各服务的 `GET /events` 汇成一条 SSE `/api/events`，补上 `svc` 与 `tab`，网页按 `tab` 只刷对应标签，不轮询 |
| 批量队列 | `src/batch.rs` | 勾选多本后网关后台一本一本「加入 xochitl」，状态落盘、重启续跑、可全部中止；只有书架回报 `ok` 才算成功 |
| 管理台 | `src/manage.rs` | 各服务"未装 / 已装未开 / 已开"三态、启停、卸载；探测 xovi 与 qt-resource-rebuilder |
| 系统增强开关 | `src/enhance/` | 开关表 `TOGGLES`（荧光笔汉字吸附、单击翻页、导入 md），并告诉网页扩展是否真的加载进 xochitl（`toggles[].loaded`） |
| 设备健康 / OTA / 清理 | `src/device/` | 「管理 → 设备健康」五个二级标签，只在打开或刷新时采集；OTA 后页头提示重装；清理早期遗留文件与书库同名副本（失败项 `{name,message}`，部分失败标 `partial`） |

## 对外接口

**不用登录**：`GET /login` · `POST /login` · `POST /logout` · `GET /ca.crt`（下载私有 CA）· `GET /health` · `GET /favicon.ico`

**登录后（Cookie 或 Basic）**：

| 类别 | 接口 |
|---|---|
| 页面 | `GET /` · `GET /ui/locales/{lang}` · `GET/POST /password` · `GET /api/session` |
| 服务发现 / 管理 | `GET /api/services`（每项带 `seg`）· `GET /api/manage` · `GET /api/foundation` · `POST /api/manage/{seg}/{start\|stop\|uninstall}` |
| 事件 | `GET /api/events`（SSE） |
| 批量队列 | `POST /api/batch {action:"deliver", names? \| all:true, folder?}` → `{queued, skipped}` · `GET /api/batch/status` · `POST /api/batch/stop` |
| 系统增强 | `GET /api/enhance/status`（`toggles:[{key,on,kind,loaded}]`）· `PUT /api/enhance/qol` |
| 设备健康 | `GET /api/device/health[?fresh=1]` · `GET /api/device/ota` · `GET /api/device/wifi` · `GET /api/device/cleanup` · `POST /api/device/cleanup/delete {area, names}` → `{deleted, failed, partial}` |
| 反向代理 | `GET/POST/PUT/DELETE /api/<seg>/*`，`<seg>` ∈ `books` `fonts` `wallpapers` `ink` `transcribe` `mind` `notes` |

登录规则：默认密码 `shelf`，首次登录必须改（≥6 位）；同一 IP 60 秒内输错 5 次锁这个 IP，登录口回 429、Basic 请求回 401。详见[白皮书 4.2](docs/reMarkable网关白皮书.md#42-登录与安全)。

子命令：`gateway serve [--bind]`（部署固定 `0.0.0.0:443`）· `passwd <新密码>` · `reset-password`（回默认密码并强制改）· `regen-tls`（只重签服务器证书，CA 不变）。改密码类命令要重启网关生效。

## 接入一个新服务

1. 服务用 `rmsvc_core::service::run_or_exit` 启动（自动登记注册表、自带 `/health`）。
2. 在 `src/manage.rs` 的 `MODULES` 加一行（URL 段、服务名、安装令牌、兜底显示名、有没有 `/events`、事件唤醒谁、事件归哪个标签）。
3. 语言包加 `manage.modules.label.<seg>`。

代理只认服务名，不关心源码在哪条线的目录里。

## 构建、部署、测试

- **依赖**：只依赖 [`../rmsvc-core`](../rmsvc-core/README.md)（打开 `gateway` feature：HTTPS、私有 CA、登录、mDNS 只有网关用）；独立 Cargo 项目，不在任何 workspace 里。
- **构建/部署**：没有自己的脚本，由 `shelf/build.sh`（顺手编译本目录）和 `packaging/deploy.sh`（打包二进制 + `systemd/gateway.service`）代管。单独交叉编译：`cargo build --release --target aarch64-unknown-linux-musl`（用本目录 `.cargo/config.toml` 的 CC/AR 覆盖）。网关要和各服务**一起部署**（网页依赖服务新下发的字段）。
- **测试**（在仓库根目录跑；2026-10-10 实跑）：
  - `cargo test --locked --manifest-path gateway/Cargo.toml`：77 个（含 6 个接口线上格式快照、动态 i18n 键覆盖）；
  - `node --test gateway/ui/test/*.test.mjs`：6 个文件共 19 项（core 纯函数与笔记刷新闸门、拼接产物可解析且无加载期副作用、断网兜底与 401/403 去向、XSS 转义、语言包一致、低空间判据）；`for f in gateway/ui/*.js; do node --check "$f"; done`；
  - 浏览器冒烟（手动，不进 CI）：`PUPPETEER_NODE_MODULES=<含 puppeteer 的 node_modules 目录> node gateway/ui/test/smoke.puppeteer.mjs`；
  - 可视走查：[`tools/screenshot-walkthrough/`](tools/screenshot-walkthrough/README.md)。
- **设备上的文件**：二进制 `~/.local/bin/gateway`；配置 `~/.config/shelf/gateway.json`；证书 `~/.config/shelf/tls/`；批量队列 `~/.local/state/shelf/batch.json`。

## 部署与验证现状

- 2026-10-09 第六轮审计的改动 10-09 已部署（`install-all`，整机重启后部署自检 38✓），**功能还没在真机上手测**。
- 2026-10-10 重构（网页脚本拆成 8 个源文件、接口 DTO 化、`TOGGLES`、事件按 `tab` 路由、后端下发派生字段）**未部署、未在真机上验证**，只有 host 测试。
- 完整的验证表与待手测清单见[白皮书第 8 章](docs/reMarkable网关白皮书.md#8-验证现状)。

## 目录

```
src/
  main.rs      入口、子命令、路由注册
  auth.rs      登录守卫（Cookie/Basic）、首登必改、按 IP 限速
  config.rs    gateway.json：HTTPS 开关、密码哈希、mDNS 名、额外证书名、会话天数
  proxy.rs     /api/<seg>/* 反向代理
  manage.rs    MODULES 服务表（唯一事实源）、管理台、基石探测
  events.rs    事件汇聚（补 svc/tab；网关自发事件的 area/kind/tab 常量）
  batch.rs     批量队列（逐本加入 xochitl、落盘续跑、全部中止）
  ui.rs        拼装单页 UI、登录页、改密页
  failed.rs    几个接口共用的失败项 {name,message}
  enhance/     系统增强开关（TOGGLES 表、reading-qol.json、扩展加载检测）
  device/      设备健康（health）、OTA 横幅（ota）、遗留清理（cleanup）
ui/            index.html、style.css、auth.css、locales/、test/；
               脚本 core / dom / transfer / notes / assets / manage / health / app.js（编译期按此顺序拼回一个 <script>）
systemd/gateway.service
tools/screenshot-walkthrough/   前端截图走查工具
```

## 命名遗留（刻意不动）

`~/.config/shelf/`、`$XDG_RUNTIME_DIR/shelf/`、`shelf.target`、默认密码 `shelf`、mDNS 名 `shelf.local`、Cookie `shelf_session` 仍叫 "shelf"：它们是已部署设备上的真实路径和配置，改名需要专门的迁移方案。网关本身 2026-09-11 从 `shelf/services/shelf-gateway` 正名搬到顶层，来历见白皮书附录 A。
