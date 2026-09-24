# rmsvc-core —— reMarkable 设备端 Web 服务共享底座

> **读者与用途**：要在 `shelf/`、`notes/`、`enhance/`、`gateway/` 里写或改 Web 服务的开发者。
> 各模块的约定与踩坑见 [`docs/reMarkable设备端Web服务基座白皮书.md`](docs/reMarkable设备端Web服务基座白皮书.md)；整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

## 它是什么

设备上 9 个小 Web 服务（书架、KOReader、字体、壁纸、笔记四件套、网关）都要做同样的杂活：读 XDG 路径、启动时向注册表登记、收 HTTP 请求回 JSON、接大文件上传、往 xochitl 塞书、广播“该刷新了”。
这些杂活只写一份，就是 `rmsvc-core`。**服务只写自己的业务；地基只有这一份。** 它不含任何“书 / 笔记”业务语义。

![模块地图与消费方](docs/diagrams/module-map.svg)

- 消费方都用 path 依赖（`rmsvc-core = { path = "…/rmsvc-core" }`，`..` 的个数看自己的目录深度），**不建根 workspace**；本 crate 不知道任何消费方（单向依赖，无环）。
- 谁在用：`shelf/services/{book,koreader}-serve` · `notes/services/{ink,transcribe,mind,note}-serve` + `notes/crates/vendorcfg` · `enhance/{font,wallpaper}-serve` · `gateway/`。
- 一个服务的骨架三步：`service::run(&SPEC, bind, &paths, router)`（建目录、自注册、挂 `/health`）→ 用 `http::Router` 写处理函数 → 有变更就 `EventBus::publish` 通知网页。

## 21 个模块

| 分组 | 模块 | 一句话 |
|---|---|---|
| 服务骨架 | `service` | 启动模板：解析 `--bind` → 建目录 → 自注册 → 起服务器，自带 `GET /health` |
| | `registry` | 注册与发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`，按 pid 清陈旧条目）；`SvcClient` 调另一个服务 |
| | `http` | tiny_http 适配：路由（最具体优先）、回执、守卫、TLS、SSE 流与定长下载流；并发上限 64；对端 IP 经内部头传入 |
| | `events` | 事件总线 `EventBus` + SSE；`follow()` 订阅另一个服务的 `/events` |
| 文件与数据 | `paths` | XDG 路径的唯一路径表 |
| | `fs` | 原子写（可带权限）、单段文件名校验 `plain_name`、同名不覆盖 `unique_path` |
| | `config` | JSON 配置读写模板（`load_or_default` / `load_or_seed` / `save` / `is_corrupt`） |
| | `multipart` | 流式 multipart 解析（边读边落盘）、`Content-Disposition` 下载头 |
| | `asset` | 资产仓库 + 上传流程模板，字体/壁纸/KOReader/母版库共用 |
| | `formats` | 文件格式白名单唯一事实源（书籍只收 EPUB/PDF） |
| | `ttf` | TTF/OTF 家族名、魔数、CJK 覆盖率 |
| | `cache` / `clock` / `sync` | 单值 TTL 缓存 / unix 时间戳唯一出处 / 容忍 poison 的取锁 `sync::lock` |
| xochitl | `xochitl` | 免重启进原生书库（GET-then-upload 归档，进程内“设文件夹→上传”串行）、流式 `upload_file`、超过约 100MB 上传上限的“占位 + 磁盘替换” |
| | `xochitl_conf` | 改 `xochitl.conf [General]` 单键（休眠屏 `SleepScreenPath`；文件含凭证，绝不打印行内容） |
| | `fswatch` | inotify 防抖目录监听（常驻 / 限时） |
| 对外与安全（只有网关用） | `auth` | PBKDF2 密码哈希、Basic/Cookie 解析、会话表、按 IP 的失败限速 `IpFailLimiter` |
| | `tls` / `mdns` / `netinfo` | 带名称约束的私有 CA + 服务器证书（旧 CA 自动迁移）/ mDNS 应答器（`shelf.local`）/ 本机 IPv4 表 |

## 常用 Rust 入口

它是库，没有 HTTP 端口：

| 入口 | 用途 |
|---|---|
| `service::{ServiceSpec, run, run_with, parse_bind}` | 起服务 |
| `http::{Router, bind, Request, Reply, JsonBody, ApiError, Guard, ServeOpts}` | 写路由与回执 |
| `registry::{register, find, list, SvcClient}` | 注册、发现、调另一个服务 |
| `events::{EventBus, follow}` | 发事件 / 订阅事件 |
| `asset::{AssetStore, AssetUploadFlow}` | 实现“上传→校验→安装”的仓库 |
| `xochitl::Xochitl` | 往原生书库上传 |
| `paths::Paths` | 所有文件路径的来源 |

## 构建与测试

- `cargo build --manifest-path rmsvc-core/Cargo.toml`；独立 crate，各消费方编译时一起编。
- `cargo test --manifest-path rmsvc-core/Cargo.toml`（2026-09-24 实跑 95 个单测；CI `rust` job 单列一步）。

## 注意

- 改任何模块前先想清楚几条线谁在用它（白皮书每节都列了），这里出问题理论上 5 个顶层项目一起受影响。
- XDG 路径仍叫 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、`$XDG_RUNTIME_DIR/shelf/`）：这是已部署设备上的真实路径，改名要迁移。
- 不引用旧项目的 crate（`device-core` / `weread-device`）；`xochitl`、`fswatch` 是“剥离移植”的独立实现。
- 2026-09-11 从 `shelf/crates/shelf-core` 正名搬到顶层（crate 名 `shelf-core` → `rmsvc-core`），来历见白皮书末尾。
