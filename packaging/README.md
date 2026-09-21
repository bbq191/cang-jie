# packaging —— 全新设备统一安装器

> **读者与用途**：要给设备装/卸/更新这套增强的人，以及要改这些脚本的维护者。这里讲**脚本的结构、每一步做什么、目录里各文件的职责、怎么本机测试**；
> "第一次怎么装、风险有哪些、固件升级后怎么恢复"这类面向使用者的说明在 [`../docs/INSTALL.md`](../docs/INSTALL.md)（中文）/ [`INSTALL.en.md`](../docs/INSTALL.en.md)。
> 末尾的「验证现状」是按时间累积的真机验证与踩坑记录，读现状先看前面的章节。

host 侧编排层。全新（或愿意重装的现有）reMarkable Paper Pro Move，固件跟 `firmware-allowlist.txt` 对得上时，一条命令装完当前仓库能装的一切：

```sh
cd packaging
sh install-all.sh <host>                                        # 装全部
sh install-all.sh <host> --force                                # 固件不在白名单也强装（哈希追加进本机 firmware-allowlist.local.txt）
sh install-all.sh <host> --skip chrony-cn,timezone-cn,xovi-persist   # 跳过指定步骤（写错名字会警告，不会静默忽略）
```

`<host>` 默认 `10.11.99.1`（USB 网段）。反悔想卸：

```sh
sh uninstall-all.sh <host>                                       # 卸全部（chrony-cn/timezone-cn/xovi-apply 除外，见下）
sh uninstall-all.sh <host> --purge                                # 卸的同时连 battop 历史数据一起删
sh uninstall-all.sh <host> --skip shelf                            # 跳过指定步骤，用法同 --skip
```

## 前置条件（全新设备，需手动，本脚本不代装）

以下几样是 reMarkable 官方/`vellum`/`appload` 生态自己的东西，不属于这个仓库，`install-all.sh` **不会**帮你装，缺了会在对应步骤报清楚的错误：

1. **`vellum add xovi`**——xovi 本体（`xovi-persist`/`hl-snap`/`handwriting-stroke`/`xovi-apply` 的硬前提）。
2. **`vellum add qt-resource-rebuilder`**——`shelf` 里 `font`/`book` 的字体菜单、回收站/建夹代理，以及 `sidebar-entry` 这几个可选特性依赖它；缺了这些特性自动跳过，不阻塞其它安装。
3. **`vellum add appload`**——第三方 App 加载器，KOReader 要通过它侧载；`sidebar-entry` 那步靠它暴露的 `AppLoadLauncher` 单例发起启动，缺了自动跳过。**⚠ 3.28 固件需要 appload ≥ 0.6.0**——0.5.3 及更早版本自带的内嵌 qmd 钩的是 3.27 的旧 Sidebar/MainView 锚点，3.28 已经改名，注入失败（症状：`AppLoadLauncher` 单例建不起来，`sidebar-entry` 装的按钮点了没反应）。上游 PR #59（3.28 支持）已并入 **v0.6.0（2026-09-19）**，`vellum add/upgrade appload` 拿到的就是它，2026-09-21 真机验证过（md5 与官方发布包一致、日志有 "Loaded external AppLoad hooks in main UI"、KOReader/WeRead 入口正常）；此前的"等长回填 qmd"补丁工具已删除。已装旧版的先 `vellum upgrade appload`，**升完整机重启，别 `systemctl restart xochitl`**（旧进程退出时会崩溃、触发 `OnFailure=emergency.target` 整机重启，2026-09-21 踩到）。`sidebar-entry` 那步仍然只探测开机日志、探测不到就跳过。
4. **KOReader**（经 appload 侧载）——`shelf` 的 `koreader-serve` 只是管理/配置这个已装好的 KOReader，不负责把 KOReader 本身装上去；`sidebar-entry` 那步的「KOReader」入口同理，点了没反应说明这一步没做。

装好以上四样、再跑 `install-all.sh`，才是完整的"全新设备"安装顺序。**可选、不算前置条件**：**WeRead**（第三方 reMarkable 版微信读书 app）——要装得自己下载官方发行包 SSH 装；`sidebar-entry` 会自动探测装没装，装了就把 Sidebar 入口换成「KOReader + WeRead」两项版本，没装就只有「KOReader」一项，不会因为没装 WeRead 而报错或跳过整步。

