# reMarkable 书架（shelf）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。计划全文见 未入库的计划文件。
> **书籍优化引擎（`bookconv`）的深度细节**（清洗层 / 优化遍 / 脚注 / 图片 / 格式转换 / **★xochitl 渲染硬规则** / 版本演进）**已独立成 `bookconv优化白皮书.md`**；本文只记书架侧的决策/UI/真机轮次。

## 00｜定位与原则

2026-09-03 用户提出"全面重构，补齐短板增强优势"：① 阅读系统增加 KOReader 与微信读书；② 全面支持 AZW3/PDF/EPUB；
③ 字体与屏保图片上传即可用。澄清：先「有书读」（原生 / KOReader / 微读三条投递线）再「高质量读」；上传时手选目标；
设备网页 + host CLI 共用同一 API；微读 = 设备上直接开网页版在线读（门控）。

四条硬原则（用户两轮驳回后定）：
1. **XDG 基目录规范**（Rust `shelf_core::paths` / shell / Python 三处同一张表，env 可注入测试）。
2. **设计模式去重解耦**：Repository + Template Method（资产上传 `AssetStore`/`AssetUploadFlow`，font/wallpaper/koreader/**母版库四家共用**，拒收/成功文案由仓库定）· 领域模块 + 纯适配层（book-serve `staging.rs` 领域 / `api.rs` 只取参回执）· 服务启动模板（`service::ServiceSpec`+`run`）· 配置读写模板（`config::load_or_default/seed/save`）· 端口/适配器（HTTP 只在 `http.rs`；`bind`/`Router::any`/`JsonBody` 取参门面）· Registry（服务发现）· Facade（网关代理，`manage::MODULES` 单一目录表）· Command（CLI）· 单一事实源（`formats` 格式白名单、`fs::plain_name`/`unique_path`）。共享原语：`fs::write_atomic`、`multipart::receive_part_to`、`asset::receipt/all_ok`（Rust）与 `receipts.upload_each/print_receipts`、`transport.delete_named`（host）。移动优先于复制；**旧 crate 只许剥离+re-export，不直接引用**。〔早期的 Strategy（投递目标）/ Pipeline（处理链）随直投路于 §03s 删除——规则统一后没有调用方，模式要适配需求而非反之。〕
3. **专项专用可插拔**：不做单体，按领域拆服务。
4. **不引用旧项目 crate、不对接旧路径**：能力只许剥离移植；不读写 `/home/root/weread/**`；旧 wr-serve 微读线原样兜底。

## 00b｜现状总览（2026-09-05，读本文其余历史节前先看这里）

**架构**：网关（`0.0.0.0:8778`，HTTPS 私有 CA + 登录页密码 / CLI Basic + mDNS `shelf.local`）+ 四个 loopback 领域服务（book 8790 / koreader 8791 / font 8792 / wallpaper 8793）+ 运行时注册表驱动 tab。设备固件 **3.28.0.172**（2026-09-05 从 3.27.3.0 升级，实录 §03v；appload 0.5.3 在 3.28 不兼容已停用，KOReader 暂无侧栏入口），KOReader v2026.07.1。

**读书线 = 三层 · 三动作正交**（§03r 定，§03s 收口）：内容源（网页上传 / 抓网文 / host `shelf push` / scp inbox；微读线已砍，§03u）→ **母版库** `~/.local/state/shelf/books/staging/`（原样入库，永久保留，不淘汰）→ 落库（人选：投 xochitl 只收 EPUB/PDF；加入 KOReader 收任意入库格式）。「优化」是母版库里对 EPUB 的独立动作（档位 auto / keep-spacing / plain，产物标记 full / core / old）；落库＝纯复制母版字节（投原生有体积门 `nativeUploadLimitMb`，缺省 150）。**漫画不投原生**：AZW3/EPUB 漫画由 host `shelf push` 转 CBZ 入库，只加入 KOReader。**所有书只落母版库，没有任何直投读器的路径**。落库记录 sidecar `.<书>.delivered`。

**代码落点**：book-serve `staging.rs`（领域）/ `spool.rs`（inbox 队列）/ `api.rs`（纯适配）；koreader-serve 只做"从母版库 adopt"+字体/词典/配置同步（不依赖 bookconv）；`shelf_core::formats` 是格式白名单单一事实源（三档：原生 epub/pdf · 电脑可转 azw3/mobi/azw/prc/fb2 · 仅 KOReader 其余 11 个），网页 `ui::page()` 注入；`shelf_core::asset` 是所有上传口的模板（母版库暂存在 spool `.work/` 同分区 rename）。host CLI：`push.plan()` 三路 raw / comic（→CBZ）/ wash；`comic.py` 漫画探针；bookconv CLI `epub-optimize`（与设备同一函数）。

**已删（别再找）**：漫画 CBZ→PDF 投原生整条（`cbz2pdf` bin、`POST /staging/to-pdf`、`push --mono`，§03t 末）；book-serve `POST /?target=native|annot` 直投路与 `target.rs`/`pipeline.rs`（Strategy/Pipeline）、`/targets`、`done/` LRU；koreader-serve 直传 `POST /books` 与 `optimizeEpub`；网页读器页的传书区与 KOReader 书库浏览；host `push -t/--direct/--quality`、config `default_target/quality`；`BookConfig.optimizeDirectEpub/comicMono`。

**网页 tab**：传书（入库｜母版库，固定第一）· xochitl（原生字体，由 font-serve 注册）· KOReader（字体｜词典）· 壁纸 · 管理（固定）。

**设备杂项（§03v/§03w，全真机通）**：3.28 字体菜单 qmd 已通（qmldiff 语法坑，§04）；原生休眠屏 `SleepScreenPath=current.png` 满屏且随轮换（bind-mount 整套待退役）；WiFi 60 秒掉链＝IW612 省电，`powersave 2` 已根治 + `xovi/scripts/post-start/wifi-reconnect.sh` 看护；chrony 国内 NTP `packaging/chrony-cn.sh`；OTA 后四步恢复见 §03v（README 有"OTA 与恢复"表）。

**未闭环**：Phase E ②③④（英文书拉丁缩进观感 / 两器同字节对照 / KOReader 里内联脚注可否接受——KOReader 拿到的是母版库 Inline 产物，不再另跑 Anchor）；退役 bind-mount 壁纸整套改写 `SleepScreenPath`；appload 3.28 适配（KOReader 入口）。§05 有清单。

## 01｜架构决策

**三种拆法**：A 每服务独立对外端口（暴露面多、UI/CLI 要记端口）；**B 网关 + loopback 服务 + 注册表（采纳）**；C 单二进制 feature 编译期插拔（不能运行时拔）。
- 注册表放 `$XDG_RUNTIME_DIR`（重启即清）+ 读时按 `/proc/<pid>` 清陈旧条目：SIGTERM 杀进程 Drop 不跑也不会误报。
- 代理**剥掉服务段**（`/api/fonts/x` → 后端 `/x`）：后端直连与经网关同一套路由，`/health` 也能穿过网关。
- 每请求一线程：大文件上传不阻塞其它请求；body 流式透传（`ureq::send(reader)` + 透传 Content-Length）。
- 内存：musl 静态服务各约 0.56MB 二进制（网关 1.6MB 含 ureq/TLS）；空闲 RSS 数 MB，五个可接受。

**内容层抽离 `bookconv`**（移动，不复制）：从 weread-device `git mv` `convert/{mod,common,cbz,fb2,mobi,kf8,palm,pdfwrite}`、`optimize`、`imgopt`、`htmlproc`、`epub`、`util`；`ingest.rs`（weread/inbox 语义）留下。唯一反常边 `optimize → readlater::fetch_image` 切开：`fetch_image` + `UA` 移成 `bookconv::netimg`，readlater 改为引用；`http_agent` 在 netimg 独立实现（bookconv 不依赖 device-core）。weread-device `lib.rs` 以 `pub use bookconv::{…}` + 内联 `mod convert { pub use bookconv::convert::*; pub mod ingest; }` 保全部旧路径；顺手移除其不再用的 `zip/image/miniz_oxide/fb2` 依赖。**验收**：三个旧 crate `cargo test` 全绿（内容层 81 个测试随模块迁移）；新旧 `epub-optimize` 对同一合成 EPUB（脚注/跨章链接/双 id/2400px 图/字体锁）输出 **md5 完全一致**。

**跨 workspace 路径依赖可行**：weread-device（无 workspace）path 依赖 `shelf/crates/bookconv`（在 shelf workspace 内）编译无碍，各自 target。

## 02｜XDG 路径表

见 `shelf/README.md`「路径」。qmd 的 XHR 只能写绝对路径，写的是缺省值展开 `/home/root/.local/share/shelf/fonts.json`。

## 03｜systemd

`shelf.target`（WantedBy=multi-user）；服务 `PartOf=shelf.target` + `WantedBy=shelf.target`；网关 `ExecStartPre=-…/cangjie-lo-alias.sh`（同一份脚本，让 10.11.99.1 常驻可达以便 `/upload` 注入）与 `ExecStartPre=-/usr/bin/fc-cache`（tmpfs 索引真重启后丢）。`install.sh` 写 /usr 前实检 dm-verity。**不给 xochitl 加任何依赖**。

## 03b｜Phase 1 统一投递（2026-09-03，离线完成）

**book-serve**（loopback 8790）：（⚠ 本段直投路 `POST /?target=` 与 `Native`/`Annot` Strategy 已于 2026-09-05 删除，见 §03s；现行唯一入口是母版库 `/staging*`，§03r）`POST /?target=native|annot&folder=&optimize=auto|off` multipart 多文件流式落 `.work` → 目标 Strategy（`Native`=Precheck→Convert→Optimize→Inject；`Annot`=Precheck→Convert(cbz)→Inject，只收 PDF/CBZ、EPUB 回执"去 host 定稿"）→ done/failed 归档；`GET /status|/targets|/inbox`、`POST /inbox/retry|/inbox/delete`。自有 spool `$XDG_STATE_HOME/shelf/books/`（inbox 供 scp 追平：启动扫一遍 + inotify 8s 防抖；`.work` 崩溃残留启动时移回）。配置 `~/.config/shelf/book.json` 首启写出缺省。
**koreader-serve**（8791）：（⚠ 直传 `POST /books` 已于 §03s 删除，现只从母版库 `POST /books/adopt`）`POST /books?folder=` 原字节落 `books/[folder]/`（先 `.part` 再 rename，KOReader 扫目录不见半成品）；`GET /status`（installed/running(扫 /proc cmdline)/version(git-rev)/计数）、`GET /books`、`GET|POST /fonts`（字体镜像口，供 font-serve）、`GET /dicts`。
**网关 UI**：tab 按注册表；传书 tab 目标下拉三档→按档打 `/api/books` 或 `/api/koreader/books`；逐文件一请求 + 进度条 + 逐项回执；失败项重试/删除；字体/壁纸 tab 复用同一上传器（AssetUploadFlow 回执同形）。
**host CLI `shelf push`**：`decide_route(quality,target,ext,has_calibre)` 纯函数（单测矩阵）；Calibre 桥统一清 `VIRTUAL_ENV`/`.venv/bin`；`pdfsplit` >60MB 分卷（pymupdf 可选）。Calibre 八件套自 `reading/tools/calibre/` **整体 git mv** 到 `shelf/host/calibre/`；`epub-optimize` CLI 随之迁 `bookconv` bin（书架不引用旧项目）。
**本机冒烟（三服务，干净 env）**：注册/代理 ✓；KOReader 中文名+子目录落盘字节正确 ✓；native 在 xochitl 不可达时 `inject:` 失败入 failed、可重试/删除 ✓；annot 拒 EPUB ✓；未知目标 400 ✓；inbox 追平认领进 .work ✓。修正：xochitl 客户端加 10s 连接超时（整体 300s 只防大书误判）。
**真机待验**：见 §05 P1 清单（设备当前离线，未验）。

## 03c｜Phase 2 字体/壁纸上传即可用（2026-09-03，离线完成）

**font-serve**（8792）：`FontStore: AssetStore` —— 校验 TTF/OTF/TTC 魔数、拒与内建同名 → 落 `$XDG_DATA_HOME/fonts/` → `fc-cache -f` → 家族名 `fc-scan --format '%{family[0]}'`（无 fc-scan 时自解析 `name` 表 nameID16>1、平台3>1）→ 写 `~/.local/share/shelf/fonts.json`（version/fonts[{key,file,names{cn,tw,en},source}]，内建三项来自 `font.json` 配置、文件在才列）→ 镜像 KOReader（HTTP `koreader-serve /fonts`，失败只降级回执）。`DELETE` 只许 user。
**wallpaper-serve**（8793）：`WallpaperStore` —— `fit_to_screen`（cover/contain → 954×1696 RGBA PNG，屏常量引用 `bookconv::imgopt`）入池 `~/.local/share/shelf/wallpapers/pool/`；`activate` 用 `truncate(true)` 原地写 `current.png` **保 inode**（单测断言）；`roll` 按 `~/.local/state/shelf/wallpaper-state.json` 的 mode 轮换；`bind|unbind` 子命令 `mount --bind` 盖 `suspended.png` + 三张 carousel（透明 776 程序生成，按 `/proc/mounts` 幂等）；首张上传自动激活；删当前拒绝。sleep 钩子 5 行只转发子命令；`shelf-wallpaper-bind.service` 开机 bind。`install.sh --only wallpaper` 迁移旧 `/home/root/wallpaper/`（停旧单元/钩子、拷池图、`.migrated` 标记、旧目录不删）。
**字体菜单 qmd**：`shelf/xovi/font-menu-dynamic.qmd`（3.28 锚点）/ `-3.27.qmd`（3.27 锚点，整体重赋 model）：读 fonts.json 逐项 `append`（三语显示名）；缺文件回退内建三项；`onCompleted` + **`onVisibleChanged` 差量追加**（S-B 假设）。`shelf/install.sh --only font` 按 `/etc/version` 主次号选版本装进 qrr、把旧 `add-reading-fonts.qmd` 移到备份（避免重复追加）；不自动重启 xochitl。打包器中文化层不再带旧 qmd；fail-safe 隔离清单加入新 qmd。设置面板文案改为"切开关需重启 xochitl"（.169 事实）。
**本机冒烟**：真 TTF 上传→家族名 `LXGW WenKai`→fonts.json→KOReader fonts/ 镜像→删除 ✓；两图→954×1696 ✓；首张自动激活、设当前、模式、删当前拒 ✓；`roll` ✓；`bind` 在 host 报目标不存在（预期）✓。
**真机 spike 待做（决定 UI 文案 restartNeeded）**：S-A 传字体后不重启 `epub.setFontName(新家族名)` 渲染是否认；S-B `onVisibleChanged` 是否每次开菜单触发（journal `SHELF-FONT: visible`）；S-C `FontLoader` 备选。壁纸：上传→休眠即显示、唤醒再休眠轮换、真重启 bind 4/4。

## 03d｜Phase 3 KOReader 配置即代码（2026-09-03，离线完成）

**设计**：Lua 处理不在 host 造轮子——`shelf/koreader/merge.lua`（Lua 5.1，`include_str!` 进 koreader-serve）由 KOReader 自带 `luajit` 跑：标量覆盖、表递归、`"__DELETE__"` 删键；`--dry-run` 只出差异；写=先 `.tmp` 再 rename，头行保留 KOReader 惯例注释；输出 JSON `{changes:[{path,old,new}],written}`。**Template Method** `ConfigSync::apply`：ensure_stopped（`/proc` 扫 cmdline，运行中 409）→ backup（`~/.local/state/shelf/koreader-backups/<file>.bak.pre-shelf-<ts>`）→ stage（补丁落 `$XDG_RUNTIME_DIR/shelf/koreader/`）→ merge → verify（`dofile` 回读失败自动从备份还原）。端点 `GET|POST /config/{settings|defaults|gestures}?dry_run=1`（body=补丁 Lua 文本）、`POST /dicts?name=`（StarDict 多文件落 `data/dict/<name>/`）、`GET /dicts`；fonts/dicts 共用 `receive_into`。
**profile**：`settings.reader.patch.lua`（脚注四键、wf_level=1、full_refresh_count=16、avoid_flashing_ui、color_rendering=false、floating_punctuation、cre_font、footer{reclaim_height,progress_style_thin,battery=false,time=false}）、`defaults.custom.lua`（DTAP_ZONE 死区）、`gestures.patch.lua`（长按左上角退出）——**键名来自白皮书 §11.1b，具体值/结构标注"待 pull 核对"**（尤其 gestures 上下文键名、DTAP_ZONE 字段、footer 轮显项）。
**CLI**：`shelf koreader pull`（三份原文落 `$XDG_CACHE_HOME/shelf/snapshots/<ts>/`）· `diff`（dry-run 列 old→new）· `sync [--dry-run] [--fonts] [--dicts]`。
**验证**：merge.lua 本机 luajit：dry-run 不写、写入后回读、二次幂等零差异、删键/新表/嵌套 ✓；`ConfigSync` 单测（host luajit）✓；CLI 对假网关 ✓。
**真机待验**：pull 快照 md5 与设备一致；运行中 sync 被拒；退出后 diff→sync→回读→启 KOReader 逐项勾（脚注弹窗/滑动返回/无锯齿/死区/退出手势/footer）；二次 sync 零 diff；字体/词典同步可用。

## 03e｜Phase 4 原生高质量门 · Phase 5 微读 spike 脚本（2026-09-03）

**P4**：host 路（wash / 定稿 / 裁边）产物必过 `check_output.py`（PDF：宽高比/outline/内链/字体子集/屏上字号列宽；EPUB：DRM 硬拦/nav+ncx 命中率/双 id/锚点），不过不推，`--skip-check` 强推、`--quality device` 走设备兜底；设备路回执 = Pipeline 各步 notes 拼接（"已转换为 epub；已优化（N 章，a→b 字节）；已导入《…》"）。网关传书 tab 只读展示 `reading-qol.json` 四开关（点击翻页/快速黑白/清残影/字体增强，书架不写它，改去设置→系统增强）。可选的 `linkcheck`（扫 xochitl 渲染 PDF 命名链接数）未做，不阻塞。
**P5**：`shelf/weread-web/spike.sh`（recon 侦查写 findings.md → fetch 解包+ldd 缺库 → run 看门狗+停 xochitl 前台跑 → restore 拉起并核 NRestarts）。门控判据与回退见其 README。**需用户在设备旁执行**（涉及停 xochitl，本会话不代跑）。

## 03f｜真机首轮（2026-09-03，固件 3.27.3.0 build 20260612，全程 WiFi 10.42.0.224）

**部署**：`rm-ssh-over-wlan on` 后 WiFi SSH 通；`shelf/deploy.sh` 一次装齐五服务（dm-verity 未激活，单元写 /usr），`/api/services` 五项、UI 12KB 可达。
**P1 通**：KOReader 投递中文名+子目录，`find|hexdump` 字节与 md5 一致（⚠ busybox `ls`/`grep` 看中文名会显示 `?`，别信）；native 投递 4s："已优化（3 章，27734→5077 字节）；已导入"，书库根出现《书架真机测试》（xochitl 用 EPUB `dc:title` 作 visibleName，不是上传文件名；`library` 文件夹不存在按设计回落根）；annot 拒 EPUB；`uploadReachable=true`。
**P2 通**：11MB 真 TTF 上传 20s，fc-list 认出家族名 `LXGW Neo XiHei Screen Full`，fonts.json + KOReader `fonts/` 镜像到位；两图缩成 954×1696，首张自动激活、bind 4/4，`roll` 保 inode(2184) 且 `/usr/share/remarkable/suspended.png` 真身 md5 随之变，unbind 还原原生/bind 回来，sleep 钩子 before/after 模拟通。**字体菜单**：新 qmd 加载后 `SHELF-FONT327: onCompleted model=8 rows=4`（4 原生 + 内建 3 + 上传 1）——"上传即出现在菜单"成立；**S-A 通**：用户在菜单选「LXGW Neo XiHei Screen Full」→ `.content` fontName 变 → scp 回 `<uuid>.pdf` `pdffonts` 嵌入 `LXGWNeoXiHeiScreenFull CID TrueType`——**上传→菜单→渲染全程免重启 xochitl**（只 fc-cache）。`restartNeeded=false` 定案。**S-B 通**：再传第二个字体后重开菜单，`SHELF-FONT327: visible model=9 rows=5`——`onVisibleChanged` 差量追加在 3.27 成立，菜单每进程只建一次的限制被绕开。字体线"上传即可用"在 3.27 完整闭环（3.28 版 qmd 同机制，待 3.28 机验证）。
**P3 通**：`pull` 拉到真实三份配置；按快照校正 profile 后 `diff` **零差异**——配置即代码闭环幂等成立。
**真机纠出 4 个 bug（已修）**：① `/health` 被 `GET /{name}` 通配抢先（`service::run` 改前置注册 + `Router::merge`）；② 安装器按 `/etc/version` 判固件版本，那是 build 号，选成 3.28 锚点——改读 `/usr/lib/os-release` `IMG_VERSION`（`update.conf` 不存在）；③ 旧 `add-reading-fonts-3.27.qmd` 与新 qmd 并存重复追加，安装器只清 3.28 名——已移备份；④ profile 三处臆测值（`floating_punctuation=1`→`true`、DTAP_ZONE 数值、`cre_font` 键不存在）按快照修正。
**⚠ 事故与恢复**：为验字体菜单跑 `systemctl restart xochitl`，新进程 maps 里 xovi.so/qrr/appload 全 0——这台重置机没装 `xovi-reenable.service`，xovi 配置在 `/etc` tmpfs，restart 即丢（KOReader 入口一起没）。恢复=`/home/root/xovi/start`（自带 restart），15s 后 xovi 4 段/qrr 5 段/appload 4 段、NRestarts 不增。安装器提示已改为指向 `xovi/start`。教训：重启 xochitl 前先核 `grep -c xovi.so /proc/<pid>/maps` 与持久化方式。**用户决定（2026-09-03）：暂不装 `xovi-reenable.service`**——真重启后需手动 `/home/root/xovi/start`（书架五服务本身在 rootfs 单元里、开机自起不受影响；受影响的是字体菜单 qmd / KOReader 入口 / 中文化）。
**壁纸休眠钩子**：首次真休眠后 `current` 未变——池里只剩一张（另一张疑被网页删除），且 systemd-sleep 不记录同步钩子执行、无法判断跑没跑；钩子改为 `logger -t shelf-wallpaper` 打点（`journalctl -t shelf-wallpaper`），补回第二张后等第二次休眠。**注册表 bug**：调试误起 `--bind 127.0.0.1:1` 第二实例把活服务的注册顶掉；`register()` 改为同名活 pid 拒绝覆盖。
**壁纸轮换根因（用户反馈"没有轮换"）**：14:50–14:53 按了 7 次电源键，journal 只有 xochitl `Changing display state from Normal to DeepSleep / DeepSleep to Normal`，**零条 `PM: suspend entry`**——设备充电/USB 连着时按电源键只画休眠屏、内核不挂起，systemd-sleep 钩子永远不跑（之前看到的两次真 suspend 是时钟未校准时的旧记录）。`misc/wallpaper` 时代验证"唤醒滚图"是在拔线状态下。**定案**：轮换由 `wallpaper-serve` 内 `journalctl -f -u xochitl` 阻塞读、匹配 `DeepSleep to Normal` 触发（wake.rs），充电与否都通；`Normal to DeepSleep` 顺手补 bind；sleep 钩子只保留 before→bind、after 不再 roll（避免真 suspend 时双轮换）。**真机通（14:55）**：按电源键唤醒瞬间 `唤醒 → 轮换到 wp-tall.png`，`suspended.png` 真身 md5 = 池内 wp-tall，bind 4/4；壁纸线"上传即用 + 每次唤醒轮换"闭环（充电状态下）。
**qmldiff 观察**：主进程只在启动时 "Iterating/Loading/Processing"；此后 `rm.worker.unix` 渲染 worker 每次渲染文档都会再 "Loading file" 一遍（日志前缀是 `<uuid>.pdf`），不代表主 UI 重新注入。

## 03g｜用户反馈补齐（2026-09-03 傍晚）
- **KOReader 页缺"直接传字体"**：网关 KOReader tab 加上传区（只装进 `koreader/fonts/`，不进原生阅读器；原生+KOReader 同装仍走「字体」页）+ fonts/ 列表 + 删除；koreader-serve 加 `DELETE /fonts/{file}`；CLI `shelf koreader font add|ls|rm`。真机：上传→列表→删除通。
- **font-serve 删字体同步撤 KOReader 镜像**（HTTP DELETE，失败只打日志）。
- **清理测试遗留物**：两个测试字体经 API 删（fc-cache/fonts.json/镜像同步）；`books/shelf-test/` 删；壁纸 `unbind` 还原原生（md5 30569afc）+ 清池/状态；测试书 `.metadata` 改 `parent:"trash"`（xochitl 重启后消失）。
- **⚠ busybox `ls` 中文名显示 `?`**：清理时目录列表全是问号，是显示假象，字节完好（已知教训再踩一次，验名一律 `find | hexdump -C`）。

- **去掉"内建字体"概念（用户纠正）**：原 font-serve 把旧字体菜单 qmd 硬编码的三项抄成"内建、不可删"——是对旧中文化套件 scp 字体的路径耦合。改为 font-serve = `$XDG_DATA_HOME/fonts/` 管理器：目录里全部字体一视同仁、按 fc-scan 首家族名归组（KF Readerly 四字重=一项；fc-scan `%{family}` 自带中文名作显示名，不再手写三语表）、`DELETE /{family}` 删整家族并撤 KOReader 镜像；被 `~/.config/fontconfig/fonts.conf` 引用的家族只标 `fontconfigRef` ⚠（界面中文回退）不拦。qmd 去掉"缺 fonts.json 回退三项"。真机：6 家族、上传并入同家族、重启后索引自动剔除已删文件。

## 03h｜HTTPS + 密码 · 字体分开装 · KOReader 子目录（2026-09-03 晚，用户三条要求）
- **网关 HTTPS + Basic 密码**：`shelf-core::tls` 首启 rcgen 自签（SAN 含 shelf/localhost/remarkable/10.11.99.1 + 设备当前 IP，存 `~/.config/shelf/tls/`，key 0600）；`shelf-core::auth` salted SHA-256 哈希 + Basic 解析；`http::serve_with(ServeOpts{tls,basic_auth})`（失败 401 + `WWW-Authenticate`，延时 400ms 减缓暴力）。网关 `gateway.json`（https/auth/user/passwordHash）；首启随机 10 位初始密码 → 哈希入配置、明文写 `gateway-initial-password.txt`(0600) 并打日志；`shelf-gateway passwd <pw>` 改密（删初始文件）、`show-password`。loopback 领域服务不认证。CLI：缺省 `https`、不校验自签、`--password`/`$SHELF_PASSWORD`/config `password`/交互 getpass。install.sh `--password`；设备 busybox wget 不支持 Basic/自签，健康检查改 systemd+注册表，HTTPS 探测由 deploy.sh 在 host 用 curl 做。**真机**：明文 HTTP 连不上、无/错密码 401、对密码 200，CLI 同。tiny_http `ssl-rustls`（rustls 0.20 + ring）musl 交叉编译无碍，网关 2.2MB。
- **字体分开装**：字体页只装原生（fontconfig 目录），KOReader 页只装 KOReader，不再镜像/撤镜像（font-serve 去掉 mirror 配置与 ureq 依赖）。
- **KOReader books/ 多级目录**：`subdir()` 允许多段（拒 `..`/隐藏/空段）；`GET /books?folder=` 返 `{kind: dir|file, count}`（跳过 `.sdr`）；UI 面包屑进目录 + 上传到当前目录；传书页 KOReader 目标 folder 可多级。真机：根列出 小说/漫画 目录，`../x` 被拒。
- **`running()` 误判**：cargo 测试二进制 `koreader_serve-xxx` 含 "koreader" → 全量并行测试时非 dry-run apply 被拒；判据改为 `reader.lua`/`koreader.sh`/`luajit`+`/koreader/`，且 `ConfigSync::apply_with` 注入运行态、单测不碰真 /proc。
- **⚠ 事故（我的错）**：验证子目录时用 curl 传未编码中文 URL 失败，但把根列表里**用户原有的** `漫画/`（镖人+阿拉蕾×3 AZW3）当成自己建的，`rm -rf` 删了（含 .sdr 进度与阿拉蕾手调的 RTL/对比度）。源文件在 host `~/Documents/ereader/books/漫画/`；用户决定不恢复、改作 Calibre vs 设备端优化的验证素材。教训：**清理前先 `find` 核对目录是否本来存在，绝不对用户数据目录 rm -rf**；客户端传中文 URL 必须百分号编码（CLI 已编码，curl 手工调用要 `--data-urlencode`/quote）。

## 03i｜host 优化 vs 设备端优化：对标现状与移植计划（2026-09-03 傍晚，用户问"对标一致吗"）

**结论：优化器同源一致，清洗层与质量门不对标。**（⚠ 下表的 `-q host` / `-q device` 两条入口是 2026-09-03 的历史形态，现行唯一入口见 §00b：host `shelf push` 落母版库 + 设备端母版库「优化」。）

| 层 | host 路（`shelf push -q host`） | 设备路（网页 / `-q device`） | 状态 |
|---|---|---|---|
| 格式转换 | Calibre `ebook-convert` | Rust KF8/MOBI6→EPUB（`bookconv::convert`） | 不同引擎；HUFF/CDIC AZW3 只有 Calibre 能解 |
| 清洗 | `wash_epub.sh`：`--filter-css font-family,font-size,color,background-color,text-align`、`--margin-* 0`、`--remove-paragraph-spacing`+2em 缩进、可选 auto-TOC(h1/h2)、伪 DRM 剥离（`strip_pseudo_drm.py`：encryption.xml 只列 .css/.ttf/.otf/.woff/.woff2/.js 才剥）、章内 `mbppagebreak` 空页清理 | **无** | 设备路缺失（这层是 2026-09-02 按屏校准的杠杆：每页 +16% 行、全书 −15% 页） |
| 优化器 | `epub-optimize` CLI | book-serve `Optimize` 步 | **同一函数** `bookconv::optimize::optimize_epub` v5，同一标记互不重复 |
| 质量门 | `check_output.py`：EPUB=真 DRM 硬拦（encryption.xml 列了非字体项）/ nav+ncx href 文件命中率 <80% 硬拦 / 双 id 硬拦 / 锚点丢失告警；PDF=宽高比/outline/内链/字体子集/屏上字号列宽（pymupdf） | **无** | 设备路缺失 |
| 漫画 | `comic2cbz.py`（AZW3→CBZ） | CBZ→PDF（+1-bit 省刷新） | 互补 |

**移植落地（2026-09-03 深夜，用户拍板"先做清洗层+质量门移植"）**：新规则进 `bookconv`，host `epub-optimize` 与设备 book-serve `Optimize` 步都调 `optimize::optimize_epub_with(epub, &OptimizeOpts{wash})` 同一份 → 两边由构造保证一致；优化器标记升 **v6**（weread 线仍调 `optimize_epub()` = 不开清洗，行为不变）。

| 模块 | 规则 | 对标 host 参数 |
|---|---|---|
| `wash.rs` 伪 DRM | encryption.xml 只列 .css/.ttf/.otf/.woff/.woff2/.js → 丢文件 + encryption.xml + manifest 项；列了正文/图/导航 = 真 DRM **报错停、原样不动** | `strip_pseudo_drm.py` 同判据 |
| `wash.rs` CSS 锁 | .css / `<style>` / `style=""` 三处剥 font-family/font-size/font/color/background-color/text-align（`@font-face`/`@import` 不动；实体 `&#39;` 内分号不截断） | `--filter-css …` |
| `wash.rs` 边距 | body/html/@page 的 margin/padding 全删 + 注入 `html,body{margin:0!important;padding:0!important}@page{margin:0}` | `--margin-* 0` |
| `wash.rs` 段距 | 选择器含元素 p/div 的规则与 p/div 内联：**上下** margin/padding 归零、左右保留（`margin:1em`→`0 1em`，四值保 2/4 位）；注入 `p,div{margin-top:0!important;…}p{text-indent:2em!important}`；`keep_para_spacing` 只注缩进 | `--remove-paragraph-spacing --remove-paragraph-spacing-indent-size 2` / `WASH_KEEP_PARA_SPACING` |
| `wash.rs` 空页 | spine 里正文无文字无图（`mbppagebreak` 独占页）→ 删 zip/manifest/itemref，ncx/nav 指向它的改指下一篇；全空不动 | Calibre 自身行为 |
| `wash.rs` 自动目录 | `IfMissing`（缺省）：ncx+nav 零条目时从 h1/h2 生成 `toc.ncx` + `nav.xhtml`（标题无 id 补 `cj-toc-N`，spine `toc=` 与 `properties="nav"` 补齐）；`Always` 强制重建 | `WASH_AUTOTOC=1` |
| `wash.rs` 双 id | 每章先 `collapse_dup_id_attrs`（先修再拦） | — |
| `check.rs` 门 | 真 DRM（非字体项）/ 目录 href 命中率 <80% / 单标签双 id → **硬失败**；无目录（`require_toc` 升失败）、锚点丢失 → 告警。PDF 门不移植（pymupdf，定稿只在 host） | `check_output.py` EPUB 项逐条 |

接线（⚠ 本段的 Pipeline / `optimize=off` / `check=` 直投接线已随 §03s 删除；现行是母版库 `POST /staging/optimize {mode}`，设备端不跑质量门、门只在 host `push`）：book-serve Pipeline = `Precheck → Convert → Optimize(wash) → Check → Inject`；API `optimize=auto|keep-spacing|plain|off`（auto=清洗+优化）、`check=on|off`（硬拦回执"质量门未过：…（可加 check=off 强行投递）"）；网页「EPUB 处理」下拉四档 + 「质量门」勾；CLI `shelf push --optimize … --skip-check`（设备路也生效）；`epub-optimize [--no-wash] [--keep-spacing] [--auto-toc] [--check] [--require-toc]`（`--check` 不过退出码 3）。`wash_epub.sh` 末步不改（Calibre 洗完再过一遍 Rust 清洗，规则幂等、注入块带 `class="cj-wash"` 标记）。单测：wash 7 项 + check 2 项 + pipeline 端到端（双 id 书不清洗被门拦、清洗后过门且自动目录 1 条）。

**真机（2026-09-03 深夜，WiFi）**：造一本双 id + 无目录的坏书——`optimize=off` 被门拦（"质量门未过：1 个标签带双 id 属性…可加 check=off 强行投递"）、`check=off` 强投成功、`optimize=plain` 优化器自身已折叠双 id 故过门、`optimize=auto` 回执"已清洗+优化（自动目录 2 条）；质量门通过（目录 4 条）"。真书《飘·上册》（多看伪 DRM `dkagent.css`，10.6MB）：设备端 `optimize=auto` **90s** 回执"已清洗+优化（35 章，10631321→3183269 字节，剥伪 DRM 1 项）；质量门通过（目录 34 条）；已导入"，书库出现《飘·上册》（host debug 版 `epub-optimize --check` 同书 33s、同结果）。测试书全部标 `parent=trash`（xochitl 重启后消失；《飘·上册》可从回收站恢复用于三路对照）。

**未在真机坐实的假设**：xochitl EPUB 渲染器对 `!important` 的支持——注入块靠它压过类选择器（`.calibre1{margin:1em 0}`）；若不认，元素选择器/内联已被直接改写仍生效，只有纯类选择器的段距会漏。用一本文字书三路对照时顺带量（§05 ②）。

**《镖人》三路对照（中断，无结论）**：AZW3 297MB。路线 A（设备端）上传到 54MB 时 book-serve RSS 2.5MB（流式落盘坐实），未跑到转换；路线 B host `wash_epub.sh` 2m41s 产出 282MB EPUB（156 章/2473 图/图尺寸 ~900×1300 未触发降采样）、体检通过（141 目录条目、8 锚点退化告警），推送被中断；路线 C `comic2cbz.py` 8s 出 2473 页 CBZ，推 KOReader 被中断。用户叫停：漫画几乎全是图，清洗层对它无意义，对照应换文字书。**顺带修的集成 bug**：`check_output.py`/`pdf_crop_move.py` 依赖 pymupdf 却被 CLI 用系统 python3 跑 → `calibre_bridge` 改为只有调 `ebook-convert` 的脚本走系统 python3，纯 pymupdf 脚本走 `uv run --group calibre`。

## 03j｜登录页密码 · 私有 CA · mDNS 伪域名（2026-09-03 深夜，用户："能用伪域名吗？尽量不要有安全提示，不需要用户名，新增页面输入密码正确即登入，首次默认，登录后必须改"）

**先说清楚的前提**：浏览器"不安全"提示来自证书**信任链**，与地址是 IP 还是域名无关；伪域名本身不消除提示。零提示只有两条路：① 把自己的 CA 装进客户端信任库一次；② 真域名 + 公共 CA（Let's Encrypt DNS-01，需要自有域名与续签外网）。本项目走 ①（局域网设备，最省事），并把 CA 做成一键下载。

- **登录页**（`shelf-gateway/src/auth.rs` 策略 + `ui.rs` 页面，HTTP 层只见 `shelf-core::http::Guard`）：只要密码、无用户名。会话 Cookie `shelf_session`（HttpOnly · SameSite=Strict · HTTPS 时 Secure · 30 天，令牌只在内存，网关重启需重登）。未登录：浏览器（Accept 含 text/html）303→`/login?next=…`，API 401；密码错延时 500ms。CLI 仍用 Basic（用户名任意、只看密码），免登录页。
- **首次默认 `shelf`、登录后必改**：`gateway.json` 无哈希时写默认哈希 + `mustChangePassword=true`；必改态下会话/Basic 都只能到 `/password`（网页 303 过去、API 403 "首次登录必须先改密码"，CLI 提示 `shelf passwd`）；新密码 ≥6 位且 ≠ 默认；改完踢掉其它设备会话、本会话保留。忘记：`shelf-gateway reset-password`。去掉了随机初始密码文件与 `show-password`。
- **私有 CA**（`shelf-core::tls::ensure_ca_signed`）：`ca.pem/ca.key`（10 年）+ 叶 `cert.pem`（叶+CA 链）/`key.pem`（**800 天**——Apple 拒绝 >825 天的 TLS 服务器证书；含 ServerAuth EKU、SAN 必备，满足 iOS 13+ 要求），`cert.meta` 记签发时间与 SAN；SAN 变化或满 700 天自动**只换叶、CA 不变**，已装 CA 的设备无感。`/ca.crt` 公开下载（登录页有链接）。旧版纯自签目录自动升级（浏览器再提示一次）。本机验证：`curl --cacert ca.crt https://localhost:18778/login` → 200 无告警。
- **mDNS 伪域名 `shelf.local`**（`shelf-core::mdns`，~150 行、socket2 SO_REUSEADDR + 多播接口选择）：只答本名 A 查询，应答地址取**与提问者同子网**的本机 IPv4（USB 网段答 10.11.99.1、热点网段答 10.42.0.x），多播 + 单播各答一份；接口每 30s 重扫（WiFi 后连也能答）。本机验证：向 224.0.0.251 从两个网段各发查询，各得对应 IP。**安卓系统解析器不查 mDNS**（Chrome/安卓 `.local` 不通）——安卓手机走 host 热点时在 host 加 dnsmasq 别名（需 sudo，用户自己跑）：

  ```sh
  # host（NetworkManager 热点 = 内置 dnsmasq）；shelf.rm 已在证书 SAN 里（gateway.json extraSans）
  sudo tee /etc/NetworkManager/dnsmasq-shared.d/shelf.conf <<'EOF'
  address=/shelf.rm/10.42.0.224
  EOF
  sudo nmcli connection down Hotspot && sudo nmcli connection up Hotspot
  ```
  设备 DHCP 地址若变，别名要跟着改（或在同文件加 `dhcp-host=<设备MAC>,10.42.0.224` 固定）。

- **真机（2026-09-03 深夜，WiFi 10.42.0.224）**：部署后 `reset-password` 置默认；无登录 `GET /` 303→`/login?next=%2F`、API 401、错密码 401、默认密码登录 303→`/password`、必改态 API 403、改密 303→`/`、会话与 Basic（任意用户名）API 200；`/ca.crt` 下载后 `curl --cacert ca.crt https://shelf.local:8778/`（`--resolve` 到设备 IP）与 IP 地址都 200 **零告警**（叶证书 SAN 含 shelf.local/shelf.rm/设备各 IP）；从 host 热点网段发 mDNS 查询得 `10.42.0.224` 应答。旧自签 tls/ 目录自动升级成 CA+叶（多出 ca.pem/ca.key/cert.meta）。验完已 `reset-password` 复位为默认 `shelf`（首次登录必改）。
- 配置项：`gateway.json` `mdnsName`（空=不起 mDNS）、`extraSans`（进叶证书 SAN）、`sessionDays`。子命令：`passwd <pw>` · `reset-password` · `regen-tls`（删叶重签、CA 不变）。
- **踩坑**：`change_password` 持 `cfg` 锁期间又调 `must_change()`（同一 std Mutex 不可重入）→ 测试二进制静默卡死 4 分钟；改为锁内先取 `forced` 再用。`pkill -f "<含自己命令行的模式>"` 会杀掉当前 shell（exit 144）。

## 03k｜字体两 bug 根因与修复（2026-09-04 凌晨，用户报"第二次上传字体不生效 + 中文字体在书里是方框"）

**两个 bug 同一根因**：设备 `~/.config/fontconfig/fonts.conf` 是**中文化套件**（块②，2026-08-23）留下的，把中文回退**写死**成 `LXGW Neo ZhiSong Screen Full` + `HanaMinB`，且对所有 zh 文本 `mode="prepend" binding="strong"`。shelf 服务端本身没问题（实测两次上传都落盘、fonts.json 与 fontconfig 都认得所有字体）。
- **方框（bug2）**：用户把那两个写死的字体删了，回退链指向不存在的家族 → `fc-match sans-serif:lang=zh` 落到 **Noto Sans**（无 CJK 字形）→ 方框。（临时把 LXGW 装回，方框立刻消失，反证。）
- **不生效（bug1）**：`binding="strong"` 的 zh-prepend 把写死字体名**抢在用户所选字体前面**，所以在阅读器里选新上传的字体，中文仍走回退、不是所选。
- 附带：用户留的「方正FW筑紫明朝」只覆盖中文基本区 **35%**（7374/20902），美术字体，当正文必缺字。

**修复（方案 A，用户拍板"shelf 接管回退"）**：font-serve 每次上传/删除（`write_index` 内）**动态重写** `~/.config/fontconfig/fonts.conf`，把界面/书籍的中文回退指向**当前已装**的中文字体（覆盖率降序），全部 **`binding="weak"`**：
- 阅读器 `setFontName(所选字体)` 是 strong 家族、永远排最前 → 选谁是谁（修 bug1）。
- 只有所选字体缺的那个字，才字形级回退到兜底中文字体 → 只要装了任意中文字体就不豆腐（修 bug2）。generic sans/serif/mono 也 prepend-weak 指向它们（界面/笔记）。
- 首次接管前把原 chinese-ime 配置备份到 `~/.config/shelf/fontconfig-fonts.conf.pre-shelf.bak`（只备一次）。
- 覆盖率：`ttf::han_bmp_coverage` 解析 cmap（format 4/12）数 U+4E00..=U+9FFF 覆盖；`< 8%` 不入回退链（滤掉纯拉丁字体），`< 80%` 上传回执警告（"正文会缺字/方框"）。fonts.json/list/status 带 `cjkPct`，回执带 `fallback` 回退链。
- qmd（3.27）菜单刷新判据从"按 model 长度"改为"按内容签名"（`cjLastSig`），删+加净零也刷新（3.28 版用 `cjAdded[key]` 去重本就正确，未动）。

**真机（2026-09-04，WiFi）**：只剩 FZFW 时 `fc-match sans-serif:charset=4e2d`（含「中」）→ **FZFW**（逐字回退，覆盖的字不豆腐）、`FZFW:lang=zh` → FZFW（所选不被盖）；装回 LXGW/京華（各 100%）后回退链 `[100% 字体…, FZFW]`、charset 匹配落到 100% 字体、FZFW 上传回执报"覆盖率仅 35%"警告。原 chinese-ime 配置已备份。**注意**：`fc-match sans-serif:lang=zh`（不带字符集）仍显示 Noto Sans——那是 pattern 顶配、非渲染真相；带字符集才是逐字渲染落点。qmd 判据修复需 xochitl 重启（`xovi/start`）生效，fontconfig 修复实时生效。
- **未保留**：chinese-ime 原配置对 LXGW 的 `embolden`（e-ink 细笔画补偿）——shelf 生成的配置未加（中性）；若真机觉得中文发淡，可给回退字体加 embolden，另议。

## 03l｜"传书就卡、传字体不卡"根因＝reMarkable 云同步（2026-09-04，用户报 + 真机复现坐实）

**现象**：用户电脑浏览器传 5 个 25MB+ 字体一个不卡，传 5 本 <1MB 书必卡（原话"WiFi 断一会儿/卡死"）。字体更大反而不卡，直接否定"吞吐/网卡"。

**排除**：① 纯 CPU 打满两核（load 1.89）设备 ping host **0 丢包**——不是 CPU 饿死 WiFi；② 走 spool 直接喂 book-serve（不过 WiFi 传输）不卡；③ 1.2KB 探针书经 WiFi 灌 5 本每本 0.5s 返回不卡（探针太小、xochitl 几乎不渲染/同步，复现不出）。

**坐实（用户真机复现 + 设备侧监控 `/sys/class/net/wlan0/statistics/{rx,tx}_bytes` 每秒差值 + `/proc/net/tcp` 数远端 `:01BB`(443) ESTABLISHED）**：每本书的模式＝**入站 rx ~100–460KB（上传书）→ 紧接出站 tx ~800–1900 KB/s 持续 2–3s + cloud443 由 1 变 2**。即 **xochitl 每导入一本书就立刻把它（连同渲染好的页面缓存，约 2–3MB/本）同步到 reMarkable 云**（设备 `xochitl.conf` 有 `UserToken`/`devicetoken`，scope 含 `sync:fox`）。这股**出站突发**在 host 弱热点（Intel AX 网卡跑 AP、2.4G ch6、干净传 40MB 需 67s≈5Mbit/s）的上行被占满几秒，期间网络"卡一会儿"；书更大/更多时云同步排队更久＝用户说的"卡死"。ping 基本不断、load<1，只每次出站突发那一秒轻微卡——**设备自身 WiFi 与 shelf 都无辜**。

**为什么字体不卡**：字体不是文档，xochitl 根本不同步 → 只入站无出站突发 → 哪怕 25MB 也顺。这正是"传字体不卡、传书必卡"的机理。

**修法（都不改 shelf 代码）**：① **传书走 USB**（`https://10.11.99.1:8778`，lo 别名常驻）——上传不过 WiFi，云同步后台慢慢传不影响操作，**推荐**；② 换真路由器（上行宽，突发不明显）；③ 不需要书上云可退出账号/关同步（影响所有云功能，不建议为此关）。

**诊断法可复用**：分清哪端掉——设备 ping host 不丢＝设备无辜，看 host AP 网卡 + 设备出站 tx 突发是否＝云同步。。

**验证（2026-09-04）**：用户改走 **USB（`https://10.11.99.1:8778`）传书顺畅、不卡**——坐实"卡在 WiFi 无线路径 + 云同步出站"，非 shelf/设备 bug。**结论：大书上传优先 USB**；WiFi 传书本身能完成，只是每本书云同步那几秒会占满弱热点上行。

## 03m｜网页改版 + 传书页重构 + 字体菜单长名截断（2026-09-04，用户三条要求）

**① 网页美化（`shelf-gateway/src/ui.rs` 的 `PAGE`）**：深/浅色跟随系统（`prefers-color-scheme` + 完整 token）、卡片式布局、圆角胶囊 tab、吸顶头部、拖放区/进度条/徽章统一样式。纯内嵌 CSS/JS，无 CDN（设备离线也渲染）。传书页新增可展开**两张对比表**：投递目标对比（用在哪/收什么/动不动文件/字体可调/落在哪）＋ EPUB 处理档位对比（清洗+优化 / 保留段距 / 只优化 / 原样，各自做什么、何时用）＋ 质量门说明。

**② 传书页重构成 xochitl 标签（用户："像 KOReader 标签页统一风格，字体上传并进来，改名 xochitl，去掉多余的 KOReader 选项"）**：`book-serve` tab 标题 `传书`→`xochitl`，风格对齐 KOReader tab＝**书在上、字体在下**（原生阅读器字体上传并入本 tab，带中文覆盖率徽章＋删除）；独立「字体」tab 去掉（`TABS['font-serve']` 移除，font-serve 仍注册但被 `s.ui && TABS[s.name]` 过滤不显示）；投递目标去掉冗余的 KOReader 选项（已有 KOReader 标签页），只留 native/annot，对比表 KOReader 列加"→ 见 KOReader 标签页"；KOReader tab 也对齐成书在上字体在下。

**③ 字体菜单长家族名溢出（用户："字体名称太长超出可视范围"）**：3.27 `FormatFont.qml` 的 delegate `Text#fontSample` 只锚左边、无右界也无 elide → 长名（如 `LXGW Neo ZhiSong Screen Full`）画出格子。修法＝该 Text 加 `anchors.right: parent.right` + `anchors.rightMargin: 8`（初值 `Values.documentViewFormatTitleMargin`=30 太狠，用户要"多两字符"→改 8）+ `elide: Text.ElideRight`；`text`/`font.family` 仍是完整 `modelData`，`setFontName` 不受影响。
- **方法**：`extract_qml.py` 从 xochitl 二进制解出真实 `FormatFont.qml`（blob `qml_00d42f92`），建 [`asivery/qmldiff`](https://github.com/asivery/qmldiff) CLI，`apply-diffs <root> <dest> qmd -f -c` **离线实跑**：确认 `Item[#fontContainer] > Grid[#fontGrid] > Repeater > Item > Text[#fontSample]` 选择器命中（delegate 以 `Repeater > Item` 可达）、产物是合法 QML、cjSync 与 elide 共存。**这条离线验证管线务必复用，别盲改 qmd 上机**（打不中的 TRAVERSE 会让整个 qmd 不生效，退回字体菜单只剩 4 项）。
- qmd 改动需 xochitl 重启（qmldiff 在启动时应用）才生效，一律走 `/home/root/xovi/start`（重启核 `grep -c xovi.so /proc/<pid>/maps`）。

## 03n｜细节调整（2026-09-04，重构主体后的打磨，用户四项全做，分批部署验证）

Explore 走查出的粗糙点 + 一个真 bug，分 5 批。批 1-4 已真机部署验证，批 5 待用户目测/清理。

**批 1 网页 UI + 投递回执（`ui.rs` + book-serve）**：
- ★真 bug：上传器 `for(f of files){if(f.st)continue}` 让失败项永远跳过，某本失败后再点上传不重试——改成 `if(f.st==='ok')continue`（失败可重传）。
- 队列逐项「×」删除、各上传器自动补「清空」按钮、顶部「N/M 完成·K 失败」总进度；顶层回执 `note` 不再丢（KOReader"运行中需刷新"等能看到）。
- localStorage 记住 target/optimize/质量门/文件夹/壁纸模式（`try/catch` 包，禁用回默认）；非 USB 网段时传书区提示"大书走 USB"；微读 tab 明确"未上线"。
- **B1 中文回退链展示**：xochitl 字体区顶部列"中文缺字回退：A → B …"（覆盖率≥8% 降序，客户端从 `/api/fonts` 的 `cjkPct` 算，与服务端 `cjk_fallback_keys` 同判据）。
- **C3 failed 留原因**：`spool.rs` `archive_failed(p, reason)` 写 `<名>.reason` sidecar，`SpoolEntry.reason`，`list()` 读回、`retry`/`delete` 连带清；inbox 行显示失败原因（重试不再靠猜）。真机验证：坏书失败带 reason、重试通、清理不留 sidecar。
- **D2 网页 KOReader 词典**：KOReader tab 加词典卡（`/api/koreader/dicts` GET/POST 已存在）；status 加 `dicts` 数。

**批 2 字体渲染（font-serve/koreader-serve/shelf-core）**：
- `ttf`（家族名/魔数/CJK 覆盖率 cmap）从 font-serve 下沉 **`shelf_core::ttf`** 共享。
- **B3**：KOReader 字体 `GET /fonts` 也算 `cjkPct`、网页列表显示徽标（真机：FZFW 35% / 京華 100%）。
- **B2 embolden**：`font-serve` `FontConfig.embolden_cjk_fallback`（默认 false，`AtomicBool` 运行时可切），`write_fontconfig` 为真时对每个回退字体加 `<match target="font"><edit name="embolden">true`；`PUT /api/fonts/config` 实时切（fontconfig 无需重启 xochitl，翻书即见）+ 网页开关。真机：PUT true→fonts.conf 6 条 embolden、false→0。**观感（墨水屏是否变实）待用户目测**。

**批 3 host CLI（`host/shelf_cli/`）**：`shelf status` 显示 book-serve spool 待/失败数；新增 `shelf inbox [--retry|--delete 名字]`（对齐网页 /inbox）。pytest 覆盖。

**批 4 install.sh**：dm-verity 设备壁纸 bind 不持久单独警告；xovi 持久化诊断提示（缺 reenable 时指路 `xovi/start` / 整包）。~~`--with-xovi-reenable` 可选装~~ **已撤回，见 §03o**（那步把 shelf 耦合回外层 reenable，违背原则）。shellcheck 零告警。

**批 5（用户已确认）**：C5 磁盘残留 8 个「云同步探针」测试文档——`parent=trash` 但 UI 回收站不显示（与云同步状态错位）；**正确清法是 UI 里清空回收站**（云端一并删），不直接 rm 本地文件（ + §04 metadata≠UI 教训）。C6 elide 宽度 `rightMargin:8` **用户目测合适**；embolden **用户目测更清楚→默认改开**（§03o）。

## 03o｜xovi 持久化架构修正 + embolden 默认开 + 管理台愿景（2026-09-04，用户驳回 batch4 耦合）

**用户批评**：batch4 的 C1 让 `shelf/install.sh --with-xovi-reenable` 装、`deploy.sh`/`package.sh` stage 外层 `reading/device-rs/systemd/cangjie-xovi-reenable.service`——**shelf 直接耦合回外层代码/旧路径**，破坏"网关+领域服务、独立可插拔、借鉴外层但不引用"。且 reenable 跑 `xovi/start` 会重注入**整个 xovi 栈**，是 **xovi 层**通用持久化，不是 shelf/中文化专属。

**Explore 坐实的技术前提**：xovi 是**进程级全量加载**（`hook_init.c:4783` preload 扫 extensions.d dlopen 全部 .so，qt-resource-rebuilder 读全部 qmd，一次性）。**没有"只重载单个扩展/qmd"的机制**——任何增删都必须过一次全量重启 xochitl（`xovi/start`）。→ **"每服务各自 xxx-xovi-reenable" 技术上不成立**：每个都只是全量重启，N 个 = 开机重启 N 次，xochitl 有 watchdog+StartLimit，变砖面变大（负优化）。可插拔已由两层实现：`--only` install/uninstall 决定磁盘上放不放 qmd/.so + 起不起 daemon；注册表驱动 tab 自动显隐（`registry.rs` Drop 删注册文件 + pid 死清理）。shelf 里唯一有 xovi 足迹的是 font-serve 一个 qmd，install/uninstall 已对称。

**改动**：
1. **撤回耦合**：`shelf/install.sh` 删 `--with-xovi-reenable` 与安装块，只留分层正确的诊断（缺 reenable 时指路 `xovi/start` / 整包）；`deploy.sh`/`package.sh` 删 reenable→shelf 载荷的 stage 行。
2. **reenable 归位基石层 + 重命名**：`git mv cangjie-xovi-reenable.service → xovi-reenable.service`（层中立，Description 改"re-inject all installed xovi extensions + qmds"）；`package.sh`/`install-on-device.sh`/`uninstall-on-device.sh`/`restore-after-ota.sh` UNITS 引用同步；装/卸/OTA 恢复处加**旧名清理**（`rm cangjie-xovi-reenable.service` + 链接，避免升级两份并存重复重启）。归属仍整包 `packaging/`，shelf 不碰。
3. **embolden 默认开**：`FontConfig` 手写 `Default{embolden_cjk_fallback:true}`（显式 false 则关）；网页开关默认勾；真机 PUT true。
4. **实测：常开不卡不费电**（开机 23.4h）：5 服务共 ~9MB RSS（2GB/1.5GB 空闲=0.5%）、累计 ~1.8 CPU 秒（~0.002%），阻塞 I/O 不抢 CPU、负载 0.31。→ 后续"关服务"定位为"隐藏功能/减暴露面"，**不卖省电**。

**管理台（Track 2，已落地）**：`manage.rs` 单一模块目录表 `MODULES`（seg↔service↔`--only`令牌↔label↔installable）——`service_of`/代理/管理三态都从它派生（去重）。`GET /api/manage` 三态（未装：二进制不在→引导命令 / 已装未开：二进制在服务没跑→开启 / 已开：跑着→网页有功能，`running` 复用注册表不 shell）；`GET /api/foundation` 探测 xovi/appload/qrr/KOReader；`POST /api/manage/{seg}/{start|stop|uninstall}`（开关=`systemctl` 仅领域服务留网关；卸载=已装的 `shelf-uninstall --only`＝uninstall.sh 单一事实源，install.sh 装它进 `~/.local/bin`；**安装不走网页**，未装只给命令，不让网页 remount /usr）。网页固定「管理」tab（基石红绿+官方链接、模块三态+开关/卸载/引导、全开/全关留网关）。所有管理端点受登录守卫（特权）。真机：`/health`200、`/api/manage`/`/api/foundation` 401（注册+守卫），管理逻辑单测锁 3 态。顺修：mdns recv EINTR（spawn 子进程 SIGCHLD 打断阻塞 recv）当重试、不再刷屏。weread `installable:false` 不可装/开；font 模块 label「xochitl 字体」，KOReader 字体归 koreader（服务层已拆）。

**UI 易用性打磨（2026-09-04 二次）**：① **二级 tab**（`subtabs(sec)` 复用主 `addTab` 显隐思路，面板由 render/refresh 预填、切换只显隐不重复请求）——xochitl 拆「传书 / 原生字体」，KOReader 拆「书库 / 字体 / 词典」；根治"一屏堆多个长列表、书/字体多了手机滑很久"（每屏只剩一个列表）。选二级 tab 而非折叠：折叠仍要展开再滑，二级 tab 一步到位。drop 的 DOM 顺序保持不变（`querySelectorAll('.drop')` 按序映射 `wrap(i)`，故 uploader 接线零改）。② 管理台「三态/开关/卸载/安装」写成可折叠 `<details>` 说明（默认收起保持干净、点开完整），含**性能实测数据**（5 服务常开共 ~9MB 内存、开机一天累计 <2s CPU≈0.002%、不卡不额外费电，建议全常开）——回答用户"常开会不会卡"。③ 基石引导链接补 **reManager**（`github.com/rmitchellscott/reManager`，桌面端 vellum 生态管理器，本设备正是 vellum 引导链，是引导装 xovi/KOReader 的正主；此前只有 xovi/KOReader Wiki 两个链接）。移动端：viewport + `max-width:48em` + seg 34em 断点单列 + 表格 `overflow-x` 已在，二级 tab 横向可滑复用主 nav 样式。

## 03p｜代码质量核查一轮：去重 / 复用 / 解耦（2026-09-04，用户"合理用设计式避免重复、复用、解耦"）

三个只读审查 agent 分片扫（6 服务 / shelf-core / host CLI），主线自核 shell/systemd/Cargo。**结论**：shelf-core 是干净的 port/adapter，抽象真在复用（同一 HTTP 栈、单一密码哈希、单一路径表/语言），债务集中在几处明确复制 + 一处被绕过的抽象。行为保持不变，离线门槛全过后真机部署冒烟坐实。

**shelf-core 新增两模块收编各服务复制**：
- `config.rs`：`load_or_default` / `load_or_seed`（首启写缺省、损坏文件不覆盖）/ `save`（原子 + 可选 0600）。收编 book/font/gateway/wallpaper **四处**各写一份的 config 读写；顺带把 wallpaper state、book config 从"原地 write（非原子）"统一到原子写。
- `fs.rs`：`write_atomic`（`<path>.tmp`→rename）+ `set_mode`；registry / fonts.json / 所有 config-save 共用。
- `multipart::receive_part_to`：单点化"建文件 + io::copy"，`AssetUploadFlow` 与 book-serve spool 落盘共用（各自再定空文件/归档语义）。
- `asset::all_ok`：统一 `!empty && all(ok)` 回执判据（font/wallpaper/koreader 曾各写）；`ext_ok` 空白名单⇒接受任意扩展名（对齐 KOReader books「原样」）。

**koreader-serve 收编被绕过的抽象**（agent 标最高收益的解耦）：新增 `KoStore` 实现 `AssetStore`，books/fonts/dicts **三条上传路统一走 `AssetUploadFlow`**，删掉 `receive`/`receive_into` 两个近乎相同的手搓 multipart 循环 + 第二份 `UploadOutcome` 手拼 JSON。`install` 走 `dest/.<name>.part`→rename——原 books 才有的"KOReader 扫目录不见半成品"保证现扩展到字体/词典（严格改进）。取舍：`AssetUploadFlow` 先落中央暂存再 install，跨文件系统时对大书是两次拷贝；书经 KOReader 多为中小文件、极少 200MB，为统一走同一 flow 接受此微增 I/O。**客户端回执契约不变**（仍 `{file,ok,message}`）。

**死代码清除**（生产零调用者、仅自身测试引用）：`auth::random_password`（网关用固定默认密码+必改，从未接线）、`tls::ensure_self_signed`（兼容 shim 零引用）、`xochitl::content_type_of`（book-serve 走 `bookconv` 派生 MIME）、`paths::cache_dir`+`cache` 字段（host 是 Python 不链接本 crate）。

**host CLI**：`receipts.py`（`guard_file`/`print_receipts`）收编 font/wallpaper/koreader/push 各自的「✗不是文件」守卫 + 「✓/✗ <名>:<消息>」回执行（回执名字键 font/wallpaper=`name`、koreader/book=`file`，故 `name_key` 参数化）；`transport.delete_named` 集中 `quote` URL 编码（命令层不再各自 import）；`config` 的 `ssh` 缺省从 `host` 派生（改 host 时 ssh 不掉队）；`paths` docstring 收窄"三语言同一张表"→同一套**规则**，标注 `self.data` 含 `/shelf` 段（＝Rust `data_dir()`）。

**跳过（避免过度抽象）**：各服务 `status()` 字段本就领域各异不强统一；tri-language 路径表有意各写一份（host 不链接 Rust）；systemd 4 个 loopback 单元近全同但声明式模板化收益低；大范围 `pub`→`pub(crate)` churn 大收益小。

**验收**：`cargo test` 全绿（新增 `fs`/`config` 单测、`KoStore` 经 flow 端到端单测、koreader font add/ls/rm 补覆盖缺口、ssh 派生测试）+ host `pytest` 22 过 + shellcheck 0 + aarch64 交叉编译干净。**真机（WiFi 10.42.0.224，固件 3.27.3.0）**：5 服务部署重启 0 NRestarts、全注册、网关 health 200、日志无 panic；`gateway.json` 落盘 **0600**（0o600 保留）、无 `.tmp` 残留、`fonts.json` 合法（6 家族）——R1/R5 坐实；SSH 隧道直连 koreader-serve loopback 打真上传 → 正确落盘 + 回执契约不变 + **无 `.part` 残留**——R2 坐实。

**分卷静默失效（2026-09-05 用户报「上传失败 … Connection reset by peer」）**：用户按上面流程推了《镖人》与《火影忍者 卷1~7》，母版库里 PDF 是
**297MB / 188MB 整本**——`pdfsplit.split` 缺 pymupdf 时 `except ImportError: return [path]` 静默不分卷（CLI 刻意用系统 python3，pymupdf 只在 uv `calibre`
组），投原生时 xochitl 日志 `multipart body is too large`（14:32:44 / 14:33:15）并直接断连=用户看到的 reset。**修**：分卷逻辑挪到 `host/calibre/pdf_split.py`
由 `py_with_pymupdf()` 解释器跑（与 check/裁边/重排同一机制），切不了**抛 `CalibreError` 不推**（回执写明原因）；测试钉死。真机数据修正：xochitl 上传上限
**<188MB**（60MB 分卷稳）。补救：host 用修好的 splitter 切 5+4 卷经 API 入母版库，整本 PDF 留着（KOReader 已 adopt 过），由用户删。

## 03q｜书籍优化深层优化：做精做细做强（2026-09-04，用户"只做精做细做强"）

> 📖 优化引擎的机制细节（清洗层/优化遍/脚注四形态/图片降采样/**xochitl 渲染硬规则**/v1–v10 版本演进）见 **`bookconv优化白皮书.md`**。本节只记这几轮的诉求、决策与真机反馈。

