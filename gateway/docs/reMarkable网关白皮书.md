# reMarkable 设备端网关（gateway）白皮书

> 记"怎么决定、为什么正名搬顶层、踩了什么坑"。读现状先看 §00b；待办看 §05；踩坑看 §04。
> **反向代理/注册表/事件汇聚这套架构本身的决策过程**是在还叫 `shelf-gateway`、还挂在
> `shelf/services/` 下时定的，记在 `shelf/docs/reMarkable书架白皮书.md` §01（"三种拆法"）+
> §03z（事件推送）+ §03aa/§03ah（渲染自检/`proxy` 模块注释修正）等历史节，本文不重复搬运，
> 只记"正名搬顶层"这件事本身，以及正名之后独立存在时的现状、职责边界、维护纪律。

## 00｜定位与原则

`gateway` 是 `shelf/`、`notes/`、`enhance/` 三条项目线共用的**唯一对外入口**：一个 HTTPS 反向
代理 + 单页 UI 托管 + 服务注册表 + 事件汇聚器。它自己不做任何"书"/"笔记"/"字体"/"壁纸"这类
业务，只做三件事：① 托管单页前端；② `/api/services` 列注册表；③ `/api/<service>/*` 反向代理
到对应领域服务的 loopback 端口。

**2026-09-11 正名搬顶层**：原名 `shelf-gateway`，位置在 `shelf/services/shelf-gateway`。这个
名字和位置在用户判断"壁纸/字体该不该挪进 enhance"的过程中被指出不再准确——它托管的网页
早就不只是"书架"了：`notes/` 的「笔记」tab、`enhance/` 的系统增强/实验室/电池刺客开关都挂
在这一个网关下面，网页总标题也在 2026-09-10（`shelf/docs/reMarkable书架白皮书.md` §03am）就
已经从"书架"改成了"秘密花园"。继续叫 `shelf-gateway`、放在 `shelf/services/` 里，跟它三线
共用入口的实际定位不符。用户拍板正名：crate/二进制改名 `gateway`，目录搬到仓库顶层，
`shelf/notes/enhance` 三条业务线通过路径依赖/协同构建脚本接入它，而不是它属于其中任何一条线。

原则延续自 shelf-gateway 时代：
1. **Facade（网关代理）**：`manage::MODULES` 是 URL 段 ↔ 服务名的单一事实源，管理台三态/
   代理路由/CLI `status` 都从它派生，不允许各处各写一份映射。
2. **薄客户端消费 `enhance/` 产出**：`src/enhance/battop.rs` 只是 `systemctl start/stop` +
   读 `summary.json`，不关心 `battop` 源码在仓库哪个位置——这条原则这次继续适用，
   `wallpaper-serve`/`font-serve` 挪进 `enhance/` 后，`manage.rs` 里的注册表项完全不用改，
   代理机制只认服务名字符串，不关心源码物理位置。
3. **不做业务、只做转发+托管**：任何具体领域逻辑都不该长在这里，长在这里的唯一例外是
   `src/enhance/{mod,qol,battop}.rs`——刻意不升成独立 service，因为这几个是"网页开关薄薄
   一层"，没有独立进程边界的必要（决策记在 `shelf/docs/reMarkable书架白皮书.md` §03aj）。

## 00b｜现状总览（2026-09-11）

**职责**：① 托管单页 UI（`ui/index.html`/`style.css`/`app.js`/`auth.css` 真文件，编译期
`include_str!` 拼进二进制；`ui/locales/{zh-CN,en-US}.json` 是 i18n 语言包，`GET /ui/locales/
{lang}` 分发）；② `/api/services` 列服务注册表（网页据此隐藏未安装服务的 tab）；③ `/api/
<service>/*` 反向代理到该服务的 loopback 端口，剥掉 `<service>` 段、body 流式透传。服务
缺席 → 404「未安装」。

**对外**：部署固定 `0.0.0.0:443`（2026-09-10 前是 `:8778`，见书架白皮书 §03am）；**HTTPS**
私有 CA 签发（首启生成，`/ca.crt` 可下载装信任，叶证书 800 天自动续签）；**登录页密码**
（无用户名，首次默认 `shelf`，登录后强制改，CLI 走 Basic）；**mDNS `shelf.local`** 伪域名
（iOS/macOS/Windows/Linux 直接可用，安卓不解析 `.local`）。

**子命令**：`serve [--bind]` · `passwd <新密码>` · `reset-password`（回默认并强制改）·
`regen-tls`（重签叶证书）。

**代码结构**：