## 装什么、按什么顺序

`install-all.sh` 只编排，不重新实现任何构建/传输逻辑——先过固件安全门，再按 `lib.sh` 里的**步骤表 `STEP_ORDER`** 依次调用十一个各自独立可用的部署脚本（`uninstall-all.sh` 共用同一张表，所以安装与卸载清单对称）：

| 顺序 | 步骤名 | 脚本 | 装什么 | 前置 |
|---|---|---|---|---|
| 1 | `chrony-cn` | `deploy-chrony-cn.sh` | 国内 NTP（chrony 服务器换成阿里云/腾讯云等） | 无，跟 xovi/vellum 完全无关 |
| 2 | `chrony-boot-wakelock` | `deploy-chrony-boot-wakelock.sh` | 开机头几十秒持一把 wakelock，防自动休眠打断 chronyd 首次校时（根因见「验证现状」） | 无；设备镜像缺 `/sys/power/wake_lock` 时优雅跳过 |
| 3 | `timezone-cn` | `deploy-timezone-cn.sh` | 默认时区设为 Asia/Shanghai | 无；缺 `/usr/share/zoneinfo/Asia/Shanghai` 时优雅跳过 |
| 4 | `battop` | `deploy-battop.sh` | 电池刺客（纯 Rust systemd 常驻采样服务）；装完 start、**有意不开机自启** | 无 |
| 5 | `wifi-watch` | `deploy-wifi-watch.sh` | WiFi 载波假死看护（`wifi-watch/`：脚本 → `~/.local/bin`，单元 → `/usr`）；链路正常时只读 sysfs carrier、零 fork | 无 |
| 6 | `xovi-persist` | `deploy-xovi-persist.sh` | xovi 开机持久化恢复链（`xovi-reenable.service`） | 设备已 `vellum add xovi` |
| 7 | `hl-snap` | `deploy-hl-snap.sh` | 荧光笔 CJK 精确吸附（独立最小 xovi 扩展）——只落盘 | 同上 |
| 8 | `handwriting-stroke` | `deploy-handwriting-stroke.sh` | CJK 手写笔迹渲染优化（独立最小 xovi 扩展）——只落盘 | 同上 |
| 9 | `sidebar-entry` | `deploy-sidebar-entry.sh` | Sidebar 一级直达「KOReader」入口（装了 WeRead 就自动带上「WeRead」项）——只落盘 | 设备已 `vellum add qt-resource-rebuilder` + `vellum add appload`（且 appload 在这台固件上验证过能正常挂载，见上面「前置条件」第 3 条）；任一条件不满足自动跳过 |
| 10 | `shelf` | `deploy.sh` | 网关 + book/koreader/font/wallpaper 四个领域服务 + 笔记线（ink/transcribe/mind/note） | 无（字体菜单、回收站/建夹代理 qmd 依赖 `qt-resource-rebuilder`，缺了自动跳过，只落盘） |
| 11 | `xovi-apply` | `deploy-xovi-apply.sh` | 统一让上面落盘的扩展/qmd 生效：重启 xochitl 一次 + 健康检查 | 同 6/7/8/9 |

### 为什么 hl-snap / handwriting-stroke / sidebar-entry 只落盘、最后统一重启

xovi 没有"只重载一个扩展"的机制，让新扩展/qmd 生效的唯一办法是重启 xochitl。如果每一步各自重启，短时间内重启多次会撞 xochitl 自带的 watchdog + StartLimit——真机验证过连续两次就触发了一次意外整机重启（2026-09-11）。所以 `install-all.sh` 给这三步传 `DEFER_XOVI_START=1`（设备端 `install.sh --no-restart`）让它们只落盘，`shelf` 的 qmd 本来就只落盘，全部落盘完在最后由 `xovi-apply` 统一重启一次。单独跑这几个脚本（不设这个环境变量）行为不变——落盘后立即重启并做健康检查。

### 怎么"重启"xochitl：`cj_xochitl_apply` 的判定（2026-09-20）

