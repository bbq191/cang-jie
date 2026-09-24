# reMarkable 设备端 Web 服务基座（rmsvc-core）白皮书

> **读者与用途**：要改 `rmsvc-core`，或在 `shelf/`、`notes/`、`enhance/`、`gateway/` 里写 Web 服务、想知道基座提供什么、有哪些约定的人。
> 先读“现状”；后面按模块分组，每个模块写“谁在用”和“关键约定”。被推翻的做法只留结论和教训。
> 入口文档（模块速查、Rust API 入口）见 [`../README.md`](../README.md)；网关怎么用这些模块（登录、代理、闸门）见 [网关白皮书](../../gateway/docs/reMarkable网关白皮书.md)。
> 所有数字以 2026-09-24 的代码为准（`rmsvc-core/src`）。

## 现状（2026-09-24）

**一句话**：设备上 9 个小 Web 服务（书架 2 个、系统增强 2 个、笔记 4 个、网关）都要做同样的杂活——读 XDG 路径、向注册表登记、收 HTTP 请求回 JSON、接大文件上传、往 xochitl 塞书、广播“该刷新了”、登录和 TLS。这些杂活只写一份，就是 `rmsvc-core`。它是纯基础设施，不含任何“书 / 笔记 / 字体”业务语义。

![模块地图与消费方](diagrams/module-map.svg)

**术语**

| 词 | 意思 |
|---|---|
| 注册表 | 每个服务启动时在 `$XDG_RUNTIME_DIR/shelf/services/<名>.json` 写下端口和 pid；网关读目录就知道谁活着 |
| path 依赖 | 消费方在 `Cargo.toml` 里直接写相对路径 `rmsvc-core = { path = "…/rmsvc-core" }`，不经 workspace |
| 剥离移植 | 需要旧项目某项能力时不依赖旧 crate，而是把验证过的结论独立重写一份 |
| Repository / Template Method | `asset` 把“上传→暂存→查扩展名→校验→安装→回执”流程写一次，各仓库只实现差异 |

**关键事实**

| 项 | 值 |
|---|---|
| 模块数 | 20 个（`lib.rs`） |
| 消费方 | `shelf/services/{book,koreader}-serve` · `enhance/{font,wallpaper}-serve` · `notes/services/{ink,transcribe,mind,note}-serve` + `notes/crates/vendorcfg` · `gateway/` |
| 依赖方向 | 单向：消费方 → 本 crate；本 crate 不知道任何消费方，不引用旧项目 crate（`device-core` / `weread-device`） |
| workspace | 不建根 workspace，各项目各管各的 `target/` |
| 测试 | 91 个单测（`cargo test --manifest-path rmsvc-core/Cargo.toml`，2026-09-24 实跑），CI `rust` job 单列一步 |
| 真机 | 所有消费方已部署在设备上并正常运行（2026-09-11 起 `install-all.sh` 真机跑通；2026-09-24 网关 `active`） |

**三条要记住**：① 这里出问题，理论上 5 个独立顶层项目一起受影响，改任何模块前先查谁在用（下面每节都列了）；② 公开结构体（如 `http::Request`）被各服务直接构造，加字段会波及全部调用方，宁可走内部头或新函数；③ XDG 路径仍叫 `shelf`（已部署设备的真实路径，改名要迁移）。

## 01｜服务骨架：service / registry / http / events

### service —— 启动模板（9 个服务都用）

`service::run(&SPEC, bind, &paths, router)`：解析 `--bind` → 建齐 XDG 目录 → 写注册表 → 起 HTTP 服务器，自动挂 `GET /health`（返回 `{ok, service, version}`）。`run_with` 多一个 `ServeOpts`（TLS、登录守卫、并发上限），只有网关用。`ServiceSpec.tab` 给出网页标签名和顺序（font-serve、koreader-serve、wallpaper-serve、note-serve 注册了；网关前端再把前三个收进“其他”标签）。

### registry —— 服务注册与发现（网关、笔记四服务用）

