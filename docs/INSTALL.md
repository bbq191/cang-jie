# 安装部署指南

**[English](INSTALL.en.md)** · 返回 [README](../README.md)

> **这份文档给谁**：第一次给 reMarkable Paper Pro Move 装这套增强的人、想更新或卸载的人、固件升级（OTA）后要恢复功能的人。
> - **第一次装**：按顺序读「适用范围 → 装之前 → 安装 → 装完之后」。
> - **以前装过、现在更新**：直接重跑 `install-all.sh`（见「安装」）；已移除的组件会被自动清掉，见「从旧版升级」。
> - **卸载**：读「卸载」。**升级了固件**：读「固件升级（OTA）之后」。**出问题**：查「常见问题」。
>
> 想先了解这套东西是什么，读 [`OVERVIEW.md`](OVERVIEW.md)。脚本内部怎么写、全部参数与环境变量、每项核对的含义、怎么本机测试，在 [`../packaging/README.md`](../packaging/README.md)（维护者向，本文不重复）。

## 一句话

在你的电脑上跑**一条命令**，脚本会编译好程序、经 ssh 装到设备上、整机重启一次让改动生效，回来后自动核对装得对不对。全程不改 xochitl 本身，大部分东西放在设备的 `/home`（升级固件也不丢）。

**本文会反复出现的几个词**：

| 词 | 意思 |
|---|---|
| xochitl | 设备自带的阅读/笔记主程序。本项目不改它，只在它旁边加东西 |
| vellum | 设备上的第三方包管理器，用来装 xovi 等社区组件 |
| xovi / 扩展 | 第三方的扩展加载框架：xochitl 启动时把 `extensions.d/` 里的 `.so` 插件一起加载。本项目有两个扩展：`hl-snap`（荧光笔吸附）和 `ui-font`（界面字体） |
| qmd / 界面补丁 | 对 xochitl 界面描述文件（QML）的补丁，由 qt-resource-rebuilder 在 xochitl 启动时打上 |
| 只落盘 / 生效 | 扩展和界面补丁只在 xochitl 启动时读，所以文件放到位（落盘）后还要**整机重启一次**才生效 |
| OTA | 固件在线升级。会整体替换系统分区 `/usr`、`/etc`，不动放数据的 `/home` |
| dm-verity | 系统分区的只读校验。开着时脚本一律不写 `/usr` |

![安装与卸载流程](diagrams/install-flow.svg)

## 适用范围

**reMarkable Paper Pro Move（imx93-chiappa），固件 3.28.0.172**。这是目前唯一在真机上验证过的版本，安装脚本动手前会自动核对（见「动手前的自动检查」）。其它固件版本、其它 reMarkable 型号都没验证过；强装的话，界面补丁可能定位错，轻则某个功能不生效，重则影响设备正常使用。

## 装之前

### 设备上：先手动装好 2 样东西

它们属于 reMarkable 第三方生态，不属于这个仓库，`install-all.sh` **不会**代装。前提是设备已经能用 ssh 登录 root、装好了 vellum——怎么开启（reMarkable 需要先打开开发者模式）、怎么装 vellum，请看 reMarkable 官方说明和 vellum 自己的文档。