| 文件 | 职责 |
|---|---|
| `main.rs` | 入口 + 子命令分发 + `ServiceSpec`（`name: "gateway"`，`label: "秘密花园"`） |
| `auth.rs` | 密码策略 + Basic 认证 |
| `proxy.rs` | `/api/<seg>/*` 反向代理（`forward` 只转发 `Content-Type`/`Content-Disposition` 两个响应头，其余一律丢弃——不给后端服务开口子夹带 `Set-Cookie` 这类敏感头，历史踩坑见书架白皮书 §03ah） |
| `manage.rs` | `MODULES` 单一事实源（URL 段 ↔ 服务名映射） + 管理台三态 |
| `events.rs` | `Hub`：汇聚各服务 `GET /events`，网关 `GET /api/events` 对网页做 SSE 汇聚 |
| `enhance/mod.rs`、`enhance/qol.rs`、`enhance/battop.rs` | 系统增强开关的网页控制面（`hlSnapCjk`/`hwStrokeEnabled`/`notesImportMdEnabled`/battop 启停），薄客户端，见 §00 原则 2 |
| `ui/` | 单页前端（437 个 i18n key，2026-09-10 起正文全量中英文切换，见书架白皮书 §03an） |

**依赖**：只依赖顶层 `../rmsvc-core`（共享基座）——不依赖 `shelf/crates/bookconv`、不依赖
`notes/` 任何 crate。**不在任何 workspace 内**，独立顶层 Cargo 项目。

**测试**：16 个单测，`cargo test --manifest-path gateway/Cargo.toml` 独立跑。

**谁在托管谁**：本目录只放网关自己的代码；`shelf/`、`notes/`、`enhance/` 三条业务线通过
`manage::MODULES` 里的几行映射"挂上来"，各自的架构决策/真机记录都在各自仓库线的白皮书，
本文不代管、不重复记。

## 01｜架构决策：为什么正名，而不是继续留在 shelf 底下

跟 `rmsvc-core` 正名同一次连锁判断（详见 `rmsvc-core/docs/reMarkable设备端Web服务基座白皮书.md`
§01），对网关这一半，关键判断点是：**"谁托管网页 UI"不等于"UI 里的功能归哪条业务线"**。
`enhance/` 的 `battop`/`hl-snap` 开关早就挂在这个网关的网页里，但从没有人因此觉得 `battop`
的源码"应该"待在 `shelf/` 底下——网关本来就是三线共用的展示层，跟它自己的源码归属是两件
事。这次只是把"网关本身"也按同样的逻辑对待：它服务三条线，就不该继续算"shelf 的一个服务"。

**crate/二进制名怎么选**：候选过保留 `shelf-gateway`（只挪目录不改名）、`hub`、`panel`，
用户在"crate 名要变，包括 shelf-gateway 是否要抽离并改名"这句话里已经把"改名"放进了范围，
最终选 `gateway`——简单直接，不像 `hub`/`panel` 那样引入新的隐喻需要额外解释，且跟仓库里
`*-serve` 这套领域服务命名（`book-serve`/`font-serve`…）自然区分开（网关不是一个"领域"，
是转发层）。

**没有改的东西**（详见 §05 命名遗留）：`SPEC.name`/日志前缀/CLI 子命令帮助文本这层全部
改成了 `gateway`，但 XDG 运行时命名空间（`$XDG_RUNTIME_DIR/shelf/services/`）、`shelf.target`、
默认密码字面量 `"shelf"`、mDNS 域名 `shelf.local` **没有动**——这几处牵连已部署设备的真实
路径/配置，需要专门的迁移方案，这次范围明确排除。

## 02｜systemd

单元文件 `systemd/gateway.service`（原 `shelf/systemd/shelf-gateway.service`，2026-09-11 随
目录搬迁+改名）：`PartOf=shelf.target`、`After=home.mount network-online.target`、
`ExecStartPre=-/bin/sh …/lo-alias.sh`（脚本源 `enhance/lo-alias/`，同批去掉 `cangjie-` 前缀）、
`ExecStartPre=-/usr/bin/fc-cache`、`ExecStart=…/gateway serve --bind 0.0.0.0:443`。
`CPUWeight=20`/`MemoryMax=192M`/`Nice=5` 软限（交互式重活需要突发，只降权不硬顶）。

`shelf.target` 这个 systemd 目标名字**没有跟着改**——它是 `shelf/`、`notes/`、`gateway/`、
`enhance/wallpaper-serve`/`font-serve` 全部服务共用的开机分组目标，改名同样牵连已部署设备，
见 §05。

