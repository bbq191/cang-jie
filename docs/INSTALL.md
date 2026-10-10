# 安装部署指南

**[English](INSTALL.en.md)** · 返回 [README](../README.md)

> **这份文档给谁**：第一次给 reMarkable Paper Pro Move 装这套增强的人、想卸载的人、固件升级（OTA）后要恢复功能的人。
> - **第一次装**：按顺序读「适用范围 → 装之前 → 安装 → 装完之后」。
> - **以前装过、现在更新**：直接重跑 `install-all.sh`（见「安装」）；已移除的组件会被自动清掉，见「从旧版升级：清理已移除的组件」。
> - **卸载**：读「卸载」。**升级了固件**：读「固件升级（OTA）之后」。**出问题**：查「常见问题」。
>
> 想先了解这套东西是什么，读 [`OVERVIEW.md`](OVERVIEW.md)。脚本内部怎么写、全部参数与环境变量、每项核对的含义、怎么本机测试，在 [`../packaging/README.md`](../packaging/README.md)（开发者向，本文不重复）。

**本文会反复出现的几个词**：

| 词 | 意思 |
|---|---|
| xochitl | 设备自带的阅读/笔记主程序。本项目不改它，只在它旁边加东西 |
| xovi / 扩展 | 第三方的扩展加载框架，xochitl 启动时把 `extensions.d/` 里的 `.so` 插件一起加载。本项目现在有两个扩展：`hl-snap`（荧光笔吸附）和 `ui-font`（界面字体） |
| qmd / 界面补丁 | 对 xochitl 界面描述文件（QML）的补丁，由 qt-resource-rebuilder 在 xochitl 启动时打上 |
| 只落盘 / 生效 | 扩展和界面补丁只在 xochitl 启动时读，所以文件放到位（落盘）后还要**整机重启一次**才生效 |
| OTA | 固件在线升级。会整体替换系统分区 `/usr`、`/etc`，不动放数据的 `/home` |
| dm-verity | 系统分区的只读校验。开着时脚本一律不写 `/usr` |

## 适用范围

**reMarkable Paper Pro Move（imx93-chiappa），固件 3.28.0.172**。这是目前唯一在真机上验证过的版本，
安装脚本动手前会自动核对（见「固件安全门」）。其它固件版本、其它 reMarkable 型号都没验证过；强装的话，
界面补丁可能定位错，轻则某个功能不生效，重则影响设备正常使用。

## 装之前

### 设备上：2 样东西要你手动装好

它们属于 reMarkable 第三方生态，不属于这个仓库，`install-all.sh` **不会**代装。缺了的话，对应步骤会报错或跳过，
并告诉你该跑哪条命令。vellum（设备上的包管理器）本身怎么装，看 vellum 自己的文档。

