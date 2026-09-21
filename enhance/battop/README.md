# battop —— 电池刺客

电量异常排查用的常驻采样诊断进程（Rust，纯 std 零依赖）。类 htop、非实时、带时间窗（今日/7天/30天/全部）的电池/后台占用追踪器：每 10 分钟采样一次，数据放 `/home/root/battop/data`，并预聚合成 `summary.json` 给网页展示。

- **历史**：2026-09-09 从旧的 `misc/battery-audit/battop/` 搬进 `enhance/`（早于这条线，不是本线首创）；[`FINDINGS.md`](FINDINGS.md) 是当时调查的产物，记录了 2026-08-29 cgroup/RCU 死锁事故与修复；[`history/`](history/) 是"怎么发现该建 battop"的早期诊断脚本遗留。
- 决策过程/真机验证见 `../docs/reMarkable系统增强线白皮书.md` §03b。

## 构建

```sh
cargo build --release --target aarch64-unknown-linux-musl
```

## 部署

**推荐（host 侧一键构建+推送+安装）**：`cd packaging && sh deploy-battop.sh <host>`。二进制先以 `battop.new` 推到 `/home/root/battop/`、md5 校验通过后由设备端 `install.sh` 原子 rename 覆盖（不再"先 stop 服务再 scp 覆盖"）；旧二进制备份进 `~/cangjie-backups/`（保留最近 5 份）。`CJ_BATTOP_BIN=<已编好的二进制>` 可跳过交叉编译。它也是 `packaging/install-all.sh` 的一步，见 `../../packaging/README.md`。

**手动在设备上跑**（root）：

```sh
sh install.sh [--start | --no-start]
```

`install.sh` 需要同目录的 `devlib.sh`，以及 `battop.new`（新二进制，优先）或已在的 `battop`——这些由 host 侧脚本一起推送。固定装在 `/home/root/battop`，不走 shelf 的 XDG 那套。

装的是常驻服务（`Type=simple`，进程内 `loop{sample_once; sleep(10min)}`），**不是**早期那版靠 `battop.timer` 反复拉起 oneshot 的架构（那版触发过整机冻死，见 `FINDINGS.md`）。

**有意不开机自启**（2026-09-20）：只 `start`、不 `enable`、不在 `/usr` 建 wants 链接——2026-08-29 采样触发过内核 cgroup/RCU 死锁冻死整机，根因未彻底排除。要用就在网页「管理 → 系统增强」里打开电池刺客开关（`POST /api/enhance/battop/start`）或 `systemctl start battop`；想恢复自启是需要自己评估的决定。

启动策略：单元首次安装 → start；已在跑且二进制/单元有变化 → restart；已在跑且无变化 → 不动；已装但当前停着（用户在网页关了）→ 保持停着；`--start` 强制启动，`--no-start` 不启动。

## 开关 · 网页数据展示

网关（[`../../gateway/src/enhance/battop.rs`](../../gateway/src/enhance/battop.rs)）负责 `systemctl start/stop battop.service` 和转发数据：

- **开关**在「管理 → 系统增强」（2026-09-21 起；此前在「实验室」）。
- **数据展示**在「管理 → 电池刺客」独立二级 tab，**运行时才出现**（`battop.running` 门控），下含「耗电情况」（按应用/按进程下拉切换）与「唤醒源」两个三级 tab，各自再按 4 个时间窗切换。数据源就是本服务聚合的 `summary.json`（`GET /api/enhance/battop/summary` 原样转发，网页不重新聚合）。
- 演进细节见 `../../shelf/docs/reMarkable书架白皮书.md` §03ak-§03am。
- 旧版原生「设置」页的"电池审计"面板（若设备上仍在）只是只读展示 `summary.json`；其源码不在本仓库，未核对。