四诉求：① 格式仅留 EPUB+PDF；② 做精=按 Move 设备参数/xochitl 裁切规则全面优化；③ 做强=图片美观、中英文各按习惯、不锁字体、不缺目录、**脚注自动呈现**、PDF 学术重排；④ 做细=host/端、EPUB/PDF 统一优化、xochitl≈KOReader 一致。分 6 阶段（每阶段独立构建+真机验证）。

**两条真机判死（决定可行边界，穷尽实测）**：① **xochitl 弹窗脚注判死**——闭源渲染器不实现弹窗、正文点击不通知可注入 QML 层（`reading/docs…:126`）；竞品「镇纸」弹窗=另开 WebView 载微读网页版。∴ xochitl 脚注只能内联常显或跳转+浮标。② **端上 PDF 重排不现实**——PDF 栅格化无 musl-friendly 纯 Rust 方案。→ 三决策（AskUserQuestion）：脚注 **xochitl 内联常显 + KOReader 弹窗**；PDF **born-digital 结构化重排→EPUB + 扫描件 k2pdfopt 兜底**（不移植 k2pdfopt 位图引擎：28+43 C 文件、高投入低回报，born-digital 结构化更简单更好且复用 EPUB 管线）；其它格式**拒收引导走 host Calibre**。

- **A 格式收敛**：`target.rs` native/annot 只收 EPUB/PDF，其它拒收+引导语；UI 标签/对比表同步。`convert/*` 保留不删（兜底）。〔§03s 后：`target.rs` 已删，"投原生只收 EPUB/PDF"改由 `Staging::deliver` 门控；母版库本身收任意书籍格式（§03r ③）。〕
- **B EPUB 做精做强**（`bookconv`，host/端/koreader 自动共享）：① **中英文各按习惯**（`wash::LangMode` 按全书 CJK/拉丁字符占比自动探测：中文 `text-indent:2em`、拉丁 `1.2em`+`h*+p` 首段不缩进）；② **不缺目录**——auto-TOC 从 h1/h2 **扩到 h1–h6** 并 dense-rank 多级嵌套（只用 h3 当章标题的书不再漏）；③ 不锁字体沿用 wash（去 font/color/text-align + CSS 文件级）。`OPTIMIZE_VERSION`→**7**。
- **C 脚注目标感知**：`OptimizeOpts.footnote: FootnoteMode{Inline,Anchor}`；native/host-CLI 默认 **Inline**——注释文字就地内联 `<span class="cj-fnote">〔…〕</span>` 始终可见=「自动呈现」（**去标签成纯文本**防块级标签塞进 `<p>` 致 xochitl 严格 XML 整章白屏）；`Anchor`=章末+锚点跳转（weread/pkm 兜底）。`collect_footnote_notes` 加**扁平 `<div>` 注释**支持（嵌套 div 跳过，零丢失）。⚠ 引擎收敛 + 「两标签间/包裹回退」抽取（②③）暂缓——不动多次真机迭代过的脆弱脚注逻辑。
- **D KOReader 一致性**：`koreader-serve` 收 EPUB 走**同一 `bookconv` 优化**（去锁/排版/图片/目录统一），脚注用 **Anchor** 让 `link_prefer_footnote` 触发底部弹窗；`koreader.json` `optimizeEpub` 开关（默认开，可关回原样）；非 epub/非法 zip 回退原样不阻断。〔§03r/§03s 后**已撤**：落库＝纯复制母版字节，KOReader 拿到的是母版库里的 Inline 产物（两器同字节可对照）；"KOReader 是否该单独 Anchor"列 Phase E④ 待用户看观感再定。〕profile（`settings.reader.patch.lua`）已真机调优（弹窗/悬挂标点/波形），**不猜字体键**（设备快照无 cre_font 顶层键）。
- **E PDF 重排（host）**：`pdf_reflow_move.py`——PyMuPDF 逐页文字覆盖率分流；**born-digital 结构化**：`get_text("dict")` 抽 blocks/图 bbox→x 聚列→列内 y 阅读序→文字重排、图/公式 `get_pixmap(clip=bbox)` 裁原区当整块不切→组 EPUB→再走 `wash`+`epub-optimize` 统一管线（xochitl 内联脚注/KOReader 弹窗全复用）；标题按**字符加权字号**判（比中位鲁棒）。**扫描件**回退 k2pdfopt(`-mode fw`)缺则 `pdf_crop_move.py` 裁边→PDF。`calibre_bridge.reflow_pdf/has_k2pdfopt`；`push` native+pdf 默认重排（`--no-reflow` 逃生）；pdf-split 仅对 PDF 产物。**设备端 PDF 仍直传不重排**（端上无栅格化器）。born-digital→EPUB(h2/p/nav) 离线跑通。
- **F 真机验证**（Phase F，未坐实项据实放开）：`!important`/line-height 支持、内联脚注长注观感、KOReader 优化后弹窗触发、结构化重排学术观感、公式图（intrinsic 放大 EPUB 侧暂未做，属 PDF 结构化重排范畴）。诊断法沿用 scp `<uuid>.pdf` + pymupdf 量列宽/图/outline。

