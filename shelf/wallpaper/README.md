# 壁纸（wallpaper-serve）

休眠屏"上传即用"。逻辑全在 Rust `services/wallpaper-serve`（缩放 954×1696、池化、`enable|disable|roll|activate` 子命令）；
本目录 2026-09-06～09-11 只剩这份说明，2026-09-11 起多了 [`legacy-bind-mount/`](legacy-bind-mount/)——下面「历史（已退役）」提到的 bind-mount 方案脚本本体，从无主的顶层 `misc/wallpaper/` `git mv` 归档到这里（本工具的唯一现实关联方就是接手它的 `wallpaper-serve`）。

**机制（2026-09-06 定稿，3.28.0.172 真机）**：xochitl 有隐藏键 `xochitl.conf [General] SleepScreenPath=<png>`——指向
`~/.local/share/shelf/wallpapers/current.png` 后，休眠屏原生满屏显示该图、插画卡自动隐藏、**每次休眠重读文件**。
所以键只写一次（激活首张时 `native.rs` 自动写，或安装器 `wallpaper-serve enable`），换图永远是**原地覆盖 current.png**
（保 inode），轮换由 `wallpaper-serve serve` 监听 xochitl 日志 `DeepSleep to Normal` 触发（`wake.rs`；充电时按电源键内核
不 suspend、systemd-sleep 钩子不可靠，2026-09-03 真机）。**零 `/usr` 写入、零 bind-mount、没有开机单元和 sleep 钩子。**
首次写键后要重启一次 xochitl（`/home/root/xovi/start`）才被读进 `isettings`；网页壁纸页和 `GET /status` 的
`native.restartPending` 会提示。卸载 `wallpaper-serve disable` 删键还原原生休眠屏。

改 `xochitl.conf` 的纪律：`shelf_core::xochitl_conf` 只动 `[General]` 单键、tmp+rename 原子写、首次改前留 `xochitl.conf.shelf-bak`；
文件含 DeveloperPassword / UserToken，**任何地方都不打印行内容**。

路径（XDG）：池 `~/.local/share/shelf/wallpapers/pool/*.png`、`current.png` 同级、状态 `~/.local/state/shelf/wallpaper-state.json`。

**历史（已退役）**：2026-09-03～09-05 用 bind-mount 覆盖 `/usr/share/remarkable/suspended.png` + 三张 776×776 透明卡盖插画
（`shelf-wallpaper-bind.service` + `system-sleep/shelf-wallpaper.sh` + `blank776.png`）。发现 `SleepScreenPath` 后整套删除；
安装器/卸载器里的旧残留清理块已于 2026-09-06 删除（真机零残留，白皮书 §03ab）；万一从更老的备份恢复，按 §03x 手工清。书架白皮书 §03w/§03x。
