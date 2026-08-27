# 电池 / 后台进程审计报告

**设备**:reMarkable Paper Pro Move,固件 20260806,uptime 1 天。
**日期**:2026-08-27。**方法**:功耗代理量(累计 CPU、生命期 CPU 占比、唤醒源、休眠健康、应用归属)。
脚本见同目录 `battery-audit.sh`(入口)+ `bataudit{,2,3}.sh`。

## 结论:无电池刺客

- **休眠健康**:一天 8 次 `PM: suspend entry` = 8 次 `exit`;journal **零** `Deferring suspend` / `Can't suspend`——没有任何进程阻止休眠。
- **无自旋**:load avg ≈ 0;自家 daemon(`wr-stars`/`wr-serve`)均 `S`(睡眠)态,非轮询。
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
| 7 | 0.03 | wr-stars-daemon | **cang-jie** PKM daemon |

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
| **wr-serve · wr-stars-daemon · wr-renew.timer** | **cang-jie(本项目)** | 阅读面板 / PKM daemon / cookie 续期 | 低;wr-renew 每小时 |

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