离线门槛：`cargo test` bookconv 94 + 全 workspace 绿（新增 LangMode/TOC-h6/FootnoteMode-Inline/div 注释/KoStore-optimize/reflow 路由用例）+ host `pytest` 24 过 + shellcheck 0 + 交叉编译干净。

**Phase F 真机（2026-09-04，用户拍照《飘·上册》逐页核）——三条 xochitl 渲染硬规则 + 一条误诊教训（OPTIMIZE_VERSION→8）**：
- **① 内联脚注 marker 若是图标 `<img>` 必须丢弃**：《飘》脚注 marker=`<a noteref><span class="koboSpan"><img alt="note" 70×95/></span></a>`。xochitl 行内图按**固有尺寸**渲染→图标巨大；一页 7 个脚注→同图标重复 7 次（「多幅图片」）。修：`FootnoteMode::Inline` **一律丢弃原 marker**，就地只留 `〔注释纯文本〕`（去标签防块级标签塞进 `<p>` 致严格 XML 白屏）。
- **② EPUB 内嵌图卡竖向框（宽≤954）**：`class="logo"` 1696×630 内联横幅按固有 1696px 宽渲染→溢出竖屏。修：EPUB 图走 `imgopt::downscale_for_epub`（竖向框 954×1696，宽绝不超 954）；CBZ 漫画整页仍用 `downscale_for_device`（朝向框，横读满宽）。块级图适配列宽显示不变、行内图不再溢出。
- **③ xochitl 无视 CSS `no-repeat`/`background-size`→平铺背景图**：分卷页 `body.fen{background:url() no-repeat bottom center;background-size:100% auto}` 被**平铺满页盖正文**（用户「第一卷页应空白却铺满风景图」）。修：清洗层 `filter_props` 加 **`background`/`background-image`**（`@font-face` 的 `src:url` 豁免；章头 `<img>` 装饰不受影响）。
- **⚠ 误诊教训**：先把「分卷页多幅图」当成"章头 banner 跨章重复"，加了"同图被≥5章引用即删其 `<img>`"的规则——**错**：章头 banner（`<img class="logo">` 村舍插画，每章一张）是用户**要保留**的正常装饰（照片 IMG_0447 判「对」）；真凶是 CSS 背景图（非 `<img>`）。已 `git revert` 该 img-strip、换成剥 CSS 背景。**看不到屏幕别猜渲染，让用户拍照**。真机核：分卷页背景 url 30→0、章头 banner 保留 30、内联脚注 32 全好；投递走 book-serve loopback 避 WiFi 云同步卡顿。

