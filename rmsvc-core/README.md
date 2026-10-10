# rmsvc-core —— reMarkable 设备端 Web 服务共享底座

**一句话**：设备上 8 个小 Web 服务（书架 1 个、系统增强 2 个、笔记 4 个、网关）共用的 Rust 库。读路径、登记服务、收发 HTTP、接大文件上传、往 xochitl（reMarkable 自带的书库/阅读/笔记程序）塞书并认出新文档、广播"该刷新了"——这些杂活只写一份，就在这里。它是库，**没有自己的端口**，也不含任何"书 / 笔记 / 字体"业务。

- **给谁用**：要在 `shelf/`、`notes/`、`enhance/`、`gateway/` 里写或改 Web 服务的开发者。
- **怎么开始**：先看下面的模块图和"一个服务的骨架"；要改某个模块，先读[白皮书](docs/reMarkable设备端Web服务基座白皮书.md)第 4 章里那个模块的"谁在用"和约定。
- 整个仓库里的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

![模块地图与消费方](docs/diagrams/module-map.svg)

## 一个服务的骨架

```rust
let args: Vec<String> = std::env::args().skip(1).collect();
let bind_addr = service::parse_bind(&args, SPEC.default_bind);
let paths = Paths::from_env();                          // 所有路径从这里取
let router = Router::new().get("/status", bind(&st, |s, _| Ok(Reply::ok(&s.status()))));
service::run_or_exit(&SPEC, &bind_addr, &paths, router, ServeOpts::default());
// ↑ 建目录 → 写注册表 → 起 HTTP（自带 GET /health）；出错打日志并非零退出，交给 systemd 拉起
```

有变更就 `bus.publish(area, kind)`（`/events` 路由写 `|s, r| Ok(s.bus.sse_reply_for(r))`），网关把事件推给网页，网页自己刷新。接口出错用 `ApiError::{bad, not_found, conflict, internal, …}` 显式选状态码（没有 `From<String>`）。完整步骤见白皮书第 3 章。

- 消费方都用 path 依赖（`rmsvc-core = { path = "…/rmsvc-core" }`，`..` 的个数看自己的目录深度），**不建根 workspace**；本 crate 不知道任何消费方（单向依赖，无环）。
- 谁在用：`shelf/services/book-serve` · `notes/services/{ink,transcribe,mind,note}-serve` + `notes/crates/{vendorcfg,notesvc}` · `enhance/{font,wallpaper}-serve` · `gateway/`。

## 22 个模块

