# cang-jie

**[English](docs/README.en.md)**

reMarkable Paper Pro Move 的设备增强套件。**不修改 xochitl**（设备自带的阅读/笔记应用）本体，
而是用两样东西给它补功能：[xovi](https://github.com/asivery/xovi)（第三方扩展加载框架）加载的小插件，
和一组跑在设备上的网页服务。补的是书籍管理、笔记整理和一些系统层面的小改进。

> **这一页讲**：能做什么、怎么装和卸、细节去哪看。
> 想 10 分钟看懂全貌，读 [`docs/OVERVIEW.md`](docs/OVERVIEW.md)；想知道最近改了什么，读 [`docs/CHANGELOG.md`](docs/CHANGELOG.md)。
>
> 这是带完整开发历史的**私有**仓库。对外分享的是精简的公开发行版 [`rm-tweak`](https://github.com/bbq191/rm-tweak)
> （没有开发历史，Apache-2.0）。2026-09-25 同步过一次：有 shelf / notes / enhance / gateway / rmsvc-core / packaging
> （含 `install-all.sh` 和 `verify-on-device.sh`），没有 `defw/`。

## 能做什么

| 你想要 | 对应的功能 | 在哪 |
|---|---|---|
| 把书弄进设备，而且在墨水屏上好读 | 网页上传或抓网文，书先进"母版库"。EPUB 可一键"优化"（排版、目录、封面、脚注）；有文字层的 PDF 按原格式转成 EPUB。然后加入原生阅读器或 KOReader（KOReader 预置文字书 / 漫画两套阅读方案）。超过 xochitl 约 100MB 上传上限的大书也能整本进（最大 1GB），漫画有专门处理；母版库里的书可以下载原件、改名 | [`shelf/`](shelf/README.md) |
| 把荧光笔勾画和旁边的手写批注变成能整理的笔记 | 合上书自动收进条目库。在手机网页上校对手写转写、全文搜索、选 AI 模型提问，再投回设备笔记本（标题、列表、复选框用设备自带样式）或导出 Obsidian markdown | [`notes/`](notes/README.md) |
| 系统层面的小改进 | 荧光笔划中文"划哪吸哪"、手写笔画粗细优化、xochitl 阅读器单击翻页与日漫翻页规则、电池耗电诊断、字体和壁纸上传即用 | [`enhance/`](enhance/README.md) |
| 在手机或电脑上统一操作以上功能 | 一个 HTTPS 网页入口，带登录密码，把请求转给各个服务；另有「设备健康」页和固件升级后的重装提示 | [`gateway/`](gateway/README.md) |
| 新设备一条命令装完 | 带固件兼容性校验，装完自动整机重启一次并只读核对设备；也能一条命令卸载 | [`packaging/`](packaging/README.md) |

另有两个"幕后"目录：[`rmsvc-core/`](rmsvc-core/README.md)（各网页服务共用的基础库，不含业务逻辑）和
[`defw/`](defw/README.md)（xochitl 3.28.0.172 的逆向分析资料，用来给 xovi 插件找挂接点）。

所有功能都跑在设备的**官方系统**上，不重新打包 xochitl。整体架构（详见 [`docs/OVERVIEW.md`](docs/OVERVIEW.md)）：

![整体架构与部署拓扑](docs/diagrams/architecture-topology.svg)

## 快速开始

**只支持 reMarkable Paper Pro Move、固件 3.28.0.172**。这是目前唯一验证过的版本，安装脚本会核对，不符默认拒装。

1. 在设备上手动装好四样基础组件：`vellum add xovi`、`vellum add qt-resource-rebuilder`、`vellum add appload`（≥ 0.6.0），再经 appload 侧载 KOReader。它们不属于本项目，安装脚本不代装。
2. 电脑上要有 Rust 交叉编译环境，并能免密 ssh 到设备的 root（清单见 [INSTALL.md「装之前」](docs/INSTALL.md#装之前)）。用 USB 线连上设备，然后：

   ```sh
   git clone https://github.com/bbq191/rm-tweak.git
   cd rm-tweak/packaging
   sh install-all.sh --dry-run        # 先预演：只打印计划，不连设备
   sh install-all.sh 10.11.99.1
   ```
   最后一步会整机重启设备一次（约 1 分钟），回来后脚本自动核对，逐项打 ✓/⚠/✗。
3. 浏览器打开 `https://10.11.99.1/`，默认密码 `shelf`，第一次登录必须改密码。

卸载：`sh uninstall-all.sh 10.11.99.1`。前置条件、风险、固件升级（OTA）后怎么恢复，全部在 **[docs/INSTALL.md](docs/INSTALL.md)**。

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

个人自用项目，不对外分发预编译产物。涉及第三方许可证的地方（词典数据、字体等）以代码注释里的实测记录为准，
不构成法律意见。与 reMarkable、[xovi](https://github.com/asivery/xovi)、vellum、KOReader 等第三方项目或商标没有
从属关系。
