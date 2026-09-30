# enhance —— 系统增强线

**一句话**：一组互相独立的小工具，让 reMarkable 更顺手——划中文精确吸附、阅读器翻页、字体和壁纸上传即用。它们都**不修改 xochitl**（reMarkable 自带的阅读/笔记程序）本身。

- 整个仓库里的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。
- 原理、决策、真机验证、踩坑见 [白皮书](docs/reMarkable系统增强线白皮书.md)（先读 §00b 现状）。
- 改 qmd 补丁（如 `reader-page-turn.qmd`）后要离线验证，用 qmldiff 的 `apply-diffs` 实跑，**不要信 `check-compatibility`**（它基本不做校验，垃圾语法也报无错），见白皮书 §04「qmd 补丁怎么离线验证」。
- 2026-09-29 设备已卸载 KOReader、第三方 WeRead、appload 和侧栏入口，本线各工具只服务 xochitl。
- **2026-09-30 已移除**：手写优化（`handwriting-stroke/`，xovi 扩展 `hw-stroke.so`）和电池刺客（`battop/`，耗电采样服务），连同网页开关、安装步骤一起删了（用户要求）。旧设备上的残留在重跑 `install-all.sh` 时自动清，只清这两样的命令见 [`../docs/INSTALL.md`](../docs/INSTALL.md)「装之前」；来龙去脉见白皮书 §03n。白皮书里关于它们的章节是历史记录，源码可在 git 历史里找（删除提交之前的版本）。

![enhance 的工具怎么接到设备上](docs/diagrams/enhance-overview.svg)

## 组件一览

| 组件 | 是什么 | 状态 | 在网页哪里开关 |
|---|---|---|---|
| [`hl-snap/`](hl-snap/README.md) | xovi 扩展：荧光笔划中文时划哪吸哪，不再吸整行 | ✅ 真机通 | 管理 → 系统增强，默认开 |
| [`shelf/xovi/reader-page-turn.qmd`](../shelf/xovi/reader-page-turn.qmd) | qmd 补丁：xochitl 阅读器单击翻页、日漫翻页规则 | ✅ 真机通（09-24）；方向只看书里自带的标记（09-25 加的母版库按书设方向已于 09-30 移除） | 管理 → 系统增强，默认关 |
| [`wallpaper-serve/`](wallpaper-serve/README.md) | Web 服务（8793）：休眠壁纸上传即用、每次休眠后轮换 | ✅ 真机通 | 其他 → 壁纸 |
| [`font-serve/`](font-serve/font-serve.service) | Web 服务（8792）：xochitl 字体上传即装、中文回退链 | ✅ 真机通 | 其他 → xochitl |
| [`shared/`](shared/PROVENANCE.md) | xovi 扩展用的特征码扫描 + trampoline 代码（带 host 单测；原先两个扩展共用，现在只有 hl-snap） | — | 不单独部署 |
| ~~`handwriting-stroke/`~~ | xovi 扩展：按笔尖角度 + 运笔速度调整手写笔画粗细 | **2026-09-30 已移除** | 原在管理 → 实验室 |
| ~~`battop/`~~ | 采样服务「电池刺客」：按进程/应用/唤醒源统计耗电 | **2026-09-30 已移除** | 原在管理 → 系统增强 + 「电池刺客」数据页 |
| [`lo-alias/`](lo-alias/README.md) | 让 `10.11.99.1` 在不插 USB 时也可达的小脚本 | — | 不单独部署（网关启动前调用） |

**xovi 扩展**是什么：xovi 是第三方扩展加载框架，xochitl 启动时把 `~/xovi/extensions.d/` 下的 `.so` 加载进进程；扩展用 hook（把某个函数入口改成先跳到自己的代码）改变行为。换固件后找不到目标函数，扩展会自动不加载、退回原生行为。完整流程图见白皮书 §02。

**开关 ≠ 已生效**：扩展（现在只有 hl-snap）的网页开关旁有「已加载 / 未加载」徽章，表示 `.so` 是否真的在 xochitl 进程里；「管理 → 基石」也列出了 xochitl 里实际生效的扩展。