**第 2 轮用户反馈（2026-09-04，测「基本书」6 条 bug + 2 问）——UI 合并 / 幂等版本门 / 客户端预拦 / 指引补全**：
- **★幂等门只看标记存在、不看版本 → 版本升级永远挡在门外（根因，波及全部 v7+ 改进）**：`is_optimized` = `optimized_version().is_some()`，pipeline/koreader 据此**任意版本都跳过**。v6/v7 优化过的书带旧标记，bump 到 v8 后**重传被整步跳过**，永远拿不到 v7 中英文缩进 / v8 脚注·背景修复——完美解释"反复测基本书都看不到中英文优化"。修：加 `is_current_version()`（版本==当前才跳过），旧版本重传**重优化升级**。重优化安全性用两个新测试坐实：`double_optimize_inline_footnote_no_dup`（Inline 跑两遍）+ `reoptimize_relinked_footnote_no_dup`（模拟 v6 产物=注释已移同章末尾+同章锚点 marker，跑 v8 不翻倍）。⚠ 只对**下次重传**生效；设备上存量老书需重传才升级。（另一候选因：xochitl「文字与布局」全局排版是否覆盖书内 `text-indent`——待真机核，证据倾向 xochitl 认书 CSS〔已知它认 background/inline-img〕，置信中。）
- **传书目标合并为文件夹选择（bug 4，用户拍板）**：原 `原生阅读/原生批注` 二选一对 PDF 只差落地文件夹（优化/体检对 PDF 都是空操作，Check 有 `content_type!=Epub` 守卫，安全）。UI 改为单一「传书到 xochitl」+ 文件夹预设（书库/批注/自定义），统一走 `target=native`。服务端零改（annot target 保留供 CLI）。
- **客户端格式预拦（bug 6）**：`uploader()` 加第 4 参 `okExt`，选中即按扩展名判、不合格标红不上传（书 `.epub/.pdf`、字体 `.ttf/.otf/.ttc`、词典 StarDict 全套、壁纸 `.jpg/.png`；KOReader 书库任意格式不拦）。drop `<input>` 也加 `accept`。
- **文案去生硬（bug 3）+ PDF 端上不重排讲清（bug 2，非 bug=设计如此）**：删「固定版式 PDF 上手写定稿」等黑话；档位标「PDF 不受影响永远原样投」；加提示"PDF 想重排请电脑 `shelf push`"。
- **host `shelf push` 进阶指引 + KOReader 安装/配置指引进网页（bug 5 / 非 bug 2）**：「管理」页加 `shelf push` 卡（强在哪/怎么装/常用命令）；KOReader 页加安装+profile 已调优说明。
- **端/host 一致性（非 bug 1，答用户）**：一致——两条路末步都调同一 `epub-optimize`(=`bookconv::optimize`)，host 只多一层 Calibre 级 CSS 拍平 + 非 EPUB/PDF 转码。

