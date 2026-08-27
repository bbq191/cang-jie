# 电池审计 App 方案文档(battop）

> 一个"类 htop、非实时、带时间窗"的设备电池/后台占用追踪器。
> 现状调研见同目录 `FINDINGS.md`。
> 决策记录:查看器最初定 Web 面板,后**改为设备端注入面板(参考墨香)**——见 §9 实现状态。

## 9. 实现状态（v1 已真机落地，2026-08-27）

- **采集器 `battop`**(Rust,`battop/`):systemd timer 每 ~10 分钟 oneshot 采样,产出 `data/samples-*.tsv` + 预聚合 `data/summary.json`(4 窗口 × 应用/进程双分组 + 放电%)。已部署 `/home/root/battop/`,timer active,自身 ~4.3 CPU 秒/天。
- **查看器改为设备端注入 QML 面板**(不是 Web):注入系统增强设置页,作为 hub 第 6 项「电池审计」→ 全屏二级页(`xovi-extensions/reading-qol/settings-reading-enhance.qmd` 的 `cjBatteryPage`/990006)。QML 用 XHR 同步读 `summary.json` → JSON.parse → 渲染。参考墨香:衬线品牌标题 + 自绘等宽标签胶囊 + 去框化条形排行(序号/细条/发丝线);归属复选框 = 自绘(不用独立 `ArkControls.Toggle`,那要 Panel 宿主)。
- **踩坑(致命)**:QML `Item` 的 **`data` 是承载子元素的保留默认属性**;`property var data` 覆盖它 → 所有子元素不被 parent → `parent.X` anchor 全 null → **空白页**(无致命日志,只有下游 anchor-null 警告,apply-diffs 也过)。改名 `sumData` 即好。。
- **数据契约**:`summary.json` = `{generated, windows:{today,7d,30d,all}}`;每窗口 `{discharge, samples, app:[{name,ms,pct}], proc:[...]}`。应用视图套友好名(§5),进程视图 raw comm。
- 未做(后续):v2 唤醒次数/唤醒锁;v3 断 USB 真实 `current_now` 校准。

## 0. 一句话

一个**极轻采集器**周期性把"每进程/每应用的 CPU 增量 + 电量 + 充放电"落到本地小库,一个 **Web 面板**按 **当日 / 7天 / 30天 / 全部** 四窗口聚合展示,复选框切换 **按应用(systemd unit)** 或 **按进程(comm)** 归组。

## 1. 第一性原理:为什么必须有采集器(不能事后做)

- `/proc/[pid]/stat` 的 CPU 计数是**"进程自启动以来"累计,进程一死清零**;内核**不保存**历史。xochitl 等会重启。
- 所以 **7天/30天 历史无法 live 重建,必须自己长期采样落盘。**
- **推论(必须先接受)**:采集器**装上那天起**才有数据。头 30 天里"7天/30天/全部"标签是逐渐填满的;"全部"≈自安装起(受保留期上限约束)。

## 2. 两条诚实红线(度量的边界)

1. **ARM 墨水屏无 RAPL/powertop 级能量计** → 做不到精确"每进程 mAh"。可信度量=**复合代理分**(CPU 时间 + 唤醒次数 + 唤醒锁占用),再以**放电时 `capacity` 的下降**作为该窗口"总耗"地面真值。按 CPU 占比把总耗摊到进程只能算**估算**,UI 必须标"估"。
2. **长窗口按 PID 聚合无意义**(PID 回收、进程反复重启)。稳定聚合键:
   - **进程视图 = 按 `comm`(可执行名)** 聚合,跨重启合并。
   - **应用视图 = 按 systemd unit / cgroup** 聚合。
   这正好落到你的复选框:**打钩=按 unit,不打钩=按 comm**。长窗口下应用视图更准,当日进程视图更适合抓瞬时元凶。

## 3. 架构(三层)

| 层 | 选型 | 要点 |
|---|---|---|
| **采集器** | Rust daemon(项目 musl 静态 + `device-core` 底座),放 `/home/root/battop/` | 每 T 分钟(默认 10)采样;仅醒时跑,**不设唤醒告警、不持 wakelock**(否则自己变刺客) |
| **存储** | SQLite(`rusqlite` bundled 静态) 或 每日 flat 文件;放 `/home`(OTA 不丢) | **按小时预聚合**,滚动保留 40 天 |
| **查看器** | 采集器内置极小 HTTP,吐 JSON;单页 HTML(4 标签+复选框) | 自足无外链(遵项目离线纪律),浏览器看 |