重启不是无条件 `xovi/start`：设备端 `devlib.sh` 的 `cj_xochitl_apply` 先看运行中的 xochitl 进程 `LD_PRELOAD` 里有没有 `xovi.so`——**已生效 → `systemctl restart xochitl`**；**没生效（刚开机/OTA 之后）→ `xovi/start`**。因为在 xovi 已生效时跑 `xovi/start` 会 umount 再重挂 xochitl 的 drop-in 目录，运行中的 xochitl 读文件失败 SEGV，系统按设计整机自动重启（2026-09-20 真机事故；旧版无条件 `xovi/start`，重跑 `install-all.sh` 必踩）。重启前会先打印"将打断阅读"并留 `CJ_APPLY_GRACE` 秒（默认 5）宽限，不想被打断就 `--skip xovi-apply`。重启后核对 `is-active` / `MainPID` 是否变化 / `NRestarts` 不增 / 各扩展在 `maps` 里的段数。

十一个脚本都可以单独跑（`sh deploy-battop.sh <host>` 等），不依赖 `install-all.sh`——它只是把它们串起来 + 加一层固件门 + 汇总结果。任何一步失败：打印清楚是哪一步、原始错误，**不自动重试、不静默跳过**，退出非零。

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
| `tests/` | 本机模拟测试，见下 |

设备侧共享库的另一处消费者：shelf 载荷里带 `manifest.sh`（安装/卸载共用的清单）与 `devlib.sh`，`install.sh` 会把它们装到设备的 `~/.local/lib/shelf/`，`shelf-uninstall` 运行时 source。

## 固件安全门

装前 ssh 拉设备上 `/usr/bin/xochitl` 的 sha256，跟白名单比对：**仓库里的 `firmware-allowlist.txt`（每行 `sha256  <人读标签>`）+ 本机的 `firmware-allowlist.local.txt`**，命中任一份才继续。不命中默认拒装（避免在没验证过注入定位的固件上装错，qmd/hook 偏移错了轻则功能不生效重则设备行为异常）；确认要装用 `--force`，当前哈希追加进**本机**文件（已 gitignore，不再改动被 git 跟踪的白名单，避免"未验证的哈希"被误提交；路径可用 `CJ_ALLOWLIST_LOCAL` 覆盖）。为什么用 sha256 而不是版本号，见 `firmware-allowlist.txt` 内注释。

## 备份与幂等

- 设备上被覆盖的旧文件统一备份进 `/home/root/cangjie-backups/`（**绝不留在 `extensions.d/` 里**——xovi 把该目录下任意文件当扩展加载，同名重复注册是致命错误，2026-08-15 踩过）；单文件 `<basename>.bak.pre-<时间戳>`，目录型（shelf）`shelf-<时间戳>/`。
- 只保留最近 5 份（设备端环境变量 `CJ_BACKUP_KEEP`），只轮转脚本自己生成的严格时间戳命名备份；超过 `CJ_BACKUP_MAXBYTES`（64MB）的单文件备份不自动删；全程无 `rm -rf`。
- 内容没变就不备份、不重启服务；写 `/usr` 前先过 dm-verity 门。

## xovi-reenable.service 为什么放在 packaging/，不放 shelf/

`shelf/docs/reMarkable书架白皮书.md` §03o 记过一次相关的历史决策（历史文档，不改写，这里只
引用结论）：2026-09-03 曾经有一版把 `--with-xovi-reenable` 装进 `shelf/install.sh`、单元源放
`reading/device-rs/systemd/`，被撤回——理由是 xovi 持久化是**整个 xovi 层**通用的（重跑
`xovi/start` 会重注入全部扩展，不分 shelf/中文化/enhance），`shelf` 耦合它反而破坏"网关+领域
服务独立可插拔"的原则。`shelf/install.sh` 现在的头注也仍然写着这层该由"整包"装。`packaging/`
正是这次大归档后 `install-on-device.sh` 的精神继承者，装它是把当初就规划好、只是归档时连带
消失的一层补回来，不是重新踩同一个耦合坑。

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

