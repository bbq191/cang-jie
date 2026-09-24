# 电池 / 后台进程审计报告（2026-08-27）+ 08-29 冻机调查

> **这是历史调查记录**，数字和进程名反映 2026-08-27 的设备（当时还跑着 `wr-serve`、`cj-stars-daemon`、`wr-renew` 等已退役的服务）。
> battop 之后的演进（09-20 改为不开机自启、09-23 再次冻机后唤醒源改读 `/dev/kmsg`、采样循环不再创建子进程）见
> [`../docs/reMarkable系统增强线白皮书.md`](../docs/reMarkable系统增强线白皮书.md) §03b；battop 现在怎么用见 [`README.md`](README.md)。

**设备**:reMarkable Paper Pro Move,固件 20260806,uptime 1 天。
**日期**:2026-08-27。**方法**:功耗代理量(累计 CPU、生命期 CPU 占比、唤醒源、休眠健康、应用归属)。
脚本见 [`history/`](history/) 下的 `battery-audit.sh`(入口)+ `bataudit{,2,3}.sh`。

## 结论:无电池刺客

- **休眠健康**:一天 8 次 `PM: suspend entry` = 8 次 `exit`;journal **零** `Deferring suspend` / `Can't suspend`——没有任何进程阻止休眠。
- **无自旋**:load avg ≈ 0;自家 daemon(`cj-stars`/`wr-serve`)均 `S`(睡眠)态,非轮询。
- **WiFi**:`power_save: on`,审计时**未关联**(不连=不耗)。
- **最高非核心 CPU**:`memfaultd` 39 CPU 秒/天(生命期 0.04%),可忽略。
- **唯一高频定时器**:`wr-renew`(本项目 cookie 续期,每小时)。其余定时器均日级或已完成。

autosleep 机型:睡着时几乎不耗电,常驻进程多在睡,整体很省。

## 累计 CPU（1 天,前几名）

| CPU秒 | 生命期% | 进程 | 归属 |
|---|---|---|---|
| 58 | 2.16 | xochitl | reMarkable 核心 UI(正常) |
| 39 | 0.04 | memfaultd | Memfault 第三方遥测 |
| 20 | 0.02 | irq/39-elants_spi | 触控笔 SPI 中断(硬件) |
| 13 | — | dbus-daemon | 系统总线 |
| 9 | 0.03 | wr-serve | **cang-jie** 阅读面板 |
| 7 | 0.03 | cj-stars-daemon | **cang-jie** PKM daemon |

## 机器归属应用

| 进程 / 服务 | 归属 | 性质 | 电池 |
|---|---|---|---|
| xochitl | reMarkable 核心 | 主 UI | 主要但正常 |
| memfaultd · crashuploader | **Memfault**(第三方遥测 SDK) | 崩溃/遥测上报 | 低;可关 |
| remarkable-counter-metrics · slumber-metrics · battery-status-metrics · nm-metrics | reMarkable 度量 | 使用统计 | 极低;可关 |
| mdm-agent | reMarkable MDM | 设备管理(dbus 激活) | 极低 |
| rm-sync · update-engine · swupdate | reMarkable 云同步/OTA | 同步/更新检查 | 低 |
| NetworkManager · wpa_supplicant | 网络栈 | WiFi 管理 | 省电+断连=低 |
| marker-manager · tee-supplicant · irq/39-elants_spi | 硬件 | 笔/安全/触控驱动 | 正常 |
| **wr-serve · cj-stars-daemon · wr-renew.timer** | **cang-jie(本项目)** | 阅读面板 / PKM daemon / cookie 续期 | 低;wr-renew 每小时 |

## 唤醒源(打断休眠的来源)

`mwlan`(WiFi 模块)、`0-0048`(I2C 传感器)、`xochitl.batterymanager`、`sleep.resume`(唤醒锁)。均为正常唤醒,无异常频繁项。

## 可选削减(若想进一步省电,收益都不大)

1. **遥测簇**可关(不想上报即可):
   `systemctl disable --now memfaultd crashuploader remarkable-counter-metrics slumber-metrics nm-metrics battery-status-metrics.timer`
   —— 省掉那 39 CPU 秒 + 周期网络;注意这些在 rootfs,**OTA 后会复活**,且关掉 metrics 可能影响官方售后诊断。