- 文件 `$XDG_RUNTIME_DIR/shelf/services/<name>.json`：`{name, port, label, version, pid, ui?}`，原子写；进程退出时 `Registration` 的 Drop 删掉它。运行时目录重启即清，再加上按 `/proc/<pid>` 清理陈旧条目，崩溃残留不会误报。
- **同名且 pid 还活着的条目拒绝覆盖**：真机踩过——调试时起了个 `--bind 127.0.0.1:1` 的第二实例，把正在服务的 wallpaper-serve 的注册顶没了。
- `find(name)` 直接读 `<name>.json`（O(1)，09-20 前是列举整个目录）。
- `SvcClient`：服务之间互相调用的 HTTP 客户端，每次按注册表解析地址（对方重启换端口也能找到）；非 2xx 时保留对方错误体里的原因（`get_json` / `post_json` / `post_json_value` / `try_post_json`，后者返回结构化的 `SvcError`）。

### http —— tiny_http 适配层（所有服务都用）

领域模块不碰 HTTP 类型，这里是唯一适配层。

| 约定 | 说明 |
|---|---|
| 路由 | 路径模式：尾部 `/*` 前缀匹配、单段 `{param}`。**最具体的优先**：字面段多的胜、精确匹配胜尾部通配（09-20 起）。此前靠“先注册先匹配”，通配路由曾抢走字面路由；现在跟注册顺序无关 |
| 每请求一线程 + 并发上限 | 缺省同时 64 个请求（含 SSE 长连接），超了回 503 + `Retry-After: 2`，不再开线程。取值：合法并发约 30（浏览器每源 6 条 × 几个标签页 + 网关到各服务 8 条订阅），64 留一倍余量；每条线程常驻只有几十 KB。`ServeOpts.max_concurrent` 可改，`None` 不限 |
| 请求头白名单 | 处理函数只看得到 `Cookie`、`Authorization`、`Accept`、`Host`、`X-Forwarded-Proto`、`User-Agent`（外加 `Content-Type`/`Content-Length`） |
| 对端 IP | 服务器取 TCP 对端地址（HTTPS 下同样取自底层 TcpStream），写进内部头 `X-Rmsvc-Remote-Ip`（常量 `REMOTE_IP_HEADER`），处理函数用 `Request::remote_ip()`、守卫用 `GuardRequest.remote` 读。客户端自带的同名头在白名单那步就被丢掉，伪造不了。没给 `Request` 加字段，是因为它被各服务直接构造 |
| 守卫 | `Guard`：分发前先问一次，`None` 放行、`Some(reply)` 直接回。登录策略由服务自己定义（只有网关用） |
| panic | 处理函数 panic 兜成 JSON 500“服务内部错误”，并发名额照常归还（需要消费方 release 是 `panic="unwind"`，见 §05） |
| 回执 | `Reply::ok/json/error/html/bytes/redirect`；两种流：`Reply::stream`（SSE 用：接管裸 socket、一帧一 flush、读到连接关闭为止，绕开 tiny_http 攒满 8KB 才发的 chunked 缓冲）和 `Reply::sized_stream`（文件下载用：已知长度，按定长响应边读边发，发完即结束，chunked 阈值调到最大以保留 `Content-Length`） |
| 请求体 | `read_small_body`（1MB 上限）、`json()`/`JsonBody`、`form_body`、`multipart_boundary` |

**09-24 教训**：文件下载第一版用了 `Reply::stream`，reader 读完连接却不关，真机上下载永远收不完。SSE 和定长下载是两种语义，各用各的。

### events —— 事件总线（8 个服务 + 网关用）

- **格式**：一行 JSON `{"area":"books","kind":"staging","at":<unix秒>}`，只是“该刷新了”的信号，不带状态。服务在**变更发生处**调 `EventBus::publish`，网关汇聚后推给网页（原则：不轮询、不监听全盘、日志写入不触发）。
- **`EventBus`**：进程内广播，每个订阅者一条有界队列（64 条），满了丢事件。
- **心跳**：`GET /events` 缺省 20 秒一次注释心跳；请求可带 `?ka=<秒>` 要求别的间隔（夹到 5～600 秒）。网关给浏览器用 `?ka=60`。
- **`follow(paths, svc, on_json)`**：订阅另一个服务的 `/events`，阻塞不返回，放线程里跑。服务没注册就等注册表目录的 inotify 事件（兜底 5 分钟）；连上用 `?ka=120`；断线 3 秒→60 秒指数退避，一条流撑过 10 秒才重置；对方回 404（没有事件流）就长等 10 分钟或等它重新注册。网关的 `Hub` 和 transcribe-serve 订阅 ink-serve 都用它（以前各写一份，还有每 3 秒轮询的耗电问题）。

## 02｜文件与数据