**★首行缩进根因终结（2026-09-04 真机 7 版诊断书 + 《飘》活样本，OPTIMIZE_VERSION→10）**：bug 1「中英文没缩进」真机死磕出 xochitl 的 CSS 行为，纠正了此前一路的错判：
- **xochitl 只认外链 `.css` 文件里的规则，完全无视内联 `<style>` 块和元素 `style=` 属性**。⟹ **v6–v9 我们注入的内联 `<style class="cj-wash">` 排版规则（边距归零/首行缩进）在 xochitl 上从来没生效过**！之前以为有效的只是「删改源文件」类改动（剥字体锁/去背景图）。活样本：《飘》自带外链 `p{text-indent:2em}` 真机显示缩进，而我们所有内联注入都不显示。
- xochitl 的 **css 解析器很脆**：**① 不认 `!important`**（带 `!important` 的外链规则整条失效，v10 首版真机「都没缩进」，去掉即生效）；**② 不认类/相邻/at-rule 选择器**（外链 css 里混一条 `.big{}` 就让整表失效，diag5 坐实）——**只吃裸元素选择器 `p{}`**。
- 中途错误探索（记录以免重走）：先误判「xochitl 无视 text-indent」→ 试段首 `&nbsp;`（v9）——但 nbsp 宽度随字体变、且**连续 nbsp 被折叠**成一段，做不到精确 2 字；全角空格 U+3000 / em 空格 U+2003 被 xochitl **吞掉**（前置空白折叠）；inline-block 占位被无视。**全是死路**，已撤回。
- **根治（v10）**：优化器把排版规则写成**外链 `cangjie-wash.css`**（`p{text-indent:2em;margin-top:0;…}` 裸选择器、无 `!important`）+ 每章 `<head>` 注 `<link>`（相对路径 `relative_to` 算）+ OPF manifest 补 `<item>`。中文 2em / 拉丁 1.2em。**em 单位 = 字体无关、精确 2 字**。`wash_entries::add_wash_css_entry` 幂等（重洗不重复加）。真机《缩进v10无important验证》3 段精确 2 字缩进坐实。**方法论**：一个真机活样本（《飘》）＞ 七版凭空诊断——早点解剖有效样本能少走很多弯路。

