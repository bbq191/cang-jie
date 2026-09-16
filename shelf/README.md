# shelf · 书架

reMarkable Paper Pro Move 的**读书与阅读质量层**：一个网页 / 一条命令，把书弄进设备、按需优化、再选去哪个读器（原生 xochitl
或 KOReader）；把字体、壁纸"上传即可用"；把 KOReader 的调优配置固化成可一键恢复的代码。**独立于中文化套件安装**，按服务可插拔。

> 工程原则（2026-09-03 用户定，见白皮书 §00）：XDG 基目录规范 · 设计模式去重解耦 · 专项专用可插拔多服务 ·
> **不引用旧项目任何 crate**（能力只许剥离移植）· 不与旧运行期路径对接（不读写 `/home/root/weread/**`）。

## 读书线现状（2026-09-06）：三层 · 三动作正交

```
内容源 ──原样入库──►  母版库（中间层暂存池）  ──可选「优化」──►  落库（去向由人选）
 网页上传                ~/.local/state/shelf/books/staging/            📖 投入原生书库（xochitl：EPUB / PDF）
 抓网文（Readability）    · 母版永久保留，可反复落库、两读器对照           📚 加入 KOReader（母版库收的任何格式）
 电脑 shelf push          · 「优化」只对 EPUB（清洗+优化，档位三选）      · 落库＝纯复制母版字节，不再优化
 scp 进 inbox/            · 漫画（CBZ）默认只加 KOReader，够小可选投原生   · 落库记录徽章（含投原生后的渲染自检）/ 清理已落库 / 剩余空间
```

![shelf 数据流：三层·三动作正交](docs/diagrams/data-flow.svg)

**统一规则**：所有书**只落母版库**——网页、CLI、inbox 都没有直投读器的路径；"入库是入库，优化是优化，落库是落库"。
host `shelf push` 是唯一能"入库时顺带优化"的源（Calibre 深洗 / 杂格式转 EPUB / PDF 结构化重排 / **漫画出 CBZ**）。
母版库同名不覆盖、按数字前缀（`1_x`/`2_x`…）各自入库是有意设计（避免不同书撞名互相吞掉）；**网页上传和
`shelf push` 都会在传之前先核对一遍母版库现有条目，同名同大小＝已经成功落地过，自动跳过不重传**（2026-09-13
补，见白皮书 §04）——批量推送中途失败、原样重跑整条命令/整个上传队列是安全的，不会把已经成功的那几份
又传一遍变成编号副本；这条保护按文件名+字节数比较、不比内容 hash，同名不同大小（内容真的换了）仍照常传。
**`shelf push` 2026-09-14 起再加一层**：处理任何一本书之前先查一遍原始输入文件（未洗书前的
名字+字节数）是不是已经处理成功过——命中就连洗书/重排这类耗时步骤都跳过，不只是省上传流量，
真正省下处理时间（服务端 sidecar 记一份"这份母版库文件是哪个原始输入处理出来的"，`GET
/api/books/staging` 一并带出）；这层只有 `shelf push` 有，网页原样上传本来就不经处理，不受
这个局限影响。

**格式三档**（`rmsvc_core::formats` 单一事实源，网页 accept、服务端上传门、inbox、CLI 同源；按设备装的 KOReader 注册表核过）：

| 档 | 格式 | 去向 |
|---|---|---|
| 原生直读 | EPUB / PDF | 两个读器都能去 |
| 电脑可转 | AZW3 / MOBI / AZW / PRC / FB2 / **TXT** | `shelf push` 转 EPUB 进原生（TXT 按「第X章/卷」切章建两级目录，GB18030/UTF-8 自动识别）；直接上传只能加入 KOReader |
| 仅 KOReader | CBZ / CBR / DjVu / HTML / RTF / DOC / DOCX / CHM / XPS | 只能加入 KOReader（漫画 CBZ 也在此档：**默认不投原生，`shelf push` 出的灰阶 CBZ 体积够小时会顺带生成一份 PDF 给「投入原生书库」选项，超限的仍只出 CBZ、绝不分卷**，见白皮书 §03ad；TXT 直传也落这档，走 `shelf push` 才切章进原生） |

**网页 tab**（2026-09-10 重排为固定四段，见白皮书 §03al）：「传书」（固定第一位：入库拆三卡——上传/抓网文/电脑端 shelf push｜母版库）· 「笔记」（note-serve 注册，「导入 md 文档」子标签默认隐藏，开关控制）· 「其他」（xochitl(font-serve)/KOReader(koreader-serve)/壁纸(wallpaper-serve) 降一级包进来当二级子标签，只列真的装了的那几个）· 「管理」（固定；二级 tab：基石与模块/模型管理/系统增强/电池刺客〔`battop.running` 时才出现〕/实验室）。读器页不传书。

## 架构：网关 + 领域服务

