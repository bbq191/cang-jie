# wallpaper-serve —— 休眠壁纸上传即用

**一句话**：网页上传一张图，设备休眠时就满屏显示它；传多张会组成一个"池"，每次唤醒自动换下一张。

- Web 服务，只听本机 `127.0.0.1:8793`，经网关 `/api/wallpapers` 访问；网页入口「其他 → 壁纸」。
- 现状：真机通（3.28.0.172，2026-09-06 定稿）。
- 代码：`src/main.rs` 路由，`store.rs` 壁纸池，`native.rs` 写 xochitl 配置键，`wake.rs` 唤醒轮换；服务单元 `wallpaper-serve.service`。子命令 `enable | disable | roll | activate` 也在这里。

## 机制

xochitl 有一个隐藏配置键 `xochitl.conf` → `[General] SleepScreenPath=<png>`。它指向 `~/.local/share/shelf/wallpapers/current.png` 之后，休眠屏会原生满屏显示这张图、自动隐藏中间的插画卡，而且**每次休眠都重读文件**。所以：

| 动作 | 怎么做 |
|---|---|
| 写配置键 | **只写一次**：激活第一张壁纸时 `native.rs` 自动写，或手动 `wallpaper-serve enable` |
| 换图 | 永远是**原地覆盖 `current.png`**（保持同一个文件） |
| 轮换 | 服务跟着 `journalctl -f -u xochitl` 看日志，出现 `DeepSleep to Normal`（唤醒）就换下一张；池里只有一张且 `current.png` 已是它时不重写文件（2026-09-24，省闪存写入，未上真机）。代价：xochitl 每写一行日志这个服务都会醒一次（白皮书 §03j）。不用 systemd-sleep 钩子：充电时按电源键内核不 suspend，钩子不可靠（2026-09-03 真机） |
| 入池 | 缩放到 954×1696；源图先只读文件头，超过 1600 万像素或长宽比极端的直接拒收，避免解码吃光内存 |
| 卸载 | `wallpaper-serve disable` 删掉配置键，恢复原生休眠屏 |

不写 `/usr`、不做 bind-mount、没有开机单元和 sleep 钩子。`journalctl` 起不来时按 5 秒到 5 分钟指数退避重连。

**首次写键后要重启一次 xochitl** 才会读进这个键；网页壁纸页和 `GET /status` 的 `native.restartPending` 会提示。重启用 `systemctl restart xochitl`（xovi 已生效时**别**跑 `xovi/start`，见 [`../../docs/INSTALL.md`](../../docs/INSTALL.md) 问题⑤）。

**改 `xochitl.conf` 的纪律**：`rmsvc_core::xochitl_conf` 只动 `[General]` 下这一个键，先写临时文件再 rename，首次改前留 `xochitl.conf.shelf-bak`。这个文件里有 DeveloperPassword 和 UserToken，**任何地方都不打印它的行内容**。

## 路径（XDG）

| 内容 | 位置 |
|---|---|
| 壁纸池 | `~/.local/share/shelf/wallpapers/pool/*.png` |
| 当前壁纸 | `~/.local/share/shelf/wallpapers/current.png` |
| 状态 | `~/.local/state/shelf/wallpaper-state.json` |

## 历史（已退役）

2026-09-03～09-05 曾用 bind-mount 覆盖 `/usr/share/remarkable/suspended.png`，再用三张 776×776 透明图盖住插画卡（`shelf-wallpaper-bind.service` + `system-sleep/shelf-wallpaper.sh` + `blank776.png`）。发现 `SleepScreenPath` 后整套删掉，脚本存档在 [`legacy-bind-mount/`](legacy-bind-mount/)，**不要再用**；万一从更老的备份恢复出这套残留，按书架白皮书 §03x 手工清理。来龙去脉见书架白皮书 §03w / §03x / §03ab。
