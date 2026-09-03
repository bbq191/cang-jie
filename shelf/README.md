# shelf · 书架

reMarkable Paper Pro Move 的**统一投递与阅读质量层**：一个网页/一条命令，把书投给原生 xochitl 或 KOReader，
把字体、壁纸"上传即可用"，把 KOReader 的调优配置固化成可一键恢复的代码。**独立于中文化套件安装**，
按服务可插拔。

> 工程原则（2026-09-03 用户定，见白皮书 §00）：XDG 基目录规范 · 设计模式去重解耦 · 专项专用可插拔多服务 ·
> **不引用旧项目任何 crate**（能力只许剥离移植）· 不与旧运行期路径对接（不读写 `/home/root/weread/**`）。

## 架构：网关 + 领域服务

```
浏览器 / shelf CLI ──► shelf-gateway  https://0.0.0.0:8778（自签 TLS + Basic 密码）  UI + /api/services + /api/<seg>/* 反向代理
                            │  按注册表转发（剥掉 <seg>，body 流式透传）
        ┌───────────────────┼─────────────────┬──────────────────┬──────────────────┐
   book-serve          koreader-serve       font-serve       wallpaper-serve     weread-serve(预留)
  127.0.0.1:8790        :8791               :8792            :8793               :8794
  原生投递(转换/优化/注入)  投书 books/·配置同步   字体上传即装      壁纸上传即用        微读网页版(P5 门控)
```

- **注册表**：服务启动写 `$XDG_RUNTIME_DIR/shelf/services/<name>.json`（含 pid、端口、UI tab），退出即删；
  网关按它出 tab、缺席回 404「未安装」。装/卸一个服务 = 一个二进制 + 一个 systemd 单元，其余零改动。
- **URL 段 ↔ 服务**：`books→book-serve`、`koreader→koreader-serve`、`fonts→font-serve`、`wallpapers→wallpaper-serve`、`weread→weread-serve`。
  经网关 `GET /api/fonts/health` = 后端直连 `GET 127.0.0.1:8792/health`（SSH 调试同一套路由）。
- **systemd**：`shelf.target`（挂 multi-user）+ 各服务 `PartOf=shelf.target`；`systemctl disable --now font-serve` 即拔掉字体服务。
  所有单元只 `After=home.mount`，**绝不给 xochitl 加依赖**。

## 目录

```
shelf/
├── Cargo.toml · build.sh · .cargo/    内部 workspace（仓库根仍无 workspace）；musl 全静态交叉编译
├── crates/bookconv/                   ★ 通用内容层：多格式→EPUB/PDF、EPUB 优化器、e-ink 图片处理、EPUB 组装
│                                      （2026-09-03 从 weread-device 抽出；weread 线改为依赖它并 re-export 保路径）
├── crates/shelf-core/                 共享底座：XDG paths · registry · 流式 multipart · AssetStore · http 适配 · xochitl 注入 · fswatch
├── services/{shelf-gateway,book-serve,koreader-serve,font-serve,wallpaper-serve}/   各服务 crate（weread-serve 仅 README 占位）
├── systemd/                           shelf.target + 5 个 .service
├── install.sh · uninstall.sh          设备端安装/卸载（--only 按服务；写 /usr 前实检 dm-verity）
├── deploy.sh                          host 一键：build → tar-over-ssh → 设备 install.sh
├── host/                              CLI `shelf`（纯 stdlib、系统 python3）+ pytest；host/calibre/ = Calibre 前置流水线（自 reading/tools/calibre 迁入）
├── xovi/font-menu-dynamic{,-3.27}.qmd  字体菜单读 fonts.json 动态追加（缺文件回退内建三项）
├── wallpaper/                         5 行 sleep 钩子 + README（逻辑在 wallpaper-serve 子命令）
├── koreader/                          配置即代码：profile/{settings.reader.patch,defaults.custom,gestures.patch}.lua + fonts.txt/dicts.txt + merge.lua
├── weread-web/                        P5 门控 spike（rmweb × Move）
└── docs/reMarkable书架白皮书.md        设计决策 + 真机记录
```

