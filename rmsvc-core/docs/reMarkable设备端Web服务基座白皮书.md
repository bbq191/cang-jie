# reMarkable 设备端 Web 服务基座（rmsvc-core）白皮书

> 记"怎么决定、为什么正名搬顶层、踩了什么坑"。读现状先看“3 分钟读懂”和 §00b；待办看 §05；踩坑看 §04。模块清单与 Rust API 入口见 [`../README.md`](../README.md)。
> **本 crate 的架构决策大多是在还叫 `shelf-core`、还挂在 `shelf/crates/` 下时定的**——Repository/
> Template Method/Registry/Facade 这些设计模式的取舍过程记在 `shelf/docs/reMarkable书架白皮书.md`
> §01（"三种拆法"那节），本文不重复搬运那段历史，只记"正名搬顶层"这件事本身，以及正名之后
> 独立存在时的现状、消费方、维护纪律。

## 3 分钟读懂

**它是什么**：设备上十来个小 Web 服务（书架、KOReader、字体、壁纸、笔记四件套、网关）共用的“地基库”——路径、服务注册、HTTP 适配、流式上传、事件总线、往 xochitl 塞书、登录/TLS 这些每个服务都要的杂活，只写一份。服务只写自己的业务。

![模块地图与消费方](diagrams/module-map.svg)

**术语**：*注册表*＝每个服务启动时在 `$XDG_RUNTIME_DIR/shelf/services/<名>.json` 写下端口，网关读目录即知谁活着；*Repository / Template Method*＝`asset` 里把“上传→暂存→扩展名门→校验→安装→回执”流程写一次，各仓库只实现差异；*剥离移植*＝需要旧项目的某项能力时不依赖旧 crate，而是把已验证的结论独立重写一份；*path 依赖*＝消费方在 `Cargo.toml` 里直接写相对路径依赖本 crate，不经 workspace。

**三条要记住的事**：① 本 crate 不知道任何消费方（单向依赖）；② 改任何模块前先想清楚几条线谁在用；③ XDG 路径仍叫 `shelf`（已部署设备的真实路径，改名要迁移）。

## 00｜定位与原则

`rmsvc-core` 是 reMarkable 设备端**三条 Web 服务项目线**（`shelf/`、`notes/`、`gateway/`，加上
`enhance/wallpaper-serve`/`enhance/font-serve` 两个领域服务）共用的基座 crate：纯基础设施，
不含任何"书"/"笔记"/"壁纸"这类业务语义——领域逻辑一律留在各自的 `services/*`/`enhance/*`里。

**2026-09-11 正名搬顶层**：本 crate 原名 `shelf-core`，位置在 `shelf/crates/shelf-core`。
名字和位置都在暗示"这是 shelf 私有的东西"，但事实上从 `notes/` 四个服务一开始就在依赖它
起（见 `notes/docs/reMarkable笔记白皮书.md` 的既有记录），它就已经是跨线共享的基座，只是
名分没有跟上事实。用户判断后拍板正名：crate 改名 `rmsvc-core`（reMarkable service core），
目录挪到仓库顶层，跟 `shelf/`、`notes/`、`gateway/`、`enhance/`、`defw/` 并列——不建新的
"基座"分类父目录，摊平放，跟 `device-core/`（块③阅读+块⑤PKM 共用底座，历史先例）同一个模式。

四条延续自 shelf-core 时代、继续适用的工程原则：
1. **端口/适配器分层**：领域模块不碰 HTTP 类型，`http` 是唯一适配层。
2. **模板方法 + 仓库模式**：`asset::{AssetStore, AssetUploadFlow}` 让 font/wallpaper/koreader/
   母版库四家共用同一套上传流程骨架，拒收/成功文案由各自仓库实现决定。
3. **单一事实源**：`formats` 格式白名单、`fs::plain_name`/`unique_path`、`paths` 的 XDG 路径表——
   一处定义，多处消费，不允许各消费方各写一份。
4. **不引用旧项目 crate、不对接旧路径**：`xochitl`/`fswatch` 两个模块需要的能力都是"剥离移植"
   独立实现，不路径依赖 `device-core`/`weread-device`。这条原则这次也用在了自己身上——
   `enhance/hl-snap`/`enhance/handwriting-stroke` 需要的 `chinese-ime/langhook` 三个工具文件，
   在 `chinese-ime/` 挪出仓库后同样改成了剥离移植的独立副本，不是巧合，是同一条纪律。

## 00b｜现状总览（2026-09-11）

