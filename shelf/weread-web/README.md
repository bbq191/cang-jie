# weread-web · 微读网页版在线读 spike（存档，不再推进）

**状态（2026-09-05）**：方向已改——微信读书在书架里定位为**内容源**（扫码登录 → 下书成 EPUB → 原样落母版库 → 用户选读器），
不再做"设备上直接开网页版在线读"的内嵌浏览器 app（书架白皮书 §03r 决策 3：spike 未通过验证、e-ink WebKit 天花板低、基础≈0）。
本目录的 `spike.sh` 与判据**原样存档**，供将来重评时用；未执行过，**不要**把 rmweb 写进 systemd 或开机路径。

原始问题：现货浏览器 `exp78/rmweb`（MIT，WPE WebKit + Skia CPU raster，Qt6 epaper QPA 直出屏，运行时停掉 xochitl，
预编译 106MB，v0.9.1）只标 Paper Pro "Ferrari" 1620×2160，Move（"Chiappa" 954×1696）未提及。

## 步骤（`spike.sh`，逐步人工确认；设备 root SSH）
1. `spike.sh recon`：只读侦查——`/etc/version`、`uname -r`、`/sys/devices/soc0/machine`、`ls /usr/lib/plugins/platforms/`
   有无 epaper QPA、`/dev/dri`、内存；写 `findings.md`。
2. `spike.sh fetch <rmweb-0.9.1.tar.gz 本机路径>`：解到 `~/.local/share/shelf/rmweb/`（不进 rootfs/extensions.d）；`ldd` 列缺库。
3. `spike.sh run`：起看门狗 `( sleep 600; systemctl start xochitl ) &` → `systemctl stop xochitl` → 前台跑 rmweb，观察出画面/黑屏/崩溃。
4. 出画面则测：触摸、屏幕键盘、打开 ink.qq.com、扫码登录、翻页时延、退出后 xochitl 恢复。
5. `spike.sh restore`：kill 看门狗、`systemctl start xochitl`、`is-active`/`NRestarts` 检查。

回退：`systemctl start xochitl`；`rm -rf ~/.local/share/shelf/rmweb`。