### 3.1 采集器每 tick 读什么
- **系统**:`CLOCK_REALTIME` 时间戳、`/proc/uptime`(醒时秒)、`max77818_battery/{capacity,status}`(Charging/Discharging/Full)。
- **进程级**:遍历 `/proc/[pid]`:`comm`、`stat`→`utime+stime`(ticks)、`starttime`(判重启)。
- **应用级(更干净)**:直接读 cgroup `cpu.stat` 的 `usage_usec`(每 systemd service/slice),天然跨该 unit 内 PID 重启累计,免自己缝合。
- **归属**:`/proc/[pid]/cgroup` 尾段取 unit 名;配一张静态 `unit→友好应用名` 映射(见 §5)。

### 3.2 增量与重启处理
- 存上一 tick 的 `(comm 或 unit, key_cputicks)`;本 tick delta = now−prev(≥0)。
- 进程 `(pid,starttime)` 变化 = 重启:旧账已按之前的 delta 记入,新体从 0 起。末尾不足一 tick 的残值丢弃(≤T,可接受)。
- **休眠天然正确**:挂起期间 utime/stime 不涨,跨休眠的 delta 只含醒着那段——正是我们要的。

### 3.3 存储表(建议)
```
usage(hour_epoch, kind, key, cpu_ms, disc_secs)      -- kind∈{proc,app}; key=comm 或 unit
battery(hour_epoch, cap_start, cap_end, disc_secs, awake_secs)
```
40 天×24h×~50 键×2 类 ≈ 十万行级,SQLite 毫无压力;老于 40 天的行滚动删。

### 3.4 窗口(标签)= 纯 SQL
- **当日** = 本地今日 0 点起的 bucket。
- **7天/30天** = 回溯 7×24 / 30×24 小时。
- **全部** = 全部保留 bucket(受 40 天保留期封顶,UI 注明)。

### 3.5 展示(v1)
- 窗口内按 key 的 **CPU 时间(ms)** 降序 + 占比条;顶部一行 **本窗口总放电 %**(放电期 `capacity` 下降合计)作背景真值。
- 可选:**估算**每 key 放电% = 窗口放电% × (该 key CPU ÷ 放电醒时总 CPU),**标"估"**。

## 4. Web 面板
- 端点:`GET /api/usage?window=today|7d|30d|all&group=app|proc` → JSON 排行。
- 单页 HTML:4 个标签按钮 + 一个"归属应用"复选框 + 条形排行;`fetch` 刷新。自足、无外链。
- 复用 `wr-serve` 的设备端 Rust HTTP 服务样板,监听 `IP:port`(USB/WiFi 可达)。

## 5. 归属映射(unit → 友好应用名,初版)

| unit / comm | 友好名 |
|---|---|
| xochitl | reMarkable 核心 |
| memfaultd · crashuploader | Memfault 遥测 |
| *-metrics · remarkable-counter-metrics · slumber-metrics | reMarkable 度量 |
| mdm-agent | reMarkable MDM |
| rm-sync · update-engine · swupdate | reMarkable 云同步/OTA |
| NetworkManager · wpa_supplicant | 网络栈 |
| marker-manager · tee-supplicant · irq/* | 硬件驱动 |
| wr-serve · wr-stars-daemon · wr-renew · battop | **cang-jie(本项目)** |

未知 unit 回落显示 unit 名。

## 6. 部署 / 持久化(沿用壁纸那套)
- 采集器 = systemd service(+timer,`AccuracySec` 宽松、**不 `WakeSystem`**,只在醒时机会性采样);unit 在 rootfs、逻辑/数据在 `/home`,OTA 后 `install.sh` 重建。
- **自省**:这工具本身也是常驻——必须比它测的对象更省(10 分钟一 tick、间歇睡、读 ~200 个 /proc 即返回)。上真机后拿它测自己,确认 `battop` 自身占比可忽略,才算合格。

## 7. v1 范围 vs 以后
- **v1**:CPU 时间 + 窗口总放电 两个可信量把 采集→存储→四标签→复选框 端到端跑通。
- **v2**:唤醒次数 / 唤醒锁占用 进复合分(需先解决 `debugfs/wakeup_sources` 读取——本轮审计时该路径不可读,待查 `/sys/.../power/wakeup_*` 或挂 debugfs)。
- **v3**:断 USB 后的真实 `current_now` 采样(需设备旁配合),校准估算摊分。

## 8. 待定/风险
- 存储引擎:SQLite(查询爽,引入 C 依赖)vs flat 文件(零依赖,聚合放查看器)——建 v1 用 SQLite,若 musl 静态编译 `rusqlite` 有阻再退 flat。
- 采样周期 T:10 分钟是能耗/分辨率折中;抓瞬时尖峰不够细,但对"日/周/月耗电归属"足够。
- "全部"受 40 天保留期封顶;要更长得调保留期 + 接受存储增长(仍很小)。
- 是否进 cang-jie 主线:定位同壁纸——**misc 个人设备工具**,不接 xovi/主线;采集器若成熟可抽成独立 Rust crate。
