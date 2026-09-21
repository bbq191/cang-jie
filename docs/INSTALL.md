# 安装部署指南

**[English](INSTALL.en.md)** · 返回 [README](../README.md)

> **读者与用途**：第一次给 reMarkable Paper Pro Move 装这套增强、想卸载、或固件升级（OTA）后要恢复功能的人。
> 按顺序读「适用范围 → 装之前 → 安装」就能装完；出问题跳到「常见问题与风险项」；升级固件跳到「固件升级（OTA）之后」。
> 想先了解这套东西是什么，读 [`OVERVIEW.md`](OVERVIEW.md)。安装脚本内部怎么写的、怎么本机测试，在 [`../packaging/README.md`](../packaging/README.md)（开发者向，本文不重复）。

## 适用范围

**reMarkable Paper Pro Move（imx93-chiappa），固件 3.28.0.172**——目前唯一经过真机验证的版本，
装之前脚本会自动核对（见「固件安全门」）。其它固件版本、其它 reMarkable 型号未验证，强装有
QML 注入定位错位的风险（轻则某个功能不生效，重则影响设备正常使用）。

## 装之前：4 样东西需要你手动装好

它们是 reMarkable 官方/第三方生态的基础设施，不属于这个仓库，`install-all.sh` **不会**代装；
缺了会在对应步骤清楚报错并告诉你该跑哪条命令。vellum（设备上的包管理器）本身怎么装、appload/KOReader
怎么侧载，请看 vellum 与 reMarkable 社区自己的文档，这里不重复。

