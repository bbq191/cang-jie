# reMarkable 书架（shelf）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。读现状先看 §00b；待办/已闭环/已放弃看 §05；踩坑看 §04。
> **书籍优化引擎（`bookconv`）的深度细节**（清洗层 / 优化遍 / 脚注 / 图片 / 格式转换 / **★xochitl 渲染硬规则** / 版本演进）**已独立成 `bookconv优化白皮书.md`**；本文只记书架侧的决策/UI/真机轮次。

## 00｜定位与原则

2026-09-03 用户提出"全面重构，补齐短板增强优势"：① 阅读系统增加 KOReader 与微信读书；② 全面支持 AZW3/PDF/EPUB；
③ 字体与屏保图片上传即可用。澄清：先「有书读」（原生 / KOReader / 微读三条投递线）再「高质量读」；上传时手选目标；
设备网页 + host CLI 共用同一 API；微读 = 设备上直接开网页版在线读（门控）。〔后续变化：微读线 2026-09-05 砍掉（§03u）；"上传时手选目标"被母版库三层架构取代（§03r），去向在母版库里选；现状看 §00b。〕

四条硬原则（用户两轮驳回后定）：
1. **XDG 基目录规范**（Rust `shelf_core::paths` / shell / Python 三处同一张表，env 可注入测试）。
2. **设计模式去重解耦**：Repository + Template Method（资产上传 `AssetStore`/`AssetUploadFlow`，font/wallpaper/koreader/**母版库四家共用**，拒收/成功文案由仓库定）· 领域模块 + 纯适配层（book-serve `staging.rs` 领域 / `api.rs` 只取参回执）· 服务启动模板（`service::ServiceSpec`+`run`）· 配置读写模板（`config::load_or_default/seed/save`）· 端口/适配器（HTTP 只在 `http.rs`；`bind`/`Router::any`/`JsonBody` 取参门面）· Registry（服务发现）· Facade（网关代理，`manage::MODULES` 单一目录表）· Command（CLI）· 单一事实源（`formats` 格式白名单、`fs::plain_name`/`unique_path`）。共享原语：`fs::write_atomic`、`multipart::receive_part_to`、`asset::receipt/all_ok`（Rust）与 `receipts.upload_each/print_receipts`、`transport.delete_named`（host）。移动优先于复制；**旧 crate 只许剥离+re-export，不直接引用**。〔早期的 Strategy（投递目标）/ Pipeline（处理链）随直投路于 §03s 删除——规则统一后没有调用方，模式要适配需求而非反之。〕
3. **专项专用可插拔**：不做单体，按领域拆服务。
4. **不引用旧项目 crate、不对接旧路径**：能力只许剥离移植；不读写 `/home/root/weread/**`；旧 wr-serve 微读线原样兜底。

## 00b｜现状总览（2026-09-10 刷新，读本文其余历史节前先看这里）

**架构**：网关（`0.0.0.0:443`，2026-09-10 起绑标准端口不用带端口号，历史上是 `:8778`，见 §03am；HTTPS 私有 CA + 登录页密码 / CLI Basic + mDNS `shelf.local`；单页 UI 源码在 `services/shelf-gateway/ui/` 真文件，编译期 `include_str!`；用户可见标题/图标 2026-09-10 起是「秘密花园」🌿，非 `shelf.local` 这类内部命名，见 §03am）+ 四个 loopback 领域服务（book 8790 / koreader 8791 / font 8792 / wallpaper 8793）+ 运行时注册表驱动 tab（首层固定四段：传书/笔记/其他/管理，2026-09-10 重排，§03al）+ 事件总线（各服务 `GET /events` → 网关 `Hub` 汇聚 `GET /api/events`，网页零轮询、host `shelf events`，§03z）。设备固件 **3.28.0.172**（2026-09-05 从 3.27.3.0 升级，实录 §03v；appload 0.5.3 经 qmd 回填补丁在 3.28 复活），KOReader v2026.07.1。

![shelf 架构：网关 + 领域服务](diagrams/architecture.svg)

**读书线 = 三层 · 三动作正交**（§03r 定，§03s 收口）：内容源（网页上传 / 抓网文〔可选「同步优化」，§03ap〕 / host `shelf push`〔`--no-calibre` 可选跳过 Calibre 纯跑 `epub-optimize`，§03ao〕 / scp inbox；微读线已砍，§03u）→ **母版库** `~/.local/state/shelf/books/staging/`（原样入库，永久保留，不淘汰）→ 落库（人选：投 xochitl 只收 EPUB/PDF；加入 KOReader 收任意入库格式）。「优化」是母版库里对 EPUB 的独立动作（档位 auto / keep-spacing / plain，产物标记 full / core / old）；落库＝纯复制母版字节（投原生有体积门 `nativeUploadLimitMb`，缺省 150）。**投原生后自动渲染自检**（§03aa：xochitl 导入即渲染写 `pageCount`，与正文字符数期望比，<50% 判 warn，结果进边车 `.<书>.delivered.render` + `books/render` 事件 + 网页徽章）。**漫画默认只加入 KOReader，小体积时可选投原生**：AZW3/EPUB 漫画由 host `shelf push` 转 CBZ（缺省再过 16 灰省刷新档，含跨页拆分+白边裁切放大）入库；灰阶体积估算转 PDF 后仍在设备原生上传上限内，顺带生成一份 PDF 给「投入原生书库」选项，超限的仍只出 CBZ、绝不分卷（§03ad）。**所有书只落母版库，没有任何直投读器的路径**。

![shelf 数据流：三层·三动作正交](diagrams/data-flow.svg)

**格式三档**（`shelf_core::formats` 单一事实源）：原生 epub/pdf · 电脑可转 azw3/mobi/azw/prc/fb2/**txt**（TXT 由 host `txt_to_epub.py` 按「第X卷/章」切章建两级目录再洗，§03aa）· 仅 KOReader 其余 10 个。

**host CLI**（`shelf`）：`push`（Calibre 洗书 / PDF 结构化重排 / TXT 切章 / 漫画→CBZ→16 灰；`--wait` 设备睡了探 `/health` 等醒；`--no-eink-gray` 要原图；**`--no-calibre`**，2026-09-10 §03ao：EPUB 输入跳过 `ebook-convert`，只跑 `epub-optimize`，不用装 Calibre）· `doctor --render`（排版回归探针：投探针书→取回 xochitl 渲染缓存→pymupdf 量顶格/缩进→PASS/FAIL→探针自动进原生回收站，固件 OTA 后跑一次）· `events` · font/wallpaper/koreader/inbox/status/passwd。

**代码落点**：book-serve `staging.rs`（领域）/ `sidecar.rs`（落库记录边车）/ `render_check.rs`（渲染自检）/ `pending_queue.rs`（**新增**，2026-09-09 §03ag：`PendingQueue<T>` 持久化+入队去重+剔除的共用骨架）/ `trash.rs`（原生回收站队列，执行方是 `xovi/shelf-trash-agent.qmd`，现在包一层 `PendingQueue<Pending>`）/ `mkdir.rs`（原生建文件夹队列，执行方是 `xovi/shelf-mkdir-agent.qmd`，同样包 `PendingQueue<Pending>`；**2026-09-07 首个用途是 note-serve 生成《书名》笔记本文件夹，但 note-serve 2026-09-09 起已经改成复用书本自己的设备文件夹、不再调用这条链路了，当前消费方存疑，见 `mkdir.rs` 模块注释与 `notes/docs/reMarkable笔记白皮书.md` §03ae**）/ `spool.rs`（inbox 队列）/ `api.rs`（纯适配）；koreader-serve 只做"从母版库 adopt"+字体/词典/配置同步（不依赖 bookconv）；shelf-core `clock`（时间戳唯一出处）、`xochitl`（注入 + 找书/页数）、`fswatch`（常驻 + 限时）、`events`、`registry`（**2026-09-09 新增 `SvcClient`/`enc`**，§03ai：按注册表建带标准超时的 HTTP 客户端骨架 + `percent_encode` 薄封装，供跨服务调用消重复用——四个消费者全在 `notes/` 那条线，物理代码落在这里，见 §03ai）；`services/shelf-gateway/src/enhance/`（**新增目录**，2026-09-09 §03aj：`mod.rs` 路由 + `qol.rs`（`reading-qol.json` 读全量/patch/写全量）+ `battop.rs`（battop 状态探测+开关），刻意分文件不升独立 service——没有独立进程边界的理由）；host `calibre/epub_skel.py`（三处手搓 EPUB 骨架收编）、`calibre_bridge._run_json`、`transport._open`（§03ab）；`comic.py` 漫画探针；bookconv CLI `epub-optimize`（与设备同一函数）。

**已删（别再找）**：漫画 CBZ→PDF **分卷**投原生整条（`POST /staging/to-pdf`、`push --mono`，§03t 末，被否决方案）——⚠ `cbz2pdf` bin 本身 2026-09-08 已以新形态（体积门控、不分卷）复活，见 §03ad，别再当"已删"找不到；book-serve `POST /?target=native|annot` 直投路与 `target.rs`/`pipeline.rs`（Strategy/Pipeline）、`/targets`、`done/` LRU；koreader-serve 直传 `POST /books` 与 `optimizeEpub`；网页读器页的传书区与 KOReader 书库浏览；host `push -t/--direct/--quality`、config `default_target/quality`；`BookConfig.optimizeDirectEpub/comicMono`；壁纸 bind-mount 整套（§03x）与安装器里的旧壁纸/bind 迁移块（§03ab）；`paths::data_root`、`htmlproc::has_internal_anchor`、`optimize::is_current_version`（§03ab）。

**网页 tab（2026-09-10 §03al 重排为固定四段）**：传书（入库〔含电脑 `shelf push` 说明+命令，三个入库来源各自独立成卡：上传｜抓网文｜电脑端 shelf push〕｜母版库，固定第一）· **笔记**（由 `../notes` 的 note-serve 注册；「导入 md 文档」子标签默认隐藏，`notesImportMdEnabled` 开关控制，§03ak）· **其他**（新，xochitl(font-serve)/KOReader(koreader-serve)/壁纸(wallpaper-serve) 降一级包进来当二级子标签，只列当前真的装了的那几个）· 管理（固定；二级 tab：基石与模块｜模型管理｜系统增强｜实验室｜**电池刺客**〔第 5 个，`battop.running` 时才出现，§03am，下含耗电情况/唤醒源两个三级 tab〕——**没有独立顶层标签页**，§03ak 那版做过又撤了，§03al/§03am 是它之后的两次再调整）。总标题+图标 2026-09-10 起是「秘密花园」🌿（§03am），网关端口同一天改绑标准 443（不用带端口号，§03am）。

**笔记线（`notes/`）是独立仓库线，只是挂在书架同一网关上**（§03ac 记的是"书架这套可插拔机制接得住独立线"这件事本身，不是笔记线的设计）：书架侧改动仅 `manage::MODULES` 加几行、`build.sh/deploy.sh/install.sh` 顺带打包、目录表行与网页 `renderNotes`。笔记线自己的架构决策/真机记录/踩坑全部在 `notes/docs/reMarkable笔记白皮书.md`，本文不重复也不代管。

**设备杂项（§03v/§03w，全真机通）**：3.28 字体菜单 qmd 已通（qmldiff 语法坑，§04）；原生休眠屏 `SleepScreenPath=current.png` 满屏且随轮换，bind-mount 整套已退役（§03x，`shelf_core::xochitl_conf` + wallpaper-serve `native.rs`）；WiFi 连上恰 60 秒必掉的真凶＝cfg80211 regdomain 宽限（精简 regdb 的 CN 无 5150–5350，路由 5G 信道 36 被判非法）→ 连接锁 2.4G + `powersave 2`，`packaging/wifi-watch` 常驻固化（§03w）；离 USB 数秒自动休眠关 WiFi 是设备正常行为（`push --wait` 应对）；chrony 国内 NTP `packaging/chrony-cn.sh`；OTA 后五步恢复见 §05（README 有"OTA 与恢复"表）。

**2026-09-10 追加四轮（§03an-§03aq，都在同一天）**：网页正文全量 i18n（§03an，437 个 key，§03ae 那套架子当时只覆盖外壳+顶层导航，这次把传书/笔记/其他/管理四个 tab 的全部正文都补完）；host `shelf push --no-calibre`（§03ao，EPUB 输入跳过 Calibre 只跑 `epub-optimize`）；抓网文「同步优化」复选框（§03ap，`fetch_article` 加 `optimize` 参数，抓完紧接着跑一遍「清洗＋优化」）；`wash_css` 补 `figure`/`figcaption` 边距归零 + 真机排查"抓完优化还是大量留白"（§03aq，**这条排查最后的结论是"改了一处真实缺口，但没解决投诉"**，留白的真正成因是分页引擎对放不下的图片块整体挪页，没找到 CSS/EPUB 层面能调的杠杆，问题本身仍未解决，别把 §03aq 误当"留白问题已修"）。

**未闭环**：网页 UI i18n（§03ae 架子 + §03an 正文全量补完，437 key）——数据链路/内容对照已经真机验证过（curl 交叉核对 served 语言包与部署页面全部 `T` 引用零缺失），剩浏览器里人眼确认切换后排版/换行效果这一步没做。UI 人性化/触屏可用性一批小修（§03af）同样只做到"数据链路/代码逻辑确认对"，没有浏览器截图核对实际渲染效果。**图片密集网文渲染大片留白**（§03aq）——这条不是"还没人眼确认"，是"人眼确认过了、问题还在"：真机 A/B 验证过 `figure`/`figcaption` 边距不是成因，真正成因（图片块页尾放不下整体挪页、当前页剩余空间不回填）目前没有已知修法，详见 §05。Phase E ②③④、阅读线六项、代码体检、漫画超限分支复验均已闭环。「管理」二级 tab + 系统增强开关（§03aj）已真机验证通过；**CJK 手写笔迹优化 §03aj 那次还是纯占位卡片，2026-09-10 已经是真开关**（`enhance/handwriting-stroke/` 有了两个 hook 目标的真机验证，见 §03ak/系统增强白皮书 §03e-§03f，这句"纯占位"的旧结论已经过时，别再当现状引用）。真机测试顺手发现的 `hlSnapCjk` 荧光笔精确吸附不生效问题已经查清并修复——根因不在这次改动，是 `cangjie-langhook.so` 这个 xovi 扩展整个从设备上消失了，详见系统增强白皮书 §04 追记（不是书架线的事，跨块修复不在本白皮书重复记）。

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

`shelf.target`（WantedBy=multi-user）；服务 `PartOf=shelf.target` + `WantedBy=shelf.target`；网关 `ExecStartPre=-…/lo-alias.sh`（同一份脚本，让 10.11.99.1 常驻可达以便 `/upload` 注入；2026-09-11 起脚本源移到 `enhance/lo-alias/`、去掉 `cangjie-` 前缀，往后新命名不再用这个前缀）与 `ExecStartPre=-/usr/bin/fc-cache`（tmpfs 索引真重启后丢）。`install.sh` 写 /usr 前实检 dm-verity。**不给 xochitl 加任何依赖**。

## 03b｜Phase 1 统一投递（2026-09-03，离线完成）

**book-serve**（loopback 8790）：（⚠ 本段直投路 `POST /?target=` 与 `Native`/`Annot` Strategy 已于 2026-09-05 删除，见 §03s；现行唯一入口是母版库 `/staging*`，§03r）`POST /?target=native|annot&folder=&optimize=auto|off` multipart 多文件流式落 `.work` → 目标 Strategy（`Native`=Precheck→Convert→Optimize→Inject；`Annot`=Precheck→Convert(cbz)→Inject，只收 PDF/CBZ、EPUB 回执"去 host 定稿"）→ done/failed 归档；`GET /status|/targets|/inbox`、`POST /inbox/retry|/inbox/delete`。自有 spool `$XDG_STATE_HOME/shelf/books/`（inbox 供 scp 追平：启动扫一遍 + inotify 8s 防抖；`.work` 崩溃残留启动时移回）。配置 `~/.config/shelf/book.json` 首启写出缺省。
**koreader-serve**（8791）：（⚠ 直传 `POST /books` 已于 §03s 删除，现只从母版库 `POST /books/adopt`）`POST /books?folder=` 原字节落 `books/[folder]/`（先 `.part` 再 rename，KOReader 扫目录不见半成品）；`GET /status`（installed/running(扫 /proc cmdline)/version(git-rev)/计数）、`GET /books`、`GET|POST /fonts`（字体镜像口，供 font-serve）、`GET /dicts`；`GET /annotations`/`GET /vocabulary`（**新增**，2026-09-16：`books/` 下每本书的高亮标注 `<book>.sdr/metadata.*.lua` 原样读回 + 生词本插件库 `data/vocabulary_builder.sqlite3` 原样读回，只读、不写条目库——笔记线 ink-serve 拉这两个端点回流成条目，本服务只吐原始数据，见笔记线白皮书 §03al）。
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

**修法（都不改 shelf 代码）**：① **传书走 USB**（`https://10.11.99.1`，lo 别名常驻；2026-09-10 起网关绑标准 443，不用带端口号，见 §03am）——上传不过 WiFi，云同步后台慢慢传不影响操作，**推荐**；② 换真路由器（上行宽，突发不明显）；③ 不需要书上云可退出账号/关同步（影响所有云功能，不建议为此关）。

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

**追记（2026-09-09，若将来又想把"墨香管理"接进书架网页，先看这条；2026-09-10 补记端口冲突已消失但结论不变）**：`reading/device-rs` 的 `wr-serve`（墨香面板 API，`127.0.0.1:8777`）自己内嵌了一条上传页线程，**硬编码监听 `0.0.0.0:8778`**（`src/upload_server.rs`）——写这条追记那天跟 `shelf-gateway` 本体绑定的端口完全一样。代码里有一行注释说"以书架为准，`CANGJIE_UPLOAD_PAGE=0` 让出端口"，但从没有任何 systemd/安装脚本真的设过这个环境变量。**`shelf-gateway` 2026-09-10 起改绑 443（§03am），字面上的端口号冲突已经不存在**——但这不是"问题解决了"，只是巧合躲开：两者目前**没有一起跑过**（wr-serve 未接入书架的服务注册表，不受 `shelf.target` 管）依然是事实，功能重复（"传文件到 inbox"跟 book-serve「传书」入库完全重复）也依然是事实。但凡真要把墨香接进书架，第一步依然应该是砍掉 wr-serve 那条内嵌上传页，而不是庆幸端口号现在凑巧不撞车——这条建议不因为端口号变了而失效。

## 03v｜固件升级 3.27.3.0 → 3.28.0.172 实录（2026-09-05，真机）

**为什么记**：这是书架落地后第一次跨固件 OTA，把"哪些会被冲、哪些活着、怎么不循环死机"钉成事实，下次升级照抄。

**升级机制（真机核）**：OTA 走 Memfault（`memfaultd` swupdate 特性，`/tmp/remarkable-production-image-3.28.0.172-chiappa-public.swu`），写进**备用槽** root_a（`mmcblk0p2`），30 秒 SUCCESS，等重启切槽；当前槽 root_b 的 3.27 完整保留（`/proc/cmdline root=` 看在哪个槽）。3.28.0.172 的内部版本串是 `3.28.1.1666`（`strings /usr/bin/xochitl`），os-release `VERSION=5.8.203`。

**冲掉 / 活着**：新槽 `/usr` 是全新 rootfs——书架 6 个 systemd 单元、壁纸 bind 单元 + sleep 钩子全没；`/home` 原样（母版库、KOReader、xovi、`~/.local/bin` 二进制、配置、证书都在）；`/etc` overlay 照旧 tmpfs。dm-verity **未激活**（`dmsetup ls --target verity` 空），`mount -o remount,rw /` 可写，install.sh 的 verity 门照常放行。

**appload 要不要停**：appload.so **没有 `_xovi_shouldLoad` 自检**，不会"自己停"；但 xovi 的 LD_PRELOAD 只在 `/etc` tmpfs 里（`xovi/start` 现场写），重启即清，所以**首次进 3.28 是纯原厂 xochitl，任何扩展都不载入**——循环死机只可能发生在之后手动 `xovi/start` 时。且即便循环，StartLimitAction 整机重启后 `/etc` 又清空、自愈，**最坏是一次被迫重启，不会砖**。实际操作：重启前把 `appload.so` + 两份 qmd（KOReader 侧栏入口、字体菜单 3.27 锚点）挪到 `/home/root/xovi-disabled/pre-3.28-<时间>/`（extensions.d 外；md5 清单 + README 同放），`exthome/appload/koreader/` 数据不动。

**进 3.28 后顺序（实测约 8 分钟）**：① `xovi/rebuild_hashtable`（设备旁输密码，hashtab 20231 条重建）→ ② `xovi/start` 只带 qt-resource-rebuilder（`[qmldiff]: Set system version to 3.28.0.172`，xochitl NRestarts=0）→ ③ host `SHELF_NO_BUILD=1 ./deploy.sh 10.11.99.1`：五服务 active、`uploadReachable=true`、壁纸 bind 1 处、HTTPS 401 正常；install.sh 按 IMG_VERSION 选了 3.28 锚点 `font-menu-dynamic.qmd` 放进 qrr 目录（**未激活**，下次 `xovi/start` 才生效，独立一步验证）→ ④ `ssh root@10.11.99.1 sh -s < packaging/chrony-cn.sh` 恢复国内 NTP（§03w，OTA 冲 rootfs 后 chrony 会回到 Google 服务器）。 → ⑤ `packaging/wifi-watch/install.sh` 装回 wifi-watch 常驻看护（§03w）。**appload 不挪回**：0.5.3 的 qmd 钩 3.28 已删的 `SidebarFilterItem`（上游 PR #59 只改 qmd、编进 .so，重编需 rM Qt6 SDK），KOReader 暂无侧栏入口。

**可升级性的准确说法（2026-09-05 用户问"是否 99% 可用"）**：*升级零风险、数据零丢失、随时可升；升完要手工四步装回，不是"升了就能用"*。风险按层分、不合成百分比：书架五服务/壁纸/WiFi/chrony/休眠屏键只用 `/upload` 接口和系统标准件，重装即回（本次 100%）；qmldiff 注入（字体菜单）依赖 xochitl QML，大版本常要重适配（3.27→3.28 已两版 qmd）；中文输入法 langhook（本机未装）靠二进制特征码，每版真机验、有 fail-safe；KOReader 本体独立无碍，侧栏入口靠第三方 appload、3.28 目前挂着。

**顺带核实**：`/home/root/.bashrc` 没有登录触发的 xovi 恢复钩子（记忆里的 A2 登录恢复在这台重置机上**没装**）；`root` 的 shell 是 `/usr/sbin/rmdevlogin`（`exec -a -sh /bin/sh --login`）。vellum 的 `post-os-upgrade` 钩子只打印"先跑 rebuild_hashtable"，不自动做。装 KOReader 的字体/书目录 `exthome/appload/koreader/` 与 appload 是否加载无关，koreader-serve 照常工作。

## 03w｜原生自定义休眠屏 `SleepScreenPath` + WiFi 60 秒掉链根因（2026-09-05，3.28 真机）

**原生休眠屏（隐藏键，真机通）**：xochitl.conf `[General]` 加 `SleepScreenPath=<png 绝对路径>`（无需 `file://`，QML 自己补）。3.28 `SleepScreenView.qml` 解出来的机制：`logo.source = isettings.sleepScreenPath`（默认 `/usr/share/remarkable/suspended.png`）；`isCustomSleepScreenPath` 为真时 **logo 铺满整屏 PreserveAspectFit、插画卡自动隐藏**（`illustration.visible = showCarousel && !isCustom`）；图加载失败显示占位文字 "reMarkable is sleeping"。`ShowSleepScreenCarousel` 不是隐藏功能——3.28 设置里就是「休眠屏幕插图」开关（`SettingsSleepScreenToggle.qml`）。真机：停 xochitl → sed 写键 → `xovi/start`，键不被 xochitl 抹掉；用户确认休眠屏显示指定的池图（954×1696 满屏）。现指向 `~/.local/share/shelf/wallpapers/current.png`（wallpaper-serve 唤醒轮换原地覆盖的那个文件）；**用户两次休眠对照确认轮换生效**（"休眠图生效"，2026-09-05 17:10）——xochitl 每次休眠按路径重读，不吃 Qt Image 缓存。结论：bind-mount suspended.png + 三张透明插画卡 + `/usr` 写入 + sleep 钩子整套**可退役**，只留 wallpaper-serve 的唤醒轮换（原地覆盖 current.png）+ 安装器写 `SleepScreenPath`（§05 第 8 条，待做）。改键流程：`cp xochitl.conf` 备份（含 token，绝不打印）→ `systemctl stop xochitl` → `sed -i '/^\[General\]/a SleepScreenPath=…'` → `xovi/start`。

**WiFi "升级后连不上"：〔首判，2026-09-05——后被推翻，真凶见本节末"真凶（2026-09-06 定案）"段：是 cfg80211 regdomain 宽限，省电只是次要因素〕不是 3.28 回归，先怀疑 NXP IW612（`iw61x` MWLAN, SDIO）省电模式**。现象：连上 AP 后 **约 60 秒** `systemd-networkd: wlan0: Lost carrier` + `wpa_supplicant: REGDOM-CHANGE init=CORE type=WORLD`，**没有** wpa DISCONNECTED 事件、dmesg 无字，NetworkManager 仍标 `connected`、路由标 `dead`、永不自愈；`nmcli con up <SSID>` 立刻回来再掉。persistent journal 翻旧账：**3.27 那次 2 天 uptime 里 `Lost carrier` 95 次**，当天 15:29/16:21 也是连上 62 秒即掉——一直如此，只是以前主要走 USB 没察觉。`iw dev wlan0 set power_save off` 后 7 分钟以上零掉（对照：开着时 60/61/61/101/139 秒必掉）。置信度高。**根治（用户拍板，已写入）**：`nmcli con modify 我是猫 802-11-wireless.powersave 2`（按连接持久，NM 库在 `/var/lib/NetworkManager` bind 自 /home）；重新激活后 `iw dev wlan0 get power_save` = off，NM 每次激活都会关省电。代价 WiFi 开着时功耗略升（休眠时 WiFi 本就关）；xochitl 在设置里删掉重建该 WiFi 会丢此设置，换新 SSID 要再 modify 一次。

**时钟从未同步的根因与修法**：`chronyd` 4.5 配置的 4 个 `time{1-4}.google.com` 在国内不通（ping 100% 丢），启动以来没有一条 `Selected source`，`timedatectl` 一直 `synchronized: no`（RTC 本身准，差 1 秒，靠的是出厂/上次同步）。设备没有 `chronyc`、busybox 没有 `ntpd`。修法：改 **rootfs 底层** `/etc/chrony.conf`（服务器换成 `ntp.aliyun.com / ntp.tencent.com / cn.pool.ntp.org / time.cloudflare.com`），重启不丢（OTA 会冲）。**改底层 /etc 的姿势**：`/etc` 是 overlay（lower=/etc 本体，upper=/var/volatile tmpfs），必须 `mount -o remount,rw /` **先于** `mount --bind / /tmp/rootbind`（bind 继承挂载时的 ro 标志，后 remount 不传播），改 `/tmp/rootbind/etc/…`，umount，再 remount ro；**overlay 缓存 lower**：改完当场 `/etc/` 看到的仍是旧内容，重启才变——本次要立即生效就再 `cp` 一份进 overlay（落 upper tmpfs，重启自然消失、底层接管）。`mount -o remount,ro /` 偶发 `mount point is busy`，隔几秒再试就过（journal 在 /home 不占 rootfs）。改后重启 chronyd 8 秒 `Selected source ntp.tencent.com`，`synchronized: yes`。**"时不时连不上"的另一半真相（2026-09-06）**：不插 USB 时设备空闲几秒就进内核深度休眠（journal `PM: suspend entry (deep)` / `rm_sleep_monitor: Enter autosleep`，屏幕内容不变、`Woke up with reason=Ignored`），WiFi 随之断，摸一下屏就回来并重新 DHCP（还会漫游到另一 BSSID）。这不是 WiFi 故障：**要长时间可达就插 USB 供电（充电时不 autosuspend）**，排查前先 `journalctl -b | grep "suspend entry"`。

**真凶（2026-09-06 定案，八步排查后）**：`iw event` 抓到掉链是 `disconnected (local request)`，wpa_supplicant 无断开事件、NM 无 deactivating（debug 级）、nm-metrics 停掉照掉、有 ping 流量照掉——都不是。时序：连上 AP → wpa `REGDOM-CHANGE init=COUNTRY_IE alpha2=CN` → **恰好 60 s** 后 `Lost carrier` + `REGDOM-CHANGE init=CORE type=WORLD`。60 s = 内核 cfg80211 的 `REG_ENFORCE_GRACE_MS`：regdomain 变化后宽限 60 s，然后主动断开落在"新规则不允许的信道"上的连接。设备自带的 `/lib/firmware/regulatory.db` 是 reMarkable 精简版（2 KB），`iw reg get` 里 **CN 只有 2402–2472 和 5735–5835 两段，没有 5150–5350**；路由器 5G 用信道 36（5180 MHz）→ 断开 → regdom 回滚 world（5180 又合法）→ 重连 → 国家码 IE 再设 CN → 60 s 再断，无限循环。3.27 两天 95 次、昨天 60/61/61 s 全是它；省电模式那条只是叠加的次要因素（关掉后昨天 7 分钟不掉是因为当时……并未真正验证 5G 下长时间稳定）。**验证**：`nmcli con modify 我是猫 802-11-wireless.band bg` 锁 2.4 GHz（2437 在允许段）→ 3 分 20 秒 40/40 ping、零掉链、零 regdom 事件。**两条出路**：① 设备连接锁 2.4G（已设，慢一些）；② 路由器 5G 信道改 149–165（5735–5835 在允许段），再 `nmcli con modify 我是猫 802-11-wireless.band a` 或清掉 band 限制。regulatory.db 带签名（`.p7s`，内核编入的证书校验），换不了。

**常驻看护 + 固化（2026-09-06，用户拍板"所有 WiFi 走 2.4G"）**：`packaging/wifi-watch/`（`wifi-watch.service` /usr 单元 + `~/.local/bin/wifi-watch.sh`）：① 每 15s 查一次，NM 说连着而 wlan0 连续两次 NO-CARRIER 就 `nmcli con up`；② 当前活动 WiFi 连接缺 `802-11-wireless.band=bg` 或 `powersave=2` 就补上并重新激活（每个连接只查一次；新 SSID / 设置里重连后自动生效；`BAND=` 置空关掉频段固化）——因为省电关掉后**slumber 醒来仍会**出现 `Lost carrier`+`REGDOM init=CORE` 的假死（09:51 实例：醒来 37s 后掉、插 USB 也不自愈）。日志 `journalctl -u wifi-watch`。OTA 后重装（§03v 第 ⑤ 步）。

**已落地的兜底**：`packaging/xovi-post-start/wifi-reconnect.sh` → `/home/root/xovi/scripts/post-start/`（`xovi/start` 后台看护 60 秒，见 NO-CARRIER 就 `nmcli con up`，`journalctl -t xovi-wifi`），真机 `t+5s NO-CARRIER → 已激活` 通。

## 03x｜退役 bind-mount 壁纸整套，改写 `SleepScreenPath`（2026-09-06，3.28.0.172 真机）

**动机**：§03w 证明原生隐藏键 `SleepScreenPath=current.png` 满屏、隐藏插画卡、每次休眠重读——bind-mount 覆盖 `/usr/share/remarkable/suspended.png` + 三张透明插画卡 + 开机单元 + sleep 钩子那整套（§03g P2 时代）从此多余，且是书架唯一还在写 `/usr` 且要 root 挂载的地方。

**改法**：① `shelf_core::xochitl_conf`——`xochitl.conf [General]` 单键 get/set/remove：只动目标行、tmp+rename 原子写、首次改前留 `xochitl.conf.shelf-bak`；**文件含 DeveloperPassword/UserToken，模块任何路径都不返回/打印行内容**（错误只带键名）；无 `[General]` 段时补一段；单测锁"插在段头后、其它行逐字节不变、删键后与原件相同"。② wallpaper-serve `mount.rs` → `native.rs`：`enable`（写键指向 `current.png`，幂等）/ `disable`（删键）/ `restart_pending`（记住写键时 xochitl MainPID，PID 没变即"还没生效"）；激活首张时自动 `enable`；`GET /status` 出 `native:{enabled,path,restartPending}`，上传回执 note 首次提示"跑一次 xovi/start"。③ 删 `blank776.png` 生成、`CAROUSEL/SUSPENDED_PNG` 常量、wake.rs 入睡补 bind 分支、`systemd/shelf-wallpaper-bind.service`、`wallpaper/shelf-wallpaper-sleep.sh`；deploy.sh / package.sh 不再组 `wallpaper/`。④ install.sh 3b：迁旧池 → **清旧 bind 整套**（`disable --now` 旧单元、按 `/proc/mounts` 卸 4 个 bind、remount rw 删 /usr 里的单元+钩子）→ 有 current.png 就 `wallpaper-serve enable`；uninstall.sh：`disable` 删键 + 同样的旧残留清理。网页壁纸页状态改为「原生休眠屏 已启用/未启用 · 需跑一次 xovi/start 生效」。

**真机**：WiFi `192.168.1.22` 部署（USB 当时不通；known_hosts 里 192.168.1.22 是别的机器的旧键，按 USB 已知指纹核对一致后替换）。装前：4 个 bind 在挂、旧单元 active、键已在；装后：bind 0、旧单元/钩子文件全没（`reset-failed` 清掉 systemd 残留态）、键仍在、`native.enabled=true restartPending=false`、rootfs 回 ro、xochitl 未动（NRestarts=0）。**用户确认（2026-09-06）**：bind 全卸后休眠 3 次换了 3 张池图——只靠原生键 + 唤醒轮换成立，闭环。

**QSettings 运行中改键的边界**：xochitl 在 sync 时按 mtime 重读再合并，外部加的键不被抹（本次装机时 xochitl 在跑、键保住）；但 `isettings.sleepScreenPath` 只在启动时读，所以首次写键必须 `xovi/start` 一次；之后换图不再碰 conf。

## 03y｜xochitl CSS 引擎实测规则（2026-09-06，八轮"诊断 EPUB"量渲染缓存，3.28.0.172）

**方法**：造只有一个变量的诊断 EPUB（每章一种写法 + 同章一个对照 `<p>`），scp 进 inbox → 设备上 `wget --post-data` 打 `127.0.0.1:8790/staging/deliver` 投原生（免密）→ xochitl 导入即渲染 `<uuid>.pdf` → pymupdf 量每段首行相对下一行的 x 偏移（只量会换行的段）。**全程不需要肉眼，不需要用户点**；一轮 2 分钟。诊断 7–14 共 60 个变体。

**钉死的规则**（推翻/修正了记忆里"只认外链裸 p{}"的旧结论）：
1. **规则里最后一个没有分号收尾的声明被丢掉**。`p{text-indent:2em}` 整条不生效，`p{text-indent:2em;}` 生效；书 css `.calibre_ {…;text-indent:1.2em;margin:0}` 的 `margin:0` 一直被吞。→ 清洗层所有输出声明一律尾分号（`filter_decls`、`wash_css`）。keep-spacing 档位此前因此在 xochitl 上零缩进。
2. **`text-indent:0` 被当"没设"**，落回继承/其它规则；`0.01em`、`-0.01em` 生效。→ 顶格规则写 `text-indent:0.01em`。
3. **类选择器认**（`.x{text-indent:2em;}` 对 `div.x` 得 2em），且**类规则压过元素规则**（`p.cj-flush` + `.cj-flush{0.01em;}` 顶格）。此前"不认类选择器"是规则 1 的误读。
4. **同为类规则时先出现者胜**（书的表链接在前 → `.calibre_` 压住我们的 `.cj-flush`，无论我们的表链在前后、无论 class 属性里顺序）。同选择器后写的 `p{}` 也压不过先写的。
5. **不认内联 `style=""` 属性**；`div{}` 元素规则不生效（div 只吃类规则/继承）。
6. **text-indent 继承**：`<div class="calibre1">`（书的 1.2em）里的子 div 若自己"没设"（含 0）就继承缩进——Sheldon v5 顶格失败的根因。
7. 外链 css 里混类选择器不会"废整表"（诊断 11 V4/V5 对照 p 正常）；旧结论来自当年带 `!important` 的实验。`!important` 仍不认。

**最终配方（v6 真机量：章首 / 场景切换 0pt，续段 14.2pt）**：顶格段 → `<div class="cj-flush">`（**剥掉书的类与 style**，只留 cj-flush；id 等保留）+ `cangjie-wash.css` 加 `.cj-flush{text-indent:0.01em;margin-top:0;margin-bottom:0;}`；判定顶格的信号见 wash.rs `flush_first_para_after_heading`（h 标签后 / 加粗或 Chapter… 开头的标题样段后 / 双 `<br>`·空段·`* * *` 后 / 章首）。KOReader 走标准 CSS 同样顶格。

**中文书（同日闭环）**：用户经母版库「优化」后投原生的《人骨拼圖》仍零缩进——原因是这本 2017 年旧 EPUB **全书没有 `<p>`**：每章一个 `<div>`、450 个 `<br/>` 分行、每段开头两个全角空格 U+3000。`p{text-indent:2em}` 没有对象；xochitl 把 U+3000 折叠掉 → 0；KOReader 把 U+3000 按字体宽度画 → 用户看到的"中文换字体缩进跟着变"。清洗层中文模式新增 `cjk_paragraphize`：文件里 `<br` ≥4 且无 `<p>` 时按 br 切成 `<p>`（块级标签原样），所有 `<p>` 段首的全角空格/nbsp 剥掉，缩进统一走 css。真机：设备端重新「优化」→投原生，渲染缓存量到 278 个段首 24.1pt（=2em）、续行 0。

## 03z｜事件推送：网页零轮询即时刷新（2026-09-06，用户"不喜欢轮询，要事件通知，包括 host"）

**设计**（三条硬约束：不轮询、不监听全盘、日志写入不触发）：
- `shelf_core::events`：进程内 `EventBus`（订阅者各持有界通道，满了丢——事件只是"该刷新了"的信号）+ `SseStream`（`Read` 包装，20 s 无事件吐一行注释心跳）；`http::Reply` 新增 `stream` 字段。**流式回执不能走 tiny_http 的 `respond`**：它的 chunked 编码器（chunked_transfer::Encoder）攒满 8 KB 才发、外面再套 1 KB BufWriter，小帧永远滞留（真机 curl 30 s 零字节，垫到 1.2 KB 也没用）；改用 `Request::upgrade` 接管裸 socket（先发只有头的 200 + `Connection: upgrade`，浏览器/curl 对 200 忽略它、按读到关闭处理），一帧一 flush。真机隧道 curl：inbox 落库与母版库删除的三帧当秒到达。每请求一线程，长连接不堵别人。 **用户确认（2026-09-06）：浏览器页眉圆点绿、不刷新列表自己变；host `shelf events` 同步打印——网页/CLI 两端闭环。**
- 四个领域服务在**变更发生处**发事件（上传/优化/落库/mark/抓网文/删除、inbox 追平、KOReader adopt/字体/词典/配置、原生字体装删/配置、壁纸入池/激活/模式/删除/唤醒轮换），各挂 `GET /events`。唯二 inotify：inbox（本就有）与网关的注册表目录（tmpfs，服务启停）。
- 网关 `events::Hub`：每个模块一条 loopback 订阅线程（阻塞读、断了 3 s 重连、服务没起就等），事件补 `svc` 转发到自己的总线；`GET /api/events` 受登录守卫，在 `/api/{svc}` 代理通配之前注册。
- 网页：`EventSource('/api/events')`，事件只刷对应 tab（在前台立刻刷；不在前台记脏、切过去时刷）；`manage` 事件若 tab 集合变了整页重载；页眉一个小圆点显示连接状态；断线（WiFi 掉 / 休眠醒来）浏览器自动重连。
- host：`shelf events [--once] [--area …] [--raw]`（transport 加 `stream_lines`），另开终端挂着看设备动静，或脚本里 `--once` 等落库。

**耗电**：空闲时 inotify/通道 recv 都是内核阻塞，零唤醒；每条 SSE 连接 20 s 一次十几字节心跳；设备深度休眠时进程不跑、连接断、醒来重连。对比轮询（每 5 s 一个完整 HTTPS 请求）少两个数量级。

## 03aa｜投原生后的渲染自检（2026-09-06，3.28.0.172 真机）

**问题**：整章渲染失败（同一标签双 id → 严格 XML 吞整章，《消失的爱人》只出 7 页；背景图盖正文）以前都是用户翻到才发现。

**依据（真机事实）**：xochitl 导入 EPUB 时**同步渲染**，`/upload` 返回时 `<uuid>.content` 已有 `pageCount`（人骨拼圖 523、Tell Me Your Dreams 352），旁边有 `<uuid>.pdf` 渲染缓存。所以自检只读一个 JSON 字段，设备端不用解析 PDF（设备也没有 pdfinfo/mutool/python3）。

**做法**（book-serve `render_check.rs` + shelf-core `xochitl::{find_documents_since,page_count}` + bookconv `stats::text_profile`）：
1. 投书前算正文非空白字符数与 dc:title，期望页数 = 字符数 ÷ 每页字符数（真机标定：中文 460 字/页、英文 960 字符/页，两本真书 0.99 吻合）。
2. `/upload` 不回 uuid、visibleName 取自 EPUB 元数据不等于文件名 → 按 `createdTime ≥ 投书时刻` 圈候选，书名（dc:title / 文件名 stem，忽略大小写）相符者优先，否则最新一本。
3. 立即探一次；没渲染完就 `fswatch::watch_until` **限时**监听书库目录（3 s 防抖、最长 10 min，结束即撤——不给书库留常驻 inotify，守 §03z 约束；读线程靠私有"踢醒"目录退出）。超时记 `timeout`（不是错误：设备上打开一次就渲染）。
4. 判定 `pages < expected × 50%` → `warn`。阈值标定：自检发生在导入当下、xochitl 用缺省字号/边距，页数只随文字密度浮动；坏章在 xochitl 里各占 **1 页空白**，4 章坏 3 章的探针 10/29=0.34，30% 抓不住，定 50%。
5. 结果写母版库边车 `.<name>.delivered` 的 `render`（事件有损、状态必须落盘）+ 推 `books/render {name,status,pages,expected}`；网页母版库条目徽章「渲染 N 页」/「⚠ 只渲染 N 页」（红）/「渲染中…」/「未见渲染」，`shelf events` 打印字段。

**真机**：Probe Good（4 章随机词）ok 25/29；Probe Bad（h1 双 id ×3 章）warn 10/29；事件四条当秒到达，边车落盘。三本探针留在原生书库根目录（用户在设备上删）。

**排版回归探针 `shelf doctor --render`（同日）**：把 §03y 八轮手工诊断固化成一条命令。host `calibre/render_probe.py`（stdlib）造探针 EPUB——第一章拉丁：h1 后首段 `div.cj-flush`（PFLUSH1）、两个续段 `<p>`（PINDENT1/2）、`* * *` 场景切换后首段（PFLUSH2）、续段（PINDENT3）；第二章中文同规则（PFLUSH3/PINDENT4）；自带 `cangjie-wash.css`，字面与 `wash_css` 拉丁配方一致（配方改了探针要跟，测试钉住字面）。流程：`POST /api/books/staging` 入库 → `deliver {folder:"书架自检"}` → 读 `delivered.render`（pending 才挂事件流等 `books/render`）→ 新路由 `GET /api/books/staging/render/{uuid}` 取回渲染缓存 → `calibre/render_measure.py`（pymupdf）按哨兵量"首行 x − 下一视觉行 x"→ flush |em|<0.15、indent 1.2±0.2 em → PASS/FAIL；量完从母版库删探针（探针书留在原生书库「书架自检」文件夹）。
- 真机坑：pymupdf 把**同一视觉行拆成多段**——拉丁哨兵（EBGaramond）与 CJK 回退字体（KingHwaOldSongGJ）各成一 "line"，y0 差 2 pt；不合并就量出 −62 pt。`visual_lines` 先按 y（<0.6 字号）合并碎片、x 取最小，再量。
- 真机数字（3.28.0.172，缺省字号 12.05 pt）：PFLUSH1/2/3 = 0.0 pt；PINDENT1–4 = **14.27 pt = 1.184 em**（与 §03y 的 14.2 pt / 1.17 em 一致）；7/7 PASS。CJK 段走 KingHwa 回退、缩进同样由 css 给出（不靠全角空格）。

**`shelf push --wait`（同日）**：设备离 USB 后几秒就自动休眠关 WiFi（§03w），push 最常见的失败是逐本报"连不上"，而且失败发生在洗书之后。改法：洗完第一本、上传前先 `transport.reachable()` 探网关公开路由 `GET /health`（不用密码、3 s 超时；任何 HTTP 应答都算在线）；不可达时——无 `--wait` 直接 rc 2 并提示"点亮屏幕或接 USB / 加 --wait"，产物留在工作目录；有 `--wait[=秒]`（缺省 600）每 5 s 探一次、每 30 s 复述剩余时间，醒了继续上传，Ctrl-C 放弃。探活只做一次，不通就不再洗后面的书。真机：`reachable()` 对在线设备 True、对不存在地址 False；"睡着→点亮→续传"的时序由用户日常使用验证。

**原生回收站代理（同日晚，`xovi/shelf-trash-agent.qmd` + book-serve `trash.rs`）**：探针书每跑一次 `doctor --render` 就在原生书库多一本。外部进程直改 `.metadata` 的 `parent="trash"` 是判死路（阅读白皮书：运行中 xochitl 的内存文档模型会覆写回来），唯一可靠的软删是 xochitl 自己的 `EntitySelection.selectionMoveToTrash()`——从 reading 线 `trash-agent.qmd` 剥离移植：注入 Sidebar 一个隐形 Item，挂当前文件夹文档模型 `entityListModel` 的 `rowsInserted`/`modelReset`（xochitl 自己在用的信号，纯事件驱动不轮询），4 s 防抖后 `GET 127.0.0.1:8790/trash/pending`，逐个 `selection.add` + `selectionMoveToTrash`；不在文件视图或用户正手动选中则跳过。book-serve 队列 `$XDG_STATE_HOME/shelf/books/trash-pending.json`：`POST /trash/add {uuid,name}` **入队时按 visibleName 核对 uuid**（错 uuid 就是错删别的书，拒绝），`GET /trash/pending` 顺手把已进回收站/已不存在的出队（QML 端无需 ack）。doctor 在取回渲染缓存后立刻入队——落在本次 `/upload` 触发的 4 s 防抖窗内，量完探针已在回收站（可恢复）。
- 3.28 锚点核实：从设备 `/usr/bin/xochitl` 解出 556 个 QML（`extract_qml.py`），Sidebar.qml（blob qml_00dcd9d7）仍是 `#root > ColumnLayout#filterColumn`，`NavigationManager.activeContext.{explorer.entityListModel,selection}` 与 `selectionMoveToTrash` 都在（qml_00dcfe48）；本机重建 asivery/qmldiff CLI 用设备 hashtab 离线 `apply-diffs` 一次通过，再 `xovi/start` 上机。
- 真机：xochitl 启动时不触发（进的是上次视图），投一本书 → `rowsInserted` → 5 s 内 `SHELF-TRASH: moved 2 to trash`（新探针 + 一本排队的旧测试书），队列清空、`parent:"trash"` 由 xochitl 自己写入。名字守卫真机验证：同 uuid 配错名 400。

**原生建文件夹代理（2026-09-07，`xovi/shelf-mkdir-agent.qmd` + book-serve `mkdir.rs`；当时是给 `notes/` note-serve 用，⚠ 2026-09-09 起 note-serve 已经改成复用书本自己的设备文件夹、不再调用这条链路，当前消费方存疑，见 `mkdir.rs` 模块注释与 notes 白皮书 §03ae——下面这段反编译/设计记录本身仍然是这套机制真实可用的证据，只是"给谁用"这句话已经过期）**：note-serve 生成《书名》一章一本，`Xochitl::upload` 找不到目标文件夹只会 best-effort 落库根——建文件夹跟软删一样，外部进程没有合法通道，必须走 xochitl 自己的代码路。同一轮 `extract_qml.py` 全量解出真机 QML（对拍固件 md5 952f1e28f...）反解出这条路：`library-ui/window/create-collection` 对话框确认按钮调 `root.library.createCollection(parentFolderId, name)`，`root.library` 在别处（如 `PageSelection{library:Library}`）被绑定为**裸全局单例 `Library`**（`import xofm.libs.library`；跟 `LibraryController` 不是同一个对象，只有 `Library` 才有 `createCollection`）；顶层（书库根）`parentFolderId` = 空字符串（`currentFolderId` 声明处缺省值就是 `""`，跟本项目"根 parent 为空串"的一贯约定一致）。**没有点开那个新建文件夹对话框做交叉验证**（会真建一个用户可见文件夹，干扰更高），这条结论止于"反编译 + 静态调用链一致"，用真实业务场景（note-serve）间接验证。锚点选 `MainView.qml`（`cardhw-notify.qmd` 用过的同一份文件）而不是 Sidebar：Sidebar 没 `import xofm.libs.library`，要用得先加一条 `IMPORT`，而 qmldiff 的 `IMPORT` 语句强制要显式版本号，源文件是 Qt6 无版本 import，硬造版本号风险不可控；MainView 本来就有这两个 import，改动面更小。Timer 8 s 轮询 `GET 127.0.0.1:8790/mkdir/pending`（没有类似 rowsInserted 的天然事件可等——建夹必须先于 `/upload` 发生，不能等上传后的事件才反应，只能轮询，量级同 cardhw 的 5 s/trash 的 4 s）。book-serve `mkdir.rs`（`MkdirQueue`，跟 `trash.rs` 同款套路）：`POST /mkdir/add {name}` 入队（已存在就不入队）、`GET /mkdir/pending` 拉取执行、顺手把已经真实建出来的名字剔除（无需 QML 端 ack）。
- 真机（同日）：`POST /upload` 先起了个坑——`/home/root/xovi/start` 头一次跑完 `Job for xochitl.service canceled`，`/etc/systemd/system/xochitl.service.d/` 事后检查是空的（LD_PRELOAD 等全部没进程环境），本机没装 `xovi-reenable.service`、纯 vellum tmpfs 机制这次没吃上；**原地重跑一次 `xovi/start` 干净成功**（新 PID 环境变量核对齐全）。这不是 mkdir qmd 的问题，是这套 tmpfs 持久化本身偶发不稳，记一笔：`xovi/start` 跑完务必核对新 PID 的 `LD_PRELOAD`/`XOVI_ROOT` 环境变量，不能只看 `systemctl is-active`。
- 真机（正式验证）：`journalctl` 见 `[qmldiff]: Loading file shelf-mkdir-agent.qmd` + `Processing file .../MainView.qml...` 无解析错误；note-serve 生成新章节（目标文件夹不存在）→ book-serve `mkdir` 队列即时出现 `《人骨拼圖》` → ~80 s 内（Timer 首次触发时机 + 8 s 周期）日志 `SHELF-MKDIR: created 《人骨拼圖》`、书库真多出一个 `CollectionType` 文件夹、队列自动清空；**再生成一次同书**，新文档 `parent` 字段正确指向刚建出来的文件夹 uuid（旧的、建夹前落根目录的那份原样留在根，没有被追加挪动——这是设计内行为，不是遗留 bug）；重复触发不产生重名文件夹（`add()`/`pending()` 双重"已存在即不建"兜底真机成立）。全程 `NRestarts=0`，五个服务与 xochitl 健康检查干净。

**已物理删除（2026-09-15，全量代码审查审出的死代码）**：全仓库确认除本模块自身外再无任何消费方
（note-serve 唯一记录在案的调用方 2026-09-09 已改用别的机制，见上），`book-serve::mkdir.rs`/
`/mkdir/*` 三条路由/`shelf-mkdir-agent.qmd`/`shelf/install.sh` 里部署这份 qmd 的那段全部删除，
`shelf/README.md` 三处目录说明同步去掉。**已部署在真机上的旧版 qmd 不受影响，可以放着不管**：
它的轮询逻辑是 `if (x.status !== 200) { return; }`，本模块下线后请求变 404，QML 端就静默不做
任何事，不会报错/崩溃，不需要专门去手动摘除——这条链路本身就是设计成"后端消失就自然失活"的
轮询代理，跟 `shelf-trash-agent.qmd`（还在用）不是一回事，别搞混了去动它。

**中文 TXT 切章（同日，`host/calibre/txt_to_epub.py`）**：网文以 TXT 为主，此前 `.txt` 原样进母版库→只能 KOReader 且无章节（bookconv/book-serve 零 TXT 处理，Calibre 也不认中文"第X章"）。host 路：stdlib 脚本解码（utf-8-sig → utf-16 BOM → gb18030 严格 → utf-8 替换）→ 一行一段、行首全角空格/nbsp 剥掉（缩进交 css）→ `第X卷/部/集`（一级）/`第X章/回/节/话`、`序章|楔子|尾声|番外…`（二级或一级）切章，标题行 ≤40 字防"第三章说过……"误判，一个没认出就每 8000 字硬切「第 N 部分」→ 极简 EPUB3（两级 nav、dc:title/creator 取自文件名 `书名 - 作者`）→ `wash_epub.sh`（`WASH_AUTOTOC=0`，目录已有）→ `check_output.py`。`formats.rs` 把 txt 从「只能 KOReader」挪到「电脑可转」（网页格式说明随之变）。
- 真机（样本：把《人骨拼圖》EPUB 正文抽成 GB18030 TXT，38 章 24.8 万字）：`shelf push` 链 txt_to_epub → wash_epub.sh → check_output（NCX 48 条全命中）→ 投原生 **531 页（自检期望 526，ok）**，正文页 x0 只有 17.8/41.9 两档 = 首行缩进 24.1 pt = 2em，章名 24.1 pt。样本暴露一坑：TXT 开头常自带一份目录（每行"第一部　一天的國王　1"），会被切成一串空章——`drop_contents_listing`：没正文且标题（去尾页码）在后面再次出现的章视为目录行丢掉。另一事实：**xochitl 渲染缓存 PDF 从不带书签**（Tell Me Your Dreams 的也是 0 条），目录只能从 EPUB 的 ncx/nav 验，不能从缓存验。

**漫画省刷新档 `shelf push --eink-gray`（同日，`host/calibre/comic_gray.py`）**：依据墨水屏波形按内容分档（彩重 / 256 灰中 / ≤16 灰轻 / 1-bit 最轻， 真机坐实），CBZ→CBZ 逐页：缩进屏盒（长边 ≤1696 且短边 ≤954）→ 平均色度 ≥ 0.06 保色 JPEG q85，否则 L → 16 级等距灰 + Floyd-Steinberg → **4-bit PNG**。阈值与采样法镜像设备端 `imgopt.rs`（`COLOR_KEEP_CHROMA`），前身是已删的 `einkify_epub.py`。用户定默认关、体积实测再议——实测《阿拉蕾（第 1 部）》AZW3：comic2cbz 1092 页 171.2 MB → 16 灰 1085 页 / 保色 7 页 **108.2 MB**（48 s）。此前担心"抖动噪点 Flate 压不动、比 JPEG 大 2–3 倍"没发生：原图远超屏幕分辨率，降采样省下的远多于抖动多出的；4-bit PNG 原始数据只有 8-bit 灰的一半。**用户目视《阿拉蕾①》16 灰翻页明显少闪 → 定默认开（`--no-eink-gray` 关；16 灰失败退回原图 CBZ 不挡推送）。** Pillow 显式进 `calibre` 依赖组（`getdata` 在 Pillow 12 弃用，改 `tobytes`）。

## 03ab｜代码体检与重构（2026-09-06，六项闭环后）

用户："如果没有值得新增的，就优化代码：设计模式、解耦、删失效代码"。先全量扫（死 pub 项 = 在 shelf 与 reading/knowledge 非测试代码零引用；重复小助手；过期注释；host 死函数），结论是**结构本身已经对**（端口/适配器、AssetStore 模板、ServiceSpec 模板、Hub 汇聚都在），能改的是四类零碎，各开一支 `--no-ff` 并入：

1. **shelf-core / 服务**：新 `clock` 模块收编 8 处 `SystemTime::now().duration_since(UNIX_EPOCH)…`（事件/证书/会话盐/备份戳/边车/自检/壁纸随机种子）；删死项 `paths::data_root`、`htmlproc::has_internal_anchor`、`optimize::is_current_version`（reading/device-rs 也零引用，编译核过），`WASH_MARK` 收私、`events::subscribers` 限测试；`xochitl.rs` 找文件夹/找文档两处各扫一遍 `.metadata` 合成 `metadata_entries` + `is_live`；book-serve 落库记录边车（`Delivered`/`RenderCheck`/读改删）从 500 行的 `staging.rs` 拆成 `sidecar.rs`（Repository），`staging` 与 `render_check` 只通过它落盘；`Cargo.toml` 头里"book-serve → device-core"的过期依赖注释改正。
2. **网关 UI**：`ui.rs` 484 行 Rust 原始字符串里嵌 CSS+JS → `ui/{index.html,style.css,app.js,auth.css}` 真文件，编译期 `include_str!` 拼成单页（网页仍零外链），`ui.rs` 剩 61 行；CI 加 `node --check app.js`（此前只能手工抽 `<script>` 段查语法）。
3. **host**：`calibre/epub_skel.py` 收编 pdf_reflow / txt_to_epub / render_probe 三处手搓 mimetype+container+opf+nav（两级目录、可选 css/图片/作者，布局统一 `text/cN.xhtml`）；`calibre_bridge` 五处"跑脚本解析末行 JSON"合成 `_run_json`（render_probe 也改成末行 JSON 契约）；`transport` 的 `_do`/`get_bytes`/`stream_lines` 三条请求路径合成 `_open`（鉴权头、401/403/HTTP 错、连不上的翻译只有一处）。探针改布局后真机重投 7/7 PASS（`../cangjie-wash.css` 相对链接 xochitl 认）。
4. **install.sh / uninstall.sh**：删旧 misc/wallpaper 搬池、bind-mount 退役清理（安装/卸载两侧）、blank776、旧 `add-reading-fonts.qmd` 搬迁四个迁移块（真机逐项确认零残留），install.sh 224→193 行，shellcheck 零告警；重部署五服务 active、四 `/health` 通。

**没动的**（评估后认为不值得或有风险）：`bookconv::convert` 整族（reading 线在用）；四个服务各自一行 `GET /events` 路由（不算重复，塞进 `service::run` 反而把总线所有权搞乱）；`multipart.rs`/`http.rs` 体量大但职责单一。**踩坑**：中途 `cargo fmt --all` 把 64 个文件重排进了提交，回滚重放——本仓库 Rust 从不走 rustfmt，别跑 fmt。

## 03ac｜可插拔机制验证：接住一条独立仓库线（2026-09-06 晚）

笔记线（独立仓库 `notes/`）选择复用书架的网关/注册表/事件汇聚/部署链，而不是另起一套——这是**笔记线自己的架构决策**（理由与取舍见 `notes/docs/reMarkable笔记白皮书.md` §01），本节只记**书架这边为了接住它、实际改了什么**，证明可插拔机制这套设计经得住"外部独立线接入"的考验：`manage::MODULES` 加几行映射、`build.sh/deploy.sh` 在对方 `Cargo.toml` 存在时顺带编译打包、`install.sh/uninstall.sh` 令牌同规则加、网页按同一事件总线接一个新 tab——书架自身代码只在这几处各加一点，没有为笔记线开任何专门口子。book-serve 原有的原生回收站队列（`POST /trash/add`）也顺带被笔记线未来的软删需求复用，无需书架新增能力。

**真机**：设备当时只在 WiFi（USB 网卡没起，`10.11.99.1`/mDNS 都不通，局域网扫 8778 找到 `192.168.1.22`），`SHELF_NO_BUILD=1 ./deploy.sh 192.168.1.22`：书架 5 服务 + 笔记线 3 服务共八项 `active`、注册表 8 项、HTTPS 探测 401 正常；书架侧行为零变化。**书架自身的遗留 gotcha**：`MODULES` 现在是跨两个仓库的单一事实源——以后任何独立线要接进来，都得先来这个文件登记一行，别只改对方仓库就以为完事，README 已写明。

## 03ad｜漫画跨页拆分 + 白边裁切放大 + 小体积漫画可选投原生（2026-09-08）

用户拿真机漫画库（阿拉蕾①、火影忍者卷 1、镖人）复查 `shelf push` 的漫画管线，报了两个问题：① 漫画优化按屏幕尺寸缩放，"第 2 页内容裁到第 1 页"；② 想让不需要分卷的小体积漫画也能选择投原生阅读器。真机排查出两件事分开修：东立扫描版《火影忍者》卷 1 的图片是**两页拼一张的扫描跨页图**（1674×1250，宽高比 1.34，中间有装订缝），旧管线把整张跨页图当一页缩放，症状就是"内容裁到别的页"；《阿拉蕾①》则是正常一页一图，只是约 8% 白边没利用。跟 §03t 那次"漫画→PDF→分卷投原生"被否决不是一回事：那次是"强制转 PDF、超限就分卷、分卷也投原生"，这次是"小的可以投原生、大的老老实实留 KOReader，绝不强制分卷"，正好避开了当初被否决的点。

**跨页识别 + 拆分**（`host/calibre/comic_gray.py`）：宽高比 `w/h` 超过 `SPREAD_RATIO_MIN=1.05` 才可能是跨页；在图片中段（35%–65% 宽度）找一条内容密度最低的竖直窄带当装订缝（不假设正中间，兼容装订缝偏移/扫描略歪）；找不到干净装订缝时，比例夸张到 `SPREAD_RATIO_CONFIDENT=1.3` 仍按猜的位置拆并标记 `flagged`（建议人工核对），比例不夸张就不拆；拆完两半仍明显偏宽（真三联跨页/扉页）就放弃拆分、原图直出、记入 `flagged`。默认从右往左排序（东亚漫画传统，`rtl=True`；`--manga-ltr` 覆盖）。内容密度剖面用 PIL 原生 `resize(..., Image.BOX)` 把二值掩码缩成 1 像素厚做逐列/逐行平均实现，**不引入 numpy 新依赖**。

**白边裁切 + 放大**：从四边向内扫内容包围盒（背景阈值灰度 <245 算内容，行/列内容占比 >0.5% 才算"有内容"），单轴裁掉超过 `MAX_TRIM_FRAC=20%` 就放弃裁该轴——防止大面积均匀浅色内容（满页留白分镜、渐变背景）被误判成白边整个裁没。裁完允许放大回填屏幕（原来的 `fit_screen` 从不放大），封顶 `MAX_UPSCALE=1.5` 防止拉花。

**真机样本验证**（不进单测，过程见对话记录）：
- 阿拉蕾①抽 5 页，白边裁掉后放大填屏，人工过图确认内容没裁坏。
- 火影忍者卷 1 全本（96 页真实 CBZ）：全部正确识别成跨页并拆成 192 页；抽查 page 48/49（原始截图里就是跨页 bug 现场）拆分后各自完整、无内容互相侵入，跟最初报的 bug 症状对上号；2 页因为没有干净装订缝（跨页大幅面全跨插图）被标记"建议人工核对"，符合预期；**page 1（封面/书脊/封底三联扫描，2819×1250 异常宽）被拆成两张仍偏横，没有互相混内容，但是本次验证中唯一不够干净的结果**——因为是封面美术不是叙事页，影响小，没有专门处理，留作已知的小瑕疵。

**小体积漫画可选投原生**：`cbz2pdf`（`bookconv::convert::cbz::cbz_to_pdf` 本体 2026-09-05 §03t 就没删过，只删了 shelf 侧调用方，这次复活成薄 CLI，`[[bin]]` 加进 `bookconv/Cargo.toml`，照抄 `epub-optimize.rs` 的风格）：`cbz2pdf [--mono] 输入.cbz 输出.pdf`，缺省 `EinkTone::Off`（原样直嵌不重新抖动——喂给它的是 `comic_gray.py` 已经处理过的 16 灰/保色 CBZ，再抖一遍只会烧画质）。`push.py::comic_prepare()` 灰阶 CBZ 处理完之后新增一步：查询设备当前原生上传上限（`ctx.transport.get("/api/books/status")` 的 `nativeUploadLimitBytes`，查不到就退回跟 book-serve 缺省值一致的 150MB 静态兜底，查询失败不阻断推送）。灰阶 CBZ 体积 **×1.5** 估算 PDF 体积（真机实测 PNG 页解码后走纯 zlib 压缩、没有 PNG 自己的逐行预测滤波，膨胀比预想的"5% 余量"大得多，实测约 1.45×，改用 1.5× 留余量）仍在上限内才调 `cbz_to_pdf` 生成 PDF，跟 CBZ 一起落母版库（两个独立文件，同一本书两种产物，复用现有"母版库同一本可投不同读者"模型，网页不用改）。超限只出 CBZ，跟以前一样，**绝不触发 `pdfsplit` 分卷**（`run()` 的分卷循环对 `route=="comic"` 整个跳过）。新增 `--manga-ltr`（跨页拆分从左往右）、`--comic-native`/`--no-comic-native`（默认开，关掉就完全不生成投原生 PDF）两个 CLI 开关。

**真机验证通过**（拿真实《火影忍者》卷1 96 页 CBZ，过完整管线——comic_gray 拆跨页+裁白边+16灰 → 55.98MB 灰阶 CBZ → cbz2pdf → 80.97MB PDF）：设备真实 `nativeUploadLimitBytes` 查到是 157286400（=150MB，跟静态兜底值分毫不差）；80.97MB PDF 直接调 book-serve `/staging/deliver` 投原生成功，xochitl 正确解析出全部 192 页；设备生成的首页缩略图人工核对内容正确。

**"超限只出 CBZ、不分卷"分支复验（2026-09-09 真机）**：原计划设想"阿拉蕾①会是够小的测试样本"不成立——阿拉蕾①加了跨页裁切放大后灰阶体积反而涨过上限；这次拿阿拉蕾①与镖人两本实测都落进"太大只出 CBZ"分支，`shelf push --wait` 全程真机跑通：
- **阿拉蕾①**（171.2MB AZW3，1092 页）：拆跨页 0 页（本来就是正常单页书，符合预期）／16 灰 1085 页／保色 7 页；体积 171.2MB → **229.0MB**（母版库落库实测 240,213,756 字节，与控制台估算一致）——超过 150MB 上限，控制台打印"体积估算超原生上传上限（150.0MB），只出 CBZ（KOReader）——不强制分卷投原生"，母版库确认**只有一个 `.gray.cbz` 条目，没有伴生 PDF**。全程 1m57s。
- **镖人**（283.2MB AZW3，2473 页，§03t 最早报"投不了原生"bug 的那本）：拆跨页 **17 页**（首次真机验证到跨页拆分对真正的跨页图生效，之前只有火影忍者卷1 测过）／16 灰 2489 页／保色 1 页；体积 283.2MB → **686.9MB**（母版库落库实测 720,467,259 字节）——体积不降反涨超一倍，原因与阿拉蕾①同源：16 灰 Floyd–Steinberg 抖动后的位图是高熵噪点、Flate 压不动，叠加跨页拆分+放大后总像素数增加（§03t 早年"《镖人》mono 只缩到 3/4"那条踩坑记录的同一根因，这次在跨页拆分场景下更明显）；同样超过 150MB 上限，只落 `.gray.cbz`，没有 PDF。**17 处装订缝识别不出干净切割点，按"最空一列"猜测拆分并标记 `flagged` 建议人工核对**（如 `0090.jpg` 猜测切割点 x=624），符合设计预期（找不到干净装订缝时不会瞎切，而是标记出来）。全程 6m1s（页数是阿拉蕾①的 2.3 倍，处理时间约 3 倍，跨页识别+更多页数叠加）。
- 两次推送母版库前后核对（`GET /api/books/staging`）确认**全程只多出这两个 `.gray.cbz` 条目，没有任何 `.pdf`、没有分卷 part 文件**——`route=="comic"` 时代码里对分卷循环的跳过逻辑真机同样生效，不是只在离线单测里成立。至此"超限只出 CBZ 不分卷"分支真机坐实，§05 未闭环清空。

**离线测试**：`uv run --group calibre pytest shelf/host/tests/` 57→66（跨页拆分+白边裁切 +9）→71（体积门 +5）全绿；`cd shelf && cargo build --workspace && cargo test --workspace` 零警告全绿（新增 `cbz2pdf` bin）。

**代码落点**：`shelf/host/calibre/comic_gray.py`（跨页拆分/白边裁切/放大本体，`looks_like_spread`/`find_gutter_x`/`split_spread`/`content_bbox`/`crop_border`）；`shelf/crates/bookconv/src/convert/cbz.rs::cbz_to_pdf`（本体未删过）；`shelf/crates/bookconv/src/bin/cbz2pdf.rs`（新 CLI，风格照抄 `epub_optimize.rs`）；`shelf/host/shelf_cli/calibre_bridge.py::_cbz2pdf_bin/cbz_to_pdf`（定位编译产物照抄 `wash_epub.sh` 找 `epub-optimize` 的顺序：环境变量 `CBZ2PDF_BIN` 覆盖 → PATH → `shelf/target/release/` 兜底）；`shelf/host/shelf_cli/commands/push.py::comic_prepare/_native_limit_bytes`。

## 03ae｜网页 UI i18n 架子：主界面外壳 + 顶层导航（2026-09-09，部分真机验证）

用户要求"考虑增加 UI 页面 i18n，可配置各国语言文件"。调研坐实现状：`app.js` 约 383 行（52%）+ `ui.rs` 35 行是中文硬编码，且不少是"条件分支+变量插值"的复合文案，登录页/改密码页更是 Rust 端独立拼接的一套机制，机械抽取全部工作量大、风险高。跟用户核实范围后按推荐方案落地：**先搭架子 + 只迁移主界面外壳与顶层导航**，登录页/改密码页与各模块正文文案（字体/词典/壁纸/传书/笔记的具体内容、批量操作提示等）暂不动，留作后续候选。

**方案**：新增 `ui/locales/zh-CN.json`/`en-US.json`（扁平 key→字符串，先只有约 12 个 key：`app.title`/`nav.*`/`main.loading`/`tab.*`/`action.delete`/`list.empty`），继续走 `include_str!` 编译进二进制（延续"单文件零外链"部署架构，不用改 `build.sh`/`deploy.sh`/`install.sh`）。`ui.rs` 新增 `locale_json(lang)` 按语言码分发，不认识的语言码落中文（不是空白页）。`main.rs` 新增路由 `GET /ui/locales/{lang}`（路由只支持整段 `{param}`，不支持段内混 literal 后缀，前端仍按 `/ui/locales/zh-CN.json` 带扩展名请求，服务端自己剥掉 `.json`）——这条路由落在登录守卫保护范围内（`auth::is_public` 允许列表没加它），符合"只有登录后的主界面才需要它"这个实际使用场景。

前端 `app.js` 新增 `I18N`/`T(key)`（缺 key 兜底显示 key 本身，不留空白也不掩盖翻译缺口）+ `currentLang()`（`localStorage` 记住选择，缺省按 `navigator.language` 猜、猜不出落中文）。启动 IIFE 最前面先 `fetch` 语言包，再渲染顶层导航——`index.html` 骨架里的 `书架`/`改密码`/`CA 证书`/`退出`/`加载服务列表…` 几处文案，以及顶层固定 tab「传书」「管理」+ `TABS` 表里「笔记」「壁纸」两个模块 tab 标题（`xochitl`/`KOReader` 是产品专名不翻译，原样留着），全部改走 `T()`。顺手把两处真正跨模块复用的通用原子文案（`fillList` 的"（空）"占位、`delBtn` 的"删除"按钮）也收进语言表——这两处出现在字体/词典/壁纸等好几个列表页，属于"低成本、高复用"的迁移对象，不算破例扩大范围。`index.html` 新增一个 `<select id="langsel">` 语言切换器（中文/English），选择后存 `localStorage` 并整页刷新。

**没做**：登录页/改密码页（`ui.rs::login_page`/`password_page`，Rust 端 `format!` 直接拼 HTML，跟前端这套 key 查表机制是两套体系，需要专门设计）；各模块正文文案（字体上传提示、笔记整理页的复合条件文案、壁纸/传书等的操作说明——这些量大且很多是条件分支拼出来的完整句子，机械抽取风险跟这次的收益不成比例）；后端 `ServiceSpec.label`（`main.rs` 里各服务的中文标签，会出现在跨服务 `/api/services` 注册表里，影响面更广，这次没有覆盖）。

**离线**：新增语言文件 key 集合一致性测试（`locale_files_have_identical_key_sets`，防止漏改一份 JSON 导致运行时查不到 key）+ 语言回退测试（`locale_json_falls_back_to_chinese_for_unknown_lang`）；`cargo test -p shelf-gateway` 9→11；`node --check app.js` 通过；`cargo clippy --all-targets` 核对没有新增警告；`sh build.sh` 交叉编译 aarch64-musl 零警告通过。

**部分真机验证（2026-09-09）**：`curl` 真机确认三种情形都对——`/ui/locales/zh-CN.json`/`/ui/locales/en-US.json` 各自返回对应语言正确内容、`/ui/locales/fr-FR.json`（不认识的语言码）正确落回中文而不是 404/空白；主页 HTML 真机确认已经在served（`id="langsel"`/`id="applogo"`/`id="apptitle"` 等新标记都在）；服务部署后 `NRestarts=0`。**没做的部分**：浏览器里实际点开语言切换器、肉眼看文案真的切换成英文——这需要真实浏览器渲染，这次只做到"数据链路通"（服务端按语言码正确分发 + 前端资源已经served），没有做到"人眼看过切换后的页面"，跟这条线一贯的"前端可视渲染未经人眼确认"缺口一致。

## 03af｜网页 UI 人性化/触屏可用性一批小修（2026-09-09，离线）

三维审计（代码结构/业务闭环/UI）里 UI 那一路专门核查了 `shelf-gateway/ui/` 的人性化设计与手机端/PC端便利性，找到的确凿问题里最突出的一类是"关键解释信息只存在于 `title` 属性，触屏设备完全看不到"——真机是触屏设备，这类问题直接影响手机端能不能用明白。这次按审计报告的高/中优先级顺序打包修了一批：

- **禁用按钮的原因说明**（"投入原生书库"超限、"加入 KOReader"未安装）：以前只写进 `title`，`btn()` 辅助函数补了一行可见小字（`flex-basis:100%` 独占一行，不挤在按钮同一行）。
- **徽章的完整解释**（渲染自检失败、"已优化·未清洗"、"已投原生·旧"…）：全局委托一个点击处理——任何带 `title` 的 `.badge` 点一下就 `alert` 出完整内容，配 `cursor:help` 给一个"这能点"的视觉提示，不用逐个改模板字符串，覆盖全站现有及未来的同类徽章。
- **"重扫"按钮补二次确认**：全站唯一一处"文案暗示有代价但没拦截"的按钮，补一句说明数据风险其实不大（已校对内容不会丢）。
- **fetch 非 JSON 响应的兜底错误文案**：从裸的"HTTP 状态码"改成一句人话+括号里的状态码。
- **通用空状态"（空）"补引导**：`fillList` 新增可选 `emptyMsg` 参数，KOReader字体/词典/资产页（字体/壁纸/词典）各自传一句"去哪里做什么"，不再是四个不同页面共用一个没有引导的占位符。
- **响应式**：说明表格（`table.cmp`）加窄屏媒体查询去掉 `min-width:34em`（用量表之前单独修过这个问题，说明表/优化档位表被漏改）；上传队列"×"移除按钮/`.subnav` 二级标签/头部纯文字链接的触控热区都偏小，补了 padding/最小尺寸（`.subnav` 没有直接顶到 44px 建议值——两层叠着用时会占太多屏幕，跟"整理"页几轮反馈压密度的方向冲突，是密度和热区之间的折中）。
- **可访问性**：传书页几个 select/input（文件夹/筛选）补 `label`/`aria-label`；壁纸缩略图从 `alt=""`（装饰性）改成描述性 `alt`；上传队列"×"按钮补 `aria-label`。
- **母版库列表主次视觉**：`.btn-bad` 早就存在但只用在转写失败按钮上，这次给"删除"（销毁性操作）也用上，跟"优化"（编辑性操作）在颜色上区分开——单行最多堆 5 徽章+4 按钮时这点区分很必要。

**离线**：`node --check app.js` 通过；`cargo test -p shelf-gateway` 11 个测试不受影响（纯前端改动，无 Rust 契约变化）；`sh build.sh` 交叉编译 aarch64-musl 零警告通过。

**⚠️ 还没真机验证**：这批全是前端改动，跟这条线一贯的缺口一样——没有浏览器截图核对实际渲染效果，只确认了代码逻辑本身（哪些是代码事实、哪些需要人眼确认，审计报告里已经分清楚）。

## 03ag｜消掉 book-serve 两个队列的重复：新增 `PendingQueue<T>`（2026-09-09，离线）

三维审计的代码结构那一路发现 `book-serve::trash.rs`（原生回收站队列）和 `mkdir.rs`（原生建文件夹队列）逐行重复：两者都是 `struct { file, lib_dir, lock }`，`load()`/`save()` 字节级相同（读 JSON、原子写），`add()` 都是"校验 → 查重 → push → save"，`pending()` 都是"按 `.metadata` 状态过滤 → 剔除已完成项 → save → 返回 (剩余, 剔除数)"——跟笔记线的 `ChapterStore<T>` 是同一类"形状相同、只有记录类型和校验/剔除谓词不一样"的重复，这次没被套用到这两处。

**修复**：新增 `book-serve::pending_queue::PendingQueue<T>`，只抽持久化+入队去重+剔除这层通用外壳（`new(file)`/`list()`/`add(exists, make)`/`prune(keep)`）；`TrashQueue`/`MkdirQueue` 各自只留领域校验（uuid 形状/名字核对/是否已在回收站；文件夹名合法性/是否已存在）包一层 `PendingQueue<Pending>`。`add()` 里有个借用检查器的小坑：两个闭包（`exists`/`make`）都要用到同一个 `uuid`/`name`，如果先转成 `String` 再共享，第一个闭包借用、第二个闭包要移动会冲突——保持参数原样的 `&str`（`Copy` 类型）让两个闭包各自拷贝一份就没这问题，写进了 `trash.rs` 的代码注释里。

**离线**：`pending_queue` 新增 2 测（入队去重+跨实例持久化、剔除后跳过无谓写盘）；`TrashQueue`/`MkdirQueue` 原有测试全部不变全绿（纯内部实现重构，不改变可观察行为）；`cargo test --workspace` 全绿零警告；`cargo clippy -p book-serve` 核对没有新增警告；`sh build.sh` 交叉编译 aarch64-musl 零警告通过。

## 03ah｜`shelf-gateway::proxy` 模块文档修正："body 流式透传"跟实现不符（2026-09-09，离线）

三维审计发现模块顶部注释写"body 流式透传"，实际只有请求方向真流式（`send(&mut *req.body)`），响应方向整体缓冲进内存（`into_reader().read_to_end(...)`）。评估过要不要顺手改成真流式（`shelf_core::http::Reply::stream`），结论是**不改行为，只改注释**——那套流式通道底层走 `tiny_http` 的 `upgrade("sse", ...)` 直接接管裸 socket，是专门给 SSE（`/api/events`）设计的，拿去代理任意大小的下载响应之前要先确认对非 SSE 场景是不是语义正确（有没有 Content-Length/chunked 头协商），这条低优先级项本身的收益不值得为此承担这份不确定性。详细评估记在笔记线白皮书 §03aj（这次是笔记/书架两条线一起收尾的一批低优先级审计项，字符截断函数消重复那部分是纯笔记线代码，写在那边）。

**离线**：纯注释改动，`cargo test -p shelf-gateway` 不受影响；`sh build.sh` 交叉编译 aarch64-musl 零警告通过。

## 03ai｜`shelf-core::registry` 新增 `SvcClient`/`enc`（2026-09-09，离线，代码改在这边、消费方全在 notes 那条线）

三维审计的代码结构那一路发现 `notes/` 三个服务（`mind-serve`/`note-serve`/`transcribe-serve`）各自的 `ink.rs`（访问 ink-serve 的 HTTP 客户端）、`note-serve::trash.rs`（访问 book-serve 回收站队列的客户端）——四处 `struct{paths,agent}`、`new()`、`base()`、`get_json()`、`enc()` 几乎逐字节相同。这条重复只能在 `shelf-core` 修：`registry` 本来就管服务发现，加一层"按发现结果建客户端"是同一职责的自然延伸，且笔记线四个消费者已经全部依赖 `shelf-core`。新增 `SvcClient::new(paths, service, timeout_secs)` + `base()`/`get_json()`/`post_json()` + 逃生舱 `agent()`（`transcribe-serve::crop()` 要下载原始字节不是 JSON，走这个）；顺手把 `enc()` 也收成 `registry::enc()`（薄封装 `multipart::percent_encode`）。

**这次改动的落点在 `shelf/crates/shelf-core/src/registry.rs`，但当前 shelf 自身代码（`services/`、`crates/bookconv`）零处调用 `SvcClient`**——纯粹是为了接住 notes 线的消重复需求；四个消费者改造、具体重复证据、错误文案格式等细节记在 `notes/docs/reMarkable笔记白皮书.md` §03ai（两边巧合用了同一个字母，不是同一节，注意区分：那边是"消费方视角"，这边是"shelf-core 这次多了什么"）。这条延续了 §03ac"笔记线自己的架构决策记在它自己的白皮书，本文只记'它那边为了接住我们、改了什么'"的一贯原则，反过来也成立——shelf-core 为了接住 notes 的需求改了什么，本文也该记一笔，不能完全空白。

**离线**：`shelf-core` 新增 2 测（`svc_client_base_url_uses_registry_and_errors_with_service_name_when_not_running`/`enc_percent_encodes_path_segments`）；`cargo test --workspace`（shelf 51 个）零回归；`cargo clippy -p shelf-core` 无新增警告；`sh build.sh` 交叉编译 aarch64-musl 零警告通过。

## 03aj｜「管理」拆二级 tab + 系统增强开关上网页（2026-09-09，真机通）

用户对「管理」tab 提了四点：① `shelf push` 卡片挪去「传书」页；② 原来挂在「管理」最下面的「模型管理」卡片单独拆成二级 tab；③「管理」再加一个二级 tab 放"系统增强工具开关"（cjk 画线吸附/cjk 手写笔迹优化/电池刺客）；④ 统一视觉风格，command 说明块看不清。

**①②纯前端重排**：`renderTransfer()`「入库」子面板追加第三块 `shelf push` 说明（原样复用例子/强在哪/怎么装，去掉"命令见「管理」页"的间接引用）；`renderManage()` 顶部加 `.subnav`（复用 `subtabs()`），拆「🏗 基石与模块」/「🧠 模型管理」/「⚙️ 系统增强」三个二级 tab，模型管理卡片（`mountModelPanel` 两个挂载点）原样搬进第二个 tab。

**③是本轮唯一有真实后端工作量的点，三个"开关"现状差异很大**：
- **cjk 画线吸附**＝`reading-qol.json` 的 `hlSnapCjk`（默认开，langhook C hook 消费，见系统增强白皮书 §04）。**有地基，做成真开关**。
- **cjk 手写笔迹优化**：用户明确澄清是"设备手写笔锋按 CJK 书写习惯（运笔粗细/顿挫）渲染优化"，**跟 AI/大模型识别（`cardhw`）完全无关**（第一轮理解错了，以为是 `cardhwEnabled` 那个视觉转写开关）。全仓库搜索确认这个渲染优化功能**目前完全不存在**：没有配置键、没有 hook、没有反编译记录，唯一沾边的"够不够到 xochitl 原生渲染层"先例（笔记页背景滤镜想接近同一层）是**判死**的（：C++ `SceneView` tile 增量渲染够不到）。真要做需要独立立项做原生笔画渲染层的逆向工程，不是包一层网页开关就能上线的。**这次做成「未上线」占位卡片**，不接后端，等真正探路完成再回来接。
- **电池刺客（battop）**：原生设置页只有只读展示（读 `summary.json`），**从没有过开关**——这是它第一次有开关。2026-08-29 出过 cgroup/RCU 死锁（`enhance/battop/FINDINGS.md`），已修复为常驻 `Type=simple`（不再靠 timer 反复拉起 oneshot触发 cgroup 迁移），单纯 `systemctl start/stop` 不重现那次事故的触发条件（触发条件是"反复重启"，不是"启动过一次"）。**有地基，做成真开关**。

`reading-qol.json` 有全量写回铁律（系统增强白皮书 §08）：新写 `shelf/services/shelf-gateway/src/enhance/qol.rs` **不照抄 QML 那种手写全部字段的方式**——`patch()` 把整份文件当成不透明的 `serde_json::Map` 读进来，只覆盖调用方明确要改的键，其余原样写回，天然不怕将来别处新增字段导致这里漏改。`enhance/battop.rs` 状态探测（unit 文件是否存在 + `systemctl is-active`）+ 开关（复用 `manage::run` 这个 `pub(crate)` helper，不重新实现一遍 `Command` 样板）。

**目录结构**：`enhance.rs` 没做成单文件，改成 `enhance/{mod,qol,battop}.rs` 目录——用户主动问"以后可能会有很多系统增强能力，要不要单独文件夹"，权衡后**没有**升到独立 service/crate 那一级（这两个能力都是同机文件 I/O / `systemctl` 直调，没有独立进程边界的理由，硬拆一个新服务只会多一层 IPC/注册/代理开销），但目录级别的分文件是低成本的，以后每加一个新能力就是加一个新文件 + `mod.rs` 挂一个路由，不会让某个文件越滚越大。

**④命令说明块视觉语言修正**：根因不是背景色对比度（第一轮猜错了）——`.opt-note` 和「笔记」页的 `.entry-quote`（划线摄取转写后显示的引用摘录）结构几乎一样：同量级小字号（`.82em`/`.85em`）、同 `color:var(--mute)`、同浅底、同左侧色条。命令是要人读要人抄的可操作内容，借用"安静小字引用摘录"的视觉语言会显得不起眼、字也偏小。新增 `.cmdblock` 组件：字号跟正文同级、等宽字体、整块实线边框（不是引用框那种只有左侧色条），跟 `.entry-quote` 拉开区分度。

**真机验证（用户密码授权，curl 功能性测试）**：部署走完整备份→scp→md5 校验→`systemctl restart`→健康检查流程（`is-active=active`、MainPID 从 110918 变 120674、`NRestarts=0`、日志无崩溃）。功能性：`PUT /api/enhance/qol {hlSnapCjk:false}` → 设备上 `cat reading-qol.json` 确认其余 6 个键（`tapPageTurn`/`fastMono`/`refresh`/`refreshByChapter`/`refreshEvery`/`fontEnhance`）原样保留，只有 `hlSnapCjk` 变了；再 `PUT {hlSnapCjk:true}` 复原。`POST /api/enhance/battop/start`（该设备未装 battop）返回干净的 400 错误文案，不崩不 500。未登录状态下 `/api/enhance/status` 干净 401（确认新路由接进了鉴权守卫，没有意外裸露）。命令块字体清楚了（用户确认）。⚠ 用户顺手测试划线吸附（`hlSnapCjk` 全程未变、文件 mtime 事发前后都是 2026-09-02，跟这次改动无关）没观察到效果——见系统增强白皮书 §04 的独立追记，需要单独重新验证那个 C hook 现在还生不生效，不在这轮任务范围内。

**离线**：`shelf-gateway` 新增 2 测（`hl_snap_cjk_defaults_true_when_missing`/`patch_preserves_unknown_keys`）；`cargo test -p shelf-gateway`（13 个）零回归；`cargo clippy -p shelf-gateway --all-targets` 无新增警告；`sh build.sh` 交叉编译 aarch64-musl release 零警告通过。

## 03ak｜「管理」加「实验室」二级 tab + 独立「电池刺客」标签页 + 「导入 md 文档」改文件上传（2026-09-10，真机通）

§03aj 那次「CJK 手写笔迹优化」还是纯占位卡片（"完全没有代码地基"）——`enhance/handwriting-stroke/` 这条线后来（同一天另一个会话）真机验证出两个 hook 目标（`FUN_00f47530`+`FUN_00f4c8d0`，见系统增强白皮书 §03e-§03f），有真实可用的效果了。用户这次提了五条：①「管理」新开「实验室」二级 tab，把「CJK 手写笔迹优化」「电池刺客」从「系统增强」搬过去；②新增「导入 md 文档」开关，控制「笔记」tab「导入」子标签的显示；③完成 CJK 手写笔迹优化开关；④完成电池刺客开关+独立标签页；⑤「导入」改名「导入 md 文档」、交互从打字改成上传文件。

**两个设计取舍先讲清楚**（都是权衡过风险/工作量后拍的，不是唯一解）：
- **CJK 手写笔迹优化开关纯网页层实现，不碰设备端 C 代码**：`hw_stroke.c` 没有独立的"开关"字段，`hwStrokeNibMinRatio`/`hwStrokeSpeedMinRatio` 两个强度阈值本身就是"1.0=关、<1.0=开"的语义。`qol::hw_stroke_enabled()` 派生读（只读 `hwStrokeNibMinRatio` 一个键判断），`mod.rs::set_qol()` 收到 `hwStrokeEnabled` 时同步写两个 ratio（开＝`0.6`，真机之前验证过的强度；关＝`1.0`），角度/宽度/速度阈值几个精调字段不碰，留给手改 `reading-qol.json`。代价是网页开关关了再开会把手动精调的值冲成固定 `0.6`——这个边界情况接受，换来的是零设备端改动、当天就能上线。
- **「导入 md 文档」不做成真正的 multipart 上传**：新交互是"选一个 .md 文件"，但前端用 `FileReader.readAsText()` 在浏览器里把文件读成字符串，**继续走原来的 JSON POST**（`{title,markdown}`），`note-serve` 的路由/`publish::import_markdown()` 一行都没改。用户角度"选文件"和"打字"的体验差异已经做到了，没必要为了这层用户看不见的传输方式差异去碰一个已经真机验证过的端点，多一层没必要的风险面。

**「实验室」二级 tab**：`renderManage()` 的 `subnav`/`subpanel` 从三个扩到四个（跟 §03aj 同一套 `subtabs()` 机制，纯加一对）。「系统增强」只留 CJK 画线吸附（`hlSnapCjk`，真正"系统级"、默认开的东西）；「实验室」放"还在打磨/覆盖面没到日常好用程度"的功能：CJK 手写笔迹优化开关（真开关）、电池刺客卡片（下移）、导入 md 文档开关（新，默认关——新功能第一次上线不想让用户点开笔记 tab 就撞见半成品）。

**电池刺客卡片抽成 `mountBattopCard(container)`**：因为这次它要同时出现在两个地方——「实验室」子标签和新的独立顶层「电池刺客」标签页。battop 不是 `/api/services` 注册表里的 service（独立 systemd unit，不走服务反代），走「传书」「管理」那种手动 `addTab()` 固定注册，不能靠 `svcs.forEach` 动态生成；两处各自独立 `refresh`，不共享内存态，是这个 app 里 tab 之间一贯的模式。**这次只搬现有状态卡片**（启停按钮+最近采样时间），不解析 `summary.json` 里"类 htop"的当日/7天/30天/全部时间窗详细数据——那是独立的后续任务，用户明确拍板范围排除。

**「导入」→「导入 md 文档」，textarea → 文件选择控件**：`<textarea>` 换成 `<input type=file accept=".md,.markdown">`，`onchange` 读文件（`FileReader`）、顺手拿文件名当默认标题（仍可编辑）。子标签的显示/隐藏跟 `notesImportMdEnabled` 走——按钮+`subpanel` 用 `hidden` 属性（不是从 DOM 移除，`subtabs()` 是纯位置下标配对，移除会让后面的子标签错位），挂在笔记 tab 自己的 `refresh()`（每次从别的 tab 切回来都会调，不用额外接 SSE）；开关被关掉时如果正好停在「导入」子面板，先 `.click()` 切回「浏览」再设 `hidden`（避免 `.subpanel.on` 类跟 `[hidden]` 同时存在——author 样式表的 class 选择器会赢过浏览器 UA 样式表给 `[hidden]` 的默认 `display:none`，顺序反了会看见"隐藏了按钮但面板还在"）。

**真机验证（curl 功能测试，用户口头授权+密码）**：完整流程 `build.sh`（host workspace 全测试 21 个 crate 全绿 + aarch64-musl 交叉编译）→ `deploy.sh`（备份→scp→装→`shelf.target` 9 服务全部重启）→健康检查（`is-active=active`、MainPID 变化、`NRestarts=0`、启动日志无报错）。功能性：`GET /api/enhance/status` 返回四个字段（`hlSnapCjk`/`hwStrokeEnabled`/`notesImportMdEnabled`/`battop`）；`PUT /api/enhance/qol {hwStrokeEnabled:false}` → 真机 `reading-qol.json` 确认 `hwStrokeNibMinRatio`/`hwStrokeSpeedMinRatio` 变 `1.0` 且其余 6 个 hwStroke 阈值字段+`hlSnapCjk`+其它 4 个跟这次无关的键原样保留（全量防覆盖验证过）；`{hwStrokeEnabled:true}` 复原为 `0.6`；`{notesImportMdEnabled:true}` 单独写入新键成功。`POST /api/notes/books/{uuid}/import-md {title,markdown}`（模拟前端 `FileReader` 读出的内容）→ 真机 `xochitl` 目录确认生成完整 `.metadata`/`.content`/`.rm`/`.thumbnails`，笔记本真实可用。用户真机写字确认 `journalctl -u xochitl` 里 `[hw-stroke:f4c8d0]` 日志持续、开关状态跟 `reading-qol.json` 一致（那次笔画 `w≈3` 低于 `hwStrokeNibWidthLow=5.0` 阈值、`nib_ratio`/`speed_ratio` 仍是 `1.0`——这是既有的"细笔画自动排除"设计生效，不是这次开关失效，需要用更粗的笔才能肉眼看到比例变化）。

**用户真机测试顺手发现一个遗留 bug**：battop 卡片"未装"提示文案还写着 `misc/battery-audit/battop/install.sh`——这个路径是 2026-09-09 之前的，battop 早就 `git mv` 到 `enhance/battop/` 了（§03b），这次改动只是原样把旧文案挪了地方，没检查内容是不是还准。改成 `enhance/battop/install.sh`，重新交叉编译+部署确认（第二次 MainPID 变化、`active`）。**全仓库搜索确认这是唯一一处"指导操作"的路径引用还停留在旧位置**（另外三处是文档里"git mv 自 misc/battery-audit/battop/"这类历史陈述，本身准确，不用改）。

**离线**：`shelf-gateway` `qol.rs` 新增 3 测（`hw_stroke_enabled_defaults_false_when_missing`/`hw_stroke_enabled_true_when_ratio_below_one`/`notes_import_md_enabled_defaults_false`）；`cargo test -p shelf-gateway`（16 个）零回归；`node --check app.js` 语法过；`ui/locales/{zh-CN,en-US}.json` 各加一个 `tab.battop` key（顶层 tab 是这套 i18n 架子唯一覆盖的范围，新顶层 tab 照此惯例走 `T()`，其余卡片/子标签文案跟旁边现状一致继续硬编码中文不新增 key）；`locale_files_have_identical_key_sets` 测试确认两份文件 key 集合仍然一致。

**用户当天追问了两点，都是对的，回来改**：

1. **"反复启停 battop 是不是会触发 cgroup 死锁死机"**——重新核对 `FINDINGS.md` 原始事故记录，`battop.rs`/网页文案原来的"这里的开关只是一次性启停，不会重现那次事故的触发条件"这句话**没讲清楚边界**：真正触发崩溃的是`cgroup_procs_write`（进程启动时被 systemd 塞进 cgroup 这个动作）撞上内核罕见的 RCU stall——**这个内核问题本身没有被修复，是概率事件**；旧架构靠 `battop.timer` 每 10 分钟重启一次，把这个动作干到 144 次/天，144 倍放大了撞上那个罕见窗口的概率；新架构只在真正启动那一刻迁一次 cgroup，**是把触发频率降下来，不是把这类风险归零**——网页上点"启动"依然是同一类 cgroup 迁移操作，正常偶尔点几下风险可以忽略，但如果短时间内在网页上反复连点启停（现在这个按钮没做任何防连点/限流），理论上就是在人为复现旧 timer 那种高频触发条件。把这条边界写进 `battop.rs` 头注和网页卡片文案，不再只说"不会重现"。
2. **"battop 启动才应该显示电池刺客标签"**——检查「管理」页自己的帮助文案（"已装·未开...网页看不到它的功能"、"已开...顶部有它的标签页"）确认：字体/KOReader/壁纸/笔记这些服务 tab 本来就是"运行才出现"，独立顶层「电池刺客」标签页固定常显是这次设计跟既有约定不一致，用户指出来是对的。改法：页面 IIFE 里 `addTab` 电池刺客之前先并行拉一次 `/api/enhance/status`，只有 `battop.running` 才 `addTab`；`battop_toggle()` 成功后 `hub.bus.publish("manage","battop")`（battop 不在服务注册表里，原来的"注册表目录 inotify → manage 事件"这条通路碰不到它，得手动发一次），前端 SSE 的 `ev.area==='manage'` 比较逻辑把 `battop.running` 拼进原来只比较 `/api/services` 的那个 key 里，运行态一变就跟服务启停一样触发整页重载，标签页才会跟着出现/消失。⚠️ **这条一天之内就被推翻了**：同一天用户后来直接拍板"电池刺客降级移入管理"——独立顶层标签页整个撤掉，这套"运行才 addTab+SSE 事件"机制跟着一起撤（见 §03al），battop 现在只活在「管理→实验室」的卡片里，不再是顶层 tab。

## 03al｜首层标签重排（传书/笔记/其他/管理）+「入库」拆三卡 + battop 真机装机验证 + 电池审计数据表接入（2026-09-10，真机通）

同一天用户又提了四条，一次性做完：

**① 首层标签重排为「传书/笔记/其他/管理」**：原来"传书 + 服务注册表动态生成的几个 tab（xochitl/KOReader/笔记/壁纸，按 order 排）+ 管理"这套顺序，改成固定四段——「笔记」单独占第二位，xochitl(font-serve)/KOReader(koreader-serve)/壁纸(wallpaper-serve) 全部降一级包进新的「其他」tab 当二级子标签（`renderOther(sec,svcs)`，只列当前真的装了/在跑的那几个，跟原来"没装就不出现"的规则一致，只是从"顶层不出现"变成"其他里不出现这一项"）。

**踩了一个真实的嵌套坑**：KOReader 自己的 `render()` 内部本来就有一层字体/词典 `subnav`，包进「其他」之后变成三级嵌套（其他 tab 的 subnav → KOReader 子面板 → KOReader 自己的字体/词典 subnav）。共用的 `subtabs(sec)` 辅助函数原来用 `sec.querySelectorAll('.subpanel')`（不限定层级）找面板——嵌套之后会把 KOReader 自己的两个内层 subpanel 也扫进来，外层按钮数（3 个：xochitl/KOReader/壁纸）跟扫到的 panel 数（3+2=5 个）对不上，点哪个都错位。改成 `:scope > .subpanel`/`:scope > .subnav`（只找直接子元素）——这是对现有所有调用点都**无副作用**的收紧（没有嵌套的场景，直接子元素和"随便哪层"结果完全一样），顺手把这类潜在坑一次性堵死，不是专门为这次改的特例分支。

**② 「传书 → 入库」三个入库来源拆成三张独立卡片**：原来"上传/抓网文/电脑端 shelf push"挤在同一张 `.card` 里用 `<h3>` 分隔，看着像"上传"带了两个附属步骤；用户反馈这是三条互不依赖、各走各路的入库路径，该有三张卡片的视觉分量——跟「系统增强」/「实验室」那种并排卡片同一个语言，纯 HTML 结构调整，逻辑零改动（`uploader($('.up',sec),...)` 等选择器都是 `sec` 范围查找，跟卡片怎么分组无关）。

**③ battop 真机实装，走完整闭环**：交叉编译 `enhance/battop`（musl 全静态）→ scp 二进制+`install.sh` → 真机跑 `install.sh`（`mount rw` → 写 `/usr/lib/systemd/system/battop.service` → `enable --now` → `mount ro`，这条路径本来就是 battop 自己一直在用的既有安装方式，不是新踩的坑）→ `systemctl is-active`=`active`。curl 验证 `/api/enhance/status` 的 `battop.installed/running` 从 `false` 变 `true`，`start`/`stop` 循环两轮都确认状态正确翻转、`lastSampleAt` 每次都是新时间戳（不是缓存旧值）。

**④ 电池审计数据真的接上了**：用户看了纯开关卡片后反馈"应该呈现跟以前设备端电池审计对标的网页版，不是文字描述开关功能"——查了 `enhance/battop/src/main.rs::write_summary`，`summary.json` 本来就是"4 个时间窗（今日/7天/30天/全部）× 应用/进程/唤醒源 top15”的预聚合（battop 自己算好，不是网页现算），新增 `battop::summary()` 读取 + `GET /api/enhance/battop/summary` 原样转发；`mountBattopCard` 卡片里加一层时间窗 `subnav`（今日/7天/30天/全部），每个窗口展示放电%/mAh/均值mA/采样数 + 按应用累计时长 top 列表 + 唤醒源计数列表——真机 curl 确认端点返回真实数据（`generated`/`windows.{today,7d,30d,all}` 齐全，装上没多久已经能看到 `xochitl`/`koreader-serve`/`memfaultd` 等真实条目）。

**⑤ 电池刺客独立顶层标签页撤销**：同一批反馈里用户直接拍板"电池刺客：降级移入管理"——上一版（§03ak 追记②）刚做完的"运行才 addTab + `hub.bus.publish` 触发整页重载"这套机制整个撤掉，`battop_toggle()` 恢复成不需要 `events::Hub` 的单一函数，路由绑回 `&paths`；`renderBattop`（独立 tab 的薄包装函数）删除，`mountBattopCard` 只保留在「管理→实验室」这一个挂载点。`tab.battop` 这个 i18n key 也一并删掉（无引用了），新增 `tab.other`。

**真机验证**：`cargo build`/`cargo test -p shelf-gateway`（16 个）全绿，`node --check app.js` 语法过，交叉编译+部署+健康检查（`is-active`/`NRestarts=0`）；curl 确认 `/api/enhance/battop/summary` 返回真实聚合数据；「其他」tab 嵌套结构（三级 subnav）**逻辑推演过、`:scope >` 收紧过、装置健康**，但"浏览器里点开其他/KOReader 子标签，字体/词典切换正常工作"这个纯前端交互细节没有拿到用户的肉眼确认，如实记在这——理论分析认为没问题（`:scope >` 精确限定了各层各自的查询范围），但按项目纪律不能替代真人点一遍的确认。

## 03am｜电池刺客再拆分（二级 tab + 按进程）+ 总标题改「秘密花园」+ 端口改 443（2026-09-10，真机通）

同一天用户又提了四条：

**① 电池刺客第三次调整落点**：§03al 那版是"实验室卡片里直接放时间窗+应用+唤醒源"，用户这次要求"实验室只留开关和说明，打开后「管理」多一个二级 tab「电池刺客」，下面两个三级 tab：耗电情况、唤醒源"。`mountBattopCard` 拆成两个函数——`mountBattopToggleCard`（实验室卡片，纯 `<label class="toggle">` 开关，checked 直接对应 systemd 的 running 状态，onchange 打 `/api/enhance/battop/{start|stop}`，不再挂状态 kv/单独按钮）+ `renderBattopDetail`（新内容，管理页第 5 个二级 subpanel，只有 `battop.running` 时才显示——显隐规则/实现原样照抄「笔记」tab「导入 md 文档」子标签那套 hidden 属性+"当前若在被隐藏的标签上先点回默认标签再设 hidden"写法，不是新发明一套）。

**嵌套到了四层**：管理 subnav（1）→ 电池刺客 subpanel → 耗电情况/唤醒源 subnav（2）→ 对应 subpanel → `renderBattopWindowed` 自己的时间窗 subnav（3）→ 时间窗 subpanel。`subtabs()` 之前已经改成 `:scope >` 限定直接子元素（§03al 那次为了「其他」tab 三级嵌套改的），这次直接复用，没有再踩新坑——每一层只认自己的直接子节点，层数再深也不会互相干扰。

**② 耗电情况补齐"按进程"**：`summary.json` 本来就有 `proc`（按 comm 分组）跟 `app`（按 systemd unit/友好名分组）两个数组，§03al 那版详细视图只展示了 `app`，这次在"按应用"列表下面加一段"按进程"，`battopTopList(d.proc)` 复用同一个渲染 helper，零新增数据处理。

**③ 总标题「书架」改「秘密花园」**：改的是**用户可见的字符串**，不是仓库目录/crate/二进制名（`shelf/`/`shelf-gateway`/`shelf-core` 这些开发侧命名原样不动，只是显示文案）。改动点：`ui/locales/{zh-CN,en-US}.json` 的 `app.title`、`ui/index.html` 静态 `<title>`/`<span id=applogo>`（JS 加载完会被 `T('app.title')` 覆盖，但改这里让 JS 跑起来之前的瞬间文案也对得上）、`src/main.rs` 的 `ServiceSpec.label`（这个字段会经 `/api/services` 出现在「管理→基石与模块」的模块列表里，是用户能看到的第二处）、`src/ui.rs` 里服务端直接拼的登录页/改密码页（这两页不走 SPA 的 `T()`，是 Rust 端 `format!` 拼死的字符串，之前审计这块 i18n 架子时就标注过"正文文案暂不迁移"，这次连着一起手动改了，不然会出现"主界面叫秘密花园，登录页还叫书架"的割裂）。**图标**：`style.css` 里 `.logo::before{content:"📚"}` 换成 `"🌿"`；另外这次顺手给页面**新增**了一个之前完全没有的 `<link rel="icon">`（之前浏览器标签页图标是空的/浏览器默认图标，属于"顺手补齐"不是"替换"）——emoji 内联 SVG data URI，零外部请求零构建步骤。

**④ 网关端口从 8778 改绑标准 443**：真机 `ss -tln` 确认设备上没有任何进程占用 443（`10.11.99.1:80` 是 xochitl 自己的 `/upload` 监听，跟 443 不冲突），`shelf-gateway.service` 本来就没写 `User=`（systemd 默认跑 root），绑 443 这个特权端口不需要额外提权配置。只改了**部署单元**的 `ExecStart --bind`（`shelf/systemd/shelf-gateway.service`），`main.rs` 里 CLI 的 `default_bind` **刻意留在 8778**——本地 `cargo run` 手动跑不用配 root 权限就能测，部署这条路径已经靠 systemd 单元显式 `--bind` 覆盖了默认值，两者不冲突。连带改了 `deploy.sh`（HTTPS 探测 URL）、`install.sh`（安装完打印的地址）、`mdns.rs`/`ui.rs` 的文档注释、`README.md` 三处地址引用。真机验证：`curl https://10.11.99.1/api/enhance/status`（不带端口号）返回真实数据；`curl https://10.11.99.1:8778/...` 确认连接被拒（旧端口真的关了，不是留了个仍然监听的旧进程）。

**真机验证**：`cargo build --workspace`/`cargo test -p shelf-gateway`（16 个）全绿，`node --check app.js` 语法过；交叉编译全 workspace（6 个 crate）+ `deploy.sh` 走完整备份→部署→健康检查（9 服务 active，`shelf-gateway` MainPID 变化、`NRestarts=0`）；curl 确认新标题/图标/443 端口都生效（`<title>秘密花园</title>`、`<link rel="icon">` 内容正确、`/api/services` 里 `shelf-gateway` 的 `port:443`、旧 8778 连接被拒）。**浏览器里"点开管理→电池刺客二级tab→耗电情况/唤醒源两个三级tab切换是否顺畅"这个纯交互细节，跟 §03al 那次「其他」tab 嵌套一样，没有拿到用户肉眼确认**——理论分析 + curl 数据层验证都过了，按项目纪律仍要如实标注这条缺口。

**同一天追加两条小调整**：① 「耗电情况」的"按应用/按进程"改成下拉选择（`<select>`），不再两份列表一起摆——下拉放在时间窗 `subnav` 外面（跨时间窗持续存在，不随点时间窗按钮重建），切下拉时用新增的 `battopActiveWindowIdx()` 读一遍当前选中的是哪个时间窗，`renderBattopWindowed()` 也跟着加了 `activeIdx` 参数，重画列表内容时把原来选的时间窗原样传回去，不会因为切了下拉就把时间窗选择弹回"今日"。② 「电池刺客」二级 tab 挪到「实验室」前面（按钮+对应 subpanel 一起挪，下标 `manageNav.children[3]` 跟着改），纯 DOM 顺序调整。③ 用户指出电池刺客详情页跟其它页面风格不统一——之前 `renderBattopDetail` 的内容是"裸"的（直接躺在 subpanel 里，没有 `.card` 包边框/底色/阴影），补齐：`renderBattopWindowed` 每个时间窗的内容包一层 `.card`（跟 KOReader 字体/词典两个子标签各自一张卡同一个规矩）；耗电情况/唤醒源各自开头加一张说明卡（标题+一句话介绍，跟「系统增强」那些卡片一个语言）——"按应用/按进程"下拉放进耗电情况这张说明卡里（下拉要跨时间窗持续存在，不能塞进每次切时间窗都可能重画的 `renderBattopWindowed` 内容里，这条边界这次顺手理清楚，之前是含糊的）；"还没有采样数据"的空状态提示也包了 `.card`。

## 03an｜网页正文全量 i18n（传书/笔记/其他/管理四个 tab）+ 两处小样式修复（2026-09-10）

同一天先处理两个小请求，再接到本节主体：

**① 「模型管理」子标签拆卡统一风格**：这个子标签原来是"说明卡（h2+lead）把 `#modelcards` 容器整个包在自己里面"，模型卡（视觉/文字两张）嵌在说明卡内部——是「管理」页里唯一卡中卡的地方，跟「传书·入库」「引导·基石」那种"说明卡起头，后面各功能各自平铺一张卡"的既有语言不一致。改成说明卡补 `<h2>` 标题、跟 `#modelcards` 平级。纯 DOM 结构调整，`mountModelPanel` 内部逻辑一字未动。

**② 模型卡「最近错误」行文本溢出页面**：`data-stat` 显示上游 API 报错原文，可能是没有空格可断行的长串（URL/JSON 片段），默认换行规则遇不到断点就不换行，把 flex 卡片撑宽、页面横向溢出。补 `overflow-wrap:anywhere`——比常见的 `word-break:break-word` 更彻底，连浏览器算 flex 子项 min-content 尺寸时都会把这个属性纳入考虑，不只是视觉换行。

**③ 主体：网页正文全量 i18n**。用户明确要求"完成 shelf/notes/enhance 涉及的各前端功能页面的 i18n"——§03ae 当时的"先只覆盖主界面外壳+顶层导航，正文文案暂不迁移"这条边界这次要补完。探查规模比预想大：`app.js` 1023 行里 526 行含中文，散布在 10 个渲染函数/区块（传书 `renderTransfer`/`stagingList`、笔记 `renderNotes`、其它 `renderOther`/`assetTab`/字体/KOReader/壁纸、管理台 `renderManage`、模型管理 `mountModelPanel`、电池刺客三个函数），大量文案深度嵌套在三元表达式/模板插值里（渲染自检徽章 title、按钮禁用原因、confirm/alert 弹窗）。

**跟用户确认的范围**（`AskUserQuestion` 两问）：① 全量精细翻译到 title 提示语/confirm·alert 弹窗/三元分支文案，允许顺手优化措辞；② 登录页/改密码页（`src/ui.rs` 的 `login_page`/`password_page`，纯 Rust `format!` 拼字符串，未登录态不跑 JS、读不到 `LS` 里的语言选择）这次不做——§03ae 那条"正文文案暂不迁移"这半句作废，"登录页/改密码页暂不迁移"这半句继续成立，要做需要另一套"未登录态也能传语言"的机制（比如写 cookie 给 Rust 端读），工程量和风险面跟这次的纯前端抽取不是一回事。

**设计要点**：

- `T()` 加可选 `vars` 参数做插值（`{name}` 占位符 `split/join` 替换），零参调用行为不变，向后兼容现有 10 处调用点。
- **发现一条这次任务最大的隐藏坑**：`I18N` 是启动 IIFE 里异步 `fetch` 填充的，`T()` 在填充完成前兜底显示 key 本身。现有代码从没踩过这个坑，因为原本仅有的 10 处 `T()` 全在函数体内（延迟执行）。这次要抽取的文案里有三个是模块顶层 `const 模板字符串`（`FMT_TIERS`/`GUIDE`/`OPTTABLE`），脚本解析时就立即求值一次、结果被"烤死"——如果直接塞 `T()`，效果是永远显示 key 兜底文本，不报错，肉眼很难发现。改成零参数函数（`FMT_TIERS()`/`GUIDE()`/`OPTTABLE()`），调用点加括号。同理 `STYLE_NAMES`/`STATUS_NAMES`/`PROVIDER_NAMES`/`BATTOP_WINDOWS` 四个顶层字典改成只存 i18n key 名（不存翻译文字），真正的 `T()` 查找挪到函数体内的调用点；`DEST_ICON` 因为混了 emoji/SVG+文字，改成惰性函数（`DEST_ICON[dv]()`）。这条规则已经写进 `app.js` T() 定义处的头注，作为以后再扩充 i18n 覆盖面时的硬性前提。
- Key 命名延续现有扁平点号约定（不引入嵌套 JSON），按源码区块分八个命名空间：`transfer.*`（含 `GUIDE`/`OPTTABLE`/`FMT_TIERS`）、`notes.*`（含三个状态字典）、`assets.*`+`koreader.*`+`wallpaper.*`（字体/词典/壁纸三个资产页）、`other.*`（`renderOther` 服务 label，复用 `tab.wallpaper`）、`manage.*`、`models.*`（含 `PROVIDER_NAMES`）、`battop.*`、`common.*`（`j`/`postJ`/`uploader`/`cjkBadge`/`delBtn` 等跨函数复用的原子文案，比如"未登录"/"上传中…"/"清空"）。
- **明确不翻的范围**：代码注释（项目约定注释保持中文）；登录页/改密码页（本轮排除，见上）；专有名词/技术字面量（文件扩展名、`shelf push` 等 shell 命令本身、URL、`xochitl`/`KOReader`/`Obsidian`/厂商名——厂商名后面括注的中文说明要翻，专名本身不翻）；`fmtB()` 的 `B`/`KB`/`MB` 单位（中英通用缩写）；`shelf push` 示例命令里的示例文件名（`论文.pdf`/`小说.azw3`，纯装饰性、非语义字符串，留作插图，不影响可用性）。

**执行方式**：单个 feature 分支内按区块拆 6 个提交（i18n 架子+`transfer.*` → `common.*` → `assets.*`/`koreader.*`/`wallpaper.*`/`other.*` → `notes.*` → `manage.*`/`models.*`/`battop.*`），每个提交各自过 `node --check`+`cargo test -p shelf-gateway`，不攒到最后一次性验证。

**规模**：locale 文件从 §03ae 的 13 个 key 扩到 **437 个**（`zh-CN.json`/`en-US.json` 逐 key 对照翻译，无缺项）。

**离线验证**：每个提交 `node --check app.js` + `cargo test -p shelf-gateway`（16/16，含现成的安全网 `locale_files_have_identical_key_sets`）；每个区块改完后跑脚本核对该区块函数体范围内残留中文字符（排除注释），确认目标区块用户可见字符串已清空；全文件收尾扫描：526 行含中文收敛到只剩 `/* */`/`//` 注释、两处历史死代码（`TABS['note-serve'/'wallpaper-serve']` 的 `.title` 字段，`titleKey` 优先级更高、这两个 `.title` 永远不会被渲染）、shelf push 示例文件名，没有漏翻的用户可见字符串。