| 模块 | 谁在用 | 关键约定 |
|---|---|---|
| `paths` | 9 个服务 | XDG 基目录的**唯一路径表**，所有文件路径从这里取。设备 HOME 是 `/home/root`；配置 `~/.config/shelf/<服务>.json`，数据 `~/.local/share/shelf/`，状态 `~/.local/state/shelf/`，运行时 `$XDG_RUNTIME_DIR/shelf/`（注册表、上传分片，重启即清），二进制 `~/.local/bin`。外部约定可用 `SHELF_KOREADER_ROOT`、`SHELF_WEREAD_ROOT` 覆盖。`app_config_dir("notes")` 这类接口给非 `shelf` 命名空间的消费方 |
| `fs` | 8 个 | `write_atomic`：先写同目录临时文件再 rename；临时名 `<path>.<pid>.<序号>.tmp`，多线程/多进程同时写同一目标不会互相截断（09-20 前固定用 `<path>.tmp`）。`write_atomic_mode`：临时文件**创建时**就带指定权限，含密钥的文件没有“先宽后紧”的窗口（09-24）。`plain_name` 校验单段文件名（不含 `/`、不是 `.`/`..`、不以 `.` 开头）；`unique_path` 同名不覆盖（`1_x`、`2_x`…）；`move_unique` 跨设备回退 copy+rm |
| `config` | 7 个 | JSON 配置模板：`load_or_default`、`load_or_seed`（首启写出缺省）、`save`（原子写，可选 0600）。`is_corrupt` 判断“文件在但解析不了”，给启动时要落盘的调用方决定是否跳过，免得把损坏的配置覆盖成缺省（09-24） |
| `multipart` | book-serve、note-serve、网关 | 流式 multipart/form-data 解析，每个 part 以 `Read` 交出、边读边落盘，多文件一次 POST 也不把请求体读进内存。分隔符扫描记进度、按首字节跳查（200MB 上传体解析 1245ms → 46ms，09-22）。`percent_decode` 遇多字节字符不再 panic；`content_disposition(filename)` 生成下载头（ASCII 兜底名 + RFC 5987 UTF-8 名，笔记导出与原件下载共用） |
| `asset` | book-serve、koreader-serve、font-serve、wallpaper-serve | `AssetStore`（仓库：`validate`/`install`/`list`/`remove`）+ `AssetUploadFlow`（上传流程写一次）。拒收/成功文案由各仓库覆盖 |
| `formats` | 5 个 + 网关 | 文件格式白名单的**单一事实源**：书籍只收 `epub`/`pdf`（09-18 起），字体 `ttf/otf/ttc`，词典 `ifo/idx/dict/dz/syn/oft`，图片 `jpg/jpeg/png`。网页 `accept`（网关注入）和服务端上传门同源 |
| `ttf` | font-serve、koreader-serve | TTF/OTF 家族名（nameID 16 优先）、魔数校验、CJK 覆盖率；汉字覆盖数钳到区内总码位、够数即停（防恶意字体堆重叠段导致数亿次迭代，09-22） |
| `cache` | book-serve、koreader-serve | 单值 TTL 缓存 `TtlCache`，给每次刷新都会打、但算一次很重的 `/status`（如 3 秒 TTL）；本服务操作完成时 `invalidate`。计算期间持锁，并发请求等同一份结果 |
| `clock` | 8 个 | unix 时间戳唯一出处；取不到时间回 0 |

## 03｜和 xochitl 打交道：xochitl / xochitl_conf / fswatch

- **`xochitl`**（book-serve、note-serve）：往设备原生书库免重启塞文件，剥离移植自旧项目的真机结论。
  - `POST http://10.11.99.1/upload`（xochitl 的网页接口只绑 USB 网口，设备端靠 lo/usb1 别名让这个地址常驻可达）。
  - **GET-then-upload 归档**：先 `GET /documents/<文件夹 uuid>` 把“当前文件夹”设好（这是 xochitl 的全局状态），再上传，文件就落进该文件夹；`.metadata` 里的 parent 会被忽略。
  - **防复制风暴**：大书上传慢时会 408 或读超时，但文档其实已建好——这类错误**绝不重试**（`upload_likely_delivered`）。
  - `upload_file` 流式上传磁盘文件，不整本读进内存（09-19 OOM 审计：旧路径峰值能到原文件 2 倍多）。
  - `upload_large_file` 绕过网页上传约 100MB 的硬限：先传几 KB 的占位文档（EPUB 要带真书名和封面）让 xochitl 建好条目，再把磁盘上的文件原子替换成真文件。EPUB 删掉占位的渲染缓存，首次打开时重渲染；PDF 要一并改 `.content` 里的逐页表和页数。2026-09-20 真机验证：154MB PDF、153MB EPUB 都能打开。失败时占位可能留在书库里，不做危险的回滚删除。
  - `xochitl::library`：书库 `.metadata`/`.content` 的只读查询（找文件夹、去重命名、按创建时间找“刚进库的那本”、渲染页数），纯文件读取，不碰 HTTP。
