# battop —— 电池刺客

电量异常排查用的常驻采样诊断进程（Rust，纯 std 零依赖）。类 htop、非实时、带时间窗（当日/7天/30天/全部）的电池/后台占用追踪器。2026-09-09 `git mv` 自 `misc/battery-audit/battop/`（早于 `enhance/` 这条线，不是本线首创，只是搬了家）；`FINDINGS.md` 是那次调查的产物、跟着搬过来了——2026-08-29 那次 cgroup/RCU 死锁事故+修复的完整记录在里面。2026-09-11 `misc/battery-audit/` 剩下的诊断脚本历史（`bataudit*.sh`/`battery-audit.sh`/`APP-DESIGN.md`）也一并 `git mv` 进 [`history/`](history/)——那是"怎么发现该建 battop"这个更早期过程的遗留，跟 battop 本体（真正依赖它的模块）放一起比继续留在无主的 `misc/` 下更合适，`misc/` 目录本身已删空。

决策过程/真机验证见 `../docs/reMarkable系统增强线白皮书.md` §03b；设备端采样器的定位/架构见系统增强白皮书（`xovi-extensions/docs/reMarkable系统增强白皮书.md`）§10。

## 构建

```sh
cargo build --release --target aarch64-unknown-linux-musl
```

## 部署

```sh
sudo sh install.sh
```

装的是常驻服务（`Type=simple`，进程内 `loop{sample_once; sleep(10min)}`），**不是**早期那版靠 `battop.timer` 反复拉起 oneshot 的架构（那版触发过整机冻死，见 `FINDINGS.md`）。固定装在 `/home/root/battop`，不走 shelf 的 XDG/`bin_dir` 那套。

**host 侧一键构建+推送+安装**：`packaging/deploy-battop.sh <host>`（2026-09-11 新增，做的就是上面"先 scp 二进制再跑 install.sh"这两步的自动化，不改任何逻辑），也是 `packaging/install-all.sh` 全新设备统一安装器调用的其中一步，见 `../../packaging/README.md`。

## 开关 · 网页数据展示

原生设置页「系统增强→电池审计」页只是只读展示（读 `summary.json`）。网关网页这边经过几轮调整（2026-09-09 §03aj 起、2026-09-10 §03ak-§03am 定型）：开关（`systemctl start/stop battop.service`，`gateway/src/enhance/battop.rs`——网关 2026-09-11 从 `shelf/services/shelf-gateway` 正名搬顶层）现在在「管理→实验室」，只留开关+风险说明；真正的数据展示挪到「管理→电池刺客」独立二级 tab（**运行时才出现**，`battop.running` 门控），下含「耗电情况」（按应用/按进程下拉切换）/「唤醒源」两个三级 tab，各自再按 4 个时间窗（今日/7天/30天/全部）切换——数据源就是本目录常驻聚合的 `summary.json`（`GET /api/enhance/battop/summary` 原样转发，网页不重新聚合）。细节见 `shelf/docs/reMarkable书架白皮书.md` §03ak-§03am。
