# gateway —— 设备端 Web 网关

`shelf/`、`notes/`、`enhance/` 三条线共用的唯一对外入口。2026-09-11 从 `shelf/services/shelf-gateway`
正名搬到顶层——它托管的网页早就不只是"书架"了（笔记 tab、系统增强/实验室/电池刺客开关都挂在这
一个网关下面，网页总标题也已经是"秘密花园"而不是"书架"），继续叫 `shelf-gateway`、放在 `shelf/`
里，跟它的实际定位不符。

## 职责

① 托管单页 UI（`ui/` 下 `index.html`/`style.css`/`app.js`/`auth.css` 真文件，编译期 `include_str!`
拼进二进制，`ui/locales/{zh-CN,en-US}.json` 是 i18n 语言包）；② `/api/services` 列服务注册表；
③ `/api/<service>/*` 反向代理到该服务的 loopback 端口（流式转发 body）。服务缺席 → 404「未安装」，
UI 据 `/api/services` 隐藏对应 tab。

对外 **HTTPS**（私有 CA 签发，首启生成，`/ca.crt` 可下载装信任）+ **登录页密码**（无用户名；首次
默认 `shelf`，登录后必改；CLI 用 Basic）+ **mDNS `shelf.local`** 伪域名。部署固定 `0.0.0.0:443`。

子命令：`serve [--bind]` · `passwd <新密码>` · `reset-password`（回默认并强制改）· `regen-tls`（重签
叶证书）。

## 目录

```
src/
  main.rs      入口 + 子命令 + ServiceSpec（name: "gateway"）
  auth.rs      密码策略 + Basic 认证
  proxy.rs     /api/<seg>/* 反向代理
  manage.rs    URL 段 ↔ 服务的 MODULES 单一事实源（管理台三态/代理/CLI status 都从它派生）
  events.rs    Hub：汇聚各服务事件，/api/events 对网页 SSE
  enhance/     系统增强开关后端（mod/qol/battop.rs）——hlSnapCjk/hwStrokeEnabled/notesImportMdEnabled/
               battop 的网页控制面，实际工具源码分别在 enhance/{hl-snap,handwriting-stroke,battop}/、
               notes/（notesImportMdEnabled 控制笔记 tab 的导入子标签），本目录只是薄客户端
               （systemctl/文件读写，不关心那些工具源码放在仓库哪个位置）
ui/            单页前端（见上）
systemd/gateway.service   开机单元（PartOf=shelf.target；shelf.target 名字暂未跟着改，见下）
```

## 依赖

只依赖顶层 `../rmsvc-core`（共享基座）——不依赖 `shelf/crates/bookconv`、不依赖 `notes/` 任何 crate。
`shelf/Cargo.toml` 的内部 workspace **不再包含本目录**，是完全独立的顶层 Cargo 项目。

## 构建 / 部署

自己没有独立的 `build.sh`/`deploy.sh`——跟 `notes/` 一样，被 `shelf/build.sh`（顺手 `cd ../gateway &&
cargo build`）和 `shelf/deploy.sh`（打包 `../gateway/target/.../gateway` 二进制 + `../gateway/systemd/
gateway.service`）代管，因为它历史上就是跟着书架整包一起装的，这次正名只是挪了源码位置，没有另起
一套独立的安装流程。真要单独重编：`cargo build --release --target aarch64-unknown-linux-musl`（需要
本目录 `.cargo/config.toml` 的 CC/AR 覆盖，跟 `shelf/`、`notes/` 同一份）。

## 命名遗留

XDG 运行时注册表路径、`shelf.target` systemd 目标、登录默认密码字面量 `shelf`、mDNS 域名
`shelf.local` 这几处**仍然叫 "shelf"**——这次重构只改了 crate/二进制/systemd 单元这一层的名字，
没有动这些会牵连已部署设备真实路径/配置的更深层命名，是刻意留白，不是遗漏。