## 03｜构建与部署：没有自己的 build.sh/deploy.sh

`gateway/` 没有独立的安装/部署脚本——历史上它就是跟着书架整包一起装的，这次正名只挪了
源码位置，没有另起一套独立的安装流程。实际构建/打包由 `shelf/build.sh`（顺手 `cd ../gateway
&& cargo build`）和 `shelf/deploy.sh`（把 `../gateway/target/.../gateway` 二进制 + `../gateway/
systemd/gateway.service` 打进同一个部署包）代管，跟它代管 `../notes`（真正独立的仓库线）
是同一套跨目录协同模式——先例已经验证过能跑通，这次只是多接一个消费方。

`gateway/.cargo/config.toml`（CC/AR 覆盖）是独立成顶层项目后必须补的东西，详见
`rmsvc-core` 白皮书 §04 的踩坑记录（同一个坑，网关和 `rmsvc-core` 的两个新消费方
`wallpaper-serve`/`font-serve` 都踩了一遍）。

## 04｜踩坑

- **`ServiceSpec.name` 改名不影响任何反向依赖**：搬迁前担心过改 `SPEC.name`（`"shelf-
  gateway"` → `"gateway"`）会不会连累别的服务按名字查找网关，排查后确认没有任何消费方
  按这个字符串反查网关（网关只是注册表的读者+代理者，不是被别人查找的对象），改起来
  比预想安全。
- **测试假数据是孤儿，容易在改名时被漏掉**：`shelf/host/tests/test_cli.py` 里 `FakeGateway`
  的假 `/api/services` 响应硬编码了 `{"name": "shelf-gateway", "label": "书架"}`——这两个
  值早就跟真实网关（`"gateway"`/`"秘密花园"`）不一致了（"书架"这个标题在 §03am 就改成
  "秘密花园"了），因为纯字符串常量、host CLI 从不真的按 `name` 字段做逻辑判断，一直没
  报错也没人注意。这次改名顺手核对了一遍才发现，修正为当前真实值。**教训**：纯装饰性
  的测试假数据不会因为逻辑测试失败而暴露自己过期，改一处现实值时值得顺手 grep 一下
  测试 fixture 里有没有抄了旧值。
- 其余交叉编译相关的坑（`.cargo/config.toml`、`version.workspace` 字段）跟 `rmsvc-core`
  是同一批，记在那边白皮书 §04，不重复记。
- **「管理」页 8 个服务名漏了 i18n**（2026-09-11 发现）：`manage.rs` 的 `MODULES` 单一事实源
  里 `label` 字段是硬编码中文字面量（`"母版库 / 落原生"`/`"xochitl 字体"`/`"笔记·矿（条目库）"`
  等），`ui/app.js` 渲染「Shelf features」列表时直接拿 `m.label` 塞进 DOM，没走 `T()` 那套
  i18n 查找——切到英文界面后这 8 个服务名还是中文，跟周围全英文的按钮/说明文字混在一起。
  发现经过：不是代码审计发现的，是把 `gateway` 真的编出来在本机跑起来、用无头浏览器登录后
  切英文实际看界面才看出来（配 Reddit 宣传截图时顺手做的）——纯靠读代码/grep 字符串没查出来，
  因为 `label` 字段类型是 `&str`，静态看不出它到底有没有经过 i18n 层。**教训**：i18n 覆盖率
  审计光扫 `T(...)` 调用点不够，还得扫有没有"后端直接吐字符串、前端原样渲染"这种绕过 i18n
  管线的路径，最好真的切一次语言、把每个页面都看一遍。修法：`app.js` 改成优先查
  `T('manage.modules.label.'+m.seg)`，`zh-CN.json`/`en-US.json` 各补 8 个 key，`m.label`
  降级为查不到 key 时的兜底。`cargo test`（含 locale 两文件 key 集合一致性测试）+
  `node --check` 全过。