做法：`tests/stubs/` 下放假的 `ssh`/`scp`/`systemctl`/`mount`/`dmsetup`/`id`/`sleep`/`curl`/`journalctl`/`rcc`/`python3` 塞进 `PATH`，用临时目录当"设备"；假 `ssh` 把远端命令直接在本机沙箱里执行，所以设备端脚本（`devlib.sh`、`shelf/install.sh`、各 heredoc 脚本）跑的是**真代码**，只是 rootfs/systemd/mount 被桩住并写日志，可断言"有没有 remount rw、最后一次 mount 是不是 ro、有没有跑 `xovi/start`"。覆盖：`devlib` 各函数、shelf 安装的幂等/缺载荷不留半成品/rw 窗口失败恢复 ro/只重启有变化的服务、shelf 卸载与安装清单对称、`deploy.sh`（密码含特殊字符、`shelf-pkg` 换位）、hl-snap 部署（原子落位/备份不进 `extensions.d`）、`xovi-apply` 与 `sidebar-entry` 的"xovi 已生效 → restart、绝不 `xovi/start`"判定、整轮 `install-all` → `uninstall-all` 对称，以及静态守卫（`remount,rw` / `xovi/start` 只许出现在库里）。**拒绝以 root 运行**（设 `CJ_SIM_ALLOW_ROOT=1` 才强行跑）。这些是本机模拟，**不能代替真机验证**。

## 固件升级（OTA）之后

恢复流程、逐项对照表与流程图统一放在 [`../docs/INSTALL.md`](../docs/INSTALL.md)「固件升级（OTA）之后」一节（本文不再另写一份）。要点：先设备旁手动 `xovi/rebuild_hashtable`，再在电脑上重跑 `sh install-all.sh <host>`（新固件哈希不在白名单要 `--force`）。

## 明确不做的事（已知缺口，别当成已经解决）

- **不装 vellum/xovi/qt-resource-rebuilder/appload 本体、不侧载 KOReader**——这是所有脚本共同的手动前置条件，见上面「前置条件」一节，本脚本不代为安装，缺失时子脚本会清楚报错，`install-all.sh` 收尾摘要会再提醒一次。
- **不装中文化**（输入法/候选栏/词典/UI 汉化）——那条链路（`chinese-ime/langhook/`）随 2026-09-11 全仓库大归档挪出了 git 仓库，不随本安装器分发；设备上已部署的部分仍在运行。
- **不升级 appload**——3.28 需要 ≥ 0.6.0，已装旧版的自己 `vellum upgrade appload` 后整机重启，不在编排里。
- **`chrony-cn` / `timezone-cn` 没有卸载语义**（配置覆写），见上面「卸载」。
- **卸载的真机效果没有验证过**：`uninstall-all.sh` 只做过本机模拟测试（假 host 验证参数解析/`--skip`/摘除动作/与安装对称），需要用户在已装过 `install-all.sh` 的设备上跑一遍，确认各单元/扩展确实被摘掉、shelf 服务确实停用，且不影响没被点名要卸的其它功能。
- 旧的 `packaging/package.sh`（打 `cangjie-full-*.tar.gz` 单体安装包那套）**没有**恢复——它已随归档挪出仓库且按旧目录结构找载荷，不是 `install-all.sh` 的设计参照；这次是纯编排现有独立脚本，不是复刻旧的单体打包架构。


> **⚠ 下面「验证现状」是按时间累积的真机验证与踩坑记录（2026-09-11 起）**，其中的步骤号、"八步/十个脚本"等是当时的说法，以上面的步骤表为准；2026-09-20 脚本重构（共用 `lib.sh`/`devlib.sh`、原子替换、备份轮转、xovi 已生效走 `systemctl restart`）之后，个别做法（如 battop 部署"先 stop 再 scp 覆盖"）已被取代，保留原文作依据。

## 验证现状（如实说明，不夸大）

**真机验证过、确认能跑通的部分**：固件安全门（3.28.0.172 真机 sha256 命中白名单）；
`deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh` 两步整个流程（构建→推送→设备端安装→
`journalctl` 确认 hook 已加载、`is-active`=active、`NRestarts`=0）；`install-all.sh` 整个
八步全部跑通（用户 2026-09-11 确认"已成功安装"，当时还没有 `sidebar-entry` 这步）。