**真机验证（比 §03ae 那轮更进一步，但仍有边界）**：`sh build.sh` + `SHELF_NO_BUILD=1 sh deploy.sh 10.11.99.1` 部署；`curl` 拉取真机上实际 served 的 `zh-CN.json`/`en-US.json`（各 437 key，集合完全一致，抽查若干条翻译内容正确）+ 真机首页 HTML；写脚本把首页 HTML 里全部 416 处字面量 `T('key')` 引用跟真机 served 的 `zh-CN.json` 交叉核对，**零缺失**——这确认的是"部署到设备上的这份 app.js 里，每一个翻译引用在设备实际提供的语言包里都能查到对应内容"，比本地文件自查更进一步（排除了部署/`include_str!` 编译环节出岔子的可能）。**没做的部分**：跟 §03ae 一样，浏览器里真正打开语言切换器、人眼确认文案切成英文后的排版/换行/组件对齐效果——这次做到的是"数据链路+内容对照 100% 确认"，视觉渲染这一步仍待人眼确认，这条缺口跟着 §03ae 一起挪到 §05，不重复开一条。

## 03ao｜`shelf push --no-calibre`：纯 EPUB 优化、跳过 Calibre（2026-09-10）

用户问"我们是不是缺失一个单纯 EPUB 优化不转格式的逻辑"。先核实现状而不是直接动手：**核心能力本来就不缺**——`bookconv::optimize::optimize_epub_with` 是唯一的纯 EPUB→EPUB 优化实现，已经被两处调用且强制门控只收 EPUB：设备网页「母版库→优化」按钮（`book-serve::Staging::optimize`，非 EPUB 直接 400）、独立 CLI 二进制 `epub-optimize`（用法固定"输入.epub 输出.epub"，无任何格式转换参数）。真正缺的是**host `shelf push` 这条 CLI 没有暴露"跳过 Calibre、只跑 epub-optimize"的入口**——`push.plan()` 对 EPUB 输入原来只有两条路：有 Calibre 就必然先跑 `ebook-convert` 深洗再叠加 `epub-optimize`（两步捆死）；`--no-optimize` 则两步都不跑。没有 Calibre 装机时甚至连"只跑我们自己的 epub-optimize"都拿不到，直接掉进"什么优化都没有"。