2. **本项目 `wr-renew`**:每小时续 cookie。阅读功能不常用时可放宽 timer 周期(改 `OnCalendar`),但成本本就很低。
3. WiFi 已省电,无需动。

## 测量限制

审计时设备**插着 USB**(经 USB gadget 连接),`max77818_battery current_now=0`,**测不到实时放电电流**。要真实 mA 级 drain,需**断 USB** 后在设备本地读 `/sys/class/power_supply/max77818_battery/current_now`(负=放电),或按 `capacity` 随时间的下降率估算——两者都会断开 USB 连接,需在设备旁操作。

## ⚠ 2026-08-29 冻机事故 + 架构改常驻（brick 根因）

**现象**：设备 UI 冻在某界面、ping/SSH 全不通但 USB 链路仍 LOWER_UP（＝硬卡死，非 xochitl 崩、非拔线），长按电源 25-30s 强制重启才恢复。

**内核日志（`journalctl -b -1`）铁证**：`task:(battop):57059 state:D`（不可中断睡眠）卡在
`cgroup_procs_write → vfs_write → percpu_down_write(cgroup_threadgroup_rwsem) → synchronize_rcu`，
blocked 94→124→157s；连 `systemd:1` 也被这把 cgroup 锁堵死；随后 `rcu_preempt detected stalls` +
内核向 CPU1 发 NMI（CPU1 卡住）→ 整机雪崩。

**根因判断**：`cgroup_procs_write` 是 **systemd 启动 oneshot 服务时把子进程 PID 写进 service cgroup.procs
的常规迁移动作**（battop 的 `main()` 那次还没执行到）——**不是 battop 逻辑 bug**，是里面的
`synchronize_rcu` 撞上一次内核 RCU stall（大概率叠加当时并发的 WiFi association）。但 **battop 靠
`battop.timer` 每 ~10min 重启 oneshot＝144 次/天 cgroup 迁移**，把这个罕见 stall 的暴露窗口放大了 144 倍
（3 天 ~432 次中一次，概率吻合）。

**修法（架构改常驻）**：battop 从「timer 反复拉起 oneshot」改为 **Type=simple 常驻服务，进程内 `loop { sample_once; sleep(10min) }`**——内核只在**开机迁一次 cgroup**，暴露窗口 144次/天 → 1次/开机。
- `thread::sleep`(CLOCK_MONOTONIC) 休眠时不推进 → 天然"醒时每 N 分钟"，与旧 timer 的 awake-only 语义一致；休眠中进程随之冻结、不持 wakelock、不阻止休眠、CPU 零占用。
- 间隔可 `BATTOP_INTERVAL_SECS` 覆盖（缺省 600）。
- 附带（当时）：`journalctl`（读 31 天内核日志那步）改**有界执行**（`run_bounded`，超 20s 就 SIGKILL）。**已过时**：2026-09-23 起唤醒源改读 `/dev/kmsg`，采样循环里不再有任何子进程，`run_bounded` 随之删除。
- install.sh（当时）：停删 `battop.timer`、改 enable `battop.service`；去掉会永久阻塞 install 的前台"首次采样"（常驻服务启动即首采）。**已过时**：2026-09-20 起安装器只 start、不 enable。

**注**：当时 `systemctl enable` 的符号链接落在 `/etc`（tmpfs），重启即清，所以 battop 重启后天然处于关闭状态。2026-09-20 把这个"碰巧"定为设计：安装器不再 enable，要用就在网页打开。

## 后续：2026-09-23 常驻模型下又冻了一次

冻结前最后一条日志与 battop 刷新唤醒源缓存（当时要 fork 一次 `journalctl`）的时间精确重合到秒。这不是 08-29 那条 `cgroup_procs_write` 路径（fork 出的子进程直接继承父进程 cgroup，不走那个 syscall），只是时间吻合、没有内核栈证据。处理：唤醒源改为直读 `/dev/kmsg`，采样循环里最后一次创建子进程也去掉了（`src/wake.rs` 头注；commit 说明记真机确认零子进程、唤醒源数据与 `dmesg` 一致）。详见白皮书 §03b。