**2026-09-15 补（全量代码审查批2，涉及 xovi/mprotect 高危代码路径）——已用真机验证通过**：
- `enhance/hl-snap/src/hl_snap.c`/`enhance/handwriting-stroke/src/hw_stroke.c` 各自约 50 行
  逐字节重复的通用 trampoline 安装代码（`patch_target`/`make_call_through_stub`）收进
  `enhance/shared/trampoline_patch.c`（新文件，见该目录 `PROVENANCE.md`），两边改成调用
  `cj_patch_target(target, handler, PATCH_LEN, tag, &stub)`，纯参数化提取、不改任何逻辑。
  **真机验证**：`make aarch64`（真实 Makefile，走已提交的 `xovi_glue.c`，非手动模拟）编出
  两个新 `.so`，经 `deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh` 部署到真机，
  `journalctl` 核对最终存活的 xochitl 进程上 `[hl-snap] 荧光笔EXPAND hook 安装完成`、
  `[hw-stroke]` 三个 hook（变宽几何/第二几何/分派诊断）全部"安装完成"，日志格式/行为跟
  重构前逐字节一致；两次连续重启 `NRestarts` 全程 `0`；`reading-qol.json` 里用户原有配置
  原样保留没被覆盖。
- `deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh`/`deploy-battop.sh` 补齐"部署前备份+
  md5 校验"（`deploy-sidebar-entry.sh` 一直有这两步，另外几个部署脚本当时漏了）：host 侧
  推送后 md5 核对本地/远端文件一致；hl-snap/hw-stroke 的设备端 `install.sh` 在覆盖
  `extensions.d/` 里的旧 `.so` 前先备份到 `$HOME/cangjie-backups/`（**绝不能**备份在
  `extensions.d/` 里，xovi 会把目录下任意文件当扩展加载，见 工程纪律 记录的教训）；
  battop 的二进制不在 extensions.d，备份改在 host 侧 scp 覆盖前、通过 ssh 在同目录里做。
  **真机验证**：两次部署 host 侧 md5 校验都显示"一致"；设备端 `ls`/`md5sum` 确认
  `/home/root/cangjie-backups/` 下真的新增了 `hl-snap.so.bak.pre-20260915-100340`/
  `hw-stroke.so.bak.pre-20260915-100429`，且备份文件的 md5 精确匹配部署前抓的旧版本
  基线（不是误备份成刚部署的新版本）。battop 那条host 侧 scp 前备份分支这次没有触发
  （没有重跑 `deploy-battop.sh`），仍待下次装 battop 时顺带确认。

`deploy-sidebar-entry.sh`（2026-09-13 新写，从 §「明确不做的事」上一版遗留的空白里补上）：
独立跑过两条路径都真机通过——① 默认模式（这台设备当时已装 WeRead）：探测到
`qt-resource-rebuilder` 存在→探测到 WeRead 已装→选中两项版 qmd→本地 `rcc` 编译→推送→
md5 校验一致→`xovi/start`→`journalctl` 确认 `CJ-SIDEBAR[8]: KOReader`/`CJ-SIDEBAR[9]:
WeRead`、`NRestarts=0`；② `DEFER_XOVI_START=1` 模式：同样的探测+推送+校验，最后打印"只落盘
不跑 xovi/start"就退出，没有触发 xochitl 重启（人工核对期间 xochitl 进程没变化）。

**2026-09-14 补验（零风险分支）：qt-resource-rebuilder / appload 缺失这两条跳过分支——已用
真机验证通过**。两条检查都在做任何实际操作（本地 `rcc` 编译/`scp`/`xovi/start`）之前就
`exit 0`，真机上临时把对应目录改名挪开（`mv .../qt-resource-rebuilder{,.testmove}` 等）、
跑本脚本、确认打印跳过信息+`exit=0`+没有触发任何后续步骤、再把目录名改回来——全程可逆，
没有碰 xochitl。

