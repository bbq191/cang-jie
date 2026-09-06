# KOReader 配置即代码

把设备上手改的 KOReader 调优固化成仓库文件，可 diff、可一键幂等恢复。（书从哪来：网页「传书 → 母版库」点「加入 KOReader」，
`koreader-serve` 从母版库纯复制到 `books/[目录]/`；本目录只管配置/字体/词典。）**所有 Lua 处理在设备端**：
koreader-serve 内嵌 `merge.lua`，用 KOReader 自带 `luajit` 深合并（标量覆盖、表递归、`"__DELETE__"` 删键），
写前备份到 `~/.local/state/shelf/koreader-backups/`，写后 `dofile` 回读校验；**KOReader 运行中拒绝写**（退出回写会覆盖）。

```
shelf koreader pull                 # 拉三份配置快照到 $XDG_CACHE_HOME/shelf/snapshots/<时间>/
shelf koreader diff                 # profile/ 对设备 dry-run，列出将改的键（old→new）
shelf koreader sync [--dry-run]     # 应用 profile/（settings/defaults/gestures）
shelf koreader sync --fonts --dicts # 另按 fonts.txt / dicts.txt 同步字体与词典
```
profile 三文件的键来自《阅读白皮书》§11.1b；标注"待核对"的值需先 `pull` 看设备实况再定。

**KOReader 入口现状（2026-09-05，固件 3.28.0.172）**：KOReader 本体（v2026.07.1，官方支持 Move）与本目录/koreader-serve 都正常；
但**启动入口 appload 0.5.3 在 3.28 上不兼容**（其 qmd 钩 3.28 已删的 `SidebarFilterItem`，上游 PR #59 只改 qmd、编进 .so，重编需 rM Qt6 SDK），
已挪到 `/home/root/xovi-disabled/pre-3.28-*/`（extensions.d 外），**暂无侧栏入口**；等上游发版或自 fork 重编。KOReader 根目录仍是
`~/xovi/exthome/appload/koreader/`（书/字体/词典照常同步进去）。书架白皮书 §03v。
