# enhance —— 系统增强线

**一句话**：一组互相独立的小工具，让 reMarkable 更顺手——划中文精确吸附、xochitl 界面字体、阅读器单击翻页、阅读字体和休眠壁纸上传即用。它们都**不修改 xochitl**（reMarkable 自带的书库 / 阅读 / 笔记程序）本身。

- **给谁用**：有 reMarkable Paper Pro Move（固件 3.28.0.172）、已装 xovi 的用户，以及要改这些工具的开发者。
- **怎么开始**：整包安装 `sh packaging/install-all.sh <设备IP>`（见 [`../docs/INSTALL.md`](../docs/INSTALL.md)），装完在网关网页的「管理 → 系统增强」和「其他」里开关。
- **想弄懂原理**：读[白皮书](docs/reMarkable系统增强线白皮书.md)——第 1 章五分钟读懂，第 3 章讲每个工具怎么工作，第 7 章是真机验证现状。整个仓库里的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

![系统增强线：各组件怎么接到设备上](docs/diagrams/enhance-overview.svg)

## 组件一览

| 组件 | 是什么 | 状态 | 在网页哪里开关 |
|---|---|---|---|
| [`hl-snap/`](hl-snap/README.md) | xovi 扩展：荧光笔划中文时划哪吸哪，不再吸整行 | ✅ 真机通，日常在用 | 管理 → 系统增强，缺省开 |
| [`ui-font/`](ui-font/README.md) | xovi 扩展 + qmd：把 xochitl **界面**的字体换成上传的字体，阅读器字体不变 | ✅ 真机目测通过（10-07：界面换成更纱，阅读不变） | 其他 → xochitl → 界面字体，整机重启生效 |
| [`shelf/xovi/reader-page-turn.qmd`](../shelf/xovi/reader-page-turn.qmd) | qmd 补丁：xochitl 阅读器单击左右边缘翻页（源码在书架，原理记在本线） | ✅ 真机通（09-24）；10-07 删掉日漫翻页规则，已部署、未手测 | 管理 → 系统增强，缺省关 |
| `font-serve/` | Web 服务（`127.0.0.1:8792`）：阅读字体上传即装、中文回退链；也管界面字体 | ✅ 真机通 | 其他 → xochitl |
| [`wallpaper-serve/`](wallpaper-serve/README.md) | Web 服务（`127.0.0.1:8793`）：休眠壁纸上传即用、每次休眠后轮换 | ✅ 真机通 | 其他 → 壁纸 |
| [`lo-alias/`](lo-alias/README.md) | 让 `10.11.99.1` 在不插 USB 时也可达的小脚本（网关启动前调用） | ✅ 真机通（09-25 无 USB 冷启动） | 无 |
| [`shared/`](shared/PROVENANCE.md) | C 共用件：特征码扫描、跳板、PC 相对指令检查、极简 JSON 读取 `minijson`（编进两个扩展，带 host 单测） | — | 不单独部署 |

**几个词**：

- **xovi 扩展**：xovi 是第三方扩展加载框架，xochitl 启动时把 `~/xovi/extensions.d/` 下的文件加载进进程；扩展用 hook（把某个函数入口改成先跳到自己的代码）改变行为。换固件后找不到目标函数，扩展会自动不加载、退回原生行为。流程图见白皮书 3.1 节。
- **qmd 补丁**：qt-resource-rebuilder 读的 QML 补丁文件，xochitl 启动时读一次，用来改界面。
- **开关 ≠ 已生效**：扩展和补丁的网页开关旁有「已加载 / 未加载 / 待重启」徽章，由网关读 xochitl 主进程的 `/proc/<pid>/maps` 判定；「管理 → 基石」也列出 xochitl 里实际生效的扩展。

**现状（2026-10-10）**：设备上是 10-09 部署的版本（部署自检通过，荧光笔吸附、壁纸、字体这些功能本身没再手测）。10-10 第七轮审计与重构第二阶段的改动（两个扩展的 `minijson` 与 PC 相对指令防线、两个服务迁到 rmsvc-core 新接口与错误码分级）**已合并、未部署**，见白皮书第 7 章。

**已移除（2026-09-30，别再加回）**：手写优化（`handwriting-stroke/`）和电池刺客（`battop/`）。重跑 `install-all.sh` 会自动清掉旧设备上的残留；见白皮书附录 A.4。

## font-serve（没有单独 README）