用户确认要补（`AskUserQuestion`，同时要求"也需要洗书"——即新开关不能只是 `epub-optimize --no-wash`「只优化不清洗」那档，要包含 `optimize_epub_with` 自带的清洗层，对应网页「清洗＋优化」默认档）。

**实现**：`calibre_bridge.py` 新增 `epub_optimize_bin()`（定位二进制：`WASH_OPTIMIZE_BIN` 环境变量覆盖——跟 `wash_epub.sh` 认的是同一个变量名，两条路径共用一个"在哪找"的旋钮 → PATH → 仓库内 `shelf/target/release/epub-optimize`）+ `optimize_only(src, out, keep_spacing)`（直接调二进制，不经 shell 脚本，默认走清洗+优化，`keep_spacing=True` 对应「清洗但保留段距」档）。`push.py` 新增 `--no-calibre` 参数 + `optimize_only_prepare()` + `plan()` 第三条路 `optimize-only`：EPUB 输入直接调 `optimize_only`（产物仍过 `_gate` 体检）；非 EPUB 没法只靠这条路径转格式，退化成 `raw`（原样传，不是报错，也不静默切回 Calibre）；`--no-calibre` 判在漫画分支之前短路——用户明确要求不用 Calibre，AZW3/EPUB 漫画解包成 CBZ 本来就依赖它，这时不该偷偷还是用上。

