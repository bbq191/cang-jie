# shelf · 书架

reMarkable Paper Pro Move 的**读书与阅读质量层**：一个网页，把书弄进设备、按需优化、再选去哪个读器（原生 xochitl
或 KOReader）；把字体、壁纸"上传即可用"；把 KOReader 的调优配置固化成可一键恢复的代码。**独立于中文化套件安装**，按服务可插拔。
（2026-09-18 前还有一条 host CLI 命令行路，已砍，见下方「host CLI」节。）

> 工程原则（2026-09-03 用户定，见白皮书 §00）：XDG 基目录规范 · 设计模式去重解耦 · 专项专用可插拔多服务 ·
> **不引用旧项目任何 crate**（能力只许剥离移植）· 不与旧运行期路径对接（不读写 `/home/root/weread/**`）。

## 读书线现状（2026-09-18 刷新：host 已砍）：两层 · 两动作正交

> **⚠️ 2026-09-18：host 部分（`shelf/host/`，Python CLI + Calibre 转换管线）已整个砍掉**——
> 用户明确表态以后不再使用 PC 端，只走网页/设备端交互。源码留档在本机 `/home/afu/Projects/oldbak/
> cang-jie/shelf-host/`，不随仓库走、不再维护。随这次砍掉的能力：`shelf push`（Calibre 深洗 /
> 杂格式转 EPUB / PDF k2pdfopt 结构化重排 / 漫画→CBZ 转换+跨页拆分+16 灰省刷新档）、
> `shelf doctor --render`（真机排版回归探针 CLI）、`shelf notes pull`（笔记线 md 导出拉到本机
> Obsidian vault，详见 `notes/README.md`）、其余 `shelf font/wallpaper/koreader/events/inbox`
> 等子命令。下面这节和"host CLI"整节是**改前的旧架构描述，为了不误导先整段砍掉**；被砍的历史
> 决策/完整命令参考仍能在 git 历史里找到这份 README 旧版本。
>
> **同一天再收紧一步**：用户接着明确要求"从此开始入库只入 PDF 和 EPUB，不论格式是否支持"——
> 原「仅 KOReader」档（CBZ/CBR/DjVu/HTML/HTM/RTF/DOC/DOCX/CHM/XPS）**整档砍掉**，两档收成一档，
> 见下方「格式」小节，`rmsvc_core::formats::KOREADER_ONLY_EXTS` 常量已删除。

```
内容源 ──原样入库──►  母版库（中间层暂存池）  ──可选「优化」──►  落库（去向由人选，永远保留）
 网页上传                ~/.local/state/shelf/books/staging/            📖 加入原生书库（xochitl：EPUB / PDF）
 抓网文（Readability）    · 母版永久保留，可反复落库、两读器对照           📚 加入 KOReader（EPUB/PDF；旧格式遗留条目也能加）
 scp 进 inbox/            · 「优化」只对 EPUB（清洗+优化，不分档位）       · 落库＝纯复制母版字节，不再优化
                         · 漫画（EPUB）自动识别保画质+裁边，超限按卷拆分   · 落库记录徽章（含加入原生后的渲染自检）/ 清理已落库 / 剩余空间
```

![shelf 数据流：三层·三动作正交（旧图，含已砍的 host 一路，未重画）](docs/diagrams/data-flow.svg)

**统一规则**：所有书**只落母版库**——网页、inbox 都没有直投读器的路径；"入库是入库，优化是优化，落库是落库"。
入库来源只剩网页上传/抓网文/scp 进 inbox 三条，**入库时不再有"顺带优化/顺带转格式"这一步**（那是
已砍的 `shelf push` 独有的能力）——所有格式转换/深洗需求，请先在电脑上自行处理好再上传 EPUB/PDF；
入库后可以在网页母版库页手动点「优化」。母版库同名不覆盖、按数字前缀（`1_x`/`2_x`…）各自入库是
有意设计（避免不同书撞名互相吞掉）；网页上传会在传之前先核对一遍母版库现有条目，同名同大小＝已经
成功落地过，自动跳过不重传（2026-09-13 补，见白皮书 §04）。

