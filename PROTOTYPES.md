# Python 原型索引（host 侧 ↔ 上机生产）

本仓库**所有 Python 都是 host 侧**（测试 / 算法原型 / 离线工具），**没有一行跑在设备上**——
设备上的交付是 Rust（`reading/device-rs`、`knowledge/pkm/`）、C（`chinese-ime/langhook`、`pinyin-engine/c`）
与 qmldiff。Python 按项目纪律「有 Python 参照的算法先 Python 后 Rust/C，逐字节差分对拍」
留作**算法原型 + 对拍基准**，故与生产码**共置**（改算法时两地紧挨、diff-test 直接读同目录）。

本文件是这条「host 原型 ↔ 上机生产」映射的**单一索引**：想找某个上机模块的 Python 原型在哪、
怎么跑，看这张表即可。**统一环境已由根 uv 管理**（单个 `.venv` + 依赖按线分组），不必逐目录建环境。

## 统一 uv 环境

```bash
uv sync                 # 建 .venv 并装 default-groups（weread/knowledge/pkm/pinyin/tools/test）
uv sync --all-groups    # 装全部线
uv run pytest <path>    # 在统一环境跑测试
```

依赖按线分组在根 [`pyproject.toml`](pyproject.toml) 的 `[dependency-groups]`：
`weread` · `pkm` · `pinyin` · `tools` · `test`。`package = false`（虚拟项目：只管环境不构建自身）。

> **例外（刻意不进统一环境）**：`chinese-ime/pinyin-engine/ui/` 的候选栏 GUI demo 依赖 **PySide6**，
> 重且仅演示用，走**系统 python3**（见 `ui/render_demo.py` 注释），不装进 `.venv`。

## 映射表：上机生产模块 → host Python 原型/工具

| 块 | 上机生产（设备上跑） | host Python 原型/工具 | uv 组 | 怎么跑 |
|---|---|---|---|---|
| 2 中文化 | `chinese-ime/pinyin-engine/c`（C → `langhook.so`） | `chinese-ime/pinyin-engine/src`（拼音/双拼/繁体/词典**算法原型**）+ `tests` | `pinyin` | `cd chinese-ime/pinyin-engine/c && make diff-check`（逐字节对拍 C↔Python，用根 `.venv`）；`uv run pytest chinese-ime/pinyin-engine/tests` |
| 3 阅读 | `reading/device-rs`（Rust：sign/codec/download/epub…） | `reading/protocol`（微信读书协议**原型**，已移植 Rust）+ `reading/highlights`（`.rm` 反解 `reverse.py`） | `weread` | `uv run pytest reading/tests`；对拍参照见各 `.py` |
| 3 阅读 | —（host 侧编排，非上机） | `reading/tools`（**活跃 host CLI**：`weread.py`/`login.py`/`sync_highlights.py`…，你实际在跑的） | `weread` | `uv run python reading/tools/weread.py …` |
| 5 PKM | `knowledge/pkm/`（Rust：`stardetect`/`cardsync`/`cardnote` + `wr-stars-daemon`） | `knowledge/pkm-semantic/proto`（★待办星检测**算法原型 + 阈值标定 + 真机 fixture**） | `pkm` | `cd pkm-semantic && uv run pytest proto`（18 条差分）；对拍锚点=`knowledge/pkm/tests/stars_fixture.rs` 读 `knowledge/pkm-semantic/proto/testdata` |
| 杂项 | —（host 只读工具） | `rm-export/export.py`（扫 xochitl 镜像 → 高亮导 Markdown，复用 `reading/highlights/reverse.py`） | `weread` | `uv run python rm-export/export.py --src <镜像> --out <目录>` |
| 杂项 | —（离线构建工具） | `xovi-extensions/reading-qol/tools/extract_qml.py`（从 xochitl 二进制解 QML） | `tools` | `uv run python xovi-extensions/reading-qol/tools/extract_qml.py …` |

## 反向：原型 ↔ 生产的就近指针

各生产模块的源码/README 里带有指向其 Python 原型的注释（例：`knowledge/pkm/src/stardetect.rs`
顶部 `//! 移植自 knowledge/pkm-semantic/proto`），各原型的 README 也指回生产实现（例
[`knowledge/pkm-semantic/README.md`](knowledge/pkm-semantic/README.md) → `knowledge/pkm/`）。**新增原型/移植时，请同步更新
本表 + 两端指针**，保持这条映射不漂移。

## 为什么原型不集中到一个文件夹

评估过「把所有 host Python 抽到顶层 `prototypes/`」：视觉上 host/上机分层更直观，但会**切断对拍共置**
（`knowledge/pkm/tests` 读 `knowledge/pkm-semantic/proto/testdata`、`pinyin-engine/c` 的 `make diff-check` 读同树 Python）、
使几十处「移植自 X」指针失效、且与「按功能块组织」的仓库结构对冲。共置在本项目是**差分测试刚需**，
不是待整理的乱——所以选择**保持块内共置 + 本索引**：统一管理靠根 uv 已达成，可发现性靠本文件补齐。