**跟已有代码的关系**：`optimize_only` 不是重新实现，是 `wash_epub.sh` 末步那个"叠加设备优化器"逻辑的独立 Python 化——`optimize_epub_with` 内部本来就含伪 DRM 剥离/CSS 锁剥离/边距段距归零，唯一没有的是 `wash_epub.sh` 专属的"读 `ebook-meta` 按 series 重命名"那部分（那属于 Calibre 元数据能力，不属于"优化"）。

**离线验证**：`push.py`/`calibre_bridge.py` 新增 8 个单测（`plan()` 三种场景：EPUB 走 optimize-only 且不受 Calibre 装没装影响、非 EPUB 退化 raw、漫画 EPUB 也退化 optimize-only 不偷用 Calibre；端到端确认 `cb.wash` 不被调用；`--keep-spacing` 透传；`epub_optimize_bin()` 四级定位优先级）；`uv run pytest reading/tests knowledge/pkm-semantic/proto shelf/host/tests`（CI 原命令）271 个全绿。

**真机验证边界**：这是纯 host CLI 改动，不碰设备行为，不需要真机验证——但仍然做了"非 mock 的真实调用"验证（不满足于只测 monkeypatch）：`cargo build --release -p bookconv --bin epub-optimize` 编出真二进制，拿 `reading/.cache/publish/` 下一份真实缓存 EPUB 直接跑 `epub-optimize`（真产物、真字节数变化）+ `cb.optimize_only()` 真调用（非 mock）+ `shelf push --no-calibre --dry-run` 对 `.epub`/`.azw3` 两种输入的路由打印确认符合设计。同一个 `optimize_epub_with` 函数本身早就随设备网页「优化」按钮真机验证过，这次只是新开了一个不经 Calibre 的调用入口，不重复走真机流程。

