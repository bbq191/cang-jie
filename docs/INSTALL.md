# 安装部署指南

**[English](INSTALL.en.md)** · 返回 [README](../README.md)

> **读者与用途**：第一次给 reMarkable Paper Pro Move 装这套增强、装完想卸载、或固件升级（OTA）后要恢复功能的人。
> 先读「装之前」和「推荐安装顺序」，再跑一条命令；出问题看「风险项预警」和「遇到问题」。想先了解这套东西是什么，读 [`OVERVIEW.md`](OVERVIEW.md)。
> 脚本细节（每一步做什么、目录里各脚本的职责、怎么本机测试）在 [`../packaging/README.md`](../packaging/README.md)。

## 适用范围

**reMarkable Paper Pro Move（imx93-chiappa），固件 3.28.0.172**——这是目前唯一经过真机验证的
固件版本，装之前脚本会自动核对（见下「固件安全门」）。其它固件版本/其它 reMarkable 型号未验证，
贸然强装有 QML 注入定位错位的风险（轻则某个功能不生效，重则影响设备正常使用）。

## 装之前：这几样需要你自己手动做一遍

以下都是 reMarkable 官方 / 第三方生态自己的基础设施，不属于这个仓库，`install-all.sh` **不会**
替你装——缺了会在对应步骤给出清楚的报错，告诉你该跑哪条命令：

