# packaging —— 全新设备统一安装器

> **读者与用途**：要给设备装/卸/更新这套增强的人，以及要改这些脚本的维护者。
> 这里讲**脚本的结构、每一步做什么、各文件的职责、怎么本机测试**。"第一次怎么装、风险有哪些、固件升级后怎么恢复"
> 这类面向使用者的说明在 [`../docs/INSTALL.md`](../docs/INSTALL.md)（中文）/ [`INSTALL.en.md`](../docs/INSTALL.en.md)，本文不重复。
> 末尾「验证现状」是真机验证与踩坑的整理，读现状先看前面的章节。

这是 **host 侧编排层**（在你的电脑上跑，经 ssh 装到设备）。设备固件跟 `firmware-allowlist.txt` 对得上时，一条命令装完当前仓库能装的一切：

```sh
cd packaging
sh install-all.sh <host>                                        # 装全部
sh install-all.sh <host> --force                                # 固件不在白名单也强装（哈希追加进本机 firmware-allowlist.local.txt）
sh install-all.sh <host> --skip chrony-cn,timezone-cn,xovi-persist   # 跳过指定步骤（写错名字会警告，不会静默忽略）
```

`<host>` 默认 `10.11.99.1`（USB 网段）。反悔想卸：

```sh
sh uninstall-all.sh <host>              # 卸全部（chrony-cn / timezone-cn / xovi-apply 除外，见「卸载」）
sh uninstall-all.sh <host> --purge      # 同时删 battop 的二进制与历史采样数据
sh uninstall-all.sh <host> --skip shelf # 跳过指定步骤，用法同 --skip
```

## 前置条件（需手动，本脚本不代装）