依赖方向（单向无环）：`services/* → shelf-core`；`book-serve → bookconv`；`weread-device → bookconv`。
**shelf 不依赖 device-core / weread-device**。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{shelf-gateway,*-serve,cangjie-lo-alias.sh}` |
| 配置 | `~/.config/shelf/<service>.json` |
| 数据 | `~/.local/share/shelf/`（fonts.json、壁纸池）· `~/.local/share/fonts/`（用户字体，fontconfig 标准位） |
| 状态 | `~/.local/state/shelf/`（spool、轮换状态） |
| 运行时 | `/tmp/shelf-0/shelf/{services,upload}`（`XDG_RUNTIME_DIR` 缺省回落；重启即清） |
| 外部约定 | KOReader 根 `SHELF_KOREADER_ROOT`（缺省 `~/xovi/exthome/appload/koreader`）；xochitl 书库 `~/.local/share/remarkable/xochitl` |

## 访问与密码

- 浏览器开 `https://<设备IP>:8778/`（自签证书，首次点「高级 → 继续访问」），用户 `shelf`，密码=安装输出打印的初始密码
  （设备上 `shelf-gateway show-password` 可再看；改密：`shelf-gateway passwd <新密码>` 后 `systemctl restart shelf-gateway`）。
- CLI：`shelf -p <密码> …` / 环境变量 `SHELF_PASSWORD` / `config.toml` 的 `password` / 不给则交互输入。
- 只有网关对外；领域服务只绑 127.0.0.1，无需认证。

## 构建 · 部署 · 卸载

```sh
cd shelf && sh build.sh                       # host 测试 + aarch64 musl 全静态（5 个二进制）
sh deploy.sh 10.11.99.1                       # 组载荷 → 设备 /home/root/shelf-pkg → install.sh
sh deploy.sh 10.11.99.1 --only font,wallpaper # 只装/更新部分服务
ssh root@10.11.99.1 sh /home/root/shelf-pkg/shelf/uninstall.sh [--only font] [--purge]
```
整包路径：`packaging/package.sh` 把 `shelf/` 作为第 5 层打进 `cangjie-full-*.tar.gz`，`install.sh` 直接调用 `shelf/install.sh`。

## host CLI

```sh
shelf/host/bin/shelf services | status | doctor
shelf/host/bin/shelf push 书.azw3 论文.pdf -t native|annot|koreader [-f 文件夹] [-q auto|host|device] [--no-optimize] [--no-split] [-n]
   quality=auto：host 有 ebook-convert → native 走 wash_epub.sh→体检→推；annot 走 epub2pdf_move.sh / pdf_crop_move.py→体检→推；
                 否则直推网关（设备端 Rust 转换/优化兜底）。>60MB PDF 自动 pymupdf 分卷（可选依赖）。
shelf font add 字体.ttf | ls | rm <file>                  # 装进 ~/.local/share/fonts + fc-cache + fonts.json + KOReader fonts/
shelf wallpaper add 图.jpg [--activate] | ls | set <name> | mode sequential|random|fixed | rm <name>
shelf koreader pull | diff | sync [-n] [--fonts] [--dicts]   # 配置即代码（Lua 合并在设备端跑）
shelf koreader font add 字体.ttf | ls | rm <file>         # 只装进 KOReader（原生+KOReader 同装用 shelf font）
```
配置 `$XDG_CONFIG_HOME/shelf/config.toml`（host/port/ssh/default_target/quality/split_pdf_mb）。

## 阶段状态

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 骨架 · bookconv 抽离（md5 对拍一致）· CI · 打包/编排接入 · 五服务注册/代理本机冒烟 | ✅ 离线完成 |
| P1 | 统一投递：三目标手选（native/annot/koreader）、格式自动处理、host Calibre 优先/设备兜底、完整单页 UI、`shelf push` | ✅ **真机通**（3.27.3.0，2026-09-03）：三目标投递、拔插、WiFi 访问 |
| P2 | 字体/壁纸上传即可用：font-serve（fontconfig+fonts.json+KOReader 镜像）、wallpaper-serve（954×1696 池化/轮换/bind 子命令）、动态字体菜单 qmd（3.27/3.28）、sleep 钩子+开机 bind 单元、旧壁纸工具迁移、CLI font/wallpaper | ✅ **真机通**（3.27.3.0，2026-09-03）：字体上传→菜单差量→选中即渲染全程免重启（S-A/S-B）；壁纸缩放/激活/bind + 唤醒日志触发轮换真机通 |
| P3 | KOReader 配置即代码：`koreader/profile/` 三份补丁 + `merge.lua`（设备端 luajit 深合并，dry-run/备份/回读/幂等）+ `/config/{file}` 端点（运行中拒写）+ 词典上传 + CLI pull/diff/sync | ✅ **真机通**（2026-09-03）：pull→profile 校正→diff 零差异 |
| P4 | 原生高质量门：`shelf push` host 路强制 `check_output.py`（`--skip-check` 逃生）；设备路回执带转换/优化摘要；网关只读展示阅读增强开关 | ✅ 离线完成 |
| P5 | 微读网页版门控 spike：`weread-web/spike.sh recon\|fetch\|run\|restore`（看门狗 600s 自动拉起 xochitl） | 脚本就绪，**需用户设备旁执行** |