**三方共用，单向依赖，无循环**：`shelf/services/{book,koreader}-serve`、`gateway/`、
`notes/services/{ink,transcribe,mind,note}-serve` + `notes/crates/vendorcfg`、
`enhance/{wallpaper,font}-serve`——全部通过 `path = "../rmsvc-core"`（或从更深子目录数够层数）
路径依赖，没有反向依赖，本 crate 不知道、也不关心任何消费方的存在。

**不建 workspace**：跟 `device-core/` 一样，本 crate 独立编译（`cargo build --manifest-path
rmsvc-core/Cargo.toml`），各消费方各自的 Cargo workspace/独立项目各管各的 `target/`，没有
根 workspace 把大家绑在一起。

**模块一览**（详见各模块文档注释）：

| 模块 | 职责 |
|---|---|
| `paths` | XDG 基目录规范的单一路径表，三方共用同一张表；`app_config_dir("notes")` 这类通用接口预留给非默认命名空间的消费方 |
| `registry` | 服务自注册/发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`），网关据此拔插；`SvcClient`/`enc`（2026-09-09 加，见下）是跨服务 HTTP 客户端骨架 |
| `http` | tiny_http 适配层：`Router`/`bind`/`ApiError`/`Reply`/`JsonBody`——领域模块唯一允许碰 HTTP 类型的地方 |
| `multipart` | 流式 multipart/form-data 解析，多文件落盘不进内存（设备 MemoryMax 友好） |
| `asset` | 资产仓库抽象（Repository）+ 上传流程模板（Template Method）+ receipt |
| `xochitl` | 原生书库免重启注入（`/upload` GET-then-upload 归档、防复制风暴判据、`unique_document_name`） |
| `xochitl_conf` | 改 `xochitl.conf` 的原子写工具（休眠屏 `SleepScreenPath` 键；只动 `[General]` 单键、首次改前留 `.shelf-bak`，文件含凭证不打印行内容） |
| `fswatch` | inotify 目录监听，常驻+限时两种模式 |
| `events` | 事件总线（EventBus + SSE），供网关 `Hub` 汇聚 |
| `config` | 配置读写模板（`load_or_default`/`load_or_seed`/`save`） |
| `fs` | 原子写、`plain_name`、`unique_path` |
| `clock` | 时间戳唯一出处 |
| `formats` | 文件扩展名白名单单一事实源（`IMAGE_EXTS`/`FONT_EXTS`…） |
| `ttf` | TTF/OTF 解析：`name` 表家族名、魔数校验、CJK 覆盖率（cmap），`font-serve`/`koreader-serve` 共用 |
| `service` | 服务启动模板（解析 `--bind` → 建目录 → 自注册 → 起服务器，自带 `GET /health`） |
| `cache` | 单值 TTL 缓存（`/status` 这类重活接口降频，操作后可主动失效） |
| `tls` | 私有 CA（10 年）+ 叶证书（800 天，过期前 30 天或 SAN 变化时自动换叶） |
| `auth` | PBKDF2-HMAC-SHA256（60 万轮）密码哈希——旧版单轮加盐 SHA-256 仍可校验、改密后自动升级；Basic/Cookie 解析、内存会话表、失败限速器 `FailLimiter`（2026-09-09 审计加固） |
| `mdns`/`netinfo` | 极简 mDNS 应答器（`shelf.local`）、本机 IPv4 表（读 `/proc/net`，不 fork 进程） |

**测试**：76 个单测（2026-09-22 数，随代码增长），`cargo test --manifest-path rmsvc-core/Cargo.toml` 独立跑，CI `rust` job
单列一步（仿 `device-core` 先例）。

## 01｜架构决策：为什么正名，而不是继续留在 shelf 底下

真正促成这次决定的是一次连锁判断，不是单一动机：

1. 用户先问"wallpaper/font 这两个领域服务能不能挪进 `enhance/`"（概念上它们更像系统增强，
   不是"书架内容管理"业务）。
2. 排查发现 `wallpaper-serve`/`font-serve` 深度依赖 `shelf-core` 提供的整套 HTTP/服务注册/
   资产上传框架——不是"引用了几个工具函数"那种浅依赖，是**建在这套框架之上**。真要挪走还
   保持功能，要么复刻一份框架（分叉 shelf-core，往后两份要分别维护，违背基座只有一份的
   初衷），要么让 `enhance/` 反向依赖 `shelf/crates/shelf-core`（依赖方向倒挂——网关到现在
   都只是"消费" `enhance/` 的产出，比如读 `battop` 的 `summary.json`，不是反过来）。
3. 追问"能不能先把 shelf 整体瘦身/拆分，分基座和业务"——这问题问到了根子上：shelf-core
   本来就已经是事实上的共享基座（`notes/` 四个服务一直在依赖它），只是名分没跟上。把它
   正名搬顶层，`wallpaper-serve`/`font-serve` 挪进 `enhance/` 后继续依赖顶层 `rmsvc-core`，
   跟 `notes/` 现在的用法完全对称——不需要分叉框架，也不需要反向依赖，一次正名同时解决
   两个问题。

**crate 名怎么选**：候选过 `rmsvc-core`（reMarkable service core）、`hub-core`、`panel-core`，
用户选 `rmsvc-core`——延续仓库里 `device-core` 的命名风格（`<领域>-core`），且不像
`hub-core`/`panel-core` 那样隐含"网关"的意味（本 crate 跟网关是平级消费关系，不是网关的
附属物）。

**目录位置怎么选**：候选过"建一个『基座』父目录把 `rmsvc-core`/`gateway` 都塞进去"，用户
问过这个问题，最终选择摊平放顶层——理由见 §00 最后一段（`device-core` 先例 + "分块是心智
地图不等于目录嵌套"的既有项目纪律）。

**没有做的事**：没有改 XDG 运行时命名空间（`~/.config/shelf/`、`$XDG_RUNTIME_DIR/shelf/
services/`）、没有改默认密码字面量 `shelf`、没有改 mDNS 域名 `shelf.local`——这几处都会
牵连已部署设备的真实路径/配置，需要专门的迁移方案，这次范围明确排除在外，详见 §05。

## 02｜跨 workspace 路径依赖怎么算深度

这条纯粹是操作上容易算错的地方，记一笔：`path = "../rmsvc-core"` 里 `..` 的个数取决于
**消费方自己的目录深度**，不是固定值。搬迁前（`shelf/crates/shelf-core`）和搬迁后
（顶层 `rmsvc-core/`）两种情况下，同一个消费方要填的相对路径段数常常不一样：

| 消费方位置 | 搬迁前（`shelf/crates/shelf-core`） | 搬迁后（顶层 `rmsvc-core/`） |
|---|---|---|
| `shelf/services/<name>/` | `../../crates/shelf-core` | `../../../rmsvc-core` |
| `notes/services/<name>/`、`notes/crates/vendorcfg/` | `../../../shelf/crates/shelf-core` | `../../../rmsvc-core`（碰巧深度不变，只是尾段变短） |
| `gateway/`（原 `shelf/services/shelf-gateway/`） | `../../crates/shelf-core` | `../rmsvc-core` |
| `enhance/{wallpaper,font}-serve/`（原 `shelf/services/*`） | `../../crates/shelf-core` | `../../rmsvc-core` |

搬迁那天真的因为算错深度导致过一次 `cargo build` 报"找不到 crate"，靠 `cargo build`
本身的报错信息（Cargo 会直接说清楚它去哪找过、没找到）定位修正，不是靠人肉数 `../`。

## 03｜维护纪律

改这里的任何模块前，先想清楚三条线（`shelf`/`notes`/`gateway`）+ 两个 `enhance/` 服务谁在用
它、会不会连累无关消费方——这是"基座"跟"业务 crate"最大的区别：`bookconv` 出问题只影响
书处理相关的几个地方，`rmsvc-core` 出问题理论上五个独立顶层项目全灭。

**独立顶层 Cargo 项目需要自己的 `.cargo/config.toml`**：这条是搬迁当天踩出来的坑，详见 §04。

## 03b｜2026-09-24 新增的公共能力

- `tls`：CA 带名称约束 + 旧 CA 自动迁移（细节与用户须知见网关白皮书 §03c）。新增依赖 `x509-parser`（本来就经 rcgen 在依赖树里）；测试用 `rustls-webpki` 做完整链校验，含"同一把 CA 私钥硬签 `evil.com` 会被拒"的反证。
- `auth::IpFailLimiter`：按来源 IP 的登录失败限速，取代全局 `FailLimiter`（只有网关在用）。
- `http`：`GuardRequest.remote`、`Request::remote_ip()`、常量 `REMOTE_IP_HEADER`——没给 `Request` 加字段，是因为各服务直接构造这个公开结构体，加字段会波及全部调用方。
- `fs::write_atomic_mode`：临时文件创建时就带指定权限（含密钥的文件不再有先宽后紧的窗口）；`config::is_corrupt`：判断配置文件存在但解析不了，给"启动时落盘一次"的调用方决定要不要跳过。
- `multipart::content_disposition`：下载用的 `Content-Disposition`（ASCII 兜底名 + RFC 5987 UTF-8 名），笔记导出与母版库原件下载共用。

## 04｜踩坑

- **独立顶层 crate 不会自动继承调用方目录的 cargo 配置**（2026-09-11，搬迁当天实测）：
  `gateway/`、`enhance/{wallpaper,font}-serve/` 独立成顶层 Cargo 项目后，第一次交叉编译在
  `ring`（rcgen 的传递依赖）这步直接报 `failed to find tool "aarch64-linux-musl-gcc"`——
  原来 `shelf/.cargo/config.toml`（`CC_aarch64_unknown_linux_musl = "aarch64-linux-gnu-gcc"`
  这条 env 覆盖）只在从 `shelf/` 目录发起 `cargo build` 时生效，`shelf/build.sh` 里
  `(cd ../gateway && cargo build …)` 这种跨目录子 shell 调用，子目录自己没有 `.cargo/
  config.toml` 就完全吃不到这条配置。`notes/` 之所以没踩这个坑，是因为它从一开始就是
  独立项目、一直带着自己那份 `.cargo/config.toml`。**修法**：每个独立顶层 Rust 项目
  （`gateway/`、`enhance/wallpaper-serve/`、`enhance/font-serve/`）各自一份 `.cargo/
  config.toml`，内容跟 `shelf/`/`notes/` 完全一致——不是共享一份，是各自独立的物理副本
  （Cargo 没有"引用别处配置"的机制）。这个坑不会在 `cargo test`（host 编译，不需要
  CC/AR 覆盖）里暴露，只在 `--target aarch64-unknown-linux-musl` 交叉编译时才会炸，
  容易被"host 测试全绿"误导为已经验证充分。
- **`version.workspace = true` 这类字段离开 workspace 就报错**：`gateway`/`wallpaper-serve`/
  `font-serve` 原来的 `Cargo.toml` 用 `version.workspace = true`/`edition.workspace = true`/
  `license.workspace = true` 继承自 `shelf/Cargo.toml` 的 `[workspace.package]`；挪出
  workspace 后这些字段全部要改成字面量，`[profile.release]`（`opt-level="z"`/`lto`/
  `strip`/`codegen-units=1`）同理要各自复制一份，不再能从 workspace 继承。**panic 策略后来改了**：当时复制的是 `panic="abort"`；2026-09-19 真机（《镖人》）踩到 abort 下 `catch_unwind` 完全无效、一次 panic 摔掉整个进程，`gateway`/`shelf`/`notes` 的 release profile 已改为 `panic="unwind"`（见各自 `Cargo.toml` 注释），代价是二进制体积略增；`enhance/{font,wallpaper}-serve` 目前仍是 `abort`（没有依赖 `catch_unwind` 的后台线程）。

## 05｜命名遗留 + 待办

**命名遗留（有意不动，范围外）**：
- XDG 运行时命名空间仍然是 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、
  `$XDG_RUNTIME_DIR/shelf/services/`）——这是已部署设备上的真实文件路径，重命名它需要
  给已有安装写迁移逻辑（旧路径读不到就去新路径找，或者提供一次性搬家脚本），这次没有
  一并做。
- `shelf.target`（systemd 目标）、默认密码字面量 `"shelf"`、mDNS 域名 `shelf.local`——
  同样牵连已部署设备，理由同上。

**待办**：
- 上面这几处命名遗留要不要处理、什么时候处理，还没有排期，等用户下次明确要动再展开
  迁移方案设计（不是简单改字符串，要考虑已部署设备的兼容读取）。
- ~~本次重构（正名 + wallpaper/font 迁移）全程只做了 host 侧验证（`cargo test`/交叉编译
  产物检查），**没有推到真机验证**~~——**追记（2026-09-11）**：`packaging/install-all.sh`
  真机跑通后这条已经不成立。`rmsvc-core` 是 `gateway`/`book-serve`/`koreader-serve`/
  `font-serve`/`wallpaper-serve`/笔记线四服务共同的基座，这些服务在真实设备上全部部署+
  启动成功（用户确认"已成功安装"，健康检查全部 `active`），"运行时行为不变"不再是推断，
  是真机坐实的结论。