## 03r｜读书线重构：中间层（母版库）三层架构（2026-09-05，用户"逻辑很乱"提出改造）

**问题**：读书线让用户一次背 3 维决策（读器×投递口×文件夹），"优化"与"落库"耦合；根因是把机器判不了的"用途分类"（侦探小说/漫画是闲书还是研读）塞进流程。**方案**（计划全文见 plan 文件）：**三动作正交解耦**——入库 / 优化 / 落库；中间夹**母版库**（中间层暂存池）作交汇点。统一规则：**所有源一律原样直传母版库，"优化"是母版库独立动作，"落库"也独立**。四决策：母版库=可反复落库（一本落两读器对照）/ 指引=决策辅助（不替用户分类，讲清读器差异）/ 微读=内容源（非独立 app）/ 设备端优化=保留（够用默认，host 是高质量路）。

**Phase A 母版库基础设施（✅ 真机通 2026-09-05，USB 部署 192.168.1.21）**：`shelf-core::paths::staging_dir()`（`state_dir()/books/staging`，/home 分区不丢、不套 spool 的 LRU 淘汰）；book-serve spool 加 `stage_new/list_staging(StagingEntry 带 format+optimized，复用 optimized_version_file)/optimize/deliver/remove`；5 路由（GET/POST /staging、/staging/{optimize,deliver,delete}）；koreader-serve `POST /books/adopt` 从共享 staging 读→`KoStore::install` 纯复制。**关键设计：落库=纯复制原字节不优化**（优化只在母版库动作里），保证一本母版落两读器是**同字节可对照**（兑现"两器一致"）。真机端到端：上传→母版库(optimized:false)→优化(1400→1778,标记 false→true)→投 xochitl(visibleName 坐实)→adopt KOReader(1778 同字节)→**母版保留、两库并存**。

**Phase B 内容源汇入母版库（✅ 真机通 2026-09-05）**：① **网文获取下沉**——`readlater` 的抓取+Readability 抽取+scraper 白名单核心从 `reading/device-rs` 下沉进 `bookconv::article`（纯函数，device-rs re-export 保 `save_article` 上传半，移除 readability-rust/scraper/ego-tree 依赖）；book-serve `POST /api/books/staging/fetch-article {url}` → 组 EPUB 落母版库。真机：scraper/html5ever **aarch64-musl 交叉编译通过**；设备联网抓 runoob/阮一峰两篇成功落母版库（404 时正确返回错误=通路工作）。② **host push 改造**——`push.py` 统一"洗书→落母版库(`/api/books/staging`)"，**去掉 -t 目标/输出路径**（根治用户连踩的用法坑），`--to-pdf` 定稿 / `--no-optimize` 原样（`--direct` 直投逃生随后按用户"规则一致"去掉，见 Phase C）；去 decide_route/quality/koreader直投。host pytest 全绿(24)。⚠**技术债**：网文/转换类产物走 `assemble_optimized` 会打优化标记→母版库显示 `optimized:true`，但其实没跑 wash 层（外链 css 缩进/脚注），母版库语义下会让用户以为已优化而跳过；待澄清（网文也过 wash，或标记语义分层）。**Phase C UI 三层重构（✅ 功能真机通 2026-09-05：上传→洗书→落原生→落 KOReader 全链用户走通；结构按用户反馈二次定稿）**：首版把母版库列表复用进 xochitl/KOReader 三个 tab、读器页仍留传书区，用户指出"页面逻辑混乱、书从哪进有三个答案"。**定稿（用户拍板）**：① **「传书」固定 tab 放第一位＝唯一总入口**，二级 tab **入库**（上传任意格式 / 抓网文 / 微信读书〔占位，Phase D〕/ 电脑 shelf push）｜**母版库**（落库设置行：投原生文件夹 / KOReader 目录 / 优化档位 / 投完清除；搜索＋格式/状态筛选；最新入库在前；**按格式门控按钮**：EPUB→优化/投原生/加入 KO，PDF→投原生/加入 KO，其它→只能加入 KO；「加入 KOReader」按是否已装门控）。② **读器页彻底不传书**：xochitl 只剩原生字体（去阅读增强状态、失败队列），KOReader 只剩字体＋词典（去书库浏览——"有什么去 KOReader 里看"）。**网页直传取消，电脑 `shelf push --direct` 也去掉**（用户："规则一致，所有书只允许落母版库"）——没有任何绕过母版库直投读器的路径，去向只在网页选。③ **母版库改收任意格式**（推翻 Phase A"只收 EPUB/PDF"——那是 xochitl 的约束，不该限制 KOReader）；投原生时对非 EPUB/PDF 明确拒绝并指引。④ 指引＝决策辅助（三步走 + 两读器各擅长 + "拿不准先投一个母版还在"，不问闲书/研读）；微读独立 tab 删除，归入库内容源。设计：`stagingList` 单一渲染函数按格式/安装状态门控，不再三处复用。**观感（措辞/手机排版）待用户浏览器核**。

**规则漏洞两处（2026-09-05，用户问"逻辑是否清晰"时自查出，先于新功能修）**：① **inbox 追平绕过母版库**——`process_inbox` 老逻辑 scp 进 `inbox/` 就自动优化直投 native，违反"所有书只落母版库"；改为原样 `stage_new` 落母版库（任意格式），老 `POST /?target=` 直投接口标废。② **"已优化"徽章说谎**——网文 / 格式转换产物走 `assemble_optimized`（默认 `optimize_epub`，无 wash）却写同一标记，母版库显示已优化并隐藏「优化」按钮，其实没洗缩进/脚注。改 `marker_value(full)`：含 wash 写版本号本身，无 wash 写 `<版本>-core`；`is_current_version` 只认 full；`StagingEntry.level` = full / core / old / none，UI 徽章「已优化 / 已优化·未清洗 / 旧版优化 / 未优化」，非 full 仍给「优化」按钮。weread 线只看 `is_optimized`（有无标记），不受影响。**真机通（2026-09-05）**：scp 进 inbox/ → 8s 防抖后日志「已入母版库」、xochitl 无该书；抓网文 → 母版库 `level:core`、点优化 → `full`。⚠ 验证时踩了自己的坑：watcher 是 8 秒防抖，5 秒就去查以为没触发。

**第二步"能用→好用"小迭代（✅ 真机通 2026-09-05）**：① **落库记录** sidecar `.<书>.delivered {native,koreader}`——deliver 自动记 native，KOReader adopt 由前端调 `POST /staging/mark` 记（各服务只写自己目录）；UI 徽章「已投原生 / 已加入KO」，落库时间早于母版 mtime（之后又优化过）标「·旧」提示可重投；删书连带删 sidecar。② **「清理已落库」**批量删已投过的母版。③ KOReader 目录改 **datalist 下拉**（候选=现有目录）。④ `GET /staging` 带 **freeBytes**（`df -k` 解析——busybox 设备名过长会把数字换行，需拍平表头后所有行再取第 4 token，真机 `/dev/mapper/home-encrypted-disk` 坐实），<300MB 红字告警。⑤ 同名重复入库回执「已有同名，存为 1_x」。⑥ CLI `push` 成功后打印网页去向提示；加 `--keep-spacing`（透传 `WASH_KEEP_PARA_SPACING=1`，与网页档位对齐）。**Phase E ① PDF 结构化重排——真机《财新周刊》"整页大片空白"根因与修（2026-09-05）**：解剖产物（不猜）定位三根因：**每页一章**（91 页→91 xhtml，读器每章末强制翻页，一页 PDF 重排后占 1.3 屏就留 70% 空白）；**段落碎成行**（PyMuPDF 对杂志每行一个块，"块=段"不成立，每行独立缩进）；**图近整屏高**推到下页留白。修：整本平铺成单元→文档级段落合并（同列小间距续接；新段信号＝相对上一行起点缩进≥1 字 / 短行＋句末标点；换列换页按"上段未完"续接）→按标题分章（字号≥1.35×正文且在列首/页顶，排除正文中大字引语；无标题每 12 页一章；<200 字迷你章并入下章）→图封顶宽≤954/高≤60% 屏。**两个二次坑**（dump 单页逐行数据才看出）：分栏按"块中点在中线左/右"会把单栏页的宽块甩到"右栏"排最后、阅读序全乱→改为只在存在贯穿页面竖向空白带时分栏；行尾零宽空格 `​` 挡住句末标点判断→全局剥零宽字符。量化：章 91→10、段落 1209→770、句中断开 48%→12%、中位段长 47→109 字。**第三坑**：修分栏后发现旧中点分栏在双栏页只收文字块、**图块被静默丢掉**（8 张→39 张才是全量），而 954 宽照片存 PNG 一张 1.3MB → 21MB；改图块存 JPEG q80（回退 PNG）→ 3.6MB。**✅ 真机通（用户核 v3："空白没了，段落正常"）**。已知小瑕疵待后续：目录混入 3 个"文｜某某"署名当标题；"句中断开 12%"里含图注/列表项，真实断段更低未细分。Phase D–E 其余（微读内容源 / 英文排版·两器对照·KOReader 脚注观感）待用户核。

## 03s｜代码质量核查二轮：母版库领域化 · 直投路删除 · 上传模板统一（2026-09-05，用户"审视 shelf/ 合理用设计模式、去重、解耦"）

**动机**：§03r 中间层落地后代码里留着两套并行的"收书"路（旧 `POST /?target=` Strategy+Pipeline 直投 vs 新 `/staging*`），三份手搓 multipart 循环（book-serve 两份 + shelf-core `AssetUploadFlow`），格式白名单在网页 JS / book-serve / font-serve / koreader-serve 四处各写一份，服务 main 里 `let (k1..k10) = (k.clone()…)` 的克隆串，"缺 name" 取参样板 ×7，单段文件名校验 ×4。全部按"单一事实源 + 模板方法 + 门面"收。

