# rmsvc-core —— reMarkable 设备端 Web 服务共享底座

**一句话**：设备上 8 个小 Web 服务（书架 1 个、系统增强 2 个、笔记 4 个、网关）共用的 Rust 库。读路径、登记服务、收发 HTTP、接大文件上传、往 xochitl（reMarkable 自带的书库/阅读/笔记程序）塞书、广播"该刷新了"——这些杂活只写一份，就在这里。它是库，**没有自己的端口**，也不含任何"书 / 笔记 / 字体"业务。

- **给谁用**：要在 `shelf/`、`notes/`、`enhance/`、`gateway/` 里写或改 Web 服务的开发者。
- **怎么开始**：先看下面的模块图和"一个服务的骨架"；要改某个模块，先读[白皮书](docs/reMarkable设备端Web服务基座白皮书.md)里那个模块的"谁在用"和"关键约定"。
- 整个仓库里的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

![模块地图与消费方](docs/diagrams/module-map.svg)

## 一个服务的骨架

```rust
let paths = Paths::from_env();                          // 所有路径从这里取
let router = Router::new().get("/status", bind(&st, |s, _| Ok(Reply::ok(&s.status()))));
service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default());
// ↑ 建目录 → 写注册表 → 起 HTTP（自带 GET /health）；出错打日志并非零退出，交给 systemd 拉起
```

有变更就 `EventBus::publish(area, kind)`，网关把事件推给网页，网页自己刷新。

- 消费方都用 path 依赖（`rmsvc-core = { path = "…/rmsvc-core" }`，`..` 的个数看自己的目录深度），**不建根 workspace**；本 crate 不知道任何消费方（单向依赖，无环）。
- 谁在用：`shelf/services/book-serve` · `notes/services/{ink,transcribe,mind,note}-serve` + `notes/crates/vendorcfg` · `enhance/{font,wallpaper}-serve` · `gateway/`。

## 22 个模块

