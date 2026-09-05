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

**架构**：网关（`0.0.0.0:8778`，HTTPS 私有 CA + 登录页密码 / CLI Basic + mDNS `shelf.local`）+ 四个 loopback 领域服务（book 8790 / koreader 8791 / font 8792 / wallpaper 8793）+ 运行时注册表驱动 tab；`weread-serve` 8794 预留。设备固件 3.27.3.0，KOReader v2026.07.1。

**读书线 = 三层 · 三动作正交**（§03r 定，§03s 收口）：内容源（网页上传 / 抓网文 / host `shelf push` / scp inbox / 微读〔Phase D 待接〕）→ **母版库** `~/.local/state/shelf/books/staging/`（原样入库，永久保留，不淘汰）→ 落库（人选：投 xochitl 只收 EPUB/PDF；加入 KOReader 收任意入库格式）。「优化」是母版库里对 EPUB 的独立动作（档位 auto / keep-spacing / plain，产物标记 full / core / old）；落库＝纯复制母版字节。**漫画不投原生**：AZW3/EPUB 漫画由 host `shelf push` 转 CBZ 入库，只加入 KOReader。**所有书只落母版库，没有任何直投读器的路径**。落库记录 sidecar `.<书>.delivered`。

**代码落点**：book-serve `staging.rs`（领域）/ `spool.rs`（inbox 队列）/ `api.rs`（纯适配）；koreader-serve 只做"从母版库 adopt"+字体/词典/配置同步（不依赖 bookconv）；`shelf_core::formats` 是格式白名单单一事实源（三档：原生 epub/pdf · 电脑可转 azw3/mobi/azw/prc/fb2 · 仅 KOReader 其余 11 个），网页 `ui::page()` 注入；`shelf_core::asset` 是所有上传口的模板（母版库暂存在 spool `.work/` 同分区 rename）。host CLI：`push.plan()` 三路 raw / comic（→CBZ）/ wash；`comic.py` 漫画探针；bookconv CLI `epub-optimize`（与设备同一函数）。

**已删（别再找）**：漫画 CBZ→PDF 投原生整条（`cbz2pdf` bin、`POST /staging/to-pdf`、`push --mono`，§03t 末）；book-serve `POST /?target=native|annot` 直投路与 `target.rs`/`pipeline.rs`（Strategy/Pipeline）、`/targets`、`done/` LRU；koreader-serve 直传 `POST /books` 与 `optimizeEpub`；网页读器页的传书区与 KOReader 书库浏览；host `push -t/--direct/--quality`、config `default_target/quality`；`BookConfig.optimizeDirectEpub/comicMono`。

**网页 tab**：传书（入库｜母版库，固定第一）· xochitl（原生字体，由 font-serve 注册）· KOReader（字体｜词典）· 壁纸 · 管理（固定）。

**未闭环**：
1. **Phase E ②③④**：英文书拉丁缩进（1.2em、标题后首段不缩进）观感；同一母版落 xochitl + KOReader 并排对照；KOReader 里内联脚注〔…〕能否接受（若不能，落库时对 KOReader 另跑 Anchor 是唯一备选，但会打破"两器同字节"）。
2. **Phase D 微读内容源**：复用 `reading/device-rs` 下书栈（扫码登录 / 抓章 / 组 EPUB），落母版库；形态待定（立 `weread-serve` 8794 或 book-serve 代理）。
3. 母版库里遗留的漫画 PDF（用户推的《镖人》297MB / 《火影》188MB 整本，漫画不投原生后无用）由用户在网页删；KOReader 里旧的 282MB《镖人.epub》同。
4. 真重启后 `shelf.target` / 壁纸 bind 自起（重启会丢 xovi，需手动 `xovi/start`，用户暂不装 reenable）；拔线真 suspend 下钩子 bind + 唤醒轮换只触发一次。
5. 3.28 固件机验证 `font-menu-dynamic.qmd`（3.28 锚点版含 elide 补丁未上机）。
6. PDF 结构化重排小瑕疵：署名"文｜某某"混进目录；"句中断开 12%"含图注/列表未细分。

**已闭环（真机）**：§03f 首轮五服务 · §03g/§03h 字体分开装/子目录/HTTPS · §03j 登录/CA/mDNS · §03k 字体两 bug · §03l 传书卡＝云同步 · §03m/§03n/§03o 网页改版/细节/管理台 · §03p 质量一轮 · §03q 优化做精 + 首行缩进 v10 · §03r 母版库 Phase A/B/C + 财新重排 · §03s 质量二轮 + 格式三档 · §03t 漫画通道（host 真书探针 → CBZ；漫画不投原生）+ 分卷静默失效修。

**已放弃**：P5 微读网页版内嵌浏览器 spike（§03r 决策 3：微读定位为内容源）；设备端 AZW3/MOBI/FB2 → EPUB 转换（§03s，杂格式走电脑 Calibre，`bookconv::convert` 本体留给 reading 线）。
