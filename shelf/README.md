# shelf · 书架

reMarkable Paper Pro Move 的**读书与阅读质量层**：一个网页 / 一条命令，把书弄进设备、按需优化、再选去哪个读器（原生 xochitl
或 KOReader）；把字体、壁纸"上传即可用"；把 KOReader 的调优配置固化成可一键恢复的代码。**独立于中文化套件安装**，按服务可插拔。

> 工程原则（2026-09-03 用户定，见白皮书 §00）：XDG 基目录规范 · 设计模式去重解耦 · 专项专用可插拔多服务 ·
> **不引用旧项目任何 crate**（能力只许剥离移植）· 不与旧运行期路径对接（不读写 `/home/root/weread/**`）。

## 读书线现状（2026-09-05 定稿）：三层 · 三动作正交

```
内容源 ──原样入库──►  母版库（中间层暂存池）  ──可选「优化」──►  落库（去向由人选）
 网页上传                ~/.local/state/shelf/books/staging/            📖 投入原生书库（xochitl：EPUB / PDF）
 抓网文（Readability）    · 母版永久保留，可反复落库、两读器对照           📚 加入 KOReader（母版库收的任何格式）
 电脑 shelf push          · 「优化」只对 EPUB（清洗+优化，档位三选）      · 落库＝纯复制母版字节，不再优化
 scp 进 inbox/            · 漫画（CBZ）只加入 KOReader，不投原生          · 落库记录徽章 / 清理已落库 / 剩余空间
 微信读书（Phase D 待接）
```

**统一规则**：所有书**只落母版库**——网页、CLI、inbox 都没有直投读器的路径；"入库是入库，优化是优化，落库是落库"。
host `shelf push` 是唯一能"入库时顺带优化"的源（Calibre 深洗 / 杂格式转 EPUB / PDF 结构化重排 / **漫画出 CBZ+PDF**）。

**格式三档**（`shelf_core::formats` 单一事实源，网页 accept、服务端上传门、inbox、CLI 同源；按设备装的 KOReader 注册表核过）：

| 档 | 格式 | 去向 |
|---|---|---|
| 原生直读 | EPUB / PDF | 两个读器都能去 |
| 电脑可转 | AZW3 / MOBI / AZW / PRC / FB2 | `shelf push` 转 EPUB 进原生；直接上传只能加入 KOReader |
| 仅 KOReader | TXT / CBZ / CBR / DjVu / HTML / RTF / DOC / DOCX / CHM / XPS | 只能加入 KOReader（漫画 CBZ 也在此档：**漫画不投原生**） |

**网页 tab**：「传书」（固定第一位：入库｜母版库）· xochitl（原生字体，font-serve）· KOReader（字体｜词典，koreader-serve）· 壁纸 · 「管理」（固定）。读器页不传书。

## 架构：网关 + 领域服务

```
浏览器 / shelf CLI ──► shelf-gateway  https://0.0.0.0:8778 · shelf.local（私有 CA TLS + 登录页密码/CLI Basic）  UI + /api/services + /api/manage + /api/<seg>/* 反向代理
                            │  按注册表转发（剥掉 <seg>，body 流式透传）
        ┌───────────────────┼─────────────────┬──────────────────┬──────────────────┐
   book-serve          koreader-serve       font-serve       wallpaper-serve     weread-serve(预留 8794)
  127.0.0.1:8790        :8791               :8792            :8793               微读作为内容源（下书→母版库，Phase D）
  母版库(入库/优化/落库) 从母版库落书·字体·   原生字体上传即装   壁纸上传即用
  + 投 xochitl + inbox   词典·配置同步       (fontconfig 回退链)  (bind-mount 休眠屏)
```

- **注册表**：服务启动写 `$XDG_RUNTIME_DIR/shelf/services/<name>.json`（含 pid、端口、UI tab），退出即删；
  网关按它出 tab、缺席回 404「未安装」。装/卸一个服务 = 一个二进制 + 一个 systemd 单元，其余零改动。
- **URL 段 ↔ 服务**（`shelf-gateway/src/manage.rs` 的 `MODULES` 单一事实源，管理台三态/代理/CLI `status` 都从它派生）：
  `books→book-serve`、`koreader→koreader-serve`、`fonts→font-serve`、`wallpapers→wallpaper-serve`、`weread→weread-serve`。
  经网关 `GET /api/fonts/health` = 后端直连 `GET 127.0.0.1:8792/health`（SSH 隧道调试同一套路由）。
