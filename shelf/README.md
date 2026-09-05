# shelf · 书架

reMarkable Paper Pro Move 的**统一投递与阅读质量层**：一个网页/一条命令，把书投给原生 xochitl 或 KOReader，
把字体、壁纸"上传即可用"，把 KOReader 的调优配置固化成可一键恢复的代码。**独立于中文化套件安装**，
按服务可插拔。

> 工程原则（2026-09-03 用户定，见白皮书 §00）：XDG 基目录规范 · 设计模式去重解耦 · 专项专用可插拔多服务 ·
> **不引用旧项目任何 crate**（能力只许剥离移植）· 不与旧运行期路径对接（不读写 `/home/root/weread/**`）。

## 架构：网关 + 领域服务

```
浏览器 / shelf CLI ──► shelf-gateway  https://0.0.0.0:8778 · shelf.local（私有 CA TLS + 登录页密码/CLI Basic）  UI + /api/services + /api/<seg>/* 反向代理
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
├── crates/shelf-core/                 共享底座：XDG paths · registry · 流式 multipart(+receive_part_to) · AssetStore/上传模板 · config(读写模板) · fs(原子写) · http 适配 · xochitl 注入 · fswatch · tls/auth/mdns/netinfo/ttf
├── services/{shelf-gateway,book-serve,koreader-serve,font-serve,wallpaper-serve}/   各服务 crate（weread-serve 仅 README 占位）
├── systemd/                           shelf.target + 5 个 .service
├── install.sh · uninstall.sh          设备端安装/卸载（--only 按服务；写 /usr 前实检 dm-verity）
├── deploy.sh                          host 一键：build → tar-over-ssh → 设备 install.sh
├── host/                              CLI `shelf`（纯 stdlib、系统 python3）+ pytest；host/calibre/ = Calibre 前置流水线（自 reading/tools/calibre 迁入）
├── xovi/font-menu-dynamic{,-3.27}.qmd  字体菜单读 fonts.json 动态追加（缺文件回退内建三项）
├── wallpaper/                         5 行 sleep 钩子 + README（逻辑在 wallpaper-serve 子命令）
├── koreader/                          配置即代码：profile/{settings.reader.patch,defaults.custom,gestures.patch}.lua + fonts.txt/dicts.txt + merge.lua
├── weread-web/                        P5 门控 spike（rmweb × Move）
└── docs/
    ├── reMarkable书架白皮书.md          书架侧设计决策 + 真机记录（服务/UI/投递/字体）
    └── bookconv优化白皮书.md            书籍优化引擎：清洗层/优化遍/脚注/图片/格式转换/★xochitl 渲染硬规则/版本演进
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

- 地址：`https://<设备IP>:8778/`，或伪域名 **`https://shelf.local:8778/`**（网关自带 mDNS 应答；iOS/macOS/Windows/Linux 直接可用，
  **安卓系统不解析 .local**——安卓手机走 host 热点时在 host 加 dnsmasq 别名，见白皮书 §03j）。
- 登录页只要密码、无用户名：**首次默认 `shelf`，登录后强制改**（≥6 位、不能是默认）。改密：网页右上「改密码」/ `shelf passwd` /
  设备上 `shelf-gateway passwd <新密码>`；忘记：`shelf-gateway reset-password`（回默认并再次强制改）。改密后其它设备会话失效。
- 证书：私有 CA 签发（`~/.config/shelf/tls/ca.pem`）。登录页「下载 CA 证书」装进手机/电脑信任库**一次**，此后不再有"不安全"提示
  （叶证书 800 天自动续签、CA 不变）；不装就点「高级 → 继续访问」。
- CLI：`shelf -p <密码> …` / 环境变量 `SHELF_PASSWORD` / `config.toml` 的 `password` / 不给则交互输入（Basic，网关只看密码）。
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
shelf/host/bin/shelf push 论文.pdf 书.epub [--to-pdf] [--no-optimize] [--keep-spacing] [--no-reflow] [--no-split] [-n]
   **只落母版库**（中间层），去向在网页「传书 → 母版库」选（xochitl / KOReader）——与网页规则一致，没有 -t 目标、没有直投读器的选项，一并根治"目标/输出路径当参数"的坑。
   有 Calibre → 洗书后落母版库：EPUB 深洗 / AZW3·MOBI·FB2 转 EPUB / **PDF 默认结构化重排**（born-digital→EPUB→洗书；
                 扫描件 k2pdfopt/裁边→PDF，`--no-reflow` 原样）；`--to-pdf` 定稿固定版式 PDF（手写批注用）。>60MB PDF 自动分卷。
   `--no-optimize` 不洗原样传母版库。无 Calibre → 原样传母版库（网页里可再点优化）。
   清洗层+质量门已同源（白皮书 §03i/§03q/§03r）。