- **`/api/foundation` 新增 WeRead 只读探测**（2026-09-13）：用户拿到一份外部第三方发行包
  `remarkable-weread-v1.0.0-universal-*`（跟本项目早年自建、2026-09-05 已砍的旧微读双向同步
  管线完全无关——是别人做的独立 app，自带 `install.sh`，纯 SSH 直装到设备
  `~/.local/opt/remarkable-weread/`，不经过 `shelf-install`/`packaging/`），装完要求"以同样
  的形式显示在 KOReader 下方"。`koreader` 本来就同时出现在两处：`MODULES`（`koreader-serve`，
  本项目自己的服务，管 adopt 书进 KOReader）和 `foundation()`（对 KOReader 本体装没装的只读
  探测，跟本项目服务无关）。WeRead 没有 adopt 需求（微信扫码云同步，不需要书架传书），只对应
  第二处——`Paths` 加 `weread_root()`（默认 `~/.local/opt/remarkable-weread`，可用
  `SHELF_WEREAD_ROOT` 覆盖，跟 `koreader_root`/`SHELF_KOREADER_ROOT` 同构），`foundation()`
  探测标记文件 `bin/start-remarkable-weread.sh`（装机脚本给出的确定性 SSH 启动入口），
  `app.js` 基石 kv 列表里紧跟 `KOReader` 那对 `<b>/<span>` 后面加一对 `WeRead`——kv 是
  `grid-template-columns:auto 1fr` 两列网格，紧跟着写就是下一行，天然渲染在 KOReader 正下方，
  不需要额外布局代码。真机验证：真的把这份包 SSH 装到设备（`sh remarkable-weread/install.sh`，
  日志确认 `RemarkableWeRead 1.0.0 installed for move`+3.28 走 SSH 启动器不装 AppLoad 入口）、
  交叉编译新 `gateway` 二进制、备份旧二进制后原地替换、`systemctl restart gateway.service`
  确认 `active`/`NRestarts=0`、HTTPS 401（需登录，非崩溃）；`foundation()` 返回值里
  `weread` 字段的真实布尔值没有登录态截图肉眼确认（没有网页密码），但探测路径
  `bin/start-remarkable-weread.sh` 已经用 `ls`/`cat` 直接核对过在设备上确实存在，逻辑由
  `foundation_probes_weread_alongside_koreader` 单测覆盖。
  **追记（2026-09-13，同日）**：装完用户反馈"设备端 KOReader 下没有入口"——上面说的"不装
  AppLoad 入口"是这份第三方发行包自己的保守判断（作者大概率不知道本项目已经用 PR#59 补丁把
  AppLoad 在 3.28 上救活了，见），它其实**随包带了**
  一份现成的 AppLoad 应用包（`~/.local/opt/remarkable-weread/appload/{external.manifest.json,
  appload-launch.sh,icon.png}`，`appload-launch.sh` 注释原文就写"AppLoad 和 3.28 及以后的
  无图标模式共用同一个持久启动入口"），只是装机脚本按 OS 版本判断跳过了"拷进 AppLoad 目录"这
  一步。既然这台设备的 AppLoad 本来就是活的（`~/xovi/exthome/appload/koreader/` 现役），手动把
  这份随包目录原样复制（不是软链，没把握 AppLoad 扫描器对符号链接目录的 `d_type` 处理方式，
  用真文件更稳）到 `~/xovi/exthome/appload/weread/`，`xovi/start` 重扫后 xochitl 重启完成、
  `xovi.so` 重新出现在其 `/proc/<pid>/maps`、`xochitl.service`/`gateway.service` 都
  `active`——图标应该已经在设备侧边栏 KOReader 下方出现，但侧边栏本身长什么样只有肉眼能看，
  这条最终还是要用户自己看设备确认。
  **追记二（2026-09-13，同日，真正根因）**：用户反馈"没出现"——上一条追记的判断是错的。
  往 `~/xovi/exthome/appload/weread/` 补目录只解决了"AppLoad 自己认不认识这个外部 app"，不
  解决"用户怎么点到它"：Sidebar 上原生真的有一个"AppLoad"二级菜单入口，AppLoad 扫到的新
  app 本该出现在那个菜单里——但 `koreader-sidebar-entry.qmd`（本项目 2026-09-02 就写好、一直
  部署到今天的 QMLDiff 补丁，代码不在这个 git 仓库、在 `oldbak/xovi-extensions/reading-qol/`，
  仍是这台设备的现役 payload）**运行时把原生"AppLoad"那一级菜单项直接隐藏了**（`c.visible =
  false`，为了把 KOReader 从二级菜单提到侧边栏一级直达）——这条路当时是死的，`~/xovi/exthome/
  appload/weread/` 目录再对也没有入口能点进去。真正需要的是照同一套机制**再给 WeRead 也开一条
  一级直达**：同一个 QMLDiff INSERT 块里紧跟 `cjKoreaderEntry` 后面加一个同构
  `ArkControls.SidebarItem`（`id: cjWereadEntry`，`iconSource: qrc:/cangjie/icons/weread`，
  `onClicked` 调 `CJAppLoad.AppLoadLauncher.launchApplication("external::weread", …)`），图标
  换成 "WR" 字标（`assets/cangjie-icons.qrc` 从单图标扩成两图标，同一套 alpha 蒙版 + `rcc`
  编译 + qt-resource-rebuilder `.rcc` 通道，见该目录 `README.md`）。**真机验证**：用真实
  `xochitl` 3.28.0.172 二进制（`md5 952f1e28f…`，跟这条 qmd 头注记录的 `qml_00dcd9d7` blob
  一致，同一个固件版本）离线跑通整条 `extract_qml→qmldiff apply-diffs` 管线，零解析错误、
  产物结构正确（KOReader 项后紧跟 WeRead 项）；再备份设备上原 `.qmd`/`.rcc`（`.bak.pre-weread`
  后缀）、部署新版、`systemctl restart xochitl` 后 `NRestarts=0`/`active`，
  `journalctl` 里 `CJ-SIDEBAR[8]: KOReader`→`CJ-SIDEBAR[9]: WeRead` 顺序确认无误——这是
  真机日志坐实的，不是猜的。**教训**：这条线索本该在第一次回复"应该已经出现"之前就想到——
  `koreader-sidebar-entry.qmd` 的头注原文明确写着"隐藏 AppLoad 菜单项"，只检查了"appload 认
  不认识这个 app"这一层就下结论，没有反向确认"隐藏了原生入口之后，新装的第三方 app 到底还有
  没有别的路能进沙盒外部启动"。详见 `oldbak/xovi-extensions/reading-qol/README.md`
  「koreader-sidebar-entry」条目 2026-09-13 追加说明。
  **追记三（同日，两个后续问题）**：①用户问"退出 WeRead 每次都要 `xovi/start` 吗"——查真机
  日志证实**不需要**：WeRead 自带的 `return-to-appload.sh` 退出钩子只是
  `systemctl stop/start xochitl.service`（普通服务重启，不是设备断电重启），`/etc` 里
  `xovi/start` 早先写好的 `00-xovi.conf` 这份 systemd drop-in 留在 tmpfs 里不受影响，
  任何一次 xochitl 服务重启（不管谁触发的）都会重新吃到它——真机 `journalctl` 看到用户这次
  登录/退出 WeRead 之后新 PID 照样自动 `Loading file koreader-sidebar-entry.qmd`+
  `CJ-SIDEBAR[9]: WeRead`，`xovi.so` 也还在 maps 里。只有设备真的断电重启（tmpfs 清空）才
  需要手动 `xovi/start`，这是老规矩，跟装 WeRead 无关。②用户问"退出 WeRead 有个重新加载的
  读条，KOReader 没有，是 WeRead 自己的机制吗"——是的，两者架构不同：KOReader 的
  `external.manifest.json` 是 `"qtfb": true`，走 appload 的 qtfb 桥显示，**xochitl 全程不停**，
  切换零感知；WeRead 的清单是 `"qtfb": false` + `"disablesWindowedMode": true`，走独占物理
  framebuffer 的"接管"模式（systemd 单元 `ExecStartPre` 直接 `mask`+`stop xochitl.service`），
  退出时 `return-to-appload.sh` 要重新 `start xochitl.service`——那条读条就是 xochitl 真的被
  完整重启一遍的正常现象，不是故障。③用户反馈图标"WR"两个字母看着比 KOReader 的"Ko"小一号、
  W 看着没有大写——测量两张 alpha 蒙版图的字形包围盒实锤坐实了"看着小"是真的（Ko 高 87px、
  WR 高仅 69px，同一张 192px 画布）；"W 没大写"是错觉，实际是大小写都对（W/R 都是大写）只是
  显得局促。改成跟"Ko"同样"首字母大写+第二个字母小写"的 `We`（呼应 KOReader→Ko 的截取规则，
  WeRead→We），重新量字号让高度对齐 Ko（前两字母 `We`，Noto Sans Bold 110px，包围盒
  165×79，Ko 是 139×87，视觉重量基本打平，比硬凑 WR 两个大写字母挤边框更协调）。改的只是
  `assets/we-icon.png`（原 `wr-icon.png` 删除）+ `cangjie-icons.qrc` 里那一行 `alias`，
  QML 结构完全没动，不需要重新跑 qmldiff 离线管线，只重建 `.rcc`、备份旧的
  （`.bak.pre-we-resize`）、部署、`systemctl restart xochitl` 确认 `active`/`NRestarts=0`。
  **追记四（同日，收尾）**：用户直接看设备肉眼确认"侧边栏有 Ko 和 We"，两个入口图标显示
  正常、大小视觉重量也对齐——这条 WeRead 线到此闭环。中途试过用设备内置 `/usr/bin/screenshot`
  （给 xochitl 发 `SIGUSR2` 触发它自己截图）想留一张真机截图当证据，结果这个固件版本的
  xochitl 根本没接这个信号的处理逻辑，发信号的默认效果是终止进程——连续两次把 xochitl 干崩
  （`journalctl`：`Main process exited, code=killed, status=12/USR2`），systemd
  `Restart=on-failure` 都自动拉回来了、没造成实质损坏，但 `NRestarts` 一度顶到 3（该服务
  `StartLimitBurst=4`/10 分钟窗口，见 `docs/INSTALL.md`「风险项预警」③），没敢再试第三次。
  最终还是靠用户肉眼确认收尾，没有留下截图——工具拿不到证据时老实说拿不到，不假装/不硬凑。
  这个"内置 screenshot 脚本在这固件上会崩 xochitl"的坑记进了本机项目记忆
  `screenshot-sigusr2-crashes-xochitl`，以后别再走这条路。
  **追记五（同日，全链路真机通）**：用户报告"扫码登录后使用正常，退出后回到系统正常"，
  查 `journalctl -u remarkable-weread-app.service` 完整时间线核实：启动→扫码登录成功→
  书架加载（18 本书）→进阅读器、下载并自动缓存全部章节→切换两次阅读字体+翻章节→
  `AppController::exitApp: showing notice, will restore xochitl`→e-ink 面板正常关闭→
  xochitl 干净重启（非崩溃触发）→`remarkable-weread-app.service: Deactivated
  successfully`，全程零错误日志。至此从"Sidebar 点击启动"到"扫码登录"到"实际读书"到
  "干净退出回系统"整条链路端到端真机验证通过，不再只是"图标显示对了"这一层。
