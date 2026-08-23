# LXGW Neo XiHei Screen Full（霞鹜新晰黑 屏幕阅读版·补全）

- 用途：xochitl **UI 界面主字体**（黑体，Screen 版为屏幕清晰预优化，e-ink 不发虚）。简繁+英单文件覆盖（46324 码位，门/門·见/見·国/國·臺 均含）。
- 来源：`lxgw/LxgwNeoXiZhi-Screen` release `26.07.14` 的 `LXGWNeoXiHeiScreenFull.ttf`（本仓库存为 `LXGWNeoXiHeiScreenFull.ttf`）。
- 许可证：**IPA Font License Agreement v1.0**（实测下载 LICENSE 确认，`LXGWNeoXiHei-IPA-License.md`）。允许免费使用、原样不改名可再分发（须附许可证副本）；派生（改字体）须同证且不得用同名。
- 本项目用法：**原样、不改名、独立 `.ttf` 放 /home 运行时 mmap，不编译进 `.so`、不修改**——符合 IPA 原样再分发条款。设备 3.28.0.169 真机验证：设为 sans-serif 首选、界面清晰不发虚，用户确认选定（对比过 Sarasa UI SC Regular/SemiBold/合成Medium）。
- 部署：`.ttf` → `/home/root/.local/share/fonts/`，`deploy/fontconfig-cangjie.conf` 把 `sans-serif` 及 zh-* 指向 `LXGW Neo XiHei Screen Full`（纯 /home，无 verity 风险）。