## 03ap｜「抓网文」补「同步优化」复选框（2026-09-10，真机通）

用户反馈"抓网文需要增加一个复选框（同步优化），即设备端进行针对设备的优化。否则默认给设备渲染页面会有大量留空，但要给用户选择权"。核实根因（§03r 早年就记过这条技术债，但当时只改成"分级标记+手动优化按钮"，没有再往前一步做"抓取时同步优化"）：`bookconv::article` 抽正文时属性白名单本来就不留 `class`/`style`（`is_whitelisted`/`keep_attr`），网文产出的 XHTML 正文完全没有任何排版样式；`fetch_article` 走的是 `assemble_optimized`→`optimize_epub` 缺省（`wash: None`），只做核心遍（脚注/图片降采样/对比度），**不做**边距/段距归零、不注入外链缩进 css——xochitl 只认外链 `.css` 里的裸元素规则、无视内联 style（§03y 七条实测规则之一），两件事叠一起＝正文按阅读器默认段距渲染，页面大片留空。这条路径产出的 EPUB 标记是 `level:"core"`，母版库列表已经有「优化」按钮能补（`level!=='full'` 就给按钮），用户现在其实能手动点一步修复——这次要做的是把"手动补一步"变成"抓取时可选自动做"，不是新造一条修复路径。

**实现**：`Staging::fetch_article` 加 `optimize: bool` 参数，落库后（`stage_new` 拿到落地名）若为 true 就紧接着调**同一个** `self.optimize(&name, OptimizeMode::Auto)`——不是另起一条优化实现，是母版库列表「优化」按钮那个函数原地复用；返回值改成 `FetchArticleOutcome{name,title,optimized,optimize_error}`。同步优化失败不阻断抓取结果（已经抓到的文章不因为这一步失败就整个丢掉，`Result` 仍是 `Ok`，只是 `optimize_error` 带上原因），API 层按 `optimize_error`/`optimized` 两种状态组不同的中文回执（"…并同步优化" / "…（同步优化失败：…，可在列表里手动点「优化」）" / 不带后缀的原样文案，向后兼容旧客户端不传 `optimize` 字段的情况）。

前端「抓网文」卡片 URL 输入行下面加一个 `<label class="toggle">` 复选框，**缺省勾选**（本机 `localStorage` 记住选择，跟 `folderPreset`/`optmode`/`stgclear` 同一个"per-viewer 便利态"规矩，键名 `artopt`）——给用户选择权体现在"能关"，不是"缺省关"：默认解决"大量留空"这个已知问题，想要最原始抓取结果（比如想自己去母版库挑别的优化档位）可以手动关掉。

**离线验证**：`book-serve` 17 个单测全绿（无回归，`fetch_article` 本身因为依赖真实网络抓取，这条项目一贯不写进 CI 单测——见 §04"会话里 shell cwd 漂移"附近几条踩坑记录同类考量，`optimize()`/`stage_new()` 各自的正确性已经被现成测试覆盖，新增的只是两者之间的组合调用，逻辑简单）；`cargo build -p book-serve` 零警告；`node --check app.js`+`cargo test -p shelf-gateway`（16/16，新增 2 个 `transfer.fetchArticle.*` key，`zh-CN`/`en-US` 439 key 集合一致）。

**非 mock 真实调用验证**（先于真机，host 侧）：临时脚本（未提交）+ 临时 `#[test]`（未提交，验证完即删）直接对真实 URL（`runoob.com` 一篇教程页，跟 §03r 那次真机验证用的是同一类稳定站点）跑通整条链路——`optimize_epub_with(wash:None)` 产物不含外链 `cangjie-wash.css` 引用（现状/未勾选），`optimize_epub_with(wash:Some(default))` 产物含引用（勾选后）；`Staging::fetch_article(url, true)` 落库后 `level=="full"`，`fetch_article(url, false)` 落库后 `level=="core"`（现状行为不变）。

**真机验证（2026-09-10）**：`sh build.sh`+`SHELF_NO_BUILD=1 sh deploy.sh 10.11.99.1` 部署；`curl` 真机 `POST /api/books/staging/fetch-article` 分别带 `optimize:true`/不带该字段各抓一次同一篇文章——`optimize:true` 落库 `level:"full"`、体积 3761 字节，回执"…并同步优化"；不带字段落库 `level:"core"`、体积 3488 字节，回执不变（旧格式请求向后兼容，行为跟改动前一致）；两次体积跟 host 侧非 mock 验证的字节数完全一致（同一份代码、同一份输入，确定性可复现）。首页 HTML 确认 `id="artopt"` 复选框+新 i18n key 已下发。验证完把两条测试文章从母版库删掉，设备恢复干净。

**追记（2026-09-10，§03aq）**：本节"默认解决『大量留空』这个已知问题"这句判断**过于乐观**——同一天用户拿另一篇图片密集的网文复现，真机 A/B 逐页渲染对比证实：`optimize:true` 这一步（清洗+优化，含边距/段距归零、外链缩进 css）本身没问题、确实生效了，但对"图片夹在正文中间导致的大片留白"这类场景**没有实质改善**，那类留白的真正成因是分页引擎对图片块的处理方式，不是"正文没有排版样式"这条根因能覆盖的全部——本节修的是一个真实存在且已确认生效的缺口（网文正文样式缺失），但不是用户报告的留白问题的完整解法，详见 §03aq。

## 03aq｜真机排查"抓完优化还是大量留白"：wash 补 figure/figcaption 边距 + 证伪 + 找到真正成因（2026-09-10，真机 A/B 验证）

用户拿真实文章反馈"为何抓完优化后还是有大量留白"（<https://aeon.co/essays/why-the-pan-american-highway-only-half-exists>），并在消息里自己点出"有图，夹在中间"。没有直接照着这个猜测动手，先用项目一贯的方法核实：host 侧非 mock 真实抓取这篇文章（`bookconv::article::build_article_epub`），过 `optimize_epub_with`，检查产出的 XHTML。

**第一个真实发现（但后来证明不是这次留白的成因）**：`wash_css()`（`crates/bookconv/src/wash.rs`）只对 `<p>` 元素清零过 margin/padding/text-indent，从来没管过 `<figure>`/`<figcaption>`——`article.rs` 网文管线的图片正是包成 `<figure><img/><figcaption>…</figcaption></figure>`，这两个元素的默认边距完全没被清零过。这是一处真实的代码缺口（不是这次凭空找的，是读 `wash_css()` 源码直接看出来的），补上了：
```
figure{margin:0;padding:0;}
figcaption{margin:0;padding:0;}
```
**两条规则必须分开写，不能写成 `figure,figcaption{}`**——xochitl 的 CSS 解析器很脆，逗号/复合选择器整条规则直接失效，`wash::tests::lang_aware_indent` 测试早就断言过这条红线（`!cjk.contains(',')`），这次新增 `figure_and_figcaption_margin_zeroed_as_separate_bare_rules` 测试同样守住这条线。`keep_para_spacing` 档位（"清洗但保留段距"）不该连带保留图片边距——那个开关管的是段落呼吸感，跟图片周围该不该有默认边距是两件事，所以 figure/figcaption 不受这个开关影响，始终清零。

**真机 A/B 验证（诊断EPUB→投原生→量 xochitl 渲染 PDF，不肉眼猜）**：把同一篇文章分别用修复前/修复后的 `optimize_epub_with(wash:Some(default))` 各生成一份 EPUB，上传母版库、投入原生书库，等 xochitl 渲染完，拉回它自己的渲染缓存 PDF（`GET /staging/render/{uuid}`），25 页逐页对比。**结果：修复前后像素级完全一致**——这条边距缺口不是这次投诉的留白成因，只是顺手堵上的一个真实但无关的缺口。

**真正的成因（同一次诊断里直接看出来的）**：留白集中出现在两处——① 第 1 页正文结尾（"...he believed he was chasing the future."）后半页空白，翻页后整页是第一张图；② 一组两张图的画廊占满大半页后，剩余空间空白，翻页后是下一组图片。两处都是同一个模式：**图片块（单张或几张连续的画廊）在当前页剩余空间放不下时，被整体推到下一页，当前页剩下的空间不回填**——这是渲染引擎自己的分页决策，跟外链样式表毫无关系（改 CSS 前后像素级一致就是最直接的证据），项目这边没有找到能从 EPUB/CSS 层面调整这个行为的手段。图片密集的网文，尤其原站带"图片轮播/画廊"结构一次给出 3-4 张连续大图的，留白会更明显——本质是"一篇图多的文章在小尺寸墨水屏上分页，总有几页排不满"，跟传统印刷排版里"孤图不裂开、宁可留白也不切开"是同一类取舍，没有能同时做到"零改动/零留白/图片一张不少"的三全解法。

**顺手发现、这次没有动手修的问题**：aeon.co 原站用 JS 轮播只显示 1 张图、点箭头切换，`readability_rust`+`article.rs` 白名单抽取时把轮播里全部 3-4 张**不同**图片（逐张核对过 `src`，不是重复）连同各自 figcaption 原样展开——内容更完整，但页数/留白也跟着涨，这是提取策略"要内容全还是要页数少"的取舍，不是 bug。另外渲染结果里出现"1 of 4"/"1 of 3"这类轮播页码指示文本混进正文——**在原始抓取的 HTML 源码里直接文本搜索找不到这几个字符串**，来源没有查清楚（确认不是 `article.rs` 自己拼的代码，猜测是 `readability_rust` 库内部对某种无障碍属性/隐藏计数元素的转写，没有深入验证），留作已知问题，不影响能不能读，只是正文里偶尔多出几句看着突兀的短句。

**离线验证**：`wash::tests::figure_and_figcaption_margin_zeroed_as_separate_bare_rules`（新增，覆盖裸元素规则+禁止逗号选择器+`keep_para_spacing` 档位同样清零）；`cargo test -p bookconv` 110/110（原 109+新增 1）；`cargo test --workspace` 全绿。

**真机验证边界**：这次的"真机验证"就是整个排查过程本身（诊断EPUB→投原生→渲染PDF逐页比对），不是事后补一道验证——诚实的结论是"改了一处真实存在的代码缺口，但这个缺口不是用户报告的留白问题的成因"，不是"已经解决用户报告的问题"。测试产物（母版库+原生书库两份 "Continental divide" 文档）已清理，原生库走的是回收站软删（可恢复）；核对时发现设备原生书库里已经有两份用户自己创建、打开过的同标题文档（`lastOpened`/`lastOpenedPage` 非零、时间戳早于本次测试），**只删了本次自己新建的两份**（uuid 精确匹配、`lastOpened:"0"` 确认从未被打开过），用户自己的两份原样保留未动。

## 03ar｜koreader-serve 新增高亮/生词只读端点，绕开一个 SQLite 交叉编译坑（2026-09-16，host 侧真机通）

给笔记线（notes/）的 KOReader 高亮/生词回流功能（笔记线白皮书 §03al）打地基：本仓库只加两个只读端点，条目库的创建/合并逻辑不在这边（"KOReader 高亮/生词回流 PKM"§05 已放弃列表原来记的"留给笔记线"，这次真正落地）。

**`GET /annotations`**：扫 `books/` 下每本书的 `<basename>.sdr/metadata.<ext>.lua` 标注 sidecar（KOReader 原生格式，路径规则读它自己的 `docsettings.lua` 核实）。不在 Rust 里写 Lua 语法解析器，沿用 `config.rs`（`merge.lua`）同一策略——交给 KOReader 自带 `luajit` 跑新脚本 `shelf/koreader/annot.lua`（`dofile` 出真表、手写 JSON 序列化，逻辑抄自 `merge.lua` 的 `jval`）。

**`GET /vocabulary`**：读 `data/vocabulary_builder.sqlite3`（KOReader 内置生词本插件库）。第一版用 `rusqlite`（bundled sqlite3 C 源码）做生产依赖，host 编译/测试都过，但交叉编译到 `aarch64-unknown-linux-musl` 链接失败——`sqlite3.c` 调 `open64`/`stat64` 这类 glibc LFS64 符号名，本仓库交叉工具链是"`aarch64-linux-gnu-gcc`（glibc 头文件）编 C + `rust-lld` 链 musl"的组合（`.cargo/config.toml` 注释"ring 的 C 用 glibc 的 gcc 编，产物 libc 无关，能链进 musl"）——这句话对 `ring`（纯计算不碰 libc 文件 I/O）成立，对真要读文件的 SQLite 不成立；本机没装 musl 原生交叉 gcc。跟用户过了三个选项（`koreader-serve` 单独退到 `aarch64-unknown-linux-gnu` 动态链 / 手写纯 Rust 只读解析器 / 装 musl 原生交叉工具链），拍板选手写：`sqlite_min.rs`，零 C 依赖，只实现读表要的最小子集（文件头+table b-tree interior/leaf+溢出页+record 变长编码，sqlite 官方文件格式文档，十余年没变过的公开格式）。`rusqlite` 降级成**只在 host 测试用**的 dev-dependency，生产二进制不链它，只用它造真实 `.sqlite3` 文件当 fixture 差分测试手写解析器（单页小表/`INTEGER PRIMARY KEY` 别名/3000 行强制 interior page/长文本强制溢出页链/真实 `vocabulary`+`title` 两表 schema 端到端，14 个测试）。交叉编译恢复成功，产物仍是全静态（`file` 确认 `statically linked`）。

**验证**：`koreader-serve` 新增 `annot`（3 测）+`vocab`（2 测）+`sqlite_min`（5 测）共 10 个测试，`cargo test -p koreader-serve` 14 个全绿（原 4 + 新增 10）；`shelf/build.sh` 完整跑一遍（host 构建+测试+aarch64-musl 交叉编译，CI 同路径）全过；clippy 零新增告警。**host 侧真实端到端跑通**（细节/踩坑见笔记线白皮书 §03al，两个仓库共同验证的同一次冒烟测试）：真实 `koreader-serve` 起服务、真实 `luajit` 解析手写的标注 sidecar fixture、真实 `sqlite3` 文件（Python `sqlite3` 库现造）读取正确。**当天设备恢复连接后补做真机验证**：拉真机上真实积累的 6 本书 `.sdr`（3 漫画+3 小说，两种后缀 `metadata.cbz.lua`/`metadata.epub.lua`）验证 `annot.lua` 全部解析正确（含一条"纯书签无文字"标注被正确过滤）；拉真机 `vocabulary_builder.sqlite3` 时**发现真 bug**：路径读源码时想当然写成 `data/`，真机实测在 `settings/`（两张表结构/字段名本身是对的），已修复（`main.rs`/`vocab.rs`）。修复后部署到真机（`koreader-serve`+`ink-serve` 各自备份原二进制、逐个重启+健康检查 `active`/新 PID/`NRestarts=0`，现有条目库数据完好），真实划一条高亮+真实加一个生词，两次 `POST /koreader/import` 都正确识别新内容并落条目库，细节见笔记线白皮书 §03al（同一次真机验证，两个仓库共同确认）。

## 03as｜`shelf notes pull` 补 `config.toml` 的 `notes_vault` 配置项（2026-09-16）

真机验证 `shelf notes pull`（笔记线白皮书 §03ak/§03al）时用户指出：本机 Obsidian vault 落哪个目录不该只靠每次手敲 `--out`，得有个配置项——vault 在用户磁盘上哪个位置只有用户自己知道，CLI 不该替用户猜（缺省 `$XDG_DATA_HOME/shelf/notes-vault` 只是兜底，不是大多数人真实 vault 的位置）。

`shelf_cli/config.py` 的 `DEFAULTS`/`Config` 加一个 `notes_vault: str` 字段（缺省空串＝未设置）；`commands/notes.py` 的落地目录判定改成三级优先级：`--out` 命令行 > `config.toml` 的 `notes_vault` > 缺省 XDG 位置。跟本 CLI 其它配置项（`host`/`password` 等）同一套 `tomllib` 读取机制，没有新加载路径。

**验证**：新增 1 个测试（`test_pull_without_out_uses_configured_notes_vault_not_xdg_default`，临时 `XDG_CONFIG_HOME` 写一份真实 `config.toml` 验证不给 `--out` 时确实落到配置的目录，不是 XDG 缺省位置），`uv run pytest shelf/host/tests` 94 个全绿（原 93 + 新增 1）。纯 host 侧 Python 改动，不涉及设备端代码，不需要真机验证。

## 03at｜真机重大发现+已修复：`gateway`/`shelf-gateway` 两个 systemd 单元并存，真机一直在跑 2026-09-10 的旧二进制（2026-09-16，真机通）

本轮部署笔记「整理」区提示停留时间的小改动（见下条）时，例行查了一下 `shelf-gateway.service` 健康状态，发现 `NRestarts=4517`、`ActiveState=activating`（卡死重启循环），日志报 `绑定 0.0.0.0:443（TLS）失败: Address in use`——查下去牵出一个比本次改动大得多的真问题。

**根因**：网关 2026-09-11 正名（`shelf-gateway`→`gateway`）之后，`/usr/lib/systemd/system/` 下同时存在**两份 systemd 单元**——旧的 `shelf-gateway.service`（`ExecStart=/home/root/.local/bin/shelf-gateway`）和新的 `gateway.service`（`ExecStart=/home/root/.local/bin/gateway`），**两个都被 `shelf.target` 静态 `Wants` 着**（`/usr/lib/systemd/system/shelf.target.wants/` 下两个符号链接都在）。这台设备上正名之后没有人清理掉旧单元——本该只保留新的一份。开机/`shelf.target` 启动时两个单元竞争同一个 443 端口，谁先起来谁占住，另一个陷入"启动即失败再重启"的死循环，`RestartSec=5` 意味着**每 5 秒失败重启一次、可能已经持续了两天多**（4517 次 ≈ 6.5 小时是保守估计，真实次数取决于这个状态存在了多久，`NRestarts` 计数器不会自己清零）。

**更严重的连带发现**：`/home/root/.local/bin/gateway`（新名字）确实一直在正常运行——`ps` 显示 PID 886 从 **2026-09-14 11:58:12** 就在跑（`gateway.service`），一开始被我误判成"占着端口不放的孤儿进程"直接 kill 掉，导致 443 端口被旧的 `shelf-gateway.service` 抢到、真机上从"跑 9-14 之后的新二进制"**倒退**成"跑 2026-09-10 17:18 的旧二进制"——这是我这次操作本身引入的一次真实倒退，好在很快查出来并纠正（`systemctl stop shelf-gateway.service` + `systemctl start gateway.service`，只是运行态操作，没有再误杀）。

**行动（分两步，第二步用户明确拍板后才做）**：先只做运行态修复（`systemctl stop shelf-gateway.service`＋`systemctl start gateway.service`），没有立即碰 `/usr`。用户看完风险说明后明确要求"删掉旧单元的 systemd 符号链接，并核查其他模块有无此问题"——**先核查**：扫了 `shelf.target.wants/` 下全部 10 个单元的符号链接时间戳+`ExecStart` 二进制是否存在，只有 `shelf-gateway.service` 是 2026-09-10（正名前一天）的旧符号链接，其余 9 个（`book-serve`/`font-serve`/`ink-serve`/`koreader-serve`/`mind-serve`/`note-serve`/`transcribe-serve`/`wallpaper-serve`/`gateway`）全部是 2026-09-11 14:28（正名当天）重新装的，二进制路径全部存在——**只有 gateway 这一处踩了坑，不是系统性问题**（gateway 是唯一"连目录带二进制名字一起搬"的服务，其它服务当初改名幅度更小）。核实完只删了最小必要的一个文件——`mount -o remount,rw /` → `rm /usr/lib/systemd/system/shelf.target.wants/shelf-gateway.service`（只删这个 `.wants` 符号链接，不动 `/usr/lib/systemd/system/shelf-gateway.service` 单元文件本体，也不动 `/home/root/.local/bin/shelf-gateway` 二进制——留痕不做多余清理）→ `systemctl daemon-reload` → `mount -o remount,ro /` 改回只读。**验证**：`systemctl list-dependencies shelf.target` 确认树里只剩 `gateway.service`，`gateway.service` 全程 `active`/`NRestarts=0`/PID 没变，整个操作零停机。这是对真机 ext4 rootfs 的一次真实写操作（remount rw 期间理论上仍有对应风险窗口），但操作本身（删一个符号链接、不写新内容、不碰任何 xochitl 相关路径）跟 工程纪律 记录的两次变砖事故（写 `xochitl.service.d/` 新建 drop-in 配置）在"写入内容/写入路径"上完全不同类，真机验证后确认安全、改动会持久保留（`/` 是真实 ext4 rootfs 不是 tmpfs，普通重启不会把这次删除冲回去，只有固件 OTA 才会覆盖）。

**次生怀疑，未证实**：Sep 13 那几次真机部署（`gateway.bak.pre-dedupe-batch-fix`/`gateway.bak.pre-dedupe-upload`/`gateway.bak.pre-weread-probe`，见 `/home/root/.local/bin/` 残留的备份文件）落的都是新路径 `gateway`——如果当时 `gateway.service`（PID 886 那条，9-14 才起的）还没起来、旧单元占着端口的话，那几次部署验证是否真的验证到了"当前对外提供服务的那个进程"存疑；但 886 从 9-14 就稳定在跑，9-13 那几次部署本身应该是提前一天备好新二进制、9-14 服务重启时生效，时间线对得上，**大概率没问题**，只是没法百分之百倒推确认，列出来存个疑，不是坐实的问题。

## 03au｜「整理」区状态提示停留时间 1.5s→3s，⚠️ 只是标，真根因是 SSE 抢跑重画（2026-09-16/17，真机通）

用户真机反馈「推送本章」/「重新转写」/「提问」三处点完之后弹出的状态文字（"✓ md 已导出"这类）"闪一下就没了"——`gateway/ui/app.js` 里这三处操作共用同一个模式：显示结果文案 → `wait(1500)` → 触发整页重画（`renderBook`/`reloadBook`），重画会把状态文案连同整个 DOM 一起冲掉。1.5 秒对短句都不够看清，先改成 3 秒。纯前端常量改动，`node --check` 通过，无需改后端。部署新 `gateway` 二进制后 `curl` 核对首页返回的正文里确实是 `wait(3000)`（3 处都改了）。

**⚠️ 这次改动没有真正解决问题**：用户隔天真机复验后反馈"重复推送的提示看不清，一闪而过"——延时从 1.5s 加到 3s 根本没用，因为**真正冲掉状态文案的从来不是那个 `wait()` 计时器到期后的主动重画，是另一条完全独立的路径**：这三个操作（推送本章/重新转写/提问）在后端各自会让对应服务发一次 `entries` 事件（`ink-serve`/`transcribe-serve`/`mind-serve` 各自 `bus.publish`），笔记 tab 正开着时，网页的 SSE 订阅（`es.onmessage`，`gateway/ui/app.js` 文件末尾 IIFE）一收到这个事件就立刻调 `sec.refresh()` 整段重画——这个重画跟点击处理函数自己那个 `await wait(3000)` 完全不同步、快得多（网络一个来回的量级，通常远小于 1 秒），状态文案真正是被**这次 SSE 触发的重画**冲掉的，不是被 3 秒后那次主动重画冲掉的，所以延长 `wait()` 的时长对这个根因毫无帮助，纯粹是误诊后的"标"，用户复验一次就现了原形。

**真根因排查+修法**：`renderNotes(sec)` 新增一个 `holdRefreshUntil` 时间戳变量，三处点击处理函数在**动作一开始**（发请求之前）和**结果文案刚显示出来那一刻**分别写一次这个时间戳（前者防请求处理期间被抢跑，防御性给了 15s 上限；后者才是真正要保的"文案至少停留 3 秒"的窗口）；`refresh()`（SSE 触发的那个重画入口）起手先看这个时间戳，没过就直接跳过这次重画——不会漏更新，因为点击处理函数自己那句 `await wait(3000);await reloadBook(...)`/`renderBook(...)` 本来就会在窗口结束后主动刷新一次，两条路径不用同时抢着刷，只要 SSE 那条让个路。纯前端改动，`node --check` 通过；不改后端事件发布逻辑（服务该发事件还是发，只是网页这边不再被这类"自己刚发起的动作触发的事件"打断自己正在展示的提示）。

**教训**：跟 §03am 那次"用户反馈还没修复才发现真根因"是同一类教训——第一轮只对着"延时是不是不够长"这一个可能性动手，没有去验证"到底是不是这个计时器把文案冲掉的"，如果当时去查一下重画到底是从哪条代码路径触发的，会立刻看到 SSE 那条路径快得多，延时数字改多大都没用。

## 03av｜书籍优化架构调整：拆 EPUB 线/PDF 线，EPUB 线四原则落地（2026-09-17，⚠️ 只 host 单测验证，未上真机）

用户拍板把「书籍优化」拆成 **EPUB 线**和 **PDF 线**两条独立业务逻辑（PDF 线原则用户还没给，本节
只记 EPUB 线），并提了三个架构约束：① 设备端只收 EPUB/PDF，其余格式请用户自行转换后上传；
② EPUB 优化全部在设备侧进行；③ EPUB 线四条优化原则（保留目录+拆分章节序号、解锁字号但保留
原书颜色/加粗、注释移到引用段落之后、漫画自动识别不压画质）。

