# gateway —— 设备端 Web 网关

**一句话**：设备上那个“秘密花园”网页（`https://shelf.local/`）就是它提供的。它是书架、笔记、系统增强三条线共用的唯一对外入口：一个网址、一次登录，后面的小服务都躲在它身后。
**给谁看**：想知道网页是谁提供的、请求怎么走到各个服务、怎么构建部署的人。**怎么开始**：不用单独装，`packaging/install-all.sh` 或 `packaging/deploy.sh` 会连它一起装（见下文「构建、部署、测试」）；设计与踩坑看 [`docs/reMarkable网关白皮书.md`](docs/reMarkable网关白皮书.md)（开头有「给新读者」导读），整体位置看 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

## 它是什么

设备上有好几个各管一摊的小服务（书架、字体、壁纸、笔记四件套），它们都只听本机 `127.0.0.1`，外面直接碰不到。
网关是站在最前面的“前台”：**浏览器只需要认识它——一个网址（`https://shelf.local/`）、一次登录**，转发、批量排队、登录限速它来管。它自己不做“书 / 笔记 / 字体”的业务。
它是 `shelf/`、`notes/`、`enhance/` 三条线共用的唯一对外入口。

![请求怎么走](docs/diagrams/request-flow.svg)

| 职责 | 代码 | 一句话 |
|---|---|---|
| 托管单页 UI | `ui/`、`ui.rs` | 页面文件编译期打进二进制，零外链；中英文语言包在 `ui/locales/` |
| HTTPS 与登录 | `auth.rs`、`config.rs` | 私有 CA 签的证书 + 只要密码的登录页；命令行用 Basic（验过的密码缓存 10 分钟，免每次 60 万轮 PBKDF2）；输错按来源 IP 限速 |
| 服务发现 + 反向代理 | `manage.rs`、`proxy.rs` | `/api/services` 列出在跑的服务、每项带 URL 段 `seg`（网页据此出标签页）；`/api/<seg>/*` 剥掉 `<seg>` 转给对应服务；上传边读边转，有长度的下载/大应答（>256KB）边读边发；GET/DELETE 不转发请求体也不带 `Content-Length`；只设空闲超时（连 3 秒 / 读 900 秒 / 写 120 秒，不设总时长，大文件慢传不被截断），共用一个连接池；只透传 `Content-Type`、`Content-Disposition` 和 200 时的 `Cache-Control`；服务没起 → 404，网页隐藏对应标签 |
| 事件汇聚 | `events.rs` | 各服务的 `GET /events` 汇成一条 SSE `/api/events`，网页不轮询；前台 tab 按事件来源只重取受影响的那几个接口（白皮书 §5.1）；浏览器放弃重连时网页自己退避重开（§03）；服务上下线靠基座的 `registry_wake`（与订阅线程共用一条 inotify） |
| 批量队列 | `batch.rs` | 勾选多本后由网关后台一次一本地“加入 xochitl”（等这本在书架那边处理完才取下一本；网页的加入只走这里），状态落盘、重启从被打断那本续跑（等 book-serve 期间 `waitingService=true`）、可全部中止（已交给书架的那本会跑完）。只有书架回报 `ok` 才算成功，超时、查不到、结果不明一律记失败并提示去 xochitl 书库核对。原来的并发/内存闸门 `budget.rs` 2026-10-07 删除：书架不再优化书后它已拦不到东西，见白皮书 §04 |
| 管理台 | `manage.rs` | 各服务“未装 / 已装未开 / 已开”三态、启停、卸载；探测 xovi 与 qt-resource-rebuilder |
| 系统增强开关 | `enhance/` | 荧光笔汉字吸附、阅读器单击翻页、导入 md；并显示扩展是否真的加载进 xochitl（与设备健康共用 maps 解析，`(deleted)` 的映射照样算已加载）。已移除：手写笔迹优化、电池刺客（2026-09-30），漫画页边距开关、日漫翻页规则（2026-10-07） |
| 设备健康 / OTA 提示 / 清理 | `device/` | 「管理 → 设备健康」五个二级 tab（概览 / 服务 / 扩展 / 日志 / 清理），只在打开或点刷新时采集；OTA 后页头横幅提示重装；清理早期遗留文件与 xochitl 书库同名副本（后者进 xochitl 回收站，可恢复） |

## 对外接口

**不用登录**：`GET /login` · `POST /login` · `POST /logout` · `GET /ca.crt`（下载私有 CA 装信任）· `GET /health`

**登录后（Cookie 或 Basic）**：

| 类别 | 接口 |
|---|---|
| 页面 | `GET /` · `GET /ui/locales/{lang}` · `GET/POST /password` · `GET /api/session` |
| 服务发现 / 管理 | `GET /api/services`（每项带 `seg`）· `GET /api/manage` · `GET /api/foundation` · `POST /api/manage/{seg}/{start\|stop\|uninstall}` |
| 事件 | `GET /api/events`（SSE） |
| 系统增强 | `GET /api/enhance/status` · `PUT /api/enhance/qol`（`hlSnapCjk` / `notesImportMdEnabled` / `tapPageTurn`）（已移除的 `hwStrokeEnabled`、`comicMinMargin`、`rtlPageTurn` 单独传回 400；`/api/enhance/battop/*` 2026-09-30 删） |
| 设备健康 | `GET /api/device/health[?fresh=1]` · `GET /api/device/ota` · `GET /api/device/wifi` · `GET /api/device/cleanup` · `POST /api/device/cleanup/delete {area, names}` |
| 批量队列 | `POST /api/batch {action: deliver, names? \| all:true, folder?}`（`optimize` 等旧动作回 400） → `{queued, skipped}` · `GET /api/batch/status`（`running` `waitingService` `action` `total` `done` `current` `queued` `failed`） · `POST /api/batch/stop` |
| 反向代理 | `GET/POST/PUT/DELETE /api/<seg>/*`，`<seg>` ∈ `books` `fonts` `wallpapers` `ink` `transcribe` `mind` `notes` |

