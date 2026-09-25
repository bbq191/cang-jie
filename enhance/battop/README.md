# battop —— 电池刺客

**一句话**：查"谁在偷偷耗电"的采样诊断进程。打开后每 10 分钟（只算设备醒着的时间）记一次各进程 CPU 用量、电量和唤醒源，网页上按今日 / 7 天 / 30 天 / 全部四个时间窗排行。

- Rust，纯 std 零依赖；常驻 systemd 服务 `battop.service`（`Type=simple`，进程内循环采样）。
- **有意不开机自启**：它曾两次和整机冻死扯上关系（2026-08-29 坐实，2026-09-23 时间吻合），平时关着，要查耗电时再在网页打开。来龙去脉见 [白皮书 §03b](../docs/reMarkable系统增强线白皮书.md)。
- 本身几乎不耗电：不持唤醒锁、不主动唤醒设备，采样循环里**不创建任何子进程**（2026-09-23 起）。
- [`FINDINGS.md`](FINDINGS.md)：2026-08-27 那次电池审计的原始结论和 08-29 冻机的内核证据；[`history/`](history/)：更早的诊断脚本和设计文档，纯存档。

## 开关与数据展示

网关 [`gateway/src/enhance/battop.rs`](../../gateway/src/enhance/battop.rs) 负责启停和转发数据：

| 在哪 | 做什么 |
|---|---|
| 网页「管理 → 系统增强」→「电池刺客」开关 | `systemctl start/stop battop.service`（`POST /api/enhance/battop/{start,stop}`），立即生效；设备真重启后回到"关" |
| 网页「管理 → 电池刺客」子页（**服务在跑时才出现**） | 「耗电情况」（按应用/按进程切换）与「唤醒源」两页，各有 4 个时间窗；数据是 battop 预聚合好的 `summary.json`（`GET /api/enhance/battop/summary` 原样转发） |

也可以在设备上直接 `systemctl start battop` / `systemctl stop battop`。旧版原生「设置」页若还有"电池审计"面板，它只读同一份 `summary.json`（该面板源码不在本仓库，未核对）。

## 采什么、存在哪

| 数据 | 来源 | 说明 |
|---|---|---|
| 各进程 CPU 增量 | `/proc/<pid>/stat` | 按进程名、按 systemd unit（"应用"）两种方式聚合 |
| 电量、放电 mAh | 电量计 sysfs（`max77818_battery` 的 `capacity`、`charge_now`、`current_now`） | `charge_now` 是库仑计，放电 mAh 比按百分比估算准 |
| 唤醒源 | `/dev/kmsg` 里的 `PM: active wakeup source: <名>` | 只含**本次开机以来**的记录（旧版 fork `journalctl` 能跨开机查 31 天，为去掉子进程接受了这个退化）；约每 50 分钟刷新一次缓存；按记录序号增量并入（`wakes.cursor`），时间按不含休眠的单调时钟换算（09-25 前按含休眠的 uptime 换算，事件被提前"开机以来累计休眠时长"，面板「今日」唤醒数不准） |

数据目录 `/home/root/battop/data`（`BATTOP_DIR` 可改）：按天的样本文件 + baseline + `wakes.tsv` 唤醒缓存 + `summary.json`，40 天前的样本自动清理。每轮重算 `summary.json` 时，历史样本文件按（大小, mtime）缓存整文件聚合，只重读被时间窗起点切开或变过的文件（2026-09-24，host 实测每轮 34.7 → 4.0 ms，输出逐字节不变；未上真机）。采样间隔 `BATTOP_INTERVAL_SECS`（默认 600 秒）。间隔用单调时钟计，设备休眠时不走，所以是"醒着每 10 分钟"。

## 构建

```sh
cargo build --release --target aarch64-unknown-linux-musl
cargo test          # host 单测（含与旧实现逐字节对拍的黄金文件、缓存与全量重扫的 60 轮对拍）
```

## 部署

**推荐**（host 侧一键构建 + 推送 + 安装）：

```sh
cd packaging && sh deploy-battop.sh <host>        # CJ_BATTOP_BIN=<已编好的二进制> 可跳过交叉编译
```

新二进制先以 `battop.new` 推到 `/home/root/battop/`，md5 校验通过后由设备端 `install.sh` 原子 rename 覆盖；旧二进制备份进 `~/cangjie-backups/`（保留最近 5 份）。它也是 `packaging/install-all.sh` 的一步。

**手动在设备上跑**（root）：

```sh
sh install.sh [--start | --no-start]
```

需要同目录的 `devlib.sh`，以及 `battop.new`（优先）或已有的 `battop`。固定装在 `/home/root/battop`；单元文件 `battop.service` 装进 `/usr/lib/systemd/system/`（先过 dm-verity 检查，verity 开着就只放二进制、不装单元）。安装器**只 start、不 enable**，也清掉旧 `battop.timer` 的遗留。

启动策略：

| 情况 | 行为 |
|---|---|
| 单元第一次装 | start |
| 正在跑，且二进制或单元有变化 | restart |
| 正在跑，没变化 | 不动 |
| 已装但停着（用户在网页关了） | 保持停着 |
| `--start` / `--no-start` | 强制启动 / 不启动 |

想恢复开机自启是需要自己评估的决定，安装器不会悄悄做。