`vellum add xovi`、`vellum add qt-resource-rebuilder`、`vellum add appload`（**3.28 固件需要 ≥ 0.6.0**）、经 appload 侧载 KOReader。
各自缺了会怎样、装的顺序与注意事项见 [`INSTALL.md`「装之前」](../docs/INSTALL.md#装之前4-样东西需要你手动装好)。这里只补两条实现层面的事：

- `sidebar-entry` 靠 appload 暴露的 `AppLoadLauncher` 单例发起启动，并**只读当次开机的 journal 探测** appload 是否健康（找一行 `Loaded external AppLoad hooks in main UI`），探测不到就跳过、不硬装一个点了没反应的按钮。旧版（≤ 0.5.3）钩的是 3.27 的旧锚点，3.28 上注入失败；上游 PR #59 已并入 **v0.6.0（2026-09-19）**，2026-09-21 真机验证过（md5 与官方发布包一致）；此前的"等长回填 qmd"补丁工具已删除（git 历史可找回）。
- 升级 appload 后**整机重启，别 `systemctl restart xochitl`**（旧进程退出时崩溃，触发 `OnFailure=emergency.target` 整机重启，2026-09-21 踩到）。
- **WeRead** 是可选的第三方 app：`sidebar-entry` 自动探测，装了用「KOReader + WeRead」两项版 qmd，没装用只有「KOReader」的版本，不会因没装 WeRead 而报错或跳过整步。

## 装什么、按什么顺序

![install-all.sh 流程](../docs/diagrams/install-flow.svg)

`install-all.sh` 只编排、不重新实现任何构建/传输逻辑：先过固件安全门，再按 `lib.sh` 里的**步骤表 `STEP_ORDER`** 依次调用 11 个各自独立可用的部署脚本（`uninstall-all.sh` 共用同一张表，所以安装与卸载清单对称）：

| 顺序 | 步骤名 | 脚本 | 装什么 | 前置 |
|---|---|---|---|---|
| 1 | `chrony-cn` | `deploy-chrony-cn.sh` | 国内 NTP（chrony 服务器换成阿里云/腾讯云等） | 无，跟 xovi/vellum 无关 |
| 2 | `chrony-boot-wakelock` | `deploy-chrony-boot-wakelock.sh` | 开机头一小段持一把 wakelock，防自动休眠打断 chronyd 首次校时（根因见「验证现状」） | 无；设备缺 `/sys/power/wake_lock` 时优雅跳过 |
| 3 | `timezone-cn` | `deploy-timezone-cn.sh` | 默认时区设为 Asia/Shanghai | 无；缺 `/usr/share/zoneinfo/Asia/Shanghai` 时优雅跳过 |
| 4 | `battop` | `deploy-battop.sh` | 电池刺客（纯 Rust systemd 常驻采样服务）；装完 start，**有意不开机自启** | 无 |
| 5 | `wifi-watch` | `deploy-wifi-watch.sh` | WiFi 载波假死看护（`wifi-watch/`：脚本 → `~/.local/bin`，单元 → `/usr`）；链路正常时只读 sysfs carrier、零 fork | 无 |
| 6 | `xovi-persist` | `deploy-xovi-persist.sh` | xovi 开机持久化恢复链（`xovi-reenable.service`） | 设备已 `vellum add xovi` |
| 7 | `hl-snap` | `deploy-hl-snap.sh` | 荧光笔 CJK 精确吸附（独立最小 xovi 扩展），只落盘 | 同上 |
| 8 | `handwriting-stroke` | `deploy-handwriting-stroke.sh` | CJK 手写笔迹渲染优化（独立最小 xovi 扩展），只落盘 | 同上 |
| 9 | `sidebar-entry` | `deploy-sidebar-entry.sh` | 侧栏一级直达「KOReader」入口（装了 WeRead 自动带上「WeRead」项），只落盘 | 已装 qt-resource-rebuilder 与 appload（且 appload 探测健康）；任一不满足自动跳过 |
| 10 | `shelf` | `deploy.sh` | 网关 + book/koreader/font/wallpaper 四个领域服务 + 笔记线（ink/transcribe/mind/note） | 无（qmd 依赖 qt-resource-rebuilder，缺了自动跳过，只落盘） |
| 11 | `xovi-apply` | `deploy-xovi-apply.sh` | 统一让上面落盘的扩展/qmd 生效：重启 xochitl 一次 + 健康检查 | 同 6/7/8/9 |

11 个脚本都可以单独跑（`sh deploy-battop.sh <host>` 等），不依赖 `install-all.sh`——它只是把它们串起来 + 加一层固件门 + 汇总结果。任何一步失败：打印是哪一步、原始错误，**不自动重试、不静默跳过**，退出非零。

### 为什么 hl-snap / handwriting-stroke / sidebar-entry 只落盘、最后统一重启

xovi 没有"只重载一个扩展"的机制，让新扩展/qmd 生效的唯一办法是重启 xochitl。每步各自重启的话，短时间多次重启会撞 xochitl 自带的 watchdog + StartLimit——真机验证过连续两次就触发一次意外整机重启（2026-09-11）。所以 `install-all.sh` 给这三步传 `DEFER_XOVI_START=1`（设备端 `install.sh --no-restart`）让它们只落盘，`shelf` 的 qmd 本来就只落盘，全部落盘完由 `xovi-apply` 统一重启一次。单独跑这几个脚本（不设这个变量）行为不变：落盘后立即重启并做健康检查。

### 怎么"重启"xochitl：`cj_xochitl_apply` 的判定（2026-09-20）

重启不是无条件 `xovi/start`。设备端 `devlib.sh` 的 `cj_xochitl_apply` 先看运行中 xochitl 进程的 `LD_PRELOAD` 里有没有 `xovi.so`：

- **已生效 → `systemctl restart xochitl`**；
- **没生效（刚开机/OTA 之后）→ `xovi/start`**。

因为 xovi 已生效时跑 `xovi/start` 会 umount 再重挂 xochitl 的 drop-in 目录，运行中的 xochitl 读文件失败 SEGV，系统按设计整机自动重启（2026-09-20 真机事故；旧版无条件 `xovi/start`，重跑 `install-all.sh` 必踩）。重启前先打印"将打断阅读"并留 `CJ_APPLY_GRACE` 秒（默认 5）宽限，不想被打断就 `--skip xovi-apply`。重启后核对 `is-active` / `MainPID` 是否变化 / `NRestarts` 不增 / 各扩展在 `maps` 里的段数。

## 本目录文件导览

| 文件 | 职责 |
|---|---|
| `install-all.sh` / `uninstall-all.sh` | 统一安装/卸载编排；步骤表来自 `lib.sh`，卸载对每个非"配置覆写/纯动作"步骤都必须有 `uninstall_<步骤名>` 函数（测试会核对） |
| `lib.sh` | host 侧共用库：`rssh`/`rssh_in`/`rscp`（统一 `BatchMode` + `ConnectTimeout`，`CJ_SSH_TIMEOUT` 缺省 8 秒，设备休眠/断线时快速失败而不是卡死）、`shquote`（把任意字符串安全拼进远端命令行）、`dev_script`（把 `devlib.sh` + heredoc 脚本体经 `ssh sh -s` 送上设备执行）、`push_verified`（scp 到暂存路径后逐个 md5 对拍，不对就删暂存并失败，绝不落到最终位置）、步骤表 `STEP_ORDER`/`STEP_DEFER`/`STEP_CONFIG_ONLY`、参数解析、固件安全门 |
| `devlib.sh` | **设备侧**共用库（POSIX sh，兼容 busybox）：rootfs 读写窗口（remount rw 后无论成败/被信号打断都恢复 ro）、`cj_safe_replace` 原子替换（先写暂存再 rename，不在运行中进程已映射的 inode 上原地写）、备份与轮转、`/usr` 下单元的安装/删除（先过 dm-verity 门）、`cj_xochitl_apply`/`cj_xochitl_health` |
| `deploy-usr-unit.sh` | 把一个 systemd 单元装进设备 `/usr` 的统一部署器；`deploy-chrony-boot-wakelock.sh` / `deploy-xovi-persist.sh` / `deploy-wifi-watch.sh` 是它的薄包装 |
| `deploy-xovi-ext.sh` + `xovi-ext-install.sh` | 装一个"独立最小 xovi 扩展"的统一部署器（host 侧构建+推送）与设备侧安装流程；`deploy-hl-snap.sh` / `deploy-handwriting-stroke.sh` 是薄包装，各扩展的 `deploy/install.sh` 只剩数据（名字、配置键）并 source `xovi-ext-install.sh` |
| `deploy.sh` | shelf 整包部署：组载荷（`bin/ systemd/ lo-alias/ xovi/ install.sh uninstall.sh manifest.sh devlib.sh`）→ 本地打成 tar → 推到设备 `shelf-pkg.new`，校验有 `install.sh` 后才换掉 `shelf-pkg` → 设备端 `install.sh`；`--password` 经标准输入走 0600 临时文件，不上命令行 |
| `deploy-battop.sh` / `deploy-sidebar-entry.sh` / `deploy-xovi-apply.sh` / `deploy-chrony-cn.sh` / `deploy-timezone-cn.sh` | 各自的部署脚本 |
| `firmware-allowlist.txt` / `firmware-allowlist.local.txt` | 固件白名单：仓库里被 git 跟踪的一份 + 本机一份（`--force` 追加到后者，已 gitignore） |
| `wifi-watch/` | 看护脚本与单元；`xovi-reenable.service`、`chrony-boot-wakelock.service`、`sidebar-entry-*.qmd`/`.qrc`/`.png`、`chrony-cn.sh`、`timezone-cn.sh` 是其余步骤的载荷 |
| `tests/` | 本机模拟测试，见「测试」 |

**脱离编排手动跑设备端 `install.sh` 时的文件依赖**（只手拷单个 `install.sh` 不够，会清楚报错）：`shelf/install.sh` 要同目录的 `manifest.sh` 与 `devlib.sh`（`deploy.sh` 已一起打进载荷）；`hl-snap`/`handwriting-stroke` 的 `deploy/install.sh` 要同目录的 `xovi-ext-install.sh` 与 `devlib.sh`（`deploy-hl-snap.sh` 等会一起推送）；`battop/install.sh` 要同目录的 `devlib.sh`。shelf 载荷里的 `manifest.sh`（安装/卸载共用的清单）与 `devlib.sh` 会被装到设备的 `~/.local/lib/shelf/`，`shelf-uninstall` 运行时 source。

## 固件安全门

装前 ssh 拉设备上 `/usr/bin/xochitl` 的 sha256，跟白名单比对：**仓库里的 `firmware-allowlist.txt`（每行 `sha256  <人读标签>`）+ 本机的 `firmware-allowlist.local.txt`**，命中任一份才继续；不命中默认拒装。确认要装用 `--force`，当前哈希追加进**本机**文件（已 gitignore，不改被 git 跟踪的白名单，避免"未验证的哈希"被误提交；路径可用 `CJ_ALLOWLIST_LOCAL` 覆盖）。为什么用 sha256 而不是版本号，见 `firmware-allowlist.txt` 内注释；当前白名单里有 3.28.0.172（现役）与 3.28.0.169（历史记录）两行。

## 备份与幂等

这是备份规则的权威说明（用户向文档只放一句话版）：

- 设备上被覆盖的旧文件统一备份进 `/home/root/cangjie-backups/`（**绝不留在 `extensions.d/` 里**——xovi 把该目录下任意文件当扩展加载，同名重复注册是致命错误，2026-08-15 踩过）；单文件 `<basename>.bak.pre-<时间戳>`，目录型（shelf）`shelf-<时间戳>/`。
- 只保留最近 5 份（设备端环境变量 `CJ_BACKUP_KEEP`），只轮转脚本自己生成的严格时间戳命名备份；超过 `CJ_BACKUP_MAXBYTES`（64MB）的单文件备份不自动删；手工命名的备份与任何用户数据不碰；全程无 `rm -rf`。
- 内容没变就不备份、不重启服务；写 `/usr` 前先过 dm-verity 门，激活则跳过。

## xovi-reenable.service 为什么放在 packaging/，不放 shelf/

xovi 持久化是**整个 xovi 层**通用的（重跑 `xovi/start` 会重注入全部扩展，不分 shelf / enhance / 其它），`shelf` 耦合它会破坏"网关+领域服务独立可插拔"的原则。2026-09-03 曾有一版把它装进 `shelf/install.sh`，被撤回（见 `shelf/docs/reMarkable书架白皮书.md` §03o，历史文档不改写）；`shelf/install.sh` 头注也写着这层该由"整包"装。`packaging/` 正是这层的归属。

## 卸载（`uninstall-all.sh`）

对称卸载，步骤表与安装共用。每一步：

| 步骤 | 卸载动作 |
|---|---|
| `chrony-boot-wakelock` / `xovi-persist` / `wifi-watch` | 停用并删 `/usr` 单元（同一套 dm-verity 门 + 带 trap 的 rw 窗口）；`wifi-watch` 另删 `~/.local/bin/wifi-watch.sh` |
| `hl-snap` / `handwriting-stroke` | 从 `extensions.d` 摘除 `.so`（含 `.crashed` 标记）；不碰 `reading-qol.json`（多个扩展共用）、不碰 `cangjie-backups/` |
| `sidebar-entry` | 从 qt-resource-rebuilder `exthome` 摘除 qmd/rcc |
| `battop` | 停用并删单元；`--purge` 才连 `/home/root/battop`（二进制 + 历史采样数据）一起删 |
| `shelf` | **优先**调设备上的 `~/.local/bin/shelf-uninstall`（`install.sh` 每次更新它，单一事实源），没有才退回 `shelf-pkg/shelf/uninstall.sh`；清单与安装共用 `manifest.sh`；默认保留用户数据，`--purge` 不作用于 shelf |
| `chrony-cn` / `timezone-cn` / `xovi-apply` | 配置覆写与纯动作，没有卸载语义，不动 |

摘掉 xovi 内容后当前运行中的 xochitl 仍是旧映射，要等下次重启才真正停止生效；卸载脚本不主动重启（要重启：`systemctl restart xochitl`，xovi 已生效时别用 `xovi/start`）。

## 测试：不碰真机验证脚本

```sh
bash packaging/tests/run_sim_tests.sh      # 也由 packaging/tests/test_install_scripts_sim.py 的 pytest 调用，CI 会跑
```

做法：`tests/stubs/` 下放假的 `ssh`/`scp`/`systemctl`/`mount`/`dmsetup`/`id`/`sleep`/`curl`/`journalctl`/`rcc` 等塞进 `PATH`，用临时目录当"设备"；假 `ssh` 把远端命令直接在本机沙箱里执行，所以设备端脚本（`devlib.sh`、`shelf/install.sh`、各 heredoc 脚本）跑的是**真代码**，只是 rootfs/systemd/mount 被桩住并写日志，可断言"有没有 remount rw、最后一次 mount 是不是 ro、有没有跑 `xovi/start`"。

覆盖：`devlib` 各函数；shelf 安装的幂等 / 缺载荷不留半成品 / rw 窗口失败恢复 ro / 只重启有变化的服务；shelf 卸载与安装清单对称；`deploy.sh`（密码含特殊字符、`shelf-pkg` 换位）；hl-snap 部署（原子落位、备份不进 `extensions.d`）；`xovi-apply` 与 `sidebar-entry` 的"xovi 已生效 → restart、绝不 `xovi/start`"判定；整轮 `install-all` → `uninstall-all` 对称；静态守卫（`remount,rw` / `xovi/start` 只许出现在库里）。**拒绝以 root 运行**（设 `CJ_SIM_ALLOW_ROOT=1` 才强行跑）。这些是本机模拟，**不能代替真机验证**。

## 固件升级（OTA）之后

恢复流程、逐项对照表与流程图统一放在 [`../docs/INSTALL.md`](../docs/INSTALL.md#固件升级ota之后)（本文不再另写一份）。要点：先设备旁手动 `xovi/rebuild_hashtable`，再在电脑上重跑 `sh install-all.sh <host>`（新固件哈希不在白名单要 `--force`）。

## 明确不做的事（已知缺口，别当成已经解决）

- **不装 vellum/xovi/qt-resource-rebuilder/appload 本体、不侧载 KOReader**——手动前置条件，缺失时子脚本清楚报错，`install-all.sh` 收尾摘要再提醒一次。
- **不装中文化**（输入法/候选栏/词典/UI 汉化）——那条链路（`chinese-ime/langhook/`）随 2026-09-11 全仓库大归档挪出了 git 仓库，不随本安装器分发；设备上已部署的部分仍在运行。
- **不升级 appload**——3.28 需要 ≥ 0.6.0，已装旧版的自己 `vellum upgrade appload` 后整机重启。
- **`chrony-cn` / `timezone-cn` 没有卸载语义**（配置覆写）。
- **卸载的真机效果没有验证过**：`uninstall-all.sh` 只做过本机模拟测试，需要在已装过 `install-all.sh` 的设备上跑一遍，确认各单元/扩展确实被摘掉、shelf 服务确实停用，且不影响没被点名要卸的其它功能。
- 旧的 `packaging/package.sh`（打 `cangjie-full-*.tar.gz` 单体安装包）**没有**恢复：它已随归档挪出仓库且按旧目录结构找载荷；本目录是纯编排现有独立脚本，不是复刻旧的单体打包架构。

---

## 验证现状（如实说明，不夸大）

> 这是按时间累积的真机验证与踩坑整理（2026-09-11 起）。步骤号、"八步/十个脚本"等是当时的说法，**以上面的步骤表为准**；2026-09-20 脚本重构（共用 `lib.sh`/`devlib.sh`、原子替换、备份轮转、xovi 已生效走 `systemctl restart`）之后个别做法已被取代（如 battop 部署"先 stop 再 scp 覆盖"→ 现在推 `battop.new` 后原子 rename）。**本文档没有记录 2026-09-20 重构之后对整轮 `install-all.sh` 的真机复验**，重构后的脚本主要靠上面的本机模拟测试。

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

### 没有真机验证

- **卸载脚本**（`uninstall-all.sh`）：只有本机模拟。
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
- **旧版 battop 重装 ETXTBSY**：`scp` 直接覆盖正在执行的二进制被内核拒绝。→ 曾改成"推送前先 stop"，2026-09-20 之后改为推 `battop.new` 再原子 rename（不必停服务）。