（`GET /api/budget/status`、`POST /api/budget/cancel` 2026-10-07 随闸门删除。）

登录规则一句话：默认密码 `shelf`，首次登录必须改（≥6 位）；同一 IP 60 秒内输错 5 次锁这个 IP，登录口回 429、Basic 请求回 401。详见白皮书 §01。

子命令：`gateway serve [--bind]`（部署固定 `0.0.0.0:443`）· `passwd <新密码>` · `reset-password`（回默认密码并强制改）· `regen-tls`（只重签服务器证书，CA 不变）。改密码类命令要重启网关生效。

> **部署状态**：2026-10-09 第六轮审计的改动（确认框焦点在「否」时按 Enter 误执行、认证配置锁被 panic 毒化后谁都登不进、`/api/services` 带 `seg`、`lowSpace` 优先读后端、页面隐藏时不取数等）**10-09 已部署**（`install-all`，整机重启后部署自检 38✓），同日稍后的审计后续（批量投递取 `names` 改用基座 `opt_str_list`、网页发模型单价改用 `inputPer1k`/`outputPer1k`）随下一次部署上机；这些功能都**还没在真机上手测**，手测清单见白皮书 §09。
> KOReader 相关的代理、批量动作和网页入口 2026-09-30 已全部撤掉（设备 09-29 卸载 KOReader）。

## 接入一个新服务

1. 服务用 `rmsvc_core::service::run` 启动（自动注册、自带 `/health`）。
2. 在 `src/manage.rs` 的 `MODULES` 加一行（URL 段、服务名、安装令牌、显示名、有没有 `/events`）。

代理只认服务名，不关心源码在哪条线的目录里。

## 构建、部署、测试

- **依赖**：只依赖 [`../rmsvc-core`](../rmsvc-core/README.md)（HTTP 服务器与路由、注册表、事件、TLS、鉴权原语，以及 2026-10-09 起的 `proc`/`config`/`http` 小工具和测试沙箱 `Paths::sandbox`）；独立 Cargo 项目，不在任何 workspace 里。
- **构建/部署**：没有自己的脚本，由 `shelf/build.sh`（顺手编译本目录）和 `packaging/deploy.sh`（打包二进制 + `systemd/gateway.service`）代管。单独交叉编译：`cargo build --release --target aarch64-unknown-linux-musl`（用本目录 `.cargo/config.toml` 的 CC/AR 覆盖）。
- **测试**（在仓库根目录跑；2026-10-09 实跑）：`cargo test --manifest-path gateway/Cargo.toml`（62 个）；前端 `node --check gateway/ui/app.js` 与 `node --test gateway/ui/test/*.test.mjs`（4 个文件共 13 项：断网兜底与 401/403 去向、XSS 转义、语言包一致、低空间判据）；浏览器冒烟手动跑 `PUPPETEER_NODE_MODULES=<含 puppeteer 的 node_modules 目录> node gateway/ui/test/smoke.puppeteer.mjs`；可视走查见 [`tools/screenshot-walkthrough/`](tools/screenshot-walkthrough/README.md)。
- **设备上的文件**：二进制 `~/.local/bin/gateway`；配置 `~/.config/shelf/gateway.json`；证书 `~/.config/shelf/tls/`；批量队列 `~/.local/state/shelf/batch.json`。

## 目录

```
src/
  main.rs      入口、子命令、路由注册
  auth.rs      登录守卫（Cookie/Basic）、首登必改、按 IP 限速
  config.rs    gateway.json：HTTPS 开关、密码哈希、mDNS 名、额外证书名、会话天数（读写走 rmsvc-core 的 config 模板）
  proxy.rs     /api/<seg>/* 反向代理
  manage.rs    MODULES 服务表（唯一事实源）、管理台、基石探测
  events.rs    事件汇聚（spawn 返回总线；Hub 结构 2026-10-07 删除）
  batch.rs     批量队列（含全部中止、等一本处理完的轮询）
  ui.rs        拼装单页 UI、登录页、改密页
  enhance/     系统增强开关（qol / loaded）
  device/      设备健康（health）、OTA 横幅（ota）、遗留清理（cleanup）
ui/            index.html、style.css、app.js、auth.css、locales/、test/（前端取数时机与界面规则见白皮书 §05）
systemd/gateway.service
tools/screenshot-walkthrough/   前端截图走查工具
```

## 命名遗留（刻意不动）

`~/.config/shelf/`、`$XDG_RUNTIME_DIR/shelf/`、`shelf.target`、默认密码 `shelf`、mDNS 名 `shelf.local` 仍叫 “shelf”：它们是已部署设备上的真实路径和配置，改名需要专门的迁移方案。2026-09-11 起网关本身从 `shelf/services/shelf-gateway` 正名搬到顶层，来历见白皮书末尾。
