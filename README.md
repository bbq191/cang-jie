# cang-jie

**[English](docs/README.en.md)**

reMarkable Paper Pro Move 的设备增强套件——**不修改 xochitl（设备官方阅读应用）本体**，通过
xovi 扩展 + 独立 Web 服务，给设备加上书籍管理、笔记增强、系统优化等一整套功能。

> **读者与用途**：第一次来到这个仓库的人。这里回答"这是什么、有哪些部分、怎么装、去哪看细节"；
> 10 分钟看懂全貌读 [`docs/OVERVIEW.md`](docs/OVERVIEW.md)，最近改了什么读 [`docs/CHANGELOG.md`](docs/CHANGELOG.md)。
>
> 这是带完整开发历史的**私有**仓库；另有一份精简的公开发行版 [`rm-tweak`](https://github.com/bbq191/rm-tweak)
> （不带开发历史，只含当前七条项目线的代码和顶层 README/INSTALL 等文档，Apache-2.0 开源）——对外分享/收藏用那个。

## 这是什么

围绕一台墨水屏平板搭的一套个人自用工具：把书弄进设备、按需清洗优化、选去哪个读器（原生
xochitl 或 KOReader）；荧光笔勾画配合手写批注，自动转写成 AI 可处理的笔记，同步进设备笔记本
或 Obsidian；再加上几个底层单点增强（划词精确吸附、手写笔锋渲染、电池诊断、字体/壁纸一键换）。
所有功能都跑在设备的官方系统上，靠 [xovi](https://github.com/asivery/xovi)（第三方扩展加载
框架）+ 一组独立的轻量 Web 服务实现，不侵入、不重打包 `xochitl` 本身。

## 功能一览

七条独立的顶层项目线，各自可插拔、可单独安装：

| 项目 | 是什么 | 文档 |
|---|---|---|
| [`shelf/`](shelf/) 书架 | 书籍（EPUB/PDF）导入 → 母版库 → 按需优化 → 加入原生书库或 KOReader；纯网页操作（含批量队列） | [README](shelf/README.md) |
| [`notes/`](notes/) 笔记线 | 荧光笔勾画 + 旁边手写批注，合书自动摄取 → 手机整理页校对/选 AI 模型转写与问答 → 投影回设备笔记本或 Obsidian | [README](notes/README.md) |
| [`enhance/`](enhance/) 系统增强 | 荧光笔 CJK 精确吸附、手写笔锋渲染优化（两个独立 xovi 扩展）；电池诊断采样器；字体/壁纸上传即用（两个 Web 服务） | [README](enhance/README.md) |
| [`gateway/`](gateway/) 网关 | 上面三条线共用的唯一 Web 入口：HTTPS（私有 CA）+ 登录密码 + 反向代理到各领域服务 + 批量队列与并发闸门 | [README](gateway/README.md) |
| [`rmsvc-core/`](rmsvc-core/) 服务基座 | 各 Web 服务共用的基础设施 crate（路径/注册表/HTTP 适配/事件总线等），不含任何业务逻辑 | [README](rmsvc-core/README.md) |
| [`defw/`](defw/) 固件逆向 | xochitl 3.28.0.172 的 Ghidra 逆向工程产物，支撑上面几个 xovi 扩展的 hook 定位 | [README](defw/README.md) |
| [`packaging/`](packaging/) 安装器 | 全新设备一条命令装完以上全部（含固件兼容性校验） | [README](packaging/README.md) |

## 近期更新

用户可见的变化按日期记在 **[docs/CHANGELOG.md](docs/CHANGELOG.md)**。最近几项：漫画优化保持 EPUB、书名统一规范、突破 xochitl 约 100MB 上传上限（占位 + 磁盘替换）、服务端批量队列与母版库页重做、优化提速与省电。

## 快速开始

只支持 **reMarkable Paper Pro Move，固件 3.28.0.172**（目前唯一验证过的版本）。

```sh
git clone https://github.com/bbq191/rm-tweak.git   # 公开发行版；私有开发仓库 cang-jie 只有维护者能 clone
cd rm-tweak/packaging
sh install-all.sh 10.11.99.1
```

完整的前置条件、分步说明、固件安全门、故障排查见 **[INSTALL.md](docs/INSTALL.md)**。

## 文档导航

| 想了解 | 读 |
|---|---|
| 10 分钟看懂全貌（架构图、一本书的旅程、术语表） | [docs/OVERVIEW.md](docs/OVERVIEW.md) |
| 怎么安装、固件升级后怎么恢复 | [docs/INSTALL.md](docs/INSTALL.md) |
| 最近改了什么 | [docs/CHANGELOG.md](docs/CHANGELOG.md) |
| 各条线的细节与决策记录 | 上表各线 README 与其 `docs/` 白皮书 |

## 历史与范围

这个项目最早从"reMarkable 中文输入法"起步（仓库名 `cang-jie` 即"仓颉"，中国传说中造字的人），
后来陆续长出了阅读增强、PKM 知识管理、手写识别等更多方向。2026-09-11 做过一次大规模仓库整理：
中文输入法、完整阅读管线、PKM/手写识别这几条较早期的功能线，连同它们的逆向基座代码，整体移出
了这个 git 仓库（不是删除，是搬到维护者本机另一个不随 git 走的目录）——**这些功能目前在设备上
仍是现役、仍在正常运行**，只是源码不再是这个仓库的一部分，也不再有活跃开发。当前仓库积极维护
的是上表这七条线，走的是比早期更保守的架构（独立 Web 服务 + 尽量小的 xovi 扩展面），跟设备官方
系统的耦合程度比早期低很多。

## 打赏

如果这个项目帮到了你，欢迎请作者喝杯咖啡——纯自愿，打不打赏都不影响任何功能的使用。收款码、
金额使用说明等详见独立页面 **[DONATE.md](docs/DONATE.md)**。

## 协议 / 免责

个人自用项目，不对外分发预编译产物。涉及第三方许可证的地方（词典数据、字体等）以代码内注释的
实测记录为准，不构成法律意见。跟 reMarkable、[xovi](https://github.com/asivery/xovi)、vellum、
KOReader 等第三方项目/商标没有从属关系，均为独立第三方工具/商标。