- 路由（经网关前缀 `/api/fonts`）：`GET /` 按家族归组的清单 · `POST /` 上传（multipart，多文件）· `DELETE /{family}` · `GET /status` · `PUT /config {emboldenCjkFallback}`（中文回退字体加粗，fontconfig 实时生效）· `GET /events`；界面字体 `GET /ui` · `POST /ui` · `DELETE /ui/{family}` · `PUT /ui/select {sans, serif}`。
- 落点：阅读字体装进 fontconfig 用户字体目录 `~/.local/share/fonts/`（界面字体在子目录 `shelf-ui/`），字体菜单读 `~/.local/share/shelf/fonts.json`，回退规则写 `~/.config/fontconfig/fonts.conf`，界面字体选择写 `~/.local/share/shelf/ui-font.json`。
- 一批上传后只跑一次 `fc-cache -f ~/.local/share/fonts`（只扫用户字体目录）；`fc-scan` 30 秒、`fc-cache` 120 秒超时。
- 代码：`src/main.rs` 路由、`store.rs` 扫描与 fc-cache、`fontconfig.rs` 生成 `fonts.conf`、`ttf.rs` 解析字体家族名与 CJK 覆盖率、`ui.rs` 界面字体选择。原理见白皮书 3.5 节与书架白皮书的字体章节（[`../shelf/docs/reMarkable书架白皮书.md`](../shelf/docs/reMarkable书架白皮书.md) 第 F 章、§03k、§03bd）。
- 错误码（2026-10-10 起，未部署）：没有这个字体家族 404、选没装的界面字体 400、删字体文件 / 重写索引 / 存选择失败 500；上传请求体不是合法 multipart 400、暂存目录建不起来 500。
- host 测试：`cd font-serve && cargo test --locked`（21 项，2026-10-10 实跑）。

## 构建与测试

```sh
cd hl-snap && make aarch64        # hl-snap.so（已提交进仓库）
cd ui-font && make aarch64        # ui-font.so（已提交进仓库）
cd shared && make test            # 扫描 / 跳板 / PC 相对检查 / minijson 的 host 单测
cd ui-font && make test           # 导入表改写四种链接方式 + 真实 Qt 6（有 Qt6Gui 开发包时）
cd font-serve && cargo test --locked        # 21 项
cd wallpaper-serve && cargo test --locked   # 16 项
```

xovi 胶水 `xovi_glue.{c,h}` 已提交进仓库，平时 `make aarch64` 只需要 aarch64 交叉编译器、不需要 asivery/xovi clone；改了 `.xovi` 才要 `make glue XOVI_DIR=<clone>`。门槛：交叉编译零警告（CI 传 `-Werror`）；动态符号最高 hl-snap `GLIBC_2.17`、ui-font `GLIBC_2.34`（过高会在设备上静默加载失败）。细节见白皮书第 5 章。

## 部署

一般用整包安装：`sh packaging/install-all.sh <host>` 按顺序装好全部组件，xovi 扩展只落盘，最后一步 `xovi-apply` 统一**整机重启**一次（见 [`../docs/INSTALL.md`](../docs/INSTALL.md)）。`wallpaper-serve` / `font-serve` / `lo-alias` 随其中的 shelf 步安装。

单独更新某个工具，用 [`../packaging/`](../packaging/README.md) 里的一键脚本（构建 → 推送并 md5 校验 → 设备端安装）：

```sh
cd packaging
sh deploy-hl-snap.sh <host>
sh deploy-ui-font.sh <host>
sh deploy.sh <host> --only font,wallpaper    # font-serve / wallpaper-serve + 网页 + 相关 qmd
```

单独跑时，内容没变、也没有别的待生效改动就不重启。各扩展的设备端 `deploy/install.sh` 需要同目录的 `xovi-ext-install.sh` 与 `devlib.sh`，见各自 README。

⚠ **让扩展生效一律整机重启**（2026-09-25 起）。停止 xochitl 本身就有概率在它退出途中崩溃（xochitl 自己的问题，与换没换 `.so` 无关），所以部署脚本不再 stop / restart xochitl：xochitl 正在用旧版时，新版先放进待换入区 `~/.cangjie-stage/so-pending/`，整机重启前换入。手动操作时同样只用 `reboot`；**绝不**在 xovi 已生效时跑 `xovi/start`（会让 xochitl 崩溃、整机重启）。为什么这样设计见白皮书 3.8 节，机制见 [`../packaging/README.md`](../packaging/README.md)「怎么让改动生效」。

## 跟其它目录的关系

- [`../gateway/src/enhance/`](../gateway/src/enhance/) 是本线的**网页控制面**：开关登记在 `mod.rs` 的 `TOGGLES` 表，只读写 `~/.local/share/cangjie-ime/reading-qol.json`（hl-snap 与翻页 qmd 读的就是这份文件）、读 xochitl 的 `/proc/<pid>/maps`，和本目录源码没有代码依赖。
- `wallpaper-serve/`、`font-serve/` 依赖 [`../rmsvc-core`](../rmsvc-core/README.md)，由网关反向代理。
- 两个 qmd（`ui-font-tokens.qmd`、`reader-page-turn.qmd`）源码在 [`../shelf/xovi/`](../shelf/xovi/)，随书架的 font / book 服务安装。
- [`../defw/`](../defw/README.md)（xochitl 3.28.0.172 逆向工具与方法）**不属于**本线，是共享的逆向基座。
