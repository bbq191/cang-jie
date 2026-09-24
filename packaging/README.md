# packaging —— 全新设备统一安装器

> **读者与用途**：要给设备装/卸/更新这套增强的人，以及要改这些脚本的维护者。
> 这里讲**脚本的结构、每一步做什么、各文件的职责、怎么本机测试**。"第一次怎么装、风险有哪些、固件升级后怎么恢复"
> 这类面向使用者的说明在 [`../docs/INSTALL.md`](../docs/INSTALL.md)（中文）/ [`INSTALL.en.md`](../docs/INSTALL.en.md)，本文不重复。
> 末尾「验证现状」是真机验证与踩坑的整理，读现状先看前面的章节。

这是 **host 侧编排层**（在你的电脑上跑，经 ssh 装到设备）。设备固件跟 `firmware-allowlist.txt` 对得上时，一条命令装完当前仓库能装的一切：

```sh
cd packaging
sh install-all.sh --dry-run                                     # 先预演：纯本机，只打印计划，不 ssh
sh install-all.sh <host>                                        # 装全部
sh install-all.sh <host> --force                                # 固件不在白名单也强装（哈希追加进本机 firmware-allowlist.local.txt）
sh install-all.sh <host> --skip chrony-cn,timezone-cn,xovi-persist   # 跳过指定步骤（写错名字会警告，不会静默忽略）
sh install-all.sh <host> --force-apply                          # 没有待生效改动也重启 xochitl（缺省不重启，见「待生效标记」）
```

`<host>` 默认 `10.11.99.1`（USB 网段）。反悔想卸：

```sh
sh uninstall-all.sh <host> --dry-run    # 先预演：只打印将执行的步骤，不 ssh
sh uninstall-all.sh <host>              # 卸全部（chrony-cn / timezone-cn / xovi-apply 除外，见「卸载」）
sh uninstall-all.sh <host> --purge      # 同时删 battop 的二进制与历史采样数据
sh uninstall-all.sh <host> --skip shelf # 跳过指定步骤，用法同 --skip
```

全部参数与环境变量见下文「参数与环境变量」；所有脚本 `-h` 可看用法，未知选项一律退出码 2 且不连设备。
> ⚠ 真机验证范围见文末「验证现状」：2026-09-22 合并后整轮 `install-all.sh` 在真机跑过一次；卸载全流程、"无改动不重启"、2026-09-24 的 `.so` 换入顺序，以及 2026-09-24 审计的全部脚本改动（四栏汇总、单独部署不重启、关键区忽略信号等）仍只有本机模拟。

## 前置条件（需手动，本脚本不代装）