- **systemd**：`shelf.target`（挂 multi-user）+ 各服务 `PartOf=shelf.target`；`systemctl disable --now font-serve` 即拔掉字体服务。
  所有单元只 `After=home.mount`，**绝不给 xochitl 加依赖**。网页「管理」页可开关/卸载单个服务（安装不走网页）。

### 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| books | `GET /status` · `GET /inbox` · `POST /inbox/{retry,delete}` · `GET /staging` → `{items, freeBytes}` · `POST /staging`（multipart 原样入库）· `POST /staging/optimize {name, mode}` · `POST /staging/to-pdf {name, mono}` · `POST /staging/deliver {name, folder?, keep?}` · `POST /staging/mark {name, target}` · `POST /staging/fetch-article {url}` · `POST /staging/delete {name}` |
| koreader | `GET /status` · `GET /books[?folder=]` · `POST /books/adopt {name, folder}`（从母版库落书）· `GET|POST /fonts` · `DELETE /fonts/{file}` · `GET|POST /dicts[?name=]` · `GET|POST /config/{settings\|defaults\|gestures}[?dry_run=1]` |
| fonts | `GET /` · `POST /` · `DELETE /{family}` · `PUT /config {emboldenCjkFallback}` · `GET /status` |
| wallpapers | `GET /` · `POST /[?activate=1]` · `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}` · `GET /status` |
| 网关自身 | `GET /api/services` · `GET /api/manage` · `GET /api/foundation` · `POST /api/manage/{seg}/{start\|stop\|uninstall}` · `/login` `/logout` `/password` `/ca.crt` |

上传回执统一 `{ok, items:[{name, ok, message, item?}], …}`（`shelf_core::asset::receipt`）；成功项 `name` 是落地名。

## 目录

```
shelf/
├── Cargo.toml · build.sh · .cargo/    内部 workspace（仓库根仍无 workspace）；musl 全静态交叉编译
├── crates/bookconv/                   ★ 通用内容层：多格式→EPUB/PDF、EPUB 优化器+清洗层+质量门、e-ink 图片处理、EPUB 组装、网文抽取
│   └── src/bin/epub_optimize.rs         host/设备共用 CLI（wash_epub.sh 末步）
├── crates/shelf-core/                 共享底座：paths(XDG) · formats(格式白名单) · registry · multipart(流式) · asset(AssetStore+上传模板+receipt)
│                                      · http(Router/bind/JsonBody/Guard) · config · fs(原子写/plain_name/unique) · xochitl 注入 · fswatch · tls/auth/mdns/netinfo/ttf
├── services/book-serve/               staging.rs(母版库领域：入库/优化/转PDF/落库) · spool.rs(inbox 队列) · api.rs(纯 HTTP 适配) · service_state.rs
├── services/koreader-serve/           koreader.rs(目录模型+KoStore) · config.rs(ConfigSync+merge.lua) · main.rs
├── services/{font-serve,wallpaper-serve,shelf-gateway}/
├── systemd/                           shelf.target + 5 个 .service + 壁纸开机 bind 单元
├── install.sh · uninstall.sh          设备端安装/卸载（--only 按服务；写 /usr 前实检 dm-verity）
├── deploy.sh                          host 一键：build → tar-over-ssh → 设备 install.sh（自动备份到 /home/root/cangjie-backups）
├── host/                              CLI `shelf`（纯 stdlib、系统 python3）+ pytest；shelf_cli/comic.py 漫画探针；host/calibre/ = Calibre 前置流水线
├── xovi/font-menu-dynamic{,-3.27}.qmd  字体菜单读 fonts.json 动态追加
├── wallpaper/                         5 行 sleep 钩子 + README（逻辑在 wallpaper-serve 子命令）
├── koreader/                          配置即代码：profile/{settings.reader.patch,defaults.custom,gestures.patch}.lua + fonts.txt/dicts.txt + merge.lua
├── weread-web/                        〔存档〕微读网页版内嵌浏览器 spike——方向已改为"微读=内容源"，见其 README
└── docs/
    ├── reMarkable书架白皮书.md          书架侧设计决策 + 真机记录（服务/UI/母版库/字体/管理台）；开头有「现状总览」
    └── bookconv优化白皮书.md            书籍优化引擎：清洗层/优化遍/脚注/图片/格式转换/★xochitl 渲染硬规则/版本演进
```

