# KOReader 配置即代码

把设备上手改的 KOReader 调优固化成仓库文件，可 diff、可幂等恢复。（书从哪来：网页「传书 → 母版库」点「加入 KOReader」，
`koreader-serve` 从母版库纯复制到 `books/[目录]/`；本目录只管**配置补丁**。）

## 目录里有什么

| 文件 | 作用 |
|---|---|
| `profile/settings.reader.patch.lua` | 补丁：深合并进设备 `settings.reader.lua`（脚注、刷新波形、悬挂标点、状态栏等） |
| `profile/defaults.custom.lua` | 补丁：深合并进设备 `defaults.custom.lua` |
| `profile/gestures.patch.lua` | 补丁：深合并进设备 `settings/gestures.lua`（防误触、退出手势等） |
| `profile/fonts.txt` · `profile/dicts.txt` | 字体 / StarDict 词典**清单**（每行一个本机路径；数据本身不入库）。⚠ 2026-09-18 host `shelf` 命令行砍除后，**没有工具再自动读这两份清单**，它们只作"该装哪些字体/词典"的备忘；实际安装走网页「其他 → KOReader」上传（`POST /fonts`、`POST /dicts?name=`） |
| `merge.lua` | 合并器：被 `koreader-serve` 通过 `include_str!` 内嵌，设备端用 KOReader 自带 `luajit` 执行 |

补丁语义：标量覆盖、表递归、值为字符串 `"__DELETE__"` 删键。

## 怎么应用（当前现状）

**所有 Lua 处理都在设备端**，由 `koreader-serve`（`127.0.0.1:8791`，经网关为 `/api/koreader/…`）执行：
写前备份到 `~/.local/state/shelf/koreader-backups/<文件>.bak.pre-shelf-<时间戳>`，写后回读校验；
**KOReader 运行中拒写**（返回 409；它退出时会回写配置覆盖你的改动，必须先退出 KOReader）。

| 接口 | 作用 |
|---|---|
| `GET /config/{settings\|defaults\|gestures}` | 读设备上的当前原文 |
| `POST /config/{settings\|defaults\|gestures}[?dry_run=1]` | body = 补丁 Lua 文本；`dry_run=1` 只返回将改的键（path/old/new），不写 |

例（在 host 上，经网关；网关需登录，这里的 cookie 是登录后浏览器/`curl -c` 拿到的）：
`curl -k -b cookie.txt --data-binary @shelf/koreader/profile/settings.reader.patch.lua 'https://shelf.local/api/koreader/config/settings?dry_run=1'`，看差异没问题再去掉 `dry_run=1` 重发。
**没有一键脚本、没有网页按钮**——2026-09-18 之前的 `shelf koreader pull/diff/sync` 命令行已随 host CLI 整体砍除（清单见 `../docs/reMarkable书架白皮书.md` 附录 B），需要时手工调上述接口，或在设备上直接改。

profile 三文件的键来自旧《阅读白皮书》§11.1b（该文档在 2026-09-11 整理时已挪出仓库）；补丁文件里标注"待核对"的值，应先 `GET /config/...` 看设备实况再定。

## KOReader 入口现状（2026-09-21，固件 3.28.0.172，appload 0.6.0）

KOReader 本体（v2026.07.1，官方支持 Move）与本目录 / koreader-serve 都正常。

- 启动入口是 appload（xochitl 侧栏里的外部应用启动器）。**appload ≥ 0.6.0 起原生支持 3.28**（上游 v0.6.0，2026-09-19 发布，另加 3.29 支持），2026-09-21 官方升级并真机验证（侧栏 KOReader/WeRead 入口点开正常）。
- 历史：0.5.3 在 3.28 上不兼容（其 qmd 钩了 3.28 已删的 `SidebarFilterItem`），2026-09-06 曾用 PR #59 的 qmd 等长回填进 `.so` 顶过；该回填补丁工具已删除，不再需要。
- 升级注意：换 appload 文件后**别 `systemctl restart xochitl`**（旧进程退出时崩溃 → 整机重启一次），直接整机重启。
- 已知变化：0.6.0 的实体键（左 / 主页 / 右）发 Qt 原始键码，KOReader 的 qtfb 输入层仍按 0/1/2 映射 → 实体键失效（Move 没有实体翻页键，只影响外接键盘）；未在真机上专门验证键码。
- KOReader 根目录仍是 `~/xovi/exthome/appload/koreader/`（可用环境变量 `SHELF_KOREADER_ROOT` 覆盖），书/字体/词典照常同步进去。详见书架白皮书 §03v。