**格式：只收 EPUB / PDF**（`rmsvc_core::formats` 单一事实源，`BOOK_EXTS == NATIVE_EXTS`，网页 accept、服务端上传门、inbox 同源）。演进：2026-09-17 起不再有"电脑可转"这一档——AZW3/MOBI/AZW/PRC/FB2/TXT 不再自动转 EPUB，母版库直接拒收（书架白皮书 §03av）；**2026-09-18 起"仅 KOReader"那一档（CBZ/CBR/DjVu/HTML/HTM/RTF/DOC/DOCX/CHM/XPS）也整档砍掉**——用户明确要求"从此开始入库只入 PDF 和 EPUB，不论格式是否支持"：**这是策略收紧，不是技术能力判断**，KOReader 本来能读这些格式，但不想再维护两档，一律只收两个读器都能去的 EPUB/PDF。两档收成一档，`KOREADER_ONLY_EXTS` 常量已删除。已经在库里的旧格式条目（改规则前上传的）不受影响，仍可加入 KOReader，只是不会再有新的这类条目。

| 档 | 格式 | 去向 |
|---|---|---|
| 原生直读（唯一档） | EPUB / PDF | 两个读器都能去；母版库「优化」按钮在设备侧就地优化（字号解锁/保留原书颜色/注释移段末/漫画自动识别保画质+超限按卷拆分投原生，§03av/§03ax） |

**网页 tab**（2026-09-10 重排为固定四段，2026-09-18 入库卡从三张收窄成两张，见白皮书 §03al）：「传书」（固定第一位：入库拆两卡——上传/抓网文｜母版库）· 「笔记」（note-serve 注册，「导入 md 文档」子标签默认隐藏，开关控制）· 「其他」（xochitl(font-serve)/KOReader(koreader-serve)/壁纸(wallpaper-serve) 降一级包进来当二级子标签，只列真的装了的那几个）· 「管理」（固定；二级 tab：基石与模块/模型管理/系统增强/电池刺客〔`battop.running` 时才出现〕/实验室）。读器页不传书。

## 架构：网关 + 领域服务