```
浏览器 / shelf CLI ──► gateway（../gateway，2026-09-11 正名搬顶层）  https://0.0.0.0:443 · shelf.local（私有 CA TLS + 登录页密码/CLI Basic）  UI + /api/services + /api/manage + /api/<seg>/* 反向代理
                            │  按注册表转发（剥掉 <seg>，body 流式透传）
        ┌───────────────────┼─────────────────┬──────────────────┐
   book-serve          koreader-serve       font-serve       wallpaper-serve
  127.0.0.1:8790        :8791               :8792            :8793
  母版库(入库/优化/落库) 从母版库落书·字体·   原生字体上传即装   壁纸上传即用
  + 投 xochitl + inbox   词典·配置同步       (fontconfig 回退链)  (原生 SleepScreenPath 键)
        │                                   `../enhance/font-serve`  `../enhance/wallpaper-serve`
        │                                   2026-09-11 挪出 shelf（概念上更贴近系统增强）
        └── 笔记线（`../notes`，独立仓库线，只是挂在同一网关/同一载荷/同一 shelf.target 上）：服务列表/职责/端口见 `notes/README.md`
```

![shelf 架构：网关 + 领域服务](docs/diagrams/architecture.svg)

- **注册表**：服务启动写 `$XDG_RUNTIME_DIR/shelf/services/<name>.json`（含 pid、端口、UI tab），退出即删；
  网关按它出 tab、缺席回 404「未安装」。装/卸一个服务 = 一个二进制 + 一个 systemd 单元，其余零改动。
- **URL 段 ↔ 服务**（`../gateway/src/manage.rs` 的 `MODULES` 单一事实源，管理台三态/代理/CLI `status` 都从它派生）：
  `books→book-serve`、`koreader→koreader-serve`、`fonts→font-serve`、`wallpapers→wallpaper-serve`；其余独立线（如笔记线）加进来只是多几行同规则映射，见各自 README。
  经网关 `GET /api/fonts/health` = 后端直连 `GET 127.0.0.1:8792/health`（SSH 隧道调试同一套路由）。
- **systemd**：`shelf.target`（挂 multi-user）+ 各服务 `PartOf=shelf.target`；`systemctl disable --now font-serve` 即拔掉字体服务。
  所有单元只 `After=home.mount`，**绝不给 xochitl 加依赖**。网页「管理」页可开关/卸载单个服务（安装不走网页）。

### 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| books | `GET /events`（SSE） · `GET /status` · `GET /inbox` · `POST /inbox/{retry,delete}` · `GET /staging` → `{items, freeBytes}`（条目 `delivered.render`＝投原生后的渲染自检 `{uuid,pages,expected,status}`）· `POST /staging`（multipart 原样入库）· `POST /staging/optimize {name, mode}` · `POST /staging/deliver {name, folder?, keep?}`（EPUB 投完起线程等 xochitl 渲染、核对页数，结果推 `books/render` 事件）· `POST /staging/mark {name, target}` · `POST /staging/fetch-article {url, optimize?}`（`optimize` 缺省 false，请求了就抓完紧接着跑一遍「清洗＋优化」再落库，跟母版库列表里点「优化」是同一个函数，见白皮书 §03ap） · `POST /staging/delete {name}` · `GET /staging/render/{uuid}`（xochitl 渲染缓存 PDF，`doctor --render` 取回量测）· **原生回收站队列** `POST /trash/add {uuid, name}`（name 须与书库 visibleName 相符）· `GET /trash/pending`（Sidebar 代理 qmd 拉取，由 xochitl 自己的 `selectionMoveToTrash` 执行）· `GET /trash` |
| koreader | `GET /status` · `GET /books[?folder=]` · `POST /books/adopt {name, folder}`（从母版库落书）· `GET|POST /fonts` · `DELETE /fonts/{file}` · `GET|POST /dicts[?name=]` · `GET|POST /config/{settings\|defaults\|gestures}[?dry_run=1]` |
| fonts | `GET /` · `POST /` · `DELETE /{family}` · `PUT /config {emboldenCjkFallback}` · `GET /status` |
| wallpapers | `GET /` · `POST /[?activate=1]` · `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}` · `GET /status` → `{native:{enabled,path,restartPending}}` |
| 笔记线 ink / transcribe / notes | 见 `../notes/README.md`「主要 API」（条目库 / 转写 / 投影）；事件 `area=notes` |
| 网关自身 | `GET /api/services` · `GET /api/manage` · `GET /api/foundation` · `POST /api/manage/{seg}/{start\|stop\|uninstall}` · **`GET /api/events`（SSE 事件流：各服务 `GET /events` 汇聚，`{svc,area,kind,at}`，`books/render` 另带 `name/status/pages/expected`；网页零轮询、host `shelf events`）** · `GET /ui/locales/{lang}`（语言包 JSON，前端按 `zh-CN.json`/`en-US.json` 请求、服务端剥 `.json`；不认识的语言码落中文，见白皮书 §03ae） · **系统增强开关**（「管理」页系统增强/实验室/电池刺客几个二级 tab，2026-09-09 §03aj 起、2026-09-10 §03ak-§03am 扩展）：`GET /api/enhance/status` → `{hlSnapCjk, hwStrokeEnabled, notesImportMdEnabled, battop:{installed,running,lastSampleAt}}` · `PUT /api/enhance/qol {hlSnapCjk?, hwStrokeEnabled?, notesImportMdEnabled?}`（只 patch 传入的键，`reading-qol.json` 其余键原样保留；`hwStrokeEnabled` 是网页层派生态，翻译成 `enhance/handwriting-stroke/` 的 `hwStrokeNibMinRatio`/`hwStrokeSpeedMinRatio` 两个真实字段） · `POST /api/enhance/battop/{start\|stop}`（未装 battop 时 400，不装） · `GET /api/enhance/battop/summary`（4 个时间窗×应用/进程/唤醒源 top15，原样转发 battop 自己聚合的 `summary.json`，未采样过返回 `{available:false}`） · `/login` `/logout` `/password` `/ca.crt` |