- **`xochitl_conf`**（wallpaper-serve）：改 `~/.config/remarkable/xochitl.conf` 的 `[General]` 单键，目前只用于休眠屏 `SleepScreenPath`。文件里有 `DeveloperPassword` 等凭证，本模块**绝不返回、绝不打印任何行内容**；整文件读入、只动目标行、原子覆盖，首次改前留一份 `.shelf-bak`。新值要等 xochitl 下次启动才生效。
- **`fswatch`**（book-serve、ink-serve、网关）：inotify 防抖目录监听（单层）。空闲时阻塞读、零唤醒。`watch_debounced` 常驻；`watch_until` 限时，回调说“完了”就撤，用于有头有尾的等待（投原生后等 xochitl 渲染完），不给书库目录留常驻监听。

## 04｜对外与安全：auth / tls / mdns / netinfo（只有网关用）

这四个模块只有网关在用；**用户可见的行为、流程图、真机状态都写在网关白皮书 §01**，这里只记模块层面的约定。

- **`auth`**：`hash_password`/`verify_password`（`pbkdf2$<轮数>$<盐>$<摘要>`，PBKDF2-HMAC-SHA256 60 万轮、16 字节盐；旧版单轮 SHA-256 仍可校验）；`parse_basic`、`parse_cookie`；`SessionStore`（32 字节随机令牌、绝对过期、容量上限，满了淘汰最早到期的）；`IpFailLimiter`（按来源 IP 的滑动窗口失败计数，IPv4 映射的 IPv6 与纯 IPv4 算同一来源；有 `*_at(now)` 版本便于测试）。09-24 之前的全局计数器 `FailLimiter` 已删除。
- **`tls`**：`ensure_ca_signed(dir, extra_sans)` 读取或生成私有 CA（10 年）+ 服务器证书（800 天；名字列表变化或签发满 700 天重签）；CA 带名称约束（`PERMITTED_DNS`、`PERMITTED_V4`，路径长度 0），约束外的 SAN 剔除并打日志；旧的无约束 CA 自动备份为 `.bak-<秒>` 后重建。依赖 `x509-parser` 解析已有 CA（本来就经 rcgen 在依赖树里）。测试用 `rustls-webpki` 做完整链校验，包括“用同一把 CA 私钥硬签 `evil.com` 会被拒”的反证。流程图见网关白皮书的 [`ca-migration.svg`](../../gateway/docs/diagrams/ca-migration.svg)。
- **`mdns`**：极简 mDNS 应答器，只回答本机名的 A 查询，应答地址选和提问者同子网的本机 IPv4（USB 网段问就答 `10.11.99.1`）。接口重扫与读超时合并为 60 秒。绑不上 5353（别的 mDNS 服务在跑）只打日志，不影响网关。
- **`netinfo`**：本机 IPv4 表（证书 SAN、mDNS 选址用），读 `/proc/net/fib_trie` + `/proc/net/route`，不 fork 进程（09-20 前每 30 秒 fork 一次 `ip`）；读不到才回落到 `ip -4 -o addr`。

## 05｜维护纪律、构建与测试

- **改之前先查谁在用**：上面每节都列了消费方。`bookconv` 出问题只影响书处理；这里出问题理论上 5 个顶层项目全受影响。
- **公开结构体不随便加字段**：`http::Request` 等被各服务直接构造（包括测试），加字段会波及全部调用方。09-24 的对端 IP 就是为此走了内部头。
- **path 依赖的深度**：`..` 的个数取决于消费方自己的目录深度——`gateway/` 写 `../rmsvc-core`，`enhance/*-serve/` 写 `../../rmsvc-core`，`shelf/services/*/`、`notes/services/*/`、`notes/crates/vendorcfg/` 写 `../../../rmsvc-core`。写错时 `cargo build` 会直接说它去哪找过，照着改。
- **每个独立顶层项目各带一份 `.cargo/config.toml`**（交叉编译的 CC/AR 覆盖），原因见 §06。
- **release profile**：`gateway`、`shelf`、`notes` 是 `panic="unwind"`，基座的 panic 兜底和各服务的 `catch_unwind` 才真正生效；`enhance/{font,wallpaper}-serve` 仍是 `abort`（没有依赖 `catch_unwind` 的后台线程）。
- **测试**：`cargo test --manifest-path rmsvc-core/Cargo.toml`，91 个；HTTP 服务器、TLS 握手取对端 IP、名称约束链校验都有真起服务器 / 真证书链的测试。

