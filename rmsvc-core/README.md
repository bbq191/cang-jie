# rmsvc-core —— reMarkable 设备端 Web 服务共享底座

`shelf/`、`notes/`、`gateway/` 三条线共用的基座 crate。2026-09-11 从 `shelf/crates/shelf-core` 正名
搬到顶层——它早就不是 shelf 私有物了（`notes/` 的四个服务从一开始就在依赖它），只是名分上（目录
位置）之前还挂在 `shelf/` 底下，容易让人误以为是 shelf 私有 crate。搬迁只改了路径和 crate 名（`shelf-core`
→ `rmsvc-core`，模块路径 `shelf_core::` → `rmsvc_core::`），不改任何逻辑。

## 提供什么

纯基础设施，不含任何"书"/"笔记"这类业务语义：

- `paths` —— XDG 基目录规范的单一路径表。
- `registry` —— 服务自注册/发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`），网关据此拔插；
  `SvcClient`/`enc` 是跨服务 HTTP 客户端骨架。
- `http` —— tiny_http 适配层：路由、JSON 回执、查询串（领域模块不碰 HTTP 类型，这是唯一适配层）。
- `multipart` —— 流式 multipart/form-data 解析（多文件落盘不进内存）。
- `asset` —— 资产仓库抽象（Repository）+ 上传流程模板（Template Method）。
- `xochitl` / `xochitl_conf` —— 原生书库免重启注入、休眠屏 `SleepScreenPath` 键。
- `fswatch` —— inotify 目录监听（常驻+限时两种）。
- `events` —— 事件总线（EventBus + SSE）。
- `config` / `fs` / `clock` / `formats` / `ttf` / `tls` / `auth` / `mdns` / `netinfo` —— 各类共用小工具。

**不引用旧项目任何 crate**（device-core / weread-device），需要的能力按"剥离移植"独立实现
（`xochitl`、`fswatch` 两模块自己注明来源）。

## 谁在用

`shelf/services/{book,koreader,font,wallpaper}-serve`、`gateway/`、`notes/services/{ink,transcribe,
mind,note}-serve` + `notes/crates/vendorcfg`——全部通过 `path = "../rmsvc-core"`（或从更深的子目录数
对层数）依赖，不建根 workspace。改这里的任何模块前，先想清楚三条线谁在用它、会不会连累无关服务。

XDG 路径本身仍然叫 `shelf`（`~/.config/shelf/`、`~/.local/share/shelf/`、`$XDG_RUNTIME_DIR/shelf/`）——
这是已部署设备上的真实文件路径，这次搬迁**不改**，重命名它是另一件更大的事（涉及真机迁移），没有
一并做。

## 测试

`cargo test --manifest-path rmsvc-core/Cargo.toml`（独立 crate，CI 单独一条 `rust` job 步骤，仿
`device-core` 先例）。

## 文档

决策记录/踩坑/正名搬迁的完整过程见 [`docs/reMarkable设备端Web服务基座白皮书.md`](docs/reMarkable设备端Web服务基座白皮书.md)。
