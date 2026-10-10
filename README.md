# cang-jie

**[English](docs/README.en.md)**

给 reMarkable Paper Pro Move 用的设备增强套件：在手机或电脑的浏览器里管理设备上的书、把书上的勾画和手写批注整理成笔记，
外加几项系统层面的小改进。

它**不修改 xochitl**（设备自带的阅读 / 笔记 / 书库主程序）本体，而是用三种"旁路"手段补功能：

- **设备上的网页服务**：一组常驻在设备上的小程序，经 xochitl 自己的网页上传接口和它的书库目录跟它打交道；
- **xovi 扩展**：xovi 是第三方的扩展加载框架，在 xochitl 启动时把我们的 `.so` 小插件加载进它的进程；
- **qmd 界面补丁**：对 xochitl 界面描述文件（QML）的补丁，由第三方组件 qt-resource-rebuilder 在启动时套上，用来加少量入口和代理。

> 本仓库 2026-10-09 起公开（Apache-2.0）。早先的公开版 [`rm-tweak`](https://github.com/bbq191/rm-tweak) 是从这里导出的无历史快照，
> 已归档，以后只在这里更新。

## 能做什么

| 你想要 | 怎么做到 | 代码在 |
|---|---|---|
| **把书放进 xochitl** | 书先在电脑上用姊妹项目 [sheng-ren](https://github.com/bbq191/sheng-ren)（booklib，`xochitl` 阅读模式）优化好，再在网页上传，或 scp 进设备的 `~/.local/state/shelf/books/inbox/`，书就进了**母版库**（设备上原样保存书的暂存池）；勾选后点「加入」，可选放进哪个文件夹（没有就让 xochitl 自己建）。书架只收 EPUB / PDF，原样投书、不改书；母版库里的书可以下载原件、改文件名 | [`shelf/`](shelf/README.md) |
| 放超过 100MB 的大书 | xochitl 网页上传约 100MB 封顶；超过 90MB 的书自动走"占位 + 磁盘替换"，整本进，最大 1GB | `shelf/` |
| 批量加入 | 母版库里勾多本，网关在后台一本一本做完，关掉浏览器也接着跑 | [`gateway/`](gateway/README.md) |
| 漫画留白最少 | sheng-ren 优化过的漫画，加入后第一次打开时自动把页边距设到最小 | `shelf/` |
| 换书不丢进度 | 电脑上的 sheng-ren 可以绕过母版库直接导入，或原地替换一本已有的书；替换一本正在读的书后重新打开，自动跳回原来读到的地方 | `shelf/` |
| **把勾画和手写批注变成笔记** | 合上书后，荧光笔勾画和旁边的手写自动收进条目库；在手机网页上校对手写转写（视觉模型）、全文搜索、对单条问 AI；再按章投回设备笔记本（标题、列表、复选框用设备自带样式），或导出 Obsidian Markdown | [`notes/`](notes/README.md) |
| 荧光笔划中文"划哪亮哪" | xochitl 原本按空格分词，划中文会吸整行；扩展 `hl-snap` 修掉 | [`enhance/`](enhance/README.md) |
| 阅读器单击翻页 | 点屏幕左右边缘翻页（开关默认关） | `enhance/`（补丁随 `shelf/` 装） |
| 换 xochitl 界面字体 | 书库、设置、对话框换成你上传的字体；阅读器里的字体不变 | `enhance/` |
| 阅读字体、休眠壁纸 | 网页上传即用；壁纸可固定一张，或每次休眠按顺序 / 随机换一张 | `enhance/` |
| 设备健康 | 网页「管理 → 设备健康」看各服务状态、内存、扩展是否真的载入、上次开机日志；固件升级冲掉组件时网页顶部提示怎么恢复 | `gateway/` |

所有操作都在一个网页里：浏览器打开 `https://10.11.99.1/`（USB 连接）或 `https://shelf.local/`（同一 WiFi；安卓不解析 `.local`），
输入登录密码。网页有中文、英文两种界面语言。

## 整体架构

![核心架构：设备上有什么、谁和谁说话](docs/diagrams/architecture-topology.svg)

要点：所有服务都跑在设备上；**只有网关 `gateway` 对外**（`0.0.0.0:443`，HTTPS + 登录密码），其余服务只听 `127.0.0.1`，
由网关按服务注册表转发；电脑只在安装、卸载、更新时用到。一次请求、一个事件具体怎么走，见
[`docs/OVERVIEW.md`](docs/OVERVIEW.md) 里的模块交互图。

## 怎么装

**只支持 reMarkable Paper Pro Move、固件 3.28.0.172**（目前唯一验证过的版本，安装脚本会核对，不符默认拒装）。

1. 在设备上手动装好两样第三方基础组件：`vellum add xovi`、`vellum add qt-resource-rebuilder`（vellum 是设备上的包管理器）。安装脚本不代装。
2. 电脑上准备 Rust 交叉编译环境，并能免密 ssh 到设备 root（清单见 [`docs/INSTALL.md`](docs/INSTALL.md)）。用 USB 线连上设备，然后：

   ```sh
   git clone https://github.com/bbq191/cang-jie.git
   cd cang-jie/packaging
   sh install-all.sh --dry-run        # 先预演：只打印计划，不连设备
   sh install-all.sh 10.11.99.1
   ```

   最后一步会整机重启设备一次（约 1 分钟），回来后脚本自动只读核对，逐项打 ✓ / ⚠ / ✗。有步骤失败时不重启，修好后重跑同一条命令。
3. 浏览器打开 `https://10.11.99.1/`，默认密码 `shelf`，第一次登录必须改。想让浏览器不再报"不安全"，点网页顶栏的「CA 证书」下载并装一次。

更新：重跑 `sh install-all.sh 10.11.99.1`，内容没变就不重启。卸载：`sh uninstall-all.sh 10.11.99.1`。
前置条件、风险、固件升级（OTA）后怎么恢复，全部在 **[docs/INSTALL.md](docs/INSTALL.md)**。

## 仓库结构

| 目录 | 一句话 |
|---|---|
| [`shelf/`](shelf/README.md) | 书架：母版库、加入 / 直接导入 xochitl、大书通道，以及随它安装的 5 个 qmd 补丁（回收站代理、建文件夹代理、漫画页边距、阅读位置、单击翻页） |
| [`notes/`](notes/README.md) | 笔记线：勾画 + 手写 → 条目库 → 转写 / 问 AI → 设备笔记本或 Obsidian（4 个服务：ink、transcribe、mind、note） |
| [`enhance/`](enhance/README.md) | 系统增强：xovi 扩展 `hl-snap`（荧光笔中文吸附）、`ui-font`（界面字体），字体服务 `font-serve`、壁纸服务 `wallpaper-serve`，以及 `lo-alias`（让 xochitl 的上传口在拔掉 USB 后仍可达） |
| [`gateway/`](gateway/README.md) | 网关：唯一对外入口，HTTPS、登录、反向代理、事件汇聚、批量队列、管理台与设备健康，托管整个网页 |
| [`rmsvc-core/`](rmsvc-core/README.md) | 服务基座：网关和各服务共用的 Rust 库（服务注册表、HTTP、事件总线、跟 xochitl 打交道的那一层），不含业务逻辑，不单独运行 |
| [`defw/`](defw/README.md) | xochitl 3.28.0.172 的固件逆向资料，给 xovi 扩展找挂接点用，不上设备 |
| [`packaging/`](packaging/README.md) | 安装器：在电脑上运行，经 ssh 把上面这些装到设备；带固件校验、部署后核对，以及对称的卸载 |

改代码的人：每次 push / PR，GitHub Actions 跑 shellcheck、安装脚本的沙箱模拟测试、网页脚本检查与 node 测试、enhance 的 C 单测、
各 Rust crate 的 `cargo test`、依赖漏洞检查（cargo audit）以及 aarch64 交叉编译（`.github/workflows/ci.yml`）。这些只证明代码在电脑上行为正确，
**改变设备行为的改动仍要在真机上验证**。

## 文档地图

![文档地图](docs/diagrams/docs-map.svg)

| 想了解 | 读 |
|---|---|
| 系统全貌、两张核心图、设计原则、验证现状、术语 | [docs/OVERVIEW.md](docs/OVERVIEW.md) |
| 怎么装、卸、更新，固件升级后怎么恢复 | [docs/INSTALL.md](docs/INSTALL.md) |
| 安装脚本内部结构、本机测试（开发者） | [packaging/README.md](packaging/README.md) |
| 最近改了什么 | [docs/CHANGELOG.md](docs/CHANGELOG.md) |
| 每条线的细节、设计决策、真机验证记录 | 上表各目录的 README 与其 `docs/` 白皮书 |
| 书怎么被优化 | 不归本仓库：[sheng-ren](https://github.com/bbq191/sheng-ren) 的 `docs/typesetting.md`、`docs/xochitl.md` |

## 历史与范围

项目最早是"reMarkable 中文输入法"（仓库名 `cang-jie` 即"仓颉"，传说中造字的人），后来陆续做过阅读增强、知识管理、手写识别。
2026-09-11 做过一次大整理：中文输入法、早期的完整阅读管线、知识管理、手写识别及它们的逆向资料整体移出了本仓库，
**源码不在这里，也不随本仓库的安装脚本分发**。之后又陆续移除过几项功能（KOReader 相关、耗电诊断「电池刺客」、手写笔画优化、
设备端的书籍优化与格式转换等），细节见 [CHANGELOG](docs/CHANGELOG.md)。

现在维护的就是上面七个目录。架构比早期保守：以独立的网页服务为主，xovi 扩展只留两个，尽量少碰官方系统的内部。

## 打赏

如果这个项目帮到了你，欢迎请作者喝杯咖啡。纯自愿，打不打赏都不影响任何功能。收款码见 **[DONATE.md](docs/DONATE.md)**。

## 协议 / 免责

本仓库代码以 **[Apache License 2.0](LICENSE)** 开源，可自由使用、修改、分发（含商用），需保留版权声明；协议本身附带专利授权条款。
原本作为个人自用工具开发，按现状（as-is）提供，不附带任何担保。

涉及第三方许可证的地方（字体等）以代码注释里的实测记录为准，不构成法律意见。与 reMarkable、
[xovi](https://github.com/asivery/xovi)、vellum 等第三方项目或商标没有从属关系。