上传回执统一 `{ok, items:[{name, ok, message, item?}], …}`（`rmsvc_core::asset::receipt`）；成功项 `name` 是落地名。

## 目录

```
shelf/
├── Cargo.toml · build.sh · .cargo/    内部 workspace（仓库根仍无 workspace）；musl 全静态交叉编译
├── crates/bookconv/                   ★ 通用内容层：多格式→EPUB/PDF、EPUB 优化器+清洗层+质量门、e-ink 图片处理、EPUB 组装、网文抽取
│   └── src/bin/epub_optimize.rs         host/设备共用 CLI（wash_epub.sh 末步）
├── services/book-serve/               staging.rs(母版库领域：入库/优化/落库) · sidecar.rs(落库记录边车) · render_check.rs(投原生后渲染自检) · pending_queue.rs(PendingQueue\<T\>：持久化+入队去重+剔除共用骨架，2026-09-09 §03ag) · trash.rs(原生回收站队列) · spool.rs(inbox 队列) · api.rs(纯 HTTP 适配) · service_state.rs
├── services/koreader-serve/           koreader.rs(目录模型+KoStore) · config.rs(ConfigSync+merge.lua) · annot.rs(**新增**，读 .sdr 高亮标注，annot.lua+luajit) · vocab.rs(**新增**，读生词本 sqlite) · sqlite_min.rs(**新增**，手写纯 Rust 只读 SQLite 解析器，交叉编译避坑见白皮书 §03ar) · main.rs
../enhance/{font-serve,wallpaper-serve}/  2026-09-11 从 services/ 挪出去（概念上更贴近系统增强，
                                      不是"书架内容管理"业务）；wallpaper-serve 原依赖 bookconv 的两个
                                      屏幕尺寸常量已复制成本地值，不再跨线依赖 bookconv，见各自 README。
../rmsvc-core/                         2026-09-11 从 crates/shelf-core 正名搬顶层（shelf/notes/gateway 三方共用，不是 shelf 私有）：
                                      paths(XDG) · formats(格式白名单) · registry(+SvcClient/enc：跨服务 HTTP 客户端骨架，供 notes 线四个服务消重复用，2026-09-09 §03ai) · multipart(流式) · asset(AssetStore+上传模板+receipt) · xochitl_conf(休眠屏键) · events(事件总线+SSE)
                                      · http(Router/bind/JsonBody/Guard) · config · fs(原子写/plain_name/unique) · clock(时间戳唯一出处) · xochitl 注入/找书/页数 · fswatch(常驻+限时) · tls/auth/mdns/netinfo/ttf
../gateway/                           2026-09-11 从 services/shelf-gateway 正名搬顶层（shelf/notes/enhance 三条线共用的唯一前端，不是 shelf 一个服务）：
                                      auth/proxy/manage/events(Hub 汇聚)/enhance/{mod,qol,battop}.rs(系统增强开关：hlSnapCjk/hwStrokeEnabled/notesImportMdEnabled+battop 均真开关，2026-09-09 §03aj 起、2026-09-10 §03ak-§03am 扩展，刻意不升独立 service)；ui/{index.html,style.css,app.js,auth.css} 真文件，编译期 include_str! 拼成单页（CI node --check）；ui/locales/{zh-CN,en-US}.json 是 i18n 语言包（2026-09-09 起，§03ae 先搭架子+覆盖外壳/顶层导航；2026-09-10 §03an 补完传书/笔记/其他/管理四个 tab 的全部正文，437 key；登录页/改密码页仍不迁移，见白皮书 §03ae/§03an），GET /ui/locales/{lang} 分发
├── systemd/                           shelf.target + book/koreader-serve 两个 .service；font/wallpaper-serve 的单元跟着 2026-09-11 挪进各自 `../enhance/<name>/` 目录；其余独立线自己的单元在各自仓库，随载荷一起装
├── install.sh · uninstall.sh          设备端安装/卸载（--only 按服务；写 /usr 前实检 dm-verity；--purge 不碰其余独立线的用户数据目录）；设备侧自包含脚本，随载荷推到设备上跑，不依赖 host 侧编排
../packaging/deploy.sh                host 一键：build → tar-over-ssh → 设备 install.sh（自动备份到 /home/root/cangjie-backups；`GATEWAY_BINS`/`ENHANCE_BINS`/`NOTES_BINS` 顺带打包 `../gateway`/`../enhance/{font,wallpaper}-serve`/`../notes` 的二进制与单元）。2026-09-11 从 shelf/deploy.sh 搬到 `packaging/`——它编排的是跨四个目录的安装，逻辑上属于"全项目安装编排"，见 `../packaging/README.md`；也是 `packaging/install-all.sh` 统一安装器调用的其中一步
├── host/                              CLI `shelf`（纯 stdlib、系统 python3）+ pytest；shelf_cli/comic.py 漫画探针（PDF 分支要 spawn pymupdf 子进程 pdf_comic_probe.py，2026-09-14 补）；host/calibre/ = Calibre 前置流水线 + 独立脚本（epub_skel 共享 EPUB 骨架 / txt_to_epub / comic_gray / pdf_comic_probe / render_probe+measure）
├── xovi/                              font-menu-dynamic{,-3.27}.qmd 字体菜单读 fonts.json 动态追加（3.28 / 3.27 真机通）· shelf-trash-agent.qmd 原生回收站代理（Sidebar 注入，拉 book-serve /trash/pending）；改 qmd 先用 qmldiff CLI 离线实跑（白皮书 §04）
├── koreader/                          配置即代码：profile/{settings.reader.patch,defaults.custom,gestures.patch}.lua + fonts.txt/dicts.txt + merge.lua
└── docs/
    ├── reMarkable书架白皮书.md          书架侧设计决策 + 真机记录（服务/UI/母版库/字体/管理台）；开头有「现状总览」
    └── bookconv优化白皮书.md            书籍优化引擎：清洗层/优化遍/脚注/图片/格式转换/★xochitl 渲染硬规则/版本演进
```