`font-serve` 没有自己的 README：路由、字体目录、`fonts.json` 索引写在 `font-serve/src/main.rs` 头注，原理在 [`../shelf/docs/reMarkable书架白皮书.md`](../shelf/docs/reMarkable书架白皮书.md) 的字体章节（第 F 章、§03k、§03bd）。

## 构建与部署

一般用整包安装：`sh packaging/install-all.sh <host>` 会按顺序装好全部组件，xovi 扩展只落盘，最后一步 `xovi-apply` 统一**整机重启**一次让它们生效（见 [`../docs/INSTALL.md`](../docs/INSTALL.md)）。`wallpaper-serve` / `font-serve` / `lo-alias` 随其中的 shelf 步安装。

单独更新某个工具，用 [`../packaging/`](../packaging/README.md) 里的一键脚本（构建 → 推送并 md5 校验 → 设备端安装）：

```sh
cd packaging
sh deploy-hl-snap.sh <host>
```

（`deploy-handwriting-stroke.sh`、`deploy-battop.sh` 随功能一起在 2026-09-30 删除。）

单独跑时，内容没变、也没有别的待生效改动就不重启。各工具的设备端 `install.sh` 也能脱离编排单独跑，但需要同目录的 `devlib.sh` 等文件，见各自 README。

xovi 扩展的 `.so` 已提交进仓库；构建用的 xovi 胶水 `xovi_glue.{c,h}` 也已提交，平时 `make aarch64` 不需要 asivery/xovi clone，只有改了 `.xovi` 才要 `make glue XOVI_DIR=<clone>` 重新生成（`make clean` 只删 `.so`，不删胶水，2026-09-30 起）。2026-09-25 起扩展的 `Makefile` 加了 `-ffile-prefix-map=$(CURDIR)=.`，调试信息里不再带开发机的绝对路径（09-24 曾出现同一份源码因构建路径不同编出两个 md5）。反汇编不变。这版 `.so` 已于 09-25 13:10 经 WiFi 部署真机：整机重启时由待换入区换入，md5 与仓库一致，三个 hook「安装完成」。

⚠ **让扩展生效一律整机重启**（2026-09-25 起）。停止 xochitl 本身就有概率在它退出途中崩溃（xochitl 自己的问题，与换没换 `.so` 无关），所以部署脚本不再 stop / restart xochitl：xochitl 正在用旧版时，新版先放进待换入区 `~/.cangjie-stage/so-pending/`，整机重启前换入（你自己 `reboot` 开机时也会换入）。手动操作时同样只用 `reboot`；**绝不**在 xovi 已生效时跑 `xovi/start`（会让 xochitl 崩溃、整机重启）。机制见 [`../packaging/README.md`](../packaging/README.md)「怎么让改动生效」。

## 跟其它目录的关系

- [`../gateway/src/enhance/`](../gateway/src/enhance/) 是本线的**网页控制面**：只调 `systemctl`、读写 `~/.local/share/cangjie-ime/reading-qol.json`（hl-snap 读的就是这份文件）、读 xochitl 的 `/proc/<pid>/maps`，和本目录源码没有代码依赖。
- `wallpaper-serve/`、`font-serve/` 依赖 [`../rmsvc-core`](../rmsvc-core/README.md)，由网关反向代理。
- [`../defw/`](../defw/README.md)（xochitl 3.28.0.172 逆向产物）**不属于**本线，是共享的逆向基座；已移除的 `handwriting-stroke/` 的研究用过它。
- 迁移、改名的历史见白皮书附录「迁移沿革」；设备上谁在定时唤醒 CPU 见白皮书 §03j；2026-09-24 第三轮审计的改动（当天已部署）见白皮书 §03k；2026-09-25 第四轮审计的改动（battop 唤醒时间换算、font-serve 开机复用索引、壁纸去重时钟、两个代理 qmd 出错退避）见白皮书 §03l；2026-09-30 第五轮审计的改动（壁纸目录被删后重建监听、battop「应用」视图把现役服务归进 `cang-jie` 组、`make clean` 不再删胶水、建文件夹代理失败打日志；**只在开发机测过，未部署、未上真机**）见白皮书 §03m；同日移除手写优化与电池刺客见白皮书 §03n。