## 06｜踩坑

- **独立顶层 crate 不继承调用方目录的 cargo 配置**（2026-09-11，搬迁当天）：`gateway/`、`enhance/{wallpaper,font}-serve/` 独立成顶层项目后，第一次交叉编译在 `ring` 这步报 `failed to find tool "aarch64-linux-musl-gcc"`。原因：`shelf/.cargo/config.toml` 的 CC 覆盖只在从 `shelf/` 发起 `cargo build` 时生效，`shelf/build.sh` 里 `(cd ../gateway && cargo build …)` 这种跨目录调用吃不到。修法：每个独立项目各带一份内容相同的物理副本（Cargo 没有“引用别处配置”的机制）。这个坑只在 `--target aarch64-unknown-linux-musl` 交叉编译时炸，host 上 `cargo test` 全绿会让人误以为验证充分。
- **`version.workspace = true` 离开 workspace 就报错**：挪出 `shelf` workspace 的 crate 要把 `version`/`edition`/`license` 改成字面量，`[profile.release]` 也要各自复制一份。
- **`panic="abort"` 让 `catch_unwind` 失效**（2026-09-19，真机《镖人》）：一次 panic 摔掉整个进程、卡住所有在途操作。`gateway`/`shelf`/`notes` 已改 `unwind`，二进制约大 8%。
- **SSE 通道拿去发文件下载**（2026-09-24）：见 §01 http。
- **通配路由抢字面路由**：旧的“先注册先匹配”下，`GET /{name}` 抢过 `/health`（wallpaper-serve 真机踩过）。现在按具体程度分发；`service.rs` 里“`/health` 必须先注册”的注释是那时留下的，已不再是必要条件。

## 07｜命名遗留与待办

**命名遗留（刻意不动）**：XDG 命名空间 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、`~/.local/state/shelf/`、`$XDG_RUNTIME_DIR/shelf/services/`）、`shelf.target`、默认密码 `shelf`、mDNS 名 `shelf.local`。这些是已部署设备的真实路径和配置，改名要给已有安装写兼容读取或一次性搬家脚本。

**待办**

- 命名遗留要不要处理，没有排期；要动时先设计迁移方案，不是简单改字符串。
- `lib.rs` 模块注释里 `auth` 仍写“salted SHA-256”，已过时（实际是 PBKDF2），下次改代码时顺手更正。

## 附｜来历

- **2026-09-11 正名搬顶层**：原名 `shelf-core`，在 `shelf/crates/` 下。起因是一次连锁判断：用户问“wallpaper/font 能不能挪进 `enhance/`”→ 排查发现这两个服务**建在** shelf-core 的整套框架之上，要么复刻一份框架（两份分别维护），要么让 `enhance/` 反向依赖 `shelf/`（方向倒挂）→ 追问下去发现 shelf-core 早就是事实上的共享基座（`notes/` 四个服务一开始就依赖它），只是名分没跟上。正名搬顶层一次解决两个问题。
- **命名**：选 `rmsvc-core`（reMarkable service core），延续 `device-core` 的 `<领域>-core` 风格；没选 `hub-core`/`panel-core`，因为它们暗示“网关附属物”，而本 crate 和网关是平级的消费关系。目录摊平放顶层，不建“基座”父目录。
- **Repository / Template Method / Registry / Facade 这些设计取舍**是 shelf-core 时代定的，记在 `shelf/docs/reMarkable书架白皮书.md` §01（三种拆法）。
- **旧章节号对照**：旧 §00b 模块一览 → §01～§04；旧 §02 路径深度 → §05；旧 §03 维护纪律 → §05；旧 §03b 09-24 新增能力 → 分散到 §01 http、§02 fs/config/multipart、§04 auth/tls；旧 §04 踩坑 → §06；旧 §05 待办 → §07。