**架构收敛**：优化前 EPUB 优化有两条并行路径——host CLI `epub-optimize`（`shelf push` 前置调用）
和设备 `book-serve`（母版库「优化」按钮手动触发），同一个 Rust 函数
`bookconv::optimize::optimize_epub_with` 两处调用。这次不动"两条路径都在"这件事本身（host CLI
仍保留，给 `--no-calibre` 等场景用），但把"host 会用 Calibre 自动把 AZW3/MOBI/AZW/PRC/FB2/TXT
转成 EPUB 再推设备"这条**彻底退役**（用户原话，且明确要求连带禁止这几个格式的其他用途——见下）。

**四条原则的落地，全部在 `shelf/crates/bookconv`（Rust，设备侧/host CLI 共用同一份逻辑）**：

1. **目录**：`wash.rs::auto_toc()` 原来只在**全书完全没有目录**时从 h1–h6 生成；这次补两处：
   ① 新增 `split_numbered_title` 启发式——标题文本"标题+编号"结尾（如原书「第一章 1」）拆成
   父级标题+缩进子级编号两条 TOC 条目、同指一个锚点（EPUB3 nav 文档规范原生支持的嵌套 `<ol>`
   结构，不是 hack）；编号 >99 判定是印刷页码残留、不拆，避免误伤。② 新增无 h1–h6 语义标题时的
   兜底——退化到按 spine 文件边界生成目录、取正文首段文本当标题；多数页面没有可提取文本（疑似
   漫画/画册）时不生成，避免灌一堆无信息量的"正文 N"。
2. **字号解锁、保留原书样式**：`wash.rs::DEFAULT_FILTER_PROPS` 从
   `["font-family","font-size","font","color","background-color","background-image","background","text-align"]`
   收窄成 `["font-family","font-size","font","background-image","background"]`——`color`/
   `background-color`/`text-align` 不再剥。查证发现这条本来就是照抄 Calibre `--filter-css` 的
   通用参数集，不是针对 xochitl 真机验证过的必要行为；`background`/`background-image` 因为是
   坐实的渲染 bug（§03③，xochitl 无视 `no-repeat` 把背景图平铺满页盖正文）继续剥，跟字号锁无关。
   **风险未验证**：放开 `background-color` 后如果原书有"深底浅字"高亮块，Paper Pro Move 彩色
   e-ink 屏低对比场景下可能比剥离前更难读——`boost_text_contrast()` 目前只处理文字颜色/字重，
   不处理背景色对比度，真机验证时要专门挑一本带彩色底纹的技术书测。
3. **注释移到引用段落之后 → 真机反馈后撤回，改回 `Anchor`（同日内两次决策，完整记录）**：现有
   `FootnoteMode` 只有 `Anchor`（跳章末，但 reMarkable 会吞互相引用的锚点对导致**点了跳不回来**，
   真机坐实过）和 `Inline`（就地内联，**打断段内阅读**）两种，都不完全符合"既不影响连续阅读体验
   也不影响注释理解"。当天先新增第三种 `ParagraphEnd`：不跳转、不建反向锚点，在"含有该引用的
   整段"结束后插分割线+注释块，marker 原地改纯 `<sup>N</sup>`；设备侧 `Staging::optimize()` 从
   `Inline` 切到它，功能层真机验证（构造测试书真实上传优化、解包核对产物字节）也确认按设计跑通。
   **但用户随后拿一本真实转换的书（《赎罪》）在设备上实际测试后反馈"注释并未在当前页最下面，
   而是在注释标记的段末"**——这才发现"段末"只是我对"就近可见"的一种近似实现，用户真正想要的
   是"翻到哪页，注释就固定在那页最下面"，两者是不同的东西。核实后确认这不是实现细节问题：EPUB
   是流式重排文本，"这段内容最终落在第几页"是阅读器翻页时才计算出的运行时结果，做书阶段根本
   不知道，没法把"这条注释属于第 N 页"这种信息预先写进源文件——真正的页底部定位需要固定版式
   （每页内容在制作时就已经定死，是 PDF 线的领地），不是 EPUB 格式原生能表达的语义。用户确认后
   拍板改回 `Anchor`，`FootnoteMode::ParagraphEnd` 连同实现和测试**整个删除**（不留作死代码——
   这不是"部分正确以后可能用得上"的方案，是已经证实不符合预期的方案）。`book-serve::Staging::
   optimize()` 与 host CLI `epub-optimize` 的脚注缺省最终都是 `Anchor`，与 weread/pkm 线一致。
4. **漫画自动识别、不压画质、允许裁边**：新增 `comic_detect` 模块，移植 host 侧
   `comic.py::epub_image_stats()` 的判定算法（沿 OPF spine 统计 `<img>`/`<image>` 数与可见文字数，
   图 ≥20 张且平均每张图配的文字 <40 字判漫画）到 Rust，供 `optimize_epub_with` 内部直接判——
   之前一本 EPUB 格式的漫画走母版库「优化」会被当成普通文字书处理。命中后：`imgopt::trim_margins`
   四边纯色/近纯色留白裁边（边缘整行/列颜色高度一致才裁、单边最多裁 15% 防误判裁没内容），
   `imgopt::downscale_for_epub_comic` 超限时仍缩进屏幕框但用 quality 95（普通插图是 85）。跟已有
   的"漫画省刷新"灰阶抖动（`imgopt.rs::dither_bilevel`，CBZ 转换路径专用）方向相反，两者不共用。

**格式收窄的连带影响（用户明确拍板，非默认选项）**：问过用户"AZW3/MOBI/PRC 除了转 EPUB 洗书
外还有一条独立用途——`shelf push` 会解析 PalmDB 直判它们是不是漫画、是则转 CBZ 进 KOReader，
这条跟书籍优化无关，是现有能用的漫画管线，退役要不要连带影响它"，用户选择**连带禁止，一律
拒收**——于是 `comic.py` 的 `palmdb_image_ratio`/AZW3 分支也一并删除。⚠️ **这是主动放弃一条已验证
可用的能力**：书架白皮书本节以上（§03，2026-09-05 格式分档那次调查）实测坐实过 AZW3/MOBI/AZW/
PRC/FB2/TXT 全部是设备装的 KOReader crengine 真能读的格式，不是"读不了才收窄"，是用户为了规则
一致性主动收窄。`rmsvc_core::formats::HOST_CONVERTIBLE_EXTS` 整档删除，`BOOK_EXTS` 现在等于
`NATIVE_EXTS ∪ KOREADER_ONLY_EXTS`——母版库上传门（各服务共用同一份白名单）自动拒收，`shelf push`
一侧不需要额外的拒绝逻辑，非 EPUB/PDF 源格式原样透传给服务端、由既有回执机制清楚打回执。

**待确认、未落地的一条建议**：host 不再预优化 EPUB 后，网页直接上传的 EPUB 在用户手动点母版库
「优化」按钮前仍是未解锁字号/未拆注释的原始状态——这个 gap 在改动前就存在（不是这次引入的
回归），建议 `book-serve` 收到 EPUB/PDF 上传时自动跑一遍优化（`OPTIMIZE_MARKER` 幂等标记已经
支持"跳过已优化产物"，具备做自动触发的基础设施），但这条只是建议，还没有得到用户拍板，未实现。

**验证状态（2026-09-17 更新）**：`book-serve`/`gateway` 已交叉编译部署真机（备份+md5核对+
`systemctl restart`+`ActiveState/MainPID/NRestarts` 健康检查，走标准流程），**功能层真机验证已过**——
host SSH 隧道到 `book-serve` 内网端口（127.0.0.1:8790，走 gateway 不用密码），用真实 HTTP
multipart 上传四本构造的测试 EPUB（覆盖四条原则各一本）+ 一份 `.azw3` 假文件，逐一调
`POST /staging/optimize` 后 scp 回产物解包核对：
- ① TOC 拆分：「第一章 1」在真机产物的 `nav.xhtml`/`toc.ncx` 里确实拆成父级「第一章」+缩进子级
  「1」两条，「后记」（无编号尾巴）没被误拆——两处都指向同一锚点，符合设计。
- ② 颜色/字号：`style="color:#0000ff;font-weight:bold;font-size:9px;"` 真机优化后变成
  `style="color:#0000ff;font-weight:bold;"`——颜色/加粗保留，`font-size:9px` 锁被剥掉。
- ③ 段末块注释：marker 真机产物里变成纯 `<sup>1</sup>`（不再是链接），紧跟在引用段落 `</p>`
  之后插入 `<hr/>` + `<div class="cj-fnote-para"><p>1. …</p></div>`，位置符合设计（**历史记录**：
  `ParagraphEnd` 本身随后被真机真书测试反馈撤回删除，见上面原则③正文——"功能按设计跑通"这条
  验证结论仍然成立，只是"设计本身不是用户想要的"，两件事分开看）。
- ④ 漫画裁边+保画质：构造 25 页、每页 2200×3400（四边留 250px 纯白边）的测试漫画 EPUB，真机
  优化后单页图片变成 954×1623——尺寸换算跟"先裁掉四边留白再按屏幕框缩放"完全吻合（不裁的话
  同样输入应该落在 954×1474，实测不是这个数），确认 `trim_margins` 真的在裁、裁完还按漫画
  quality 路径重编码。
- ⑤ 格式拒收：真机上传 `.azw3` 假文件，收到 `不是书籍格式，母版库只收 .epub .pdf .cbz .cbr
  .djvu .html .htm .rtf .doc .docx .chm .xps`——干净的回执，不是裸错误堆栈。

**⚠️ 意外发现（真机性能问题，功能正确但速度值得记录）**：25 页 2200×3400 的测试漫画在真机上
`POST /staging/optimize` 跑了 **2 分 19 秒**（host 上同等规模操作是秒级）——`imgopt::trim_margins`
的逐行/逐列纯色扫描是 O(留白像素数)，在 reMarkable 这颗弱 ARM 芯片上被放大了两个数量级。这本
测试书页数（25）和分辨率（2200×3400）都不算离谱（真实漫画常见上百页、更高分辨率），**真实漫画
优化耗时可能是几分钟到十几分钟量级**，母版库「优化」按钮目前是同步阻塞调用，没有进度提示——
这是本轮验证意外揪出的、原设计没有预料到的真实性能问题，不是这次原则4实现的功能 bug，但会
直接影响用户体验（点了优化按钮长时间没反应，看起来像卡死）。列入待办（见 §05）：要么给
`trim_margins` 提速（比如降采样后先扫一遍定位大致边界，再在原图小范围精扫），要么给母版库
「优化」加异步进度反馈。

**仍未验证（需要用户在真机屏幕上肉眼确认，本环境没有屏幕访问能力）**：以上都是"处理产物的 HTML/
图片字节层面正确"，不等于"xochitl 渲染出来视觉正常"——① 彩色高亮块在 Paper Pro Move 屏幕上的
实际可读性（原则2标注的风险点，需要真书）；② TOC 两级嵌套在原生目录 UI 里是不是真的有缩进层级
观感；③ 段末块注释在实际翻页阅读中是否顺眼、有没有意外的排版异常（比如恰好卡在页边界）；
④ 一本长期在跑的普通文字书完整走一遍新流程做回归，确认没有破坏正常渲染。这几条需要用户挑一本
真书跑一遍、拿设备实际翻看确认。

## 04｜踩坑

- **挪代码时顺手带走的文案不代表内容还准（2026-09-10 用户真机测试逮到）**：§03ak 把「系统增强」卡片原样搬进「实验室」，battop"未装"提示里的路径 `misc/battery-audit/battop/install.sh` 是 §03aj 写的，那时候还没意识到这个路径已经在更早的 §03b 里 `git mv` 到 `enhance/battop/` 了——挪动/重构代码只挪了位置没重新核对内容，字面拷贝把旧错误也一起搬了过去，还搬了一次都没发现（两轮都没查）。**教训**：移动/复用一段包含具体路径/命令/版本号的文案时，顺手核对一遍还准不准，不能假设"没人提过所以肯定没问题"——原样复制不代表内容仍然正确，只代表格式没错。
- **`hidden` 属性和 class 选择器同时作用于同一元素，谁赢看样式表来源不看后写的规则**：`.subpanel.on{display:block}` 是页面自己的 author 样式表规则，浏览器给 `[hidden]` 属性的默认 `display:none` 是 UA（浏览器内置）样式表规则——同等"正常优先级"声明下 author 样式表**无条件**赢过 UA 样式表，跟谁的选择器更具体、谁写在后面都无关。所以"关掉一个用 `hidden` 属性控制显隐的子标签"时，如果它当时正好带着 `.on` class（用户之前点开过），单纯设置 `hidden=true` 不会真的隐藏——得先把 `.on` 挪到别的标签上（这次做法：`hidden` 之前先 `.click()` 切到默认子标签）。这个坑没有真机复现出来（本地手动检查代码路径时想到的，提前避开了），但值得记下来——下次给任何"class 控制显示 + hidden 属性叠加控制可见性"的场景直接抄这条顺序。
- **xochitl CSS 引擎七条实测规则见 §03y**（尾分号 / 0 当没设 / 类规则认且压元素 / 同类先出现者胜 / 不认内联 style / text-indent 继承 / 混类选择器不废表）。改排版规则前先用诊断 EPUB 量渲染缓存，别靠肉眼。
- **磁盘 metadata ≠ xochitl/UI 实际状态（2026-09-04 用户纠正）**：直接 `sed` 改 `.metadata` 的 `parent=trash` 并不等于"已进回收站"——xochitl 运行时在内存缓存、写回时覆盖，云同步也可能还原；出现过磁盘 8 个探针 `parent=trash` 但 UI 回收站只见真实书的错位。**涉及书库状态以设备 UI/xochitl 实际为准，不拿磁盘 metadata 当真相**；清测试文档走正常删除流程或停 xochitl 后操作，别边跑边改。
- **qmd 语法比 QML 窄，且解析错误=整份不应用、xochitl 不崩不报**（2026-09-05）：`({})`、裸 `if (` handler 都让 qmldiff 报 `expected item assignment value token`；只在 journal 有一行 `[qmldiff]: Error while processing file tree`，菜单静默缺项（用户"选不到已安装的字体"）。规矩：写/改 qmd 先 `git clone asivery/qmldiff && cargo build --release`，`qmldiff apply-diffs <root> <dest> x.qmd -f -c` 对 `extract_qml.py` 解出的真 QML 实跑（root 按资源路径摆），过了再上机；上机后 `journalctl -u xochitl | grep qmldiff` 必看。
- multipart 流式解析：`fill()` 用 `Vec::resize(+64KB)` 在逐字节到达的流上变成 memset 风暴（测试 50s）；改栈上临时块 `extend_from_slice` → 0.8s。
- **xochitl `/upload` 有体积上限**（实测 282MB EPUB 被 `multipart body is too large` 拒；此前 60～285MB 间见过 413）：大 PDF 靠 `pdfsplit` 60MB 分卷；EPUB 不能分卷——漫画别走 EPUB（§03t）。
- 会话里 shell cwd 会在 `cang-jie/` 与 `shelf/` 间漂移：`curl -F` 之类落地文件一律写绝对路径到 scratchpad，否则测试文件会混进仓库（2026-09-05 误提交两个临时文件后已删）。
- busybox：`head` 要 `-n`、无 `timeout`/`base64`/`od -A`、`ls` 中文名显示 `?`（验名 `find | hexdump -C`）。
- busybox `nc`/`wget` 读不了 SSE 流（wget 缓冲到结束、nc 拿不到响应）：验证长连接用 host 侧 `ssh -L` 隧道 + `curl -sN`；后台进程要 `setsid … < /dev/null`，否则 ssh 会话挂住；`pkill -f` 的模式别写进自己的命令行（会把自己杀了，exit 144）。
- `shelf/build.sh` 必须在 `shelf/` 目录跑（仓库根没有 build.sh）。
- 本机冒烟别忘 `env -i`：host 桌面环境自带 `XDG_STATE_HOME/XDG_CONFIG_HOME` 会盖过 `HOME` 覆盖，把测试数据写进真实用户目录。
- pytest 要从仓库根跑（`uv run pytest shelf/host/tests`）。
- 多个测试文件对同一个 `http.server` Handler 类 monkeypatch，module fixture 共用服务器线程时 patch 链互相覆盖会递归死循环（pytest 挂死）。各文件用自己的 Handler **子类** + 自己的 fixture。
- **`shelf push` 多文件批量部分失败后重跑不是幂等的，会静默产生重复条目（2026-09-13，Reddit 用户提问，读代码坐实非猜测）**：`push.py::run()` 顺序遍历 `args.files`，`receipts.py::upload_each` 逐文件单独 POST，一个文件失败只置 `rc=1` 继续下一个（不中止整批），**整条命令没有任何跨次运行的状态记忆**——重跑同一条命令永远对参数列表里全部文件重新走一遍。落到 staging 端，`rmsvc-core::fs::unique_path`（`dir/name` 存在就依次退到 `1_name`/`2_name`…）**纯按文件名判重、不比内容 hash、绝不覆盖**——这条本身是 §03r 那次重构就定下的有意设计（避免不同书撞名互相吞掉，见上面"⑤ 同名重复入库回执"那条），但组合上 CLI 零跳过逻辑就成了一个真实 gap：`shelf push a.epub b.epub` 若 a.epub 已经成功、b.epub 失败，原样重跑会让 a.epub 在母版库里落成 `1_a.epub`——**成功的静默重复，不是报错，容易被回执文案"已有同名，存为 1_a.epub"一句话带过忽略**。目前没有 CLI 命令能列出 staging 内容核对（`shelf status` 只给 pending/failed 计数，`shelf inbox` 是 SCP 补录队列，跟 staging 是两码事），唯一现实的规避是重跑前去网页确认哪些文件已经成功、只重传真正失败的那些。**已修（同日）**：`push.py` 新增 `_staging_snapshot()`——首次探活成功后查一次 `GET /api/books/staging`，取 `{(name, bytes)}` 快照；上传每个文件前先比对，同名同大小＝上一轮已经成功落地，跳过重传并打印"已在母版库（同名同大小），跳过重传"；同名不同大小（内容真的换了）照常传，不误伤。按文件名+字节数比较、不做内容 hash（同名同大小但内容真的不同这种极端情况会被误判跳过——已知取舍，真撞上了改个文件名即可绕过）；查询失败（设备不可达/接口异常）时快照为 `None`，退回这次修复前的行为（不阻断推送）。新增两条 host 测试：`test_push_rerun_skips_already_staged_file`（同名同大小跳过 / 同名不同大小照传）、`test_push_batch_partial_failure_then_rerun_lands_once`（贴 Reddit 原话场景：a 成功 b 失败后重跑整条命令，a 不重复、b 补传），`uv run pytest shelf/host/tests` 87 项全绿。**追记（同日，网页上传口同一个坑）**：用户追问网页
`https://10.11.99.1/` 的上传口是不是也有这个问题——读 `gateway/ui/app.js` 的通用 `uploader()`
发现**分两种情况**：同一个页面会话内点"重传"其实是安全的（`f.st==='ok'` 的项跳过、只重传失败
项，队列状态活在浏览器内存里，比 CLI 严谨），但**刷新页面后重新拖同一批文件**或**手滑拖两次
同一文件**这两条路照样能撞上同一个 staging 端根因，跟 CLI 是同一个 gap 的另一种触发方式。
**已修**：`uploader()` 新增可选第 6 参 `dedupeApi`，`go.onclick` 时若传了这个参数就先
`GET` 一次拿现有条目做 `name|bytes` 快照，上传每一项前先查快照命中就跳过（打法跟 CLI 一致：
按名字+大小，不比 hash）；只有母版库这一个上传口（`/api/books/staging`）传这个参数，字体/
壁纸/词典那三个 `uploader()` 调用点不传、行为完全不变。新增 i18n key
`common.alreadyStaged`（中英对照）。**真机+浏览器端到端验证**：真机部署新 `gateway` 二进制
（`include_str!` 编译进二进制，光改 JS/JSON 不重新编译不生效）、`systemctl restart` 确认
`active`/`NRestarts=0`；但真机网页密码不知道，改用本机起一份隔离的 `book-serve`+`gateway`
（**`env -i` 清空桌面环境自带的 `XDG_STATE_HOME`——这条正是 §04 那条"本机冒烟别忘 env -i"
教训现场又踩了一次，一开始没加 `env -i`，`Paths::resolve` 真的读到了宿主机真实
`/home/afu/.local/state/shelf`，及时发现清理掉了没造成实际污染**），配 puppeteer 真开一个
无头浏览器登录→改密→上传测试文件→刷新页面→重新上传同一文件→读 `/api/books/staging` 核对：
第二次上传消息是"已在母版库…跳过重传"、staging 里确实只有一条 `dedupe-test.epub`、没有
`1_dedupe-test.epub`，`PASS`。**追记（同日，真机部署后立刻暴露漏网分支）**：部署到真机后用户
实际用页面传了一本真书（漫画 PDF），反馈"已入母版库（已有同名，存为 1_x）"+"已入母版库（已有
同名，存为 2_x）"两条——防重没生效，反而一次操作造出两份编号副本。根因：`existing` 快照只在
`go.onclick` 一开始 `GET` 一次，**循环内上传成功后没有把这份文件加回 `existing`**——本机那个
测试只覆盖了"刷新页面重新传"这一种触发方式（快照本来就没有它，检测对了），没覆盖"同一批里
排了两份同名同大小文件"这种（第一份传完落地了，但快照没更新，第二份循环到时还是拿最初那份
旧快照比对，一样查无此文件，跟着传了第二次）——CLI 那版 `_staging_snapshot` 我在 `push.py`
里正确地在每次上传成功后 `staged.update(...)`，移植到 JS 时这一步漏掉了。**已修**：
`x.onload` 里上传成功（`it.ok`）后立刻 `existing.add(f.file.name+'|'+f.file.size)`。新增
`test-dedupe2.js`：`<input multiple>` 一次塞两份同一个文件（复现"同一批排两份"），真机同款
隔离环境+puppeteer 重跑，两条队列消息变成"已入母版库"+"already staged…skipped"，
`/api/books/staging` 只有一条，`PASS`；已重新交叉编译部署真机（备份旧二进制、`NRestarts=0`）。
**教训**：把同一个逻辑从一种语言/运行时搬到另一种时，"当时为什么要有这一行"的原因（这里是
"同批次内部也可能撞上"）容易在移植时漏掉，只对着"看起来等价"的代码结构誊抄，两个分支
（跨会话 vs 同批次）各自的触发条件不一样，得各写一条测试分别验证，不能只测其中一种就当整个
函数验证过了。目前真机母版库里应该还留着这次测试造出的 `1_x`/`2_x` 两份真实重复文件，需要
用户自己在网页删掉多余的两份（不清楚具体是哪本书、不替用户做删除决定）。
- **扫描版漫画 PDF 走 `shelf push` 会必然失败，且 `.pdf` 从没被 `comic.is_comic()` 判过是漫画
  （2026-09-13，真机用户实测踩到）**：用户 `shelf push` 一本 Anna's Archive 下的纯扫描图片
  PDF（《照明商店》第 11-20 话，同系列第 1-10 话此前是走网页原样上传成功的——网页上传不碰
  "洗书重排"这段逻辑，两次经历不冲突，只是没走同一条路），报错
  `pdf_reflow_move.py 失败（rc=2）：扫描件重排失败（无 k2pdfopt 且裁边未产出）`。根因是两层
  "遇到扫描件就拒绝处理"的设计叠加：①`pdf_reflow_move.py`（`shelf/host/calibre/
  pdf_reflow_move.py`）采样前 10 页可抽字符数判定"非 born-digital（扫描件）"后，优先尝试外部
  工具 `k2pdfopt`（本项目有意不内嵌位图重排引擎，见 §03t 附近"不移植 k2pdfopt：28+43 C
  文件、高投入低回报"，要求用户系统自行装好，`shelf doctor` 只提示"缺（可选）"**没有任何
  安装指引**，全仓库找不到一处安装命令/URL）；②没装就退回 `pdf_crop_move.py` 裁边兜底，这个
  脚本自己也判一遍扫描/文字/混合三态，**扫描型按设计主动拒绝产出**（`return 3`，不落盘，理由
  写在脚本注释里："扫描/混合型：设备上用 KOReader 重排(KOPT) 或 xochitl Adjust view；硬裁加
  `--force-crop`"）——两条路都不通，`pdf_reflow_move.py` 把这个组合失败包成 rc=2 报出来，不是
  哪一步真的崩了。**现在能用的路**：`shelf push --no-reflow` 跳过整段重排逻辑原样落库（代码
  验证过，最简单可靠），落库后网页"加入 KOReader"用它自带 KOPT 重排着看，或直接投 xochitl
  用内置 Adjust view；`--comic` 强制走漫画 CBZ 管线理论上可达（`is_comic()` 传 `args.comic=True`
  时不检查后缀），但项目从未测试过 Calibre `ebook-convert` 对纯扫描图片 PDF 转 EPUB 再抠图这条
  路能不能干净还原每页，**未经验证，不当确定可靠方案**；长期是装 `k2pdfopt` 让默认重排真正
  生效。**真正的架构缺口**：`comic.is_comic()`（`shelf/host/shelf_cli/comic.py`）只判
  `.cbz`/PalmDB 家族（`.azw3`/`.mobi`/`.azw`/`.prc`）/`.epub` 三类，**`.pdf` 从设计上完全不
  在判断范围内**，尾部直接 `return False` 兜底——项目自己整套成熟的漫画 CBZ 管线（跨页拆分/
  白边裁切/16 灰省刷新）对这类扫描版漫画 PDF（Anna's Archive 这类来源很常见）完全够不着，只能
  走给论文/杂志设计的文字结构化重排路，而这条路对纯图片扫描件唯一的兜底又是可选外部依赖。
  这个组合失败场景、以及"扫描漫画 PDF 该不该走漫画管线"这个问题，此前都没有被讨论过——
  **这次只记发现，用户明确说暂不验证 `--comic` 效果、也暂不扩展 `is_comic()` 支持 PDF**，
  留在这里等以后要做再回来接。**已修（2026-09-14）**：`comic.is_comic()` 补上 `.pdf` 分支——
  抽样统计"有图且几乎无文字"的页占比，图片页 ≥20 且占比 ≥60% 判漫画，跟 PalmDB/EPUB 两条判定
  同一套阈值；判定本身要真正打开、逐页解析，做不到其余分支"零依赖毫秒级"，拆成独立子进程脚本
  `shelf/host/calibre/pdf_comic_probe.py`（pymupdf，走 `calibre_bridge.py::pdf_comic_stats()`），
  不直接拖累 `comic.py` 其余分支的"廉价探针"承诺，缺 pymupdf（`calibre` 依赖组）时静默退回
  False（走原来的文字书重排路，不阻断推送）；magic 头不是 `%PDF-`（含路径不存在）时提前 False，
  连子进程都不 spawn。**这次顺带验证了之前"未经验证、不当确定可靠方案"的 `--comic`/
  `ebook-convert` 这条路，结论是可靠**：拿用户真机报的原始文件（照明商店 第11-20话，81MB）
  实测——探针判出 2375 页、抽样 ratio=1.0；`ebook-convert` 3.3 秒转出 76MB 中转 EPUB；
  `comic2cbz.py` 抽出 2376 张真实页图（含自动生成封面），肉眼核对多张（含中段 1187 页）画面
  完整、对话气泡文字清晰、没有乱码/黑屏/裁切错位；`comic_prepare()` 全流程（含 16 灰+跨页
  拆分）跑通，155 秒出一份 247MB 灰阶 CBZ，过程里的"装订缝不够干净"提示是 `comic_gray.py`
  既有逻辑，不是这次改动引入的新问题。`shelf push --dry-run` 复核路由从"扫描件重排失败"变成
  正确的"漫画 CBZ→母版库"。**没有验证到的**：这次没有设备密码，没有做真正的网络上传落地
  这最后一步（上传逻辑本身没改，风险低，但没有"落进真机母版库"这条实锤）；新增
  `test_comic_probe_pdf` 单测（mock 掉 pymupdf 子进程，CI 不装 calibre 依赖组）只验证
  `comic.py` 自己的阈值分支逻辑，探针脚本本身的正确性靠上面这次手动真实文件验证背书，不是
  靠单测。