`vellum add xovi`、`vellum add qt-resource-rebuilder`、`vellum add appload`（**3.28 固件需要 ≥ 0.6.0**）、经 appload 侧载 KOReader。
各自缺了会怎样、装的顺序与注意事项见 [`INSTALL.md`「装之前」](../docs/INSTALL.md#装之前)。这里只补两条实现层面的事：

- `sidebar-entry` 靠 appload 暴露的 `AppLoadLauncher` 单例发起启动，并**只读当次开机的 journal 探测** appload 是否健康（找一行 `Loaded external AppLoad hooks in main UI`），探测不到就跳过、不硬装一个点了没反应的按钮。旧版（≤ 0.5.3）钩的是 3.27 的旧锚点，3.28 上注入失败；上游 PR #59 已并入 **v0.6.0（2026-09-19）**，2026-09-21 真机验证过（md5 与官方发布包一致）；此前的"等长回填 qmd"补丁工具已删除（git 历史可找回）。
- 升级 appload 后**整机重启，别 `systemctl restart xochitl`**（旧进程退出时崩溃，触发 `OnFailure=emergency.target` 整机重启，2026-09-21 踩到）。
- **WeRead** 是可选的第三方 app：`sidebar-entry` 自动探测，装了用「KOReader + WeRead」两项版 qmd，没装用只有「KOReader」的版本，不会因没装 WeRead 而报错或跳过整步。

## 装什么、按什么顺序

![install-all.sh 流程](../docs/diagrams/install-flow.svg)

`install-all.sh` 只编排、不重新实现任何构建/传输逻辑：先做装前检查（`require_device` ssh 连通 → 固件安全门 → `preflight_device` 设备预检，见下节），再按 `lib.sh` 里的**步骤表 `STEP_ORDER`** 依次调用 11 个各自独立可用的部署脚本（`uninstall-all.sh` 共用同一张表，所以安装与卸载清单对称）：

| 顺序 | 步骤名 | 脚本 | 装什么 | 前置 |
|---|---|---|---|---|
| 1 | `chrony-cn` | `deploy-chrony-cn.sh` | 国内 NTP（chrony 服务器换成阿里云/腾讯云等） | 无，跟 xovi/vellum 无关 |
| 2 | `chrony-boot-wakelock` | `deploy-chrony-boot-wakelock.sh` | 开机头一小段持一把 wakelock，防自动休眠打断 chronyd 首次校时（根因见「验证现状」） | 无；设备缺 `/sys/power/wake_lock` 时单元照装，开机时由 `ConditionPathExists` 跳过 |
| 3 | `timezone-cn` | `deploy-timezone-cn.sh` | 默认时区设为 Asia/Shanghai | 无；缺 `/usr/share/zoneinfo/Asia/Shanghai` 时警告后退出 0（汇总里仍记"已安装"） |
| 4 | `battop` | `deploy-battop.sh` | 电池刺客（纯 Rust systemd 常驻采样服务）；装完 start，**有意不开机自启** | 无 |
| 5 | `wifi-watch` | `deploy-wifi-watch.sh` | WiFi 载波假死看护（`wifi-watch/`：脚本 → `~/.local/bin`，单元 → `/usr`）；链路正常时只读 sysfs carrier、零 fork | 无 |
| 6 | `xovi-persist` | `deploy-xovi-persist.sh` | xovi 开机持久化恢复链（`xovi-reenable.service`） | 设备已 `vellum add xovi` |
| 7 | `hl-snap` | `deploy-hl-snap.sh` | 荧光笔 CJK 精确吸附（独立最小 xovi 扩展），只落盘 | 同上 |
| 8 | `handwriting-stroke` | `deploy-handwriting-stroke.sh` | CJK 手写笔迹渲染优化（独立最小 xovi 扩展），只落盘 | 同上 |
| 9 | `sidebar-entry` | `deploy-sidebar-entry.sh` | 侧栏一级直达「KOReader」入口（装了 WeRead 自动带上「WeRead」项），只落盘 | 已装 qt-resource-rebuilder 与 appload（且 appload 探测健康）；任一不满足自动跳过 |
| 10 | `shelf` | `deploy.sh` | 网关 + book/koreader/font/wallpaper 四个领域服务 + 笔记线（ink/transcribe/mind/note）；随 font/book 带的 qmd（字体菜单、回收站代理、建文件夹代理、漫画页边距、单击翻页）只落盘 | 无。qmd 需要 qt-resource-rebuilder：缺了只在这一步内部打印一行"跳过 qmd"，服务照装，汇总里这一步仍记"已安装" |
| 11 | `xovi-apply` | `deploy-xovi-apply.sh` | 统一让上面落盘的扩展/qmd 生效：**有待生效标记（含待换入的 `.so`）或 xovi 未生效才**重启 xochitl 一次 + 健康检查；`--force` 无条件重启 | 同 6/7/8/9 |

11 个脚本都可以单独跑（`sh deploy-battop.sh <host>` 等），不依赖 `install-all.sh`——它只是把它们串起来 + 加装前检查 + 汇总结果。任何一步失败：打印是哪一步、原始错误，**不自动重试、不静默跳过**，退出非零。

**收尾汇总分四栏**（2026-09-24 起；此前三栏，"前置条件不满足"混在"已安装"里容易漏看）：

| 栏 | 什么时候进这一栏 |
|---|---|
| 已安装 | 步骤脚本退出 0，且没调 `step_skipped` |
| 已跳过（--skip） | 命令行 `--skip` 点名的步骤，根本没跑 |
| 已跳过（前置条件不满足，非失败） | 步骤脚本调了 `lib.sh` 的 `step_skipped "<原因>"` 后退出 0；汇总里每步一行并写明原因。目前只有两处会调：`sidebar-entry`（没装 qt-resource-rebuilder / 没装 appload / 本次开机日志里没有 appload 挂载成功的信号）和 `deploy-usr-unit.sh`（dm-verity 激活且这个 `/usr` 单元**从没装过**，即 `chrony-boot-wakelock` / `xovi-persist` / `wifi-watch` 三步） |
| 失败 | 步骤脚本退出非 0 |

`step_skipped` 靠 `run_step` 导出的 `CJ_STEP_SKIP_FILE` 把原因写回编排方；单独跑某个脚本时它只打印一行"……——跳过，非失败"。**其余"步骤内部跳过一部分"的情况仍记"已安装"**：`shelf` 没装 qt-resource-rebuilder 时只跳过 qmd、`timezone-cn` 缺 zoneinfo、`xovi-apply` 设备没装 xovi 也没有待生效改动——看汇总之外，还要看这些步骤自己打印的那一行。

### 装前检查（非 `--dry-run` 时，任何一项不过就一个步骤都不执行）

1. **`require_device`**：`ssh true` 通不通（`BatchMode`，`CJ_SSH_TIMEOUT` 秒超时）。不通 → 打印 ssh 原始报错 + 中文排查步骤（休眠/没插 USB、IP 不对、host key 变了、没配免密），退出 1。各 `deploy-*.sh` 单独跑时同样先过这一关。
2. **`fw_gate`** 固件安全门：见下「固件安全门」。
3. **`preflight_device`**（只读，设备端 `dev_script`）：必须 root；`$HOME`（`/home/root`）可写；`df` 剩余空间 **< `CJ_MIN_FREE_KB`（50MB）退出 1 拒装、< `CJ_WARN_FREE_KB`（200MB）警告**（读不到空间只警告）；然后**只报告**：xovi 本体 / qt-resource-rebuilder / appload 有无、dm-verity 是否激活、xochitl 里 xovi 是否已生效。这些"缺"不拦装，对应步骤自己会报错或跳过。

`--dry-run` 不做以上任何一项——它只在本机走一遍步骤表，对每步打印 `[dry-run] 将执行：<命令>`，最后一行汇总列出计划步骤（`--skip` 的照常记为已跳过）。

### 为什么 hl-snap / handwriting-stroke / sidebar-entry 只落盘、最后统一重启

xovi 没有"只重载一个扩展"的机制，让新扩展/qmd 生效的唯一办法是重启 xochitl。每步各自重启的话，短时间多次重启会撞 xochitl 自带的 watchdog + StartLimit——真机验证过连续两次就触发一次意外整机重启（2026-09-11）。所以 `install-all.sh` 给这三步传 `DEFER_XOVI_START=1`（设备端 `install.sh --no-restart`）让它们只落盘，`shelf` 的 qmd 本来就只落盘，全部落盘完由 `xovi-apply` 统一重启一次。单独跑这几个脚本（不设这个变量）：落盘后立即重启并做健康检查；但**没东西要生效时不重启**（2026-09-24）——`sidebar-entry` 看 `cj_apply_needed`（见下节）；`hl-snap`/`handwriting-stroke` 在此之外还要求本扩展 `.so` 没变且已在运行中的 xochitl 里加载（`maps` 里找得到），否则照样重启。

### 待生效标记：重复运行不再每次重启 xochitl（2026-09-22）

旧版 `xovi-apply` 每次都重启，重跑 `install-all.sh` 必闪屏。现在靠**待生效标记**：

- **记**：`hl-snap` / `handwriting-stroke`（`xovi-ext-install.sh`，`--no-restart` 与单独跑两种模式都记——单独跑时重启失败，标记留着，之后 `deploy-xovi-apply.sh` 还能补上）、`sidebar-entry`（`deploy-sidebar-entry.sh`）、`shelf` 的 qmd（`shelf/install.sh`，标记名 `shelf-qmd`）在**真的写入了与设备上不同的内容**时调 `cj_pending_mark <名字>`，在 `/run/cangjie-pending-apply/<名字>` 建一个空文件。`/run` 是 tmpfs，设备重启即清（重启后一切本来就是新载入的）；`/run` 写不了时退到 `~/.cangjie-stage/pending-apply/`——宁可多重启也不漏。目录可用 `CJ_PENDING_DIR` 覆盖。
- **判**：`devlib.sh` 的 `cj_apply_needed` = "`cj_pending_list` 非空，或 xovi 还没在 xochitl 里生效"。`cj_pending_list` 除了标记，**还把待换入区 `~/.cangjie-stage/so-pending/` 里的每个 `.so` 列成 `so-pending:<文件名>`**——标记在 `/run` 重启即清、待换入区在 `/home` 不清，2026-09-24 之前"放进待换入区后设备重启过"会被误判无需重启，新版永远换不进去。`deploy-xovi-apply.sh` 与单独跑的 `hl-snap`/`handwriting-stroke`/`sidebar-entry` 共用这个判据。`deploy-xovi-apply.sh` 没有 `--force` 时：
  - `cj_apply_needed` 为假（xovi 已生效且无标记）→ 不重启，直接结束；
  - 无标记、xovi 未生效、且没有 `xovi/start`（设备根本没装 xovi）→ 跳过；
  - 其余（有标记，或 xovi 未生效）→ 走下面的 `cj_xochitl_apply`。
- **清**：`cj_xochitl_apply` 重启成功后 `cj_pending_clear`（只删目录里的常规文件再 `rmdir`）。
- **`--force-apply`**（`install-all.sh`）/ **`--force`**（`deploy-xovi-apply.sh`）：无视标记强制重启。用 `--skip xovi-apply` 跳过时标记保留，之后单独 `sh deploy-xovi-apply.sh <host>` 即可补上。
- `uninstall-all.sh` 摘掉扩展后**不清标记**（卸载的东西也要等 xochitl 重启才停止生效）。
- **单独跑**（不带 `DEFER_XOVI_START`）的 `deploy-hl-snap.sh` 等落盘后立即重启并清标记；什么都不需要生效时不重启（见上）。

### 怎么"重启"xochitl：`cj_xochitl_apply` 的判定（2026-09-20）

重启不是无条件 `xovi/start`。设备端 `devlib.sh` 的 `cj_xochitl_apply` 先看运行中 xochitl 进程的 `LD_PRELOAD` 里有没有 `xovi.so`：

- **已生效 → `systemctl restart xochitl`**；
- **没生效（刚开机/OTA 之后）→ `xovi/start`**。

因为 xovi 已生效时跑 `xovi/start` 会 umount 再重挂 xochitl 的 drop-in 目录，运行中的 xochitl 读文件失败 SEGV，系统按设计整机自动重启（2026-09-20 真机事故；旧版无条件 `xovi/start`，重跑 `install-all.sh` 必踩）。重启前先打印"将打断阅读"并留 `CJ_APPLY_GRACE` 秒（默认 5）宽限，不想被打断就 `--skip xovi-apply`。重启后核对 `is-active` / `MainPID` 是否变化 / `NRestarts` 不增 / 各扩展在 `maps` 里的段数。

**扩展 `.so` 有变化时不再 restart，改成 stop → 换文件 → start（2026-09-24）**：换掉运行中 xochitl 已映射的扩展 `.so` 再 `systemctl restart xochitl`，旧进程退出时会 SEGV → `OnFailure=emergency.target` → 整机重启。2026-09-21（appload）、09-24（hw-stroke）两次真机复现，第二次用的已经是"先写暂存再 rename 换新 inode"，照样崩。现在：

- `xovi-ext-install.sh` 发现 xochitl 正映射着旧版时，把新版放进**待换入区** `~/.cangjie-stage/so-pending/`（不在 `extensions.d`；可用 `CJ_SO_PENDING_DIR` 覆盖），由 `cj_xochitl_apply` 在 `systemctl stop xochitl` 之后、`start` 之前换入；`install-all` 的延后重启走同一条路。同一个新版已经在待换入区（上一轮 `--no-restart` 放进去、还没重启）时不再重复备份和重放。
- **stop 到 start 是关键区**：这段忽略 HUP/PIPE/INT/TERM。脚本经 ssh 跑，host 侧 Ctrl-C 或拔线后下一次输出会收到 SIGPIPE，不接住的话 shell 被杀，xochitl 就停在那里、屏幕没有界面，直到整机重启。
- 待换入区在 `/home`，设备中途重启也不丢，重启后仍算"待生效"（见上节），下次重启 xochitl 时照样换入。
- 卸载 `hl-snap`/`handwriting-stroke` 时一并撤掉待换入区里的同名版本（否则下次重启 xochitl 会把刚卸的扩展装回来）。
- xovi 未生效（走 `xovi/start`）时 xochitl 没映射扩展，待换入的 `.so` 直接换入——但先确认 `xovi/start` 存在，不存在就报错、什么都不换。

本机模拟测试覆盖了这些分支；**真机上还没用"有变化的 .so"走过一遍**。

![更新扩展 .so：先停、再换、再起](../docs/diagrams/so-swap-order.svg)

## 本目录文件导览

| 文件 | 职责 |
|---|---|
| `install-all.sh` / `uninstall-all.sh` | 统一安装/卸载编排；步骤表来自 `lib.sh`，卸载对每个非"配置覆写/纯动作"步骤都必须有 `uninstall_<步骤名>` 函数（测试会核对），并按步骤表**逆序**执行 |
| `lib.sh` | host 侧共用库：`rssh`/`rssh_in`/`rscp`（统一 `BatchMode` + `ConnectTimeout`，`CJ_SSH_TIMEOUT` 缺省 8 秒，设备休眠/断线时快速失败而不是卡死）、`shquote`（把任意字符串安全拼进远端命令行）、`dev_script`（把 `devlib.sh` + heredoc 脚本体经 `ssh sh -s` 送上设备执行）、`push_verified`（scp 到暂存路径后逐个 md5 对拍，不对就删暂存并失败，绝不落到最终位置）、步骤表 `STEP_ORDER`/`STEP_DEFER`/`STEP_CONFIG_ONLY`、`parse_step_args`（install/uninstall 共用的 `[host] --force --purge --force-apply --dry-run --skip -h` 解析，未知参数退出 2）、`host_arg`（薄 `deploy-*.sh` 共用的 `[host]` 解析）、`require_device`（ssh 连通检查）、固件安全门 `fw_gate`、设备预检 `preflight_device`、`run_step`（含 `--dry-run` 与四栏记账）、`step_skipped`（步骤报"前置条件不满足"） |
| `devlib.sh` | **设备侧**共用库（POSIX sh，兼容 busybox）：rootfs 读写窗口（remount rw 后无论成败/被信号打断都恢复 ro；信号含 ssh 断连后的 SIGPIPE，2026-09-24 补，`chrony-cn.sh`/`timezone-cn.sh` 各自的窗口同样补了）、`cj_safe_replace` 原子替换（先写暂存再 rename，不在运行中进程已映射的 inode 上原地写）、备份与轮转、`/usr` 下单元的安装/删除（先过 dm-verity 门）、`cj_backup_if_differs`（内容没变不重复备份）、待生效标记 `cj_pending_mark`/`_list`/`_clear` 与判据 `cj_apply_needed`、待换入区 `cj_so_stage`/`_unstage`/`_commit`、`cj_xochitl_apply`/`cj_xochitl_health` |
| `deploy-usr-unit.sh` | 把一个 systemd 单元装进设备 `/usr` 的统一部署器；`deploy-chrony-boot-wakelock.sh` / `deploy-xovi-persist.sh` / `deploy-wifi-watch.sh` 是它的薄包装。dm-verity 激活时见「备份与幂等」 |
| `deploy-xovi-ext.sh` + `xovi-ext-install.sh` | 装一个"独立最小 xovi 扩展"的统一部署器（host 侧构建+推送）与设备侧安装流程；`deploy-hl-snap.sh` / `deploy-handwriting-stroke.sh` 是薄包装，各扩展的 `deploy/install.sh` 只剩数据（名字、配置键）并 source `xovi-ext-install.sh` |
| `deploy.sh` | shelf 整包部署：组载荷（`bin/ systemd/ lo-alias/ xovi/ install.sh uninstall.sh manifest.sh devlib.sh`）→ 本地打成 tar → 推到设备 `shelf-pkg.new`，校验有 `install.sh` 后才换掉 `shelf-pkg` → 设备端 `install.sh`；`--password` 经标准输入走 0600 临时文件，不上命令行；第一个参数是选项（如 `--only`）时 host 用默认；`--only` 的选服务规则（给的 + 网关总会装、去重、未知令牌退出 2）与设备端 `install.sh` 共用 `shelf/manifest.sh` 的 `shelf_select`（2026-09-24 收成一处，两边算出的清单必然一致）；推送前按 `manifest.sh` 核对要装的二进制都在（缺了指路 `sh shelf/build.sh`，不再传完才失败）；装完无论成败兜底清掉设备上的密码临时文件 |
| `deploy-battop.sh` / `deploy-sidebar-entry.sh` / `deploy-xovi-apply.sh` / `deploy-chrony-cn.sh` / `deploy-timezone-cn.sh` | 各自的部署脚本（都支持 `-h`、未知/多余参数退出 2、ssh 不通中文报错退出 1；`deploy-xovi-apply.sh` 另有 `--force`） |
| `firmware-allowlist.txt` / `firmware-allowlist.local.txt` | 固件白名单：仓库里被 git 跟踪的一份 + 本机一份（`--force` 追加到后者，已 gitignore） |
| `wifi-watch/` | 看护脚本与单元；`xovi-reenable.service`、`chrony-boot-wakelock.service`、`sidebar-entry-*.qmd`/`.qrc`/`.png`、`chrony-cn.sh`、`timezone-cn.sh` 是其余步骤的载荷 |
| `tests/` | 本机模拟测试，见「测试」 |

**脱离编排手动跑设备端 `install.sh` 时的文件依赖**（只手拷单个 `install.sh` 不够，会清楚报错）：`shelf/install.sh` 要同目录的 `manifest.sh` 与 `devlib.sh`（`deploy.sh` 已一起打进载荷）；`hl-snap`/`handwriting-stroke` 的 `deploy/install.sh` 要同目录的 `xovi-ext-install.sh` 与 `devlib.sh`（`deploy-hl-snap.sh` 等会一起推送）；`battop/install.sh` 要同目录的 `devlib.sh`。shelf 载荷里的 `manifest.sh`（安装/卸载共用的清单）与 `devlib.sh` 会被装到设备的 `~/.local/lib/shelf/`，`shelf-uninstall` 运行时 source。

## 参数与环境变量

**命令行参数**（`-h` 看用法；未知参数退出码 2，且此时不会去连设备）：

| 脚本 | 参数 | 说明 |
|---|---|---|
| `install-all.sh` | `[host]` | 默认 `10.11.99.1` |
| | `--dry-run` | 纯本机，只打印计划；不 ssh、不做固件门/预检 |
| | `--skip a,b`（或 `--skip=a,b`） | 跳过步骤；名字不在 `STEP_ORDER` 里只警告 |
| | `--force` | 固件不在白名单也装，哈希追加进本机 `firmware-allowlist.local.txt` |
| | `--force-apply` | 传给 `xovi-apply` 的 `--force`：无待生效改动也重启 xochitl |
| | `-h`/`--help` | 用法；`--purge` 是 uninstall 的，这里会拒绝 |
| `uninstall-all.sh` | `[host]` `--dry-run` `--skip` `-h` | 同上语义；`--force`/`--force-apply` 是 install 的，这里会拒绝 |
| | `--purge` | 只额外删 battop 的 `/home/root/battop`（二进制+历史采样数据）；shelf 用户数据不受影响 |
| `deploy-xovi-apply.sh` | `[host]` `--force` | 无待生效改动也重启 |
| `deploy.sh` | `[host] [--only a,b] [--no-systemd] [--password PW]` | 后三项原样转给设备端 `shelf/install.sh`；`SHELF_NO_BUILD=1` 跳过交叉编译 |
| 其余 `deploy-*.sh` | `[host]` `-h` | — |
| `shelf/install.sh`（设备端） | `--only` `--no-systemd` `--src` `--password`/`--password-file` `-h` | `--password` 明文会让密码短暂出现在设备 `ps`；经 `deploy.sh` 走的是 `--password-file` |
| `shelf/uninstall.sh`（设备端，装成 `shelf-uninstall`） | `--only` `--purge` `--dry-run` `-h` | `--dry-run` 只列出将删除的现存路径 |

**环境变量**：

| 变量 | 默认 | 作用域 | 说明 |
|---|---|---|---|
| `CJ_SSH_TIMEOUT` | 8 | host | ssh `ConnectTimeout` 秒数 |
| `CJ_MIN_FREE_KB` | 51200 | host→设备预检 | `/home` 剩余空间低于此值（≈50MB）拒装 |
| `CJ_WARN_FREE_KB` | 204800 | host→设备预检 | 低于此值（≈200MB）只警告 |
| `CJ_ALLOWLIST` / `CJ_ALLOWLIST_LOCAL` | 仓库/本机白名单文件 | host | 改固件白名单路径 |
| `CJ_PENDING_DIR` | `/run/cangjie-pending-apply` | 设备 | 待生效标记目录 |
| `CJ_PENDING_FALLBACK` | `~/.cangjie-stage/pending-apply` | 设备 | `/run` 写不了时的标记退路 |
| `CJ_SO_PENDING_DIR` | `~/.cangjie-stage/so-pending` | 设备 | 待换入的扩展 `.so`（与 `extensions.d` 同分区、绝不在其中） |
| `CJ_APPLY_GRACE` / `CJ_HEALTH_SLEEP` | 5 / 5 | 设备 | 重启前宽限秒数 / 重启后等多久再做健康检查 |
| `CJ_BACKUP_KEEP` / `CJ_BACKUP_MAXBYTES` | 5 / 64MB | 设备 | 备份保留份数 / 单文件超此大小不自动删 |
| `DEFER_XOVI_START=1` | — | host（`install-all` 自动设） | hl-snap/handwriting-stroke/sidebar-entry 只落盘不重启 |
| `SHELF_NO_BUILD=1` · `CJ_SKIP_BUILD=1` · `CJ_BATTOP_BIN` · `XOVI_DIR` | — | host | 跳过/指定构建产物（见各脚本 `-h`） |
| `CJ_HOME` `CJ_SYSD` `CJ_XOVI` `CJ_PROC` `CJ_BACKUP_DIR` `CJ_STAGE_DIR` | 设备真实路径 | 设备 | `devlib.sh` 的路径，仅供模拟测试重定向 |
| `CJ_CHRONY_CONF` `CJ_LOCALTIME` `CJ_ZONEINFO` `CJ_MOUNTS` `CJ_TMPDIR` | `/etc/chrony.conf` 等 | 设备 | `chrony-cn.sh`/`timezone-cn.sh` 的目标路径，**仅供模拟测试，真机别设** |

## 固件安全门

装前 ssh 拉设备上 `/usr/bin/xochitl` 的 sha256，跟白名单比对：**仓库里的 `firmware-allowlist.txt`（每行 `sha256  <人读标签>`）+ 本机的 `firmware-allowlist.local.txt`**，命中任一份才继续；不命中默认拒装。确认要装用 `--force`，当前哈希追加进**本机**文件（已 gitignore，不改被 git 跟踪的白名单，避免"未验证的哈希"被误提交；路径可用 `CJ_ALLOWLIST_LOCAL` 覆盖）。为什么用 sha256 而不是版本号，见 `firmware-allowlist.txt` 内注释；当前白名单里有 3.28.0.172（现役）与 3.28.0.169（历史记录）两行。

## 备份与幂等

这是备份规则的权威说明（用户向文档只放一句话版）：

- 设备上被覆盖的旧文件统一备份进 `/home/root/cangjie-backups/`（**绝不留在 `extensions.d/` 里**——xovi 把该目录下任意文件当扩展加载，同名重复注册是致命错误，2026-08-15 踩过）；单文件 `<basename>.bak.pre-<时间戳>`，目录型（shelf）`shelf-<时间戳>/`。
- 只保留最近 5 份（设备端环境变量 `CJ_BACKUP_KEEP`），只轮转脚本自己生成的严格时间戳命名备份；超过 `CJ_BACKUP_MAXBYTES`（64MB）的单文件备份不自动删；手工命名的备份与任何用户数据不碰；全程无 `rm -rf`。
- 内容没变就不备份、不重启服务。"内容没变不备份"由 `cj_backup_if_differs`（`cmp` 对拍）实现——否则重复部署 5 次就会把真正有价值的旧版本挤出"保留最近 5 份"（`shelf/install.sh`、`sidebar-entry`、`xovi-ext-install.sh`、`deploy-usr-unit.sh` 都走它）。扩展 `.so` 放进待换入区后再次 `--no-restart` 部署同一个新版，也不再备份（2026-09-24）。
- 写 `/usr` 前先过 dm-verity 门（`deploy-usr-unit.sh`），激活时分两种（2026-09-24）：
  - 这个单元**从没装过** → 什么都不写，汇总记"已跳过（前置条件不满足）"；
  - **以前装过**（verity 是后来才开的）→ `/usr` 里的单元不动；`~/.local/bin` 下的脚本照常更新，且**脚本真有变化、该单元装完要启动（目前只有 `wifi-watch`）时 `systemctl restart` 它**，让在跑的服务用上新脚本。

## 开机启动顺序（2026-09-24）

![设备开机时序](../docs/diagrams/boot-order.svg)

真机重启实测（`journalctl -b -o short-monotonic`）：

| 时刻 | 发生什么 |
|---|---|
| 4.04s | xochitl 以原厂状态启动——早于 `/home` 挂载，**不等 `/home` 上的任何东西**（红线，见 `xovi-reenable.service` 头注） |
| 5.26s | `/home` 挂载 |
| 5.46–7.49s | `xovi-reenable` 跑 `xovi/start`，xochitl 带着 xovi 重启（7.40s 起），三个 hook 装上 |
| 7.53–9.04s | 书架/笔记/增强 9 个常驻服务依次起来（改前 ≈11.6s） |
| 37.4s | `multi-user.target`：被 `chrony-boot-wakelock` 持锁等 NTP 校时拖住，**有意如此**（防开机头几十秒自动休眠打断校时） |

09-24 调整了什么（都只改我们自己单元的排序，不给 xochitl 加任何依赖；模拟测试有 5 项守着）：

- **9 个常驻服务 `After=xovi-reenable.service`**：先让 xochitl 带 xovi 重启完再起，不跟它抢 CPU（服务都 `Nice=5`、`CPUWeight=20`，但 9 个进程同时初始化仍会拖慢 xochitl 起界面）。没装 `xovi-reenable` 时这条自动失效。
- **不再 `Wants/After=network-online.target`**：它们只监听回环或 `0.0.0.0`，调云端是按需发、失败重试。原来全系统只有它们拉这个 target，开机要专门等 `NetworkManager-wait-online`（真机 3.4s，没有 WiFi 时要等到超时）。重启后 wait-online 仍在跑（7.9–12.1s，依赖查询里看不出谁拉的），但已不在我们的关键路径上。
- **`xovi-reenable` 加 `TimeoutStartSec=120`**：oneshot 缺省启动超时是 infinity，上面那条排序让它一卡住所有服务都跟着等。平时 2 秒左右跑完。
- **网关 `After=NetworkManager.service`**：启动前 `lo-alias.sh` 要给 `usb1` 挂地址、mDNS 要枚举接口。
- **`fc-cache` 从网关挪到 `font-serve`**：字体归字体服务管；原来网关每次启动都要同步等它（约 1.5s），拖慢网页入口。

安装顺序（上面的步骤表）不变：配置类 → 独立服务 → xovi 持久化 → 只落盘的扩展/补丁 → 书架 → 最后统一重启 xochitl 一次。

## xovi-reenable.service 为什么放在 packaging/，不放 shelf/

xovi 持久化是**整个 xovi 层**通用的（重跑 `xovi/start` 会重注入全部扩展，不分 shelf / enhance / 其它），`shelf` 耦合它会破坏"网关+领域服务独立可插拔"的原则。2026-09-03 曾有一版把它装进 `shelf/install.sh`，被撤回（见 `shelf/docs/reMarkable书架白皮书.md` §03o，历史文档不改写）；`shelf/install.sh` 头注也写着这层该由"整包"装。`packaging/` 正是这层的归属。

## 卸载（`uninstall-all.sh`）

对称卸载，步骤表与安装共用，但**逆序**执行（`shelf → sidebar-entry → handwriting-stroke → hl-snap → xovi-persist → wifi-watch → battop → chrony-boot-wakelock`；配置覆写/纯动作步骤跳过）。`--dry-run` 只在本机打印这个计划。每一步：

| 步骤 | 卸载动作（另每步都清 deploy 推到设备的载荷目录，见下） |
|---|---|
| `shelf` | **优先**调设备上的 `~/.local/bin/shelf-uninstall`（`install.sh` 每次更新它，单一事实源），没有才退回 `shelf-pkg/shelf/uninstall.sh`；清单与安装共用 `manifest.sh`；默认保留用户数据，`--purge` 不作用于 shelf；成功且书架二进制已清掉后才删 `shelf-pkg`/`shelf-pkg.new`（必须是真目录且含 `shelf/` 标记） |
| `sidebar-entry` | 从 qt-resource-rebuilder `exthome` 摘除 qmd/rcc |
| `hl-snap` / `handwriting-stroke` | 从 `extensions.d` 摘除 `.so`（含 `.crashed` 标记），并撤掉待换入区里的同名新版（否则下次重启 xochitl 会被换回来，2026-09-24 修）；运行中的 xochitl 还加载着它时提示"要立刻停用请整机重启"（见下）；不碰 `reading-qol.json`（多个扩展共用）、不碰 `cangjie-backups/` |
| `xovi-persist` / `chrony-boot-wakelock` | 停用并删 `/usr` 单元（同一套 dm-verity 门 + 带 trap 的 rw 窗口） |
| `wifi-watch` | 同上删单元；单元删掉后才删 `~/.local/bin/wifi-watch.sh` |
| `battop` | 停用并删单元；`--purge` 才连 `/home/root/battop`（二进制 + 历史采样数据）一起删 |
| `chrony-cn` / `timezone-cn` / `xovi-apply` | 配置覆写与纯动作，没有卸载语义，**有意不卸**（改前备份在 `cangjie-backups/`，要还原自己取） |

**载荷目录与暂存**：每步清 `deploy-*` 推到设备上的载荷（`/home/root/pkg-<名>/`、`hl-snap/`、`hw-stroke/`、`shelf-pkg/`）——`cj_rm_payload` 只 `rm` 已知文件再 `rmdir`，目录里有别的东西就留着，全程无 `rm -rf`（`shelf-pkg` 例外，见上，且有符号链接/标记校验）；最后 `cleanup_staging` 清 `~/.cangjie-stage` 里本项目的中转文件。

**dm-verity 激活时**：`/usr` 单元删不掉（`cj_remove_usr_unit` 返回 3，卸载视为"如实跳过"不算失败）。此时**保留**这些单元依赖的东西：`wifi-watch.sh`（单元 `Restart=always`，删了脚本它会每 10 秒起一次并失败）、shelf 二进制与 `shelf-pkg`（`shelf/uninstall.sh` 在单元残留时保留二进制与 `shelf-uninstall`，`uninstall-all` 见书架二进制还在就保留 `shelf-pkg` 这条"可写后再卸一次"的退路，并且不清待生效标记）。设备可写后**重跑一次 `uninstall-all.sh`** 即收敛。没有任何 `/usr` 单元残留时不再 remount rw（幂等）。

摘掉 xovi 内容后当前运行中的 xochitl 仍是旧映射，要等下次重启才真正停止生效；卸载脚本不主动重启。只摘了 qmd：`systemctl restart xochitl`（xovi 已生效时别用 `xovi/start`）；摘了 xochitl 正加载着的扩展 `.so`（该步会提示）：建议整机重启——"删掉运行中已映射的 `.so` 再让 xochitl 退出"与 H3 是同一类操作，这条路径没有真机验证过。

## 测试：不碰真机验证脚本

```sh
bash packaging/tests/run_sim_tests.sh      # 现为 226 项断言（2026-09-24 审计后）；也由 packaging/tests/test_install_scripts_sim.py 的 pytest 调用
```

做法：`tests/stubs/` 下放假的 `ssh`/`scp`/`systemctl`/`mount`/`dmsetup`/`id`/`sleep`/`curl`/`journalctl`/`rcc` 等塞进 `PATH`，用临时目录当"设备"；假 `ssh` 把远端命令直接在本机沙箱里执行，所以设备端脚本（`devlib.sh`、`shelf/install.sh`、各 heredoc 脚本）跑的是**真代码**，只是 rootfs/systemd/mount 被桩住并写日志，可断言"有没有 remount rw、最后一次 mount 是不是 ro、有没有跑 `xovi/start`"。

覆盖：`devlib` 各函数（含待生效标记、备份去重）；shelf 安装的幂等 / 缺载荷不留半成品 / rw 窗口失败恢复 ro / 只重启有变化的服务 / verity 下已有单元照常重启；shelf 卸载与安装清单对称、verity 下保留二进制、`--dry-run`；`deploy.sh`（密码含特殊字符、`shelf-pkg` 换位、选项当首参、推送前核对二进制、密码文件兜底清理）；hl-snap 部署（原子落位、备份不进 `extensions.d`；xochitl 正映射旧版时放进待换入区、由 `cj_xochitl_apply` 在 stop 与 start 之间换入）；`xovi-apply`"无待生效改动不重启"与 `sidebar-entry` 的"xovi 已生效 → restart、绝不 `xovi/start`"判定；整轮 `install-all` → `uninstall-all` 对称；参数解析 / `--dry-run` / `-h` / 设备不可达 / 设备预检（磁盘空间）；`uninstall-all` 的载荷清理保守性与 wifi-watch 在 verity 下留脚本；`chrony-cn`/`timezone-cn`（路径覆盖；只测非 overlay 与 verity 路径，overlay 底层改写只能真机验证）；静态守卫（`remount,rw` / `xovi/start` 只许出现在库里）；2026-09-24 审计新增：卸载连带撤掉待换入的 `.so`、设备重启后待换入区仍算待生效、单独部署无变化不重启、stop→换入→start 中途断连（SIGPIPE）仍会 start、前置条件不满足的跳过单列、`shelf_select`。**拒绝以 root 运行**（设 `CJ_SIM_ALLOW_ROOT=1` 才强行跑）。这些是本机模拟，**不能代替真机验证**。

**CI 现状**：`.github/workflows/ci.yml` 会跑 shellcheck、上面这套模拟测试、各 Rust crate 的 `cargo test` 与交叉编译冒烟。但 GitHub Actions 从 2026-09-20 起因账户扣费失败，每次都在几秒内失败、**没有执行任何检查**（`gh run list` 可见）；这段时间的改动靠本地在干净 checkout 里跑同样的检查。CI 恢复前，别把"push 了没报错"当成"测过了"。

## 固件升级（OTA）之后

恢复流程、逐项对照表与流程图统一放在 [`../docs/INSTALL.md`](../docs/INSTALL.md#固件升级ota之后)（本文不再另写一份）。要点：先设备旁手动 `xovi/rebuild_hashtable`，再在电脑上重跑 `sh install-all.sh <host>`（新固件哈希不在白名单要 `--force`）。

## 明确不做的事（已知缺口，别当成已经解决）

- **不装 vellum/xovi/qt-resource-rebuilder/appload 本体、不侧载 KOReader**——手动前置条件，缺失时子脚本清楚报错，`install-all.sh` 收尾摘要再提醒一次。
- **不装中文化**（输入法/候选栏/词典/UI 汉化）——那条链路（`chinese-ime/langhook/`）随 2026-09-11 的仓库整理挪出了 git 仓库，不随本安装器分发。（2026-09-24 只读查看设备：`extensions.d/` 里没有 `cangjie-langhook.so`。）
- **不升级 appload**——3.28 需要 ≥ 0.6.0，已装旧版的自己 `vellum upgrade appload` 后整机重启。
- **`chrony-cn` / `timezone-cn` 没有卸载语义**（配置覆写）。
- 旧的 `packaging/package.sh`（打 `cangjie-full-*.tar.gz` 单体安装包）**没有**恢复：它已随归档挪出仓库且按旧目录结构找载荷；本目录是纯编排现有独立脚本，不是复刻旧的单体打包架构。

## 已知限制（别当成已经解决）

- **只在本机模拟验证、没在真机走过的分支**：卸载全流程（逆序 + 载荷目录清理 + verity 保留分支 + 撤掉待换入 `.so`）；单独部署"没变化不重启"；stop→换入→start 关键区忽略 HUP/PIPE/INT/TERM（2026-09-24）；`xovi-apply`"无待生效改动就不重启"；`--force-apply`/`--force`；`cj_backup_if_differs` 的"内容没变不备份"；`deploy.sh` 的推送前核对与密码文件兜底清理；2026-09-24 的 `.so` 待换入区（stop → 换入 → start）。首次安装这条主路径在真机跑过，见「验证现状」。
- **`xovi-reenable.service` 防护（2026-09-22）**：单元带 `ExecCondition=/bin/sh -c '! grep -q "xovi[.]so" /proc/<xochitl MainPID>/maps'`——xochitl 已映射 `xovi.so` 就跳过（systemd 把 ExecCondition 非 0 视为"跳过"而非"失败"），不再对已生效的 xochitl 跑 `xovi/start`（那会让它 SEGV → 整机重启）。三种情形（已生效/未生效/xochitl 不在跑）有本机模拟测试；新版单元已部署到设备（2026-09-24 只读核对：设备上的单元与仓库一致）。"xovi 已生效时手动重跑它会被跳过"这一行为本身没有在真机上专门触发过。
- **`/usr` 下单元的写入仍靠"dm-verity 门 + 带 trap 的 rw 窗口"**，不是彻底不碰 `/usr`；同样只在模拟里测过，历史上写 `/usr` 触发过 A/B 回滚变砖（2026-08-16）。
- **`shelf/install.sh --password 明文` 直接在设备上跑时密码短暂出现在设备 `ps`**（`deploy.sh` 走 0600 临时文件 + `--password-file` 不受影响）。
- **待生效标记只覆盖已接入的四处**（`hl-snap` / `handwriting-stroke` / `sidebar-entry` / `shelf` qmd）。手工替换 `.so`/qmd 不会留标记——用 `--force-apply`/`--force`。`/run` 与退路目录都写不了时只警告，此时 `xovi-apply` 可能误判"无需重启"，按提示手动 `systemctl restart xochitl`。

---

## 验证现状（如实说明，不夸大）

> 这是按时间累积的真机验证与踩坑整理（2026-09-11 起）。步骤号、"八步/十个脚本"等是当时的说法，**以上面的步骤表为准**；2026-09-20 脚本重构（共用 `lib.sh`/`devlib.sh`、原子替换、备份轮转、xovi 已生效走 `systemctl restart`）之后个别做法已被取代（如 battop 部署"先 stop 再 scp 覆盖"→ 现在推 `battop.new` 后原子 rename）。

### 真机验证过

| 日期 | 内容 | 结果 |
|---|---|---|
| 2026-09-11 | 固件安全门（3.28.0.172 真机 sha256 命中白名单） | 通过 |
| 2026-09-11 | `deploy-hl-snap.sh` / `deploy-handwriting-stroke.sh` 整流程（构建→推送→设备端安装→`journalctl` 确认 hook 已加载、`is-active`=active、`NRestarts`=0） | 通过 |
| 2026-09-11 | `install-all.sh` 当时的八步（还没有 `sidebar-entry`） | 用户确认"已成功安装" |
| 2026-09-11 起 | `xovi-persist` 核心承诺：一次意外整机重启（见下"连续重启"）后，`xovi-reenable.service` 自动触发并成功退出，运行中 xochitl 的 `maps` 含 `xovi.so`——无需手动 `xovi/start`。已知无害怪癖：`systemctl status` 显示 `disabled; preset: disabled`，是"手写 `multi-user.target.wants/` 软链装单元"的 `is-enabled` 显示问题，开机是否激活看 `.wants/` 软链在不在，别被标签误导 | 通过 |
| 2026-09-13/14 | `deploy-sidebar-entry.sh`：默认模式（探测到 WeRead → 两项版 qmd → 本地 `rcc` → md5 → 重启，`CJ-SIDEBAR[8]: KOReader`/`[9]: WeRead`）；`DEFER_XOVI_START=1` 模式（只落盘不重启）；qt-resource-rebuilder / appload 缺失的跳过分支（临时改目录名，`exit 0` 且无任何后续动作，可逆）；没装 WeRead 时退回单项版 qmd（两次真实重启、间隔约一分钟，`NRestarts=0`） | 通过 |
| 2026-09-14 | `chrony-cn.sh` / `timezone-cn.sh` 的**改写**分支（故意改回非目标状态）+ 重启后持久化；`timezone-cn` 缺 zoneinfo 时的优雅跳过 | 通过 |
| 2026-09-14 | `chrony-boot-wakelock`（根因与修法见下）：重启后 `journalctl -u chronyd -b` 零 "Forward time jump detected"，服务 34 秒内持锁→放锁，`System clock synchronized: yes`，`remarkable-enable-slumber.service` 随后正常打开自动休眠 | 通过 |
| 2026-09-15 | trampoline 通用安装代码收进 `enhance/shared/`（纯参数化提取）：`journalctl` 确认 hl-snap 与 hw-stroke 三个 hook 全部"安装完成"，日志逐字节一致，两次连续重启 `NRestarts` 全程 0，`reading-qol.json` 用户配置原样保留 | 通过 |
| 2026-09-15 | `deploy-hl-snap/handwriting-stroke` 补齐"部署前备份 + md5 校验"：`cangjie-backups/` 下真的新增备份，且备份 md5 精确匹配部署前的旧版本 | 通过（battop 的备份分支当时没触发，待下次装 battop 顺带确认） |
| 2026-09-21 | appload 0.6.0：md5 与官方发布包一致、日志有 "Loaded external AppLoad hooks in main UI"、KOReader/WeRead 入口正常 | 通过 |
| 2026-09-22 | 审计分支合并后整轮 `install-all.sh`（9 个服务 + 扩展 + 一次 xochitl 重启），服务健康、`NRestarts` 为 0；新版 `xovi-reenable.service`（带 `ExecCondition`）随之部署，2026-09-24 只读核对设备上的单元与仓库一致 | 通过（走的是"有改动 → 重启"这一支） |
| 2026-09-24 | 第三轮审计合并后整轮 `install-all.sh`：两个扩展 `.so` 有变化 → 先进待换入区，`xovi-apply` 列出 `so-pending:hl-snap.so so-pending:hw-stroke.so` → **stop → 换入 → start**，MainPID 30840→37709、`NRestarts` 0、设备未整机重启（uptime 连续）、待换入区清空、三个 hook "安装完成" | **通过**（stop→换→start 第一次在真机走有变化的 `.so`） |
| 2026-09-24 | 开机顺序调整后真机重启：xochitl 4.04s 原厂启动（未被拖慢）、`xovi-reenable` 5.46–7.49s 自动恢复 xovi、9 个服务 7.53–9.04s 起来且全部 active、三个 hook 装上、网页 401 正常 | 通过 |

### 没有真机验证

- **卸载脚本**（`uninstall-all.sh`）：只有本机模拟；2026-09-22 起的逆序 / 载荷清理 / verity 保留分支同理。
- **"无改动不重启"**：`xovi-apply` 在无待生效标记时跳过重启、`--force-apply`、单独跑 `deploy-hl-snap/handwriting-stroke/sidebar-entry` 没变化不重启（2026-09-24）——只有本机模拟。
- **2026-09-24 审计的其余脚本改动**：汇总第四栏"前置条件不满足"、stop→换入→start 关键区忽略信号、rw 窗口补 SIGPIPE、dm-verity 下已装单元的 `wifi-watch` 脚本更新后重启服务、卸载撤掉待换入 `.so` 与"仍加载着就提示整机重启"——只有本机模拟。
- **`.so` 待换入区（2026-09-24）**：stop → 换入 → start 已在真机走通（见上表）；"设备重启后待换入区里的 `.so` 仍算待生效"这一支没真机走过。
- `sidebar-entry` 在 appload 版本过旧时"探测不到信号就跳过"这一分支：逻辑只是一行 `grep -q`，靠代码审查；真机只确认了"正面信号存在"这一半。
- `deploy-xovi-apply.sh`：整体随 `install-all.sh` 整轮真机跑通，但没单独逐项核对它打印的 hl-snap/hw-stroke 段数、NRestarts 细节。
- `deploy-battop.sh` 的 host 侧覆盖前备份分支（旧版）与 2026-09-20 之后的"`battop.new` 原子 rename"新流程：没有重跑验证。

### 踩过的坑（已修）

- **连续重启撞 StartLimit**：`hl-snap`/`handwriting-stroke` 各自的设备端 `install.sh` 都跑一次 `xovi/start`，`install-all.sh` 连续调用时短时间重启 xochitl 两次，撞 watchdog+StartLimit 触发意外整机重启，紧接着的下一步因设备在重启窗口期 `ssh: Connection refused`。→ 设备端 `install.sh` 加 `--no-restart`，`deploy-xovi-apply.sh` 成为唯一的重启点、放最后。
- **`timezone-cn.sh` 幂等分支被判失败**：脚本末条语句写成 `[ "$changed" = "1" ] && echo ...`，`changed=0` 时用这条判断的非零退出码收尾。→ 改成显式 `if` + `exit 0`。
- **`chrony-boot-wakelock` 的根因**：重启后 `timedatectl` 的 `System clock synchronized` 卡在 `no` 超过 10 分钟，但 NTP 配置本身没问题、时间也准。`journalctl -u chronyd` 显示 chronyd 选中源后打出 `Forward time jump detected!` → `Can't synchronise: no selectable sources`，之后失败退避。同一台设备的内核日志里，这条消息第一次出现的时刻与 `PM: suspend entry (deep)` / `suspend exit` 完全对上：官方 `remarkable-enable-slumber.service` 开机后几十秒就 `echo mem > /sys/power/autosleep`，设备离 USB/触屏几秒就真挂起，撞上 chronyd 刚选中源、准备 `makestep` 的窗口，被 chrony 的连续性检查误判为"系统钟被外部动了"（chrony-users 邮件列表[2020-10 帖](https://listengine.tuxfamily.org/chrony.tuxfamily.org/chrony-users/2020/10/msg00019.html)里维护者确认了这类触发条件）。→ 新增独立单元 `chrony-boot-wakelock.service`：开机后持标准 wakelock（`/sys/power/wake_lock`），轮询 `timedatectl show -p NTPSynchronized` 最多 120 秒、同步即放锁；`Before=remarkable-enable-slumber.service` 只是排序声明（不是 `Requires=`），**不碰官方单元本身**（红线：改核心服务启动依赖/时机推演有误就是变砖级事故），这个单元失败/超时/缺 wake_lock 都不拖累任何其它单元。
- **`deploy-battop.sh` 里 cargo 找不到 `.cargo/config.toml`**：Cargo 搜配置是按**当前工作目录**往上找，不看 `--manifest-path`，在 `packaging/` 下直接调用会吃不到 CC/AR 覆盖，链接 `crt1.o` 时报 `Relocations in generic ELF`。→ 先 `cd enhance/battop/` 再 `cargo build`。
- **部署脚本构建前 `make clean` 会删掉仓库里已提交的产物**：`hl-snap.so`、`hw-stroke.so` 与 `xovi_glue.{c,h}` 都已提交（不用每次现建才能部署），但本机没有 `asivery/xovi` clone 时编不出新的，`make clean` 先删后编译失败，仓库里可用产物被脚本自己删了（当场发现、`git checkout` 救回）。→ 不清；构建失败时退回用已提交的版本并打印清楚的提示。
- **shelf 健康检查太急**：原来固定 `sleep 1` 就查 `is-active`，而 `gateway` 首次启动要签发私有 CA/自签证书，1 秒不够被误判"没起来"。→ `shelf/install.sh` 的健康检查改成轮询（最多 10 秒）。
- **换了运行中 xochitl 已映射的扩展 `.so` 再 restart → 整机重启**（2026-09-21 appload、2026-09-24 hw-stroke 两次真机）：旧进程退出时 SEGV，触发 `OnFailure=emergency.target`。第二次已经是"先写暂存再 rename 换新 inode"，照样崩。→ `.so` 待换入区 + stop → 换入 → start（见上文与图）。
- **旧版 battop 重装 ETXTBSY**：`scp` 直接覆盖正在执行的二进制被内核拒绝。→ 曾改成"推送前先 stop"，2026-09-20 之后改为推 `battop.new` 再原子 rename（不必停服务）。