shelf font add 字体.ttf | ls | rm <file>                  # 只装原生阅读器：~/.local/share/fonts + fc-cache + fonts.json（KOReader 字体用 shelf koreader font）
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
| P2 | 字体/壁纸上传即可用：font-serve（fontconfig+fonts.json，只管原生；KOReader 字体由 koreader-serve 单独装）、wallpaper-serve（954×1696 池化/轮换/bind 子命令）、动态字体菜单 qmd（3.27/3.28）、sleep 钩子+开机 bind 单元、旧壁纸工具迁移、CLI font/wallpaper | ✅ **真机通**（3.27.3.0，2026-09-03）：字体上传→菜单差量→选中即渲染全程免重启（S-A/S-B）；壁纸缩放/激活/bind + 唤醒日志触发轮换真机通 |
| P3 | KOReader 配置即代码：`koreader/profile/` 三份补丁 + `merge.lua`（设备端 luajit 深合并，dry-run/备份/回读/幂等）+ `/config/{file}` 端点（运行中拒写）+ 词典上传 + CLI pull/diff/sync | ✅ **真机通**（2026-09-03）：pull→profile 校正→diff 零差异 |
| P4 | 原生高质量门：`shelf push` host 路强制 `check_output.py`（`--skip-check` 逃生）；设备路回执带转换/优化摘要；网关只读展示阅读增强开关 | ✅ 离线完成 |
| P4b | **清洗层 + 质量门移植进 bookconv**（`wash.rs`/`check.rs`，对标 host Calibre 规则；Pipeline 加 `Check` 步；优化器 v6；`optimize=auto\|keep-spacing\|plain\|off`、`check=off`） | ✅ 真机通（坏书被门拦 / 《飘·上册》伪 DRM 剥离 90s 进库；白皮书 §03i） |
| P5 | 微读网页版门控 spike：`weread-web/spike.sh recon\|fetch\|run\|restore`（看门狗 600s 自动拉起 xochitl） | 脚本就绪，**需用户设备旁执行** |
| 追加 | HTTPS+密码、字体/KOReader 分开装、KOReader 多级目录、字体去"内建"、壁纸唤醒日志轮换、KOReader 直传字体 | ✅ 真机通（2026-09-03） |
| 追加2 | 登录页（无用户名、首次默认 `shelf` 登录后必改）、私有 CA + `/ca.crt` 免提示、mDNS `shelf.local` 伪域名 | ✅ 真机通（白皮书 §03j） |
| 追加3 | 字体两 bug 根治：font-serve 接管 fontconfig 中文回退（weak 绑定、覆盖率降序）、cmap 覆盖率检测/警告、字体菜单按内容刷新 | ✅ 真机通（白皮书 §03k） |
| 追加4 | 网页改版（深浅色/卡片/两张对比表）、传书页并入原生字体上传并改名 **xochitl**（书上字体下）、去掉冗余 KOReader 选项、字体菜单长家族名 elide 截断（离线 qmldiff 验证） | ✅ 真机通（白皮书 §03m） |
| 诊断 | "传书卡传字体不卡"根因＝reMarkable 云同步每本书出站 2-3MB 挤满弱热点上行；**大书上传优先 USB** `https://10.11.99.1:8778` | ✅ 复现坐实 + USB 验证（白皮书 §03l） |
| 追加5 | 服务管理台（固定「管理」tab）：基石(xovi/appload/qrr/KOReader)红绿引导 + 模块三态(未装/已装未开/已开)、开关(仅停后台)、网页卸载(调 `shelf-uninstall`)、安装走引导；embolden 默认开；`cangjie-xovi-reenable`→`xovi-reenable`(归基石层，撤回 shelf 对外层耦合) | ✅ 部署+守卫真机通（白皮书 §03o） |
| 追加6 | UI 易用性：二级 tab（xochitl 传书/字体、KOReader 书库/字体/词典，治手机长拉）、管理台三态/开关/卸载完整说明 + 常开性能实测数据、基石引导补 reManager 链接 | ✅ 部署真机通（白皮书 §03o 末） |
| 质量 | 代码质量核查一轮：`config`/`fs` 共享模块 + `receive_part_to`/`all_ok` 原语收编各服务复制；koreader-serve `KoStore` 收编被绕过的 `AssetUploadFlow`；死代码清除；host `receipts`/`delete_named` 收编 CLI 重复 | ✅ 行为不变，离线全过 + 真机冒烟(R1/R5/R2 坐实，白皮书 §03p) |
| 书籍优化 | 深层优化(做精做细做强)：格式仅 EPUB/PDF；中英文各按习惯排版(LangMode)；目录 h1–h6 多级；**脚注 xochitl 内联常显〔…〕/ KOReader 弹窗**(FootnoteMode·xochitl 弹窗真机判死)；OPTIMIZE_VERSION→7；KOReader 收 EPUB 走同一优化；**host PDF 重排**(born-digital 结构化→EPUB / 扫描件 k2pdfopt) | ✅ 离线全过(cargo/pytest/交叉编译)；真机验证=白皮书 §03q Phase F 待做 |