依赖方向（单向无环）：`services/* → shelf-core`；`book-serve、wallpaper-serve → bookconv`；`reading/device-rs → bookconv`（re-export 保路径）。
**shelf 不依赖 device-core / weread-device**；koreader-serve 不依赖 bookconv（落库纯复制）。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{shelf-gateway,*-serve,shelf-uninstall,cangjie-lo-alias.sh}` |
| 配置 | `~/.config/shelf/<service>.json`（book：书库文件夹/xochitl 主机/超时/**`nativeUploadLimitMb` 投原生体积门 150**；font；gateway）· `~/.config/shelf/tls/`（CA+叶证书） |
| 数据 | `~/.local/share/shelf/`（fonts.json、壁纸池）· `~/.local/share/fonts/`（用户字体，fontconfig 标准位） |
| 状态 | `~/.local/state/shelf/books/staging/`（**母版库**，不淘汰）· `books/{inbox,.work,failed}`（追平队列）· `wallpaper-state.json` · `koreader-backups/` |
| 运行时 | `/tmp/shelf-0/shelf/{services,upload,koreader}`（`XDG_RUNTIME_DIR` 缺省回落；重启即清） |
| 外部约定 | KOReader 根 `SHELF_KOREADER_ROOT`（缺省 `~/xovi/exthome/appload/koreader`）；xochitl 书库 `~/.local/share/remarkable/xochitl` |

## 访问与密码

- 地址：`https://<设备IP>:8778/`，或伪域名 **`https://shelf.local:8778/`**（网关自带 mDNS 应答；iOS/macOS/Windows/Linux 直接可用，
  **安卓系统不解析 .local**——安卓手机走 host 热点时在 host 加 dnsmasq 别名，见白皮书 §03j）。**传大书走 USB `https://10.11.99.1:8778`**（不占 WiFi，白皮书 §03l）。
- 登录页只要密码、无用户名：**首次默认 `shelf`，登录后强制改**（≥6 位、不能是默认）。改密：网页右上「改密码」/ `shelf passwd` /
  设备上 `shelf-gateway passwd <新密码>`；忘记：`shelf-gateway reset-password`（回默认并再次强制改）。改密后其它设备会话失效。
- 证书：私有 CA 签发（`~/.config/shelf/tls/ca.pem`）。登录页「下载 CA 证书」装进手机/电脑信任库**一次**，此后不再有"不安全"提示
  （叶证书 800 天自动续签、CA 不变）；不装就点「高级 → 继续访问」。
- CLI：`shelf -p <密码> …` / 环境变量 `SHELF_PASSWORD` / `config.toml` 的 `password` / 不给则交互输入（Basic，网关只看密码）。
- 只有网关对外；领域服务只绑 127.0.0.1，无需认证。

## 构建 · 部署 · 卸载

```sh
cd shelf && sh build.sh                       # host 测试 + aarch64 musl 全静态（5 个二进制）
sh deploy.sh 10.11.99.1                       # 组载荷 → 设备 /home/root/shelf-pkg → install.sh（先备份旧二进制/单元）
sh deploy.sh 10.11.99.1 --only font,wallpaper # 只装/更新部分服务；SHELF_NO_BUILD=1 跳过编译
ssh root@10.11.99.1 sh /home/root/shelf-pkg/shelf/uninstall.sh [--only font] [--purge]
cargo build --release -p bookconv --bin epub-optimize   # host 侧 push 洗书要用的 CLI（shelf/target/release/）
```
整包路径：`packaging/package.sh` 把 `shelf/` 作为第 5 层打进 `cangjie-full-*.tar.gz`，`install.sh` 直接调用 `shelf/install.sh`。
⚠ 设备上 `systemctl restart xochitl` 会丢 xovi（字体菜单/KOReader 入口一起没），重启 xochitl 一律 `/home/root/xovi/start`。

## host CLI