依赖方向（单向无环）：`services/* → ../rmsvc-core`；`book-serve → bookconv`；`reading/device-rs → bookconv`（re-export 保路径，reading/ 现已归档）。
**shelf 不依赖 device-core / weread-device**；koreader-serve 不依赖 bookconv（落库纯复制）；`../enhance/wallpaper-serve` 也不再依赖 bookconv（两个屏幕尺寸常量已复制成本地值）。

## 路径（XDG，设备 HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{gateway,*-serve,shelf-uninstall,lo-alias.sh}` |
| 配置 | `~/.config/shelf/<service>.json`（book：书库文件夹/xochitl 主机/超时/**`nativeUploadLimitMb` 投原生体积门 150**；font；gateway）· `~/.config/shelf/tls/`（CA+叶证书） |
| 数据 | `~/.local/share/shelf/`（fonts.json、壁纸池）· `~/.local/share/fonts/`（用户字体，fontconfig 标准位） |
| 状态 | `~/.local/state/shelf/books/staging/`（**母版库**，不淘汰）· `books/{inbox,.work,failed}`（追平队列）· `wallpaper-state.json` · `koreader-backups/` |
| 运行时 | `/tmp/shelf-0/shelf/{services,upload,koreader}`（`XDG_RUNTIME_DIR` 缺省回落；重启即清） |
| 笔记线 | 自成一套 `notes` XDG 路径，与书架的 `shelf` 命名空间互不相干，详见 `../notes/README.md` |
| 外部约定 | KOReader 根 `SHELF_KOREADER_ROOT`（缺省 `~/xovi/exthome/appload/koreader`；appload 0.5.3 经 PR #59 qmd 回填补丁在 3.28 复活，见 koreader/README）；xochitl 书库 `~/.local/share/remarkable/xochitl` |

## 访问与密码

- 地址：`https://<设备IP>/`，或伪域名 **`https://shelf.local/`**（网关自带 mDNS 应答；iOS/macOS/Windows/Linux 直接可用，
  **安卓系统不解析 .local**——安卓手机走 host 热点时在 host 加 dnsmasq 别名，见白皮书 §03j）。**传大书走 USB `https://10.11.99.1`**（不占 WiFi，白皮书 §03l）。2026-09-10 起绑标准 443 端口，网址不用带端口号。
- 登录页只要密码、无用户名：**首次默认 `shelf`，登录后强制改**（≥6 位、不能是默认）。改密：网页右上「改密码」/ `shelf passwd` /
  设备上 `gateway passwd <新密码>`；忘记：`gateway reset-password`（回默认并再次强制改）。改密后其它设备会话失效。
- 证书：私有 CA 签发（`~/.config/shelf/tls/ca.pem`）。登录页「下载 CA 证书」装进手机/电脑信任库**一次**，此后不再有"不安全"提示
  （叶证书 800 天自动续签、CA 不变）；不装就点「高级 → 继续访问」。
- CLI：`shelf -p <密码> …` / 环境变量 `SHELF_PASSWORD` / `config.toml` 的 `password` / 不给则交互输入（Basic，网关只看密码）。
- 只有网关对外；领域服务只绑 127.0.0.1，无需认证。

## 构建 · 部署 · 卸载

**前置依赖**（一次性）：`rustup target add aarch64-unknown-linux-musl` + 装 aarch64 交叉 gcc/ar（Arch：`pacman -S aarch64-linux-gnu-gcc`；只用来编 `ring` 的 C 部分，产物本身是 musl 全静态、跟设备 libc 版本无关）。链接器/CC/AR 配置在 `.cargo/config.toml`，不用手改。改代码前先看工程纪律（真机验证、分支策略、离线门槛等）——两条线（shelf/notes）都遵守同一份；2026-09-16 起 `dev` 分支已删除，日常开发直接在 `master` 上开 feature 分支。

```sh
cd shelf && sh build.sh                                # host 测试 + aarch64 musl 全静态（书架 2 个二进制；../gateway/../enhance/{wallpaper,font}-serve/../notes 存在时顺带编它们）
cd ../packaging && sh deploy.sh 10.11.99.1             # 组载荷 → 设备 /home/root/shelf-pkg → install.sh（先备份旧二进制/单元）；设备只在 WiFi 上时给 WiFi IP
sh deploy.sh 10.11.99.1 --only font,wallpaper          # 只装/更新部分服务；SHELF_NO_BUILD=1 跳过编译
ssh root@10.11.99.1 sh /home/root/shelf-pkg/shelf/uninstall.sh [--only font] [--purge]
cargo build --release -p bookconv --bin epub-optimize   # host 侧 push 洗书要用的 CLI（shelf/target/release/）
```
整包路径：2026-09-11 起是 `packaging/install-all.sh <host>`——统一编排固件安全门 + `enhance/` 三个独立
xovi 扩展/工具（hl-snap/handwriting-stroke/battop）+ `packaging/deploy.sh`（原 shelf/deploy.sh，处理
shelf 本体+网关+笔记线+两个领域服务），见 `../packaging/README.md`。旧的 `packaging/package.sh` 打
`cangjie-full-*.tar.gz` 那套单体打包方式已随 2026-09-11 大归档整体挪出仓库（现只在
`/home/afu/Projects/oldbak/packaging/`，且经核实那份现在实际是断的——它按旧路径找 `shelf/` 载荷，
`shelf/` 早就独立到仓库顶层了），不是这次 `install-all.sh` 的设计参照。
⚠ 设备上 `systemctl restart xochitl` 会丢 xovi（字体菜单/KOReader 入口一起没），重启 xochitl 一律 `/home/root/xovi/start`。

## 固件升级（OTA）与恢复

**升级零风险、数据零丢失，随时可升；升完要手工装一遍功能才回来**——不是"升了就能用"。设计上我们不在启动路径留任何东西
（xovi 预载在 `/etc` tmpfs、单元在 `/usr`），新固件永远以纯原厂起来；`/home` 原样。3.27.3.0 → 3.28.0.172 实录见白皮书 §03v。

| 项目 | 位置 | OTA 后 | 恢复 |
|---|---|---|---|
| 母版库 / KOReader / 字体 / 壁纸池 / 配置 / 证书 / 休眠屏 conf 键 `SleepScreenPath` | `/home` | 保留 | 无 |
| WiFi 看护钩子 `xovi/scripts/post-start/` · NM `powersave 2` | `/home` | 保留 | 无 |
| 字体菜单 qmd · 回收站代理 qmd | `/home`（hashtab 过期） | 文件在、未注入 | ① `xovi/rebuild_hashtable`（设备旁输密码）② `xovi/start` |
| 书架五服务 | `/usr` | **冲掉** | ③ `cd packaging && SHELF_NO_BUILD=1 sh deploy.sh 10.11.99.1`（或整体用 `sh install-all.sh 10.11.99.1`，见下） |
| xovi 开机持久化恢复链（`xovi-reenable.service`，重启自动重跑 `xovi/start`，2026-09-11 补，`../packaging/README.md`） | `/usr` | **冲掉** | ⑥ `cd packaging && sh deploy-xovi-persist.sh 10.11.99.1` |
| chrony 国内 NTP | rootfs `/etc` | **冲掉** | ④ `cd packaging && sh deploy-chrony-cn.sh 10.11.99.1`（2026-09-11 补，脚本已经从 `oldbak/` 重写回 `packaging/chrony-cn.sh`，不再需要手动从本机备份找） |
| 默认时区 Asia/Shanghai（`timezone-cn.sh`，2026-09-11 新增，`../packaging/README.md`） | rootfs `/etc` | **冲掉** | ⑦ `cd packaging && sh deploy-timezone-cn.sh 10.11.99.1` |
| wifi-watch 常驻看护（wlan0 假死自动 `nmcli con up`；固化所有 WiFi 连接 2.4G + 省电关——路由 5G 信道 36 不在设备精简 regdb 的 CN 允许段，白皮书 §03w） | `/usr` 单元 + `~/.local/bin` 脚本 | 单元**冲掉** | ⑤ ⚠️ `packaging/wifi-watch/` 目前仍只在 `oldbak/packaging/wifi-watch/`，未随 `install-all.sh` 恢复（这一条是真实缺口，跟上面几条不同——那几条已经补回仓库了） |

④⑥⑦ 三步（连同 ③ 书架五服务）现在也可以一条命令全做：`cd packaging && sh install-all.sh 10.11.99.1`（见该目录 `README.md`）。①②（`rebuild_hashtable`/`xovi/start`）仍然只能手动——前者要交互输密码，后者是它们的共同前提，`install-all.sh` 不代做。

升级前把与新固件不兼容的 xovi 扩展（如 appload）挪出 `extensions.d/`（放 `/home/root/xovi-disabled/`，绝不留在目录里）；appload 的 3.28 补丁见系统增强白皮书 §12.1。
**风险分层**（不要合成一个百分比）：书架这一层只用 xochitl 的 `/upload` 网页接口和系统标准组件，换固件重装即回（本次 100%）；
字体菜单这类 qmldiff 注入依赖 xochitl 内部 QML，大版本常要重适配（3.27→3.28 已是两版 qmd）；KOReader 本体独立无碍，
但侧栏入口靠第三方 appload，每个大版本可能要重打补丁（3.28 靠 PR #59 qmd 回填，系统增强白皮书 §12.1）。

## host CLI

```sh
shelf/host/bin/shelf services | status | doctor
shelf/host/bin/shelf push 论文.pdf 书.epub [--to-pdf] [--no-optimize] [--no-calibre] [--keep-spacing] [--no-reflow] [--no-split] [--skip-check] [-n] [--wait[=秒]] [--no-eink-gray] [--manga-ltr] [--no-comic-native]
#   漫画缺省过省刷新档：先跨页拆分（东立扫描类两页拼一图，识别装订缝拆开，默认从右往左、--manga-ltr 改从左往右）+ 白边裁切放大，
#   再 CBZ 逐页缩到屏盒、黑白页转 16 灰抖动 4-bit PNG（轻波形，用户目视翻页明显少闪）、彩页保色（《阿拉蕾①》1092 页 171MB→108MB，16 灰 1085/保色 7）；
#   灰阶 CBZ 体积估算转 PDF 后仍在设备原生上传上限内，顺带生成一份 PDF 给「投入原生书库」选项（--no-comic-native 关掉）；--no-eink-gray 要原图（连带不做跨页拆分/白边裁切）
#   --wait：设备离 USB 几秒就自动休眠关 WiFi，push 上传前先探 /health；不可达时每 5 秒探一次等它醒（点亮屏幕/接 USB），缺省最多 600 秒；不加 --wait 则直接报错、不传
   **只落母版库**，去向在网页「传书 → 母版库」选。路线自动定（`push.plan`）：
   · 有 Calibre → 洗书：EPUB 深洗 / AZW3·MOBI·AZW·PRC·FB2 转 EPUB / **PDF 默认结构化重排**（born-digital→EPUB→洗书；扫描件优先 k2pdfopt——**host 通常没装这个外部工具（本项目有意不内嵌，没有安装指引），没装时唯一的回退是裁边脚本，但裁边对纯扫描图片按设计主动拒绝产出，两条路都不通就直接报错退出**，不是静默降级；报错时按提示改用 `--no-reflow` 原样传，或自行装好 `k2pdfopt`（本仓库没有安装指引）再重跑）——**这条路只吃到非漫画的扫描 PDF（如扫描版论文/杂志）**，扫描版漫画 PDF
     2026-09-14 起会被下面的漫画判定先拦下来走 CBZ 管线，不会碰到这条报错路，见白皮书 §04「扫描版漫画 PDF」条）；
     产物必过 `check_output.py` 质量门（`--skip-check` 强推）。`--to-pdf` 定稿固定版式 PDF（手写批注用）。>60MB PDF 自动分卷（需 uv `calibre` 组的 pymupdf；切不了会报错不推，xochitl 收不下 188MB 整本）。
   · **漫画**（AZW3/MOBI/EPUB 里几乎全是整页图，或 PDF 抽样页几乎全是"有图无字"——`comic.py`
     自动判，PDF 分支 2026-09-14 补）→ 转成 **CBZ** 进母版库，网页点「加入 KOReader」；**默认不投原生，体积够小时会顺带出一份 PDF 给「投入原生书库」选项，超限的仍只出 CBZ、绝不分卷**（§03ad，2026-09-08）。
     `--comic / --no-comic` 覆盖判断；CBZ 输入原样入库。
   · `--no-optimize` 或无 Calibre → 原样传母版库（网页里可再点优化）。
   · `--no-calibre`：**EPUB 输入**只跑 `epub-optimize`（跟网页「母版库→优化」按钮/`wash_epub.sh` 末步同一个函数），
     跳过 `ebook-convert`，不用装 Calibre（`--keep-spacing` 同样生效）；非 EPUB 没法只靠这条路径转格式，一律原样传。
shelf font add 字体.ttf | ls | rm <家族名>                # 只装原生阅读器：~/.local/share/fonts + fc-cache + fonts.json + 中文回退链
shelf wallpaper add 图.jpg [--activate] | ls | set <name> | mode sequential|random|fixed | rm <name>
shelf koreader pull | diff | sync [-n] [--fonts] [--dicts]   # 配置即代码（Lua 合并在设备端跑）
shelf koreader font add 字体.ttf | ls | rm <file>         # 只装进 KOReader
shelf notes pull [--out 目录]                              # 笔记线 md 导出拉到本机 Obsidian vault（--out > config.toml 的 notes_vault > 缺省 $XDG_DATA_HOME/shelf/notes-vault，见笔记白皮书 §03ak；镜像覆盖不是合并，本机手改过的文件下次拉取会被覆盖）
shelf inbox [--retry 名 | --delete 名]                    # scp 追平队列里失败的书
shelf events [--once] [--area books|koreader|fonts|wallpapers|manage] [--raw]   # 订阅设备事件流（SSE），有变更就打印
shelf doctor --render [--keep]     # 真机排版回归探针：投探针书→等渲染自检→取回 xochitl 渲染缓存→pymupdf 量顶格/首行缩进→PASS/FAIL（固件 OTA 后跑一次）；量完探针自动排进原生回收站（设备回到书库视图即执行）
shelf passwd [--new …]
```
配置 `$XDG_CONFIG_HOME/shelf/config.toml`（host/port/scheme/password/verify_tls/split_pdf_mb/notes_vault——最后这个是 `shelf notes pull` 缺省落地目录，不设就用 XDG 缺省位置，见上）。

## 演进记录（详见白皮书各节）

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 骨架 · bookconv 抽离（md5 对拍一致）· CI · 打包/编排接入 · 五服务注册/代理 | ✅ |
| P1 | 统一投递（当时是三目标手选 native/annot/koreader + 设备端转换管线）| ✅ 真机通（2026-09-03）；**2026-09-05 被母版库三层架构取代**（§03r/§03s），直投路已删 |
| P2 | 字体/壁纸上传即可用（fontconfig+fonts.json、954×1696 池化/轮换、动态字体菜单 qmd；壁纸最初 bind-mount+sleep 钩子，2026-09-06 换原生键） | ✅ 真机通 |
| P3 | KOReader 配置即代码（profile 三份补丁 + 设备端 merge.lua + pull/diff/sync） | ✅ 真机通 |
| P4/P4b | 质量门 + 清洗层进 bookconv（host/端同一优化器，`optimize` 档位） | ✅ 真机通（§03i） |
| P5 | 微信读书（先内嵌浏览器 spike，后改内容源） | ⛔ **已砍（2026-09-05，§03u）**：微读不再是书架内容源；reading/ 旧下书线不受影响 |
| 追加 1–6 | HTTPS+密码、登录页/私有 CA/mDNS、字体两 bug 根治、网页改版、管理台三态、二级 tab | ✅ 真机通（§03h–§03o） |
| 质量一轮 | config/fs/receive_part_to/all_ok 共享原语；KoStore 收编；死代码清除 | ✅（§03p） |
| 书籍优化 | 做精做细做强：LangMode 中英文排版、目录 h1–h6、脚注 Inline/Anchor、host PDF 重排、**首行缩进根因＝xochitl 只认外链 css（v10）** | ✅ 真机通（§03q） |
| **中间层** | **母版库三层架构**：入库/优化/落库正交、传书总入口、读器页不传书、落库记录、网文抓取、财新 PDF 重排修空白 | ✅ 真机通（§03r） |
| 质量二轮 | 母版库领域化、直投路删除、上传模板/格式白名单/取参单一事实源、格式三档展示 | ✅ 真机通（§03s） |
| 漫画通道 | AZW3/EPUB 漫画自动识别 → CBZ 给 KOReader；**漫画不投原生**（曾做过 CBZ→PDF **分卷**投原生，用户否决后删——2026-09-08 §03ad 部分修订：小体积不分卷可选投原生，跟这次被否决的"分卷了也投原生"是两回事）；镖人 282MB EPUB 撞 xochitl 上传上限根因；分卷静默失效修 | ✅ 真机通（§03t） |
| 固件 3.28 | OTA 3.27.3.0→3.28.0.172 实录：appload 停用、hashtab 重建、deploy 重装；3.28 字体菜单 qmd 修 qmldiff 语法（`({})`/裸 `if(` 整份不应用）后通 | ✅ 真机通（§03v，§05 第 5 条） |
| 设备杂项 | 原生休眠屏隐藏键 `SleepScreenPath`（满屏+随轮换）；WiFi 连上恰 60 秒必掉＝cfg80211 regdomain 宽限（精简 regdb 的 CN 无 5150–5350，路由 5G 信道 36 被判非法）→ 连接锁 2.4G + `powersave 2`，`packaging/wifi-watch` 常驻固化；chrony 国内 NTP 幂等脚本 `packaging/chrony-cn.sh` | ✅ 真机通（§03w） |
| 壁纸退役 bind | wallpaper-serve 改写 `SleepScreenPath`（`shelf_core::xochitl_conf`），删 bind 单元/sleep 钩子/透明卡/`mount.rs`（安装器的旧残留清理块已于 2026-09-06 随体检删除，真机零残留） | ✅ 真机通（§03x） |
| 事件推送 | shelf-core `events`（EventBus+SSE）· 四服务在变更处发事件 · 网关 `Hub` 汇聚 `/api/events` · 网页 EventSource 零轮询 · CLI `shelf events`；tiny_http 流式回执必须 `Request::upgrade` 一帧一 flush | ✅ 真机通（§03z） |
| 阅读线六项 | 投原生后渲染自检（`pageCount` vs 正文字符数，<50% warn，边车+事件+徽章）· `shelf doctor --render` 排版回归探针（§03y 八轮手工诊断固化，真机 7/7）· `push --wait`（探 `/health` 等设备醒）· 中文 TXT 切章两级目录 · Phase E ④ 脚注（Gulliver 两器观感通过）· 漫画 16 灰省刷新档（默认开，171→108MB） | ✅ 真机通（§03aa） |
| 代码体检 | shelf-core `clock` · 死项清除 · `xochitl` 扫 metadata 合一 · book-serve `sidecar.rs` · 网关 UI 拆 `ui/` 真文件（CI node --check）· host `epub_skel`/`_run_json`/`transport._open` · install.sh 删迁移块 | ✅（§03ab） |
| **可插拔机制验证** | 笔记线（独立仓库线 `notes/`）挂上同一网关/build/deploy/install 机制，证明书架的插件式服务架构能接其他独立线，不用改书架自身代码；细节全在 `notes/README.md`/`notes/docs/reMarkable笔记白皮书.md`，本仓库不复述 | ✅ 真机验证通过 |
| 漫画跨页拆分+投原生 | `comic_gray.py` 加跨页识别拆分（东立扫描类两页拼一图，找装订缝拆开）+ 白边裁切放大；`cbz2pdf` 复活成体积门控 CLI，灰阶 CBZ 够小顺带出投原生 PDF，超限只出 CBZ、绝不分卷 | ✅ 真机通：火影忍者卷1（够小投原生）+ 阿拉蕾①/镖人（超限只出 CBZ 不分卷，2026-09-09，§03ad） |
| UI i18n 架子 | 网页 UI 语言包架子：主界面外壳+顶层导航可切中/英，登录页/模块正文暂不迁移 | ⚠ 部分真机验证（§03ae，2026-09-09）：语言包端点/前端资源已验证，浏览器里人眼确认切换效果未做 |
| 三维审计（设计模式/业务闭环/UI） | UI 人性化/触屏可用性一批小修（§03af）+ `book-serve` 两个队列消重复新增 `PendingQueue<T>`（§03ag）+ `proxy.rs` 模块注释修正（§03ah）+ `shelf-core::registry` 新增 `SvcClient`/`enc`（§03ai，消费方在 notes 线） | ✅ 离线全绿；§03af **前端渲染效果未经人眼确认**，其余三项纯内部重构/注释改动无需真机验证 |
| 管理页拆二级 tab + 系统增强开关 | `shelf push` 卡片挪去「传书」页；「管理」拆三个二级 tab（基石与模块/模型管理/系统增强）；系统增强页新增 `hlSnapCjk`/battop 真开关（CJK 手写笔迹渲染优化无代码地基，纯占位）；命令说明改用独立 `.cmdblock` 视觉语言，不再借用引用摘录样式 | ✅ 真机通（§03aj，2026-09-09）：部署健康检查+功能性 curl 验证+用户确认字体清楚；顺手发现的 `hlSnapCjk` 实际吸附不生效问题已查清修复（跟本轮改动无关，是 `cangjie-langhook.so` 整个从设备消失，见系统增强白皮书 §04 追记） |
| 实验室 tab + 导入md改文件上传 | 「实验室」二级 tab（CJK 手写笔迹优化此时已有代码地基，接成真开关；battop 卡片下移；导入md文档可见性开关）；「入库」拆三卡；笔记「导入」改名「导入 md 文档」+ textarea 改文件选择 | ✅ 真机通（§03ak/§03al，2026-09-10）：curl 全链路验证+用户真机写字确认+battop 真机实装完整闭环 |
| 电池刺客/标题/端口三连调 | 电池刺客最终落点「管理→电池刺客」二级 tab（耗电情况/唤醒源两个三级 tab，各自时间窗+按应用/按进程下拉，内容包 `.card` 统一风格）；「实验室」只留开关；总标题「书架」（内部项目名不变）→ 网页展示改「秘密花园」🌿；网关端口 8778→443（不用带端口号） | ✅ 真机通（§03am，2026-09-10）：curl 确认新标题/图标/443 端口/summary 数据端点全部生效，浏览器交互细节未经人眼确认（如实标注） |
| 正文全量 i18n + 两处小样式修复 | 传书/笔记/其他/管理四个 tab 全部正文（含 confirm/alert/badge title 深度嵌套文案）抽 key，437 个 `zh-CN`/`en-US` 对照 key（§03ae 架子的 13 个 key 之外全部补完）；模型管理子标签拆卡统一风格；模型卡「最近错误」行长文本溢出修复（`overflow-wrap:anywhere`） | ✅ 真机验证数据链路+内容对照（§03an，2026-09-10）：curl 交叉核对真机 served 语言包与部署 app.js 全部 416 处 `T()` 引用零缺失，浏览器人眼渲染确认未做（如实标注，延续 §03ae 缺口） |
| `shelf push --no-calibre` | host CLI 补"跳过 Calibre、只跑 epub-optimize"这条路（EPUB 输入直调跟网页「优化」按钮同一个函数，`--keep-spacing` 同样生效）；核实后纯优化能力本来不缺（设备按钮+独立 `epub-optimize` 二进制早就有），缺的只是 `shelf push` 没暴露这个入口 | ✅ 离线全绿（§03ao，2026-09-10）：8 个新单测 + CI 原命令 271 个全绿；纯 host 改动不碰设备行为，另做了非 mock 真调用验证（真二进制+真 EPUB+真 CLI dry-run） |
| 抓网文「同步优化」复选框 | `fetch_article` 加 `optimize` 参数，请求了就抓完紧接着跑一遍「清洗＋优化」（跟母版库「优化」按钮同一个函数），补齐网文正文没有任何排版样式（边距/段距/缩进）这个真实缺口；网页复选框缺省勾选、本机记住选择，用户可关掉要最原始结果 | ✅ 真机通（§03ap，2026-09-10）：curl 真机分别带/不带 `optimize` 各抓一次同一篇文章，`level` full/core 各自落地正确、体积跟 host 侧非 mock 验证完全一致；**不是"留白根因"的完整修复**，见下一行 §03aq——同一篇文章真机 A/B 渲染对比后确认图片密集网文的大片留白另有成因，不是这一步能解决的 |
| wash 补 figure/figcaption 边距 + 留白成因排查 | 用户拿真实网文反馈"抓完优化还是大量留白，图夹在中间"；核实 `wash_css()` 确实只清零过 `<p>`，`figure`/`figcaption` 从没被管过，补上（两条裸元素规则分开写，不能用逗号选择器，xochitl 解析器脆）；真机 A/B 逐页渲染对比证明这条修复对该文章的留白**零改善**，真正成因是图片块在页尾放不下时整体推到下一页、页尾剩余空间不回填——分页引擎自身行为，非 CSS 可调；顺手发现 aeon.co 轮播组件被整组抽出+来源不明的"N of M"页码文本混入正文，留作已知问题未修 | ✅ 真机 A/B 验证 + 离线全绿（§03aq，2026-09-10）：`cargo test -p bookconv` 110/110（新增 1 个测试）；如实记录"改了但没解决投诉"，不算已闭环的留白问题，算已闭环的 figure/figcaption 缺口修复 |
