# 壁纸（wallpaper-serve）

休眠屏"上传即用"。逻辑全在 Rust `services/wallpaper-serve`（缩放 954×1696、池化、`bind|unbind|roll` 子命令）；
本目录只有 5 行 sleep 钩子。机制（真机结论，自 `misc/wallpaper/` 剥离移植）：`/usr/share/remarkable/suspended.png`
+ 三张 776×776 carousel 插画卡用 bind-mount 覆盖（rootfs 只读也可挂、真身不改、umount 即还原）；xochitl
每次休眠重读磁盘，换图零重启；换图原地覆盖 `current.png` 保 inode。

路径（XDG）：池 `~/.local/share/shelf/wallpapers/pool/*.png`、`current.png` 同级、状态 `~/.local/state/shelf/wallpaper-state.json`。
单元：`shelf-wallpaper-bind.service`（开机 bind）+ `/usr/lib/systemd/system-sleep/shelf-wallpaper.sh`（入睡前补 bind）。
**轮换**由 `wallpaper-serve serve` 监听 xochitl 日志 `DeepSleep to Normal` 触发（充电时按电源键内核不 suspend、sleep 钩子不跑，2026-09-03 真机）。
`shelf/install.sh` 检测旧 `/home/root/wallpaper/` 会迁移池图并停用旧 `cangjie-wallpaper.service`。

**2026-09-05 翻案（3.28.0.172 真机）**：xochitl 有隐藏键 `xochitl.conf [General] SleepScreenPath=<png>`——设为 `current.png` 后
休眠屏原生满屏显示该图、插画卡自动隐藏、且每次休眠重读文件（随 wake.rs 轮换变图，用户两次休眠对照确认）。**bind-mount +
三张透明插画卡 + `/usr` 写入 + sleep 钩子整套可退役**，只留唤醒轮换 + 安装器写 conf 键（书架白皮书 §03w / §05 第 8 条，待做）。
当前设备两套并存：conf 键已写（优先生效），bind 仍挂着无害。改 conf 键必须停 xochitl 后改、先备份、绝不打印其中的 token。
