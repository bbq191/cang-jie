# reMarkable 设备端 Rust 架构与解耦

> 2026-08-25。三个设备端 Rust crate（`device-core` / `weread-device` / `pkm-device`）的**结构、依赖、模块职责**，
> 以及本轮解耦的**方法论（如何解耦）与模块演变**。跨块文档（触及块3阅读 + 块5 PKM + 共享底座），归顶层 `docs/`。
> 各块功能设计见对应白皮书（[阅读](../reading/docs/reMarkable阅读白皮书.md) / [PKM](../pkm/docs/reMarkablePKM白皮书.md) /
> [系统增强](../xovi-extensions/docs/reMarkable系统增强白皮书.md)）；本文只讲 **crate/模块的形状与解耦**。

## 一、Crate 结构（现状）

```
device-core/            共享底座 crate（块3阅读 + 块5 PKM 都用的低层设备能力）
  └ epubindex inject notebook_rm fswatch          ← 零 crate 内依赖、自足；精简外部依赖

reading/device-rs/      weread-device（块3 阅读）—— 微信读书下书 + EPUB 优化 + 面板服务
  ├ 依赖 device-core，并 `pub use device_core::{那4个}` re-export（保 weread_device::epubindex 路径不变）
  ├ 下载协议: codec sign obfuscate fetch login qr renew agent
  ├ 内容/优化: htmlproc epub optimize    面板后台: autoopt
  └ bins: wr-download wr-serve wr-renew wr-fetch wr-probe wr-spike

pkm/                    pkm-device（块5 PKM）—— ★待办 + 汇总本 + 查字典生词本
  ├ 依赖 device-core（normal）；weread-device 仅 dev-dependency（1 个 fixture 测试用）
  ├ 星→卡片: stardetect cardsync cardnote starscan
  ├ 汇总本: cardindex cardagg cardreview cardstats
  ├ 注入底层: notebook_sync         查字典(块4跨块): dict cardvocab locate vocabscan
  └ bins: wr-stars-daemon wr-stars wr-nbtest
```

**依赖图（单向、无环）**：

```
        device-core  ←──────────────┐
           ▲   ▲                     │ (dev-dependency 仅测试)
           │   └──────── pkm-device ─┘
        weread-device                  ✗ reading 不反向依赖 pkm
```

- `device-core` 谁都不依赖（除 remarkable_lines 等外部 crate），是最底座。
- `weread-device` 依赖 `device-core`（并 re-export 那 4 个模块）。
- `pkm-device` **生产依赖只有 `device-core`**；`weread-device` 是 **dev-dependency**（仅 `tests/stars_fixture.rs`
  测 `epub::assemble`）→ **`cargo build --release --bin wr-stars-daemon` 不编译整条 weread 管线**。

## 二、解耦方法论（如何解耦——四条可复用原则）

本项目设备端 Rust 反复用到的解耦手法，按施力点分四类：

### 原则 1 — 编排下沉可测模块（bin 只做派发）

**问题**：二进制（daemon / 服务器）把"读配置 + 事件循环 + 业务编排 + IO"糊在一个 `main.rs`/bin 里，
业务逻辑困在 bin 里 **host 测不了**，且和不相关的东西交织（god-object）。

**手法**：把业务**编排**抽进 lib 模块，bin 只剩「配置 + 循环 + 派发」三件事；模块里再把**纯判据**
（不含 IO 的决策）提成纯函数单测。判据式：*bin 里出现"读目录/解析/循环/调 lib"混着业务规则 → 抽*。

- daemon（`wr_stars_daemon.rs` 563→198 行）→ `notebook_sync`（设备笔记本 I/O 底层）+ `starscan`
  （星→卡片编排）+ `vocabscan`（生词本扫描）。纯判据 `is_source_book` 独立可测。
- 服务器（`wr_serve.rs` 662→528 行）→ `autoopt`（无感自动优化）。纯判据 `meta_eligible`（5 条件跳过）单测。

### 原则 2 — 共享底座抽 crate + re-export + dev-dependency 隔离

**问题**：A crate 只用 B crate 的少数自足模块，却 path 依赖整个 B → **全量编译 B**（连它一大堆用不到的东西）。

**手法**：把被 2+ 方复用的**自足模块**（零 `crate::` 内依赖）抽成独立小 crate；原 crate **re-export**
（`pub use core::…`）保调用路径不变（下游零改动）；只在 test/example 用到的重依赖降为 **dev-dependency**
（生产构建不编译它）。