- **"跳过已存在文件"这层保护只省了上传流量，没省 host 端处理时间（2026-09-13，用户追问发现的
  设计局限，暂不动手）**：`_staging_snapshot()` 判重放在 `push.py::run()` 的"处理完之后、上传
  之前"——重跑一个之前已经成功过的大部头，Calibre 洗书 / PDF 结构化重排这类**耗时的 host 处理
  步骤会被完整重跑一遍**，直到最后一步才发现该跳过，只省下网络上传和母版库重复条目，处理时间
  一分钟没省。**为什么不能简单挪到处理之前**：判重靠"文件名+字节数"，staging 里存的是**处理后**
  的字节数（洗书/优化会改变文件体积），跟本地**原始输入文件**的字节数通常对不上——直接拿原始
  输入去跟处理后的条目比字节数，大概率查无此文件，不会误跳过（安全），但也起不到省处理时间的
  作用，形同虚设。**真要做需要**：上传时 CLI 额外带一份"这份处理产物的原始输入是谁"（原始文件
  size/hash）当参数，服务端 `StagingEntry` 结构多存这一个字段；下次跑之前，CLI 先拿本地原始
  文件的身份去查一遍这份记录，查到匹配就直接跳过整个处理+上传。这是要新加字段、改一点上传
  协议的功能，比 2026-09-13 那两处纯 bug fix（CLI push + 网页上传各自补的"同名同大小跳过重传"）
  量级大一截——服务端 `StagingEntry`/上传参数、CLI 处理前置检查都要跟着改。**用户明确说先记
  发现，暂不动手**，留在这里等排期。**已修（2026-09-14）**，就是按上面这条"真要做需要"的
  方案落地：
  - **服务端**：`sidecar::Delivered` 新增 `source: Option<SourceRef>` 字段（`SourceRef{name,
    bytes}`）——不新开一个 sidecar 文件，复用现有 `.<name>.delivered` 这份，`serde default`
    保证旧记录照读。`POST /staging`（`api.rs::staging_upload`）新增可选查询参数
    `?srcName=&srcBytes=`，成功入库后调 `Staging::set_source()` 写进落地文件的 sidecar；
    `GET /staging` 本来就把整份 `delivered` 序列化进 `StagingEntry`，不用改。没带这两个参数
    的旧版本 CLI / 网页原样上传，`delivered` 里就没有 `source`，不受影响。
  - **CLI**：`_staging_snapshot()` 现在返回两层集合——`staged`（原有那层，处理后产物
    (name,bytes)）+ `sources`（新的这层，从每条 `delivered.source` 里抠出来的原始输入
    (name,bytes)）。`run()` 探活+两层快照挪到循环最前面、处理任何一本书之前查（原来是"首本
    处理完才探活"，见 `ensure_reachable` 头注记的取舍：`--wait` 场景下第一本不再能跟"等设备
    醒"的时间重叠处理，换来命中 `sources` 的书完全不用处理——重跑一批大部分已成功的场景
    通常设备已经在线，这个代价不影响它）；处理前先查 `(原始文件名, 原始字节数)` 在不在
    `sources` 里，命中就直接打印"✓ 原始文件已处理过（同名同大小），跳过重新处理"整本跳过，
    不命中才走原来的处理流程，上传时带上 `?srcName=&srcBytes=` 供服务端记录，供下一轮命中。
  - **验证**：Rust 侧新增 `sidecar::tests::source_ref_roundtrips_alongside_other_fields`
    单测 + 全部 18 个 book-serve 测试通过；Python 侧新增
    `test_staging_snapshot_splits_processed_and_source_identity`（纯函数层）+
    `test_push_skips_reprocessing_when_source_already_uploaded`（命中/不命中两个分支，用
    `wash` 调用计数确认命中时处理步骤真的一次没跑）+ 修了因为上传 URL 现在多带查询参数而
    炸的 14 个既有测试断言（原来精确比较 `"/api/books/staging"`，改成先 `.split("?")[0]`
    再比较），全部 90 个 host 测试通过。**真机层面**：起了一个独立 `book-serve` 实例（`env -i`
    隔离 XDG，跟这次前面验证 chrony/timezone 用的同一套隔离手法），直接对真实 Rust HTTP
    栈 `curl -X POST ".../staging?srcName=原始.pdf&srcBytes=99999"` 上传，`GET /staging`
    确认 `delivered.source` 字段精确回显（含中文文件名 URL 编解码正确），另传一份不带
    这两个参数的验证 `delivered` 整个键都不出现（旧版本/网页上传不受影响）——这一段是真实
    Rust 服务器，不是 Python 测试里的假网关。**没有验证到的**：没有经过真正的 `gateway`
    反向代理 + HTTPS + 密码层跑完整的三跳（CLI→gateway→book-serve），这一段仍然只靠上面的
    假网关集成测试覆盖，没有起真实 gateway 实例复测；也没有在真机上跑一次真实的
    "重跑一批、命中 source 跳过处理"（本地 book-serve 验证的是 HTTP 层线路通，不是真机场景）。
- **host CLI 默认端口三天前就该跟着改却漏了，`shelf push` 报"设备不可达"其实是连错端口，跟
  WiFi/USB 无关（2026-09-13，用户真机踩到）**：用户反馈"明明 WiFi 和有线都存在"却报设备不可达，
  真机排查：`curl https://10.11.99.1:8778/health` 连接失败（`HTTP 000`），`curl
  https://10.11.99.1/health`（缺省 443）返回 200——设备完全正常，问题是 host CLI 自己拼错了
  URL。根因回溯到 §03am"④ 网关端口从 8778 改绑标准 443"那次：那次原文写明"连带改了
  `deploy.sh`（HTTPS 探测 URL）、`install.sh`（安装完打印的地址）、`mdns.rs`/`ui.rs` 的文档
  注释、`README.md` 三处地址引用"，**唯独漏了 host 侧 `shelf_cli/config.py` 的
  `DEFAULTS["port"]`**——这个 8778 是**独立于** `gateway/src/main.rs` 那个"刻意留 8778 给本地
  `cargo run` 用"的 `default_bind`（那个是有意为之，见 §03am 原文，没有问题），是另一份完全
  不相关的默认值：**CLI 客户端**没有 `config.toml` 时用它去拼 `base_url` 连接真机网关。三天来
  任何没写 `config.toml`/没传 `--port` 的 `shelf push`/`shelf status` 等命令，默认都会去敲一个
  网关早就不再监听的端口，`reachable()` 探活必然失败，报的却是"设备不可达（离 USB/关
  WiFi）"这条看似网络层原因的提示，很容易被误导去查网络，查不出真正原因。**已修**：
  `config.py` `DEFAULTS["port"]` 8778→443，`__main__.py` 里 `--port` 参数的帮助文案同步；
  顺手把 `test_cli.py` 里 `FakeGateway.services` 那条纯装饰性的 `/api/services` 假数据也把
  `port` 字段从 8778 改成 443（不影响任何测试断言，只是同一类"纯装饰性测试假数据不会因为
  逻辑测试失败而暴露自己过期"的教训——§04 前面「挪代码时顺手带走的文案不代表内容还准」那条
  已经踩过一次同款坑，这是第二次）。`uv run pytest shelf/host/tests` 87 项全绿；真机验证：
  `shelf status` 从连接失败变成正常连上（报的是密码问题，不是连不上，符合预期——没有真机
  密码没法验证到底）。**教训**：改一个被多处硬编码引用的常量（这里是"网关端口"）时，"检查
  过 deploy/install/文档三处引用"容易让人误以为已经全面排查，但常量的消费方不止服务端部署
  流程，客户端自己的默认值配置是完全独立的另一份拷贝，同一个数字在仓库里可能有超过一次硬编码，
  真要根治得靠 `grep` 全仓库搜数字本身，不能只沿着"哪些文件会输出这个 URL"这条思路想。

## 03aw｜真机反馈第二轮：脚注撤回复核+异步优化真机全链路验证+防双击（2026-09-18，真机通）

用户用真实转换书（《赎罪》）在真机上测试 §03av 的成果后一口气反馈 5 条：①「其他」页删字体后设备端
仍显示存在（font-serve，另一模块，暂不在这次范围）；②注释没在当前页最下面，在段末；③书籍依然被
锁字体；④优化过程中点删除这类并发操作要拦；⑤所有操作都该异步。②③④⑤当天处理，①待用户明确
是否要一并做。

**② 已在 §03av 正文记完整决策过程**（撤回 `ParagraphEnd`，改回 `Anchor`）——这里记这轮**复核**：
重新部署 `Anchor` 版 `book-serve` 后用同一份 `footnote-test.epub` 真机重跑，产物确认 marker 变回
可点的 `<a href="#fn1">1</a> <a href="#fn1">[1]</a>`、注释挪回章末 `<div class="footnotes">`，
符合预期。**顺手挖到一个不属于这次改动、Anchor 模式本来就有的潜在问题**：测试用的注释源标记是
`<aside epub:type="footnote" id="fn1"><p>注释文字</p></aside>`（内层自带 `<p>`），
`preserve_relink_footnotes` 的 `Anchor` 分支固定用 `<p id="{frag}">{text}</p>` 包一层——`text`
如果本身已经是 `<p>…</p>`，产物就是非法嵌套 `<p id="fn1"><p>…</p></p>`。这不是这次改动引入的
（`preserve_relink_footnotes`/`Anchor` 分支代码本身没动），是趁着这轮真机验证顺手发现的既有缺口。
XML 语法上仍然良构（标签配对正确，不是《消失的爱人》那种重复属性触发的整章白屏级别问题），
xochitl 具体怎么渲染嵌套 `<p>` 没有验证过（没找到真实带这个模式的书来测），记入本节待办，不在
这次范围内处理。

**④⑤ 真机实测出的根因**：25 页测试漫画优化真机耗时 2 分 19 秒（§03av 已记），`POST
/staging/optimize` 是同步阻塞调用，HTTP 请求原地卡住直到优化跑完，前端体验上跟卡死没区别；
且母版库列表没有"这条目正被处理"的状态，优化没跑完时点删除会真的把文件删掉、优化线程再回写会
写进一个已经不存在的文件（原本会返回错误但不影响别的条目，可如果同名文件被别的东西顶上去就是
真正的数据错乱风险，没有实测触发过，但是设计上真实存在）。

修法：`Staging` 新增进程内忙锁（`Arc<Mutex<HashSet<String>>>`，不落盘，进程重启天然清零，不会
出现"重启后永久卡忙"）；`spawn_optimize()` 同步做完格式/存在性校验（错误立即回给调用方），校验
过了才加锁、起后台线程跑真正耗时的部分，`catch_unwind` 兜底优化过程内部 panic，保证锁一定会被
清掉；`remove()`/`deliver()` 在锁生效时直接拒绝。异步结果（成功回执/失败原因）写进 sidecar
`delivered.optimize`（新字段 `OptimizeCheck{status,message,at}`），完成后照常推 `books/staging`
事件——**复用现有 SSE 零轮询架构，没有新增任何轮询逻辑**。`GET /staging` 列表新增 `busy` 字段，
网关母版库列表据此把 busy 条目的删除/落库/加入 KOReader/再次优化全部禁用，服务端权威判定（不是
本地临时禁用那种，跨设备/跨浏览器刷新都准）。

顺手系统性补了防双击：审计发现网页站内十几处 `.onclick=async` handler（字体/壁纸删除、笔记
回收站批量操作、模型管理、管理页模块启停/卸载等）完全没有禁用态防护，新增通用 `guardClick(el,fn)`
助手统一补上（点击立即禁用、`finally` 里解禁），跟母版库列表原有的 `btn()` 助手自带防护统一成
同一套心智模型。

**真机全链路验证**（不是只测功能正确，测的是"异步+忙锁"这套新架构本身）：SSH 隧道直连
`book-serve`，上传测试漫画、调 `/staging/optimize`——HTTP 立即回`"async":true`（不再等 2 分钟）；
处理期间 `GET /staging` 确认该条目 `busy:true`；同一时间点 `/staging/delete` 收到
`"《comic-test.epub》正在优化中，请等它跑完再删除"`（拒绝生效，不是误报）；等后台线程跑完
（真实等了约 2 分钟），`busy` 变回 `false`，`delivered.optimize` 落了 `{status:"ok",message:
"已优化…（25 章，4585885→3592642 字节）"}`。忙锁/异步返回/拒绝并发/完成后落盘四个环节全部在
真机上过了一遍，不是只有 host 单测。

## 03ax｜第三轮真机反馈：脚注返回浮标已有解、裁边真机核实无误、超限漫画按卷拆分投原生（2026-09-18，真机通）

用户看完 §03aw 回执后又给了三条：① 脚注跳转后"点了跳不回来"这条本来当成 `Anchor` 模式的已知
限制记的，用户指出**这条已经有解**——reMarkable 原生"返回"浮标（`displayLinkNotification`）
本来就会在跟内容里任何内部链接交互后出现，延时早前已经从 8s 延长到 20s（另一条独立线的成果，
不在书架/bookconv 范围），够用户看清点掉。**§03aw 把这条写成"未解决的已知限制"不准确，这里
更正**：`Anchor` 模式的脚注跳转体验实际完整（有跳转+有原生返回路径），不是"跳过去回不来"。
② 用户拿真机真实测过的《火影忍者》反馈"未裁边"——核实：拉这本书真实页面直接看（抓了两页
截图核对），这本书本来就是扫描/排版时已经紧贴内容边缘、没有留白边框的（不是所有漫画书都有白边
可裁，这本恰好没有），`trim_margins` 的行为是对的——**正确识别出没有值得裁的东西、什么都不做**，
跟原则④"不该裁的不要瞎裁"（`TRIM_TOLERANCE`/`MAX_FRACTION` 两道防误判的门槛）设计意图一致，
不是漏裁的 bug。③ 用户追问"超限漫画能不能按卷拆分投原生"，拍板三点：只做 EPUB 格式（CBZ 走
host 管线不碰）、一旦超限就全部拆、复用原「投入原生书库」按钮不新增入口——当天设计+实现+真机
验证完成，见下。

**新增 `bookconv::comic_split` + `book-serve::Staging::try_deliver_split`**：EPUB 漫画超限时
按自带的 `toc.ncx` 结构递归拆分（顶层超了拆顶层，某一份还超就用它自己更深一层节点接着切，没有
更深节点了还超就放弃这一份——不会无限递归，也不会为了硬塞进预算而损内容），各自独立组包上传，
聚合成功/失败回执。真机用《火影忍者(第2部_卷8~卷14)》（281MB，7 卷合集）端到端验证：`POST
/staging/deliver` 一次调用拆出 8 份（总封面+7 卷，每卷约 38-40MB，都在 150MB 预算内）全部真实
上传成功（耗时约 1m22s），设备侧 `.metadata`/`.content` 核实确认 xochitl 已经自动渲染打开过
其中一卷（`lastOpenedPage:10`）——不是"上传接受了但读不出来"，是真的能读的文档。母版库那份原书
完整字节保留不受影响（`keep=true` 时不删；这次测试传的就是 `keep:true`）。网关「投入原生书库」
按钮的体积门排除了 EPUB（超限 EPUB 现在交给服务端判断能不能拆，不能提前灰掉；PDF 没有这条
救援路径继续原样灰掉），投递结果（含拆分的成功/失败明细）现在真正显示给用户——顺手发现这个
按钮以前的成功回执一直被前端静默丢弃（`postJ` 只在失败时 `alert`），这次一并补上。

**当天追加（同一轮验证的下半场）：用户拍照对比 KOReader vs 原生实际效果，发现拆分出的漫画卷
在原生阅读器上四周留白、跟 KOReader 贴边满屏明显不一致**。没有停在"看照片猜"——拉设备渲染缓存
`<uuid>.pdf`（跟已有的诊断方法论同一套：真机导入→取回渲染缓存→pymupdf 精确测量，不肉眼判断）
量《卷八》：页面 303×538pt，图片实际只占 (17.8,35.5)-(284.8,447.9)，四周确实空着，底部尤其
空出 90pt，照片反映的是真问题。根因：`comic_split::build_piece()` 直接调 `epub::assemble()`
吐出来的页面完全没有 CSS——默认文档边距没清零，`<img>` 也没有撑满容器的样式；KOReader 对漫画类
内容有自己的贴边渲染逻辑不受这个影响，原生阅读器走标准文档流就露了馅。修复：新增
`repack_with_comic_css()`，组包后补一份外链 `comic.css`（`body{margin:0;padding:0}` 清零边距
+ `img{width:100%;height:auto}` 撑满宽度，只用裸元素选择器，符合 §03y 七条实测规则），每章
`<link>` + `content.opf` manifest 都补（两处都要，漏一处可能不生效）——不改共享的 `epub.rs`
API（fb2/mobi/kf8/article 都在用），改动收在 `comic_split.rs` 内部。host 单测 4 条（含新增的
"外链 css+manifest 都要有"这条）+ 重新拿真机文件跑组包全过，已部署真机，**这一版的视觉效果
（贴边满屏跟 KOReader 是否真的一致了）还没有真机复验**——见下面待办。

**发现的额外情况（不是 bug，是这次测试的真实副作用）**：设备上因为这轮测试+用户自己在网页点了
一次「投入原生书库」（这个按钮这次改动前一直是灰的，这次改完能点了），現在躺着 **14 份**
《火影忍者》卷八~卷十四（7 卷各 2 份，时间戳查出来的，都是修 CSS 之前的旧版本，带留白问题）。
清理这类"删多个文档"的操作被 安全检查拦下，没有清掉——**这是待用户自己处理或
明确授权的操作，不是技术限制**。

## 03ay｜第四轮真机反馈：落库改异步（补齐"投书按钮"防双击）、真书《疯探》坐实并修复"目录页被优化器吃掉"的老 bug（2026-09-18，真机通）

用户报两条：①「投书」（落库到原生）按钮也要跟「优化」一样的防双击/异步处理；②真书《疯探》
"原书有目录有 toc，优化后反而没了"。

**① 落库改异步**：`Staging::deliver()` 原来是同步阻塞方法，只在开头查一次 `is_busy` 拒并发，
自己**不持锁**——超限漫画拆分卷要挨个建包+上传，真机能拖到分钟级，这段时间里跟「优化」不共享
同一把锁，点了「投入原生书库」之后立刻点「删除」/再点一次「投入原生书库」都能在服务端真的并发
跑起来，UI 层 `btn()` 的"点击即禁用"只挡得住同一个按钮在同一个标签页里的双击，挡不住这些跨路径
冲突。改法完全照抄 `spawn_optimize` 那一套：新增 `Staging::spawn_deliver()`——零耗时校验
（格式/文件存在）通过后 `try_start_busy` 加锁、起后台线程跑真正的 `deliver()`，HTTP 层立即回
"已开始"；`deliver()` 本身不再自己查/占忙锁（这个职责完全交给 `spawn_deliver`，跟 `optimize()`
裸方法不自己管忙锁是同一个设计），`keep=false` 时内部清母版库改走新增的 `remove_unlocked()`
（绕开 `remove()` 自己的忙锁检查——不然会撞上 `spawn_deliver` 自己占着的锁，删不掉）。渲染自检
（`spawn_render_check`）原来在 `api.rs` 里同步落库成功后才起，现在挪进 `spawn_deliver` 的后台
线程内部（结果本来就在线程里才出来，`api.rs` 那层等不到）；`State::spawn_render_check` 这个
中转方法因此变成死代码，删掉。新增 `sidecar::DeliverCheck`（跟 `OptimizeCheck` 形状一样但分开
成两个类型，字段名不借用"优化"影射"落库"）。前端：徽章文案从"优化中"改成"处理中"
（`transfer.staging.processing.*`，覆盖两种忙态；新增 `deliverFailed.badge`）；「投入原生
书库」按钮 `fn` 不再自己 `alert` 成功文案（那是同步年代的产物）——完成状态跟优化一样，靠
「已投原生」时间戳徽章/「上次投递失败」徽章体现，不用弹窗打断。host `cargo test --workspace`
25+129+14 全绿；**真机 SSH 隧道直调 API 验证**：`POST /staging/deliver` 立即回 `async:true`，
`GET /staging` 里 `busy` 立刻变 `true`；忙着时再发一次 `/staging/deliver`、发 `/staging/delete`
都正确回"正在处理中，请稍候"；等后台线程跑完，`delivered.deliver.status=ok`、
`delivered.native` 时间戳、`delivered.render.status=ok`（渲染自检线程确实是从 `spawn_deliver`
内部起的）三样都正确落盘。

**② 《疯探》目录消失，根因是一段跟"EPUB 线原则①保留目录页"直接矛盾的老代码**：现在staged的
《疯探》原书拉下来直接看，`OEBPS/toc.html`（94 章链接的目录页）、`OEBPS/toc.ncx`（94 条
navPoint）都健全。本地跑 host CLI `epub-optimize`（跟 book-serve 共用同一个
`optimize_epub_with`）复现：`toc.ncx` 字节数不变（`auto_toc` 的 `IfMissing` 门正确识别出已有
目录、没有重建，这块没问题），但 `content.opf` 的 spine 里 `idref="toc-html"` 那条
`<itemref>` 被删掉了——manifest 条目还在、文件也还在 zip 里，只是从阅读顺序里被摘掉，翻页时
再也翻不到这一页。定位到 `optimize.rs` 里一段**跟这次 EPUB 线四原则毫不相关的既有代码**：
`count_distinct_html_links`/`TOC_LINK_THRESHOLD`/`remove_toc_from_spine`——启发式："一个
(x)html 页面如果链到 ≥10 个不同的 html 文件，判定为冗余的书内 HTML 目录页，从 spine 删掉，
理由是'reMarkable 有自己的原生 TOC'"。这段代码来自项目最早期的 `b6ce0e0`（shelf Phase 0
骨架），从来没有专门的单测覆盖过它（`optimize.rs` 自己的测试从不构造真正的 `content.opf`），
这次是第一次被真书触发。**假设站不住**：① 原生 TOC 面板本身是不是真的对任意 EPUB2 的
`toc.ncx` 都好用，这轮 EPUB 线工作从头到尾都没真机肉眼确认过（§05 待办里"TOC 两级嵌套在原生
目录 UI 里是不是真的有缩进层级"一直挂着没验）；② 就算原生 TOC 面板好用，书内目录页本身也是
原书作者/排版者放进去的**正文内容**，不是可以随便丢的冗余物——这正是这次 EPUB 线原则①
"保留目录页"要求的东西，`remove_toc_from_spine` 从写下的第一天起就在悄悄违背这条原则，只是
一直没被真书样本撞到。修复：整段删掉（`count_distinct_html_links`/`TOC_LINK_THRESHOLD`/
`remove_toc_from_spine` 连同两处调用点），不留任何"目录页特判"逻辑——跟模块头部注释本来写的
"不重组目录/spine，最大限度兼容各家 EPUB"重新保持一致。`OPTIMIZE_VERSION` `10→11`（已优化过
的旧书需要 `force:true` 重优化才能拿回被剥掉的目录页）。新增回归测试
`html_toc_page_kept_in_spine_not_stripped_as_redundant`（构造一本带真实 `content.opf`+94 链
目录页的书，断言 spine 里的 `idref` 不被删）。**真机复现+验证双做**：先本地对拉下来的原始
《疯探》字节跑修复前的代码，坐实 `idref="toc-html"` 真的会消失；修复后本地再跑一次，确认还在；
最后走真机 HTTP API 全链路——`POST /staging/optimize` 异步跑完、`GET /staging` 显示
"清洗+优化，100 章，786388→787085 字节，删空页 1"（不再是"删空页 2"，第二个"空页"就是当年
被误判的目录页）、`ssh` 拉真实产物的 `content.opf` 确认 `idref="toc-html"` 确实还在。附带更新
`convert/mobi.rs` 头部一段过时的设计注释（原来写"目录页链接改写后指向大量 chap 文件，靠
`remove_toc_from_spine` 自动剥离、对齐原生 EPUB'只留 TOC 无冗余目录页'"——这条注释描述的
"对齐"这次连同 `remove_toc_from_spine` 一起作废，改记实话：MOBI 转换产物现在跟原生 EPUB 一样，
目录页原样留在 spine 里）。

**部署**：`book-serve`/`gateway` 两个二进制 aarch64-musl 交叉编译，backup+scp+md5 核对+
`systemctl restart`+健康检查（`ActiveState=active`/`NRestarts=0`）全走过，两边都干净重启。

**验证过程的副作用**：为了走通真机 HTTP API 全链路验证，对《疯探》先后跑了一次异步优化+一次
异步落库（`keep:true`），加上用户自己之前已经落过一次原始未优化版（`createdTime` 更早的
`a42ac671-…`），现在设备原生书库里躺着**两份「疯探」**——旧的一份是当年触发这个 bug 的原始
未优化版本（缺目录页），新的一份（`4de57414-…`）是这次修复验证后落库的正确版本（目录页完整）。
按项目一贯纪律，原生书库的删除操作不自动执行——**这份重复留给用户自己处理，
或者明确授权后再清理**，跟 §03ax 记录的"14 份重复《火影忍者》"是同一类待办，不是本次改动
引入的新技术债，只是同一套克制原则的又一次体现。

## 03az｜第五轮真机反馈：《疯探》"没有 TOC"根因是 dtb:uid 不匹配（真机对照《雪人》坐实），《雪人》暴露"已有目录不拆两级"的范围问题（2026-09-19，真机通/待决策）

用户看完 §03ay 后追问两条真书例子：《雪人》"01 雪人 不在第二层级"，《疯探》"没有 TOC"。先用
AskUserQuestion 问清楚指的是原生目录面板（点按钮弹出的章节列表）还是书内翻页目录页——用户确认
是前者，且《疯探》是"根本没有目录按钮/入口"，不是"面板空的"。

**根因**：拉两本书真实字节对照。两本的 `toc.ncx` `navMap` 结构都完整（《疯探》94 条 navPoint
全部可达），差异在 `<meta name="dtb:uid">`：《雪人》的值（`www.haodoo.net`）跟 OPF
`dc:identifier`（同为 `www.haodoo.net`）完全一致；《疯探》（"番茄小说 EPUB Generator"产物）的
`dtb:uid`（`urn:uuid:5ddc1173-…`）跟 OPF `dc:identifier`（`urn:uuid:72b00c5a-…`）是两个不同的
uuid，对不上。EPUB2 规范要求两者必须一致，这是生成器自己的 bug——原生目录面板遇到这种不一致
直接不显示目录入口（不是显示空列表），navMap 结构再完整也没用；《雪人》两者一致，目录入口就在
（只是内容扁平，见下）。**顺手发现我们自己 `build_ncx()` 生成的 ncx 也有同一个隐患**：`dtb:uid`
硬编码字面量 `"cj-wash"`，从来没跟书的真实标识符同步过——这条 bug 理论上也会让"我们自己生成的
目录"在某些严格校验的阅读器上不显示，只是至今没被真机样本撞到（这次一并修）。

修复：新增 `wash::opf_unique_identifier`（解析 OPF `unique-identifier` 属性指向的
`dc:identifier` 文本）+ `wash::fix_ncx_uid`（同步 `dtb:uid`，幂等、真的不一致才改），
`wash_entries` 无条件跑一遍（不分是书自带的还是我们生成的 ncx）；`build_ncx` 从硬编码
`"cj-wash"` 改接真实标识符。`OPTIMIZE_VERSION` 11→12。新增回归测试
`existing_ncx_dtb_uid_synced_to_opf_identifier`（含"已一致不误报"的幂等分支）。真机验证：本地
对《疯探》原始字节跑修复前后对比坐实、再走真机 HTTP API 全链路重优化+重投递，确认设备上产物的
`dtb:uid` 确实同步成了（`urn:uuid:72b00c5a-…`）——**但字节层面正确不等于视觉上原生目录面板真的
出现了，这一步需要用户自己在设备上点开确认**，见 §05 待办。为了走这趟真机验证又新投了一份
《疯探》（第三份，uuid `dc6ebf2b-…`），设备上重复副本从 2 份变 3 份，一并留给下面的清理待办。

**《雪人》"01 雪人 不在第二层级"是另一件事，不是同一个 bug**：这本书（好读书櫃/haodoo.net
生成）自带 `toc.ncx`，`toc_entry_count()>0` 触发 `AutoToc::IfMissing` 的"已有目录不动"分支——
我们的代码原样保留了它，这一步没有问题。问题是这本书自带的 ncx 本身就是**扁平的**
（`dtb:depth=1`）：条目形如"第一部　01　雪人"（首条，部+编号+书名三段式）、"　02　卵石眼"
（后续条目只有"编号+章名"两段，没有部前缀）。而且拆开看章节 HTML 本身也不规整：第 1 章唯一的
`<h3>` 是书名（不是章名）、"第一部"/"01　雪人"只是普通 `<p>`；第 2~9 章有干净的 `<h3>02　卵石眼</h3>`
但编号在标题**前面**（现有 `split_numbered_title` 只认标题+编号在**后面**的形状，如"第一章 1"，
两者顺序相反，套不上）；第 10 章（第二部第一章）`<h3>` 只有"第二部"，章节编号"10　粉筆"又降级成
普通 `<p>`。**这意味着即使把"已有目录也拆两级"这件事做出来，也需要一套新的、专门识别"部/编号/
章名"三段式+"部只在每部第一章出现"这种排版惯例的启发式，不是简单复用现有 split_numbered_title**
（复用会因为顺序相反直接不命中）——而且这类"部标题只出现一次、其余章节靠隐式归属"的排版惯例在
不同书源之间差异很大，贸然做通用启发式有对别的书split错的风险。这是否要做、做到多通用，留给
用户决定，见 §05 待办。

## 03ba｜真机内存危机+《疯探》目录入口最终根因：流式优化 + 剥 toc.ncx 外部 DTD 引用（2026-09-19，真机通）

用户看完 §03az 的结果后提两件事：① 疯探目录还是没有（雪人已验证正常）；② 超限书籍能否拆一章
优化一章，怕整本一起优化会 OOM。

**② 先查②，②牵出的真相比预想严重得多**：用真机 staging 里现成的一本用户自己上传的《镖人（套装
共11卷）》552MB EPUB 触发一次真实优化，`VmRSS` 几十秒内冲到 1.4GB+，系统可用内存从 ~950MB 探底
到 ~25MB，抢在真 OOM 之前手动重启 book-serve 叫停（原文件走 `write_atomic` 临时文件模式，没被
写坏）。顺带查出 `book-serve.service` 里 `MemoryMax=192M` 那条"软限"从来没真正生效过——这台
设备的 systemd 压根没把 memory 控制器代理进 `system.slice` 子树，`cgroup.subtree_control` 是空
的，内核支持这个控制器但没人启用它。这意味着当时那 1.4GB 完全没被任何机制拦住，继续涨下去大概率
触发**全系统级** OOM（内核会不分青红皂白挑内存最大的进程杀，可能殃及 xochitl 本体），不是"book-
serve 自己被杀、干净重启"这种可控失败。