| 分组 | 模块 | 一句话 |
|---|---|---|
| 服务骨架 | `service` | 启动模板：解析 `--bind` → 建目录 → 自注册 → 起服务器，自带 `GET /health`；`run_or_exit` 是各服务 `main` 的收尾 |
| | `registry` | 注册与发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`，按 pid 清陈旧条目）；`SvcClient` 调另一个服务（`try_get_typed` 直接反序列化、错误带对方状态码；`get_bytes` 带大小上限） |
| | `http` | tiny_http 唯一适配层：路由（最具体优先）、回执体 `Body`（整块 / 定长流 / SSE 事件流三选一）、`ApiError`、守卫、`TestRequest`、编解码（`encoding`）；并发上限 64、每连接 60 秒读空闲超时；处理函数和守卫 panic 都回 JSON 500 |
| | `events` | 事件总线 `EventBus`（`publish` / `publish_with`）+ SSE（`sse_reply_for` 读请求的 `?ka=`）；`follow()` 订阅另一个服务；`Wake` 与 `registry_wake()`；`Event::parse` |
| | `proc` | 带超时的子进程 `run_timeout`、读 `/proc`（开机秒数、不 fork 地按名字找进程） |
| 文件与数据 | `paths` | XDG 路径的唯一路径表；`Paths::sandbox(dir)` 给测试用，HOME 与全部 XDG 目录都落进临时目录 |
| | `fs` | 原子写（可带权限）、`plain_name` / `unique_path` / `move_unique`（跨分区先复制到临时名再改名）、`ScratchFile`、`list_files` / `clean_dir`、`write_atomic_if_changed`、剩余空间 `fs_space` |
| | `config` | JSON 配置读写（`load_or_default` / `load_or_seed` / `load_or_seed_checked` / `save` / `is_corrupt`）；文件损坏时 `load_or_seed` 打日志并留一份 `.corrupt`（只留第一份），按缺省运行 |
| | `multipart` | 流式 multipart 解析（边读边落盘；文件名引号内的 `;` 不切）、`Content-Disposition` 下载头 |
| | `asset` | 上传目标 `UploadTarget` + 上传流程模板 `AssetUploadFlow`（字体/壁纸/母版库共用），整体失败 `FlowError` 分 400 / 500；暂存在 `~/.local/state/shelf/upload`，启动清半成品 |
| | `formats` | 文件格式白名单唯一事实源（书籍只收 EPUB/PDF）；`mime_of` 扩展名→MIME 唯一表、`sniff` 按魔数认格式、`stem_of` |
| | `wire` | 跨服务 / 前后端共用的线上取值枚举：落库 / 渲染自检状态（带 `Unknown` 兜底旧值）、代理放弃记录的种类 `FailureKind`；JSON 仍是小写字符串 |
| | `cache` | 单值 TTL 缓存 `TtlCache`；按文件戳（长度 + mtime + inode）失效的 `StampCache` |
| | `clock` / `sync` | unix 时间戳唯一出处 / 容忍 poison 的取锁 `sync::lock` |
| xochitl | `xochitl` | 免重启进原生书库（"设文件夹 → `/upload`"，进程内锁 + 跨进程 `flock`，设失败退回书库根）；目标只收 `Folder`（根 / 校验过的 `FolderId`）：`upload_to` / `upload_and_claim`（按 `ClaimBy` 在快照外的新文档里认领，绝不取"最新一本"兜底）/ `upload_large`（超过约 100MB 的"占位 + 磁盘替换"）；书库只读查询（强类型 `Metadata`，`kind` 是 `EntryKind`；`read_meta` 区分"没有/读不了"）；`.content` 页表 `PageTable` |
| | `xochitl_conf` | 改 `xochitl.conf [General]` 单键（文件含凭证，绝不打印行内容，保留原权限） |
| | `fswatch` | inotify 防抖目录监听（调用方线程 `poll`，不另开读线程）：常驻 `watch_debounced`（目录被删/挪走后退避重挂）、限时 `watch_until`、等条件成立 `wait_for`（先挂监听再查）；`WatchSpec` 可改掩码与防抖 |
| 对外与安全（只有网关用，**`gateway` feature**，默认不编） | `auth` | PBKDF2 密码哈希、Basic/Cookie 解析、会话表、按 IP 失败限速、只缓存"校验通过"的 `VerifyCache` |
| | `tls` / `mdns` / `netinfo` | 带名称约束的私有 CA + 服务器证书（原子写，私钥创建即 0600）/ mDNS 应答器（`shelf.local`，地址变化才重扫）/ 本机 IPv4 表 |

另有 crate 内部的 `sys`（`poll` 小工具，`fswatch` 与 `mdns` 共用），不对外。

**`gateway` feature**：`auth` / `tls` / `mdns` / `netinfo`、它们的依赖（rcgen、x509-parser、pbkdf2、socket2……）和 HTTPS 服务端（tiny_http 的 rustls 适配）只有网关用，收在这个 feature 后面、默认关；网关写 `rmsvc-core = { path = "../rmsvc-core", features = ["gateway"] }`。其余服务不编它们（拆分那次提交记录：aarch64 release 的 ink-serve 小了 23.9%，book-serve 小 2.8%）。

**同目录的独立小 crate** [`epubpkg/`](epubpkg/src/lib.rs)：EPUB 容器 / OPF 的只读解析（container.xml → OPF、manifest/spine/Dublin Core、href 解 XML 实体与百分号、有上限地读 zip 条目）+ xochitl `.epubindex`（各 spine 文件起始页）解析；书架 `shelf-conv` 与 `book-serve`、笔记 `epubmap` 共用。它**不依赖 rmsvc-core**、也不是它的模块——只依赖 regex / zip / flate2，免得纯解析库连带编进 HTTP/TLS 那一整套。

**vendored tiny_http**：[`vendor/tiny_http`](vendor/README.md) 是 tiny_http 0.12.0 的本地副本，打了三处补丁（每连接读空闲超时、accept 暂时性错误不退出、rustls 0.20 → 0.23.45）。

## 常用 Rust 入口

| 入口 | 用途 |
|---|---|
| `service::{ServiceSpec, run, run_or_exit, run_with, parse_bind}` | 起服务 |
| `http::{Router, bind, Request, Reply, Body, JsonBody, ApiError, Guard, ServeOpts, TestRequest}` | 写路由与回执、测试 |
| `registry::{register, find, list, SvcClient, SvcError}` | 注册、发现、调另一个服务 |
| `events::{EventBus, follow, Wake, registry_wake, Event}` | 发事件 / 订阅事件 / 等变化 |
| `asset::{UploadTarget, AssetUploadFlow, FlowError}` | 实现"上传 → 校验 → 安装"的上传目标 |
| `xochitl::{Xochitl, Folder, FolderId, UploadBody, ClaimBy, ClaimWait, Metadata, EntryKind, read_meta, PageTable}` | 往原生书库上传并认领、读书库条目与页表 |
| `fswatch::{watch_debounced, watch_until, wait_for, WatchSpec}` | 等目录变化 |
| `config::{load_or_seed, load_or_seed_checked, save}` | 读写 JSON 配置 |
| `wire::{DeliverStatus, RenderStatus, FailureKind}` | 跨服务线上取值 |
| `paths::Paths` | 所有文件路径的来源（测试用 `Paths::sandbox`） |

## 构建与测试

- `cargo build --manifest-path rmsvc-core/Cargo.toml`；独立 crate，各消费方编译时一起编。
- `cargo test --locked --manifest-path rmsvc-core/Cargo.toml`：2026-10-10 实跑 155 个单测通过、3 个默认忽略（mDNS 网络命名空间端到端、由另一测试在子进程里调起的 TLS 探针、书库扫描性能测量），2 个文档测试通过、1 个默认忽略。单测经自指 dev-dependency 带 `gateway` feature 跑，不带 feature 的 `cargo test` 也覆盖网关专用模块。
- `cargo test --locked --manifest-path rmsvc-core/epubpkg/Cargo.toml`：11 个（含 `epubindex` 的真机样本用例）。
- `cargo test --manifest-path rmsvc-core/vendor/tiny_http/Cargo.toml --lib`：10 个（含补丁的单测）。
- CI（GitHub Actions）的 `rust` job 跑前两项，`cargo audit` 查 `rmsvc-core` 与 `epubpkg` 的 lockfile。

## 验证现状

- 所有消费方都已部署在设备上运行（2026-09-11 起 `install-all.sh` 真机跑通，之后多次整轮重装）；大文件通道 2026-09-20 真机验证 154MB PDF、153MB EPUB；`epubindex` 10-09 真机跑通一次。
- 2026-10-10 重构（`Folder` 上传入口、`upload_and_claim`、跨进程 flock、`http::Body`、`gateway` feature、rustls 0.23.45 等）**未部署、未在真机上验证**，只有开发机测试。逐项状态见[白皮书第 8 章](docs/reMarkable设备端Web服务基座白皮书.md#8-验证现状)。

## 注意

- 改任何模块前先查谁在用（白皮书第 4 章每节都列了）：这里出问题，理论上 shelf、notes、enhance、gateway 四个顶层项目一起受影响。
- 公开结构体（如 `http::Request`）被各服务直接构造，加字段会波及全部调用方，宁可走内部头或新函数；测试构造请求用 `http::TestRequest`。
- 只有一个服务用的东西不放这里；只有网关用的放 `gateway` feature 后面。
- XDG 路径仍叫 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、`~/.local/state/shelf/`、`$XDG_RUNTIME_DIR/shelf/`）：这是已部署设备上的真实路径，改名要迁移。
- 不引用旧项目的 crate（`device-core` / `weread-device`）；`xochitl`、`fswatch` 是"剥离移植"的独立实现。2026-09-11 从 `shelf/crates/shelf-core` 正名搬到顶层，来历见白皮书附录 A。