| 分组 | 模块 | 一句话 |
|---|---|---|
| 服务骨架 | `service` | 启动模板：解析 `--bind` → 建目录 → 自注册 → 起服务器，自带 `GET /health`；`run_or_exit` 是各服务 `main` 的收尾 |
| | `registry` | 注册与发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`，按 pid 清陈旧条目）；`SvcClient` 调另一个服务（`get_typed` 直接反序列化、`get_bytes` 带大小上限） |
| | `http` | tiny_http 唯一适配层：路由（最具体优先）、回执、守卫、TLS、SSE 与定长下载流；并发上限 64、每连接 60 秒读空闲超时（靠 [`vendor/tiny_http`](vendor/README.md) 两处补丁）；处理函数和守卫 panic 都回 JSON 500；`JsonBody`/`q_parse`/`encode_query`/`html_escape` 等取值小件 |
| | `events` | 事件总线 `EventBus`（`publish` / `publish_with` 带附加字段）+ SSE；`follow()` 订阅另一个服务；`Wake` 与 `registry_wake()`（注册表目录进程内一条 inotify）；`Event::parse` 解析事件行 |
| | `proc` | 带超时的子进程 `run_timeout`、读 `/proc`（开机秒数、不 fork 地按名字找进程 `find_process`）（2026-10-09 新增） |
| 文件与数据 | `paths` | XDG 路径的唯一路径表；`Paths::sandbox(dir)` 给测试用，HOME 与全部 XDG 目录都落进临时目录 |
| | `fs` | 原子写（可带权限）、`plain_name` / `unique_path` / `move_unique`（跨分区先复制到临时名再改名）、`ScratchFile`、`list_files` / `clean_dir`、`write_atomic_if_changed`、剩余空间 `fs_space` |
| | `config` | JSON 配置读写模板（`load_or_default` / `load_or_seed` / `save` / `is_corrupt`）；`backup_corrupt` 坏文件只留第一份 `.corrupt` |
| | `multipart` | 流式 multipart 解析（边读边落盘；文件名引号内的 `;` 不切）、`Content-Disposition` 下载头 |
| | `asset` | 资产仓库 + 上传流程模板（字体/壁纸/母版库共用），暂存在 `~/.local/state/shelf/upload`，启动清半成品 |
| | `formats` | 文件格式白名单唯一事实源（书籍只收 EPUB/PDF）；`mime_of` 扩展名→MIME 唯一表、`sniff` 按魔数认格式、`stem_of` |
| | `ttf` | TTF/OTF 家族名、魔数、CJK 覆盖率（跳过 format-12 损坏组，format-4 逐码位总数设上限） |
| | `cache` | 单值 TTL 缓存 `TtlCache`；按文件戳（长度 + mtime + inode）失效的 `StampCache` |
| | `clock` / `sync` | unix 时间戳唯一出处 / 容忍 poison 的取锁 `sync::lock` |
| xochitl | `xochitl` | 免重启进原生书库（"设文件夹 → `/upload`"，设失败退回书库根）；按**文件夹名**的 `upload*` 与按**文件夹 uuid** 的 `upload*_into` 两套入口；超过约 100MB 的"占位 + 磁盘替换"；书库只读查询（强类型 `Metadata`、区分"没有/读不了"的 `read_meta`、`live_entries`、`file_type` 等） |
| | `xochitl_conf` | 改 `xochitl.conf [General]` 单键（休眠屏 `SleepScreenPath`；文件含凭证，绝不打印行内容，保留原权限） |
| | `fswatch` | inotify 防抖目录监听：常驻 `watch_debounced`（目录被删/挪走后退避重挂）、限时 `watch_until`、等条件成立 `wait_for` |
| 对外与安全（只有网关用） | `auth` | PBKDF2 密码哈希、Basic/Cookie 解析、会话表、按 IP 失败限速、只缓存"校验通过"的 `VerifyCache` |
| | `tls` / `mdns` / `netinfo` | 带名称约束的私有 CA + 服务器证书（原子写，私钥创建即 0600）/ mDNS 应答器（`shelf.local`，地址变化才重扫）/ 本机 IPv4 表 |

另有 crate 内部的 `sys`（`poll` 小工具，`fswatch` 与 `mdns` 共用），不对外。

同目录下还有一个**独立小 crate** [`epubpkg/`](epubpkg/src/lib.rs)（2026-10-09）：EPUB 容器 / OPF 的只读解析（container.xml → OPF、manifest/spine/Dublin Core、href 解 XML 实体与百分号、有上限地读 zip 条目），笔记线 `notes/crates/epubmap` 和书架 `shelf/crates/shelf-conv` 共用。它**不依赖 rmsvc-core**、也不是它的模块——只依赖 regex/zip，免得纯解析库连带编进 HTTP/TLS 那一整套。

## 常用 Rust 入口

| 入口 | 用途 |
|---|---|
| `service::{ServiceSpec, run, run_or_exit, run_with, parse_bind}` | 起服务 |
| `http::{Router, bind, Request, Reply, JsonBody, ApiError, Guard, ServeOpts}` | 写路由与回执 |
| `registry::{register, find, list, SvcClient}` | 注册、发现、调另一个服务 |
| `events::{EventBus, follow, Wake, registry_wake, Event}` | 发事件 / 订阅事件 / 等变化 |
| `asset::{AssetStore, AssetUploadFlow}` | 实现"上传 → 校验 → 安装"的仓库 |
| `xochitl::{Xochitl, Metadata, read_meta}` | 往原生书库上传、读书库条目 |
| `fswatch::{watch_debounced, watch_until, wait_for}` | 等目录变化 |
| `paths::Paths` | 所有文件路径的来源（测试用 `Paths::sandbox`） |

## 构建与测试

- `cargo build --manifest-path rmsvc-core/Cargo.toml`；独立 crate，各消费方编译时一起编。
- `cargo test --manifest-path rmsvc-core/Cargo.toml`：2026-10-09 实跑 131 个单测通过、1 个默认忽略（需要网络命名空间的 mDNS 端到端测试），另有 1 个文档示例默认忽略。
- `cargo test --manifest-path rmsvc-core/epubpkg/Cargo.toml`：同目录的 `epubpkg`，10 个单测。
- CI（GitHub Actions）的 `rust` job 单列一步跑它（`epubpkg` 同一步）；仓库 2026-10-09 公开后 CI 恢复执行。

## 注意

- 改任何模块前先查谁在用（白皮书每节都列了）：这里出问题，理论上 shelf、notes、enhance、gateway 四个顶层项目一起受影响。
- 公开结构体（如 `http::Request`）被各服务直接构造，加字段会波及全部调用方，宁可走内部头或新函数。
- XDG 路径仍叫 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、`~/.local/state/shelf/`、`$XDG_RUNTIME_DIR/shelf/`）：这是已部署设备上的真实路径，改名要迁移。
- 不引用旧项目的 crate（`device-core` / `weread-device`）；`xochitl`、`fswatch` 是"剥离移植"的独立实现。2026-09-11 从 `shelf/crates/shelf-core` 正名搬到顶层，来历见白皮书末尾。