**2026-09-14 补验（有真实代价的分支）：没装 WeRead 时退回单项 qmd——已用真机验证通过**。
这条不是跳过退出，是真的换一份 qmd 并重启一次 xochitl，测完还要测回去，一共两次真实重启：
① 把 `/home/root/.local/opt/remarkable-weread` 整个目录改名挪开，跑本脚本——正确探测到
"没装 WeRead"、选中 `sidebar-entry-koreader-only.qmd`，真实推送+重启，`journalctl` 按
新 `MainPID` 过滤确认这次启动的 `CJ-SIDEBAR` 列表里**只有** `[8]: KOReader`、没有 WeRead
那一项；② 把目录改回原名，再跑一次本脚本——正确探测回"装了 WeRead"、换回两项版，
`journalctl` 按这次新 `MainPID` 过滤确认 `[8]: KOReader`+`[9]: WeRead` 都在，设备恢复到
测试前的原始状态。两次重启都健康（`is-active=active`、`NRestarts=0`、`MainPID` 有变化），
间隔约一分钟，没有连续触发到 StartLimit。

**仍然没有真机验证过的**：`sidebar-entry` 在 appload 版本过旧时"检测不到信号就跳过"这一分支——检查的是"这次开机 journal 里有没有那行日志"，逻辑只是一行 `grep -q`，复杂度低，靠代码审查；真机日志只确认了"正面信号确实存在"这一半（2026-09-21 appload 0.6.0 上）。

**`xovi-persist` 核心承诺——已用真机重启证实**：`packaging/xovi-reenable.service` 装完后，
那台设备真的经历过一次整机重启（见下面"真机第一轮实测暴露的真坑"那条 watchdog+StartLimit
触发的意外重启），重启后 `systemctl status xovi-reenable.service` 显示
`Active: active (exited)`、`Main PID: ... (code=exited, status=0/SUCCESS)`，journal 里
`Starting → Finished` 完整走了一遍——这是系统在开机时**自动**触发的（手动跑 `xovi/start`
不会经过这个 systemd 单元），且当时运行中的 xochitl 进程 `grep -c xovi.so .../maps` 返回
非零，证实 xovi 确实被自动重新注入了，不需要人手动跑一次 `xovi/start`。
⚠️ **已知无害的显示怪癖**：`systemctl status` 同时显示 `Loaded: ...disabled; preset:
disabled`——这不代表没生效，是"手写 `/usr/lib/systemd/system/multi-user.target.wants/`
软链装单元"这个安装方式的已知 `is-enabled` 显示怪癖（不是走 `systemctl enable` 命令生成的
软链，systemd 判定"是否 enabled"的标签逻辑对不上，但开机是否激活看的是 `.wants/` 目录里
软链在不在，这条已经真机验证过在），别被这个标签误导成"没装上"。

**真机跑过、发现问题、已修但改动本身还没有复验的部分**：
- `deploy-battop.sh`：重装（非首次装）时若 `battop.service` 已在跑，`scp` 直接覆盖正在执行的
  二进制被内核拒绝（`ETXTBSY`，报 `scp: dest open ... Failure`）。修法：推送前先
  `systemctl stop battop.service`（`install.sh` 最后会自己重新 `enable --now`）。
- `deploy.sh`（原 `shelf/deploy.sh`）：健康检查原来固定 `sleep 1` 就查 `systemctl is-active`，
  `gateway` 首次启动要签发私有 CA/自签证书，1 秒不够、还在 `activating` 就被判定为"没起来"。
  修法：`shelf/install.sh` 的健康检查改成轮询（最多等 10 秒）。

**离线验证过、后来经真机确认整体跑通、但没有单独逐项复核细节输出的部分**：
- `deploy-xovi-apply.sh`：整体流程（`xovi/start` + 健康检查）已经随 `install-all.sh` 整轮
  真机跑通，但没有单独逐项核对过它自己打印的 hl-snap/hw-stroke 段数、NRestarts 这些细节
  输出，只知道整轮成功、xovi.so 确实加载了（见上面 xovi-persist 那条 `grep -c xovi.so`）。

真要下"逐项都已验证"的结论，还需要用户后续贴出更细的单步输出再回来补充。