**做了什么（按层）**：
- **shelf-core**：① 新 `formats`（`BOOK_EXTS`/`FONT_EXTS`/`DICT_EXTS`/`IMAGE_EXTS` + `has_ext`/`ext_of`/`dotted`）——网页 accept、各服务上传门、inbox 追平、KOReader 列表全从这一份派生；② `fs::plain_name`（单段文件名校验，拒 `/`、`..`、`.` 开头）+ `unique_path`/`move_unique`（同名不覆盖 / 跨设备回退拷贝）收编 spool·壁纸·koreader 各自的 `safe`/`unique`/`archive`；③ `http`：`bind(&state, |st, r| …)` 替代克隆串、`Router::any(&[GET,POST,…], pat, f)` 一个处理函数挂多方法（网关代理 6 条路由→2 条）、`Request::json() → JsonBody{str/str_or/bool_or}` 取参门面、`multipart_boundary()`/`q_flag()`；路由匹配拆成 `Pattern`，405 判定不再每请求克隆 Route；④ `asset`：`AssetUploadFlow::in_dir(dir)`（母版库暂存在 spool `.work/` 与目标同分区、入库 rename 零拷贝）、trait 缺省方法 `reject_message`/`success_message`（拒收/成功文案归仓库定，流程不再写死"已安装"）、`receipt(items, extra)` 统一 `{ok, items, …}` 回执；成功项 `name` 用落地名。`percent_encode` 从网关 proxy 挪进 `multipart` 与 `percent_decode` 成对。
- **book-serve**：删 `target.rs`/`pipeline.rs`（Strategy/Pipeline 是为"上传→转换→优化→检查→注入"直投设计的，规则统一后没有调用方；设备端 AZW3/MOBI/FB2/CBZ 转换随之不再从 book-serve 可达——网页早已写明"其它格式只能加入 KOReader，想进原生用电脑 `shelf push` 转 EPUB"，bookconv 的转换器本体保留给 reading 线）。母版库从 `spool.rs` 拆成独立领域模块 **`staging.rs`**（`Staging`：入库 `stage_new`/`stage_from_path`/`fetch_article`、优化 `optimize(name, mode)`、落库 `deliver(name, folder, keep)`、`mark_delivered`、`list`/`remove`/`free_bytes`；`StagingStore` 是它的 `AssetStore` 适配；`OptimizeMode` 三档随之搬来）。`api.rs` 回到"只取参 + 调领域方法 + 回执"的适配层（每条路由 1–3 行）。`spool.rs` 只剩 inbox 队列（`done/` 与 100MB LRU 一并删：成功的书进母版库，不再另存）。`BookConfig` 退役 `optimizeDirectEpub`/`comicMono`（旧 json 里多余键被忽略，有测试）。`/targets` 路由与 status 里的 `targets` 删。
- **koreader-serve**：删直传 `POST /books` 与 `KoConfig.optimizeEpub`/`KoStore::optimizing`（书只从母版库 adopt，落库＝纯复制）；不再依赖 bookconv；`ConfigSync` 改持 `Arc<KoReader>`（原先 get/post 各构造一份）；`require_installed`/`running_note`/`plain_name` 收三处重复；`list_dicts`/`list_files` 归 `koreader.rs`。
- **font-serve / wallpaper-serve**：`State{store, paths}` + `bind`；壁纸上传"显式激活 / 首张自动激活"两段重复合并为 `want = activate || 无当前`；`GET /{name}` 走 `store.read`（`plain_name` 校验）；`FONT_EXT`/`&["jpg","jpeg","png"]` 改引 `formats`。font-serve 的注册 tab 改名「xochitl」（order 10），book-serve 不再挂 tab——网页 xochitl 字体页由 font-serve 存活与否驱动（原先键在 book-serve 上：book-serve 关了字体页就没了，font-serve 关了页面还在但全 404）。
- **网关**：`ui::page()` 用 `OnceLock` 把 `formats` 四张白名单注入模板 `__EXTS__`（网页 `BOOK_EXT` 等不再手抄）；`upHtml()` 生成上传区 + `uploader()` 认 `.up` 容器（KOReader 页原先用 `wrap(i)` 假 querySelector 拆两个上传器的 hack 删）；`fillList`/`delBtn`/`cjkBadge` 三个小件收编 xochitl 字体 / KOReader 字体 / 词典 / 壁纸四处列表渲染；`assetTab` 改成带 `row`/`header`/`onRender` 钩子的模板，xochitl 字体页与壁纸页都走它。`main.rs` 去掉 `service_of` 二传手，`proxy::forward` 自己查目录表。`auth::identify` 直接用 `GuardRequest::header`。
- **host CLI**：`receipts.item_name` 兼认 `name`/`file`，`upload_each(transport, api, files, query, extra, on_response)` 收编 font/wallpaper/koreader font/push 四处"守卫→逐文件 POST→回执"循环；`status` 的 URL 段映射改从网关 `/api/manage` 取（单一事实源 `manage::MODULES`）；`config.toml` 退役 `default_target`/`quality`；`passwd`/`doctor`/`__main__` 去掉 `__import__("sys")`、函数内 import 与过期文案。

**没动的**：bookconv 内容层（只把 `optimized_version`/`_file` 合成一个 `marker_in<R: Read+Seek>`、`is_html` 改引 `wash`），`htmlproc`/`wash` 的规则本体不碰；`koreader/merge.lua`、systemd 单元、install/uninstall 不碰。

**验证**：离线 `cargo test --workspace`（bookconv 104 / shelf-core 37 / book-serve 10 / koreader 4 / font 4 / wallpaper 4 / gateway 6）+ host `pytest` 25 全绿，`node --check` 内嵌 JS 通过，workspace 零警告。
**真机通（2026-09-05 13:41，USB 10.11.99.1，备份 `shelf-20260905-134052`，SSH 隧道打 loopback）**：注册表 font-serve 挂 tab「xochitl」、book-serve 无 tab ✓；`POST /staging` 一次传 jpg+epub → jpg 拒收（文案含整份白名单）、epub 落地回执 `name`=落地名 ✓；`/staging/optimize keep-spacing` 回执带统计「清洗+优化，2 章，653→1630 字节，自动目录 1 条」、列表等级 none→full ✓；`/staging/deliver` 非 EPUB/PDF 门 ✓；`/staging/mark weread` 拒 ✓；`/books/adopt` 进 KOReader 子目录（1630 字节同字节）+ `mark koreader` 后列表 `delivered.koreader` ✓；font-serve 拒 jpg（fallback 链照常）✓；旧 `POST /?target=` 404、`POST /koreader/books` 405 ✓；`.work/` 空（上传暂存已清）。测试书已从母版库与 KOReader 删除。
**格式提示分档（2026-09-05，用户"入库页格式列太多了，只列实际支持的"）**：核实结论——18 个扩展名全部是设备装的 KOReader v2026.07.1 `documentregistry` 真注册的（crengine：txt/html/rtf/doc/docx/chm/mobi/azw/prc/fb2；mupdf：pdf/cbz/cbr/xps，`libarchive.so.13` 带 `rar`/`rar5` 所以 CBR 真能开；djvu 引擎：djvu），白名单没有多收。问题在**展示**：一口气列 18 个看不出谁能去哪。改 `formats` 分三档（`NATIVE_EXTS` epub/pdf · `HOST_CONVERTIBLE_EXTS` azw3/mobi/azw/prc/fb2 = host `WASH_EXT` · `KOREADER_ONLY_EXTS` 其余 11 个），`BOOK_EXTS` 仍是三档之并（测试钉死），网页拖放框只写「EPUB / PDF 及下列格式」，下方与 GUIDE「格式」条改为三档一句话说明（`FMT_TIERS`），KOReader 页改"母版库收的所有格式 KOReader 都能读"。

## 03t｜漫画通道：AZW3 漫画 → CBZ + 固定版式 PDF（2026-09-05，用户"AZW3 漫画该怎么转 / 镖人为何投不了原生"）

**根因**：《镖人》AZW3 被当文字书走 Calibre 洗书路 → 282MB EPUB（2637 文件 / 2473 图）→ 点「投入原生书库」两次，xochitl 日志
`HttpRequest: expected multipart body is too large`（13:45:50 / 13:45:59）——撞的是 xochitl `/upload` 体积上限（此前测得 60～285MB 间 413），
不是我们的格式门。就算传上，漫画进 xochitl EPUB 重排引擎也不对（图按列宽缩、无固定页）；正确载体是**固定版式 PDF**（每页一张 954×1696）。
架构缺口：母版库三层下**没有"漫画 → 原生"的路**（`push` 见 `.azw3` 一律洗成 EPUB；`.cbz` 原样只能进 KOReader；设备端 CBZ→PDF 转换器已无调用方）。

**做法（用户定：默认原图，不抖动）**：
- **host 探针 `shelf_cli/comic.py`**（不调 Calibre，毫秒级）：PalmDB 容器数 JPEG/PNG 魔数记录，图片字节 ≥60% 且 ≥20 张判漫画；EPUB 按 spine 统计
  `<img>` 数与可见文字数，图 ≥20 且每图配字 <40 判漫画；CBZ 天然。真书：《镖人》AZW3 2498 图占 99.96% ✓，Calibre 洗过的 EPUB 2473 图 / 5527 字 ✓
  （首版"整页只有一张图的页占比"判据在洗过的 EPUB 上失败——Calibre 把十几张图塞进一个 xhtml，146 页 spine 只 24 页单图——改成图/字比后两种都命中）。
  `--comic / --no-comic` 覆盖。
- **`shelf push` 路线规划 `plan()`**：`--no-optimize`→raw；漫画且（CBZ 或有 Calibre）→comic；有 Calibre→wash；否则 raw。comic 路产**两份**入母版库：
  CBZ（`comic2cbz.py` 按 spine 抽整页图；给 KOReader 漫画模式）+ PDF（新 bookconv bin **`cbz2pdf`**，与设备端同一 `convert::cbz::cbz_to_pdf`：
  每页按屏降采样，缺省原图 JPEG 直嵌，`--mono` 黑白页 1-bit 抖动），PDF >60MB 走既有 `pdfsplit` 分卷。
- **设备端母版库加「转 PDF」**：`POST /staging/to-pdf {name, mono}`，CBZ → `<stem>.pdf` 新条目、CBZ 保留；网页 CBZ 行出「转 PDF」按钮 + 「漫画转 PDF 用 1-bit 抖动」开关；
  列表 `format` 加 `cbz`（筛选归「其它」）。真机：3 页测试 CBZ → `mini.pdf` 3473 字节 ✓。大套系走电脑（设备 CPU 慢约 10×）。
- **`imgopt` 提速**：`downscale_for_device`/`downscale_into` 原先为取尺寸把每页 JPEG 完整解码（甚至两次），改只读头取尺寸、达标页零解码。
  《镖人》2473 页原图路 host 2m36s → 1m49s（产物字节完全相同）；剩下的 44ms/页在 JPEG 头解析 + 写 297MB 产物，未再深挖。

**《镖人》实测（host）**：`comic2cbz` 0.8s → CBZ 296MB（页 927×1327 RGB JPEG，均 120KB，已 ≤ 屏、直嵌）；`cbz2pdf` 原图 297MB；`--mono` 223MB（4m31s）。
mono 只缩到 3/4 而非"1/8"：色度采样 99 页只 1 页 ≥0.06（彩封），几乎全书都转了 1-bit——**体积没降是 Floyd–Steinberg 抖动后的位图是高熵噪点，
Flate 压不动**（927×1327 1-bit 裸 154KB，压后仍 ~90KB，只比 120KB 的 JPEG 小 1/4）；代码里"~1/8"是相对 8-bit 灰 Flate 说的，对 JPEG 源不成立。
原图档不受影响（用户定默认原图）；mono 要真省体积得换 CCITT G4/JBIG2 编码，留后续。

**收口：漫画不投原生（2026-09-05，用户定）**：投原生的整条——`cbz2pdf` bin、`calibre_bridge.cbz2pdf`、`push --mono` 与 PDF 产物、
book-serve `Staging::to_pdf` / `POST /staging/to-pdf`、网页「转 PDF」按钮与抖动开关——全部删除。漫画通道现在只做：AZW3/EPUB 漫画 → `comic2cbz.py` → CBZ 入母版库 →
「加入 KOReader」；CBZ 输入按 `plan()` 归 raw 原样入库（终态，不需 Calibre）。理由：xochitl 没有固定页漫画体验、整本几百 MB 撞 `/upload` 上限、分卷读漫画体验割裂；
KOReader 漫画模式读 CBZ 本就是最优解。`imgopt` 只读头取尺寸的提速保留（EPUB 内嵌图同样受益）；`convert::cbz` 本体留给 reading 线。
我上传的 8 卷分卷 PDF 已经 API 删除；用户推的两本整本 PDF 留在母版库由用户处置。

**核查一档收口（2026-09-05，用户"核查还有哪些要更新"后做 1–6 + 12）**：① book-serve `status` 去掉早已无人展示的 `readingQol`；② host `config.toml` 退役无调用方的 `ssh`（配置同步走 HTTP），`doctor` 依赖表改为 ebook-convert / k2pdfopt；
③ **投原生体积门**：`BookConfig.nativeUploadLimitMb`（缺省 150，0=不拦；真机 188MB 被拒、60MB 稳）→ `Staging::deliver` 超限不发、回执指引"PDF 在电脑 shelf push 重推自动分卷 / EPUB 用 KOReader"，
`status` 带 `nativeUploadLimitBytes` 供网页灰掉超限书的「投入原生书库」（⑫）；④ 管理页 `shelf push` 卡改正"TXT 网页不收/网页直传"过时说法、补漫画例子；⑤ `manage::MODULES` 标签
book→「母版库 / 原生投递」、weread→「微信读书（内容源，待接）」；⑥ `book-serve.service` Description 同步。真机：`status.nativeUploadLimitBytes`=150MB；对母版库里 283MB《镖人.pdf》点投原生 → 门拦回执、xochitl 日志零新增。
**核查二档（2026-09-05，11/13/14/16）**：删无调用方的 `host/calibre/einkify_epub.py`（同等能力在 `bookconv::imgopt`，`reading/tools/calibre/README` 同步）；网页入库页与 GUIDE 把"微信读书"标为待接；"投递"用词统一为入库/落库（模块标签「母版库 / 落原生」、`book-serve.service`、bookconv/check 注释、`inbox` HELP）；§03i 对标表加"历史形态、现行见 §00b"提示。第 15 条（config 测试的 `quality` 键）已随一档 ② 改成中性键名。注释级 7–10（`wash_epub.sh` 末步说明、`comic2cbz.py`、`convert/mod.rs` 头注与 `EinkTone`）随后一并改为现状口径。
**⚠ 文档事故**：上一轮文档脚本用 `**未闭环**：`/`**已闭环（真机）**` 做切片锚点，前者在 §00b 与 §05 各出现一次，切掉了 §00b 尾～§05 整段并随 `b5fde60` 提交；从 `898f2fd` 恢复全文后重放改动。教训：切片锚点必须先 `count()==1`。
## 03u｜砍微读线（2026-09-05，用户"不做了，直接砍微读线"；范围只限书架）

**评估后砍**。Phase D 探索（两个只读 agent + 真机核）得到的事实：① 「微读 Skill」= 微信读书官方 Skills/Agent 网关（Bearer api_key，`/_list` 自描述 17 接口）**只读没正文**——书架/目录/搜书能拿，章节正文只能走 web 端点 + cookie `wr_skey`（~90 分钟过期靠 renewal 续）+ 分片 `codec`（经授权照 MiuRead 移植的 AGPL 派生，个人自用不分发）；② 现有下书栈在 `reading/device-rs`（login/qr/renew/agent/fetch/sign/obfuscate/codec/pipeline ≈1000 行，纯网络模块零 device-core 依赖，只有 pipeline 尾巴与 wr_serve 是设备耦合），按书架原则要 `git mv` 成 `shelf/crates/weread-core`（crate 级 AGPL-3.0）+ 旧 crate re-export，再立 `weread-serve` 8794（后台任务 + 进度轮询 + 55 分钟续期 + 封面代理）+ 网页扫码状态机；③ 设备与 host 当前都没有任何微读凭证（重置机），要重新扫码；④ 协议脆弱是已知常量（上游一改即断，"200 空 {}"曾卡数日）。投入约两天、收益一条依赖第三方私有协议的内容源，用户决定不做。

**砍掉的**：管理页目录表 weread 行（`installable` 机制保留）、入库页「微信读书 · 即将接入」占位与三处「待接」文案、`services/weread-serve/` 槽位 README、`weread-web/`（内嵌浏览器 spike 脚本，从未执行）。**不动的**：`reading/` 的 wr-* 下书栈、`/home/root/weread` 与 `~/.config/weread-client` 冻结路径（块③阅读旧线，与书架无关）；`bookconv` 里为微读书形态做的脚注/远程图规则保留（第三方书同样受益）。书架内容源定为四条：网页上传 / 抓网文 / 电脑 `shelf push` / scp inbox。

## 03v｜固件升级 3.27.3.0 → 3.28.0.172 实录（2026-09-05，真机）

**为什么记**：这是书架落地后第一次跨固件 OTA，把"哪些会被冲、哪些活着、怎么不循环死机"钉成事实，下次升级照抄。