1. **`vellum add xovi`** —— [xovi](https://github.com/asivery/xovi) 本体（第三方扩展加载框架）。
   本仓库大部分功能都是以 xovi 扩展的形式运行的，这是最基础的前提。
2. **`vellum add qt-resource-rebuilder`** —— QML 资源热替换。`shelf`/`gateway` 里少数可选特性
   （字体菜单、回收站/建夹网页代理）依赖它；没装这两个特性会自动跳过，不影响其它功能安装。
3. **`vellum add appload`** —— 第三方 App 加载器。
4. 通过 appload **侧载 KOReader**。

vellum 本身怎么安装、appload/KOReader 侧载的具体步骤，请参考 vellum 与 reMarkable 社区自己的
文档——这些是设备通用基础设施，不是本项目维护的内容，这里不重复。

## 推荐安装顺序

按这个顺序做，返工概率最低（下面涉及的风险项详见「风险项预警」一节，编号对应）：

1. **先确认固件版本**——设置里查看系统版本，目前只有 3.28.0.172 验证过（见上「适用范围」）。
2. **手动装齐生态基础设施**（见上一节），按依赖顺序来，不要跳步：
   1. `vellum add xovi`
   2. `vellum add qt-resource-rebuilder`
   3. `vellum add appload`——装完先确认侧边栏能不能正常出现原生「AppLoad」图标（⚠ 风险①，
      3.28 上官方发行版可能不行，不行要先处理，否则后面 Sidebar 一级入口那步会静默跳过）
   4. 通过 appload 侧载 KOReader
   5. （可选）想要 WeRead 侧边栏入口：先按 WeRead 官方发行包自己的说明装好、能正常登录

   ⚠ **第 3 步和第 5 步之间留个几分钟间隔，别背靠背连着做**（详见风险③）：检查 AppLoad 图标
   本身可能需要重启一次 xochitl，WeRead 每次启动/退出各自会再让 xochitl 停一次、起一次——
   短时间内堆几次重启，真机验证过确实会撞上重启保护、意外触发整机重启（2026-09-11）。等第 3
   步确认 `is-active`=active 稳定几分钟之后，再做第 5 步。
3. **跑 `install-all.sh`**（不带 `--skip`，一次装全部）：
   ```sh
   cd packaging && sh install-all.sh 10.11.99.1
   ```
4. **看收尾汇总**——确认「已安装」列表是不是你预期的那些步骤，有没有意外被跳过的（跳过不等于
   报错，容易漏看，见风险①③）；有失败的先处理失败项，其余步骤已经落地，不用整个重跑。
5. **改密码**——浏览器打开网关，首次登录会强制跳转改密页。
6. **人工验证 Sidebar 入口**（如果这步没被跳过）——回到设备主界面，确认侧边栏 KOReader 下方
   出现了预期的入口，点一下确认真的能跳转——这步没有脚本能替你确认，肉眼看一次最踏实。

先把手动前置一步步走完、再一条命令装全部、最后肉眼验证 UI 层的改动，是目前踩坑最少的顺序；
反过来（跳过手动前置直接装全部，事后才发现某几步"看起来失败/看起来跳过了"）回头排查更费时间。

## 一条命令装完

电脑通过 USB 线连上设备（reMarkable 默认在这条链路上把自己配成 `10.11.99.1`）：

```sh
git clone https://github.com/bbq191/rm-tweak.git   # 公开发行版；见下方说明
cd rm-tweak/packaging
sh install-all.sh 10.11.99.1
```

> 仓库地址说明：`cang-jie`（`git@github.com:bbq191/cang-jie.git`）是带完整开发历史的**私有**仓库，只有维护者能 clone；对外分享的是精简的公开发行版 `rm-tweak`（见 [README](../README.md) 头部）。上面用的是 rm-tweak；它的目录结构（含 `packaging/`）按 README 的说明与本仓库一致，但**我没有核对过公开仓库里的实际内容**，如果 clone 后没有 `packaging/install-all.sh`，以私有仓库为准。

这条命令会依次做这些事（任何一步都可以单独运行，见下「只装一部分」）：

| 步骤（`--skip` 用的名字） | 做什么 | 前置 |
|---|---|---|
| 固件安全门 | 核对设备固件跟已验证版本是否一致 | — |
| `chrony-cn` | chrony 服务器换成国内可达的（阿里云/腾讯云等） | — |
| `chrony-boot-wakelock` | 开机头几十秒持一把 wakelock，防设备自动休眠打断 chronyd 首次同步 | — |
| `timezone-cn` | 默认时区设为 Asia/Shanghai | — |
| `battop` | 电池诊断采样服务；装完会启动，但**不随开机自启**（有意的设计，见风险⑥），网页「管理→电池刺客」开关或 `systemctl start battop` | — |
| `wifi-watch` | WiFi 载波假死看护：wlan0 假死自动 `nmcli con up`，并固化 2.4G 与关省电；链路正常时零 fork | — |
| `xovi-persist` | 装一个开机自动重跑 `xovi/start` 的单元，往后重启不用再手动补 | 已 `vellum add xovi` |
| `hl-snap` | 荧光笔 CJK 精确吸附（划哪吸哪，不再"划一小段吸整行"）；只落盘 | 同上 |
| `handwriting-stroke` | 按运笔角度/速度优化手写笔画粗细；只落盘 | 同上 |
| `sidebar-entry` | 侧边栏直达「KOReader」；装了第三方 WeRead app 会自动多一项「WeRead」；只落盘 | 已 `vellum add qt-resource-rebuilder appload`（见风险①）|
| `shelf` | 九个 Web 服务：网关、书籍管理（book/koreader）、字体/壁纸上传即用、笔记线四服务 | — |
| `xovi-apply` | 上面几个 xovi 扩展/qmd 落盘完，最后统一重启 xochitl 一次让它们生效（**会打断阅读**，见风险③⑤） | — |

跑完打印一份汇总：装了什么、（如果有）哪一步失败了、还有哪些步骤需要你手动做（vellum 引导、
KOReader 侧载这些）。任何一步失败都不会自动重试、不会静默跳过——照着报错原样处理即可，脚本
全部幂等，重新跑一遍整条命令是安全的（xovi 已生效时最后一步会先提示、留 5 秒宽限再重启 xochitl，见风险③⑤）。

## 装完之后

浏览器打开 `https://10.11.99.1/`（或同一网段内 `https://shelf.local/`，注意安卓系统不支持
`.local` 域名解析）：

- 默认密码 `shelf`，**首次登录后必须改密码**（系统会强制跳转改密页）。
- 会提示证书不受信任（自签证书）——登录页有「下载 CA 证书」，装进浏览器/系统信任库一次即可
  不再提示；临时用途也可以直接点「高级 → 继续访问」跳过。

## 固件安全门

`install-all.sh` 装之前会 ssh 到设备，拉 `/usr/bin/xochitl` 的 sha256，跟仓库里
`packaging/firmware-allowlist.txt` 记录的已验证哈希比对——命中才继续装，不命中默认拒绝。这么
严格是因为字体菜单、回收站代理这类功能靠**字节级 QML 注入定位**，哪怕版本号相同、固件的热修
补丁也可能挪动内部布局，版本号不足以保证安全。

确认这台设备的固件就是你自己验证过没问题的、只是哈希没登记进白名单：加 `--force`。当前哈希会追加到**本机**文件 `packaging/firmware-allowlist.local.txt`（已 gitignore、不入 git，不会改动仓库里被跟踪的白名单）；之后同一固件不再需要 `--force`。

```sh
sh install-all.sh 10.11.99.1 --force
```

## 只装一部分 / 跳过某几步

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist
```

可跳过的步骤名：`chrony-cn`、`chrony-boot-wakelock`、`timezone-cn`、`battop`、`wifi-watch`、`xovi-persist`、
`hl-snap`、`handwriting-stroke`、`sidebar-entry`、`shelf`、`xovi-apply`。写错名字不会报错退出，但会打印一条"不是已知步骤名"的警告（并列出已知名字）。每一步对应的脚本（`packaging/deploy-<step>.sh <host>`；shelf 是 `deploy.sh`）也都可以脱离 `install-all.sh` 单独运行。

**只装书架并设网关密码**：`cd packaging && sh deploy.sh 10.11.99.1 --only book,koreader --password '新密码'`。`--only` 可选令牌是 `gateway book koreader font wallpaper ink transcribe mind note`（网关总会装），写了别的令牌设备端 `install.sh` 报错退出（退出码 2）。`--password` 的值经 ssh 标准输入写进设备上 0600 的临时文件，由设备端 `install.sh --password-file` 读后即删——含空格、引号的密码不会被远端 shell 解释，`ps` 里也看不到。

**脱离编排手动跑设备端 `install.sh` 时的文件依赖**：`shelf/install.sh` 需要同目录的 `manifest.sh` 与 `devlib.sh`（`deploy.sh` 已把它们一起打进载荷，只手拷单个 `install.sh` 不够）；`hl-snap`/`handwriting-stroke` 的 `deploy/install.sh` 需要同目录的 `xovi-ext-install.sh` 与 `devlib.sh`（`deploy-hl-snap.sh` 等会一起推送）；`battop/install.sh` 需要同目录的 `devlib.sh`。

## 这套安装器不做什么

- **不装 vellum/xovi/qt-resource-rebuilder/appload 本体，不侧载 KOReader**——见上面「装之前」，
  这些是手动前置条件。
- **不装中文输入法**——这条功能线已经从本仓库归档（见顶层 [README](../README.md)「历史与范围」），
  当前不随本安装器分发。
- **不打 appload 的 3.28 兼容补丁**：那是独立手动步骤 `packaging/deploy-appload-patch.sh <host>`，不在编排里（见风险①）。

## 卸载

`packaging/uninstall-all.sh` 一次性卸掉 `install-all.sh` 装的全部组件，步骤表与安装**共用同一份**，所以对称：

```sh
cd packaging
sh uninstall-all.sh 10.11.99.1                    # 卸全部
sh uninstall-all.sh 10.11.99.1 --skip shelf       # 跳过某步（名字同上表，写错会警告）
sh uninstall-all.sh 10.11.99.1 --purge            # 额外删 battop 的二进制与历史采样数据
```

- `chrony-cn`、`timezone-cn` 是配置覆写，`xovi-apply` 是纯动作，都**没有卸载语义**，不动；vellum/xovi/qt-resource-rebuilder/appload 本体与 KOReader 侧载从来不是本项目装的，也不卸。
- `shelf` 步优先调设备上的 `~/.local/bin/shelf-uninstall`（单一事实源），没有才退回 `shelf-pkg` 里的副本；默认保留用户数据（母版库、配置、证书、字体/壁纸池）。`--purge` **不**作用于 shelf——要连数据删，`--skip shelf` 后自己在设备上跑 `shelf-uninstall --purge`。
- 摘掉 xovi 扩展/qmd 后，运行中的 xochitl 里仍是旧的映射，要等下次 xochitl 重启才真正停止生效；卸载脚本**不主动重启**。要重启：`systemctl restart xochitl`（xovi 已生效时别用 `xovi/start`，见风险⑤）。

## 备份与幂等

所有安装脚本都幂等，覆盖设备上已有的东西之前会先备份：

- 备份统一放进 `/home/root/cangjie-backups/`（**绝不**放在 `extensions.d/` 里——xovi 把该目录下任意文件当扩展加载，同名重复注册是致命错误）；单文件备份名 `<文件名>.bak.pre-<时间戳>`，shelf 是 `shelf-<时间戳>/` 目录。
- 只保留最近 **5** 份（设备端环境变量 `CJ_BACKUP_KEEP` 可调），且只轮转脚本自己生成、名字严格符合时间戳格式的备份；手工命名的备份、单文件超过 64MB 的备份、任何用户数据都不会被自动删。
- 内容没变就不动（不重启对应服务、不堆重复备份）；写 `/usr` 前检查 dm-verity，激活则跳过（写 `/usr` 触发过 A/B 回滚变砖，2026-08-16）。

完整的架构决策、每一步踩过的坑、真机验证现状，见 [`packaging/README.md`](../packaging/README.md)——
这是面向工程细节的参考文档，本文件只是面向"第一次装"的快速上手指南。

## 风险项预警：哪些模块容易出问题

下面这几项不是"随机小概率故障"，是已知的、有明确触发条件的坑——装之前心里有数，能少走弯路。

① **AppLoad 官方发行版（v0.5.3）在 3.28 固件上有兼容问题，且失败是静默的——但不会装不上、
   不会导致 xochitl 起不来、更不会变砖**
   AppLoad 自带的内部注入补丁钩的是 3.27 的旧界面锚点，3.28 已经改名——不打第三方兼容补丁的
   话，AppLoad 自己往界面注入的启动器组件建不起来，`journalctl` 会报一条 qmldiff 层面的
   "Couldn't resolve the hashed identifier"。**这条已经在这台设备的真实历史上发生过**
   （2026-09-06，当时装的是没打补丁的原始 v0.5.3）：**`vellum add appload` 这一步本身照常
   装成功**（纯文件级安装，不看固件版本），**xochitl 也照常正常启动、照常能用**——观察到的
   唯一现象是 AppLoad 那个入口图标没出来、点不了，是"这一个功能没生效"，不是"重启失败"更
   不是"设备变砖"。会导致真正开不了机/变砖的场景（改 xochitl 的 systemd 启动依赖导致依赖
   死锁）是完全不同的另一类事故，跟这里说的 QML 锚点找不到不是一回事，机制上不会互相牵连。
   **症状**：`sidebar-entry` 那步会自动探测到这个问题并跳过（不报错、不算安装失败），侧边栏
   就是不会出现 KOReader/WeRead 入口，容易被误认为"这功能本来就没做"。**怎么确认踩没踩这个
   坑**：跑完 `install-all.sh` 看汇总里 `sidebar-entry` 是"已安装"还是"跳过"；跳过了但你
   确实需要这个入口，见 `packaging/README.md`「前置条件」第 3 条的修复指路。

② **`qt-resource-rebuilder` 缺失会让好几个功能同时静默不装，容易误判成"装坏了"**
   字体菜单增强、回收站/新建文件夹网页代理、Sidebar 一级入口——这三个互不相关的功能背后共享
   同一个前置（`vellum add qt-resource-rebuilder`）。装之前没留意这条，装完发现好几个看起来
   不相关的功能同时缺失，容易怀疑是不是哪一步真的失败了；实际上都是同一个原因，`install-all.sh`
   的汇总输出会分别标"跳过"，不是"失败"。

③ **短时间内让 xochitl 反复重启/停起，不管是谁触发的，都有撞上重启保护的风险**
   `xochitl.service` 当前配置 `Restart=on-failure`、`StartLimitBurst=4`（10 分钟窗口内），
   触发条件不看"谁"发起了重启——本仓库自己的部署脚本（`hl-snap`/`handwriting-stroke`/
   `sidebar-entry` 三步单独跑、不通过 `install-all.sh` 时，各自装完都会重启一次 xochitl）、
   `vellum add appload` 这类第三方安装器的自身重启、WeRead 这类"启动时接管屏幕/退出时交还"
   的 app（每次进出各让 xochitl 停一次起一次）——都算。**真机验证过短时间内连续重启两次就
   触发了一次意外整机重启**（2026-09-11，见「推荐安装顺序」第 3/5 步之间的提醒）——**这是
   设备自己重启了一下、重启后一切正常，不是变砖**：`uptime` 显示重启后正常起来，`xovi-persist`
   那步装的开机恢复链当场还顺带被这次意外重启验证了一遍（见 `packaging/README.md`「验证
   现状」），没有留下任何后遗症，纯粹是"多等了几分钟"的体验问题，不是数据丢失或设备损坏。
   `install-all.sh` 编排自己那几步时已经处理好了（落盘先不重启，最后 `xovi-apply` 统一重启一次；
   **xovi 已生效时走 `systemctl restart xochitl`，没生效才走 `xovi/start`**，重启前先提示"将打断阅读"并留 5 秒宽限，
   不想被打断就 `--skip xovi-apply`、稍后自己重启），**手动逐个
   跑部署脚本、或者手动交替折腾 appload/WeRead 这类会重启 xochitl 的第三方 app 时**才需要
   自己留意——间隔几分钟、确认上一次重启已经 `is-active`=active 稳定下来，再做下一件事，别
   背靠背连续折腾。

④ **固件安全门拒装不是 bug，是设计如此——`--force` 前先确认真的是同一份固件**
   字体菜单、回收站代理这类功能靠字节级 QML 注入定位，版本号相同不代表内部布局一定没变
   （热修补丁可能悄悄挪动过）。被拒装时，先确认这台设备的固件真的和你验证过的完全一致，再决定
   要不要 `--force`——盲目强装在没验证过的固件上，轻则某功能不生效，重则可能影响设备正常使用。

⑤ **装到最后一步 `xovi-apply` 时屏幕会闪烁一次，属于正常现象**——它会重启 xochitl（合成器+界面进程），
   之前会先打印"将打断阅读"并留 5 秒宽限。装的时候不要在设备上做其它操作，等收尾汇总打印完再用设备。
   **⚠ 重启 xochitl 的正确姿势**：xovi 已在运行的 xochitl 里生效（`LD_PRELOAD` 含 `xovi.so`）时，一律
   `systemctl restart xochitl`，**绝不**手动跑 `xovi/start`——它会让运行中的 xochitl SEGV，系统按设计整机
   自动重启（2026-09-20 真机事故；重跑 `install-all.sh` 旧版必踩，新版已内建这条判定）。只有 xovi 没生效
   （刚开机、OTA 之后）才用 `xovi/start`。

⑥ **battop 不随开机自启，是有意的**——2026-08-29 它的采样触发过内核 cgroup/RCU 死锁冻死整机，根因未彻底
   排除，所以只 `start`、不 `enable`。要用就在网页「管理→电池刺客」打开，或 `systemctl start battop`。想恢复开机自启
   是要自己评估的决定，别指望安装器悄悄做。

## 固件升级（OTA）之后

这是 OTA 恢复的**权威说明**（`packaging/README.md`、`shelf/README.md`、书架白皮书都链接到这里，不再各写一份表）。

![OTA 之后：哪些丢了、怎么恢复](diagrams/ota-recovery.svg)

**升级零风险、数据零丢失，随时可升；升完要重跑一遍安装才回来**——不是"升了就能用"。设计上我们不在启动路径留任何东西（xovi 预载在 `/etc` tmpfs、单元在 `/usr`），新固件永远以纯原厂起来；`/home` 原样。3.27.3.0 → 3.28.0.172 的实录见书架白皮书 §03v。

**推荐流程**

1. （升级前，可选）把与新固件不兼容的 xovi 扩展（如 appload）挪出 `extensions.d/`，放到 `/home/root/xovi-disabled/`——**绝不留在 `extensions.d/` 里**（xovi 会把目录下任意文件当扩展加载）。
2. 升级完成后，**在设备旁手动**跑 `xovi/rebuild_hashtable`（要 root 密码、交互输入，`install-all.sh` 不代做）。它是后面 qmd 重新注入的前提。
3. 在电脑上：`cd packaging && sh install-all.sh <设备IP>`。新固件的 sha256 通常不在白名单里，安全门会拒装——确认这台设备的固件就是你要装的那个版本后加 `--force`（追加进本机 `firmware-allowlist.local.txt`）。脚本全部幂等，缺什么补什么。
4. 看收尾汇总、浏览器打开网关确认；appload 的 3.28 兼容补丁是独立手动步骤（`deploy-appload-patch.sh`）。

**逐项对照**（哪些还在、哪些要重装、用哪一步恢复）

| 内容 | 位置 | OTA 后 | 恢复 |
|---|---|---|---|
| 母版库 / KOReader 配置 / 字体与壁纸池 / 证书 / 网关密码 / 休眠屏 conf 键 / `cangjie-backups/` / battop 历史数据 | `/home` | 保留 | 无 |
| shelf 服务的二进制（`~/.local/bin`） | `/home` | 保留 | 无 |
| hl-snap / handwriting-stroke 的 `.so`、Sidebar 入口与字体菜单/回收站/建夹代理的 qmd | `/home`（`extensions.d/`、`exthome/`） | 文件在，但 hashtab 过期、未注入 | 步骤 2 的 `rebuild_hashtable`，再 `install-all.sh`（`xovi-apply` 使其生效） |
| shelf 各服务与 `shelf.target` 的 systemd 单元 | `/usr` | **冲掉** | `install-all.sh` 的 `shelf` 步（或单独：`cd packaging && SHELF_NO_BUILD=1 sh deploy.sh <设备IP>`） |
| xovi 开机持久化单元 `xovi-reenable.service` | `/usr` | **冲掉** | `xovi-persist` 步 |
| 开机防休眠打断校时 `chrony-boot-wakelock.service` | `/usr` | **冲掉** | `chrony-boot-wakelock` 步 |
| 电池诊断单元 `battop.service` | `/usr`（数据在 `/home`） | 单元**冲掉** | `battop` 步（装完 start，不自启） |
| WiFi 看护单元 `wifi-watch.service` | `/usr`（脚本 `~/.local/bin/wifi-watch.sh` 在 `/home`） | 单元**冲掉** | `wifi-watch` 步 |
| 国内 NTP（chrony 配置）、默认时区 | `/etc` | **冲掉** | `chrony-cn` / `timezone-cn` 步 |
| appload 的 3.28 兼容补丁 | `/home`（xovi） | 视 appload 是否被重装 | 独立：`deploy-appload-patch.sh <设备IP>` |

**风险分层**（不要合成一个百分比）：书架这一层只用 xochitl 的 `/upload` 网页接口和系统标准组件，换固件重装即回；字体菜单这类 qmldiff 注入依赖 xochitl 内部 QML，大版本常要重适配（3.27→3.28 已是两版 qmd）；KOReader 本体独立无碍，但侧栏入口靠第三方 appload，每个大版本可能要重打补丁。

**"裸机恢复"要额外检查**：OTA 本身不会删 `/home`，但如果设备经历过更彻底的重置，`/home` 下的 payload（`extensions.d/` 里的 `.so`、各服务二进制）可能一起丢——2026-09-09 真机踩过。重跑 `install-all.sh` 前先确认这些文件还在。

## 遇到问题

先看 `install-all.sh` 跑完的汇总输出，找到具体是哪一步失败；对应的 `packaging/deploy-*.sh`
脚本内部都有详细注释说明这一步在做什么、常见失败原因。仍然没头绪，`packaging/README.md` 末尾的「验证现状」
一节记录了目前已知的真机踩坑和修法。

想在**不碰真机**的情况下确认脚本本身没被改坏：`bash packaging/tests/run_sim_tests.sh`（用假 ssh/systemctl/mount 在临时目录里跑真代码，拒绝以 root 运行，也由 CI 里的 pytest 调用）。