| # | 在设备上做什么 | 它是什么 | 缺了会怎样 |
|---|---|---|---|
| 1 | `vellum add xovi` | [xovi](https://github.com/asivery/xovi)：扩展加载框架 | 本仓库大部分功能都是 xovi 扩展，缺了相关步骤直接失败 |
| 2 | `vellum add qt-resource-rebuilder` | 界面 QML 补丁（qmd）加载器 | 字体菜单、回收站/新建文件夹代理、漫画页边距代理、侧栏入口**静默跳过**（不算失败），其余不受影响 |
| 3 | `vellum add appload`（**≥ 0.6.0**） | 第三方 App 加载器 | 侧栏 KOReader 入口不出现（`sidebar-entry` 步自动跳过） |
| 4 | 经 appload 侧载 KOReader | 第二个阅读器 | `koreader-serve` 只管理已装好的 KOReader，不负责装；侧栏入口点了没反应 |

可选：第三方 **WeRead** app（微信读书 reMarkable 版）。装了，`sidebar-entry` 会自动探测到并多加一项「WeRead」入口；没装不影响任何功能。

## 安装

![install-all.sh 流程](diagrams/install-flow.svg)

### 推荐顺序

1. **确认固件版本**：设置里看系统版本，目前只有 3.28.0.172 验证过。
2. **按依赖顺序手动装好上面 4 样**（vellum add xovi → qt-resource-rebuilder → appload → 侧载 KOReader；可选装 WeRead）。装完 appload 先确认侧栏出现了原生「AppLoad」图标（见问题①）。
   ⚠ **appload 与 WeRead 两步之间隔几分钟，别背靠背做**：检查图标可能要重启一次 xochitl，WeRead 每次进出各让 xochitl 停起一次；短时间堆几次重启会撞上重启保护、触发整机重启（2026-09-11 真机踩过，见问题③）。
3. **跑一条命令**（USB 连电脑，设备在这条链路上默认是 `10.11.99.1`）：
   ```sh
   git clone https://github.com/bbq191/rm-tweak.git   # 公开发行版；私有开发仓库 cang-jie 只有维护者能 clone
   cd rm-tweak/packaging
   sh install-all.sh 10.11.99.1
   ```
   > 关于仓库：公开版按其 README 与本仓库目录结构一致，但**未逐文件核对过公开仓库里有没有 `packaging/install-all.sh`**；clone 后若缺，以私有仓库为准。
4. **看收尾汇总**：确认「已安装」列表符合预期。"跳过"不等于"失败"，容易漏看（见问题①②）。有失败项先处理它，其余步骤已落地，不用整个重跑（重跑也安全，脚本全部幂等）。
5. **改密码**：浏览器打开下面的地址，首次登录强制跳转改密页。
6. **肉眼验证侧栏入口**（如果这步没被跳过）：回设备主界面，确认侧栏 KOReader 下方出现了预期入口并能点开——这步没有脚本能替你确认。

### 每一步装了什么

`install-all.sh` 先过固件安全门，再按下表顺序执行。表里每个**步骤名**都能用于 `--skip`，对应脚本 `packaging/deploy-<步骤名>.sh <host>`（`shelf` 步是 `deploy.sh`）也可脱离 `install-all.sh` 单独运行。

| 步骤名 | 做什么 | 前置 |
|---|---|---|
| `chrony-cn` | chrony 时间服务器换成国内可达的（阿里云/腾讯云等） | — |
| `chrony-boot-wakelock` | 开机头一小段（同步成功即放，最多 120 秒）持一把 wakelock，防设备自动休眠打断 chronyd 首次校时 | — |
| `timezone-cn` | 默认时区设为 Asia/Shanghai | — |
| `battop` | 电池诊断采样服务；装完启动，但**不随开机自启**（有意的，见问题⑥） | — |
| `wifi-watch` | WiFi 载波假死看护：wlan0 假死时自动 `nmcli con up`，并固化 2.4G 频段与关闭省电；链路正常时零 fork | — |
| `xovi-persist` | 装一个开机自动重跑 `xovi/start` 的单元，重启后不用再手动补 | 已 `vellum add xovi` |
| `hl-snap` | 荧光笔 CJK 精确吸附（划哪吸哪，不再"划一小段吸整行"）；只落盘 | 同上 |
| `handwriting-stroke` | 按笔尖角度/运笔速度优化手写笔画粗细（默认关，网页「实验室」开）；只落盘 | 同上 |
| `sidebar-entry` | 侧栏直达「KOReader」；装了 WeRead 自动多一项「WeRead」；只落盘 | 已装 qt-resource-rebuilder 与 appload（见问题①） |
| `shelf` | 九个 Web 服务：网关（gateway）、书籍管理（book / koreader）、字体/壁纸（font / wallpaper）、笔记线四服务（ink / transcribe / mind / note）；相关 qmd 只落盘 | —（qmd 依赖 qt-resource-rebuilder，缺了自动跳过） |
| `xovi-apply` | 上面"只落盘"的内容全部就位后，**重启 xochitl 一次**让它们生效（**会闪屏、打断阅读**，见问题③⑤） | — |

失败的步骤不会自动重试、也不会被静默跳过——照报错原样处理即可。

### 装完之后

浏览器打开 `https://10.11.99.1/`（同一网段也可用 `https://shelf.local/`，注意安卓系统不解析 `.local` 域名）：

- 默认密码 `shelf`，**首次登录必须改密码**（系统强制跳转改密页）。
- 会提示证书不受信任（私有 CA 自签）：登录页有「下载 CA 证书」，装进浏览器/系统信任库一次就不再提示；临时用也可点「高级 → 继续访问」。

## 常用选项

### 固件安全门

装之前 `install-all.sh` 会 ssh 到设备，取 `/usr/bin/xochitl` 的 sha256，对照 `packaging/firmware-allowlist.txt`（仓库里记录的已验证哈希）与本机的 `firmware-allowlist.local.txt`：命中才继续，不命中默认拒装。用哈希而不是版本号，是因为字体菜单、回收站代理这类功能靠**字节级 QML 注入定位**，同版本号的热修补丁也可能挪动内部布局。

确认这台设备的固件就是你验证过的、只是哈希没登记：加 `--force`。当前哈希会追加到**本机**文件 `packaging/firmware-allowlist.local.txt`（已 gitignore、不入 git，不改动仓库里被跟踪的白名单），之后同一固件不再需要 `--force`。

```sh
sh install-all.sh 10.11.99.1 --force
```

### 只装一部分

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist    # 跳过指定步骤
```

可跳过的步骤名就是上表的第一列；写错名字不会报错退出，但会打印"不是已知步骤名"的警告并列出已知名字。

**只装书架并设网关密码**：

```sh
cd packaging && sh deploy.sh 10.11.99.1 --only book,koreader --password '新密码'
```

`--only` 可选令牌是 `gateway book koreader font wallpaper ink transcribe mind note`（网关总会装，写了别的令牌设备端 `install.sh` 报错退出，退出码 2）。`--password` 的值经 ssh 标准输入写进设备上 0600 的临时文件、由设备端读后即删——含空格、引号的密码不会被远端 shell 解释，`ps` 里也看不到。

## 卸载

`packaging/uninstall-all.sh` 与安装**共用同一张步骤表**，所以是对称的：

```sh
cd packaging
sh uninstall-all.sh 10.11.99.1                    # 卸全部
sh uninstall-all.sh 10.11.99.1 --skip shelf       # 跳过某步（名字同上表，写错会警告）
sh uninstall-all.sh 10.11.99.1 --purge            # 额外删 battop 的二进制与历史采样数据
```

- `chrony-cn`、`timezone-cn` 是配置覆写，`xovi-apply` 是纯动作，都**没有卸载语义**，不动。vellum / xovi / qt-resource-rebuilder / appload 本体与 KOReader 侧载从来不是本项目装的，也不卸。
- `shelf` 步优先调设备上的 `~/.local/bin/shelf-uninstall`（单一事实源），没有才退回 `shelf-pkg` 里的副本；默认保留用户数据（母版库、配置、证书、字体/壁纸池）。`--purge` **不**作用于 shelf——要连数据删，先 `--skip shelf`，再在设备上跑 `shelf-uninstall --purge`。
- 摘掉 xovi 扩展/qmd 后，运行中的 xochitl 仍是旧映射，要等下次重启才真正停用；卸载脚本**不主动重启**。想重启：`systemctl restart xochitl`（xovi 已生效时别用 `xovi/start`，见问题⑤）。
- **未在真机验证**：卸载脚本只做过本机模拟测试（假 ssh/systemctl），真机上各单元/扩展是否被摘干净、其它功能是否不受影响，尚待验证。

## 固件升级（OTA）之后

这是 OTA 恢复的**权威说明**（`packaging/README.md`、`shelf/README.md`、书架白皮书都链接到这里，不再各写一份）。

![OTA 之后：哪些丢了、怎么恢复](diagrams/ota-recovery.svg)

**升级本身零风险、`/home` 数据零丢失；但升完要重跑一遍安装，功能才回来**——不是"升了就能用"。设计上我们不在启动路径留任何东西（xovi 预载在 `/etc` tmpfs、单元在 `/usr`），所以新固件永远以纯原厂起来。3.27.3.0 → 3.28.0.172 的实录见书架白皮书 §03v。

### 推荐流程

1. （升级前，可选）把与新固件不兼容的 xovi 扩展（如旧版 appload）挪出 `extensions.d/`，放到 `/home/root/xovi-disabled/`——**绝不留在 `extensions.d/` 里**（xovi 把该目录下任意文件当扩展加载）。
2. 升级完成后，**在设备旁手动**跑 `xovi/rebuild_hashtable`（要 root 密码、交互输入，`install-all.sh` 不代做）。它是后面 qmd 重新注入的前提。
3. 在电脑上：`cd packaging && sh install-all.sh <设备IP>`。新固件的 sha256 通常不在白名单里，安全门会拒装——确认这台设备的固件就是你要装的版本后加 `--force`。脚本全部幂等，缺什么补什么。
4. 看收尾汇总、浏览器打开网关确认。appload 需 ≥ 0.6.0（旧版先 `vellum upgrade appload` 并整机重启），不在编排里。

### 逐项对照

| 内容 | 位置 | OTA 后 | 恢复 |
|---|---|---|---|
| 母版库 / KOReader 配置 / 字体与壁纸池 / 证书 / 网关密码 / 休眠屏 conf 键 / `cangjie-backups/` / battop 历史数据 | `/home` | 保留 | 无 |
| shelf 各服务的二进制（`~/.local/bin`） | `/home` | 保留 | 无 |
| hl-snap / handwriting-stroke 的 `.so`、侧栏入口与字体菜单/回收站/建夹代理的 qmd | `/home`（`extensions.d/`、`exthome/`） | 文件在，但 hashtab 过期、未注入 | 步骤 2 的 `rebuild_hashtable`，再 `install-all.sh`（`xovi-apply` 使其生效） |
| shelf 各服务与 `shelf.target` 的 systemd 单元 | `/usr` | **冲掉** | `shelf` 步（或单独：`cd packaging && SHELF_NO_BUILD=1 sh deploy.sh <设备IP>`） |
| `xovi-reenable.service`（xovi 开机持久化） | `/usr` | **冲掉** | `xovi-persist` 步 |
| `chrony-boot-wakelock.service` | `/usr` | **冲掉** | `chrony-boot-wakelock` 步 |
| `battop.service` | `/usr`（数据在 `/home`） | 单元**冲掉** | `battop` 步（装完 start，不自启） |
| `wifi-watch.service` | `/usr`（脚本 `~/.local/bin/wifi-watch.sh` 在 `/home`） | 单元**冲掉** | `wifi-watch` 步 |
| 国内 NTP（chrony 配置）、默认时区 | `/etc` | **冲掉** | `chrony-cn` / `timezone-cn` 步 |
| appload ≥ 0.6.0 | `/home`（xovi） | 视 appload 是否被重装 | 独立：设备上 `vellum upgrade appload`，然后整机重启 |

**风险分层**（不要合成一个百分比）：书架这一层只用 xochitl 的 `/upload` 网页接口和系统标准组件，换固件重装即回；字体菜单这类 qmldiff 注入依赖 xochitl 内部 QML，大版本常要重适配（3.27→3.28 已是两版 qmd）；KOReader 本体独立无碍，但侧栏入口靠第三方 appload，每个固件大版本都要确认 appload 已支持（3.28 起需要 ≥ 0.6.0）。

**"裸机恢复"要额外检查**：OTA 本身不删 `/home`，但若设备经历过更彻底的重置，`/home` 下的 payload（`extensions.d/` 里的 `.so`、各服务二进制）可能一起丢——2026-09-09 真机踩过。重跑 `install-all.sh` 前先确认这些文件还在。

## 常见问题与风险项

下面不是"随机小概率故障"，而是有明确触发条件的已知坑。编号 ①–⑥ 被上文引用。

| # | 现象 / 场景 | 原因 | 怎么办 |
|---|---|---|---|
| ① | 侧栏没有 KOReader/WeRead 入口；汇总里 `sidebar-entry` 是"跳过"；`journalctl` 有 qmldiff 的 "Couldn't resolve the hashed identifier" | appload ≤ 0.5.3 的内置补丁钩的是 3.27 的旧界面锚点，3.28 已改名，appload 自己注入的启动器建不起来。**不会**导致装不上、xochitl 起不来或变砖，只是这一个功能不生效（2026-09-06 在这台设备上发生过，当时 v0.5.3） | `vellum list --installed \| grep appload` 看版本；旧版则 `vellum upgrade appload`。上游 **v0.6.0（2026-09-19）** 已并入 3.28 支持（另加 3.29），2026-09-21 真机验证过（日志出现 "Loaded external AppLoad hooks in main UI"，侧栏入口正常）。**⚠ 升级 appload 后不要 `systemctl restart xochitl`**：运行中的旧进程退出时崩溃，触发 xochitl 单元的 `OnFailure=emergency.target`，整机自动重启一次（2026-09-21 踩到，无数据损坏但打断使用）——换完文件直接整机重启（已装 `xovi-persist` 的话 xovi 开机后自动生效，否则重启后手动跑一次 `xovi/start`） |
| ② | 字体菜单、回收站/新建文件夹代理、漫画页边距代理、侧栏入口这几个互不相关的功能**同时**缺失 | 它们共享同一个前置：`qt-resource-rebuilder`。没装时 `install-all.sh` 汇总里各自标"跳过"，不是"失败" | `vellum add qt-resource-rebuilder` 后重跑 `install-all.sh` |
| ③ | 短时间内 xochitl 反复重启后设备整机重启了一次 | `xochitl.service` 配置为 `Restart=on-failure`、`StartLimitBurst=4`（10 分钟窗口），不管谁触发的重启都计数：本仓库单独跑的部署脚本（`hl-snap` / `handwriting-stroke` / `sidebar-entry` 不经 `install-all` 时各自装完重启一次）、`vellum add appload` 这类第三方安装器、WeRead 每次进出。**真机验证过：连续两次重启就触发了一次整机重启（2026-09-11）——这是设备自己重启后恢复正常，不是变砖**，`xovi-persist` 的开机恢复链当场还被顺带验证了一遍 | `install-all.sh` 自己已处理（落盘先不重启，最后 `xovi-apply` 统一重启一次）。**手动逐个跑部署脚本、或来回折腾 appload/WeRead 时**才需留意：每次间隔几分钟，等上一次重启 `is-active`=active 稳定后再做下一件 |
| ④ | 固件安全门拒装 | 不是 bug，是设计：版本号相同不保证内部布局没变 | 先确认设备固件确实就是你验证过的那份，再 `--force`；盲目强装在没验证的固件上，轻则某功能不生效，重则影响设备正常使用 |
| ⑤ | 装到最后一步屏幕闪一下 | `xovi-apply` 重启 xochitl（合成器+界面进程）。重启前会打印"将打断阅读"并留 5 秒宽限 | 正常现象；装的时候别操作设备。不想被打断：`--skip xovi-apply`，稍后自己重启。**重启姿势**：xovi 已在运行的 xochitl 里生效（`LD_PRELOAD` 含 `xovi.so`）时一律 `systemctl restart xochitl`，**绝不**手动跑 `xovi/start`——它会让运行中的 xochitl SEGV、系统按设计整机自动重启（2026-09-20 真机事故；新版脚本已内建这条判定）。只有 xovi 没生效（刚开机、OTA 之后）才用 `xovi/start` |
| ⑥ | 重启后电池刺客（battop）没在跑 | **有意不开机自启**：2026-08-29 它的采样触发过内核 cgroup/RCU 死锁冻死整机，根因未彻底排除，所以安装器只 `start` 不 `enable` | 网页「管理 → 系统增强」里打开电池刺客开关（开了才会出现「电池刺客」数据页），或 `systemctl start battop`。想恢复开机自启是需要自己评估的决定，别指望安装器悄悄做 |

### 其它排障

- 先看 `install-all.sh` 收尾汇总，定位是哪一步失败；对应的 `packaging/deploy-*.sh` 头注写了这一步做什么、常见失败原因。
- `packaging/README.md` 末尾「验证现状」按时间记录了已知的真机踩坑与修法。
- **不碰真机**就想确认脚本没被改坏：`bash packaging/tests/run_sim_tests.sh`（用假 ssh/systemctl/mount 在临时目录跑真代码，拒绝以 root 运行，CI 里的 pytest 也会调它）。这是本机模拟，**代替不了真机验证**。

### 备份与幂等（一句话版）

所有安装脚本幂等；覆盖设备上已有文件前先备份到 `/home/root/cangjie-backups/`（**绝不**放 `extensions.d/`——xovi 会把那里任意文件当扩展加载，同名重复注册是致命错误），只保留最近 5 份；内容没变就不动；写 `/usr` 前检查 dm-verity，激活则跳过（写 `/usr` 曾触发 A/B 回滚变砖，2026-08-16）。机制细节见 [`packaging/README.md`](../packaging/README.md#备份与幂等)。

## 这套安装器不做什么

- **不装 vellum / xovi / qt-resource-rebuilder / appload 本体，不侧载 KOReader**：见「装之前」，是手动前置条件。
- **不装中文输入法**：这条功能线已从本仓库归档（见 [README](../README.md#历史与范围)），不随本安装器分发；设备上已部署的部分仍在运行。
- **不升级 appload**：3.28 固件需要 ≥ 0.6.0，已装旧版要先手动升级并整机重启（见问题①）。
