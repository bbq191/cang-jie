# cang-jie

**[English](docs/README.en.md)**

reMarkable Paper Pro Move 的设备增强套件。**不修改 xochitl**（设备自带的阅读/笔记应用）本体，
而是用两样东西给它补功能：[xovi](https://github.com/asivery/xovi)（第三方扩展加载框架）加载的小插件，
和一组跑在设备上的网页服务。补的是书籍管理、笔记整理和一些系统层面的小改进。

> **这一页讲**：能做什么、怎么装和卸、细节去哪看。
> 想 10 分钟看懂全貌，读 [`docs/OVERVIEW.md`](docs/OVERVIEW.md)；想知道最近改了什么，读 [`docs/CHANGELOG.md`](docs/CHANGELOG.md)。
>
> 本仓库 2026-10-09 起公开（Apache-2.0）。早先的公开版 [`rm-tweak`](https://github.com/bbq191/rm-tweak) 是从这里导出的无历史快照，
> 最后一次同步是 2026-09-30，现已归档，以后只在这里更新。

## 能做什么

| 你想要 | 对应的功能 | 在哪 |
|---|---|---|
| 把书弄进设备自带的阅读器 xochitl | 书先在电脑上用 [sheng-ren](https://github.com/bbq191/sheng-ren)（booklib，`xochitl` 阅读模式）优化好，再在网页上传（或 scp 进 inbox）进"母版库"，勾选「加入 xochitl」。书架原样投书、不再改书（2026-10-07 起）。超过 xochitl 约 100MB 上传上限的大书也能整本进（最大 1GB）；sheng-ren 优化的漫画可自动把页边距设到最小；母版库里的书可以下载原件、改名。sheng-ren 也能跳过母版库直接导入或原地替换书，替换一本正在读的书后重新打开会跳回原来读到的地方 | [`shelf/`](shelf/README.md) |
| 把荧光笔勾画和旁边的手写批注变成能整理的笔记 | 合上书自动收进条目库。在手机网页上校对手写转写、全文搜索、选 AI 模型提问，再投回设备笔记本（标题、列表、复选框用设备自带样式）或导出 Obsidian markdown | [`notes/`](notes/README.md) |
| 系统层面的小改进 | 荧光笔划中文"划哪吸哪"；xochitl 阅读器单击翻页；xochitl 界面字体（书库、设置、对话框换成你选的字体，阅读字体不变）；阅读字体和休眠壁纸上传即用。已移除：手写笔画优化、耗电诊断「电池刺客」（2026-09-30）、日漫翻页规则（2026-10-07） | [`enhance/`](enhance/README.md) |
| 在手机或电脑上统一操作以上功能 | 一个 HTTPS 网页入口，带登录密码，把请求转给各个服务；另有「设备健康」页和固件升级后的重装提示 | [`gateway/`](gateway/README.md) |
| 新设备一条命令装完 | 带固件兼容性校验，装完自动整机重启一次并只读核对设备；也能一条命令卸载 | [`packaging/`](packaging/README.md) |

另有两个"幕后"目录：[`rmsvc-core/`](rmsvc-core/README.md)（各网页服务共用的基础库，不含业务逻辑）和
[`defw/`](defw/README.md)（xochitl 3.28.0.172 的逆向分析资料，用来给 xovi 插件找挂接点）。

所有功能都跑在设备的**官方系统**上，不重新打包 xochitl。整体架构（详见 [`docs/OVERVIEW.md`](docs/OVERVIEW.md)）：

![整体架构与部署拓扑](docs/diagrams/architecture-topology.svg)

## 快速开始

**只支持 reMarkable Paper Pro Move、固件 3.28.0.172**。这是目前唯一验证过的版本，安装脚本会核对，不符默认拒装。

1. 在设备上手动装好两样基础组件：`vellum add xovi`、`vellum add qt-resource-rebuilder`。它们不属于本项目，安装脚本不代装。2026-09-29 起不再需要 appload 和 KOReader（设备只用自带阅读器）。
2. 电脑上要有 Rust 交叉编译环境，并能免密 ssh 到设备的 root（清单见 [INSTALL.md「装之前」](docs/INSTALL.md#装之前)）。用 USB 线连上设备，然后：

   ```sh
   git clone https://github.com/bbq191/cang-jie.git
   cd cang-jie/packaging
   sh install-all.sh --dry-run        # 先预演：只打印计划，不连设备
   sh install-all.sh 10.11.99.1
   ```
   最后一步会整机重启设备一次（约 1 分钟），回来后脚本自动核对，逐项打 ✓/⚠/✗。
3. 浏览器打开 `https://10.11.99.1/`，默认密码 `shelf`，第一次登录必须改密码。

以前装过、要更新：直接重跑 `sh install-all.sh 10.11.99.1`，内容没变就不重启；已移除的组件（电池刺客、手写优化）会顺手清掉。

卸载：`sh uninstall-all.sh 10.11.99.1`。前置条件、风险、固件升级（OTA）后怎么恢复，全部在 **[docs/INSTALL.md](docs/INSTALL.md)**。

**改代码的人**：每次 push / PR，GitHub Actions 会自动跑 shellcheck、安装脚本的沙箱模拟测试、网关网页脚本的检查与 node 测试、enhance 两个扩展的 C 单测、各 Rust crate 的 `cargo test` 和 aarch64 交叉编译（配置在 `.github/workflows/ci.yml`）。它只证明代码在电脑上行为正确，**改变设备行为的改动仍要在真机上验证**。

## 文档导航

![文档地图](docs/diagrams/docs-map.svg)

| 想了解 | 读 |
|---|---|
| 10 分钟看懂全貌（架构图、一本书的旅程、术语表） | [docs/OVERVIEW.md](docs/OVERVIEW.md) |
| 怎么装、怎么卸、固件升级后怎么恢复 | [docs/INSTALL.md](docs/INSTALL.md) |
| 安装脚本内部结构与本机测试（开发者） | [packaging/README.md](packaging/README.md) |
| 最近改了什么 | [docs/CHANGELOG.md](docs/CHANGELOG.md) |
| 各条线的细节与决策记录 | 上表各目录的 README 及其 `docs/` 白皮书 |
| 打赏 | [docs/DONATE.md](docs/DONATE.md) |

## 历史与范围

项目最早是"reMarkable 中文输入法"（仓库名 `cang-jie` 即"仓颉"，传说中造字的人），后来陆续做了阅读增强、
知识管理、手写识别。2026-09-11 做过一次大整理：中文输入法、早期的完整阅读管线、知识管理和手写识别，
连同它们的逆向分析资料，整体移出了这个仓库。**它们的源码不在这里，也不随本仓库的安装脚本分发。**

现在仓库维护的就是上表这几条线。架构比早期保守：以独立网页服务为主，xovi 插件只保留少数几个，
尽量少碰官方系统的内部。

## 打赏

如果这个项目帮到了你，欢迎请作者喝杯咖啡。纯自愿，打不打赏都不影响任何功能。收款码见 **[DONATE.md](docs/DONATE.md)**。

## 协议 / 免责

本仓库代码以 **[Apache License 2.0](LICENSE)** 开源，可自由使用、修改、分发（含商用），需保留版权声明；
协议本身附带专利授权条款。原本作为个人自用工具开发，源码按现状（as-is）提供，不附带任何担保。

涉及第三方许可证的地方（词典数据、字体等）以代码注释里的实测记录为准，不构成法律意见。与 reMarkable、
[xovi](https://github.com/asivery/xovi)、vellum、KOReader 等第三方项目或商标没有从属关系。