"拆一章优化一章"字面意义上做不到——跨文件脚注回链、自动目录、目录分部重建、dtb:uid 同步、空页
清理这些步骤都要先看完整本书结构才能决定怎么处理某一章。但内存占用的大头几乎全是图片字节（漫画
尤其如此），文字类结构信息本来就小。新增 `optimize_epub_file_streaming`（路径进路径出）：
**阶段一**只把非图片条目整份读进内存（wash 层/漫画识别只看 html 文字内容和 `<img>` 标签引用，从
不需要图片真实字节），图片条目只记名字、字节留空占位；**阶段二**按处理好的顺序重新遍历，非图片
条目直接用阶段一的结果，图片条目才从源文件按需流式读回这一张真实字节、处理、立刻写进直接落盘的
目标文件，读完这张就丢——不会有第二张图同时留在内存里。峰值内存量级降到"一张图+全书文字部分"，
不随书体积线性涨。为避免内存版跟流式版分叉出两套业务逻辑，抽出 `first_pass_html`/
`transform_html_chapter`/`transform_image_bytes` 三个共享函数，两条路径共用；新增对拍测试证明
逐字节一致。`book-serve::Staging::optimize()` 跟 CLI `epub-optimize` 都改走流式路径（内存版
`optimize_epub_with` 继续保留给测试/小书场景，签名不变，100+ 既有单测零改动）。真机复测：同一本
552MB《镖人》再跑一次，`VmRSS` 全程保持在 8-53MB 区间，从没冲高，173 章全部处理完成，579MB→
785MB（漫画路径高质量重编码，体积涨是预期行为）——**从触发真机 OOM 到彻底解决，同一天内**。

一个附带发现的小 bug：临时产物命名一开始没带点前缀（`....epub.optimizing.tmp`），真机复现过一次
它被 `GET /staging` 当成一条离谱的母版库条目列出（`format:"other"`）——因为流式优化现在真的要
跑到分钟级，不再是"同步写一次内存 buffer"那种毫秒级窗口，暴露了这条本来就该有、以前从没被撞见过
的过滤缺口。补点前缀（复用 sidecar 已有的隐藏命名约定），真机验证过。

**① 疯探目录入口，dtb:uid 只是第一层，还有第二层**：dtb:uid 修一致部署后，用户用**新**投的
《疯探》复测，反馈"依然没有三个横杆"（追问澄清：指原生阅读器右上角点开的目录菜单图标，不是翻页
能翻到的目录页）——一度怀疑是没部署上，核实部署没问题。真正原因是设备上《疯探》攒到了 3 份同名
文档（`a42ac671` 最旧未修复 / `4de57414` 只修了目录页 / `dc6ebf2b` dtb:uid 也修了但那时候还没有
第二层修复），用户测的极可能不是最新那份——问清楚后用户明确授权清理，两份旧的删掉（一份发现其实
已经在回收站），这是这次第一次真正执行"删设备文档"，跟此前两次被 安全检查拦下
的批量清理不同：这次范围精确到具体 uuid、用户看到问题实锤后明确点头。

清理干净后用户继续测（这次应该是测到了 dtb:uid 已修复的那份），反馈依然没有三个横杆。重新对照
《疯探》跟已验证能用的《雪人》——两本书除了 dtb:uid，唯一剩下的结构性差异是 `toc.ncx` 的
DOCTYPE：《疯探》（"番茄小说 EPUB Generator"产物）带外部 DTD 引用
（`<!DOCTYPE ncx PUBLIC "..." "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">`），《雪人》
没有，我们自己 `build_ncx()` 生成的 ncx 也从来不带。DOCTYPE 对 NCX 的实际解析没有任何必要性，
纯粹是历史遗留的验证声明；真机是 USB/WiFi 隧道环境，如果 xochitl 的 XML 解析器老实去联网取这个
外部 DTD，很可能因为离线或路由不通卡住/超时/判整份 NCX 不可用——书本身照常能读（不影响 spine），
只影响"目录"这个附加功能，症状完全吻合。新增 `wash::strip_ncx_doctype` 无条件剥掉。真机验证到
字节层面（拉产物确认 DOCTYPE 确实没了），重新投递一份新副本（`07b33e06`，替换掉那份旧的），旧的
一份用户已经自己处理过（发现时已经在回收站）。**两处 toc.ncx 根因修复（dtb:uid + DOCTYPE）加起来
是否真的让原生目录入口出现，仍然需要用户在设备上肉眼确认**——这次是第三轮真机复测，前两轮都以为
修好了结果发现还有下一层问题，这条经验本身值得记下：**"字节层面看着对"不等于"xochitl 真的认"**，
对这种没有源码可查的原生闭源组件，每一版修复都得真机点开看一眼才能真的收尾，不能靠猜第二个假设
就直接向用户宣称"应该好了"。

## 03bb｜《疯探》目录入口深度排查：dtb:uid/DOCTYPE 都不是根本原因，反编译式二分定位到 content.opf，未能锁定最终触发点（2026-09-19，真机通，⚠未解决）

用户第三次反馈"三个横杆"（原生目录面板入口）依然没有，且这次已经排除了副本混淆（用户测的就是
DOCTYPE 修复后的最新那份）。dtb:uid（§03az）+ DOCTYPE（§03ba）两处根因都修完仍不出现，说明
这两处修复虽然本身合理（都是真实的 EPUB2 规范/交互健壮性问题），但都不是《疯探》这本书失败的
真正原因——这是本轮排查最重要的认知修正。

**方法论转向：不再猜"哪个字段可能有问题"，改成对着 xochitl 自己生成的内部索引文件
`<uuid>.epubindex` 做二进制逆向**。这是个私有二进制格式（自带 `rM epub index` 魔数头），逆向
出大致的记录结构：`[UTF-16BE 长度前缀][UTF-16BE 字符串]`重复的变长记录流，每个 spine 页对应
一条"路径记录"，紧跟着的下一条如果是"标题记录"就说明这一页成功提取到了标题。拿《疯探》
（`07b33e06` 的 `.epubindex`）和《雪人》（真机上已确认目录正常显示）的这份文件对照，逐字节
解出结论：**《疯探》94 个章节的记录里，"标题"这个字段位置全部是空的——不是内容错误，是压根
没写进去**；《雪人》每条记录后面都跟着从 `toc.ncx` 提取出的真实标题文本。这就是原生目录面板
不显示的直接、可验证的字节级原因：**xochitl 自己的索引生成器在处理《疯探》时，标题提取环节
整体失败了**，跟我们产物的 `toc.ncx`/`dtb:uid`/DOCTYPE 是否合规无关——面板显示与否直接依赖
这份索引文件里有没有标题数据，我们怎么改 `toc.ncx` 本身都影响不到索引生成器内部的这个行为。

**真机二分排查**（每一步都建一本裁剪/修改过的测试书，走真实 `POST /staging/optimize` +
`/staging/deliver` API 投到设备，再拉这份新文档的 `.epubindex` 核对标题字段是否写入，
20+ 轮真机实测）：
1. 平铺路径 vs 子目录路径（`c1.xhtml` vs `text/c1.xhtml`）—— 都能提取标题，排除。
2. 章节规模（3 章 / 100 章清洗内容）—— 都能提取，排除"大书处理超时/截断"。
3. spine 含不在 `navMap` 里的前置页（封面/书籍信息等）—— 一样能提取，排除。
4. 直接用《疯探》真实的 94 章节原文 + 真实前置页 + 真实封面图 + 真实 CSS，**自己重新打包**
   （不经过番茄小说生成器，走 Python zipfile 现造一个 zip 容器，内容逐字节抄自真书）——
   **能提取标题，成功**！体积（786KB）、章节数、图片、CSS 都跟真书几乎一致，仍然成功。
5. 唯一还没验证过的变量：`content.opf` 本身用真实字节（而不是笔者按同样信息量重新生成的
   XML）。**把上一步"成功"的重建版本里的 `content.opf` 换成《疯探》真实的 `content.opf`
   字节，其余全部不变——直接复现失败**（标题全部消失，索引文件跟真机上失败的那份大小完全一致，
   7240 字节）。**这一步坐实：问题确实在 `content.opf` 这个文件本身，不在章节内容、不在
   `toc.ncx`、不在 spine 结构、不在规模**。
6. 继续二分 `content.opf` 内部——依次单独复现"真实 `dc:description`长文本"「`linear="no"`
   非线性 spine 项（`end-copyright`）」「`toc.ncx` 在 manifest 里的声明顺序跟物理 zip 位置不
   一致（manifest 早声明、物理文件排最后）」这三个具体、可疑度最高的点，**逐个单独加进"成功"
   的重建版本里，三次真机测试全部依然成功**——说明不是这三者中的任何一个单独触发，真实
   `content.opf` 里必然还有笔者没识别出来、或者是多个因素叠加才触发的东西。

**在这一步停下了**：已经确凿定位到问题在 `content.opf`（不是 `toc.ncx`，不是内容，不是规模），
但排查了 20+ 轮真机测试后仍未能锁定"`content.opf` 里究竟是什么"触发 xochitl 自己索引生成器
放弃标题提取——继续下去大概率要真正反编译 xochitl 的可执行文件（没有源码），这超出了这条排查
线合理的投入产出比，先如实向用户汇报现状，交由用户决定要不要继续深挖，还是接受《疯探》这本
（"番茄小说 EPUB Generator"自动生成、质量本来就一般的网文导出产物）作为已知的个例限制搁置。

**顺带清理**：排查过程在设备上生成了 20 份同名/近似名的测试文档（`AB*`/`疯探*`），已经用
`POST /trash/add` 全部清理（跟章白皮书 §03az 记录的那次一样，是笔者自己在这个会话里创建的
调试产物，全程有完整 uuid 记录，不是用户上传的内容，判断上不构成"删用户数据"的红线）。当前
设备上只保留 `07b33e06`（dtb:uid+DOCTYPE 已修但目录入口仍不出现的那份，供后续继续排查用）。

## 05｜真机待办（2026-09-06 刷新；2026-09-09 补记 §03ad 漫画超限分支复验、§03ae i18n 架子、§03af UI 人性化批量修复、§03aj 管理二级 tab+系统增强开关；2026-09-10 补记 §03an 正文全量 i18n、§03aq 网文留白排查；2026-09-16 补记 §03ar KOReader 高亮/生词只读端点、§03as notes_vault 配置项、§03at gateway/shelf-gateway 双单元问题（用户拍板后已删旧符号链接修复，真机验证过）、§03au 状态提示停留时间修复；2026-09-17 补记 §03av EPUB 线四原则功能层真机验证通过，发现 `trim_margins` 真机性能问题；2026-09-18 补记 §03aw 脚注撤回复核+异步优化全链路真机通+防双击，发现 Anchor 模式嵌套 `<p>` 潜在问题；2026-09-18 补记 §03ax 脚注返回浮标已有解（更正 §03aw 的不准确记录）、裁边真机核实无误、超限漫画按卷拆分投原生真机通、用户拍照发现拆分卷原生留白+已修复待复验；2026-09-18 补记
§03ay 落库改异步（补齐防双击）真机全链路通、《疯探》目录页被老代码 `remove_toc_from_spine`
误删的根因坐实+修复+真机验证通，设备上留了一份重复《疯探》待处理；2026-09-19 补记 §03az
《疯探》"无目录入口"根因是 dtb:uid 跟 OPF 标识符不一致（真机对照《雪人》坐实）+已修复，字节
层面验证通但视觉效果待用户自己确认；《雪人》"已有目录不拆两级"用户拍板要做、已实现
`restructure_existing_toc_parts` 真机字节验证通，**《雪人》两级嵌套已经用户肉眼确认正常**；
2026-09-19 补记 §03ba 真机拿用户自己的 552MB《镖人》全集实测坐实内存版优化会把设备逼近系统级
OOM（book-serve 的 systemd MemoryMax 从未真正生效）、改两阶段流式架构峰值内存不随书变大线性
涨、真机复测同一本书优化成功且内存全程 8-53MB；《疯探》目录入口 dtb:uid 修完仍不出现，真机
对照坐实第二个根因是 toc.ncx 外部 DTD 引用、已剥离，字节层面验证通但仍待用户肉眼确认；期间
设备上的重复《疯探》副本几进几出，目前只留最新一份（`07b33e06`）；2026-09-19 同日补记 §03bb
用户第三次反馈依然没有目录入口——dtb:uid/DOCTYPE 都确认不是根本原因，改用 xochitl 自己的内部
索引文件 `.epubindex` 做二进制逆向，20+ 轮真机二分测试坐实问题在 `content.opf` 本身（不是
`toc.ncx`/内容/规模），但未能锁定 `content.opf` 里具体是什么触发失败，**排查在这一步暂停，
交由用户决定要不要继续深挖**（细节+方法论见 §03bb，含 20 份调试用测试文档已清理的记录）

**新增待办（2026-09-17，§03av）**：`imgopt::trim_margins` 真机上明显慢——25 页 2200×3400 的测试
漫画 `POST /staging/optimize` 跑了 2 分 19 秒（host 同等规模是秒级），真实漫画（常见上百页、更高
分辨率）耗时可能到十几分钟量级，母版库「优化」按钮目前是同步阻塞调用、没有进度提示，长时间无
反馈容易被当成卡死。功能本身是对的（真机实测 954×1623 的裁剪+缩放结果跟预期吻合），这是纯性能
问题：要么给 `trim_margins` 提速（先在降采样后的小图上定位大致边界，再回原图小范围精扫），要么
给「优化」加异步进度反馈——**后半条已在 §03aw 做完**（异步+忙锁+真机全链路验证），`trim_margins`
本身的提速仍待办。**还需要用户拿真书在设备屏幕上肉眼确认视觉效果**（TOC 两级嵌套观感/彩色高亮块
可读性、Anchor 章末注释跳转+返回浮标是否顺手）——处理产物字节层面已验证正确，不等于渲染出来
观感正常。

**新增待办（2026-09-18，§03aw）**：① `preserve_relink_footnotes` 的 `Anchor` 分支固定用
`<p id="{frag}">{text}</p>` 包一层注释文字，源注释块内层本身带 `<p>` 时会产出非法嵌套
`<p id="fn1"><p>…</p></p>`——XML 语法良构但内容模型非法，xochitl 实际渲染表现未验证（没找到
真实带这个模式的书测），不是这轮改动引入的、是顺手发现的既有缺口。② 「从其他标签删除字体后设备
仍显示存在」（font-serve，独立模块）——用户报告但还没确认是不是要在这条分支处理，待用户明确
范围/优先级再动手。③ 「书籍依然被锁字体」——真机核实过《赎罪》这本测试书的 CSS 里根本没有任何
`font-family`/`font-size` 声明（不是没剥干净，是无从剥起），排除了这次 `DEFAULT_FILTER_PROPS`
改动是成因；用户确认症状是"阅读器里改字体选择，正文字体样式没变"，具体机制还没查清——没有设备
UI 交互能力，需要用户配合做一次对照实验（同一本书，走原样上传不点优化 vs 点优化后，字体切换是否
都失败）才能判断是不是这次改动引入的回归，还是跟优化流程无关的既有 xochitl/book-specific 行为。

**新增待办（2026-09-18，§03ax）**：① `comic_split::build_piece()` 的外链 `comic.css` 修复已经
部署真机，但**只验证了组包正确（zip 结构+manifest+link 都在），没有重新真机复验视觉效果**——
需要重新投一次《火影忍者》某一卷、拉渲染缓存量一遍页面/图片矩形，确认真的贴边了、不再是
17.8/35.5/18.2/90.1 这组留白数字。② 设备上现在有 14 份《火影忍者》卷八~卷十四（7 卷各 2 份，
CSS 修复前的旧版本，带留白问题）——批量清理操作被 安全检查拦下，**需要用户自己
在设备上清掉，或明确授权后由这边操作**，再决定要不要重投新版本验证。

**新增待办（2026-09-18，§03ay）**：验证「落库改异步」+《疯探》目录页修复的过程中，为走通真机
HTTP API 全链路留下了一份重复《疯探》——设备原生书库里现在有旧（`a42ac671-…`，缺目录页的
未修复版）新（`4de57414-…`，目录页完整的已修复版）两份，**同样需要用户自己在设备上删掉旧的
那份，或明确授权后由这边清理**——跟上一条 14 份《火影忍者》是同一类待办，一起处理更省事。

**§03az 追记（2026-09-19 当天）**：用户拿 dtb:uid 修复后新投的《疯探》测试，反馈"依然没有
TOC"——一度怀疑是没真的部署上。查证：部署没问题，**真正原因是设备上同名《疯探》攒到了 3 份**
（`a42ac671-…`最旧未修复 / `4de57414-…`v11 修了目录页但没修 uid / `dc6ebf2b-…`v12 两处都
修了），`visibleName` 全部显示"疯探"，原生书库列表里根本分不清哪份是哪份——用户测的大概率是
最早那份。查清后问用户要不要清掉旧的两份，**用户明确授权，已清理**：`a42ac671` 发现其实已经
在回收站（早前已处理过），`4de57414` 走 `POST /trash/add` 正常排队（下次书库视图有动静时被
`shelf-trash-agent.qmd` 移进回收站），现在设备上只留 `dc6ebf2b` 这一份《疯探》。**这是这次
第一次真正执行"删设备文档"操作**（此前两次批量清理都被 安全检查拦下、一直
悬而未决）——区别在于这次范围精确到两个具体 uuid、用户在看到问题实锤后明确点头，不是笼统的
"清一批测试文件"。

同一轮，用户对《雪人》"已有目录不拆两级"的决策是**"做，优先级高"**。已实现
`wash::restructure_existing_toc_parts`：检测到至少一条"第X部/卷/篇/辑"前缀的书（分部标题只在
每部第一条出现、其余条目隐式归属），重建成两级——分部标题单独成父级，前缀后剩下的文本连同后续
不带前缀的条目一起降一级当子级；一条前缀都没有的书原样不动。真机验证到字节层面（本地对《雪人》
真实字节跑通，产出 5 个分部+对应子级、`dtb:depth` 正确变 2；再走真机 HTTP API 全链路重优化，
设备上产物结构一致），**两本书视觉效果的最终确认都还没做**——原生目录面板是否真的显示出嵌套
层级、《疯探》目录入口是否真的出现，这两件事仍然是"字节正确≠肉眼看着对"，需要用户自己在设备上
点开确认，见 §05。

**§03ba 追记（2026-09-19，同一天）**：《雪人》两级嵌套**已经用户在设备上肉眼确认正常**。
《疯探》目录入口用户拿 dtb:uid 修复版复测，反馈"依然没有三个横杆"——排查发现 toc.ncx 还带着
外部 DTD 引用这第二层根因，已剥离（`wash::strip_ncx_doctype`），重新投递一份新副本
（`07b33e06`，替换设备上那份旧的），**这一轮修复是否真的解决还没等到用户确认就转向了"超限书籍
优化会不会 OOM"这条更紧急的反馈**——真机拿用户自己的 552MB《镖人》全集实测坐实内存版优化确实
会把设备逼近系统级 OOM，已经改成两阶段流式架构+真机复测通过，见正文。**当前真机待验证清单**：
①《疯探》目录入口这轮（dtb:uid+DOCTYPE 都修完）是否真的出现；② 552MB《镖人》优化后的产物拿去
真机原生书库打开、翻页/渲染是否正常（目前只验证了优化过程本身不再 OOM，没有验证这本书优化完
投递到原生阅读器后的实际阅读体验——173 章、785MB 的产物比大多数测试样本大得多，是全新的观感
验证盲区）。

**未闭环**：网页 UI i18n（§03ae 架子 + §03an 正文全量补完，437 个 key，数据链路+内容对照已真机验证——`curl` 交叉核对真机 served 的语言包与部署的 app.js 里全部 416 处 `T()` 引用零缺失）——剩浏览器里实际点开语言切换器、人眼确认文案切成英文后的排版/换行/组件对齐效果这一步没做，这条缺口从 §03ae 延续到现在，覆盖面已经从"只剩顶层导航几个词"变成"正文也翻完了，只是没用真实浏览器看过"。网页 UI 人性化/触屏可用性一批小修（§03af，纯前端改动，同样没有浏览器截图核对实际渲染效果——禁用按钮说明文字是否真的显示、徽章点击 `alert` 是否真的弹出、`.btn-bad` 配色是否符合预期，这些都还没人眼确认过，只确认过新标记已 served）。**图片密集网文渲染大片留白**（§03aq，2026-09-10）——真机 A/B 验证过 `wash_css` 缺 `figure`/`figcaption` 边距归零不是成因（已经修了这个缺口，但对留白零帮助）；真正成因是分页引擎对放不下的图片块整体挪页、当前页剩余空间不回填，排查过没找到能从 EPUB/CSS 层面调的杠杆，**留白问题本身仍未解决**，比 i18n 那条视觉确认缺口更实质——不是"还没人眼看过"，是"看过了，问题还在，暂时没有已知修法"。

镖人/阿拉蕾①的"漫画体积超原生上传上限、只出 CBZ 不分卷"分支已于 2026-09-09 真机复验通过（§03ad，两本各自 `shelf push --wait` 成功、母版库确认只落 `.gray.cbz` 无伴生 PDF、无分卷痕迹）。2026-09-06 当天测试书已全部清掉（五本用户手删、最后两本由回收站代理软删）；`push --wait` "睡着→点亮→续传"的时序在日常使用里顺手验过。

**OTA 后固定五步**（§03v）：`xovi/rebuild_hashtable` → `xovi/start` → `SHELF_NO_BUILD=1 sh deploy.sh 10.11.99.1` → `ssh root@10.11.99.1 sh -s < packaging/chrony-cn.sh` → `packaging/wifi-watch/install.sh`。升完顺手 `shelf doctor --render` 看 CSS 引擎有没有变。/home 里的（母版库、KOReader、WiFi 钩子与 `powersave 2`、休眠屏 conf 键、qmd 文件）不用动。

**已闭环（真机）**：§03ac 可插拔机制接住独立仓库线（2026-09-06 晚，WiFi 部署书架+笔记线共八服务 active）· §03f 首轮五服务 · §03g/§03h 字体分开装/子目录/HTTPS · §03j 登录/CA/mDNS · §03k 字体两 bug · §03l 传书卡＝云同步 · §03m/§03n/§03o 网页改版/细节/管理台 · §03p 质量一轮 · §03q 优化做精 + 首行缩进 v10 · §03r 母版库 Phase A/B/C + 财新重排 · §03s 质量二轮 + 格式三档 · §03t 漫画通道（host 真书探针 → CBZ；漫画不投原生）+ 分卷静默失效修 + 投原生体积门 · §03v 固件 3.28 升级 + 3.28 字体菜单 qmd（首版整份不应用：qmldiff 解析不了 `({})` 与裸 `if (` handler，改 `[]`+`{ }` 后 `appended=4 count=8`，判官＝本机 asivery/qmldiff CLI）+ appload 3.28 复活（PR #59 qmd 等长回填进 .so，系统增强白皮书 §12.1；用户点侧栏 KOReader 正常起）· §03w 原生休眠屏 `SleepScreenPath` + WiFi regdomain 真凶 + wifi-watch + chrony 国内 NTP 脚本 · §03x 退役 bind-mount 壁纸整套（`xochitl_conf` + `native.rs`）· §03y xochitl CSS 引擎七条实测规则 + 英文首段顶格 v6 + 中文 br 书段落化 · §03z 事件推送（网页 + CLI 用户确认）· §03aa 阅读线六项（渲染自检 / `doctor --render` 用户 CLI PASS / `push --wait` / TXT 切章 / Phase E ④ Gulliver 两器脚注观感 / 漫画 16 灰默认开）· §03ab 代码体检四支 · PDF 结构化重排 v4（《财新》33 期：署名/图注/链接分类、节题 h3、标题分档，非句末段 15.1%→2.7%，`test_reflow.py` 锁纯函数）· §03ad 漫画跨页拆分+白边裁切放大（火影忍者卷1真机通）+ 小体积漫画可选投原生（火影忍者卷1真机通投原生 · 阿拉蕾①/镖人真机通超限只出 CBZ 不分卷，2026-09-09）· §03aj「管理」拆二级 tab + 系统增强开关（`hlSnapCjk`/battop 真开关：部署健康检查+`PUT /api/enhance/qol` 保留其余键+battop 未装时干净报错，用户确认命令块字体清楚了，2026-09-09）· §03ak「实验室」二级 tab + 独立「电池刺客」标签页 + 「导入 md 文档」文件上传改造（curl 功能测试：四开关读写+全量防覆盖+`import-md` 真实生成 `.rmdoc`；用户真机写字确认 `[hw-stroke:f4c8d0]` journal 日志跟 `hwStrokeEnabled` 开关状态一致；用户测试顺手逮到 battop"未装"提示路径过时〔仍写着搬家前的 `misc/battery-audit/`〕，修复重部署确认，2026-09-10）· §03al 首层标签重排（传书/笔记/其他/管理）+「入库」拆三卡 + battop 真机实装闭环（`install/start/stop` 循环+`lastSampleAt` 刷新确认）+ 电池审计时间窗数据表接入（`GET /api/enhance/battop/summary` 真机返回真实聚合数据）+ 电池刺客独立顶层标签页撤销回「实验室」卡片（2026-09-10）· §03am 电池刺客再拆分为「管理→电池刺客」二级 tab（耗电情况/唤醒源两个三级 tab，耗电情况补齐按进程）+ 总标题「书架」改「秘密花园」（含图标/登录页/改密码页/服务标签同步）+ 网关端口 8778 改绑标准 443（真机确认旧端口连接拒绝、新端口 curl 正常返回，2026-09-10）· §03ao `shelf push --no-calibre`（EPUB 跳过 Calibre 纯跑 `epub-optimize`，8 个新单测 + 非 mock 真调用验证）· §03ap 抓网文「同步优化」复选框（真机 curl 分带/不带 `optimize` 各抓一次同一篇文章，`level` full/core 落地正确）。**§03aq 不放进这个列表**——那条排查修的是一个真实存在的代码缺口（`figure`/`figcaption` 边距归零），但没有解决它本来要排查的问题（网文留白），后者仍在上面「未闭环」段落里。

**Phase E 实录（②③④，2026-09-06 全部闭环，用《Tell Me Your Dreams》AZW3 与《Gulliver's Travels》推进）**：
- 洗书发现两处实现缺口并修（bookconv `wash.rs`）：① 书自带类规则 `.calibre_ {text-indent:2em}` 未统一——xochitl 不认类规则走我们的 `p{1.2em}`，KOReader 认且类规则特异性更高走 2em，**两器同字节不同缩进**；现在书 css / 内联 style 里非零 `text-indent` 一律改写成本书缩进（0 与负值保留）。② "标题后首段不缩进"只写在注释里从未实现。
- **③ 用户对照通过**：两器翻到同一页首行缩进一致。**量化**（xochitl 渲染缓存 `<uuid>.pdf` 用 pymupdf 量首行 x 偏移）：英文书 KingHwa 12.1pt 下缩进 14.2pt = **1.17em**（=我们的 `p{1.2em}`，em 制、随字号缩放、不随字体家族变）。同法量《人骨拼圖》：2017 年旧 EPUB 直传、没洗过，xochitl 渲染下首行零缩进（→ `cjk_paragraphize`，§03y 末段）。
- **②**：用户追问"英文习惯不是首段不缩进吗"→ 泛化判定（前一块是 `</hN>` / 标题样段落 / 段末 ≥2 个 `<br>` 或空段·`* * *` 分隔 / 章首第一段）；内联 `style="text-indent:0"` 在 KOReader 顶格、xochitl 不顶格 → **xochitl 不认内联 `style=""` 属性**（§03y 硬规则）；改为换元素 `<div class="cj-flush">` + 外链 `.cj-flush{text-indent:0.01em}`（`0` 被当没设）→ v6 真机：章首/场景切换 0pt、续段 14.2pt，KOReader 同。用户观察"中文换字体缩进跟着变、英文不变"：em 制只随字号；随家族变的是烘进正文的全角空格——已剥。
- **④**：Sheldon 无脚注 → 用户定拉公版书 Standard Ebooks《Gulliver's Travels》（7 处 `epub:type="noteref"` 尾注），`shelf push` 链洗后（Inline：7 处内联成 cj-note；NCX 52 条）投原生 404 页（自检 ok）+ 加入 KOReader，**用户目视两器脚注观感正常**：xochitl 内联注文不碍眼、KOReader 没有弹窗被替代的落差；Inline 一条产物两器通用成立，不为 KOReader 另跑 Anchor。

**已放弃**：**微读线整条**（§03u，2026-09-05：先是内嵌浏览器 spike 未推进，后内容源方案评估后用户砍掉）；设备端 AZW3/MOBI/FB2 → EPUB 转换（§03s，杂格式走电脑 Calibre，`bookconv::convert` 本体留给 reading 线）；KOReader 高亮/生词回流 PKM（2026-09-06 用户定留给笔记线，2026-09-16 已在笔记线落地——本条线只保留 `koreader-serve` 新增的两个只读原始数据端点 `GET /annotations`/`GET /vocabulary`，条目库的创建/合并逻辑不在本仓库，见笔记线白皮书 §03al）与"稍后读"URL 队列（网文少，不做）。
