# enhance —— 系统增强线

**一句话**：一组互相独立的小工具，让 reMarkable 更顺手——划中文精确吸附、阅读器单击翻页、界面字体、阅读字体和休眠壁纸上传即用。它们都**不修改 xochitl**（reMarkable 自带的书库/阅读/笔记程序）本身。

- **给谁用**：有 reMarkable Paper Pro Move（固件 3.28.0.172）、已装 xovi 的用户，以及要改这些工具的开发者。
- **怎么开始**：整包安装 `sh packaging/install-all.sh <设备IP>`（见 [`../docs/INSTALL.md`](../docs/INSTALL.md)），装完在网关网页的「管理 → 系统增强」和「其他」里开关。
- **想弄懂原理**：读[白皮书](docs/reMarkable系统增强线白皮书.md)，先看 §00b 现状；整个仓库里的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

![enhance 的工具怎么接到设备上](docs/diagrams/enhance-overview.svg)

## 组件一览

| 组件 | 是什么 | 状态 | 在网页哪里开关 |
|---|---|---|---|
| [`hl-snap/`](hl-snap/README.md) | xovi 扩展：荧光笔划中文时划哪吸哪，不再吸整行 | ✅ 真机通，日常在用 | 管理 → 系统增强，默认开 |
| [`shelf/xovi/reader-page-turn.qmd`](../shelf/xovi/reader-page-turn.qmd) | qmd 补丁：xochitl 阅读器单击左右边缘翻页 | ✅ 真机通（09-24）；10-07 删掉日漫翻页规则，已部署、功能未手测 | 管理 → 系统增强，默认关 |
| [`ui-font/`](ui-font/README.md) | xovi 扩展 + qmd：把 xochitl **界面**的字体换成上传的字体，阅读器字体不变 | ✅ 真机目测通过（10-07：界面换成更纱，阅读不变） | 其他 → xochitl → 界面字体，整机重启生效 |
| [`wallpaper-serve/`](wallpaper-serve/README.md) | Web 服务（`127.0.0.1:8793`）：休眠壁纸上传即用、每次休眠后轮换 | ✅ 真机通 | 其他 → 壁纸 |
| `font-serve/` | Web 服务（`127.0.0.1:8792`）：阅读字体上传即装、中文回退链；10-07 起也管界面字体 | ✅ 真机通（阅读字体） | 其他 → xochitl |
| [`shared/`](shared/PROVENANCE.md) | xovi 扩展用的特征码扫描 + trampoline 代码（带 host 单测，编进 `hl-snap.so`） | — | 不单独部署 |
| [`lo-alias/`](lo-alias/README.md) | 让 `10.11.99.1` 在不插 USB 时也可达的小脚本 | ✅ 真机通（09-25 无 USB 冷启动） | 不单独部署（网关启动前调用） |

**几个词**：

- **xovi 扩展**：xovi 是第三方扩展加载框架，xochitl 启动时把 `~/xovi/extensions.d/` 下的 `.so` 加载进进程；扩展用 hook（把某个函数入口改成先跳到自己的代码）改变行为。换固件后找不到目标函数，扩展会自动不加载、退回原生行为。完整流程图见白皮书 §02。
- **qmd 补丁**：qt-resource-rebuilder 读的 QML 补丁文件，xochitl 启动时读一次，用来改界面。
- **开关 ≠ 已生效**：扩展的网页开关旁有「已加载 / 未加载」徽章，表示 `.so` 是否真的在 xochitl 进程里；「管理 → 基石」也列出 xochitl 里实际生效的扩展。

**已移除（2026-09-30，别再加回）**：手写优化（`handwriting-stroke/`，`hw-stroke.so`）和电池刺客（`battop/`）。重跑 `install-all.sh` 会自动清掉旧设备上的残留；来龙去脉见白皮书 §03n，源码在 git 历史里。

**最近一轮（2026-10-09 第六轮审计，13:48 已部署，部署自检通过，功能未手测）**：壁纸 cover 模式先裁再缩放（开发机 12MP 横图 1.72s → 1.01s）；`fc-cache` 只扫用户字体目录、一次上传多个只刷新一次；`fc-scan`/`fc-cache` 加超时；壁纸池和字体目录只认普通文件。两个 C 扩展修了审计发现的三处隐患（glyph 上界、改完代码页恢复 `r-x`、非 PIE 防递归），其中"恢复 `r-x`"已在真机 maps 里看到，另两处只有离线验证，见白皮书 §03p「已知风险」。

