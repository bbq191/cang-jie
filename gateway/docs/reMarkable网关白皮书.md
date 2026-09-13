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
