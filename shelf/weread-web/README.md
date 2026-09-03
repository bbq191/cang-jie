# weread-web · 微读网页版在线读（P5 门控 spike）

**问题只有一个**：现货浏览器 `exp78/rmweb`（MIT，WPE WebKit + Skia CPU raster，Qt6 epaper QPA 直出屏，
**运行时停掉 xochitl**，预编译 106MB，v0.9.1 2026-08-28）**只标 Paper Pro "Ferrari" 1620×2160**，
Move（"Chiappa" 954×1696）未提及——能不能显示并交互？通了才立 `weread-serve`（appload `qtfb:true`
共存入口 + 移植评估另计划），不通就记录关闭。微读墨水屏网页版入口 `ink.qq.com`。**现有微读 EPUB 下书线
（wr-serve）始终兜底，书架不碰它。**

## 步骤（`spike.sh`，逐步人工确认；设备 root SSH）
1. `spike.sh recon`：只读侦查——`/etc/version`、`uname -r`、`/sys/devices/soc0/machine`、`ls /usr/lib/plugins/platforms/`
   有无 epaper QPA、`/dev/dri`、内存；写 `findings.md`。
2. `spike.sh fetch <rmweb-0.9.1.tar.gz 本机路径>`：解到 `~/.local/share/shelf/rmweb/`（不进 rootfs/extensions.d）；`ldd` 列缺库。
3. `spike.sh run`：起看门狗 `( sleep 600; systemctl start xochitl ) &` → `systemctl stop xochitl` → 前台跑 rmweb，观察出画面/黑屏/崩溃。
4. 出画面则测：触摸、屏幕键盘、打开 ink.qq.com、扫码登录、翻页时延、退出后 xochitl 恢复。
5. `spike.sh restore`：kill 看门狗、`systemctl start xochitl`、`is-active`/`NRestarts` 检查。

## 门控判据
步骤 3 出画面 **且** 步骤 5 恢复干净 → "通"；否则关闭。**绝不**把 rmweb 写进 systemd 或开机路径。
回退：`systemctl start xochitl`；`rm -rf ~/.local/share/shelf/rmweb`。