- **母版库网页上传口补齐 CLI 同款"跳过已存在文件"保护（2026-09-13）**：Reddit 用户对
  `shelf push` CLI 提的重跑重复问题修完后，用户追问网页 `uploader()`（`gateway/ui/app.js`）
  是不是也有同一个坑——有，只是触发方式不同（同一页面会话内重传是安全的，刷新页面重新拖同一批
  文件/手滑拖两次同一文件这两条路会撞上）。修法、真机+本机隔离浏览器端到端验证细节记在
  `shelf/docs/reMarkable书架白皮书.md` §04（母版库/staging 概念本身归 shelf 线，这条只在这
  记一笔指路，不重复整段）——包含改动点（`uploader()` 新增 `dedupeApi` 参数，只有母版库这个
  调用点传）、新增 i18n key、以及验证过程里又踩了一次"本机冒烟忘 env -i"的老坑（及时发现清理，
  没有污染宿主机真实数据）。**追记：部署真机后当场被用户真实用量逮到一个漏网分支**——本机测试
  只覆盖了"刷新页面重传"，没覆盖"同一批里排两份同名同大小文件"（`existing` 快照上传成功后没
  更新，跟 CLI 那版对齐时漏移植了这一步），真机上传一本漫画 PDF 直接造出 `1_x`/`2_x` 两份真实
  重复。已修+新增第二条测试覆盖"同批次重复"+重新真机部署验证 `PASS`，细节同样记在
  `shelf/docs/reMarkable书架白皮书.md` §04（跟上面这条同一处追记）。真机母版库里那两份测试
  造出的重复文件还留着，需要用户自己去网页删，没有替用户做删除决定。
