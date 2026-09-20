# battop —— 电池刺客

电量异常排查用的常驻采样诊断进程（Rust，纯 std 零依赖）。类 htop、非实时、带时间窗（当日/7天/30天/全部）的电池/后台占用追踪器。2026-09-09 `git mv` 自 `misc/battery-audit/battop/`（早于 `enhance/` 这条线，不是本线首创，只是搬了家）；`FINDINGS.md` 是那次调查的产物、跟着搬过来了——2026-08-29 那次 cgroup/RCU 死锁事故+修复的完整记录在里面。2026-09-11 `misc/battery-audit/` 剩下的诊断脚本历史（`bataudit*.sh`/`battery-audit.sh`/`APP-DESIGN.md`）也一并 `git mv` 进 [`history/`](history/)——那是"怎么发现该建 battop"这个更早期过程的遗留，跟 battop 本体（真正依赖它的模块）放一起比继续留在无主的 `misc/` 下更合适，`misc/` 目录本身已删空。

决策过程/真机验证见 `../docs/reMarkable系统增强线白皮书.md` §03b；设备端采样器的定位/架构见系统增强白皮书（`xovi-extensions/docs/reMarkable系统增强白皮书.md`）§10。

## 构建

```sh
cargo build --release --target aarch64-unknown-linux-musl
```

## 部署

```sh
sudo sh install.sh [--start | --no-start]
```

装的是常驻服务（`Type=simple`，进程内 `loop{sample_once; sleep(10min)}`），**不是**早期那版靠 `battop.timer` 反复拉起 oneshot 的架构（那版触发过整机冻死，见 `FINDINGS.md`）。固定装在 `/home/root/battop`，不走 shelf 的 XDG/`bin_dir` 那套。

**有意不开机自启**（2026-09-20）：只 `start`、不 `enable`、不在 `/usr` 建 wants 链接——2026-08-29 采样触发过内核 cgroup/RCU 死锁冻死整机，根因未彻底排除。要用就在网页「管理→电池刺客」开（`POST /api/enhance/battop/start`）或 `systemctl start battop`；想恢复自启是需要自己评估的决定。启动策略：单元首次安装 → start；已在跑且二进制/单元有变化 → restart；已在跑且无变化 → 不动；已装但当前停着（用户在网页关了）→ 保持停着；`--start` 强制启动，`--no-start` 不启动。

**依赖与文件**：`install.sh` 需要同目录的 `devlib.sh`，以及 `battop.new`（新二进制，优先）或已在的 `battop`；这些由 host 侧脚本一起推送。**host 侧一键构建+推送+安装**：`cd packaging && sh deploy-battop.sh <host>`——二进制先以 `battop.new` 推到 `/home/root/battop/`、md5 校验通过后由设备端 `install.sh` 原子 rename 覆盖（不再"先 stop 服务再 scp 覆盖"）；旧二进制备份进 `~/cangjie-backups/`（保留最近 5 份）。`CJ_BATTOP_BIN=<已编好的二进制>` 可跳过交叉编译。它也是 `packaging/install-all.sh` 的一步，见 `../../packaging/README.md`。

## 开关 · 网页数据展示

原生设置页「系统增强→电池审计」页只是只读展示（读 `summary.json`）。网关网页这边经过几轮调整（2026-09-09 §03aj 起、2026-09-10 §03ak-§03am 定型）：开关（`systemctl start/stop battop.service`，`gateway/src/enhance/battop.rs`——网关 2026-09-11 从 `shelf/services/shelf-gateway` 正名搬顶层）现在在「管理→实验室」，只留开关+风险说明；真正的数据展示挪到「管理→电池刺客」独立二级 tab（**运行时才出现**，`battop.running` 门控），下含「耗电情况」（按应用/按进程下拉切换）/「唤醒源」两个三级 tab，各自再按 4 个时间窗（今日/7天/30天/全部）切换——数据源就是本目录常驻聚合的 `summary.json`（`GET /api/enhance/battop/summary` 原样转发，网页不重新聚合）。细节见 `shelf/docs/reMarkable书架白皮书.md` §03ak-§03am。