```sh
shelf/host/bin/shelf services | status | doctor
shelf/host/bin/shelf push 论文.pdf 书.epub [--to-pdf] [--no-optimize] [--keep-spacing] [--no-reflow] [--no-split] [--skip-check] [-n]
   **只落母版库**，去向在网页「传书 → 母版库」选。路线自动定（`push.plan`）：
   · 有 Calibre → 洗书：EPUB 深洗 / AZW3·MOBI·AZW·PRC·FB2 转 EPUB / **PDF 默认结构化重排**（born-digital→EPUB→洗书；扫描件 k2pdfopt/裁边→PDF，`--no-reflow` 原样）；
     产物必过 `check_output.py` 质量门（`--skip-check` 强推）。`--to-pdf` 定稿固定版式 PDF（手写批注用）。>60MB PDF 自动分卷（需 uv `calibre` 组的 pymupdf；切不了会报错不推，xochitl 收不下 188MB 整本）。
   · **漫画**（AZW3/MOBI/EPUB 里几乎全是整页图，`comic.py` 自动判）→ 转成 **CBZ** 进母版库，网页点「加入 KOReader」；**漫画不投原生**（用户定）。
     `--comic / --no-comic` 覆盖判断；CBZ 输入原样入库。
   · `--no-optimize` 或无 Calibre → 原样传母版库（网页里可再点优化）。
shelf font add 字体.ttf | ls | rm <家族名>                # 只装原生阅读器：~/.local/share/fonts + fc-cache + fonts.json + 中文回退链
shelf wallpaper add 图.jpg [--activate] | ls | set <name> | mode sequential|random|fixed | rm <name>
shelf koreader pull | diff | sync [-n] [--fonts] [--dicts]   # 配置即代码（Lua 合并在设备端跑）
shelf koreader font add 字体.ttf | ls | rm <file>         # 只装进 KOReader
shelf inbox [--retry 名 | --delete 名]                    # scp 追平队列里失败的书
shelf passwd [--new …]
```
配置 `$XDG_CONFIG_HOME/shelf/config.toml`（host/port/scheme/password/verify_tls/split_pdf_mb）。

## 演进记录（详见白皮书各节）

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 骨架 · bookconv 抽离（md5 对拍一致）· CI · 打包/编排接入 · 五服务注册/代理 | ✅ |
| P1 | 统一投递（当时是三目标手选 native/annot/koreader + 设备端转换管线）| ✅ 真机通（2026-09-03）；**2026-09-05 被母版库三层架构取代**（§03r/§03s），直投路已删 |
| P2 | 字体/壁纸上传即可用（fontconfig+fonts.json、954×1696 池化/轮换/bind、动态字体菜单 qmd、sleep 钩子） | ✅ 真机通 |
| P3 | KOReader 配置即代码（profile 三份补丁 + 设备端 merge.lua + pull/diff/sync） | ✅ 真机通 |
| P4/P4b | 质量门 + 清洗层进 bookconv（host/端同一优化器，`optimize` 档位） | ✅ 真机通（§03i） |
| P5 | 微读网页版内嵌浏览器 spike | ⏸ **方向已改**：微读定位为内容源（下书→母版库，§03r 决策 3），spike 不再推进 |
| 追加 1–6 | HTTPS+密码、登录页/私有 CA/mDNS、字体两 bug 根治、网页改版、管理台三态、二级 tab | ✅ 真机通（§03h–§03o） |
| 质量一轮 | config/fs/receive_part_to/all_ok 共享原语；KoStore 收编；死代码清除 | ✅（§03p） |
| 书籍优化 | 做精做细做强：LangMode 中英文排版、目录 h1–h6、脚注 Inline/Anchor、host PDF 重排、**首行缩进根因＝xochitl 只认外链 css（v10）** | ✅ 真机通（§03q） |
| **中间层** | **母版库三层架构**：入库/优化/落库正交、传书总入口、读器页不传书、落库记录、网文抓取、财新 PDF 重排修空白 | ✅ 真机通（§03r） |
| 质量二轮 | 母版库领域化、直投路删除、上传模板/格式白名单/取参单一事实源、格式三档展示 | ✅ 真机通（§03s） |
| 漫画通道 | AZW3/EPUB 漫画自动识别 → CBZ 给 KOReader；**漫画不投原生**（曾做过 CBZ→PDF 分卷投原生，用户否决后删）；镖人 282MB EPUB 撞 xochitl 上传上限根因；分卷静默失效修 | ✅ 真机通（§03t） |