| # | 在设备上运行 | 它是什么 | 缺了会怎样 |
|---|---|---|---|
| 1 | `vellum add xovi` | [xovi](https://github.com/asivery/xovi)：扩展加载框架 | 插件类步骤（`xovi-persist`、`hl-snap`、`ui-font`、`xovi-apply`）直接失败 |
| 2 | `vellum add qt-resource-rebuilder` | 界面补丁（qmd）加载器 | 界面补丁全部不装（不算失败）：字体菜单、界面字体令牌、回收站/新建文件夹代理、漫画页边距代理、阅读位置代理、阅读器单击翻页。其余不受影响（见常见问题 1） |

装完这两样，直接整机重启设备（`reboot`）让它们生效，别 `systemctl restart xochitl`（见「为什么一律整机重启」）。

2026-09-29 起**不再需要 appload 和 KOReader**（设备只用自带阅读器）。以前装过它们、或装过电池刺客和手写优化的设备，看「[从旧版升级](#从旧版升级清理已移除的组件)」。

### 电脑上：编译环境和 ssh

脚本在你的电脑上把程序编好，再经 ssh 装到设备上。脚本是 POSIX sh，在 Linux 上开发和验证；其它系统没试过。

| 需要什么 | 用在哪 | 缺了会怎样 |
|---|---|---|
| Rust（`cargo`）+ `rustup target add aarch64-unknown-linux-musl` + `aarch64-linux-gnu-gcc` | 交叉编译 8 个网页服务（`shelf/build.sh`，会先跑一遍电脑上的测试）；编译 `hl-snap` / `ui-font` 插件 | 服务编不出来：`shelf` 步失败。插件编不出来：退回仓库里已提交的 `.so` 并提示 |
| **能免密 ssh 登录设备 root** | 所有步骤（脚本不会停下来问密码） | 动手前就报错并给排查步骤。没配过先跑 `ssh-copy-id root@10.11.99.1` |
| git | 取代码 | — |

不需要 xovi 的源码：插件的 xovi 胶水代码已提交进仓库，只有改了插件的 `.xovi` 描述文件才要 clone [asivery/xovi](https://github.com/asivery/xovi) 并用 `XOVI_DIR` 指向它重新生成。

### 连接设备

| 方式 | 设备地址 | 说明 |
|---|---|---|
| USB 线（推荐，第一次装用这个） | `10.11.99.1`（脚本缺省值） | 电脑上会多出一块 USB 网卡。网卡在但连不上、IP 不对时：`sudo ip addr add 10.11.99.2/24 dev <网卡名>` |
| 同一 WiFi | 设备的 IP，或 `shelf.local`（装过一次书架之后才有） | 需要设备允许经 WiFi 的 ssh（本文没有核实具体开关）。安卓设备不认 `.local`，要用 IP |

装的时候保持设备唤醒、别拔线。脚本会在设备上拿一把 20 分钟自动释放的唤醒锁，防止两步之间自动休眠；真断了也不会留下半截状态（见常见问题 7）。

## 安装

### 一条命令装好

1. **确认固件版本**：设备设置里看系统版本，目前只有 3.28.0.172 验证过。
2. **取代码**：
   ```sh
   git clone https://github.com/bbq191/cang-jie.git
   cd cang-jie
   ```
3. **（建议）先单独编译一遍**：第一次要编很久，先编好能在动设备之前发现编译问题。之后安装时 `shelf` 步还会再调一次，已编过的部分不会重编。
   ```sh
   sh shelf/build.sh
   ```
4. **预演**（只在电脑上跑，不连设备）：
   ```sh
   sh packaging/install-all.sh --dry-run      # 只打印将执行哪些步骤；可以加 --skip 看跳过后的样子
   ```
   预演**不**核对固件、**不**检查设备，只证明命令行写对了。
5. **正式安装**（USB 连着）：
   ```sh
   sh packaging/install-all.sh 10.11.99.1
   ```
   脚本先检查 ssh、固件、设备状态（见下一节），再逐步执行，最后整机重启一次、等设备回来自动核对。
6. **看收尾汇总**（见「收尾汇总怎么看」），然后**登录网页、改密码、装证书**（见「装完之后」）。

### 动手前的自动检查

`install-all.sh`（`--dry-run` 除外）先做下面三件事，任何一件不过就**一个步骤都不执行**，设备上什么都没改：

1. **ssh 能不能通**：连不上就报错并给排查步骤（设备休眠或没插 USB → IP 不对 → 设备的 host key 变了 → 没配免密）。
2. **设备预检**（只读）：必须是 root、`/home` 可写；`/home` 剩余空间**不到 50MB 拒装、不到 200MB 警告**；再报告 xovi、qt-resource-rebuilder 装没装、dm-verity 开没开、xovi 是否已在 xochitl 里生效。缺什么只是提前告诉你，对应步骤自己会报错或跳过。
3. **固件安全门**：读设备上 `/usr/bin/xochitl` 的 sha256，对照仓库里的 `packaging/firmware-allowlist.txt` 和你电脑上的 `firmware-allowlist.local.txt`，对得上才继续，否则拒装。用哈希而不用版本号，是因为界面补丁按字节定位，同一个版本号的热修也可能挪动内部布局。确认这台设备的固件就是你要装的、只是哈希没登记，加 `--force`：当前哈希会追加到**你电脑上的** `firmware-allowlist.local.txt`（不进 git），以后同一固件不用再加。

### 每一步装了什么

表里的**步骤名**可以用在 `--skip` 里；对应脚本 `packaging/deploy-<步骤名>.sh <设备>`（`shelf` 步是 `deploy.sh`）也能单独运行。

| 步骤名 | 做什么 | 前置 |
|---|---|---|
| `chrony-cn` | 校时服务器换成国内能连上的（阿里云、腾讯云等）。配置写好就算成功；当时没联网只给提示，联网后会自己校时 | — |
| `chrony-boot-wakelock` | 开机头一小段（同步成功就放，最多 120 秒）不让设备自动休眠，免得打断第一次校时 | — |
| `timezone-cn` | 默认时区设为 Asia/Shanghai | — |
| `wifi-watch` | WiFi 假死看护：检测到断链自动重连；连上的 AP 在设备不许用的 5G 信道（5150–5350 MHz）时才锁 2.4G；打开 WiFi 省电（09-28 起，一次对照实测空闲电流约降 37%）。可在设备 `~/.config/wifi-watch.conf` 里改（`BAND=`、`POWERSAVE=`） | — |
| `xovi-persist` | 开机后自动让 xovi 重新生效，重启设备后不用手动补 | xovi |
| `hl-snap` | 荧光笔划中文"划哪吸哪"，不再"划一小段吸整行"；只落盘 | xovi |
| `ui-font` | 界面字体插件：xochitl 的界面（书库、设置、对话框、标题）用你在网页上选的字体，阅读器字体不变（2026-10-07 起）；只落盘 | xovi |
| `shelf` | 8 个网页服务：网关、书（book）、字体与壁纸（font / wallpaper）、笔记四服务（ink / transcribe / mind / note）；附带 7 个界面补丁（字体菜单、界面字体令牌、回收站代理、建文件夹代理、漫画页边距代理、阅读位置代理、阅读器单击翻页）只落盘 | 补丁需要 qt-resource-rebuilder，缺了只跳过补丁、服务照装 |
| `清理已移除:battop`、`清理已移除:handwriting-stroke` | 不是安装步骤：清掉旧设备上电池刺客、手写优化的残留，没有残留就什么都不动（见「[从旧版升级](#从旧版升级清理已移除的组件)」）。`--skip battop` / `--skip handwriting-stroke` 可跳过 | — |
| `xovi-apply` | 上面"只落盘"的东西都就位后，**有改动（或 xovi 还没生效）才整机重启一次**（约 20–60 秒，会打断阅读；没改动就不重启）。重启回来后自动跑 `verify-on-device.sh` 核对。**前面有步骤失败时不执行** | — |

推文件时脚本会核对 md5：设备上已经是同一份的文件不再重传（书架整包也一样），传坏了就删掉暂存、不安装，设备上原来的东西不动。

### 收尾汇总怎么看

最后会列出五栏：

| 栏 | 意思 | 要做什么 |
|---|---|---|
| 已安装 | 这一步成功 | — |
| 已跳过（--skip） | 你在命令行点名跳过的 | — |
| 已跳过（前置条件不满足，非失败） | 例如 dm-verity 开着、装不进 `/usr`；或前面有步骤失败，所以没整机重启 | 看后面写的原因。"跳过"不等于"失败"，容易漏看（见常见问题 1） |
| 失败 | 这一步报错了 | 往上翻这一步的原始报错。不会自动重试 |
| 未执行（设备连不上） | 中途断线后没再执行的步骤 | 设备连回来后重跑同一条命令 |

有失败项时，已经装好的不受影响；修好问题后**整条重跑**即可，脚本全部可以重复执行，内容没变的不会再写、也不会再重启设备。

## 装完之后

**先看核对结果**：最后一步整机重启后，脚本等设备回来并自动跑 `verify-on-device.sh`（`CJ_APPLY_VERIFY=0` 可关）。它只读检查设备，分 9 节（固件与开机、xochitl 与扩展、界面补丁、常驻服务、本次开机告警、飞行记录仪、端口、`/usr` 单元、磁盘），逐项给 ✓/⚠/✗，有 ✗ 时退出码非 0。全套装好大约 40 项：2026-10-09 真机是 39✓ 1⚠ 0✗，那个 ⚠ 是"开机不到 10 分钟"，刚重启完出现属正常。没触发重启、或以后单独部署某一步之后，可以自己跑：

```sh
sh packaging/verify-on-device.sh 10.11.99.1
```

设备上还留着已移除组件时它会报 ⚠ 并给出清理命令。每项含义见 [`packaging/README.md`「部署后核对」](../packaging/README.md#部署后核对verify-on-devicesh2026-09-25)。

**然后打开网页**：浏览器打开 `https://10.11.99.1/`（同一 WiFi 下也可以用 `https://shelf.local/`；安卓不认 `.local`，要用设备的 IP）。

- **改密码**：默认密码 `shelf`，第一次登录会强制跳到改密页。
- **登录限速**：同一个 IP 在 60 秒内输错 5 次会被暂时锁住；换一台设备（不同 IP）不受影响。
- **装证书**：浏览器会提示证书不受信任，因为它是设备自己签的。登录页有「下载 CA 证书」（地址 `https://<设备>/ca.crt`），装进手机或电脑的信任列表一次，以后就不提示了。iOS 装完还要在"设置 → 通用 → 关于本机 → 证书信任设置"里打开完全信任。临时用也可以点「高级 → 继续访问」。
- **从 2026-09-24 之前的版本升级的**：网关第一次启动会自动换一张新 CA（新 CA 只能给局域网名字和内网 IP 签证书；旧文件改名为 `*.bak-<时间>` 留在设备上，不删）。**每台手机和电脑都要重装一次新证书，并删掉旧的那张**——旧 CA 没有这层限制，只装新的不删旧的，风险还在。确认新证书能用后，可以删掉设备 `~/.config/shelf/tls/` 里的 `*.bak-*`。
  - 已知限制：用不在范围内的地址访问（例如运营商分配的 100.64.x.x、公网 IP，或你自己改过的 mDNS 名字），证书会对不上、浏览器报错。

## 重复运行：什么时候才重启

重跑 `install-all.sh` 是安全的，而且不会每次都闪屏。只有真的写入了新内容时，脚本才在设备上留一个"待生效标记"（放在内存盘 `/run/cangjie-pending-apply/`，设备重启即清）；最后一步看标记决定要不要重启。

| 情形 | 最后一步 `xovi-apply` 怎么做 |
|---|---|
| 这轮有内容变了（首次安装、更新了插件或界面补丁） | **整机重启一次**（先打印"将打断阅读"，等 5 秒；约 20–60 秒回来，回来后自动核对） |
| 更新了插件 `.so`，而 xochitl 正在用旧版 | 新版先放进待换入区，重启前换进去（换入到排上重启这一段，电脑断线或按 Ctrl-C 也会做完） |
| 什么都没变，xovi 已生效 | 不重启 |
| 什么都没变，但设备刚重启过、xovi 还没生效 | 装了 xovi 持久化：整机重启（开机时自动恢复）；没装：执行 `xovi/start` |
| 待换入区里有新版插件，设备先被你手动重启了 | 开机时 `xovi-reenable` 会先把它换进去，不用再跑什么 |
| 前面有步骤失败 | **不重启**。已落盘的改动留着，修好后重跑时一起生效 |
| 加了 `--force-apply` | 无论如何都整机重启一次 |
| 用 `--skip xovi-apply` 跳过了 | 标记还在，方便时补一句 `sh packaging/deploy-xovi-apply.sh <设备>` 即可 |

![让插件/界面补丁生效：换入后整机重启](diagrams/so-swap-order.svg)

**单独跑某一步也一样**：`deploy-hl-snap.sh`、`deploy-ui-font.sh` 单独运行时，有改动就整机重启一次；文件和设备上已装的逐字节相同、也没有别的待生效改动、xovi 已生效，就**不重启**。

### 为什么一律整机重启

2026-09-25 起，让改动生效不再单独重启 xochitl，而是整机重启：**停止 xochitl 本身就有一定概率在退出途中崩溃**，崩了系统会走应急路径被动整机重启（真机上多次出现，与本项目的插件无关，是 xochitl 退出时的析构顺序问题）。与其冒这个险，不如直接干净地重启。代价是每次多等 20–60 秒。所以你自己手动让改动生效时，也请直接 `reboot`，**不要** `systemctl restart xochitl`；**绝不**在 xovi 已生效时手动跑 `xovi/start`（2026-09-20 真机上让 xochitl 崩溃过）。

## 常用选项

### 命令速查

在仓库根目录执行；`<设备>` 缺省 `10.11.99.1`。每个脚本都能 `-h` 看用法；参数写错一律退出码 2，不会去连设备。全部参数与环境变量见 [`packaging/README.md`](../packaging/README.md#参数与环境变量)。

| 命令 | 参数 | 作用 |
|---|---|---|
| `sh packaging/install-all.sh [设备]` | `--dry-run` | 只打印计划，不连设备 |
| | `--skip a,b` | 跳过指定步骤（写错名字只警告，并列出已知步骤名） |
| | `--force` | 固件不在白名单也装（见「动手前的自动检查」） |
| | `--force-apply` | 最后一步无论有没有改动都整机重启一次 |
| `sh packaging/uninstall-all.sh [设备]` | `--dry-run` / `--skip a,b` / `--purge` | 见「卸载」 |
| `sh packaging/deploy.sh [设备]` | `--only a,b` · `--password 新密码` · `--no-systemd` | 只装或更新网页服务（见下） |
| `sh packaging/deploy-xovi-apply.sh [设备]` | `--force` | 单独让"只落盘"的内容生效 |
| 其余 `sh packaging/deploy-<步骤名>.sh [设备]` | `-h` | 单独跑某一步 |
| `sh packaging/verify-on-device.sh [设备]` | `--json` · `--dump` · `--from 文件` | 只读核对设备现状 |

### 只装一部分

```sh
sh packaging/install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist    # 跳过指定步骤
sh packaging/deploy.sh 10.11.99.1 --only book,font --password '新密码'                # 只装书和字体两个服务，顺便设网关密码
```

`--only` 可以写的名字：`gateway book font wallpaper ink transcribe mind note`。网关总会装；写了别的名字会报错退出。`--password` 经 ssh 标准输入传到设备上的临时文件，用完就删，不会出现在命令行和进程列表里。`deploy.sh` 推送前会核对要装的程序都编好了，缺了直接报错，提示先跑 `sh shelf/build.sh`；设了 `SHELF_NO_BUILD=1` 时它不自己编译、直接用现成的产物。用 `--only` 只装一部分时，设备上的卸载程序 `shelf-uninstall` 和它的清单也会一起刷新。

## 卸载

`uninstall-all.sh` 和安装用同一张步骤表，按**相反顺序**执行（后装的先卸），最后再清已移除组件的残留。建议先预演：

```sh
sh packaging/uninstall-all.sh 10.11.99.1 --dry-run          # 只打印计划，不连设备、不删东西
sh packaging/uninstall-all.sh 10.11.99.1                    # 卸全部
sh packaging/uninstall-all.sh 10.11.99.1 --skip shelf       # 跳过某步
```

**会做什么**：停用并删掉装过的服务、插件、界面补丁，以及部署时推到设备上的安装包目录（只删认识的文件，目录里有别的东西就留着）。卸插件时连待换入区里还没换进去的新版也一并撤掉。

**默认保留**：母版库、配置、证书、字体/壁纸池、`cangjie-backups/` 里的备份。`--purge` 目前不影响任何步骤，也不碰书架数据；要连书架数据一起删，先 `--skip shelf` 卸别的，再在设备上跑 `shelf-uninstall --purge`。

**不会做什么**：

- `chrony-cn`、`timezone-cn` 是改配置、`xovi-apply` 只是个动作，都不卸。改之前的备份在设备 `cangjie-backups/` 里，要还原自己取。
- vellum、xovi、qt-resource-rebuilder 不是本项目装的，也不卸。
- 卸载**不重启**设备。已经加载的插件和界面补丁要等下次重启才真正停用。想马上停：在设备上 `reboot`（不要 `systemctl restart xochitl`，理由见「为什么一律整机重启」）。

**dm-verity 开着时**：`/usr` 下的服务单元删不掉（脚本遇到 verity 一律不写 `/usr`）。这时卸载脚本会如实提示，并**保留**这些单元要用的程序，免得重启后单元找不到程序、反复失败。等设备可写后再跑一次 `uninstall-all.sh` 就能收尾。

中途断线时，后面的步骤记"未执行"，设备连回来后重跑同一条命令即可。

## 从旧版升级：清理已移除的组件

有些功能后来被砍了。它们的源码和安装步骤已经删掉，但以前装过的设备上还留着文件。

| 组件 | 何时移除 | 设备上可能留下什么 | 怎么清 |
|---|---|---|---|
| 电池刺客（`battop`，耗电诊断服务） | 2026-09-30 | `/usr` 里的 `battop.service`（更旧的版本还有 `battop.timer`）、`/home/root/battop/` 整个目录 | **重跑 `install-all.sh` 自动清** |
| 手写优化（`handwriting-stroke`，插件 `hw-stroke.so`） | 2026-09-30 | `extensions.d/hw-stroke.so`、待换入区里的副本、安装包目录 `/home/root/hw-stroke/` | **重跑 `install-all.sh` 自动清** |
| 书架的 `koreader-serve` 服务 | 2026-09-29 | 服务单元和程序 | 重跑 `install-all.sh`（或 `deploy.sh`）时顺手清 |
| 侧栏 KOReader 入口（`sidebar-entry`） | 2026-09-29 | 界面补丁 `koreader-sidebar-entry.qmd` 和图标包 `cangjie-icons.rcc` | **不自动清**，要手动跑一次 `uninstall-all.sh`（命令见下） |

![install-all 如何清理已移除的组件](diagrams/retired-cleanup.svg)

**自动清理怎么做**：`install-all.sh` 在最后一步 `xovi-apply` 之前，对电池刺客、手写优化各跑一次清理（与 `uninstall-all.sh` 用同一份函数）。设备上没有残留就什么都不动；如果 xochitl 当时还加载着 `hw-stroke.so`，会记一个"待生效"标记，最后一步因此**整机重启一次**让它彻底停用。`reading-qol.json` 里旧的 `hwStroke*` 设置项会原样留着，没有程序再读，无害。

侧栏入口不自动清，是因为它的图标包文件名 `cangjie-icons.rcc` 历史上别的界面补丁也用过，只在你明确要求时才删。

**不重装、只清某几样**：用 `uninstall-all.sh` 跳过其它所有步骤。先 `--dry-run` 看计划，计划里应只剩你要清的那几步：

```sh
# 只清电池刺客与手写优化（计划里应只有 handwriting-stroke、battop 两步）
sh packaging/uninstall-all.sh 10.11.99.1 --dry-run --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,sidebar-entry
sh packaging/uninstall-all.sh 10.11.99.1 --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,sidebar-entry
# 只清侧栏入口
sh packaging/uninstall-all.sh 10.11.99.1 --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,battop,handwriting-stroke
```

跑完在设备上整机重启一次（`reboot`）。`verify-on-device.sh` 看到这些残留会报 ⚠（侧栏入口补丁还在而 appload 已卸则报 ✗），给的清理命令就是上面这几条。

**真机情况**：2026-09-30 15:23 在真机上跑 `install-all.sh`，自动清掉了电池刺客（单元与 `/home/root/battop`）和 `hw-stroke.so`，只整机重启一次，之后核对 36✓ 1⚠（刚开机）0✗。上面"只清某几样"的 `uninstall-all.sh` 命令只在本机模拟测过。

## 固件升级（OTA）之后

这是 OTA 恢复的**权威说明**，其它文档都链接到这里。

![OTA 之后：哪些丢了、怎么恢复](diagrams/ota-recovery.svg)

**升级本身不会丢 `/home` 的数据，但升完要重跑一遍安装，功能才回来。** 本项目有意不在开机路径上留任何东西（xovi 的加载配置在 `/etc` 的内存层、服务单元在 `/usr`），所以新固件总是以纯原厂状态起来。

### 推荐流程

1. （升级前，可选）把与新固件不兼容的 xovi 插件（例如某个第三方插件的旧版）挪出 `extensions.d/`，放到 `/home/root/xovi-disabled/`。**绝不留在 `extensions.d/` 里**：xovi 会把那个目录下任何文件都当插件加载。
2. 升级完成后，**在设备旁手动**跑 `xovi/rebuild_hashtable`（要输 root 密码，脚本不代做）。它按新固件重新建一张界面资源的索引表，界面补丁靠这张表定位，所以它是补丁重新生效的前提。
3. 在电脑上：`sh packaging/install-all.sh <设备>`。新固件的哈希一般不在白名单里，确认版本无误后加 `--force`。OTA 后 xovi 没有生效，所以最后一步会整机重启一次，开机时由刚装回的 `xovi-reenable` 恢复 xovi，回来后自动核对。
4. 看收尾汇总，浏览器打开网关确认。

### 逐项对照

| 内容 | 位置 | OTA 后 | 怎么恢复 |
|---|---|---|---|
| 母版库、字体与壁纸池、证书、网关密码、休眠屏设置、`cangjie-backups/` | `/home` | 保留 | 不用管 |
| 各网页服务的程序（`~/.local/bin`） | `/home` | 保留 | 不用管 |
| `hl-snap`、`ui-font` 插件，各界面补丁 | `/home`（`extensions.d/`、`exthome/`） | 文件还在，但要重建 hashtable 才生效 | 第 2 步，再跑 `install-all.sh` |
| 各网页服务与 `shelf.target` 的服务单元 | `/usr` | **被冲掉** | `shelf` 步（或单独：`SHELF_NO_BUILD=1 sh packaging/deploy.sh <设备>`） |
| `xovi-reenable.service`（开机自动让 xovi 生效） | `/usr` | **被冲掉** | `xovi-persist` 步 |
| `chrony-boot-wakelock.service` | `/usr` | **被冲掉** | `chrony-boot-wakelock` 步 |
| `wifi-watch.service`（脚本在 `/home`） | `/usr` | 单元**被冲掉** | `wifi-watch` 步 |
| 国内校时服务器、默认时区 | `/etc` | **被冲掉** | `chrony-cn` / `timezone-cn` 步 |

**风险分层**：书架这层只用 xochitl 的网页上传接口和系统标准组件，换固件重装就回来；字体菜单这类界面补丁依赖 xochitl 内部 QML，大版本升级常要重新适配。

**"裸机恢复"要多查一步**：OTA 本身不删 `/home`，但如果设备做过更彻底的重置，`/home` 下的插件和程序可能也没了（2026-09-09 真机踩过）。重跑 `install-all.sh` 前先确认它们还在。

## 常见问题

下面都是有明确触发条件的已知坑，不是随机故障。

| # | 现象 | 原因 | 怎么办 |
|---|---|---|---|
| 1 | 字体菜单、回收站/新建文件夹、漫画页边距、找回阅读位置、阅读器单击翻页这几个**同时**没有（界面字体也只换了一部分） | 它们共用前置 qt-resource-rebuilder。没装时它们是 `shelf` 步里附带的补丁，**不单列**——`shelf` 仍算"已安装"，只在这一步的输出里有一行"无 qt-resource-rebuilder 目录…跳过…qmd" | `vellum add qt-resource-rebuilder` 后重跑 `install-all.sh` |
| 2 | 短时间内 xochitl 反复停起后，设备整机重启了一次 | xochitl 服务设置了 10 分钟内最多重启 4 次，不管谁触发的都算：手动 `systemctl restart xochitl`、`vellum add/del` 装卸 xochitl 插件。2026-09-11 真机上连续两次重启就触发过一次整机重启——**设备自己重启后恢复正常，不是变砖** | 部署脚本 2026-09-25 起改为整机重启，不再计入这个次数。手动装卸插件时每次间隔几分钟 |
| 3 | 装之前就报错退出：`连不上 root@…` / `只剩 N MB 可用` / `需要 root` / `固件不在白名单` | 动手前的自动检查在拦，设备上什么都没改 | 连不上：按报错里的步骤排查（休眠/没插 USB → IP → host key → 免密）；空间不足：清理 `/home/root` 和 `cangjie-backups/` 后重试；固件：先确认设备固件就是验证过的那份，再 `--force` |
| 4 | 装到最后设备重启了一次 | `xovi-apply` 让改动生效：一律**整机重启**（约 20–60 秒回来）。只有这轮真的有改动、或 xovi 还没生效时才会重启 | 正常现象，装的时候别操作设备；脚本会等设备回来并自动核对。不想被打断就 `--skip xovi-apply`，稍后再跑 `sh packaging/deploy-xovi-apply.sh <设备>` |
| 5 | 最后一步报"设备没能排上整机重启……改动尚未生效"，这一步记失败 | 设备上的 `systemctl reboot` 命令本身失败了。文件已经换好，但 xochitl 还在用旧的；脚本已把"待生效"标记补回去。这一支只在本机模拟过 | 在设备上手动 `reboot`，回来后跑 `sh packaging/verify-on-device.sh <设备>`；或者稍后重跑 `sh packaging/deploy-xovi-apply.sh <设备>` |
| 6 | 最后一步报"xochitl 里没有 xovi，设备上也没有 xovi/start"，这一步记失败 | 设备上没装 xovi（或被 vellum 删了）：整机重启也没法让插件和界面补丁生效，所以脚本直接报错、不重启（2026-10-09 起）。只在本机模拟过 | 在设备上 `vellum add xovi`，再重跑 `install-all.sh` |
| 7 | 装到一半报 ssh 断开（如 `Timeout, server … not responding`），汇总里有"未执行" | 设备休眠、拔了线或 WiFi 掉了。ssh 约 15 秒判定断线；脚本再探一次连不上，就不再执行后面的步骤。发到设备上的脚本要整份收到才执行，断在半截的那一步一条都没执行 | 唤醒设备、确认连接后整条重跑 `install-all.sh`（已装好的不会重复动） |
| 8 | 汇总里 `xovi-apply` 在"前置条件不满足"栏，说"前面有步骤失败" | 前面某一步失败，脚本这轮不整机重启（2026-10-10 起）。已落盘的改动留着待生效 | 按那一步的报错修好，再整条重跑 `install-all.sh`，会顺带生效；或现在就 `sh packaging/deploy-xovi-apply.sh <设备>` |

### 其它排障

- 先看收尾汇总，定位哪一步失败；对应 `packaging/deploy-*.sh` 的开头注释写了这一步做什么、常见失败原因。
- 网页「管理」页能直接看到插件是否真的加载进了 xochitl（"已加载 / 未加载"）。开关开着但显示"未加载"，说明插件没装上或装完还没整机重启。「管理 → 设备健康」能看到更全的状态（各服务、扩展、上次开机日志）。
- 不碰真机就想确认脚本没被改坏：`bash packaging/tests/run_sim_tests.sh`（本机模拟，448 项断言，2026-10-10 实跑全过）。它代替不了真机验证。

## 风险提示与已知限制

- **写 `/usr` 是一个例外，不是"完全不碰 `/usr`"**：几个开机服务单元（书架各服务、`xovi-reenable`、`chrony-boot-wakelock`、`wifi-watch`）必须放在 `/usr`。脚本写之前先查 dm-verity（开着就不写），再在一个限时的读写窗口里写，写完无论成败都恢复只读。历史上写 `/usr` 触发过回滚变砖（2026-08-16），所以除了这几个单元，其它东西一律放 `/home`。
- **每次生效都整机重启**，打断约 20–60 秒（原因见「为什么一律整机重启」）。
- **备份**：覆盖设备上已有文件前先备份到 `/home/root/cangjie-backups/`（**绝不**放 `extensions.d/`），只留最近 5 份，内容没变就不备份也不动。细节见 [`packaging/README.md`](../packaging/README.md#备份与幂等)。
- **不自动回滚**：某一步失败时停在"可以重跑"的状态，修好后重跑即可；回滚本身要再动 `/usr` 和 `extensions.d`，风险比失败更大。
- **在设备上直接跑 `shelf/install.sh --password 明文` 时，密码会短暂出现在设备的进程列表里**；经电脑上的 `deploy.sh --password` 传则不会。
- 卸载不还原 `chrony-cn` / `timezone-cn`，没有"一键回到装之前"。

## 验证现状（摘要）

完整记录在 [`packaging/README.md`「验证现状」](../packaging/README.md#验证现状如实说明不夸大)。上机时一步一确认：先 `--dry-run`，再单步或 `--skip` 试跑。

| 范围 | 状态 |
|---|---|
| 整轮 `install-all.sh` | **真机跑过**：2026-09-22、09-24、09-25、09-30、10-09（两次，WiFi 与 USB 各一次），10-09 核对 38✓ 1⚠、39✓ 1⚠（⚠ 都是刚开机）0✗ |
| 整轮 `uninstall-all.sh` | **真机跑过**：2026-09-25，8 步逆序全部成功，书架/笔记数据与配置保留，卸载期间 xochitl 没有重启 |
| "换入新版 `.so` → 整机重启 → 开机自动恢复 → 核对" | **真机跑过**：2026-09-25（43✓） |
| 自动清理已移除组件 | **真机跑过**：2026-09-30 15:23，只整机重启一次，36✓ 1⚠ 0✗ |
| 2026-10-10 两轮加固（断线即停、有失败不重启、载荷不重传、往返合并、设备端中断后收敛等） | **只有本机模拟**，还没上真机 |
| 断线判定、断在半截不执行、没有 xovi 时报错不重启、`systemctl reboot` 失败的处理、dm-verity 下保留程序、`--purge`、只清已移除组件的 `uninstall-all` | **只有本机模拟** |

注意：这些只证明"装上了、服务健康"，书架和笔记的各项功能是否逐项手测过，见各自的文档。

## 这套安装器不做什么

- **不装 vellum / xovi / qt-resource-rebuilder**：见「装之前」。
- **不装中文输入法**：那条功能线的源码已移出本仓库（见 [README](../README.md#历史与范围)），不随本安装器分发。