```
浏览器 ──► gateway（../gateway，2026-09-11 正名搬顶层）  https://0.0.0.0:443 · shelf.local（私有 CA TLS + 登录页密码；host CLI 已砍，2026-09-18）  UI + /api/services + /api/manage + /api/<seg>/* 反向代理
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
| books | `GET /events`（SSE） · `GET /status` · `GET /inbox` · `POST /inbox/{retry,delete}` · `GET /staging` → `{items, freeBytes}`（条目 `delivered.render`＝投原生后的渲染自检 `{uuid,pages,expected,status}`）· `POST /staging`（multipart 原样入库）· `POST /staging/optimize {name}`（2026-09-19 起不再分档位，永远跑完整清洗+优化）· `POST /staging/deliver {name, folder?}`（folder 空＝配置缺省文件夹；folder 非空且真不存在会先经 mkdir 队列同步等真建出来再投，最长等 20 秒，等不到就原样落书库根，见 `staging.rs::ensure_folder`；EPUB 投完起线程等 xochitl 渲染、核对页数，结果推 `books/render` 事件；2026-09-19 起不再有 `keep` 参数，母版库条目永远保留）· `POST /staging/mark {name, target}` · `POST /staging/fetch-article {url, optimize?}`（`optimize` 缺省 false，请求了就抓完紧接着跑一遍「清洗＋优化」再落库，跟母版库列表里点「优化」是同一个函数，见白皮书 §03ap） · `POST /staging/delete {name}` · `GET /staging/render/{uuid}`（xochitl 渲染缓存 PDF，原给已砍的 `doctor --render` CLI 取回量测用，接口本身还在，只是没有自动化消费方了）· **原生回收站队列** `POST /trash/add {uuid, name}`（name 须与书库 visibleName 相符）· `GET /trash/pending`（Sidebar 代理 qmd 拉取，由 xochitl 自己的 `selectionMoveToTrash` 执行）· `GET /trash` · **原生建文件夹队列** `POST /mkdir/add {name}` · `GET /mkdir/pending`（MainView 代理 shelf-mkdir-agent.qmd 拉取，调 xochitl 自己的 `Library.createCollection` 执行）· `GET /mkdir` |
| koreader | `GET /status` · `GET /books[?folder=]` · `POST /books/adopt {name, folder}`（从母版库落书）· `GET|POST /fonts` · `DELETE /fonts/{file}` · `GET|POST /dicts[?name=]` · `GET|POST /config/{settings\|defaults\|gestures}[?dry_run=1]` |
| fonts | `GET /` · `POST /` · `DELETE /{family}` · `PUT /config {emboldenCjkFallback}` · `GET /status` |
| wallpapers | `GET /` · `POST /[?activate=1]` · `PUT /current {name}` · `PUT /mode {mode}` · `DELETE /{name}` · `GET /{name}` · `GET /status` → `{native:{enabled,path,restartPending}}` |
| 笔记线 ink / transcribe / notes | 见 `../notes/README.md`「主要 API」（条目库 / 转写 / 投影）；事件 `area=notes` |
| 网关自身 | `GET /api/services` · `GET /api/manage` · `GET /api/foundation` · `POST /api/manage/{seg}/{start\|stop\|uninstall}` · **`GET /api/events`（SSE 事件流：各服务 `GET /events` 汇聚，`{svc,area,kind,at}`，`books/render` 另带 `name/status/pages/expected`；网页零轮询）** · `GET /ui/locales/{lang}`（语言包 JSON，前端按 `zh-CN.json`/`en-US.json` 请求、服务端剥 `.json`；不认识的语言码落中文，见白皮书 §03ae） · **系统增强开关**（「管理」页系统增强/实验室/电池刺客几个二级 tab，2026-09-09 §03aj 起、2026-09-10 §03ak-§03am 扩展）：`GET /api/enhance/status` → `{hlSnapCjk, hwStrokeEnabled, notesImportMdEnabled, battop:{installed,running,lastSampleAt}}` · `PUT /api/enhance/qol {hlSnapCjk?, hwStrokeEnabled?, notesImportMdEnabled?}`（只 patch 传入的键，`reading-qol.json` 其余键原样保留；`hwStrokeEnabled` 是网页层派生态，翻译成 `enhance/handwriting-stroke/` 的 `hwStrokeNibMinRatio`/`hwStrokeSpeedMinRatio` 两个真实字段） · `POST /api/enhance/battop/{start\|stop}`（未装 battop 时 400，不装） · `GET /api/enhance/battop/summary`（4 个时间窗×应用/进程/唤醒源 top15，原样转发 battop 自己聚合的 `summary.json`，未采样过返回 `{available:false}`） · `/login` `/logout` `/password` `/ca.crt` |

上传回执统一 `{ok, items:[{name, ok, message, item?}], …}`（`rmsvc_core::asset::receipt`）；成功项 `name` 是落地名。

## 目录

```
shelf/
├── Cargo.toml · build.sh · .cargo/    内部 workspace（仓库根仍无 workspace）；musl 全静态交叉编译
├── crates/bookconv/                   ★ 通用内容层：多格式→EPUB/PDF、EPUB 优化器+清洗层+质量门、e-ink 图片处理、EPUB 组装、网文抽取
│   └── src/bin/epub_optimize.rs         手动跑清洗+优化器的开发期小工具（原 host `wash_epub.sh` 末步用它，host 已砍，2026-09-18）
├── services/book-serve/               staging.rs(母版库领域：入库/优化/落库) · sidecar.rs(落库记录边车) · render_check.rs(投原生后渲染自检) · pending_queue.rs(PendingQueue\<T\>：持久化+入队去重+剔除共用骨架，2026-09-09 §03ag) · trash.rs(原生回收站队列) · mkdir.rs(原生建文件夹队列，2026-09-15 当死代码删过、2026-09-19 因为「加入 xochitl → 文件夹」有了真消费方复活) · spool.rs(inbox 队列) · api.rs(纯 HTTP 适配) · service_state.rs
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
├── xovi/                              font-menu-dynamic{,-3.27}.qmd 字体菜单读 fonts.json 动态追加（3.28 / 3.27 真机通）· shelf-trash-agent.qmd 原生回收站代理（Sidebar 注入，拉 book-serve /trash/pending）· shelf-mkdir-agent.qmd 原生建文件夹代理（MainView 注入，拉 book-serve /mkdir/pending，调 `Library.createCollection`；2026-09-15 因无消费方删过、2026-09-19 复活并真机验证，见白皮书对应记录）；改 qmd 先用 qmldiff CLI 离线实跑（白皮书 §04）
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
| 配置 | `~/.config/shelf/<service>.json`（book：书库文件夹/xochitl 主机/超时/**`nativeUploadLimitMb` 加入原生体积门 90**，2026-09-19 真机精确测出 xochitl `/upload` 硬上限后从未验证过的 150 改成留够安全余量的 90，见 `config.rs`；font；gateway）· `~/.config/shelf/tls/`（CA+叶证书） |
| 数据 | `~/.local/share/shelf/`（fonts.json、壁纸池）· `~/.local/share/fonts/`（用户字体，fontconfig 标准位） |
| 状态 | `~/.local/state/shelf/books/staging/`（**母版库**，不淘汰）· `books/{inbox,.work,failed}`（追平队列）· `wallpaper-state.json` · `koreader-backups/` |
| 运行时 | `/tmp/shelf-0/shelf/{services,upload,koreader}`（`XDG_RUNTIME_DIR` 缺省回落；重启即清） |
| 笔记线 | 自成一套 `notes` XDG 路径，与书架的 `shelf` 命名空间互不相干，详见 `../notes/README.md` |
| 外部约定 | KOReader 根 `SHELF_KOREADER_ROOT`（缺省 `~/xovi/exthome/appload/koreader`；appload 0.5.3 经 PR #59 qmd 回填补丁在 3.28 复活，见 koreader/README）；xochitl 书库 `~/.local/share/remarkable/xochitl` |

## 访问与密码

- 地址：`https://<设备IP>/`，或伪域名 **`https://shelf.local/`**（网关自带 mDNS 应答；iOS/macOS/Windows/Linux 直接可用，
  **安卓系统不解析 .local**——安卓手机走 host 热点时在 host 加 dnsmasq 别名，见白皮书 §03j）。**传大书走 USB `https://10.11.99.1`**（不占 WiFi，白皮书 §03l）。2026-09-10 起绑标准 443 端口，网址不用带端口号。
- 登录页只要密码、无用户名：**首次默认 `shelf`，登录后强制改**（≥6 位、不能是默认）。改密：网页右上「改密码」；
  设备上 `gateway passwd <新密码>`；忘记：`gateway reset-password`（回默认并再次强制改）。改密后其它设备会话失效。
  （host `shelf passwd` 已随 host CLI 一并砍掉，2026-09-18）
- 证书：私有 CA 签发（`~/.config/shelf/tls/ca.pem`）。登录页「下载 CA 证书」装进手机/电脑信任库**一次**，此后不再有"不安全"提示
  （叶证书 800 天自动续签、CA 不变）；不装就点「高级 → 继续访问」。
- 只有网关对外；领域服务只绑 127.0.0.1，无需认证。

## 构建 · 部署 · 卸载

**前置依赖**（一次性）：`rustup target add aarch64-unknown-linux-musl` + 装 aarch64 交叉 gcc/ar（Arch：`pacman -S aarch64-linux-gnu-gcc`；只用来编 `ring` 的 C 部分，产物本身是 musl 全静态、跟设备 libc 版本无关）。链接器/CC/AR 配置在 `.cargo/config.toml`，不用手改。改代码前先看工程纪律（真机验证、分支策略、离线门槛等）——两条线（shelf/notes）都遵守同一份；2026-09-16 起 `dev` 分支已删除，日常开发直接在 `master` 上开 feature 分支。

```sh
cd shelf && sh build.sh                                # host 测试 + aarch64 musl 全静态（书架 2 个二进制；../gateway/../enhance/{wallpaper,font}-serve/../notes 存在时顺带编它们）
cd ../packaging && sh deploy.sh 10.11.99.1             # 组载荷 → 设备 /home/root/shelf-pkg → install.sh（先备份旧二进制/单元）；设备只在 WiFi 上时给 WiFi IP
sh deploy.sh 10.11.99.1 --only font,wallpaper          # 只装/更新部分服务；SHELF_NO_BUILD=1 跳过编译
ssh root@10.11.99.1 sh /home/root/shelf-pkg/shelf/uninstall.sh [--only font] [--purge]
cargo build --release -p bookconv --bin epub-optimize   # 手动跑一遍清洗+优化器的开发期小工具（shelf/target/release/），不再有任何 CLI 调用它
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

## host CLI（2026-09-18 已砍）

`shelf` host CLI（`push`/`font`/`wallpaper`/`koreader`/`notes pull`/`inbox`/`events`/
`doctor --render`/`passwd` 等全部子命令）随 `shelf/host/` 整个目录一起砍掉——用户明确表态
以后不再使用 PC 端。这些能力目前**没有网页等价物**，是真实的功能减法，不是"挪到别处"：

- **`shelf push` 的格式转换/深洗能力整个消失**：Calibre 深洗、非漫画扫描 PDF 的 k2pdfopt 结构化
  重排、漫画→CBZ 转换（含跨页拆分/白边裁切/16 灰省刷新档/够小顺带出投原生 PDF）、AZW3/MOBI/…
  杂格式转换——这些都只发生在 host 侧，网页从来没有对应功能。以后只能自己在电脑上处理好
  EPUB/PDF/CBZ 等母版库直收的格式再上传。
- **`shelf doctor --render` 真机排版回归探针**（OTA 后拿探针书测顶格/首行缩进的 PASS/FAIL 工具）
  一并消失，没有替代——以后排版回归只能人工肉眼核对。
- **`shelf notes pull`**（笔记线 md 导出拉到本机 Obsidian vault）一并消失，`notes/README.md`
  相应部分需要单独确认现状（这是笔记线的历史功能，不是书架线自己的）。
- `shelf font/wallpaper/koreader` 几个子命令本来就是网页对应功能的另一个入口（网页从没依赖过
  它们），删除无功能损失。
- `shelf events`（订阅 SSE 打印）是纯调试便利，网页本身就是 SSE 的正主消费者，删除无功能损失。

源码留档在本机 `/home/afu/Projects/oldbak/cang-jie/shelf-host/`，不随仓库走、不再维护；完整的
旧命令参考见这份 README 在 git 历史里 2026-09-18 之前的版本。

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
| 漫画通道 | EPUB 漫画自动识别 → CBZ 给 KOReader（2026-09-17 起 AZW3/MOBI 判定连带退役，见 §03av）；**漫画不投原生**（曾做过 CBZ→PDF **分卷**投原生，用户否决后删——2026-09-08 §03ad 部分修订：小体积不分卷可选投原生，跟这次被否决的"分卷了也投原生"是两回事）；镖人 282MB EPUB 撞 xochitl 上传上限根因；分卷静默失效修 | ✅ 真机通（§03t） |
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
| EPUB 线四原则 + 格式收窄 | 拆 EPUB 线/PDF 线（PDF 线原则待定）；TOC 标题+编号拆两级/无标题兜底、只解锁字号保留原书颜色/加粗、脚注最终定案 `Anchor`（当天 `Inline`→`ParagraphEnd`→`Anchor` 两次真机反馈驱动的切换，§03aw/§03av 完整记录）、漫画自动识别（`comic_detect` 移植进 Rust）不压画质+裁边；AZW3/MOBI/AZW/PRC/FB2/TXT host 转换（含 AZW3/MOBI 漫画 PalmDB 直判）**用户明确要求连带退役**，母版库格式白名单从三档收窄成两档 | ✅ 真机通（§03av/§03aw/§03ax，2026-09-17/18）：TOC 拆分/颜色保留/脚注段落位置/漫画裁边+保画质/格式拒收全部真机 API 级验证过；`trim_margins` 真机性能问题+异步优化+防双击已修复（§03aw）；脚注最终改回 `Anchor`（§03aw），"跳转回不去"经用户指出其实已有原生返回浮标兜底、不是真限制（§03ax 更正） |
| 超限漫画按卷拆分投原生 | EPUB 漫画超过原生上传上限时，按自带 `toc.ncx` 结构递归拆分成若干份分别投递（新增 `bookconv::comic_split`），不再"大部头一律只出 CBZ"；复用原「投入原生书库」按钮，不新增入口；只做 EPUB 格式（CBZ 走 host 管线不碰） | ✅ 真机通（§03ax，2026-09-18）：《火影忍者》281MB 7 卷合集拆成 8 份（每份 38-40MB）全部真实上传成功，设备端 `.metadata` 确认 xochitl 已自动渲染打开（非"传上去但读不了"） |
| host 部分整体砍除 | 用户明确表态以后不再使用 PC 端：`shelf/host/`（Python CLI `shelf push`/`font`/`wallpaper`/`koreader`/`notes pull`/`inbox`/`events`/`doctor --render`/`passwd` + Calibre 转换管线）整个移出仓库（留档 `oldbak/`，不再维护）；`notes/host/` 空目录一并删除；`pyproject.toml` 删 `calibre` 依赖组；CI 删对应 pytest 步骤；网页「传书·入库」页原第三张"电脑 shelf push"卡片删除，入库只剩「上传」+「抓网文」两条路；`transfer.push.*`/`transfer.guide.push.*` 共 20 个 i18n key 一并删除 | ✅ 离线全绿：`cargo test`（gateway 17 + shelf workspace 183）全过、`node --check`、locale key 对称差为空、`uv sync`+剩余 `pytest` 全过；**这是真实的功能减法不是搬家**——Calibre 深洗/PDF k2pdfopt 重排/漫画转 CBZ/`doctor --render` 排版回归探针/`notes pull` 拉 Obsidian vault 均无网页等价物，随这次砍除一起消失，不是"以后要补" |
| 格式两档收成一档：只收 EPUB/PDF | 同一天用户接着明确要求"从此开始入库只入 PDF 和 EPUB，不论格式是否支持"——`rmsvc_core::formats` 的「仅 KOReader」档（CBZ/CBR/DjVu/HTML/HTM/RTF/DOC/DOCX/CHM/XPS）整档砍掉，`KOREADER_ONLY_EXTS` 常量删除，`BOOK_EXTS` 现在就是 `NATIVE_EXTS`（`["epub","pdf"]`）；网页格式说明文案（`transfer.guide.format.dd`/`transfer.upload.hint`/`transfer.upload.dropLabel`）同步简化成单档描述，`transfer.fmtTiers` key 删除；已经在库里的旧格式条目不受影响（仍可加入 KOReader），只是不会再有新的 | ✅ 离线全绿：`rmsvc-core`（53）/`gateway`（17，含格式白名单注入测试）/`shelf` workspace（183，两处上传/inbox 单测夹具改用 epub 重新验证空文件+非书籍格式+重名三条路径）/`notes`/`enhance` 三个 crate 全过、`node --check`、locale key 对称差为空 |
| 母版库页三项修复：真实文件夹下拉/用词统一/去掉优化分档与投完自动删除 | 用户反馈三点：① 「加入原生书库」文件夹下拉原来是写死的「书库/批注/自定义」预设（`annot_folder` 默认跟 `library_folder` 撞成同一个值），不反映真实文件夹——新增 `xochitl::list_folders` 扫真实 `CollectionType` 条目，`GET /status` 新增 `xochitlFolders`，网页改跟 KOReader 目录同一套自由输入框+datalist 真实候选，`annot_folder`/`folderPreset` 删除；② "投入原生书库"跟"加入 KOReader"两个按钮动词不一致，统一改「加入」，23 个 i18n key（含英文版）一并改；③ 去掉优化分档位（`OptimizeMode` 枚举+`mode` 参数删除，永远跑完整清洗+优化）和投完自动删除（`keep` 参数删除，母版永远保留，`stgclear` 复选框删除）。顺手修了三处指引用户"用电脑 shelf push"的过期文案（host 已砍） | ✅ 真机通：`GET /status` 的 `xochitlFolders` 对照设备实际 `.metadata` 核实为空（设备确无自建文件夹，坐实旧"批注"选项是假的）；真实上传 EPUB→`POST /staging/optimize`（不带 mode）→确认 `level=full`→清理测试产物；`deliver`/`keep` 移除由 26 个 book-serve 单测覆盖，未做真实投递到原生阅读器的端到端验证（会在用户设备书库留下真实文档，这次跳过，upload 路径字节本身不变）。`cargo test` 全绿：rmsvc-core 54、gateway 17、shelf workspace 183 |
| 落库进度条不刷新 + 文件夹填名不创建 | 同一天两条追加反馈：① 漫画拆分卷落库的进度条数字冻结不动，要手动刷新——根因 `try_deliver_split` 每完成一份写 sidecar 进度后没有 `bus.publish`，`deliver`/`try_deliver_split` 签名补 `bus` 参数，写完立即推事件，触发网页零轮询刷新链路；② 「加入 xochitl → 文件夹」填一个不存在的名字依然不会建文件夹——反编译 xochitl 真机二进制坐实：本地网页上传接口只有 `/documents/`/`/upload`/`/download`/`/thumbnail`，没有任何建文件夹的 HTTP 接口，唯一合法路径是 xochitl 自己的 QML 代码 `Library.createCollection(parentId, name)`；`git show` 从 2026-09-15 的删除提交里原样捞回 `mkdir.rs`（`MkdirQueue`）+ `shelf-mkdir-agent.qmd`（2026-09-07 为 note-serve 写的、9-09 没了消费方、9-15 当死代码删除——这次因为有了真消费方复活，qmd 内容一个字没改）；`Staging::deliver` 新增 `ensure_folder`：文件夹不存在就入队，`fswatch::watch_until` 同步等代理真建出来（最长 20 秒，等不到就原样走以前"找不到落书库根"的兜底，不是新错误） | ✅ 真机通：① 部署后订阅 `GET /events` 确认 SSE 流本身正常，漫画拆分卷的逐份推送这次没有用真实超限漫画重新端到端验证（结构性改动，风险很小，如实说明）；② **完整真实闭环**——`POST /staging/deliver` 一本真实测试 EPUB 到一个全新文件夹名，8 秒内状态从 pending 变 ok，真机 SSH 核对 `.metadata` 坐实文件夹真的建出来了（`type:"CollectionType"`）且书的 `parent` 字段精确指向这个新文件夹 uuid——这是当年反编译从没做过的最后一步交叉验证，这次真机坐实；顺手发现真机上这份 qmd 从 2026-09-11 起从未被摘除、一直在后台安静轮询了一周多（`journalctl` 确认 `qmldiff` 反复成功加载它、零报错），侧面印证这条注入路径本身早已稳定。测试文档/文件夹已排入原生回收站队列（`shelf-trash-agent.qmd` 需书库视图下次有动静才会真的清掉，这次没有额外触发）。`cargo test` 全绿：shelf workspace 172（book-serve 28 + bookconv 143 + koreader-serve 14） |
| 「加入 KOReader」进度条不刷新 + 《乱马1/2》文件夹名带斜杠建不出来 | 同一天再两条追加反馈（跟上一行不是同一批）：① 「加入 KOReader」点了没有进度条——根因是这个操作走 koreader-serve 独立进程的一次同步阻塞调用，压根没接入 book-serve 的忙态系统，网页新增纯前端 `localBusy` 本地忙态集合（点击时标记+立即重画进度条，操作结束即清），跟服务端 `it.busy` 合并成统一判据；② 上一行刚修完的建文件夹机制，用户报告文件夹名 `乱马1/2` 依然建不出来——`MkdirQueue::add()` 有一条"文件夹名不能带路径分隔符"校验把含 `/` 的名字直接拒绝，`ensure_folder` 对这个 `Err` 静默放弃、退回落书库根，表现跟"没建文件夹"一样；这条校验本身没有技术依据（`name` 全程只当 JSON `visibleName` 字符串走，从不落到文件系统路径），整条删除 | ✅ 真机通：① `node --check`+gateway 17 测试全过，部署后未做浏览器人眼确认（无浏览器环境）；② **两轮真实端到端复现**——先排除"机制本身有 bug"（`/mkdir/add` 手测+真实 `/staging/deliver` 走生产默认值 `library` 均在 7-10 秒内成功，坐实默认文件夹设计本来就该建出一个真的叫 `library` 的文件夹），再用用户报告的确切文件夹名 `乱马1/2` 复现出稳定失败、修复后同名字重新验证成功（`journalctl` 确认 `SHELF-MKDIR: created 乱马1/2`，`.metadata` `parent` 精确指向新文件夹），新增独立回归测试 `add_accepts_names_with_slash`。测试文档/文件夹均已排入原生回收站队列。`cargo test --workspace` 143 通过 |
| 优化操作补真实分步进度 + 确认不能靠它省内存 | 用户追问"是不是只有漫画才走流式处理，一般文字书不流式，所以进度条感觉不动"——premise 错（`Staging::optimize()` 无条件对所有 EPUB 走流式，漫画判定只影响图片画质档，不影响要不要流式），结论对（`OptimizeCheck` 压根没有 `progress` 字段，不管流不流式/是不是漫画都没法报分步进度；唯一有真实进度的"超限漫画按卷拆分"是投递阶段的机制，跟优化阶段无关）。用户确认要做后追问"能不能实质省内存"——查证不能：内存瓶颈（图片）已经是逐张处理，加进度回调不改变任何数据的内存存活时长，跟内存优化正交。实现：`DeliverProgress` 改名 `StepProgress` 给 `OptimizeCheck`/`DeliverCheck` 共用；`optimize_epub_file_streaming` 新增 `on_progress(done,total)` 回调，阶段二逐条目写出时触发；`spawn_optimize` 节流每 5 条目落一次盘/推一次 SSE（大漫画优化条目数可能上百，每条目都写原子文件是真实开销）；网页进度条渲染逻辑从只认 `dc.progress` 扩成 `dc.progress`/`oc.progress` 都认 | ✅ 真机通：拿用户原始《飘·上册》EPUB（10.6MB/35章）真实走 `/staging/optimize`，轮询 `GET /staging` 观察到进度 `36/56→41/56→46/56` 逐步推进到 `status:"ok"`，不是代码看着对，是真拿到中间态数字。`cargo test --workspace` 全绿（新增 `bookconv` 流式进度断言 done 严格递增+最后一次 done==total、`book-serve` optimize() 转发校验） |
| OOM 排查修复 + 代码质量去重（含前端） | 用户要求核查最有可能 OOM 的服务、确认拆不拆服务，并按设计模式合理去重（含 UI）。三路审计坐实：`book-serve` 是唯一有实质 OOM 历史/现存风险的服务，`koreader-serve`/`comic_split` 拆分投递/`/staging/upload` 落盘均已确认安全；不拆服务（函数级内存问题，拆服务不解决根因）；唯一未修的真实风险是 `Staging::deliver()` 落库不拆分路径（≤90MB 书峰值可叠到 ~180-270MB：整本读+自检整本解压+上传内部再克隆一份 body）。修复：`rmsvc_core::xochitl` 新增流式 `send_multipart`/`upload_file`（`ureq::Request::send(Read)`+显式 `Content-Length`，线上字节不变、不再整块囤内存）；`bookconv::stats` 新增流式 `text_profile_file`（跳过图片条目解压）。顺带去重：`staging.rs` 新增 `spawn_bg`/`busy_err` 合并 `spawn_optimize`/`spawn_deliver` 的起线程模板与三处忙锁提示；前端新增 `el()`/`renderStepProgress()`，`stagingList` 的 `btn()` 改委托全局 `guardClick`，进度组件复用到笔记「推送本章」 | ✅ 真机通（后端）：host 合成 80MB 测试 EPUB 真实走 `/staging/deliver`，投递成功（`render` 自检数字吻合），`/proc/<pid>/status` 的 `VmHWM`（进程生涯内存峰值）投递前后全程 3.2-3.5KB 量级，测试产物已清理；`note-serve` 共用改动重启健康、无真实笔记本数据做完整推送复测（如实记录缺口）。`cargo test --workspace` 全绿（rmsvc-core 新增 multipart 差分测试、bookconv 新增 text_profile 差分测试）。前端：`node --check`+逐处静态审读通过、`gateway` 17 测试全绿；用户随后给了网页登录密码，补做认证态 `curl` 全链路验证——真实 `POST /login` 拿 `shelf_session` cookie 后确认首页里嵌了新代码，再原样走一遍按钮背后的真实请求序列（入库→优化→落库→加入 KOReader→删除，全部走认证后的网关代理而非直连 book-serve），响应 JSON 形状跟 `app.js` 读取的字段一一对上；仍缺浏览器里的像素/交互观感人眼确认（本机无浏览器自动化工具），如实记录这层残留缺口。测试产物已清理，四服务重启健康 |
| imgopt 单图解码像素上限 | 用户自己点完网页确认观感没问题后要求"按日志核查是否符合开发要求"。翻真机 journal+`.metadata` 时间戳（不是听转述）坐实：用户真实投递一套《乱马1/2》8 卷漫画，`book-serve VmHWM` 冲到 271MB——正是上一行 OOM 排查顺带记录"理论边缘风险、没有真实样本、这轮不处理"的 `imgopt` 单张图片解码无像素上限，几小时内就等到真实样本。根因排查坐实是优化阶段 `trim_margins`/`downscale_for_epub_comic` 解码异常高分辨率扫描页；`comic_split` 拆分路径本身确认干净（只读原始压缩字节，不解码）。新增 `MAX_DECODE_PIXELS`（2500 万像素，按"3 字节/像素"理论估算定）+ `within_decode_budget` guard，三处解码入口统一拦截，超限图原样保留。**几十分钟后更正**：用户又优化上传一本《火影忍者》17~21卷，真机再撞 `VmHWM` 262MB——理论估算严重偏低（真机实测不同像素数下的真实开销约 9-16MB/百万像素，是理论估算的 3-5 倍），改用实测数据重新定阈值到 900 万像素 | ✅ 真机复现两轮：第一轮（2500 万像素阈值）host 合成 25 页测试漫画（一页故意 6500×8000/5200 万像素），`VmHWM` 从潜在几百 MB 压到个位数 MB；第二轮更正后重新合成测试漫画（一页 4000×4000/1600 万像素，旧阈值下会被处理），真机验证新阈值（900 万像素）下 `VmHWM` 仍稳定在个位数 MB，没有重现尖峰。两轮产物均核对超限页维度原封不动、其余页正常处理，无误伤。新增/更新回归测试，`cargo test --workspace` 全绿（bookconv 147）。测试产物已清理，`book-serve` 两轮重新部署均健康 |