## font-serve（没有单独 README）

- 路由（经网关前缀 `/api/fonts`）：`GET /` 按家族归组的清单 · `POST /` 上传（multipart，多文件）· `DELETE /{family}` · `GET /status` · `PUT /config {emboldenCjkFallback}`（中文回退字体加粗开关，fontconfig 实时生效）· `GET /events`；界面字体 `GET /ui` · `POST /ui` · `DELETE /ui/{family}` · `PUT /ui/select {sans, serif}`。
- 落点：字体装进 fontconfig 用户字体目录 `~/.local/share/fonts/`（界面字体在子目录 `shelf-ui/`），字体菜单读 `~/.local/share/shelf/fonts.json`，回退规则写 `~/.config/fontconfig/fonts.conf`。
- 代码头注：`font-serve/src/main.rs`（路由）、`src/store.rs`（扫描与 fc-cache）、`src/ui.rs`（界面字体）。原理在 [`../shelf/docs/reMarkable书架白皮书.md`](../shelf/docs/reMarkable书架白皮书.md) 的字体章节（第 F 章、§03k、§03bd）。
- 错误码（10-10 起）：没有这个字体家族 404、选没装的界面字体 400、删字体文件 / 重写索引 / 存选择失败 500（此前一律 400）。
- host 测试：`cd font-serve && cargo test`（16 项，2026-10-10 实跑；wallpaper-serve 16 项）。

## 构建与部署

一般用整包安装：`sh packaging/install-all.sh <host>` 会按顺序装好全部组件，xovi 扩展只落盘，最后一步 `xovi-apply` 统一**整机重启**一次让它们生效（见 [`../docs/INSTALL.md`](../docs/INSTALL.md)）。`wallpaper-serve` / `font-serve` / `lo-alias` 随其中的 shelf 步安装。

单独更新某个工具，用 [`../packaging/`](../packaging/README.md) 里的一键脚本（构建 → 推送并 md5 校验 → 设备端安装）：

```sh
cd packaging
sh deploy-hl-snap.sh <host>
sh deploy-ui-font.sh <host>
sh deploy.sh <host> --only font      # font-serve + 网页 + 字体相关 qmd
```

单独跑时，内容没变、也没有别的待生效改动就不重启。各工具的设备端 `install.sh` 也能脱离编排单独跑，但需要同目录的 `devlib.sh` 等文件，见各自 README。

xovi 扩展的 `.so` 和构建用的 xovi 胶水 `xovi_glue.{c,h}` 都已提交进仓库，平时 `make aarch64` 不需要 asivery/xovi clone；构建细节见 [`hl-snap/README.md`](hl-snap/README.md)「构建」。

⚠ **让扩展生效一律整机重启**（2026-09-25 起）。停止 xochitl 本身就有概率在它退出途中崩溃（xochitl 自己的问题，与换没换 `.so` 无关），所以部署脚本不再 stop / restart xochitl：xochitl 正在用旧版时，新版先放进待换入区 `~/.cangjie-stage/so-pending/`，整机重启前换入。手动操作时同样只用 `reboot`；**绝不**在 xovi 已生效时跑 `xovi/start`（会让 xochitl 崩溃、整机重启）。机制见 [`../packaging/README.md`](../packaging/README.md)「怎么让改动生效」。

## 跟其它目录的关系

- [`../gateway/src/enhance/`](../gateway/src/enhance/) 是本线的**网页控制面**：只调 `systemctl`、读写 `~/.local/share/cangjie-ime/reading-qol.json`（hl-snap 读的就是这份文件）、读 xochitl 的 `/proc/<pid>/maps`，和本目录源码没有代码依赖。
- `wallpaper-serve/`、`font-serve/` 依赖 [`../rmsvc-core`](../rmsvc-core/README.md)，由网关反向代理。
- [`../defw/`](../defw/README.md)（xochitl 3.28.0.172 逆向产物）**不属于**本线，是共享的逆向基座。
- 迁移、改名的历史见白皮书附录「迁移沿革」；设备上谁在定时唤醒 CPU 见白皮书 §03j；历轮审计（09-24 / 09-25 / 09-30 / 10-09）的改动见白皮书 §03k / §03l / §03m / §03p。