| # | 在设备上做什么 | 它是什么 | 缺了会怎样 |
|---|---|---|---|
| 1 | `vellum add xovi` | [xovi](https://github.com/asivery/xovi)：扩展加载框架 | 插件类功能都靠它，相关步骤直接失败 |
| 2 | `vellum add qt-resource-rebuilder` | 界面补丁（qmd）加载器 | 界面补丁全部不装（不算失败）：字体菜单、界面字体令牌、回收站/新建文件夹代理、漫画页边距代理、阅读位置代理、阅读器单击翻页。其余不受影响（汇总里怎么显示见问题②） |

2026-09-29 起**不再需要 appload 和 KOReader**（设备只用自带阅读器）。以前装过它们、或装过电池刺客和手写优化的设备，看「[从旧版升级：清理已移除的组件](#从旧版升级清理已移除的组件)」。

### 电脑上：编译环境和 ssh

脚本在你的电脑上把程序编好，再经 ssh 装到设备上，所以电脑需要：

| 需要什么 | 用在哪 | 缺了会怎样 |
|---|---|---|
| Rust（`cargo`）+ `rustup target add aarch64-unknown-linux-musl` + `aarch64-linux-gnu-gcc` | 交叉编译八个网页服务（`shelf/build.sh`） | `shelf` 步失败 |
| 可选：[asivery/xovi](https://github.com/asivery/xovi) 的源码 clone（`XOVI_DIR` 指向它） | 重新编译 `hl-snap` / `ui-font` 插件 | 不影响：仓库里已提交编好的 `.so`，编不了就用它 |
| **能免密 ssh 登录设备 root** | 所有步骤（脚本不会停下来问密码） | 动手前就报错并给排查步骤。没配过先跑 `ssh-copy-id root@10.11.99.1` |

## 安装

![install-all.sh 流程](diagrams/install-flow.svg)

### 推荐顺序

1. **确认固件版本**：设置里看系统版本，目前只有 3.28.0.172 验证过。
2. **按顺序手动装好设备上那 2 样**（xovi → qt-resource-rebuilder）。装完要让它们生效，直接整机重启设备（`reboot`），别 `systemctl restart xochitl`（见问题③）。
3. **先预演**（只在电脑上跑，不连设备）：
   ```sh
   git clone https://github.com/bbq191/cang-jie.git
   cd cang-jie/packaging
   sh install-all.sh --dry-run          # 只打印将执行哪些步骤；可以加 --skip 看跳过后的样子
   ```
   预演**不**核对固件、**不**检查设备，只证明命令行写对了。
4. **正式安装**（USB 连电脑，设备地址默认 `10.11.99.1`）：
   ```sh
   sh install-all.sh 10.11.99.1
   ```
   脚本先确认 ssh 能通、固件在白名单里、设备状态正常（见「装前自动检查」），再按下表逐步执行。第一次会先编译，要等一会儿。
5. **看收尾汇总**：分四栏——「已安装」「已跳过（--skip）」「已跳过（前置条件不满足，非失败）」「失败」。第三栏会写明原因（例如 dm-verity 开着、装不进 `/usr`）；"跳过"不等于"失败"，容易漏看（见问题②）。有失败项就照报错处理，其余已经装好；整条重跑也安全（脚本全部幂等，内容没变就不会再重启设备）。
6. **登录网页、改密码、装证书**：见「装完之后」。

### 每一步装了什么

表里的**步骤名**可以用在 `--skip` 里；对应脚本 `packaging/deploy-<步骤名>.sh <host>`（`shelf` 步是 `deploy.sh`）也能单独运行。

| 步骤名 | 做什么 | 前置 |
|---|---|---|
| `chrony-cn` | 校时服务器换成国内能连上的（阿里云、腾讯云等）。配置写好就算成功；当时没联网、一时没同步上只给提示，联网后会自己校时 | — |
| `chrony-boot-wakelock` | 开机头一小段（同步成功就放，最多 120 秒）不让设备自动休眠，免得打断第一次校时 | — |
| `timezone-cn` | 默认时区设为 Asia/Shanghai | — |
| `wifi-watch` | WiFi 假死看护：检测到断链自动重连；连上的 AP 在设备不许用的 5G 信道（5150–5350 MHz）时才锁 2.4G；打开 WiFi 省电（09-28 起，实测空闲电流约降 37%）。可在设备 `~/.config/wifi-watch.conf` 里改（`BAND=`、`POWERSAVE=`） | — |
| `xovi-persist` | 开机后自动让 xovi 重新生效，重启设备后不用手动补 | xovi |
| `hl-snap` | 荧光笔划中文"划哪吸哪"，不再"划一小段吸整行"；只落盘 | xovi |
| `ui-font` | 界面字体插件：让 xochitl 的界面（书库、设置、对话框、标题）用你在网页上选的字体，阅读器字体不变（2026-10-07 起）；只落盘 | xovi |
| `清理已移除:battop`、`清理已移除:handwriting-stroke` | 不是安装步骤：清掉旧设备上电池刺客、手写优化的残留（2026-09-30 起这两样已移除，见「[从旧版升级](#从旧版升级清理已移除的组件)」），没有残留就什么都不动。排在 `xovi-apply` 之前；`--skip battop` / `--skip handwriting-stroke` 可跳过 | — |
| `shelf` | 八个网页服务：网关、书（book）、字体与壁纸（font / wallpaper）、笔记四服务（ink / transcribe / mind / note）；附带的七个界面补丁（字体菜单、界面字体令牌、回收站代理、建文件夹代理、漫画页边距代理、阅读位置代理〔原地替换后找回读到的地方，2026-10-09〕、阅读器单击翻页）只落盘 | 补丁需要 qt-resource-rebuilder，缺了只跳过补丁、服务照装 |
| `xovi-apply` | 上面"只落盘"的东西都就位后，**有改动（或 xovi 还没生效）才整机重启一次**让它们生效（约 20–60 秒，会打断阅读；没改动就不重启）。重启回来后自动跑一遍 `verify-on-device.sh` 核对 | — |

"只落盘"的意思是：文件先放到位，暂不生效，最后由 `xovi-apply` 统一整机重启一次。这样避免短时间内反复重启。
失败的步骤不会自动重试，也不会被悄悄跳过。

## 装完之后

**先看核对结果**：最后一步整机重启后，脚本会等设备回来并自动跑 `verify-on-device.sh`（`CJ_APPLY_VERIFY=0` 可关）。它只读检查设备，分 9 节（固件与开机、xochitl 与扩展、界面补丁、常驻服务、本次开机告警、飞行记录仪、端口、`/usr` 单元、磁盘），逐项给 ✓/⚠/✗，有 ✗ 时退出码非 0。全套装好的设备大约 40 项：2026-10-09 加了阅读位置代理后真机是 39✓ 1⚠ 0✗，那个 ⚠ 是"开机不到 10 分钟"，刚重启完出现属正常。没有触发重启、或以后单独部署某一步之后，可以自己在 `packaging/` 下跑 `sh verify-on-device.sh <host>`。设备上还留着已移除组件时它会报 ⚠ 并给出清理命令（见「[从旧版升级](#从旧版升级清理已移除的组件)」）。每项含义见 [`packaging/README.md`「部署后核对」](../packaging/README.md#部署后核对verify-on-devicesh2026-09-25)。

浏览器打开 `https://10.11.99.1/`（同一 WiFi 下也可以用 `https://shelf.local/`；安卓不认 `.local` 域名，要用设备的 IP）。

- **改密码**：默认密码 `shelf`，第一次登录会强制跳到改密页。
- **登录限速**：同一个 IP 在 60 秒内输错 5 次会被暂时锁住；换一台设备（不同 IP）不受影响。
- **装证书**：浏览器会提示证书不受信任，因为它是设备自己签的。登录页有「下载 CA 证书」（地址 `https://<设备>/ca.crt`），装进手机或电脑的信任列表一次，以后就不提示了。iOS 装完还要在"设置 → 通用 → 关于本机 → 证书信任设置"里打开完全信任。临时用也可以点「高级 → 继续访问」。
- **从 2026-09-24 之前的版本升级的**：网关第一次启动会自动换一张新 CA（新 CA 只能给局域网名字和内网 IP 签证书；旧文件改名为 `*.bak-<时间>` 留在设备上，不删）。**每台手机和电脑都要重装一次新证书，并删掉旧的那张**——旧 CA 没有这层限制，只装新的不删旧的，风险还在。确认新证书能用后，可以删掉设备 `~/.config/shelf/tls/` 里的 `*.bak-*`。
  - 已知限制：用不在范围内的地址访问（例如运营商分配的 100.64.x.x、公网 IP，或你自己改过的 mDNS 名字），证书会对不上、浏览器报错。

## 常用选项

### 命令速查

在 `packaging/` 目录下执行；`<host>` 缺省 `10.11.99.1`。每个脚本都能 `-h` 看用法；参数写错一律退出码 2，不会去连设备。全部参数与环境变量见 [`packaging/README.md`](../packaging/README.md#参数与环境变量)。

| 命令 | 参数 | 作用 |
|---|---|---|
| `sh install-all.sh [host]` | `--dry-run` | 只打印计划，不连设备 |
| | `--skip a,b` | 跳过指定步骤（写错名字只警告，并列出已知步骤名） |
| | `--force` | 固件不在白名单也装（见「固件安全门」） |
| | `--force-apply` | 最后一步无论有没有改动都整机重启一次 |
| `sh uninstall-all.sh [host]` | `--dry-run` / `--skip a,b` / `--purge` | 见「卸载」 |
| `sh deploy.sh [host]` | `--only a,b` · `--password 新密码` · `--no-systemd` | 只装或更新网页服务（见「只装一部分」） |
| `sh deploy-xovi-apply.sh [host]` | `--force` | 单独让"只落盘"的内容生效 |
| 其余 `deploy-<步骤名>.sh [host]` | `-h` | 单独跑某一步 |
| `sh verify-on-device.sh [host]` | `--json` · `--dump` · `--from 文件` | 只读核对设备现状（见「装完之后」） |

### 装前自动检查

`install-all.sh`（`--dry-run` 除外）动手前做三件事，任何一件不过就**一个步骤都不执行**，设备上什么都没改：

1. **ssh 能不能通**：连不上就报错并给排查步骤（设备休眠或没插 USB；IP 不对；设备的 host key 变了；没配免密）。整轮只查这一次，后面各步骤不再重复。
2. **固件安全门**：见下。
3. **设备预检**（只读）：必须是 root、`/home` 可写；`/home` 剩余空间**不到 50MB 拒装、不到 200MB 警告**；再报告 xovi、qt-resource-rebuilder 装没装，xovi 是否已在 xochitl 里生效。缺什么只是提前告诉你，对应步骤自己会报错或跳过。

检查通过后，整轮安装期间还有三道保护（2026-10-09 起；正常路径已随当天两次真机部署跑过，断线等异常情形只在本机模拟过）：设备持一把带超时的唤醒锁（20 分钟后自动释放，不会在两步之间睡过去；`CJ_AWAKE=0` 关掉）；ssh 断线约 15 秒就判定并报错，不会无限挂起；发到设备上的脚本要整份收到才执行，断在半截一条都不执行。

### 固件安全门

装之前脚本读设备上 `/usr/bin/xochitl` 的 sha256，对照仓库里的 `packaging/firmware-allowlist.txt` 和你电脑上的 `firmware-allowlist.local.txt`：对得上才继续，否则默认拒装。用哈希而不是版本号，是因为界面补丁按字节定位，同一个版本号的热修补丁也可能挪动内部布局。

确认这台设备的固件就是你要装的、只是哈希没登记，加 `--force`。当前哈希会追加到**你电脑上的** `firmware-allowlist.local.txt`（不进 git），以后同一固件不用再加。

```sh
sh install-all.sh 10.11.99.1 --force
```

### 重复运行：什么时候才重启

重跑 `install-all.sh` 是安全的，而且不会每次都闪屏。只有真的写入了新内容时，脚本才在设备上留一个"待生效标记"（放在内存盘 `/run/cangjie-pending-apply/`，设备重启即清）；最后一步看标记决定要不要重启。

| 情形 | 最后一步 `xovi-apply` 怎么做 |
|---|---|
| 这轮有内容变了（首次安装、更新了插件或界面补丁） | **整机重启一次**（先打印"将打断阅读"，等 5 秒；约 20–60 秒回来，回来后自动核对）。2026-09-25 起不再单独重启 xochitl：它退出时有概率崩溃、再由系统整机重启，不如直接干净地重启 |
| 更新了插件 `.so`，而 xochitl 正在用旧版 | 新版先放进待换入区，重启前换进去（见下图；换入到排上重启这一段电脑断线或按 Ctrl-C 也会做完） |
| 什么都没变，xovi 已生效 | 不重启 |
| 什么都没变，但设备刚重启过、xovi 还没生效 | 装了 xovi 持久化：整机重启（开机时自动恢复）；没装：执行 `xovi/start` |
| 待换入区里有新版插件，设备先被你手动重启了 | 开机时 `xovi-reenable` 会先把它换进去，不用再跑什么 |
| 加了 `--force-apply` | 无论如何都整机重启一次 |
| 上一轮用 `--skip xovi-apply` 跳过了 | 标记还在，补一句 `sh deploy-xovi-apply.sh <host>` 即可 |

![让插件/界面补丁生效：换入后整机重启](diagrams/so-swap-order.svg)

**单独跑某一步也一样**：`deploy-hl-snap.sh`、`deploy-ui-font.sh` 单独运行时，有改动就整机重启一次；文件和设备上已装的逐字节相同、也没有别的待生效改动、xovi 已生效，就**不重启**。判据与最后一步 `xovi-apply` 是同一个。

### 只装一部分

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist    # 跳过指定步骤
sh deploy.sh 10.11.99.1 --only book,font --password '新密码'                # 只装书和字体两个服务，顺便设网关密码
```

`--only` 可以写的名字：`gateway book font wallpaper ink transcribe mind note`。网关总会装；写了别的名字会报错退出。
`--password` 经 ssh 标准输入传到设备上的临时文件，用完就删，不会出现在命令行和进程列表里（只有经 `deploy.sh` 传时才这样，见「已知限制」）。
`deploy.sh` 推送前会核对要装的程序都编好了；缺了会直接报错，提示先在仓库根目录跑 `sh shelf/build.sh`。
用 `--only` 只装一部分时，设备上的卸载程序 `shelf-uninstall` 和它的清单也会一起刷新（2026-09-30 起；以前只有整包装才刷，之后卸载可能漏删新加的界面补丁）。重复部署时内容没变的文件不会再上传一遍。

## 卸载

`uninstall-all.sh` 和安装用同一张步骤表，按**相反顺序**执行（后装的先卸），最后再清已移除组件的残留（见「[从旧版升级](#从旧版升级清理已移除的组件)」）。建议先预演：

```sh
cd packaging
sh uninstall-all.sh 10.11.99.1 --dry-run          # 只打印计划，不连设备、不删东西
sh uninstall-all.sh 10.11.99.1                    # 卸全部
sh uninstall-all.sh 10.11.99.1 --skip shelf       # 跳过某步
sh uninstall-all.sh 10.11.99.1 --purge            # 保留兼容，目前不影响任何步骤
```

**会做什么**：停用并删掉装过的服务、插件、界面补丁，以及部署时推到设备上的安装包目录（只删认识的文件，目录里有别的东西就留着）。卸插件时连待换入区里还没换进去的新版也一并撤掉（2026-09-24 之前不撤，下一次部署会把刚卸掉的插件又装回来）。

**默认保留**：母版库、配置、证书、字体/壁纸池、`cangjie-backups/` 里的备份。`--purge` 目前不影响任何步骤，也不碰书架数据；要连书架数据一起删，先 `--skip shelf` 卸别的，再在设备上跑 `shelf-uninstall --purge`。

**不会做什么**：
- `chrony-cn`、`timezone-cn` 是改配置、`xovi-apply` 只是个动作，都不卸。改之前的备份在设备 `cangjie-backups/` 里，要还原自己取。
- vellum、xovi、qt-resource-rebuilder 不是本项目装的，也不卸。
- 卸载**不重启**设备。已经加载的插件和界面补丁要等下次重启才真正停用。想马上停：在设备上 `reboot`（整机重启）。**别** `systemctl restart xochitl`：xochitl 自己退出时有概率崩溃，崩了系统会走应急路径整机重启（2026-09-25 真机多次），不如直接干净地重启。

**系统分区只读校验（dm-verity）开着时**：`/usr` 下的服务单元删不掉（脚本遇到 verity 一律不写 `/usr`，写 `/usr` 曾经让设备回滚变砖）。这时卸载脚本会如实提示，并**保留**这些单元要用的程序，免得重启后单元找不到程序、反复失败。等设备可写后再跑一次 `uninstall-all.sh` 就能收尾。

卸载全流程 2026-09-25 在真机整轮跑过：8 步逆序全部成功，书架/笔记数据与配置都保留，卸载期间 xochitl 没有重启。dm-verity 下保留程序、`--purge` 这两支仍只有本机模拟。

## 从旧版升级：清理已移除的组件

有些功能后来被砍了。它们的源码和安装步骤已经删掉，但以前装过的设备上还留着文件。

| 组件 | 何时移除 | 设备上可能留下什么 | 怎么清 |
|---|---|---|---|
| 电池刺客（`battop`，耗电诊断服务） | 2026-09-30 | `/usr` 里的 `battop.service`（更旧的版本还有 `battop.timer`）、`/home/root/battop/` 整个目录（程序和历史采样数据） | **重跑 `install-all.sh` 自动清** |
| 手写优化（`handwriting-stroke`，插件 `hw-stroke.so`） | 2026-09-30 | `extensions.d/hw-stroke.so`、待换入区里的副本、安装包目录 `/home/root/hw-stroke/` | **重跑 `install-all.sh` 自动清** |
| 书架的 `koreader-serve` 服务 | 2026-09-29 | 服务单元和程序 | 重跑 `install-all.sh`（或 `deploy.sh`）时顺手清 |
| 侧栏 KOReader 入口（`sidebar-entry`） | 2026-09-29 | 界面补丁 `koreader-sidebar-entry.qmd` 和图标包 `cangjie-icons.rcc` | **不自动清**，要手动跑一次 `uninstall-all.sh`（命令见下） |

![install-all 如何清理已移除的组件](diagrams/retired-cleanup.svg)

**自动清理怎么做**：`install-all.sh` 在最后一步 `xovi-apply` 之前，对电池刺客、手写优化各跑一次清理（汇总里显示为 `清理已移除:battop`、`清理已移除:handwriting-stroke`）。清理用的是和 `uninstall-all.sh` 同一份函数（在 `packaging/removal.sh`）。设备上没有残留就什么都不动；如果 xochitl 当时还加载着 `hw-stroke.so`，会记一个"待生效"标记，最后一步因此**整机重启一次**让它彻底停用（不会单独重启 xochitl）。不想清就 `--skip battop` / `--skip handwriting-stroke`。`reading-qol.json` 里旧的 `hwStroke*` 设置项会原样留着，没有程序再读，无害。

侧栏入口不自动清，是因为它的图标包文件名 `cangjie-icons.rcc` 历史上别的界面补丁也用过，只在你明确要求时才删。

**不重装、只清某几样**：用 `uninstall-all.sh` 跳过其它所有步骤。先 `--dry-run` 看计划，计划里应只剩你要清的那几步：

```sh
cd packaging
# 只清电池刺客与手写优化（计划里应只有 handwriting-stroke、battop 两步）
sh uninstall-all.sh 10.11.99.1 --dry-run --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,sidebar-entry
sh uninstall-all.sh 10.11.99.1 --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,sidebar-entry
# 只清侧栏入口
sh uninstall-all.sh 10.11.99.1 --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,battop,handwriting-stroke
```

跑完在设备上整机重启一次（`reboot`，别 `systemctl restart xochitl`）。

**核对**：`verify-on-device.sh` 看到这些残留会报出来——电池刺客、`hw-stroke.so`、旧 `koreader-serve` 报 ⚠；侧栏入口补丁还在报 ⚠，如果 appload 已经卸了则报 ✗（它会让侧栏补丁失效）。报 ⚠ 时给的清理命令就是上面这几条。

**真机情况**：2026-09-30 15:23 在真机上跑 `install-all.sh`，自动清掉了电池刺客（单元与 `/home/root/battop`）和 `hw-stroke.so`，只整机重启一次，之后核对 36✓ 0✗、xochitl 已不再加载 `hw-stroke.so`。上面"只清某几样"的 `uninstall-all.sh` 命令只在本机模拟测过。

## 固件升级（OTA）之后

这是 OTA 恢复的**权威说明**，其它文档都链接到这里。

![OTA 之后：哪些丢了、怎么恢复](diagrams/ota-recovery.svg)

**升级本身不会丢 `/home` 的数据，但升完要重跑一遍安装，功能才回来。** 本项目有意不在启动路径上留任何东西（xovi 的加载配置在 `/etc` 的内存层、服务单元在 `/usr`），所以新固件总是以纯原厂状态起来。

### 推荐流程

1. （升级前，可选）把与新固件不兼容的 xovi 插件（例如旧版的某个第三方插件）挪出 `extensions.d/`，放到 `/home/root/xovi-disabled/`。**绝不留在 `extensions.d/` 里**：xovi 会把那个目录下任何文件都当插件加载。
2. 升级完成后，**在设备旁手动**跑 `xovi/rebuild_hashtable`（要输 root 密码，脚本不代做）。它按新固件重新建一张界面资源的索引表，界面补丁靠这张表定位，所以它是补丁重新生效的前提。
3. 在电脑上：`cd packaging && sh install-all.sh <设备IP>`。新固件的哈希一般不在白名单里，确认版本无误后加 `--force`。OTA 后 xovi 没有生效，所以最后一步会整机重启一次，开机时由刚装回的 `xovi-reenable` 恢复 xovi，回来后自动核对。
4. 看收尾汇总，浏览器打开网关确认。

### 逐项对照

| 内容 | 位置 | OTA 后 | 怎么恢复 |
|---|---|---|---|
| 母版库、字体与壁纸池、证书、网关密码、休眠屏设置、`cangjie-backups/` | `/home` | 保留 | 不用管 |
| 各网页服务的程序（`~/.local/bin`） | `/home` | 保留 | 不用管 |
| `hl-snap`、`ui-font` 插件，字体菜单/界面字体令牌/回收站/建夹/漫画边距/阅读位置/阅读器翻页的界面补丁 | `/home`（`extensions.d/`、`exthome/`） | 文件还在，但要重建 hashtable 才生效 | 第 2 步，再跑 `install-all.sh` |
| 各网页服务与 `shelf.target` 的服务单元 | `/usr` | **被冲掉** | `shelf` 步（或单独：`SHELF_NO_BUILD=1 sh deploy.sh <设备IP>`） |
| `xovi-reenable.service`（开机自动让 xovi 生效） | `/usr` | **被冲掉** | `xovi-persist` 步 |
| `chrony-boot-wakelock.service` | `/usr` | **被冲掉** | `chrony-boot-wakelock` 步 |
| `wifi-watch.service`（脚本在 `/home`） | `/usr` | 单元**被冲掉** | `wifi-watch` 步 |
| 国内校时服务器、默认时区 | `/etc` | **被冲掉** | `chrony-cn` / `timezone-cn` 步 |

**风险分层**：书架这层只用 xochitl 的网页上传接口和系统标准组件，换固件重装就回来；字体菜单这类界面补丁依赖 xochitl 内部 QML，大版本升级常要重新适配。

**"裸机恢复"要多查一步**：OTA 本身不删 `/home`，但如果设备做过更彻底的重置，`/home` 下的插件和程序可能也没了（2026-09-09 真机踩过）。重跑 `install-all.sh` 前先确认它们还在。

## 常见问题

下面都是有明确触发条件的已知坑，不是随机故障。编号在上文被引用；①、⑥ 讲的是已移除的组件（KOReader 侧栏入口、电池刺客），条目已删，编号不复用。

| # | 现象 | 原因 | 怎么办 |
|---|---|---|---|
| ② | 字体菜单、回收站/新建文件夹、漫画页边距、找回阅读位置、阅读器单击翻页这几个**同时**没有（界面字体也只换了一部分） | 它们共用同一个前置 qt-resource-rebuilder。没装时：它们是 `shelf` 步里附带的补丁，**不单列**——`shelf` 仍算"已安装"，只在这一步的输出里有一行"无 qt-resource-rebuilder 目录…跳过字体菜单/界面字体/回收站/建夹/漫画页边距/阅读位置/阅读器翻页 qmd" | `vellum add qt-resource-rebuilder` 后重跑 `install-all.sh` |
| ③ | 短时间内 xochitl 反复停起后，设备整机重启了一次 | xochitl 服务设置了 10 分钟内最多重启 4 次，不管谁触发的都算：手动 `systemctl restart xochitl`、`vellum add/del` 装卸 xochitl 插件（以前还有 appload、WeRead 每次进出）。2026-09-11 真机上连续两次重启就触发过一次整机重启——**设备自己重启后恢复正常，不是变砖** | 部署脚本 2026-09-25 起改为整机重启，不再计入这个次数。手动装卸插件时每次间隔几分钟 |
| ④ | 固件安全门拒装 | 设计如此：版本号相同不保证内部布局没变 | 先确认设备固件就是你验证过的那份，再 `--force` |
| ⑤ | 装到最后设备重启了一次 | `xovi-apply` 让改动生效：2026-09-25 起一律**整机重启**（约 20–60 秒回来），不再单独重启 xochitl——单独重启它有概率在退出时崩溃、再由系统整机重启。只有这轮真的有改动、或 xovi 还没生效时才会重启 | 正常现象，装的时候别操作设备；脚本会等设备回来并自动跑一遍 `verify-on-device.sh` 核对。不想被打断就 `--skip xovi-apply`，稍后再跑 `sh deploy-xovi-apply.sh <host>`。**自己手动让它生效时**：直接 `reboot`；**绝不**手动跑 `xovi/start`（xovi 已生效时它会让 xochitl 崩溃、整机自动重启，2026-09-20 真机事故） |
| ⑦ | 装之前就报错退出：`连不上 root@…` / `只剩 N MB 可用` / `需要 root` / `固件不在白名单` | 装前自动检查在拦，设备上什么都没改 | 连不上：按报错里的步骤排查（休眠/没插 USB → IP → host key → 免密）；空间不足：清理 `/home/root` 和 `cangjie-backups/` 后重试；固件：见 ④ |
| ⑧ | 最后一步报"设备没能排上整机重启……改动尚未生效"，这一步记失败 | 设备上的 `systemctl reboot` 命令本身失败了。文件已经换好，但 xochitl 还在用旧的；脚本已把"待生效"标记补回去（2026-09-25 起，此前会白等设备重启再报成功）。这一支只在本机模拟过 | 在设备上手动 `reboot`，回来后跑 `sh verify-on-device.sh <host>`；或者稍后重跑 `sh deploy-xovi-apply.sh <host>`，它会再试一次 |
| ⑨ | 最后一步报"xochitl 里没有 xovi，设备上也没有 xovi/start"，这一步记失败 | 设备上没装 xovi（或被 vellum 删了）：这时整机重启也没法让插件和界面补丁生效，所以脚本直接报错、不重启（2026-10-09 起；以前会白白重启一次）。只在本机模拟过 | 在设备上 `vellum add xovi`，再重跑 `install-all.sh` |
| ⑩ | 装到一半报 ssh 断开（如 `Timeout, server … not responding`） | 设备休眠、拔了线或 WiFi 掉了。ssh 有保活，约 15 秒判定断线（2026-10-09 起；以前会一直挂着）。发到设备上的脚本要整份收到才执行，所以断在半截的那一步一条都没执行 | 唤醒设备、确认连接后整条重跑 `install-all.sh`（幂等，已装好的不会重复动） |

### 其它排障

- 先看 `install-all.sh` 的收尾汇总，定位哪一步失败；对应 `packaging/deploy-*.sh` 的开头注释写了这一步做什么、常见失败原因。
- 网页「管理」页能直接看到插件是否真的加载进了 xochitl（"已加载 / 未加载"）。开关开着但显示"未加载"，说明插件没装上或装完还没整机重启。「管理 → 设备健康」能看到更全的状态（各服务、扩展、上次开机日志）。
- 不碰真机就想确认脚本没被改坏：`bash packaging/tests/run_sim_tests.sh`（本机模拟，448 项断言，2026-10-10 实跑全过）。它代替不了真机验证。

### 备份与幂等（一句话版）

所有安装脚本都可以重复跑；覆盖设备上已有文件前先备份到 `/home/root/cangjie-backups/`（**绝不**放 `extensions.d/`），只留最近 5 份，内容没变就不备份也不动；写 `/usr` 前先检查 dm-verity，开着就跳过。细节见 [`packaging/README.md`](../packaging/README.md#备份与幂等)。

## 已知限制

- **哪些在真机上跑过、哪些没有**：
  - **真机整轮跑过**：`install-all.sh`（2026-09-22、09-24、09-25、09-30 各一次，10-09 两次）、`uninstall-all.sh`（2026-09-25）；"换入新版 `.so` → 整机重启 → 开机自动恢复 → 核对"2026-09-25 真机复核通过（43✓）。
  - **2026-09-30 两次真机部署**：14:10 用 `deploy.sh` 部署第五轮审计版，整机重启后核对 38✓ 1⚠（刚开机）0✗；15:23 用 `install-all.sh` 部署，**自动清掉电池刺客和 `hw-stroke.so`、只整机重启一次**，核对 36✓ 1⚠（刚开机）0✗，`/usr` 单元 12/12，9 个常驻服务无重启。第四、五轮审计改的安装脚本（`install-all` 只查一次连通、推送合批、没变的文件不重传等）都在这一轮里跑到，整轮通过。注意这只证明"装上了、服务健康"，书架和笔记的新功能还没逐项手测（清单见 [CHANGELOG 09-30](CHANGELOG.md#09-30)）。
  - **只有本机模拟、没在真机走过的**：卸载在 dm-verity 下保留程序、卸载时撤掉待换入区、"什么都没变就不重启"这一支（含单独部署）、换入关键区忽略断连信号、整机重启命令本身失败的处理、汇总的"前置条件不满足"栏、第五轮审计改的卸载行为（非空目录不中断、`--only` 刷新卸载清单等）、`uninstall-all.sh` 只清已移除组件。
  - **2026-10-09 两次真机部署**：13:48 经 WiFi、15:53 经 USB 各跑一次 `install-all.sh`，整机重启后核对 38✓ 1⚠、39✓ 1⚠（⚠ 都是刚开机）0✗。第六轮审计改的安装脚本（整组传输、ssh 保活、唤醒锁）的正常路径在这两轮里跑到；断线判定、断在半截不执行、没有 xovi 时报错不重启这些异常分支**只有本机模拟**。
  - 完整记录见 [`packaging/README.md`「验证现状」](../packaging/README.md#验证现状如实说明不夸大)。上机时一步一确认：先 `--dry-run`，再单步或 `--skip` 试跑。
- **写 `/usr` 仍靠"先查 dm-verity + 限时读写窗口"两道防线**，不是完全不碰 `/usr`；历史上写 `/usr` 触发过回滚变砖（2026-08-16）。
- **在设备上直接跑 `shelf/install.sh --password 明文` 时，密码会短暂出现在设备的进程列表里**；经电脑上的 `deploy.sh --password` 传则不会。
- 卸载不还原 `chrony-cn` / `timezone-cn`，没有"一键回到装之前"。

## 这套安装器不做什么

- **不装 vellum / xovi / qt-resource-rebuilder**：见「装之前」。
- **不装中文输入法**：那条功能线的源码已移出本仓库（见 [README](../README.md#历史与范围)），不随本安装器分发。
