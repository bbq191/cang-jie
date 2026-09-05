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
