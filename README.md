# cang-jie

把 [reMarkable Paper Pro Move](https://remarkable.com/) 的官方阅读/笔记程序 **xochitl** 从"能用"改造成"顺手"的个人项目：起于**中文输入法 + 界面汉化**，现已长成一套围绕 xochitl 的**设备增强套件**（中文化 · 阅读 · 系统增强 · PKM · 截图）。全程**不改 xochitl 本体**——各项能力以合规 [xovi](https://github.com/asivery/xovi) 扩展 / qmldiff / 设备端自足二进制的形式，放 `xovi/extensions.d/` 等处运行时动态改行为，磁盘上的原始二进制原封不动。

> 本项目是个人设备自用、不对外分发。涉及许可证的数据（rime-ice/iorest 词典、微信读书正文等）不编译进 `.so`，只做独立文件运行时只读 / 仅本机渲染，产物不进入任何再分发渠道。

## 六大分块

项目按功能/产品线组织成 6 块。目录名基本与分块对齐（2026-08-23 已把历史名 `weread-client/` 拆成 `reading/` + `pkm/`）；仅剩一处刻意的跨块共享，见下方标注：

| # | 块 | 内容 | 落点 | 状态 |
|---|---|---|---|---|
| 1 | **逆向基座** | 反编译工程 + 固件镜像，离线定位 hook 点/偏移/参数签名，一切能力的共享地基 | `ghidra-project/` · `rmfw/` | 持续维护 |
| 2 | **中文化**（显示 + 输入法） | UI 汉化（`.qm` 简/繁/港 + 字体 + 原生 Settings 集成，M0–M2）+ 拼音输入法（键盘 hook + 拼音/双拼引擎 + 候选栏 + 中英混输，M3–M7） | `chinese-ime/` | 真机全链路通过，收尾维护 |
| 3 | **阅读**（微信读书 + EPUB 优化） | 「墨香」设备自足微信读书（扫码/下书/回传/续期）+ 通用 EPUB 优化器 | `reading/` | 真机端到端验证 |
| 4 | **系统增强**（阅读/显示/笔记 UX） | 点击翻页 · 快速黑白 · 清残影 · 键盘 Mono · 阅读字体 · 荧光笔汉字精确吸附；集中在设置页「系统增强」面板 | `xovi-extensions/` + `chinese-ime/langhook/`（笔记增强） | 真机验证 |
| 5 | **PKM / 知识管理** | ★全局待办语义引擎（把设备变成 Zettelkasten 工作台的首个能力）：Python 原型标定 + Rust 生产 | `pkm-semantic/`（原型）+ `pkm/`（Rust 生产 crate，单向依赖 `reading/`） | 原型标定 + 真机端到端 |
| 6 | **额外应用** | 截图/录屏工具 | `screenshot-tool/` | 规划中（独立进程 DRM 直读已判死，须 hook xochitl） |

> **一处刻意的跨块共享**：块 4 的"笔记增强"（荧光笔吸附）逻辑是 `chinese-ime/langhook` 里的 C hook、开关 UI 在 `xovi-extensions/reading-qol`——一颗 .so 同时服务块 2 和块 4，不拆二进制。
>
> **块 5 PKM 与块 3 阅读的依赖关系**：`pkm/` 是独立 Rust crate，但**单向依赖** `reading/device-rs`（复用其 `epubindex`/`fswatch`/`inject`/`notebook_rm`）——★待办本就建在阅读栈上（读书、注入书库）。`reading/` 不反向依赖 `pkm/`。这是 2026-08-23 把历史名 `weread-client/`（曾把阅读+PKM 塞一个 crate）拆开后的正确形状。

## 白皮书（完整设计 + 真机调试记录）

- **[reMarkable 拼音输入法白皮书](chinese-ime/docs/reMarkable拼音输入法白皮书.md)** —— 块 2 输入法这条线（M3–M7）。含"从新机到当前进度"的完整复现主线。
- **[reMarkable 中文化白皮书](chinese-ime/docs/reMarkable中文化白皮书.md)** —— 块 2 UI 汉化这条线（M0–M2）。共享的环境搭建 / xovi 基础设施出处。
- **[功能路线图白皮书](docs/reMarkable功能路线图白皮书.md)** —— 跨块的"下一步做什么"优先级共识 + 6 分块地图。块 3/4/5 的设计与推进历程都落在这里，代码就近落在各自目录（`reading/` 含 README + `ATTRIBUTION.md`），不另开方案文档。

## 目录结构

| 路径 | 块 | 作用 |
|---|---|---|
| `ghidra-project/` | 1 | 反编译工程：离线定位 hook 点 / 偏移 / 参数签名 |
| `rmfw/` | 1 | 固件镜像 `out/`+`extracted/`（通用侦查资源；中文字体已移进 `chinese-ime/fonts/`） |
| **`chinese-ime/`** | 2 | **中文化核心交付**（归拢），下面几项 |
| `chinese-ime/langhook/` | 2(+4) | 设备端 hook：编译成 `cangjie-langhook.so`（合规 **xovi 扩展**，放 `extensions.d/` 自动加载），承载键盘 hook/拼音缓冲/候选栏全部逻辑 + **块 4 的荧光笔汉字吸附**；`deploy/` 含**一键安装包**。见 [langhook/README.md](chinese-ime/langhook/README.md) |
| `chinese-ime/pinyin-engine/` | 2 | 拼音引擎离线核心：`src/`（Python 参照）+ `c/`（C 移植 + blob 工具 + 差分测试）+ `data/`（词库 + 许可证留痕）+ `ui/` + `tests/` |
| `chinese-ime/{fonts,translations,docs}/` | 2 | 中文字体（+OFL）· zh `.qm` 翻译 · 两本白皮书 |
| `reading/` | 3 | 阅读线设备端总仓：「墨香」微信读书自足化 + 通用 EPUB 优化器（`device-rs/` Rust）。见 [reading/README.md](reading/README.md)、`ATTRIBUTION.md`（历史名 `weread-client/`，2026-08-23 改名） |
| `xovi-extensions/` | 4 | `reading-qol`（点击翻页/快速黑白/清残影/键盘 Mono + 设置页「系统增强」面板）+ `font-menu`（阅读字体） |
| `pkm/` | 5 | PKM ★待办**生产 Rust crate**：`stardetect`/`cardsync`/`cardnote` + `wr-stars-daemon`；单向依赖 `reading/device-rs`。见 [pkm/README.md](pkm/README.md) |
| `pkm-semantic/` | 5 | PKM ★待办检测算法的 **Python 原型 + 阈值标定**（`pkm/` 是其逐结果对拍的 Rust 生产移植）。见 [pkm-semantic/README.md](pkm-semantic/README.md) |
| `screenshot-tool/` | 6 | 截图/录屏可行性实验（`drm-probe/`），规划中 |
| `docs/` | — | **跨块白皮书**：功能路线图（含 6 分块地图）+ 网络解决方案（中文化/拼音两本就近在 `chinese-ime/docs/`） |
| `assets/` · `rm-export/` | — | 杂项媒体（logo/截图/演示，不参与构建）· `.rm` 导出脚本 |
| `pyproject.toml` · `uv.lock` | — | uv 统一 Python 环境（依赖按线分组），本地开发用 |
| 工程纪律 | — | 工程纪律（真机验证再宣称完成、一步一确认、改设备前备份等） |

## 快速开始

### 装到一部新机（SSH 后一键安装）

见 [chinese-ime/langhook/README.md](chinese-ime/langhook/README.md) 的「一键安装」——前置装好开发者模式/SSH + xovi/qt-resource-rebuilder（vellum 装的或官方），然后：

```bash
scp chinese-ime/langhook/deploy/dist/cangjie-ime-installer.tar.gz root@10.11.99.1:/home/root/
ssh root@10.11.99.1 'cd /home/root && tar -xzf cangjie-ime-installer.tar.gz && /home/root/cangjie-ime/install.sh'
```

### 本地开发 / 测试（不需要设备）

```bash
# 拼音引擎：Python 单测 + C 差分测试
cd chinese-ime/pinyin-engine && python3 -m pytest -q
cd chinese-ime/pinyin-engine/c && make test && make diff-check

# 设备端 hook：宿主机单测 + aarch64 交叉编译（xovi 扩展需 xovigen，XOVI_DIR 指向 xovi clone）
cd chinese-ime/langhook && make test && make aarch64 XOVI_DIR=<asivery/xovi clone 路径>
```

## 当前进度（块 2 中文化线）

> 其余各块的推进历程见[功能路线图白皮书](docs/reMarkable功能路线图白皮书.md)（块 3 墨香/EPUB、块 4 系统增强、块 5 PKM ★待办）。

- **M0–M2（UI 汉化）**：交叉编译工具链、xovi、字体 subset、`.qm` 翻译（简/繁/港）、原生 Settings 语言集成——基本收工。
- **M3（虚拟键盘 hook）/ M4（拼音候选可用，含逐字造句/分段提交/退格撤销）/ M6（双拼 + 繁体）**：真机全链路验证通过。
- **M5（全局可用 + 原生入口）**：核心已达成（hook 打在系统级 `VirtualKeyboard` 上、原生入口走键盘语言弹层），收尾打磨中。
- **中英混输（Phase C）**：增量式词典补全（"你好hello"），真机验证。
- **字体**：候选栏 + 整个 UI 按语言用 HarmonyOS Sans SC/TC。
- **M7（长期维护）**：多轮实战——① 固件 OTA 后各 hook 地址位移，做了**韧性重构**（每个目标字节特征码运行期自定位、掩码通配相对跳转、metaobject 靠 static_metacall 指针反查），新固件全命中、不需再推导偏移；② 设备装 vellum 后，`cangjie-langhook.so` 重构成**合规 xovi 扩展**放 `extensions.d/`（/home 持久），弃独立 drop-in；xovi 启动配置放 `/usr/lib`（rootfs，普通重启不丢），界面翻译放 `/usr/share`；固件 OTA 冲掉 rootfs 后重跑 `install.sh` 一键恢复。全部真机验证通过。详见 [langhook/README.md](chinese-ime/langhook/README.md)。