- `device-core` = epubindex/inject/notebook_rm/fswatch（都零内部依赖）。`weread-device` re-export 它们，
  自己的 bins 照旧 `weread_device::epubindex::…`。`pkm-device` 主依赖 `device-core`、把 `weread-device`
  降 dev-dependency → 生产 daemon 不再拖 5657 行 weread 管线。

### 原则 3 — 删死代码（孤儿子系统，别让它伪装成实时）

**问题**：被砍的功能残留一坨文件，git 跟踪但**无 `mod` 声明=从不编译**，还互相 `crate::` 引用缠成一团，
navigation 时看着像能用（我就被 `reverse.rs` 坑过）。

**手法**：确认**无任何实时代码/bin/example 引用**（逐一 grep），有用的算法先抢救到活模块，然后 `git rm`
（源码在 git 历史，要复活找得回）。

- 删 `reverse/notebook/bake/rmread/sync`（1798 行，被砍的微信读书双向同步）。抢救：`canon`/`locate_range`
  → `pkm/src/locate.rs`；高亮提取由 `pkm/src/cardhl.rs`（6 色版）取代。

### 原则 4 — 路径/配置无关（参数化，别硬编）

**问题**：模块里硬编设备路径、写死排除列表 → 既耦合部署布局，又没法拿 fixture 目录测。

**手法**：设备路径、排除集合等**作参数传入**，模块本身路径无关、可测。

- `vocabscan::collect(dir, en_path, zh_path)`（词典路径参数化）、`notebook_sync::collect_notebook_texts(dir, exclude)`
  （排除列表参数化）→ 都能拿临时目录 host 测。

## 三、模块演变（2026-08-25 本轮解耦）

| 环节 | 前 | 后 | 原则 |
|---|---|---|---|
| pkm daemon | 563 行 god-object（22 函数混在 bin） | 198 行派发层 + `notebook_sync`/`starscan`/`vocabscan` | ①④ |
| pkm 依赖 | path 依赖整个 `weread-device`（5657 行全编） | 生产只依赖 `device-core`；weread-device 降 dev-dep | ② |
| 共享底座 | epubindex/inject/notebook_rm/fswatch 埋在 reading | 独立 `device-core` crate（reading re-export） | ② |
| reading 死代码 | 5 个孤儿文件 1798 行（伪装成实时） | 已 `git rm`（git 历史留档） | ③ |
| wr-serve | 662 行 bin（HTTP + 后台优化逻辑混） | 528 行路由 + `autoopt` 模块 | ①④ |

**净效果**：三 crate 依赖单向无环、职责清晰；bin 都是薄派发、业务逻辑在可测模块；pkm 生产构建摆脱 reading；
reading 从"挂着死双向同步"回到"下书+优化+面板"的诚实形状。查字典（`dict`/`cardvocab`/`locate`/`vocabscan`）
概念属**块4 系统增强**、代码骑 pkm daemon（跨块，见系统增强白皮书 §08）。

## 四、验证（本轮全过）

- 三 crate 编译 + 测试全绿：device-core 6 / weread-device 21 / pkm-device 59（lib）+ 5（stars_fixture dev-dep）。
- 两个部署 bin（wr-stars-daemon / wr-serve）交叉编译 `aarch64-unknown-linux-musl` 全静态。
- 依赖结构核查：`cargo tree --edges normal` 里 pkm 只见 device-core；`--edges dev` 才见 weread-device。
- 全程**行为中性**（逻辑逐字搬移 + 等价重构）。
- **2026-08-25 已把设备全部 5 个二进制同步到本轮最新**（wr-stars-daemon + wr-serve + wr-renew + wr-download +
  wr-stars；解耦重构改了 reading crate 链接故都变）：备份 `cangjie-backups/*.bak.pre-*`、md5 本地=设备一致、
  两个服务停/换/启后健康（is-active=active / MainPID 变 / NRestarts=0 / ExecMainStatus=0）。**行为中性真机坐实**：
  daemon 既有汇总本无重生成、wr-serve 的 autoopt 后台线程正常起。

## 五、后续可选（未做，记录方向）

- reading live 部分（下载协议 codec/sign/fetch/login、htmlproc、optimize、面板路由）单一职责、内聚，**不建议再拆**（过度抽象）。
- `pending-trash` 队列逻辑在 pkm `notebook_sync` 与 reading `autoopt` 各有一份（都写同一 `/home/root/weread/pending-trash.json`）——
  若要彻底去重可上提到 `device-core`，但会扩底座职责，暂不动。
