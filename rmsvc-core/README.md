# rmsvc-core —— reMarkable 设备端 Web 服务共享底座

> **读者与用途**：要在 `shelf/`、`notes/`、`enhance/`、`gateway/` 里写/改 Web 服务的开发者。这是它们共用的**基础库**（路径、服务注册、HTTP 适配、流式上传、事件总线、xochitl 注入等），不含任何“书/笔记”业务语义。
> 整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)；决策与踩坑见 [`docs/reMarkable设备端Web服务基座白皮书.md`](docs/reMarkable设备端Web服务基座白皮书.md)。

## 一句话：它是什么

设备上有十来个各管一摊的小 Web 服务（书架、KOReader、字体、壁纸、笔记四件套、网关……）。它们都要做同样的“杂活”：读 XDG 路径、启动时向注册表登记、收 HTTP 请求回 JSON、接收大文件上传、往 xochitl 里塞书、广播“该刷新了”。
这些杂活只写一份，就是 `rmsvc-core`。**服务只写自己的业务；地基永远只有这一份。**

## 怎么被四条线复用

![模块地图与消费方](docs/diagrams/module-map.svg)

- 消费方都用 `path = "…/rmsvc-core"`（相对路径深度取决于自己所在目录）依赖，**不建根 workspace**，各自的 `target/` 各管各的；本 crate 不知道任何消费方的存在（单向依赖，无环）。
- 谁在用：`shelf/services/{book,koreader}-serve` · `notes/services/{ink,transcribe,mind,note}-serve` + `notes/crates/vendorcfg` · `enhance/{font,wallpaper}-serve` · `gateway/`。
- 一个典型服务的骨架只有三步：`service::run(&SPEC, bind, &paths, router)`（自动建目录、自注册、挂 `/health`）→ 路由里用 `http::Router` 写处理函数 → 有变更就 `EventBus::publish` 通知网页。

## 提供什么（20 个模块）

| 分组 | 模块 | 一句话 |
|---|---|---|
| 服务骨架 | `service` | 启动模板：解析 `--bind` → 建目录 → 自注册 → 起服务器，自带 `GET /health` |
| | `registry` | 服务自注册/发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`），按 `/proc/<pid>` 清理陈旧条目；`SvcClient` 是跨服务 HTTP 客户端骨架 |
| | `http` | tiny_http 适配层：路由（尾部 `/*`、单段 `{param}`）、JSON 回执、查询串、`Guard`（登录守卫）、TLS、SSE 流。领域模块不碰 HTTP 类型，这里是唯一适配层 |
| | `events` | 事件总线 `EventBus` + SSE；`follow()` 订阅另一个服务的 `/events`（注册表 inotify 唤醒、退避重连） |
| 文件与数据 | `paths` | XDG 基目录规范的单一路径表 |
| | `fs` | 原子写（同目录 `.tmp` → rename）、单段文件名校验 `plain_name`、`unique_path` 同名不覆盖 |
| | `config` | JSON 配置读写模板（`load_or_default` / `load_or_seed` / `save`，可选 0600） |
| | `multipart` | 流式 multipart/form-data 解析（多文件边读边落盘，不进内存） |
| | `asset` | 资产仓库抽象（Repository）+ 上传流程模板（Template Method），字体/壁纸/词典/母版库共用 |
| | `formats` | 文件格式白名单单一事实源（书籍/字体/词典/图片），网页 `accept` 与服务端上传门同源 |
| | `ttf` | TTF/OTF 解析：家族名、魔数校验、CJK 覆盖率 |
| | `cache` / `clock` | 单值 TTL 缓存（`/status` 这类重活接口降频）/ unix 时间戳唯一出处 |
| xochitl | `xochitl` | 原生书库免重启注入（`POST /upload` GET-then-upload 归档）、流式 `upload_file`、超过网页上传上限的大文件“占位 + 磁盘替换”`upload_large_file` |
| | `xochitl_conf` | 改 `xochitl.conf [General]` 单键（休眠屏 `SleepScreenPath`；文件含凭证，绝不打印行内容） |
| | `fswatch` | inotify 防抖目录监听（常驻 / 限时两种） |
| 对外与安全 | `auth` | PBKDF2-HMAC-SHA256 密码哈希（旧版单轮 SHA-256 仍可校验）、Basic/Cookie 解析、内存会话表、失败限速器 `FailLimiter` |
| | `tls` / `mdns` / `netinfo` | 私有 CA + 叶证书 / 极简 mDNS 应答器（`shelf.local`）/ 本机 IPv4 表 |

**不引用旧项目任何 crate**（`device-core` / `weread-device`），需要的能力按“剥离移植”独立实现（`xochitl`、`fswatch` 两模块自己注明来源）。

## 对外接口（Rust API 入口）

它是库不是服务，没有 HTTP 端口；开发者最常碰的入口：

| 入口 | 用途 |
|---|---|
| `service::{ServiceSpec, run, run_with, parse_bind}` | 起服务 |
| `http::{Router, bind, Request, Reply, JsonBody, ApiError, Guard, ServeOpts}` | 写路由与回执 |
| `registry::{register, find, list, SvcClient}` | 注册/发现/调另一个服务 |
| `events::{EventBus, follow}` | 发事件 / 订阅事件 |
| `asset::{AssetStore, AssetUploadFlow}` | 实现一个“上传→校验→安装”的仓库 |
| `xochitl::Xochitl` | 往设备原生书库上传 |
| `paths::Paths` | 一切文件路径的唯一来源 |

## 注意

XDG 路径本身仍然叫 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、`$XDG_RUNTIME_DIR/shelf/`）——这是已部署设备上的真实文件路径，搬迁**不改**，重命名它涉及真机迁移，是另一件更大的事。改这里的任何模块前，先想清楚几条线谁在用它、会不会连累无关服务。

## 测试

`cargo test --manifest-path rmsvc-core/Cargo.toml`（独立 crate，当前 76 个 `#[test]`；CI `rust` job 单独一步，仿 `device-core` 先例）。

## 历史

2026-09-11 从 `shelf/crates/shelf-core` 正名搬到顶层——它早就不是 shelf 私有物（`notes/` 四个服务一开始就在依赖它），只是名分上还挂在 `shelf/` 底下。搬迁只改路径和 crate 名（`shelf-core` → `rmsvc-core`，`shelf_core::` → `rmsvc_core::`），不改逻辑。决策记录见 [白皮书](docs/reMarkable设备端Web服务基座白皮书.md)。