- **全局并发/内存预算闸门 + 母版库批量操作（2026-09-19）**：漫画 EPUB→PDF 那条改动（见
  `shelf/docs/bookconv优化白皮书.md` §16）做完后真机测出"漫画 optimize/超限分卷投递的内存
  峰值 ≈ 处理的文件体积本身"（245MB 源书 optimize 峰值 206MB）。用户追问"一本一本点最多能
  点几个"——代码层面完全没有限制：`book-serve`/`koreader-serve` 的忙锁都按书名分别加，点
  不同的书互不阻塞，每次点击起一个新线程；同时点几本大部头会线性叠加内存，这台设备只有
  ~2GB、`systemd MemoryMax` 又没有真正生效（无安全网），是真实的 OOM 风险。要求：加全局并发
  上限，**按内存用量限流而不是按操作个数**（小书可以多点几个，大书基本只能点一个），同时把
  "批量优化"前端功能做出来、受这个限流保护，范围覆盖"优化"/"加入xochitl"/"加入KOReader"
  三个操作。
  **架构决策：为什么落在 gateway，不是 book-serve/koreader-serve 自己做**——三者是完全独立
  的进程（各自独立 `[[bin]]`、独立 systemd unit、独立端口），进程内的 `Mutex`/`HashSet` 忙
  锁天然不跨进程；但 `gateway` 是这三个操作物理上唯一必经的转发关口（`proxy.rs::forward()`），
  用进程内的锁就够，不需要引入任何跨进程锁/共享内存/IPC——这是本项目第一次遇到"三个独立
  进程需要协调"的场景，`gateway` 天然是解法，不用另起一套跨进程机制。
  **实现**：新增 `budget.rs`——不做连续字节预算求和（没有足够数据支撑不同操作类型/书籍类型
  精确的内存倍率，强行量化是假精确），按体积分两档：>90MB 算大档、同一时刻最多 1 个在跑；
  否则小档，最多 3 个并发。`forward()` 只拦截三个操作（读小 body 拿书名→本地 `stat` 母版库
  文件体积→`admit(tier)`→正常转发），其余请求（含大文件上传）不受影响。**名额释放时机区分
  同步/异步是这次设计最容易踩的点**：`优化`/`加入xochitl` 在 book-serve 是异步的（HTTP
  响应几乎立即回"已开始"，真正处理在后台线程跑），不能靠"HTTP 响应返回"这个信号释放名额，
  改成网关自己起后台线程轮询该服务 `GET /staging` 列表直到条目 `busy==false`（或者条目已经
  不在了——对应漫画→PDF 优化改名场景）才释放；`加入KOReader` 在 koreader-serve 是同步的
  （`fs::copy`+`fs::rename`），响应返回=真正做完，名额直接随请求释放，不用轮询。
  前端 `app.js` 母版库列表加勾选框+三个批量按钮，跟单条按钮并存不是替代——价值点是"选中的
  书受网关闸门保护，用户不用自己算这次该点几本"（历史上笔记模块的批量勾选被砍是因为"跟已有
  单条按钮重复"，这次特意确认过不是同一个问题）。同一轮用户反馈"母版库列表太长"，追加做了
  默认隐藏已完成的书（开关状态记 localStorage）+ 分页（"显示更多"按需追加，**不是数字页码**
  ——用户明确要求"考虑手机端操作的便利性"，数字分页触屏容易点不准，"显示更多"大按钮+符合
  手机上下滑动习惯，这是定案理由）+ 去掉"KOReader未安装"每行重复提示（改列表顶部提示一次）。
  host 单测全绿（gateway 25个，含 `budget.rs` 6个+`proxy.rs` 2个新增），aarch64-musl 交叉
  编译零告警。**真机验证现状（如实记录，不是"已验证"）**：已部署到设备，md5 校验通过、
  `gateway.service` 健康检查通过（`ActiveState=active`/`NRestarts=0`）；但闸门本身的真机
  行为（两本大书同时优化是否真的被网关串行化、VmHWM 是否没有叠加）和前端可视化部分（勾选框/
  批量按钮/隐藏已完成开关/显示更多分页/列表是否显得更紧凑）**都还没有验证过**——自动化验证
  卡在网关登录鉴权上（后台任务没有账号密码，且正确地没有尝试重置密码这种会影响用户真实登录
  凭证的操作），需要用户自己在浏览器里操作确认。改动在 `feat/gateway-concurrency-budget`
  分支，未合并 master。
  **追记一：用户真机点开批量优化 UI 后报了三条反馈，倒查出一次分支分叉 bug**——① 点了批量
  按钮，单条按钮和另外两个批量按钮照样能点，会互相打架；② 批量运行期间完全看不出在处理哪
  一本；③"加入 xochitl"对超限 PDF 一律灰掉，用户确认设备上明明支持 PDF 分卷投递。①②先按
  "运行期间整个列表+其余批量按钮全锁"修了一版，结果锁过头——"取消选择"也被锁死，选错书没法
  停。③ 查出真根因：这条分支跟已经真机验证过 PDF 分卷投递的 `feat/comic-pdf-optimize`
  （§16 见 `shelf/docs/bookconv优化白皮书.md`）是从同一个 master 提交分出去的**兄弟分支**，
  互相都没有对方的改动——设备上实际跑的 `book-serve` 早就是 comic-pdf-optimize 那条分支
  （之前会话部署+验证过），已经支持 PDF 分卷，但这条分支的前端还停在"PDF 一律不能拆"的旧
  假设。修法：先把 `feat/comic-pdf-optimize` 合并进这条分支（自动合并无冲突，`shelf`
  workspace 32+171+14 测试全绿；`fix/comic-split-text-and-layout`/`fix/comic-split-toc-loss`
  两条派生分支经 `git merge-base --is-ancestor` 核实已经完全包含在 comic-pdf-optimize 历史
  里，不用单独处理），这条分支现在**同时带着并发闸门+批量 UI 和漫画 PDF 分卷投递两块功能**，
  仍未合并 master。再改三处：`app.js` 的 `tooBig` 判断从"只有 EPUB 例外"改成"EPUB/PDF 都
  例外"（PDF 超限一样不提前灰按钮，真拆不了服务端会给清楚拒绝原因，跟 EPUB 同一套"信任服务端
  错误"套路）；批量提交从 `Promise.allSettled` 一次性并发改成逐项顺序 `await`，任意时刻
  `batchQueued` 最多一个名字，行内进度条天然变成"当前在处理哪本"的准确指示，工具栏文案补
  `{current}` 当前书名；"取消选择"从批量按钮的统一禁用列表里摘出来，永远可点，运行期间变身
  "停止"，点了置 `batchAbort`，循环每轮检查，没发出去的请求直接跳过（已经发出去、服务端在
  跑的那项救不回来，是诚实边界不是没做全），复选框同理不再锁。顺手修了一个真 bug：漫画
  optimize 会把 EPUB 改名成 PDF，旧名字从列表消失但可能还留在前端 `picked` 选中集合里、
  变成谁也点不掉的幽灵计数，改成每次 `refresh()` 按当前列表名字集合清理一遍。
  **追记二：用户追问"优化后书籍直接变 PDF 了，已优化/未优化状态意义何在"**——纠正了一个
  过度概括的前提：只有漫画 EPUB 优化后才转 PDF，普通文字 EPUB 优化后还是 EPUB，那条徽章链
  没有失去意义。但确认了一个真 gap：漫画转出来的 PDF 在列表里跟用户自己上传的原生 PDF 视觉
  上分不出来——后端 `staging.rs::list()` 其实早就用 `looks_like_own_comic_pdf`（有没有自己
  写的书签目录）正确算出了 PDF 的 `it.level`/`it.optimized`，只是前端一直没用这个信号。用户
  确认"加，别的PDF不动"后补了一条徽章分支：`format==='pdf'` 时 `it.optimized` 为真显示
  "已优化(漫画)"（`on` 样式+说明留白已裁到≈0、带书签目录），为假维持原来的"原样"，不碰其它
  PDF 的展示。用户后续追问"这个还可以用于隐藏已完成标记"，查证 `isBookDone()` 的
  `optimizedOk` 对所有非 EPUB 格式本来就无条件为真（只看有没有投递），已经是对的——用户确认
  "没有新需求，只是认可现状"，没有改代码。
  **追记二处改动的真机验证现状**：均已交叉编译部署（追记一：book-serve+gateway 一起部署；
  追记二：仅 gateway，app.js/locale 是 `include_str!` 编进二进制的），md5 校验通过、两次
  `systemctl restart` 后都 `active`/`NRestarts=0`。但**停止按钮是否真能中断、当前处理项
  指示是否清楚、PDF 分卷按钮点了是否真能成功投递、"已优化(漫画)"徽章渲染是否正常**，这几条
  都还没有用户独立复核过——闸门本身"两本大书是否真被串行化+VmHWM 不叠加"这条最核心的验证
  仍然是本节开头记的那个老缺口，没有任何进展，不要误读成已经解决。

## 05｜命名遗留 + 待办

**命名遗留（有意不动，范围外）**，理由都是"牵连已部署设备真实路径/配置，需要专门迁移
方案"：
- XDG 运行时命名空间仍是 `shelf`（`$XDG_RUNTIME_DIR/shelf/services/`、`~/.config/shelf/`）。
- systemd `shelf.target`。
- 登录默认密码字面量 `"shelf"`。
- mDNS 伪域名 `shelf.local`。

**待办**：
- 上面几处要不要处理、什么时候处理，还没有排期。
- ~~跟 `rmsvc-core` 白皮书 §05 同样的免责声明：这次重构（正名+wallpaper/font 迁移）只做了
  host 侧验证，**没有真机验证**。~~ **追记（2026-09-11）**：`packaging/install-all.sh` 真机
  跑通后这条已经不成立——`shelf` 步骤（含网关）在真实设备上部署+启动，健康检查表里
  `gateway` 从 `activating` 变 `active`，用户确认"已成功安装"。正名后的二进制/单元名在
  真机上跑起来行为符合预期，不再是"推断"。
