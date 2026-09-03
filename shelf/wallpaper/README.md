# 壁纸（wallpaper-serve）

休眠屏"上传即用"。逻辑全在 Rust `services/wallpaper-serve`（缩放 954×1696、池化、`bind|unbind|roll` 子命令）；
本目录只有 5 行 sleep 钩子。机制（真机结论，自 `misc/wallpaper/` 剥离移植）：`/usr/share/remarkable/suspended.png`
+ 三张 776×776 carousel 插画卡用 bind-mount 覆盖（rootfs 只读也可挂、真身不改、umount 即还原）；xochitl
每次休眠重读磁盘，换图零重启；换图原地覆盖 `current.png` 保 inode。

路径（XDG）：池 `~/.local/share/shelf/wallpapers/pool/*.png`、`current.png` 同级、状态 `~/.local/state/shelf/wallpaper-state.json`。
单元：`shelf-wallpaper-bind.service`（开机 bind）+ `/usr/lib/systemd/system-sleep/shelf-wallpaper.sh`（唤醒轮换）。
`shelf/install.sh` 检测旧 `/home/root/wallpaper/` 会迁移池图并停用旧 `cangjie-wallpaper.service`。