**2026-09-14 补验：`chrony-cn.sh`/`timezone-cn.sh` 的改写分支 + 重启持久化——已用真机验证**。
此前（2026-09-13）只验证过两者的幂等跳过分支，这次故意把设备状态改回非目标状态再验证"改写"
这个动作本身：
- `chrony-cn.sh`：手动把 `/etc/chrony.conf`（含 rootfs 底层）改成 `time1/2.google.com`，跑本脚本——
  正确检测到不是国内配置、改写、重启 `chronyd`、当次打印 `✅ 时钟已同步`（源选中
  `cn.pool.ntp.org`）。重启设备后复核：`/etc/chrony.conf` 仍是国内四个服务器（没有打回
  google），改写持久化成立。
- `timezone-cn.sh`：手动把 `/etc/localtime`（含底层）改回 `UTC`，跑本脚本——正确检测、改写、
  当场生效（`date` 立刻显示 CST）。重启设备后复核：`readlink /etc/localtime` 仍指向
  `Asia/Shanghai`，持久化成立。**2026-09-14 补验**：`/usr/share/zoneinfo/Asia/Shanghai`
  缺失时的优雅跳过分支——临时把这份 zoneinfo 文件改名挪开（remount rw）、跑 `timezone-cn.sh`，
  确认打印"⚠ 设备镜像没有...跳过时区设置"+`exit=0`，没有动 `/etc/localtime`；随后改回原名、
  确认 `readlink -f /etc/localtime` 恢复正常——全程可逆，真机验证通过。

**同一轮意外发现的新现象，2026-09-14 当天追查到根因并修好（`chrony-boot-wakelock.service`，
已加进第 2 步）**：重启后 `chrony-cn.sh` 写好的国内 NTP 服务器**配置本身没问题**（真机核对过
`date -u`/`hwclock -r` 跟宿主机 UTC 时间分毫不差），但 `timedatectl` 的 `System clock
synchronized` 标志在重启后卡在 `no` 长达 10 分钟以上没有恢复。追查 `journalctl -u chronyd`：
`chronyd` 选中源后打出 `System clock wrong by 1.13 秒`，紧接着 `Forward time jump
detected!` → `Can't synchronise: no selectable sources`，此后每轮重试间隔越拉越长
（2m21s→3m19s→5m11s→5m04s，典型失败退避）。

