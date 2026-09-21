# wallpaper-serve —— 休眠壁纸上传即用

网页上传一张图，设备休眠时就显示它；可以建一个"池"，每次唤醒自动轮换。Web 服务（`127.0.0.1:8793`，经网关 `/api/wallpapers` 访问）。代码在 `src/`（`main.rs` 路由、`store.rs` 壁纸池、`native.rs` 写 xochitl 配置键、`wake.rs` 唤醒轮换），服务单元 `wallpaper-serve.service`；缩放到 954×1696、`enable|disable|roll|activate` 子命令都在这里。

## 机制（2026-09-06 定稿，3.28.0.172 真机）

xochitl 有一个隐藏配置键 `xochitl.conf [General] SleepScreenPath=<png>`：指向 `~/.local/share/shelf/wallpapers/current.png` 后，休眠屏原生满屏显示该图、插画卡自动隐藏、**每次休眠重读文件**。因此：

- 键**只写一次**（激活首张时 `native.rs` 自动写，或安装器 `wallpaper-serve enable`）；
- 换图永远是**原地覆盖 `current.png`**（保 inode）；
- 轮换由 `wallpaper-serve serve` 监听 xochitl 日志里的 `DeepSleep to Normal` 触发（`wake.rs`；充电时按电源键内核不 suspend、systemd-sleep 钩子不可靠，2026-09-03 真机）；
- **零 `/usr` 写入、零 bind-mount、没有开机单元和 sleep 钩子。**

首次写键后，xochitl 要重启一次才会把键读进设置；网页壁纸页和 `GET /status` 的 `native.restartPending` 会提示。重启用 `systemctl restart xochitl`（xovi 已生效时**别**手动跑 `xovi/start`，见 [`../../docs/INSTALL.md`](../../docs/INSTALL.md) 问题⑤）。卸载用 `wallpaper-serve disable` 删键，还原原生休眠屏。

**改 `xochitl.conf` 的纪律**：`rmsvc_core::xochitl_conf` 只动 `[General]` 单键、tmp + rename 原子写、首次改前留 `xochitl.conf.shelf-bak`；文件里含 DeveloperPassword / UserToken，**任何地方都不打印行内容**。

## 路径（XDG）

| 内容 | 位置 |
|---|---|
| 壁纸池 | `~/.local/share/shelf/wallpapers/pool/*.png` |
| 当前壁纸 | `~/.local/share/shelf/wallpapers/current.png` |
| 状态 | `~/.local/state/shelf/wallpaper-state.json` |

## 历史（已退役）

2026-09-03～09-05 曾用 bind-mount 覆盖 `/usr/share/remarkable/suspended.png` + 三张 776×776 透明卡盖插画（`shelf-wallpaper-bind.service` + `system-sleep/shelf-wallpaper.sh` + `blank776.png`）。发现 `SleepScreenPath` 后整套删除，脚本本体存档在 [`legacy-bind-mount/`](legacy-bind-mount/) 仅供参考，**不要再用**；万一从更老的备份恢复出这套残留，按书架白皮书 §03x 手工清理。来龙去脉见书架白皮书 §03w/§03x/§03ab。