**升级机制（真机核）**：OTA 走 Memfault（`memfaultd` swupdate 特性，`/tmp/remarkable-production-image-3.28.0.172-chiappa-public.swu`），写进**备用槽** root_a（`mmcblk0p2`），30 秒 SUCCESS，等重启切槽；当前槽 root_b 的 3.27 完整保留（`/proc/cmdline root=` 看在哪个槽）。3.28.0.172 的内部版本串是 `3.28.1.1666`（`strings /usr/bin/xochitl`），os-release `VERSION=5.8.203`。

**冲掉 / 活着**：新槽 `/usr` 是全新 rootfs——书架 6 个 systemd 单元、壁纸 bind 单元 + sleep 钩子全没；`/home` 原样（母版库、KOReader、xovi、`~/.local/bin` 二进制、配置、证书都在）；`/etc` overlay 照旧 tmpfs。dm-verity **未激活**（`dmsetup ls --target verity` 空），`mount -o remount,rw /` 可写，install.sh 的 verity 门照常放行。

**appload 要不要停**：appload.so **没有 `_xovi_shouldLoad` 自检**，不会"自己停"；但 xovi 的 LD_PRELOAD 只在 `/etc` tmpfs 里（`xovi/start` 现场写），重启即清，所以**首次进 3.28 是纯原厂 xochitl，任何扩展都不载入**——循环死机只可能发生在之后手动 `xovi/start` 时。且即便循环，StartLimitAction 整机重启后 `/etc` 又清空、自愈，**最坏是一次被迫重启，不会砖**。实际操作：重启前把 `appload.so` + 两份 qmd（KOReader 侧栏入口、字体菜单 3.27 锚点）挪到 `/home/root/xovi-disabled/pre-3.28-<时间>/`（extensions.d 外；md5 清单 + README 同放），`exthome/appload/koreader/` 数据不动。

**进 3.28 后顺序（实测约 8 分钟）**：① `xovi/rebuild_hashtable`（设备旁输密码，hashtab 20231 条重建）→ ② `xovi/start` 只带 qt-resource-rebuilder（`[qmldiff]: Set system version to 3.28.0.172`，xochitl NRestarts=0）→ ③ host `SHELF_NO_BUILD=1 ./deploy.sh 10.11.99.1`：五服务 active、`uploadReachable=true`、壁纸 bind 1 处、HTTPS 401 正常；install.sh 按 IMG_VERSION 选了 3.28 锚点 `font-menu-dynamic.qmd` 放进 qrr 目录（**未激活**，下次 `xovi/start` 才生效，独立一步验证）→ ④ `ssh root@10.11.99.1 sh -s < packaging/chrony-cn.sh` 恢复国内 NTP（§03w，OTA 冲 rootfs 后 chrony 会回到 Google 服务器）。**appload 不挪回**：0.5.3 的 qmd 钩 3.28 已删的 `SidebarFilterItem`（上游 PR #59 只改 qmd、编进 .so，重编需 rM Qt6 SDK），KOReader 暂无侧栏入口。

**可升级性的准确说法（2026-09-05 用户问"是否 99% 可用"）**：*升级零风险、数据零丢失、随时可升；升完要手工四步装回，不是"升了就能用"*。风险按层分、不合成百分比：书架五服务/壁纸/WiFi/chrony/休眠屏键只用 `/upload` 接口和系统标准件，重装即回（本次 100%）；qmldiff 注入（字体菜单）依赖 xochitl QML，大版本常要重适配（3.27→3.28 已两版 qmd）；中文输入法 langhook（本机未装）靠二进制特征码，每版真机验、有 fail-safe；KOReader 本体独立无碍，侧栏入口靠第三方 appload、3.28 目前挂着。

**顺带核实**：`/home/root/.bashrc` 没有登录触发的 xovi 恢复钩子（记忆里的 A2 登录恢复在这台重置机上**没装**）；`root` 的 shell 是 `/usr/sbin/rmdevlogin`（`exec -a -sh /bin/sh --login`）。vellum 的 `post-os-upgrade` 钩子只打印"先跑 rebuild_hashtable"，不自动做。装 KOReader 的字体/书目录 `exthome/appload/koreader/` 与 appload 是否加载无关，koreader-serve 照常工作。

## 03w｜原生自定义休眠屏 `SleepScreenPath` + WiFi 60 秒掉链根因（2026-09-05，3.28 真机）

**原生休眠屏（隐藏键，真机通）**：xochitl.conf `[General]` 加 `SleepScreenPath=<png 绝对路径>`（无需 `file://`，QML 自己补）。3.28 `SleepScreenView.qml` 解出来的机制：`logo.source = isettings.sleepScreenPath`（默认 `/usr/share/remarkable/suspended.png`）；`isCustomSleepScreenPath` 为真时 **logo 铺满整屏 PreserveAspectFit、插画卡自动隐藏**（`illustration.visible = showCarousel && !isCustom`）；图加载失败显示占位文字 "reMarkable is sleeping"。`ShowSleepScreenCarousel` 不是隐藏功能——3.28 设置里就是「休眠屏幕插图」开关（`SettingsSleepScreenToggle.qml`）。真机：停 xochitl → sed 写键 → `xovi/start`，键不被 xochitl 抹掉；用户确认休眠屏显示指定的池图（954×1696 满屏）。现指向 `~/.local/share/shelf/wallpapers/current.png`（wallpaper-serve 唤醒轮换原地覆盖的那个文件）；**用户两次休眠对照确认轮换生效**（"休眠图生效"，2026-09-05 17:10）——xochitl 每次休眠按路径重读，不吃 Qt Image 缓存。结论：bind-mount suspended.png + 三张透明插画卡 + `/usr` 写入 + sleep 钩子整套**可退役**，只留 wallpaper-serve 的唤醒轮换（原地覆盖 current.png）+ 安装器写 `SleepScreenPath`（§05 第 8 条，待做）。改键流程：`cp xochitl.conf` 备份（含 token，绝不打印）→ `systemctl stop xochitl` → `sed -i '/^\[General\]/a SleepScreenPath=…'` → `xovi/start`。

**WiFi "升级后连不上" 的真相：不是 3.28 回归，是 NXP IW612（`iw61x` MWLAN, SDIO）省电模式**。现象：连上 AP 后 **约 60 秒** `systemd-networkd: wlan0: Lost carrier` + `wpa_supplicant: REGDOM-CHANGE init=CORE type=WORLD`，**没有** wpa DISCONNECTED 事件、dmesg 无字，NetworkManager 仍标 `connected`、路由标 `dead`、永不自愈；`nmcli con up <SSID>` 立刻回来再掉。persistent journal 翻旧账：**3.27 那次 2 天 uptime 里 `Lost carrier` 95 次**，当天 15:29/16:21 也是连上 62 秒即掉——一直如此，只是以前主要走 USB 没察觉。`iw dev wlan0 set power_save off` 后 7 分钟以上零掉（对照：开着时 60/61/61/101/139 秒必掉）。置信度高。**根治（用户拍板，已写入）**：`nmcli con modify 我是猫 802-11-wireless.powersave 2`（按连接持久，NM 库在 `/var/lib/NetworkManager` bind 自 /home）；重新激活后 `iw dev wlan0 get power_save` = off，NM 每次激活都会关省电。代价 WiFi 开着时功耗略升（休眠时 WiFi 本就关）；xochitl 在设置里删掉重建该 WiFi 会丢此设置，换新 SSID 要再 modify 一次。

**时钟从未同步的根因与修法**：`chronyd` 4.5 配置的 4 个 `time{1-4}.google.com` 在国内不通（ping 100% 丢），启动以来没有一条 `Selected source`，`timedatectl` 一直 `synchronized: no`（RTC 本身准，差 1 秒，靠的是出厂/上次同步）。设备没有 `chronyc`、busybox 没有 `ntpd`。修法：改 **rootfs 底层** `/etc/chrony.conf`（服务器换成 `ntp.aliyun.com / ntp.tencent.com / cn.pool.ntp.org / time.cloudflare.com`），重启不丢（OTA 会冲）。**改底层 /etc 的姿势**：`/etc` 是 overlay（lower=/etc 本体，upper=/var/volatile tmpfs），必须 `mount -o remount,rw /` **先于** `mount --bind / /tmp/rootbind`（bind 继承挂载时的 ro 标志，后 remount 不传播），改 `/tmp/rootbind/etc/…`，umount，再 remount ro；**overlay 缓存 lower**：改完当场 `/etc/` 看到的仍是旧内容，重启才变——本次要立即生效就再 `cp` 一份进 overlay（落 upper tmpfs，重启自然消失、底层接管）。`mount -o remount,ro /` 偶发 `mount point is busy`，隔几秒再试就过（journal 在 /home 不占 rootfs）。改后重启 chronyd 8 秒 `Selected source ntp.tencent.com`，`synchronized: yes`。**已落地的兜底**：`packaging/xovi-post-start/wifi-reconnect.sh` → `/home/root/xovi/scripts/post-start/`（`xovi/start` 后台看护 60 秒，见 NO-CARRIER 就 `nmcli con up`，`journalctl -t xovi-wifi`），真机 `t+5s NO-CARRIER → 已激活` 通。

## 04｜踩坑

- **磁盘 metadata ≠ xochitl/UI 实际状态（2026-09-04 用户纠正）**：直接 `sed` 改 `.metadata` 的 `parent=trash` 并不等于"已进回收站"——xochitl 运行时在内存缓存、写回时覆盖，云同步也可能还原；出现过磁盘 8 个探针 `parent=trash` 但 UI 回收站只见真实书的错位。**涉及书库状态以设备 UI/xochitl 实际为准，不拿磁盘 metadata 当真相**；清测试文档走正常删除流程或停 xochitl 后操作，别边跑边改。
- **qmd 语法比 QML 窄，且解析错误=整份不应用、xochitl 不崩不报**（2026-09-05）：`({})`、裸 `if (` handler 都让 qmldiff 报 `expected item assignment value token`；只在 journal 有一行 `[qmldiff]: Error while processing file tree`，菜单静默缺项（用户"选不到已安装的字体"）。规矩：写/改 qmd 先 `git clone asivery/qmldiff && cargo build --release`，`qmldiff apply-diffs <root> <dest> x.qmd -f -c` 对 `extract_qml.py` 解出的真 QML 实跑（root 按资源路径摆），过了再上机；上机后 `journalctl -u xochitl | grep qmldiff` 必看。
- multipart 流式解析：`fill()` 用 `Vec::resize(+64KB)` 在逐字节到达的流上变成 memset 风暴（测试 50s）；改栈上临时块 `extend_from_slice` → 0.8s。
- **xochitl `/upload` 有体积上限**（实测 282MB EPUB 被 `multipart body is too large` 拒；此前 60～285MB 间见过 413）：大 PDF 靠 `pdfsplit` 60MB 分卷；EPUB 不能分卷——漫画别走 EPUB（§03t）。
- 会话里 shell cwd 会在 `cang-jie/` 与 `shelf/` 间漂移：`curl -F` 之类落地文件一律写绝对路径到 scratchpad，否则测试文件会混进仓库（2026-09-05 误提交两个临时文件后已删）。
- busybox：`head` 要 `-n`、无 `timeout`/`base64`/`od -A`、`ls` 中文名显示 `?`（验名 `find | hexdump -C`）。
- `shelf/build.sh` 必须在 `shelf/` 目录跑（仓库根没有 build.sh）。
- 本机冒烟别忘 `env -i`：host 桌面环境自带 `XDG_STATE_HOME/XDG_CONFIG_HOME` 会盖过 `HOME` 覆盖，把测试数据写进真实用户目录。
- pytest 要从仓库根跑（`uv run pytest shelf/host/tests`）。
- 多个测试文件对同一个 `http.server` Handler 类 monkeypatch，module fixture 共用服务器线程时 patch 链互相覆盖会递归死循环（pytest 挂死）。各文件用自己的 Handler **子类** + 自己的 fixture。

## 05｜真机待办（2026-09-05 刷新）

**未闭环**：
1. **Phase E ②③④**：英文书拉丁缩进（1.2em、标题后首段不缩进）观感；同一母版落 xochitl + KOReader 并排对照；KOReader 里内联脚注〔…〕能否接受（若不能，落库时对 KOReader 另跑 Anchor 是唯一备选，但会打破"两器同字节"）。
2. **退役 bind-mount 壁纸整套**（§03w 已证原生 `SleepScreenPath=current.png` 满屏且随唤醒轮换）：wallpaper-serve 去掉 `bind/unbind`/`mount.rs`/三张透明插画卡，删 `shelf-wallpaper-bind.service` + sleep 钩子，安装器改为写 xochitl.conf `SleepScreenPath`（要停 xochitl 写、备份 conf、不打印 token），卸载器还原键；轮换沿用 wake.rs。退役后第 4 条自然消失。
3. appload 3.28 适配：等上游发版或自 fork 重编（需 rM Qt6 SDK）；期间 KOReader 无侧栏入口。
4. 拔线真 suspend 下钩子 bind + 唤醒轮换只触发一次（bind 退役后不再相关）。
5. 3.28 字体菜单：用户在阅读器里选中书架字体后正文渲染效果确认；菜单再开时 `SHELF-FONT: visible` 差量追加是否触发（S-B）。
6. PDF 结构化重排小瑕疵：署名"文｜某某"混进目录；"句中断开 12%"含图注/列表未细分。
7. KOReader 里旧的 282MB《镖人.epub》由用户删（母版库里的漫画 PDF 用户已删，2026-09-05 15:04 后母版库为空）。

**OTA 后固定四步**（§03v）：`xovi/rebuild_hashtable` → `xovi/start` → `SHELF_NO_BUILD=1 sh deploy.sh 10.11.99.1` → `ssh root@10.11.99.1 sh -s < packaging/chrony-cn.sh`。/home 里的（母版库、KOReader、WiFi 钩子与 `powersave 2`、休眠屏 conf 键、qmd 文件）不用动。

**已闭环（真机）**：§03f 首轮五服务 · §03g/§03h 字体分开装/子目录/HTTPS · §03j 登录/CA/mDNS · §03k 字体两 bug · §03l 传书卡＝云同步 · §03m/§03n/§03o 网页改版/细节/管理台 · §03p 质量一轮 · §03q 优化做精 + 首行缩进 v10 · §03r 母版库 Phase A/B/C + 财新重排 · §03s 质量二轮 + 格式三档 · §03t 漫画通道（host 真书探针 → CBZ；漫画不投原生）+ 分卷静默失效修 + 投原生体积门 · §03v 固件 3.28 升级 + 3.28 字体菜单 qmd（首版整份不应用：qmldiff 解析不了 `({})` 与裸 `if (` handler，改 `[]`+`{ }` 后 `appended=4 count=8`，判官＝本机 asivery/qmldiff CLI）· §03w 原生休眠屏 `SleepScreenPath` + WiFi `powersave 2` 根治 + 看护钩子 + chrony 国内 NTP 脚本。

**已放弃**：**微读线整条**（§03u，2026-09-05：先是内嵌浏览器 spike 未推进，后内容源方案评估后用户砍掉）；设备端 AZW3/MOBI/FB2 → EPUB 转换（§03s，杂格式走电脑 Calibre，`bookconv::convert` 本体留给 reading 线）。