**根因（查了 chrony 上游邮件列表 [chrony-users 2020-10 帖](https://listengine.tuxfamily.org/chrony.tuxfamily.org/chrony-users/2020/10/msg00019.html)，
维护者 Miroslav Lichvar 原话确认这条消息的触发条件：系统钟被 chronyd 自己控制之外的东西
"移动"了一下，chronyd 就会整个丢弃已有测量重新开始——常见场景正是笔记本挂起/唤醒）**：
真机 `journalctl -k` 核对同一台设备同一次重启的内核日志，"Forward time jump detected!"
第一次出现的时刻，跟内核 `PM: suspend entry (deep)` / `suspend exit` 完全对上——根因是
reMarkable 官方 `remarkable-enable-slumber.service`（`After=xochitl.service`）开机后几十秒
内就 `echo mem > /sys/power/autosleep` 打开自动休眠，这台设备离 USB/触屏几秒真的会挂起，
`chronyd` 刚选中源、正准备 `makestep` 校正系统钟的这几十秒窗口一旦撞上这次挂起/唤醒，就被
chrony 自己的连续性检查误判成"外部动了系统钟"，触发上面那条失败退避链。**不是** `chrony.conf`
配错、也不是这次改的 NTP 服务器列表的问题（这台设备改配置前本来就是这份 `rtcsync`+
`makestep 1.0 3` 配置，问题一直都在，只是这次才第一次在无人值守的重启后原样复现出来）。

**修法**：新增 `packaging/chrony-boot-wakelock.service`（+ `deploy-chrony-boot-wakelock.sh`，
已加进 `install-all.sh` 第 2 步）——不碰 `remarkable-enable-slumber.service` 本身（红线：改
核心服务的启动依赖/时机推演有误就是真机变砖级别的事故），只加一个独立的新单元：开机后持一把
标准 Linux wakelock（`/sys/power/wake_lock`），轮询 `timedatectl show -p NTPSynchronized`
最多 120 秒、一旦同步立刻放锁退出；`Before=remarkable-enable-slumber.service`（纯排序声明，
不是 `Requires=`）让自动休眠晚这几十秒才打开，不影响它最终生效。这个单元本身失败/超时/设备
缺 `/sys/power/wake_lock` 都不会拖累 `remarkable-enable-slumber.service` 或任何其它单元——
最坏情况退回没有这层保护之前的样子，不会引发级联故障。

**真机验证（2026-09-14）**：部署后重启，`journalctl -u chronyd -b` 这次全程**零** "Forward
time jump detected"；`cat /sys/power/wake_lock` 在 chronyd 完成首次同步前显示持锁中，同步
达成后自动放锁（服务 34 秒内 `Deactivated successfully`，早于 120 秒上限提前退出）；
`timedatectl` 显示 `System clock synchronized: yes`——从"卡 10+ 分钟"变成"重启后几十秒内
同步"。另外确认 `remarkable-enable-slumber.service` 紧接着正常触发、`cat /sys/power/autosleep`
仍是 `mem`——设备的自动休眠功能长期不受影响，只是这次开机延后了三十几秒才打开。

**真机第一轮实测暴露的真坑（已修，且改动本身已经在后续真机跑通中间接验证过）**：
- `timezone-cn.sh` 幂等分支（设备本来就已经是 Asia/Shanghai）打印"✅ 已是目标时区"却仍被
  `install-all.sh` 判定为失败——脚本最后一条语句写成 `[ "$changed" = "1" ] && echo ...`，
  `changed=0` 时这条 `[ ]` 测试本身为假，没有 `set -e` 不中断，但也没有后续语句覆盖 `$?`，
  脚本用这条判断的非零退出码收尾。改成显式 `if`+`exit 0`；用户之后跑通的整轮
  `install-all.sh` 没再报这一步失败，间接确认修好了。
- `deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh` 各自的设备端 `install.sh` 都会自己跑一次
  `xovi/start`——`install-all.sh` 连续调用这两步时，短时间内重启 xochitl 两次，真机撞上
  watchdog+StartLimit，触发了一次意外整机重启（`uptime` 显示重启后刚起来几分钟），紧接着的
  下一步 `handwriting-stroke` 因为设备正在重启窗口期撞上 `ssh: Connection refused`。改成
  见「为什么 hl-snap / handwriting-stroke / sidebar-entry 只落盘、最后统一重启」一节——两个设备端 `install.sh` 加 `--no-restart`，
  `install-all.sh` 新增 `deploy-xovi-apply.sh` 作为唯一的 `xovi/start` 调用点，放在最后。
  意外之喜：这次重启恰好把刚装的 `xovi-reenable.service` 现场测了一遍——**已经确认它真的在
  这次重启里自动重新加载了 xovi**（见上面"xovi-persist 核心承诺"那条，`systemctl status`
  显示自动触发+成功退出，运行中的 xochitl 也确实加载了 xovi.so）。

编排这几个脚本的过程中踩到两个真坑（离线阶段发现，已修）：
- `deploy-battop.sh` 最初用 `cargo build --manifest-path ../enhance/battop/Cargo.toml` 在
  `packaging/` 目录下直接调用——**Cargo 搜 `.cargo/config.toml`（CC/AR 覆盖）是按当前工作
  目录往上找，不看 `--manifest-path`**，导致 battop 的 CC 覆盖没生效，本机实测直接在链接
  `crt1.o` 这步报 `Relocations in generic ELF` 炸掉。改成先 `cd` 进 `enhance/battop/` 再跑
  `cargo build` 后立刻恢复正常。
- `deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh` 最初在构建前跑了 `make clean`——两个
  `.so` 都已提交进仓库（不用每次现建才能部署），但本机没有 `asivery/xovi` 的 clone、编不出
  新的，`make clean` 先把已提交的 `.so`/`xovi_glue.{c,h}` 删了，重编又失败，**结果是仓库里
  能用的产物被脚本自己删掉却没能力补回来**（当场发现、`git checkout HEAD --` 救回，没有提交
  这个状态）。改成不清、构建失败时退回用仓库里已提交的版本（缺 xovi clone 时会打印清楚的
  提示，而不是留一个空洞）。
