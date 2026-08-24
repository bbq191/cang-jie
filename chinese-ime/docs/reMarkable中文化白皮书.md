# reMarkable Paper Pro Move 中文化开发方案白皮书

> 开发方案白皮书 · v1.0

UI 界面本地化（简体 + 繁体：语言列表、翻译文件、字体）——面向零基础但愿意动手的读者，以下内容假设我会一点 Linux 命令行和基础的 C++/Python，其余的（Qt、交叉编译、逆向分析）文中都会先解释概念再给步骤。

**范围声明：** 本方案只处理"本机个人定制"——让我自己的设备显示中文、能打中文字。它不涉及破解 DRM、绕过付费校验或批量分发修改版固件；法律与许可合规相关的提醒集中在第 06 节，动手前建议先看一遍。**拼音/双拼输入法引擎、虚拟键盘 hook 是独立的姊妹文档**《[reMarkable Paper Pro Move 拼音输入法开发方案](reMarkable拼音输入法白皮书.md)》——两份文档原本是一份，输入法这条线的真机调试记录篇幅远超本文档，遂拆分，本文档第 02 节的环境搭建结论、第 04 节的 xovi hook 基础设施两份文档共用。微信读书的原生阅读方案是另一个独立主题，见同目录下《微信读书方案白皮书.html》——那份文件的环境搭建部分同样直接复用本文第 02 节的结论，不重复展开。

## 本章目录

1. [01 · 总体架构图解](#总体架构图解)
2. [02 · 开发环境搭建](#开发环境搭建)
3. [03 · UI 中文化模块](#ui-中文化模块)
4. [04 · xovi Hook 与原生 Settings 集成](#xovi-hook-与原生-settings-集成)
5. [05 · 里程碑与时间表](#里程碑与时间表)
6. [06 · 风险清单与合规提醒](#风险清单与合规提醒)

附：[▶ 完整复现路径（从新机到当前进度）](#完整复现路径从拿到新机到当前进度) · [▤ 项目目录结构](#项目目录结构cang-jie)

## ▶ 完整复现路径：从拿到新机到当前进度

这一节把 UI 汉化这条线拉成一条能从头照着走的主线——从我拿到一部全新的 Paper Pro Move，到现在 M0–M2 基本收工的状态，每步都链到下面对应的深入小节（踩坑细节都在那里，这里只列主干）。

1. **拿到新机 → 打开开发者模式 → 建立连接**：设备设置里开开发者模式（会清空数据），走 USB gadget 网卡 `10.11.99.x` 网段 SSH 进 `root@10.11.99.1`。见 02 节。
2. **搭建开发环境**：交叉编译工具链（aarch64）、`xovi` hook 框架、Ghidra 反编译环境。见 02 节。
3. **离线侦查（Ghidra，`ghidra-project/`）**：反编译 `LanguageSettings`，用 `qt_metacall` 分派点 + `QMetaObject` 字符串表解出它的 `Q_PROPERTY`/`Q_INVOKABLE`（`availableLanguageCodes`/`setLanguageCode`/`getLanguageDisplayName` 等）。见 04 节。
4. **UI 中文化模块**：装中文字体（OFL 授权字体，`rmfw/fonts/`）+ 准备翻译文件 + 菜单项汉化。见 03 节。
5. **xovi Hook 与原生 Settings 集成**：hook `LanguageSettings`，把"简体中文"/"繁体中文"追加进原生语言列表并修好重名显示。见 04 节。
6. **编译 + 部署 + 健康检查**：交叉编译零警告出 `.so`；部署前备份、`scp` 到 `/home/root/`、核对 md5、`systemctl restart xochitl`、确认 `is-active`=active / `MainPID` 变化 / `NRestarts` 不增。
7. **真机逐项验证**：原生 Settings 语言列表里出现中文项、切换后界面文字与字体正常、无重名。每步独立验证、出问题立即回退。
8. **当前进度**：M0–M2（环境/字体/菜单汉化 + 原生 Settings 语言集成）基本收工；虚拟键盘/拼音输入法这条线（M3 及以后）见姊妹文档《拼音输入法开发方案》。详见 05 节里程碑。

## ▤ 项目目录结构（cang-jie）

整个 `cang-jie` 仓库的顶层文件夹作用如下，**加粗的是 UI 汉化这条线直接相关的**；拼音输入法那条线（`pinyin-engine/` 等）见姊妹文档《拼音输入法白皮书》。

> **目录已重构（2026-08-15）**：中文化的核心交付归拢进 **`chinese-ime/`** 子文件夹（本白皮书现在就在 `chinese-ime/docs/`）。以下路径为新结构；文中其余处若写旧路径（如 `xovi-extensions/cangjie-langhook/`），一律对应 `chinese-ime/langhook/`。

- **`chinese-ime/langhook/`** — **设备端 hook**：交叉编译成 `cangjie-langhook.so`，是**合规 xovi 扩展**（放 `xovi/extensions.d/` 被自动加载，不再靠独立 LD_PRELOAD）。同一份 `.so` 同时承载 UI 汉化（`LanguageSettings` hook）和拼音输入法两条线。
- **`chinese-ime/fonts/`** — 中文化用的字体（+ OFL），从旧 `rmfw/fonts/` 移入。
- **`chinese-ime/translations/`** — zh `.qm` 界面翻译（简/繁/港）。
- **`chinese-ime/pinyin-engine/`** — 拼音输入法离线核心（Python 参照 + C 移植 + 词库 + 候选栏原型），见姊妹文档《拼音输入法白皮书》。
- `chinese-ime/docs/` — 本册 + 《拼音输入法白皮书》。
- **`ghidra-project/`**（顶层）— **反编译工程**：Ghidra 项目 + headless 脚本，定位类/属性/方法/偏移的离线侦查产物。
- `rmfw/`（顶层）— 固件镜像 `out/`+`extracted/`（通用侦查资源；中文字体已移进 `chinese-ime/fonts/`）。
- `xovi-extensions/`（顶层）— `reading-qol`+`font-menu`（阅读增强，非中文化本身）。
- `reading/`（顶层，历史名 `weread-client/`）+ `pkm/` — 独立子项目（阅读/PKM），跟中文化无关。
- 工程纪律（顶层）— 工程纪律。

## 01｜总体架构图解

动手之前先搞清楚三个东西的关系：**xochitl**（设备上运行的官方阅读/笔记程序，闭源，Qt6 编写）、**xovi**（我借来"钻空子"的第三方 hook 框架）、以及我要新增的**中文语言包模块**。全程不改 xochitl 本身的任何一个文件——这也是"不侵入"的含义：xovi 在进程运行时动态插入代码，原始二进制在磁盘上原封不动。

> **本文档只覆盖 UI 汉化这一半**——虚拟键盘 hook、拼音/双拼引擎、候选词典是独立的姊妹文档《[reMarkable Paper Pro Move 拼音输入法开发方案](reMarkable拼音输入法白皮书.md)》第 01 节有对应的、更详细的架构图（两条 hook 链路：语言切换器 + 按键处理/候选栏），本图不重复画。

![总体架构 · UI 汉化（xochitl → cangjie-langhook.so → 中文语言包）](architecture-ui.svg)

> 图注：键盘/输入法这一半（另一条独立的 hook 链路）见姊妹文档，本图只画 UI 汉化。

## 02｜开发环境搭建

好消息：前面九成的开发和调试都不需要设备在手边，可以先在电脑上把能确定的部分做完，最后再集中上机联调（第 04 节末尾会再强调一次）。

### 2.1 基础工具链

开发机需要装的东西：

| 用途              | 装什么                                                         | 备注                                                                                           |
| ----------------- | -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| 固件下载/离线分析 | `codexctl`                                                     | 本方案前期调研已验证可用，见下方命令                                                           |
| aarch64 交叉编译  | `aarch64-linux-gnu-gcc/g++`                                    | Paper Pro Move 是 ARM64，本机 x86_64 开发机要交叉编译                                          |
| 构建系统          | CMake ≥ 3.20、Ninja、git                                       | xovi/qt-resource-rebuilder 用这套构建                                                          |
| Qt 开发环境       | Qt 6.8.2（与固件一致的版本）                                   | 版本要对齐，见 2.2                                                                             |
| 翻译工具链        | `qttools`（含 `lrelease`/`lconvert`）                          | 制作 .qm 语言包用                                                                              |
| 字体处理          | `fonttools`（Python 包，含 `pyftsubset`）                      | 给字体做精简（subset）                                                                         |
| 简繁转换          | `OpenCC`                                                       | 库 + 命令行工具都要                                                                            |
| 逆向辅助          | `binutils`（`nm`/`readelf`/`objdump`）、`radare2`，可选 Ghidra | 摸清 xochitl 里有哪些可 hook 的符号；`radare2` 是本方案实际用来做交叉引用分析的工具，见 3.5 节 |

```bash
# Arch / CachyOS（我现在这台机器）
sudo pacman -S aarch64-linux-gnu-gcc aarch64-linux-gnu-binutils cmake ninja git qt6-tools python-pip opencc

# Ubuntu / Debian 系
sudo apt install gcc-aarch64-linux-gnu g++-aarch64-linux-gnu cmake ninja-build git qt6-tools-dev-tools python3-pip opencc

# codexctl（前期调研已验证可用，建议装进独立 venv，不污染系统 Python）
uv venv .venv && uv pip install --python .venv/bin/python codexctl fonttools

# fonttools 的字体裁剪工具
.venv/bin/pyftsubset --help
```

### 2.2 Qt 6.8.2 从哪来

交叉编译整个 Qt6 是一个很重的工程（官方构建都要几十分钟到几小时），对个人项目来说性价比不高。更现实的两条路：

- **路线一（推荐给起步阶段）：** 直接把固件里已经编译好的 `libQt6*.so` 拿来当"链接期的现成库"——我不需要重新编译 Qt 本体，只要保证我写的扩展代码在**符号签名（ABI）**上和固件里这份 6.8.2 兼容。Qt 官方开源版的**头文件**是通用的，不依赖具体设备构建，用 Qt 官方在线安装器勾选「Desktop Qt 6.8.2」（不需要勾嵌入式/aarch64 组件）就能拿到头文件和桌面版工具链用于本地开发调试，真正跑到设备上时再用固件里的 .so 做链接目标。
- **路线二（要跑桌面模拟界面时）：** 额外装官方在线安装器里的「Qt 6.8.2 for Desktop」，本地用 `qmlscene` 或写个 Qt Quick 壳子先把候选栏、键盘布局这些 UI 组件在电脑屏幕上调好看，等真机联调时再迁移渲染后端（e-ink 没有真彩色/动画，UI 逻辑基本能直接搬）。

### 2.3 codexctl 固件离线分析（已验证）

这是前期调研已经跑通的部分，直接复用：

```bash
# 列出 Paper Pro Move 当前所有可下载版本
.venv/bin/codexctl list --hardware rmppm

# 下载最新版固件（不需要设备）
.venv/bin/codexctl download 3.27.1.0 --hardware rmppm -o out

# 不解压也能直接浏览/读取文件系统内容
.venv/bin/codexctl ls  out/remarkable-production-memfault-image-3.27.1.0-rmppm-public /usr/lib
.venv/bin/codexctl cat out/remarkable-production-memfault-image-3.27.1.0-rmppm-public /usr/share/remarkable/xochitl/translations/reMarkable_en.qm > en.qm
```

> 这套调研跑出的固件文件（`out/` 原始包 + `extracted/` 解压镜像）已经放在本机 `rmfw/` 目录下，可以直接拿来继续分析，不用重新下载。

### 2.4 xovi / qt-resource-rebuilder / rm-appload

#### ✅ 已实地克隆 xovi 仓库核实（此前这一节的构建命令是没试过的通用猜测）

xovi 的 `CMakeLists.txt` 靠 `CMAKE_SYSTEM_PROCESSOR` 判断该编译哪套架构相关源码（`aarch64` 还是 `armv6l`/`arm`），这个变量不会凭空是 `aarch64`——本机是 x86_64，直接 `cmake ..` 会命中仓库自己写的 `message(FATAL_ERROR "xovi doesn't support 'x86_64' architecture")`，也就是说下面那行 `-DCMAKE_TOOLCHAIN_FILE=../toolchain-aarch64.cmake` 里的**工具链文件不是仓库自带的，得自己写**（设置 `CMAKE_SYSTEM_NAME Linux` + `CMAKE_SYSTEM_PROCESSOR aarch64` + 交叉编译器路径），此前这份白皮书没提这一点，容易照抄命令直接卡住。更重要的是**写扩展不是直接拿 C 文件编译就行**：得先写一份 `.xovi` 描述文件（声明 import/export/override 哪些符号），跑 `python3 util/xovigen.py -o xovi.c -H xovi.h your.xovi` 生成胶水代码，再把生成的 `xovi.c` 和自己的 `main.c` 一起编译——这一步 4.2 节原来完全没提，是本节和 4.2 节都要补的关键流程，见下方 4.2 节的实测细节。

#### ✅ 已实测跑通完整的 aarch64 交叉编译链路

`cmake`/`ninja`/`aarch64-linux-gnu-gcc` 装好之后（此前一度缺失，本节上一版曾单独提示过），三层都验证过：① **M0 里程碑验收标准达标**——一个最简单的 `hello.c` 用 `aarch64-linux-gnu-gcc -static` 编译，产出确认是 `ELF 64-bit ARM aarch64 executable`；② **xovi 框架本体**用下面写好的 `toolchain-aarch64.cmake` 配合 `cmake -G Ninja` + `ninja` 从源码完整编译成功，产出 `xovi.so` 核实是合法 aarch64 动态库，`nm -D` 能看到 `_ext_init`、`dlopen`/`dlsym` 等预期符号；③ **示例扩展**（仓库自带的 `example-extension`，hook `strdup`）用 `xovigen.py` 生成胶水代码后，同样用 `aarch64-linux-gnu-gcc -std=gnu17`（4.2 节踩坑记录提到的 GCC 16 兼容性 flag）交叉编译成功，`aarch64-linux-gnu-nm` 核实符号表里 `override$strdup`、`isDuck` 都正确导出。三层全部打通，意味着这条工具链本身没有隐藏的坑——距离"真的往 xochitl 里插自己的 hook"，只剩"知道该 hook 哪个函数"这一个悬念（3.5/4.1 节已经说明这一步 offline 反汇编到了上限，需要上机）。

```bash
sudo pacman -S cmake ninja aarch64-linux-gnu-gcc aarch64-linux-gnu-binutils

git clone https://github.com/asivery/xovi.git
git clone https://github.com/asivery/qt-resource-rebuilder.git
git clone https://github.com/rmkit-dev/rm-appload.git   # 项目名以官方仓库为准，克隆前先去 GitHub 确认最新地址

# toolchain-aarch64.cmake —— 已实测跑通的版本，直接用：
cat > toolchain-aarch64.cmake <<'EOF'
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR aarch64)
set(CMAKE_C_COMPILER aarch64-linux-gnu-gcc)
set(CMAKE_CXX_COMPILER aarch64-linux-gnu-g++)
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
EOF

cd xovi && mkdir build && cd build
cmake -DCMAKE_TOOLCHAIN_FILE=../../toolchain-aarch64.cmake -G Ninja .. && ninja
# 实测输出：Linking C shared library xovi.so —— 7 个目标全部编译成功
```

### 2.5 开启 Developer Mode 并配置 SSH

1. 设备上：**设置 → 通用 → 平板电脑 → 软件 → 高级 → 开发者模式**，打开开关。
2. **这一步会强制恢复出厂设置，清空设备上当时的所有数据**——动手前务必先把笔记同步/备份好，这不是本方案引入的风险，是 reMarkable 官方机制本身如此。
3. 开发者模式启用后，SSH 密码在**设置 → 通用 → 帮助 → 关于 → 版权与许可**页面显示（每次进这个页面可能会变，是一次性口令）；在开发机上执行 `ssh-keygen` 生成密钥对，用这个一次性密码先跑一遍 `ssh-copy-id` 把公钥导入设备，之后就能免密 SSH。

#### ✅ 已实测跑通完整连接流程（Paper Pro Move，USB 连接，固件 3.28.0.164）

USB 插上后设备端会注册一个 `cdc_ether` 网卡，官方约定的固定地址是设备侧 `10.11.99.1`；如果开发机上插了网线也没反应，先排查是不是开发机自己的网络管理工具（比如 NetworkManager）把一个不相关的旧连接 profile 自动接管了这张新出现的网卡——这是实测踩到的真实原因，不是设备的问题，确认开发机侧这张 USB 网卡的 IP 落在 `10.11.99.0/24` 网段（比如手动配成 `10.11.99.2/24`）就能连通。另外，**根分区默认是只读的**（`/dev/root` 挂载参数是 `ro`）——3.3/3.5 节要写 `/usr/share/remarkable/xochitl/translations/` 这类系统目录之前，先执行 `mount -o remount,rw /`，这一步白皮书之前完全没提到，直接照抄 `scp` 命令会碰到 `Read-only file system` 报错。

#### ⚠️ 顺手提醒一个容易忽略的信息泄露点

`/home/root/.config/remarkable/xochitl.conf` 里明文存着当前的 SSH 一次性密码（`DeveloperPassword=`）、云账号的 `UserToken`/`devicetoken`（JWT，解出来能看到注册邮箱）——排查配置的时候如果要 `cat` 这个文件，注意这些字段不要贴到聊天记录、issue、截图这类会被别人看到或者被日志留存的地方；`UserToken` 这类云端 token 通常几小时就过期，风险窗口不大，但 SSH 密码和邮箱没有自动过期机制，暴露了就是暴露了。

### 2.6 没有设备时能做多少事

把"必须要设备"的工作压缩到最后一步，前面全部离线完成：

| 工作项                              | 是否需要设备 | 怎么离线做                                                         |
| ----------------------------------- | ------------ | ------------------------------------------------------------------ |
| 固件文件系统分析                    | 否           | `codexctl ls/cat`，本方案前期已验证                                |
| 制作 .qm 语言包                     | 否           | 纯文本/工具链操作，见第 03 节                                      |
| 字体 subset                         | 否           | **已完成**：`pyftsubset` 本地跑通，简/繁/港三套裁剪字体，见 3.3 节 |
| 拼音引擎核心逻辑（切分/候选/排序）  | 否           | **已完成**：真实代码 + 25 个单元测试全过，见下方说明               |
| 候选栏/键盘 UI 布局调试             | 否           | **已完成，且早已上机验证**：见姊妹文档《拼音输入法白皮书》         |
| xovi extension 是否真的 hook 成功   | 是           | 只能上真机验证，安排在集成测试阶段                                 |
| 语言列表机制（硬编码 vs 动态扫描）  | 是           | 需要把 .qm 塞进设备目录重启验证，见 3.5 节                         |
| e-ink 真实渲染效果（重影/刷新速度） | 是           | 模拟器看不出墨水屏特性，必须真机                                   |

#### ✅ 没有设备也先动手做完了这几项——本地目录 `pinyin-engine/`

音节切分（隔音符号、贪心最长匹配、"打到一半"前缀识别）+ 候选词典（真实 RIME 数据，早期约 29 万条候选，后来换源+扩容到近 59 万条）+ 双拼预处理层（小鹤双拼/自然码，官方 schema 数据驱动）+ 繁体转换层（复用 3.2 节验证过的 OpenCC 词汇级配置），**38 个单元测试全部通过**；踩过的坑（"你好"这类常用词哪来的、词典权重列语义、简繁混排）细节和后续这条线的全部真机验证记录见姊妹文档《拼音输入法白皮书》和 `pinyin-engine/README.md`。字体 subset 也跑通了：单字面从 16.2MB 裁到 1.8MB，细节见 3.3 节。另外核实了 xovi 仓库的真实构建/扩展开发流程（`xovigen` 代码生成步骤此前完全没提到、GCC 16 的一个兼容性坑），装好 `cmake`/`ninja`/aarch64 交叉编译器后，从"编个 hello world"到"xovi 框架本体交叉编译"到"示例扩展交叉编译 + 符号核对"三层全部实测跑通，**M0 里程碑的验收标准已经达标**，细节见 2.4/4.2 节。至此，白皮书里能明确划进"不需要设备"范畴的内容已经基本做完；剩下的都是 2.6 节表格里明确标了"是"（需要设备）的项目，要等真机到手才能继续。

## 03｜UI 中文化模块

### 3.1 核心技巧：不用源码也能知道要翻译什么

xochitl 闭源，我没有它的 `.ts` 翻译源文件。但 Qt 的 `.qm` 是一个**编译产物**，里面按「上下文（比如某个界面类名）+ 原文 + 译文」三元组存了完整的字符串表——也就是说，**拿现成的 `reMarkable_en.qm` 反过来解析，就能拿到 xochitl 里全部可翻译字符串的原文**，不需要一行源码。这条路径已经在 3.27.1.0（`rmppm`）固件上完整跑通，下面是实测的执行过程，包括中途踩到的一个坑。

#### ⚠️ 踩坑记录——重定向导出的二进制被 warning 文本污染

第一次用 `codexctl cat ... > reMarkable_en.qm` 重定向导出时，`lconvert` 直接报错 `QM-Format error: magic marker missing`。定位后发现 codexctl 内部有一条 `RuntimeWarning: Public key missing` 走的是 stdout 而不是 stderr，和 `>` 重定向捕获的二进制内容混在了一起——182 字节的警告文本被写在了文件最前面，把本该在偏移 0 的 QM 文件头（16 字节 magic marker）顶到了偏移 182。加一个环境变量抑制这条 warning 即可，**之后每次用 codexctl 重定向导出二进制文件，都建议照下面的方式先加环境变量、导出后再核对一次 magic marker**。

```bash
# 加 PYTHONWARNINGS 抑制会污染 stdout 的 RuntimeWarning，再重定向导出
PYTHONWARNINGS="ignore::RuntimeWarning" \
  .venv/bin/codexctl cat out/remarkable-production-memfault-image-3.27.1.0-rmppm-public \
    /usr/share/remarkable/xochitl/translations/reMarkable_en.qm > reMarkable_en.qm

# 核对 QM 文件头是否在偏移 0（16 字节固定 magic marker）
python3 -c "
data = open('reMarkable_en.qm', 'rb').read()
magic = bytes.fromhex('3cb86418caef9c95cd211cbf60a1bddd')
print('size:', len(data), '| magic at offset 0?', data.startswith(magic))
"
```

magic marker 校验通过后，用 `lconvert` 把二进制 `.qm` 转回可编辑的 `.ts`（Qt Linguist 用的 XML 格式）：

```bash
lconvert -i reMarkable_en.qm -o reMarkable_en.ts

# 如果我用的 Qt 版本 lconvert 不支持 qm→ts 反向转换，
# .qm 的二进制格式是公开文档化的（Qt 源码 qttools/src/linguist/shared/qm.cpp），
# 社区也有现成的 Python 解析脚本，退路是自己写一个几十行的 parser——
# 格式本质就是一串「hash 表 + 字符串块」，比听起来简单
```

实测在 3.27.1.0 固件上解出 **894 条常规消息 + 24 条复数（`numerus="yes"`）消息，分布在 187 个 context 里**，`<translation>` 全部是空的——说明 `reMarkable_en.qm` 就是纯粹的源字符串表，没有夹带任何"英译英"内容，拿它当翻译源头是可靠的。

894 条消息里唯一原文只有 **788 条**（"Edit"「More」「Search」这类按钮文字在不同 context 里重复出现），所以不是逐条改 `.ts` 文件，而是先去重收集全部 `<source>`、翻译这 788 条唯一原文，再写脚本回填。回填脚本要用正则 / XML 精确定位每条 `<message>`，而不是对文件做整体字符串替换，原因有三个：

- `numerus="yes"` 的消息，英文原文是 singular/plural 两种形式，中文的 CLDR 复数规则只有一个 `other` 类别——要把两条 `<numerusform>` 合并成一条，而不是留着空的第二条。
- 少数消息带 `<comment>` 节点（比如筛选栏里同一个「Notebooks」在不同 filter 场景要分别处理），要原样保留在消息块里。
- 部分 `<source>` 内部带真实换行符（多行提示文案，比如断网提示），替换译文时要保留原样的换行位置。

```python
message_re = re.compile(
    r'(<message(?P<numerus> numerus="yes")?>\s*'
    r'(?P<precomment><comment>.*?</comment>\s*)?'
    r'<source>(?P<source>.*?)</source>\s*'
    r'(?P<postcomment><comment>.*?</comment>\s*)?'
    r'<translation[^>]*>.*?</translation>\s*'
    r'</message>)',
    re.S
)

def repl(m):
    zh = zh_map[m.group("source")]        # 788 条唯一原文 -> 中文译文的词典
    zh = xml_escape(zh)                     # & < > ' 按源文件同款转义规则处理
    if m.group("numerus"):
        # zh 只有 1 种复数形式，塞进单条 numerusform
        return build_numerus_block(m, zh)
    return build_plain_block(m, zh)

new_ts, n = message_re.subn(repl, original_ts_text)
# 实测：n == 918（894 + 24），全部命中词典，0 条遗漏
```

回填完成、语言标记从 `en_US` 改成 `zh_CN` 后，用 `lrelease` 编译回二进制，实测 **918/918 finished，0 unfinished**：

```bash
lrelease reMarkable_zh_CN.ts -qm reMarkable_zh_CN.qm
# Updating 'reMarkable_zh_CN.qm'...
#     Generated 918 translation(s) (918 finished and 0 unfinished)
```

> 建议编译完再用前面同款的 Python 三行代码核对一次 `.qm` 输出的 magic marker——流程走通不代表输出一定没问题，多一次校验成本很低。

### 3.2 简体怎么翻，繁体怎么来

**不建议繁体另起一套人工翻译**——工作量翻倍还容易两边不一致。实测流程如下，三步都已在 `zh_CN.ts → zh_TW.ts / zh_HK.ts` 上跑通：

1. 先把简体 `zh_CN.ts` 人工翻译 + 校对做扎实（机翻打底，逐条过一遍，尤其是按钮文字这种短语境最容易翻错）。788 条唯一原文人工过一遍是可行工作量，机翻打底再校对比一上来手翻快很多。
2. 用 **OpenCC** 把简体译文转换成繁体，**关键是要用词汇级配置而不是单纯字形转换**——直接字形转换只会把"网络"转成"網絡"，但台湾习惯用词是"網路"。实测两种配置在同一批词上给出的结果确实不同，词汇级转换是必需的，不是可选优化：

   | 简体原文 | s2twp.json | s2hk.json |
   | -------- | ---------- | --------- |
   | 网络     | 網路       | 網絡      |
   | 软件     | 軟體       | 軟件      |
   | 文件     | 檔案       | 文件      |
   | 设备     | 裝置       | 設備      |
   | 打印机   | 印表機     | 打印機    |
   | 视频     | 影片       | 視頻      |

3. 只对 `.ts` 文件里 `<translation>`／`<numerusform>` 标签内的文本节点跑转换，不要对整个 XML 文件做字符串替换（会破坏 `&lt;`/`&amp;` 这类转义字符），用正则或 XML 解析库精确定位节点边界。

实测环境用的是 OpenCC 的 Python 绑定（Arch 下 `pacman -S opencc` 装的系统库自带，不需要额外 `pip install`），比 8.1 脚本里描述的「CLI + 纯文本往返」更省一步——不用先把 `<translation>` 抽成单独的 txt 文件，直接在原 `.ts` 文本上用正则定位、原地转换：

```python
import re, opencc

converter = opencc.OpenCC("s2twp")   # 港版换成 opencc.OpenCC("s2hk")

numerusform_re = re.compile(r'(<numerusform>)(.*?)(</numerusform>)', re.S)
translation_re = re.compile(r'(<translation(?:[^>]*)>)(.*?)(</translation>)', re.S)

def repl_translation(m):
    open_tag, body, close_tag = m.groups()
    if "<numerusform>" in body:
        # numerus 消息：只转换每个 numerusform 里的文本，标签结构不动
        conv = lambda fm: fm.group(1) + converter.convert(fm.group(2)) + fm.group(3)
        return open_tag + numerusform_re.sub(conv, body) + close_tag
    return open_tag + converter.convert(body) + close_tag   # 普通消息直接转换

zh_tw_ts = translation_re.sub(repl_translation, zh_cn_ts_text)
zh_tw_ts = zh_tw_ts.replace('language="zh_CN"', 'language="zh_TW"')
```

如果没有 Python 环境、只想用命令行工具跑，等价做法是自己写脚本把 `<translation>` 内容逐条抽成纯文本文件、过一遍 CLI、再塞回对应节点：

```bash
opencc -c s2twp.json -i zh_CN_strings.txt -o zh_TW_strings.txt
# zh_CN_strings.txt / zh_TW_strings.txt 是我自己写脚本从 .ts 里
# 抽出来的纯文本（一行一条），转换完再塞回对应的 <translation> 节点
```

实测校验：转换后 XML 依旧合法（`ElementTree.parse` 能正常解析），消息/复数/context 计数与源文件一致（894 / 24 / 187），`%1`/`%n` 占位符、`<b>`/`<br>` 转义标签、多行文本里的换行、以及 `reMarkable`/`Connect`/`Marker`/`Type Folio` 这类产品专名全部原样保留（专名不进 OpenCC 转换，因为它只认中文字符，英文/数字/标点会原样透传，但仍建议转换后抽查几条含专名的消息）；`zh_TW.ts` 和 `zh_HK.ts` 分别编译，`lrelease` 都是 **918/918 finished，0 unfinished**。三份语言包按 `reMarkable_zh_CN.qm` / `reMarkable_zh_TW.qm` / `reMarkable_zh_HK.qm` 命名，和现有的 `reMarkable_en.qm` 保持同一套命名规则，方便 3.5 节验证时直接往 `translations` 目录里塞。

### 3.3 字体：思源黑体 / Noto Sans CJK

> **2026-08-23 字体方案演进（当前定案）**：早期思源/Noto → 统一 LXGW Neo XiHei → 阿里普惠体（为 e-ink 真粗字重、缓解晰黑单字重发虚）→ **现定案：主字体霞鹜新致宋（LXGW Neo ZhiSong Screen Full，宋体书卷气，BMP+扩展A 全覆盖）+ fontconfig 兜底花园明朝 B（HanaMinB，覆盖 CJK 扩展 B U+20000+）**。根因：词典雾凇 41448 大字表含 1.3 万扩展 B 生僻字，而 LXGW/阿里普惠体只覆盖到扩展 A → 候选框+笔记本某些生僻字（如 `hang` 尾部 𠡊）豆腐块。修复后扩展 B 字字形级回退 HanaMinB 显示、不再方块。候选框字体在 `candidatebar.qmd`(cjkFamily)，笔记本经 `fontconfig-cangjie.conf`（zh fallback 链，install.sh 部署到设备 `~/.config/fontconfig/fonts.conf`）。字体覆盖诊断/速查/踩坑（商业美术字体普遍只 43% BMP 覆盖等）。真机验证通过。

- **协议**：SIL Open Font License 1.1，个人使用可以自由嵌入到设备里，唯一的限制是不能把字体本身单独包装出售、且要保留版权声明文件——放进我的扩展包里附一份 `LICENSE.txt` 就够了。
- **体积控制**：原始字体几十 MB，大部分字我用不到。用 `pyftsubset` 只保留常用字（简体参考 GB2312 一级字库、繁体参考 Big5 常用字表）+ ASCII + 标点，实测能压到 2MB 左右，细节见下方。
- **e-ink 观感**：先拿 Regular/Medium 字重上机测试，过细的笔画（Light）在墨水屏上容易发虚，这个只能实机调，模拟器看不出来。
- **注入位置**：社区已在 reMarkable 1（固件 3.27.3.0）上验证是 fontconfig 管理的我字体目录，细节见下方补充；Paper Pro Move 上建议真机复核一次再定案（列入第 04 节集成测试清单）。

#### ✅ 已实测跑通（本机 `rmfw/fonts/`）

字符表不是手抄的——GB2312 一级字库（3755 字）和 Big5 常用字两份表都是用对应的 codec 遍历全部合法编码点直接生成的（`bytes([hi,lo]).decode('gb2312'/'big5')`），比对照资料手抄可靠，GB2312 一级实测正好 3755 字，和公开资料对得上；Big5 常用字实测 5495 字，比常见引用的 5401 字略多，大概率是 Python 内置 big5 codec 表和官方 Big5 标准有几十个字符出入，量级一致、没有深究具体差几个字，不影响拿来做字体裁剪用。用 Noto Sans CJK（思源黑体上游项目，本机系统字体自带，等同字形）裁剪：单个语言 face 原始 16.2MB → 简体裁剪后 **1.8MB**（64682 → 7776 字形），繁体/香港裁剪后各 **约 3.6MB**（Big5 常用字集比 GB2312 一级大，且保留了完整排版特性表，体积更大符合预期）。裁剪结果做了正向 + 反向校验：目标字符表里 3755 个字**全部**在裁剪后的 cmap 里能查到，没选中的繁体专用字（比如"與"）在简体裁剪版里确认被正确剔除，不是"能加载就算过关"这种弱校验。

#### ⚠️ 踩坑记录——`pyftsubset` 的 `--unicodes-file` 不是"字符文件"，是"编码点文件"

这份白皮书原来给的命令用的是 `--unicodes-file=common-3500.txt`，实测直接报 `ValueError: invalid literal for int() with base 16`——`--unicodes-file` 期望文件内容是十六进制编码点（形如 `4e2d,56fd` 或 `U+4E2D`），不是"一个字一行/连写"的原始汉字文本。想直接拿一份"字符文件"（比如上面 GB2312/Big5 生成的那种、内容就是连写的汉字）做裁剪依据，要用 `--text-file`，两个参数容易搞混、字面意思也确实容易理解反，下面命令已经改成实测跑通的版本。

```bash
# --text-file 吃的是"字符文件"（内容是连写的汉字原文，比如 gb2312_level1_3755.txt）
# --unicodes 补充 ASCII / 中英文标点 / 全角符号这些不在汉字表里、但排版必需的字符
pyftsubset NotoSansCJK-Regular.ttc \
  --font-number=2 \
  `# TTC 字体合集要指定第几个 face，2 = Noto Sans CJK SC（简体），3 = TC，4 = HK` \
  --text-file=gb2312_level1_3755.txt \
  --unicodes="U+0020-007E,U+2000-206F,U+3000-303F,U+FF00-FFEF" \
  --layout-features='*' \
  --output-file=SourceHanSans-SC-subset.otf
```

#### ✅ 社区已验证（reMarkable 1 / 固件 3.27.3.0，来源见文末）

字体目录是 `/home/root/.local/share/fonts/`，不是 `/usr/share/fonts/`——根分区当时只剩约 11MB 可用空间，系统分区塞不下字体。装完字体后要跑 `fc-cache -fv` 重建缓存，证实了本节"大概率是 fontconfig 管理"的推测。这是 reMarkable 1 而非 Paper Pro Move 上的实测结果，两者都跑 xochitl + fontconfig，路径大概率通用，但分区剩余空间、具体路径这类细节仍建议上机复核一次，不要直接当成 Paper Pro Move 已验证的结论。

> **M1 里程碑的一个重要澄清（见 05 节）**：第 04 节 Step G 之后的真机截图里，中文字符确实全部正常渲染，客观满足"中文能显示"这条验收标准——但这跟本节设计的字体裁剪/注入流程是两回事，**我自己从来没有把上面这份 subset 字体实际推上 Paper Pro Move 验证过**，中文显示正常，是这台设备本身自带的字体已经覆盖了 CJK 字符集。这套裁剪+注入方案目前是"做好了、本地校验过，但还没用上也没必要用上"的状态，不能理解成已经上机验证。

```bash
ssh root@10.11.99.1 "mkdir -p /home/root/.local/share/fonts"
scp -O SourceHanSansCN-subset.otf root@10.11.99.1:/home/root/.local/share/fonts/
ssh root@10.11.99.1 "chmod 644 /home/root/.local/share/fonts/*.otf && fc-cache -fv"
# 重启 xochitl 才能生效，但短时间内别反复重启——见 6.1 节的重启节流提醒
```

#### 📌 竞品借鉴（rmtool）·部署前用 `fc-match` 预检 CJK 覆盖，否则拒装

> 来源：`pretenderlu/rmtool`（GPL-3.0）`_rmkit_cn.py:2010 has_cjk_font()`、`:124-125` 两条 `fc-*` 命令。**源码研读结论，未在本项目验证。**

rmtool 披露了一个本项目 install.sh 重建时值得抄的安全阀：它做独立"简体中文"语言项前，会**先检查当前 sans-serif 字体是否真的覆盖简中，不覆盖就拒绝部署**（避免装完满屏豆腐块）。判定逻辑很轻量，两条 fontconfig 命令即可，无需上机反编译：

```sh
# 当前 sans-serif 首选字体
fc-match --format='%{file}\n' sans-serif | head -n 1
# 系统里所有声明覆盖 zh-cn 的字体
fc-list --format='%{file}\n' ':lang=zh-cn'
# 前者 ∈ 后者 ⇒ 有 CJK 覆盖，可部署；否则先装字体再继续
```

它同时给出一条**真机事实**（rmtool 自述）：**Move（Chiappa）稳定固件没有内置 CJK 字体**——本项目实测后需要按固件版本修正这句：**3.27.1.0 股票镜像确无 CJK**（`codexctl ls` 离线核实，`/usr/share/fonts/ttf` 只有 ebgaramond+西文 Noto），但 **.166 固件已自带 `NotoSansSC-VariableFont_wght.ttf`**（`/usr/share/fonts/ttf/noto/`，2026-08-15 Move .166 真机 `fc-list`/`find` 确认）。即 reMarkable 在某个固件版本给 Move 补了 Noto Sans SC（简体，**无繁体 TC/HK**）。含义：3.5 节记录的"设备自带字体已覆盖 CJK"其真实来源就是这份 .166 固件自带的 Noto Sans SC（不是此前实验残留，遗留矛盾据此厘清）；界面中文能显示确实靠它兜底。但字体部署对本项目**仍有价值、不能省**：① 早于 Noto Sans SC 引入的固件、以及未来 OTA 若移除它，裸机仍会豆腐块；② 繁体（TC/HK）固件自带的 Noto Sans SC 不覆盖，靠本项目主字体**霞鹜新致宋（简繁英全覆盖）**；③ 观感（本项目用致宋 + HanaMinB 兜底，非 Noto）。故 CJK 预检作为安全阀保留，只是"股票固件必无 CJK"这个论据按固件版本收窄——不再是对所有 Move 固件都成立的硬前提。（当前字体定案见 §3.3，下述历史待办里的 HarmonyOS 已全部由致宋取代。）

**待办（真机核实后并入 install.sh 重建）**：

- [✅ 离线+真机双向核实完毕] `codexctl ls` 3.27.1.0（`rmppm`）股票镜像 `/usr/share/fonts/` 只有 `ttf/ebgaramond` + `ttf/noto`（纯西文，无 CJK face）；但 **2026-08-15 Move .166 真机 `fc-list` 确认固件已自带 `/usr/share/fonts/ttf/noto/NotoSansSC-VariableFont_wght.ttf`**（简体 CJK，无繁体）——**遗留矛盾厘清**：3.6 节说的"设备自带 Noto Sans SC"就是这份 .166 固件字体，不是实验残留。rmtool"Move 稳定固件无 CJK"按版本收窄（3.27 成立、.166 已不成立，见上方正文修正）。CJK 预检因此**不是对所有固件都必需**，但作为安全阀保留（更早固件/繁体覆盖/OTA 可能移除）。真机 `fc-list :lang=zh-cn` 非空（霞鹜新致宋 + 霞鹜系列 + 固件自带 Noto SC），预检在 .166 会**通过**。
- [✅ 已落实进 install.sh，待真机重跑验证] `deploy/install.sh` 已加 `has_cjk_font` 式预检（第 2b 步，字体部署+`fc-cache` 之后、写 drop-in/翻译之前）：`fc-list --format='%{file}\n' ':lang=zh-cn'` 计数为 0 即**带明确报错 `exit 1`**，不留半装状态；设备缺 `fc-list`（BusyBox 环境不确定）时降级为检查字体文件已拷到位 + 警告、不硬中止（避免误伤）。判定较 rmtool 原式（`fc-match sans-serif ∈ fc-list :lang=zh-cn`）放宽为"存在任一覆盖 zh-cn 的字体"，因本项目 UI 走 fontconfig 把 sans-serif 指向霞鹜新致宋（+ HanaMinB 兜底）覆盖 zh-cn。真机重跑 install.sh 验证前不称"已验证"。

### 3.4 简繁双轨在 UI 层面怎么呈现

产出两份独立的 `reMarkable_zh_CN.qm` 和 `reMarkable_zh_TW.qm`，分别对应设置里的「简体中文」「繁體中文」两个语言条目，和现有的 de/en/es/fr 平级并列，而不是做成一个语言里再加切换开关——这样最符合我对"语言设置"的直觉预期。

### 3.5 语言列表怎么"上架"——已上机验证，结论：硬编码

Settings 里那个语言选择列表，此前一直不确定是硬编码枚举还是动态扫描目录，两种都要有对应预案。**已经在真机（Paper Pro Move，固件 3.28.0.164）上验证过，结论是硬编码**：

| 可能机制                          | 判断方法                                                             | 实测结果               |
| --------------------------------- | -------------------------------------------------------------------- | ---------------------- |
| 硬编码枚举（代码里写死 4 个语言） | 把 `zh_CN.qm` 塞进 `translations` 目录后重启，设置列表里**没有**新增 | ✓ 命中——这就是实际情况 |
| 动态扫描目录生成列表              | 塞文件重启后设置列表**自动出现**新语言                               | ✗ 排除                 |

#### ✅ 实测过程与结论（3.28.0.164，2026-08-09）

按 checklist 走了一遍——备份 `translations` 目录（root 分区默认只读，`mount -o remount,rw /` 之后才能写入，这一步此前没人提过，见 2.5 节补充）→ 把 `reMarkable_zh_CN.qm` 复制进 `/usr/share/remarkable/xochitl/translations/` → `systemctl restart xochitl` → 设置 → 通用 → 语言和键盘 → 系统语言，**列表里没有出现"简体中文"**，我在真机上肉眼确认过。`journalctl` 日志里能看到 `rm.localization.language Activated translation: en (setLanguageCode .../xofm/libs/localization/src/languagesettings.cpp:49)`——这条日志本身就是个意外收获：**直接暴露了源文件名和函数名**，比 strings 扫描挖出的类名更进一步，4.2 节的 hook 目标可以直接锁定 `setLanguageCode` 这个具体函数。另外测试了一条旁路思路——改系统 `/etc/locale.conf` 的 `LANG=zh_CN.UTF-8` 后重启，**xochitl 依然激活 en**，说明语言选择不跟随系统 locale，是应用自己独立维护的状态，这条路也排除了。结论：**确认要走 xovi hook 这条路，4.2 节的 hook 开发是必经步骤，没有更省事的旁路**。

#### 📌 离线阶段挖出的类名/方法名全部在新版固件上复核过，依然有效

`xofm::libs::localization::LanguageSettings`、`availableLanguageCodes`、`getLanguageDisplayName`、`getLanguageNameIndex` 等 9 个此前 3.27.1.0 上挖到的关键字符串，重新对 3.28.0.164 的 `xochitl` 二进制（从设备上 `scp` 下来，MD5 核对过和设备上一致，22083336 字节）做同样的 `strings` 扫描，全部还在，命中次数还略多——说明版本升级没有大改这部分代码，此前 radare2 反汇编查不到实现细节那个结论大概率在新版本上依然成立，不需要重新做一遍那一层分析。

#### ⚠️ Step J · 真机切中文后我反馈"基本每个页面都有未汉化内容"——排查发现根因不是版本升级，是 3.1 节的原文提取方法本身就有约 43% 的先天缺口

我给了两个真机截图例子（"Accessibility settings"页面、通用设置里的"Account"/"Turn off"），第一反应是怀疑这些字符串压根没被标记为可翻译——**这个判断错了，已经向我纠正**。用 `lconvert` 把设备真实的 3.28.0.164 `reMarkable_en.qm` 转出 `.ts` 逐条比对 3.27.1.0 版本，先排除了"版本升级引入新字符串"这个假设（**3.28.0.164 的 169 个 context/866 条消息是 3.27.1.0 那 187/915 条的严格子集，没有任何一条是新增的**，逐条比对，不是抽样）。真正原因要从 QML 源码本身找——xochitl 把 QML 文件打包进二进制的 Qt 资源系统按文件用 **zstd** 压缩（一开始按 zlib 假设扫出 0 个，换成 zstd magic byte `28 b5 2f fd` 全局扫描后，成功解压出 **552 个真实 QML 源文件**），在里面搜 `qsTr(...)` 调用，发现 "Readability"/"Handedness"/"Turn off" 这些字符串**明明白白被 `qsTr()` 标记为要翻译**，只是从没被收进 `reMarkable_en.qm`——量化结果：**979 个 QML 里标记可翻译的字符串，有 423 条（约 43%）从未进过 `en.qm`**，3.1 节"拿 `en.qm` 反解全部原文"这条提取路径从一开始就有这个缺口，跟固件版本无关。

**补全与部署（已完成）**：423 条全部翻译成简体中文（术语跟已有翻译对齐：`reMarkable`/`Connect`/`Marker` 保留英文专名，`paper tablet`→"手写平板"等）。Qt 按 `(context, 原文)` 组合匹配翻译，context 就是 QML 文件 basename，但只解压出内容没解压出文件名——用"字符串锚点法"补：一个 QML 文件里如果有别的字符串在 `en.qm` 里已知 context，就借用同一个 context（排除掉 "Left"/"Right"/"Cancel" 这类跨多个 context 复用的通用词，避免假性歧义），最终 423 条里**唯一确定 context 的有 161 条**——包括我给的例子里的大部分（"Accessibility settings"/"Readability"/"Handedness"/"Account"/"Turn off" 全部命中，只有"Toolbar position"那两句因为所在 context 在 3.28 里已经整个不存在、没有锚点可借而跳过，**没锚点的条目宁可跳过也不做无根据的猜测式插入**）。161 条按 context 合并进 `zh_CN.ts`（全部命中已有 context 块）→ `opencc`（`s2tw`/`s2hk`）转出 `zh_TW.ts`/`zh_HK.ts`（各 1079 条 translation，918 原有 + 161 新增）→ `lrelease` 编译三份 `.qm`（**1079/1079 finished，0 unfinished**，magic marker 三份一致）→ 部署替换设备 `translations` 目录（`md5sum` 两边核对一致）→ `systemctl restart xochitl` 健康（`NRestarts=0`）。**这一步全程不碰 `cangjie-langhook.so`/hook 逻辑**，只是替换静态翻译资源文件，风险类别比 Step G/I 的内存级 hook 更低；即使 context 猜错，Qt 的翻译查找是"找不到就走英文原文兜底"，不是报错/崩溃，所以整个过程零崩溃风险。剩下 262 条这一轮没有锚点可用，留作后续——要覆盖到更完整，需要真正解析 Qt 资源系统的二进制目录树拿到 QML 文件名，评估过是一项独立的逆向工程工作量，这次按"性价比优先"没做。

**一个真实的设备连接踩坑记录，附带提一句**：这次部署中途设备一度连不上，排查发现是开发机的 NetworkManager 把 reMarkable 的 USB 网卡自动套用了一个残留自其它设备的旧连接配置（静态配了一个完全无关的网段），跟 reMarkable 期望的 `10.11.99.x` 对不上——`lsusb` 确认设备全程都在 USB 总线上、xochitl 全程健康，纯粹是开发机这边网络配置的问题，手动加一个正确网段的地址（`ip addr add 10.11.99.2/24 dev <网卡名>`）后恢复，跟 2.5 节记录的"陌生 profile 接管新网卡"是同一类坑，这次是第二次真实踩上。

#### 📌 竞品借鉴（rmtool）·"硬编码语言列表"可被 QMLDiff 追加突破，做出真正独立的"简体中文"项

> 来源：`pretenderlu/rmtool`（GPL-3.0）`native-chinese/qmd/ferrari-3.28.0.166.qmd`、`xovi-src/hook.c`、`xovi-src/native-chinese-translator.xovi`。**源码研读结论，未在本项目验证；rmtool 自己也只在 Ferrari（Paper Pro）.166 实机验证，Move 仅离线验证。**

本节结论是"语言列表硬编码、塞 .qm 不会自动上架，必须走 hook"。rmtool 印证了这个前提，并给出一条**比本项目现方案更进一步**的实现——它不占用法语槽位，而是真的往列表里加了一个"简体中文"项，机制分两半：

1. **QMLDiff 往硬编码数组后追加 `zh_CN`**：用 `INSERT STREAM /.~&<hash>&~(["zh_CN"])/` 在语言列表构造处 `.concat(["zh_CN"])`，并用三元表达式把 `zh_CN` 的显示名映射成"简体中文"（`... === "zh_CN" ? "简体中文" : ...`）。即：本项目 4.1 节 `LanguageSettings.availableLanguageCodes` / `getLanguageDisplayName` 那个硬编码枚举，可以在 QML 层被 append，而不必像现在这样在 C hook 里改 `languages` 数组。
2. **一个极小的 xovi 扩展兜住 .qm 加载**：`native-chinese-translator.xovi` override 掉 `QTranslator::load(QLocale…)` / `QTranslator::load(QString…)`，当请求语言是中文时改加载中文 catalog（`hook.c:31 load_chinese_catalog`）。这样系统按 `[General] language=zh_CN` 走原生翻译加载流程即可，配 `_rmkit_cn.py:577 set_language_config()` 写 `xochitl.conf` 的 `[General] language` 键。

**与本项目现状的对照**（置信度：中——机制在 rmtool 源码坐实，但两个项目 hook 分层不同，能否平移需实测）：

|            | 本项目现方案                                                | rmtool 方案                                              |
| ---------- | ----------------------------------------------------------- | -------------------------------------------------------- |
| 语言项来源 | C hook 改 `languages` 数组（拼音文档 01 节语言切换器 hook） | QMLDiff `.concat(["zh_CN"])`                             |
| .qm 加载   | 复用 `setLanguageCode` 主 hook + 放进原生 translations 目录 | override `QTranslator::load` + `[General] language` 配置 |
| 语言正名   | 借用/新增中文项，配合键盘弹层                               | 独立"简体中文"项，不牺牲其它语言                         |

**取舍**：本项目已经用 C hook 把语言项做进键盘弹层且真机验证通过（拼音文档 Phase B），没有"占用法语槽位"的问题，所以这条借鉴**不是要替换现方案**，而是记录一条备选：若将来想把"简体中文"也作为独立**系统语言**项（而非仅键盘输入语言）上架，QMLDiff 追加 + `QTranslator::load` override 是一条已被竞品验证过路线的实现，且更贴合本项目"补丁走 QMLDiff、逻辑走 hook"的既有分层。落地前须在 Move .166 上核实那两个硬编码数组的 QMLDiff 定位 hash 能否命中。

### 3.6 系统界面字体换成汉仪铁线黑——已生效；阅读器下拉框可选字体——已查清楚卡在哪，还没做完

我提出两个独立的字体诉求：① 中文界面（设置菜单等系统 UI 文字）改用 `rmfw/fonts/` 里的汉仪铁线黑，不用设备自带的 Noto Sans SC；② 把这批字体（汉仪铁线黑、汉仪盈宋、京华老宋体、KF Readerly 等）真正加进阅读器"文字与排版"面板的字体下拉框，变成可选项。①已经真机验证生效，②反编译清楚了卡点所在，但受限于要跨进程追踪，这轮没有做完，如实记录在案。

#### ✅ ① 系统界面字体 fallback（已完成，真机验证生效）

反编译 QRC 资源找到 `ark/tokens/Style.qml`——xochitl 全部 UI 文字角色（标题、正文、按钮等）的设计 token 文件，字体全部写死 `fontFamily: "reMarkable Sans"`（少数用 `"reMarkable Serif Small"`）。这两个字体不是系统字体，是 xochitl **自己内嵌在 QRC 资源包里、启动时当"应用私有字体"注册的**（`reMarkableSans-*.ttf`/`reMarkableSerifSmall-*.ttf`，资源树里能找到 14 个字重文件），fontconfig 完全看不到它们，且只覆盖拉丁字母——中文界面能显示，靠的是 Qt 自动 fallback：渲染到 reMarkable Sans 没有的字形时，fontconfig 找"谁能覆盖这个字"，选中了设备自带的 Noto Sans SC。**改法不碰任何二进制/hook，纯 fontconfig 配置**：新增 `/etc/fonts/conf.d/99-cangjie-cjk-fallback.conf`，给 `reMarkable Sans`/`reMarkable Serif Small` 追加一条强绑定 fallback 规则指到已部署的 `HYTieXianHei`。`fc-match -s 'reMarkable Sans:lang=zh-cn'` 验证排序：改前 `Noto Sans SC` 排第一，改后 `HYTieXianHei-45J.ttf` 排第一、`Noto Sans SC` 降到第二。`fc-cache -fv` + `systemctl restart xochitl` 后真机确认中文界面文字笔画明显变粗变硬朗，符合汉仪铁线黑的观感。

> 源码：`rmfw/fonts/99-cangjie-cjk-fallback.conf`（完整规则文件，已同步部署到设备 `/etc/fonts/conf.d/`）

#### 📌 字体全量部署到设备（已完成）

`rmfw/fonts/` 下 7 个 `.ttf` 文件（不含两份字表 txt）全部 `scp` 到 `/home/root/.local/share/fonts/`（社区已验证的字体目录，`/home` 分区 45.7GB 可用，不是根分区那 83.6MB），`fc-cache -fv` 后 `fc-list` 确认全部 8 个字体（含此前已部署的思源黑体子集）都能被正确识别到中文字族名（如"汉仪铁线黑"“京華老宋體”“汉仪盈宋”）。**持久性提醒**：`/var/cache` 在这台设备上是 tmpfs 覆盖层（`upperdir=/var/volatile/cache`），字体文件本身在 `/home` 上持久，但 fontconfig 缓存索引不是——真重启设备（不是重启 xochitl 进程）后缓存会丢，需要重新跑一次 `fc-cache -fv`。

#### ⚠️ ② 阅读器下拉框可选字体（未完成，卡点已查清楚）

`FormatFont.qml` 的字体下拉框是写死 4 项的 `ListModel`，选中后调用 `root.epub.setFontName(key)`，`key` 就是下拉项 `family` 字符串原文——理论上只要 `setFontName` 不做白名单校验，任何 fontconfig 认识的字体名都能用。反编译 `EpubProperties::qt_static_metacall`（跟第 04 节 Step F 反编译 `LanguageSettings` 同一手法）没找到 `setFontName` 的踪迹——跟第 09 节拼音输入法白皮书 Step L→M 的 `insertText` 是同一类坑：QML 编译器对类型已知的调用点会直接生成 C++ 实现函数调用，绕开 `qt_metacall`。改用真机运行时枚举方法表（跟拼音文档 Step V-2 同一手法）：`methodCount=17`，`setFontName` 在全局 index 12、参数个数 1，精确命中。真机测试 `QMetaMethod::invoke` 调用 `setFontName("HYTieXianHei")`：**确认没有白名单校验**——第一次因为在文档指针还没就位时调用（反编译发现 `setFontName` 内部有 `*(long*)(this+0x40)!=0` 的守卫，文档未挂载时整个函数体直接短路跳过），排查后改成先检查文档指针非空再调用，成功后 **读回 `fontName` 属性确认变成了 `"HYTieXianHei"`**，不是白名单拒绝导致的空值。

**真正的卡点**：属性值改对了，但画面不会自动重绘。日志显示 reMarkable 把 EPUB 转成 PDF 渲染（`starting /usr/bin/xochitl_pdf_renderer to handle requests`），**真正画字的是一个独立进程 `xochitl_pdf_renderer`**，我的 hook 目前只碰得到 `xochitl` 主进程；我手动改对齐方式那次能看到字体变化，是因为那个操作走了完整原生流程、恰好触发了一次真正的 PDF 重新生成，不是我的属性写入本身生效——这两者被验证是分开的：连续两次调用 `setFontName`（模拟"再触发一次冲刷渲染"）没有让字体在画面上显现，排除了"只是差一次 kick"的简单解释。要继续，需要反编译 `xochitl_pdf_renderer` 这个全新的二进制、搞清楚 PDF 缓存失效/重新渲染的触发机制，工作量相当于重新走一遍第 04 节 Step F 那种从头分析——我已确认这轮先到此为止，结论和反编译细节保留在代码注释里，供下次继续。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2295-2297`（`EF_STATICMETACALL_VADDR`/`EF_SETTEXTFORMAT_VADDR` 两个反编译确认的地址）、`:2401-2404`（`cj_step_ef_document_attached` 文档指针守卫）、`:2429-2470`（`cj_step_ef_test_invoke_setfontname`，测试调用逻辑已注释关闭，只保留只读锚点+方法枚举）、`:2572-2630`（两个 hook 安装函数）

#### 📌 竞品借鉴（rmkit-cn）·"自建面板"范式——绕开改原生 UI 内部这条难路

> 来源：`boangs/rmkit`（GPL-3.0）`qmd-src/advanced_panel.qmd`、`upload-server-go/internal/server/applier.go`。**源码研读结论，未在本项目验证。**

先划清边界，避免误解：rmkit-cn 的字体管理管的是**系统字体**（fontconfig 的 `~/.local/share/fonts/` active 目录），**不是**本节 ② 卡住的"文档正文字体下拉框"——后者由独立进程 `xochitl_pdf_renderer` 用具体 `fontName` 渲染，rmkit-cn 也没碰。所以它**不能直接解②的卡点**。本项目 P0 注入路线其实已经绕过了②（正文字体走 `.content` 的 `fontName` 字段设定，见《功能路线图白皮书》08 节），②这条"改原生阅读器下拉框"的路本身价值已降低。

真正可借鉴的是**范式**：与其去 hook/改原生 Settings 或阅读器 UI 的内部结构（就像②里追 `setFontName` 一路追进跨进程渲染那样越挖越深），rmkit-cn 选择**在侧边栏加一个自己的按钮、弹出一个完全自建的全屏覆盖面板**（`advanced_panel.qmd`：`Sidebar.qml` 的 `filterColumn` 后加 `SidebarItem`，`Navigator.qml` 的 root 加一个自建 `Rectangle` 面板，字体/壁纸用单选卡片），面板里的逻辑全是自己的、不依赖原生控件的内部实现。**对本项目的意义**：将来任何"要给用户一个可视化设置入口"的需求（字体切换、输入法开关、微信读书登录/书架…），自建面板比 hook 进原生 UI 内部更可控、更抗固件升级——原生 UI 结构一改，改内部的 hook 就碎，自建面板只依赖少数注入锚点。

配套的字体 apply 机制（`applier.go`）可与本项目字体部署对照：**归档现有 active 字体 → 目标字体硬链接进 active 目录（失败回退拷贝）→ `fc-cache` → 重启 xochitl**；本项目 install.sh 已在做类似的 `scp` + `fc-cache`（3.3/3.6 节），差别是 rmkit-cn 支持运行时在设备上切换激活哪个字体。置信度：中（机制坐实；范式适用性高，但②的具体卡点它并不解决，需诚实区分）。

## 04｜xovi Hook 与原生 Settings 集成

### 4.1 先摸清楚要 hook 什么

#### ⚠️ 踩坑记录——.dynsym 这条路在真实固件上是走不通的

本来设想的是用 `nm -D`/`readelf --dyn-syms` 在动态符号表里搜关键词，实测在 3.27.1.0（`rmppm`）固件的 `xochitl.bin` 上完全落空——`file` 一下发现这个二进制是 **stripped**，`.dynsym` 表里 3507 条全部是外部依赖符号（`_ZN...@Qt_6`、`ASN1_STRING_*@OPENSSL` 之类），没有一条是 xochitl 自己代码里的类/函数，不管换什么关键词都不可能搜到。改用 `strings` 扫描才真正有收获——Qt 的反射机制会把类名/属性名以字符串形式硬编码进二进制，这部分不受 strip 影响。另外 `aarch64-linux-gnu-nm` 这类交叉版 binutils 装不上也不影响这一步：本机原生 `nm`/`readelf`/`c++filt` 就能完整读 ARM64 二进制的符号表，交叉版工具只有真要交叉**编译**时才需要。

#### ✅ 上机之后发现一条比反汇编更省事的路子——直接看运行日志

xochitl 的日志里很多消息自带"源文件路径:行号"（比如 `(setLanguageCode /home/runner/work/xochitl/xochitl/src/xofm/libs/localization/src/languagesettings.cpp:49)`），这是它自己的日志宏在编译时把 `__FILE__`/`__LINE__`/函数名一起打进去的，stripped 二进制不影响这个（这些是运行时生成的日志字符串，不是符号表）。`journalctl -u xochitl -f` 实时看、同时在设备上触发对应操作（比如切语言、开键盘），比对着几百条 `strings` 命中一个个反查调用关系快得多——3.5 节验证语言列表机制时就是靠这条日志直接锁定了 `setLanguageCode` 这个具体函数，比这一节原本计划的"strings 命中 → Ghidra 反查调用者"路线更直接。缺点是只能覆盖"确实会打日志"的路径，日志里没提到的函数（比如 `availableLanguageCodes` 这种查询型函数，可能压根不打日志）还是得靠下面的 strings + 反汇编。另外这条日志本身也暴露了 xochitl 是用 GitHub Actions CI 构建的（路径前缀 `/home/runner/work/...` 是 GitHub-hosted runner 的标准工作目录），源码仓库路径是 `xochitl/xochitl`，不代表仓库公开，只是 CI 构建路径带出来的信息。

```bash
# 把 xochitl 二进制拉到本地（比较大，命令行工具较慢，建议只跑一次并缓存结果）
codexctl cat out/remarkable-production-memfault-image-3.27.1.0-rmppm-public /usr/bin/xochitl > xochitl.bin

# 先确认是不是 stripped，决定走 strings 还是符号表这条路
file xochitl.bin

# 实测有效的路径：strings 扫描 + 关键词过滤，命中 315 条（3.27.1.0 rmppm 实测），细节见上方踩坑记录 + 3.5 节
strings xochitl.bin | grep -iE "language|keyboard|settings|input|typedtext|paragraphstyle|crdtstr|glyphrange|richtext" | sort -u

# .dynsym 只用来确认外部依赖库，理由同上方踩坑记录
nm -D xochitl.bin | c++filt | less
```

字符串命中只能给我线索（类名、方法名、日志文案），不能直接告诉我"这就是要 hook 的函数、它的地址在哪"——真正确定 hook 点通常还要配合反汇编工具，从字符串引用处反查调用它的函数，这一步没有捷径，是这个方案里最耗时间的部分之一，要有心理预期。

#### ✅ 已实测走通这条"反查"路径，定位到 `setLanguageCode` 的真实函数入口

先用 `.dynsym` 确认了这个函数不是导出符号（两个版本分别只有 401/367 条 defined 动态符号，全是 libc/Qt 跨模块符号，没有一条是 xochitl 自己的类方法——再次印证 4.1 节开头那条踩坑记录），说明只能走"地址级反查"这条路，不能指望 `dlsym` 按名字解析。具体做法：① 用 `r2 -q -c 'iz~setLanguageCode'` 找到字符串 `"setLanguageCode"` 和 `"languagesettings.cpp"` 在 `.rodata` 里的虚拟地址；② **r2 的自动 `aaa` 全量分析加 `axt` 交叉引用查询在这个 22MB 的 stripped 二进制上没找到结果**（大概率是递归反汇编覆盖不到这段代码路径）；③ 改用 `aarch64-linux-gnu-objdump -d` 对整个 `.text` 段做线性反汇编，手动 grep `adrp` 指令目标页 + 紧跟着的 `add` 低 12 位立即数，两者拼起来定位到具体地址——这一步的关键教训是**同一个 `__FILE__` 字符串会被同一编译单元里所有日志调用共享**，第一次搜索页地址命中了上百处引用，光看文件路径字符串分不清究竟是哪个函数在用，直到同时核对函数名字符串（`"setLanguageCode"`）、日志分类字符串（`"rm.localization.language"`）和行号立即数（`mov w3, #0x31`，即 49）三者在同一处调用点同时出现，才唯一确定了调用位置；④ 从该调用点往回找最近的函数序言（`paciasp` + `stp x29, x30, [sp, #-336]!`，ARMv8.3 指针认证 + 336 字节栈帧），确认没有跨越其他函数的 `ret`，得到函数真实入口地址。

对两个固件版本各走一遍，结果：**3.28.0.164 里 `setLanguageCode` 入口在 `0x913c70`，3.27.1.0 里在 `0xa0ff50`，两者相差 `0xfc2e0`（约 1MB）**。有意思的是，两处函数体的开头几十条指令（栈帧大小、寄存器分配、调用 `QCoreApplication::removeTranslator`/`installTranslator`、日志调用序列）**逐字节完全一致**——说明这个函数本身的源码大概率没有跨版本改动，纯粹是因为二进制里其他地方增删代码，导致链接后的绝对地址整体偏移了。这组数据直接支撑了 4.3 节关于固件升级兼容性的结论，不再是纯推测。

#### ✅ 定位 `availableLanguageCodes`（语言列表来源）时踩了新坑，换了一套方法——从真机存活对象反查 vtable，不用再猜内部数据布局

`availableLanguageCodes`、`languageCodeChanged`、`setRetailLanguageCode` 这三个字符串虽然都在二进制里（`strings` 能命中），但对它们重复上面那套"找 `adrp+add` 引用"的方法**全部零命中**——即便把条件放宽到"这个字符串所在的整页有没有被任何 `add` 指令引用过"也证实页面本身是活跃的（46/29 次引用，只是不落在这三个字符串的具体偏移上）。结论：这三个字符串是 **Qt 6 moc 生成的 compile-time metaobject 字符串表数据**，只被数据表按下标索引，没有任何一条机器指令会直接 `adrp+add` 加载它们——"字符串 xref"这条路对这类目标彻底不适用，上面 `setLanguageCode` 能走通纯粹是因为它恰好出现在一条日志语句里，运气成分比想象中大。

换了个思路：`LanguageSettings` 继承自 `QObject`，`qt_metacall` 是 `QObject` 的虚函数——它的地址就在这个类的 vtable 里，不用反汇编就能拿到。已经部署在真机上的验证性 hook（4.3 节）本来就能拿到一个真实存活的 `LanguageSettings` 实例指针，加两行纯只读的转储代码：读 `*(void**)this` 拿 vtable 指针，打印前 20 个槽位。真机结果：slot[0]/slot[1] 数值非常接近（`0x90fab0`/`0x90fcb0`），符合 `metaObject()`/`qt_metacast()` 这两个 moc 生成的"近乎双胞胎"函数通常被编译器放在相邻位置的经验；**反汇编 slot[2]（`0x9178f0`）第一条指令就是 `bl QObject::qt_metacall`**——跟 moc 生成代码"先调用基类 `qt_metacall` 做链式分派"的标准模式完全吻合，**确认 slot[2] 就是 `LanguageSettings::qt_metacall`**，不是靠猜的，是反汇编内容直接证实的。顺着往下追，属性写入分支（`QMetaObject::WriteProperty`）里有一条 **直接尾调用 `b 0x913c70`（也就是 `setLanguageCode` 本身）**——这进一步印证了 `languageCode` 是一个标准的 `Q_PROPERTY`（`READ languageCode WRITE setLanguageCode NOTIFY languageCodeChanged`），跟字符串表里那几个名字对得上。`availableLanguageCodes` 具体是 `Q_INVOKABLE` 方法还是只读属性、精确的调用分支还没有完全追出来——手工读反汇编到这一步已经明显吃力（没装 Ghidra，纯读 `objdump` 输出），这是下一步要继续的地方。

**顺带薅到一个意外收获**：同一次转储把 `setLanguageCode` 的 `QString` 参数原始内存也打印了出来（内容是 `"en"`），前 24 字节清清楚楚：`d` 指针（8 字节）+ `ptr` 指针（8 字节，正好等于 `d` 指针 +16）+ `size`（8 字节，值为 2——跟 `"en"` 两个字符精确对应）。这跟 Qt 6 公开源码里 `QArrayDataPointer<char16_t>` 的结构定义完全一致，但这次不是"查文档假设它是这样"，是**从真机真实运行的对象里直接读出来验证过的**——为后续（如果要尝试构造一个 `"zh_CN"` 的 `QString` 传给 `setLanguageCode`）打好了地基，不用再猜内存布局对不对。

#### ✅ 装了 Ghidra（12.1.2）之后彻底挖到底：`availableLanguageCodes` 根本不是方法，是 `Q_PROPERTY`

（本机 sdkman 单独装 JDK 21，没动全局默认版本。这就是之前用"字符串引用"死活找不到它的真正原因。）

反编译 `qt_metacall`（`0x9178f0`）确认它只处理 4 个 `InvokeMetaMethod` 方法（id 0-3）和几个 `Property` 相关调用类型（`ReadProperty`/`WriteProperty`/`ResetProperty` 等），但光看控制流猜不出 id 对应哪个方法名——于是直接从代码里引用的 `&PTR_staticMetaObject_01255730` 顺藤摸瓜，把这个 `QMetaObject` 结构体的 `data`（方法/属性描述表）和 `stringdata`（字符串表）两块原始内存都读了出来，按 Qt 6 moc 的 `QMetaObjectPrivate` revision 13 标准格式手动解码（表头 14 个 int：revision/className/classInfo.../methodCount=4/methodData=30/**propertyCount=2**/**propertyData=20**/.../signalCount=1）。字符串表解码结果（`(offset,length)` 数组紧跟 null-分隔的字符块，逐条验证 offset 累加关系全部吻合，不是巧合）：

```
0  "xofm::libs::localization::LanguageSettings"  // 类名
1  "QML.Element"        2  "auto"
3  "QML.Creatable"       4  "false"
5  "QML.UncreatableReason"
6  "LanguageSettings needs to be instantiated from C++"
7  "languageCodeChanged"          // 唯一的 signal，id=0
8  ""
9  "nativeLanguageName"           // Q_INVOKABLE 方法，id=1，1 个参数
10 "code"                         // 参数名
11 "languageName"                 // Q_INVOKABLE 方法，id=2，1 个参数
12 "getLanguageDisplayName"       // Q_INVOKABLE 方法，id=3，1 个参数
13 "languageCode"                 // Q_PROPERTY，本地属性索引 1，QString，NOTIFY=languageCodeChanged
14 "availableLanguageCodes"       // Q_PROPERTY，本地属性索引 0，QStringList，无 NOTIFY
```

属性表（`propertyData` 起始的 10 个 int，2 条 × 5 int/条）解码：`property[0] = {name=14(availableLanguageCodes), notify=-1(无)}`，`property[1] = {name=13(languageCode), notify=0(即 languageCodeChanged)}`。再对照之前反编译出的属性 `Read` 分支代码——属性索引 0（也就是 `availableLanguageCodes`）的读取逻辑是从 **`this+0x10` 处直接拷贝一块 24 字节的结构**（跟已经实测确认的 `QString`/`QStringList` 共用的 `{Data* d; T* ptr; qsizetype size;}` 三字长布局吻合）——**说明 `availableLanguageCodes` 背后就是一个普通的 `QStringList` 成员变量，读取时只是原样拷贝一份返回，没有任何"动态计算候选列表"的逻辑**，跟 3.5 节"硬编码枚举"的结论完全对上，而且比之前更进一步：**现在知道硬编码的具体存储位置是对象偏移 `+0x10` 这个 `QStringList` 字段**，不是某个函数的返回值逻辑。同理 `languageCode`（属性索引 1）自己的 `QString` 成员在 `+0x28`，跟 `setLanguageCode` 反汇编里 `add x21,x0,#0x28` 这行完全对得上，交叉验证成立。

**这对后续"让简体中文出现"这个目标的意义**：要修改的不是某个"生成列表"的函数逻辑，而是这个 `+0x10` 处 `QStringList` 字段的内容（要么在它被填充的时刻——大概率是构造函数或某个 init 阶段——插进去 `"zh_CN"`，要么直接 hook 属性读取分支让它在返回前追加一项）；同时，`getLanguageDisplayName`/`languageName`/`nativeLanguageName` 这三个方法很可能是 QML 侧遍历 `availableLanguageCodes` 时用来把每个 code 转成人类可读文案的辅助函数（从反编译内容看，三者都是"传入一个语言 code，返回一个显示名字符串"，内部分别用了不同的实现——`getLanguageDisplayName` 甚至有针对 `"en"`/`"es"` 硬编码的特判分支），如果只加 code 不管这几个函数，新条目大概率会显示成空白或者兜底文案，不会自动有合理的中文名字。这几点合在一起，已经能支撑起 Step G（真正尝试让语言列表变化）的具体设计，但那是行为改变类操作，风险类别更高，需要单独确认再动手。

#### ✅ Step H · 反编译 `setLanguageCode`（`0x913c70`）本体，回答"传一个未知语言代码进去会怎样"

（不需要新的设备操作，答案早就在 Step F 那次 Ghidra 批量反编译的产出里，只是当时没读这一段。）

完整控制流：① **快速路径**——如果新代码跟当前生效的 `languageCode`（`this+0x28`）内容相同，直接 `return true`，什么都不做；② **否则**，遍历两个候选翻译目录（其中一个真机核实就是 `/usr/share/remarkable/xochitl/translations`，另一个是相对路径，这台设备上不存在），对每个存在的目录：用 `QLocale::QLocale(newCode)` 直接从传入字符串构造一个 `QLocale`——**`QLocale` 的字符串构造函数是"宽容型"的，不存在"非法字符串"这个失败态，最坏情况落到默认/C locale，不会抛异常也不会崩**；再用 `QTranslator::load(locale, "reMarkable", "_", 目录)` 按 `reMarkable_<locale>.qm` 命名规则尝试加载编译好的翻译文件。**两个目录都没找到匹配文件时**，函数记一条 debug 日志后**直接 `return false`，`languageCode` 完全不修改，不触发 `languageCodeChanged` 信号**——是一次完全安全的静默失败，不是崩溃路径；只有**找到且 `QTranslator::load` + `QCoreApplication::installTranslator` 都成功**时，才会真正更新 `languageCode` 并发出变更信号。结论（置信度：高，直接来自反编译出的真实控制流，不是推测）：**`setLanguageCode` 对未知语言代码不存在崩溃风险**，它不做白名单校验然后拒绝，失败模式完全取决于"设备上有没有一个名字匹配的 `.qm` 文件"，找不到就安静地什么也不做。

**意外发现（读时不确定，用一条纯只读 `ssh find` 核实过）**：设备上**已经真实存在** `/usr/share/remarkable/xochitl/translations/reMarkable_zh_CN.qm`（85840 字节）——不是新放的，正是 3.5 节那次"塞 .qm 验证语言列表机制"实验时复制上去、之后从未清理过的那个文件（同目录下 `de`/`en`/`es`/`fr` 四个官方文件时间戳都是固件安装时的 `Jul 2`，只有 `zh_CN` 是 3.5 节实测当天的 `Aug 9`，时间线对得上）。这份文件不是占位符——3.5 节的制作流程本身就已经验证过它是从真实 894 条常规消息 + 24 条复数消息编译出的合法 `.qm`（`lrelease` 918/918 finished，0 unfinished）。也就是说：**现在点击菜单里新出现的"简体中文"，大概率不只是安全的空操作，而是真的会命中这份已经验证过的翻译文件，让界面切换成中文**——3.5 节当初"塞了 .qm 没生效"，根因不是文件本身有问题，是那时候语言列表里压根没有 `"zh_CN"` 这个选项、UI 从来没机会调用 `setLanguageCode("zh_CN")`；Step G 把选项加上之后，这条路径现在才第一次真正打通。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c`（Step H，`FUN_009174D0_VADDR` 定义在 295 行，安装函数 `cj_install_availablelanguagecodes_hook` 在 504-514 行）

#### ✅ Step I · 追加 `zh_TW`/`zh_HK` 后暴露的新问题——两项繁体中文重名，根因还是同一个 `FUN_009174d0`

我实测确认"简体中文"生效后，同一套机制又追加了 `zh_TW`/`zh_HK` 两项，结果台湾/香港两个繁体条目**显示成一模一样的文字**。反编译 `FUN_009174d0` 里 `InvokeMetaMethod`（`call_type==0`）分支找到根因：`languageName`（本地方法 id=2）走 `QLocale(code).language()` → `QLocale::languageToString()`，只看 `QLocale::Language` 这一个枚举值，不区分地区；`nativeLanguageName`（id=1）和 `getLanguageDisplayName`（id=3）都走同一个共享 helper `FUN_00c3ec80`（原始代码里这个 helper 已经对 `"es"`→`"es_US"`、`"en"`→硬编码字面量 `"English"` 做过特判，说明作者自己也遇到过"通用 code 不够用"的情况），输出**只跟 `QLocale` 解析出的 script（简体 Hans／繁体 Hant）有关，不含 territory（地区）**——`"zh_TW"` 和 `"zh_HK"` 在 Qt 里都被解析成同一个 `Hant` script，本地名称因此完全相同；`"zh_CN"` 因为 script 是 `Hans`，正好是另一个字符串，之前只加一项时看不出这个限制。这不是 hook 引入的 bug，是 Qt/CLDR"native name 只到 script 粒度"的设计限制，原始 app 代码里从来没有过要区分同 script 不同地区的场景。

### 4.2 开发 xovi extension 的大致流程

#### ✅ 已实测跑通"生成胶水代码 + 编译 + 符号核对"这一段

（此前这里完全没提生成胶水代码这一步。）

xovi 的扩展开发不是直接写 C 文件编译那么简单——第一步要写一份 `.xovi` 描述文件声明 `import`（要用哪些外部符号的未 hook 原版）/ `export`（导出给别的扩展用）/ `override`（要 hook 哪个函数），例：`override strdup` + `import strdup`；然后跑 `xovigen.py` 生成 `xovi.c`/`xovi.h` 胶水代码（这一步是纯 Python，不需要交叉编译器，离线就能跑，实测直接成功）；自己的 hook 函数命名必须遵守约定——`override$函数名`（比如 `override$strdup`），导入的原函数用 `$函数名` 引用。实测拿仓库自带的 `example-extension`（hook `strdup`）走了一遍全流程，**先用本机原生 `gcc` 做语法验证，装好交叉编译器后又用真正的 `aarch64-linux-gnu-gcc` 重新编译确认了一遍**：`xovigen.py` 生成的胶水代码 + `main.c` 一起编译成目标架构的 `.so`，`aarch64-linux-gnu-nm` 核实符号表里正确出现了 `override$strdup` 和自定义导出函数 `isDuck`，产物确认是合法的 aarch64 动态库——说明这套"描述文件 → 生成胶水代码 → 交叉编译"链路从头到尾都是通的，细节见 2.4 节。

#### ⚠️ 踩坑记录——GCC 16（本机默认版本）编译 xovigen 生成的代码会直接报错

`xovigen.py` 生成的导入符号宏用的是 `(返回类型 (*)())` 这种没写参数列表的函数指针类型（C 语言里传统写法，K&R 时代"空括号=参数不作规定"），但 GCC 16 默认的 C 语言标准（趋向 C23 语义）下空括号被视为"确实零参数"，实际调用时传了参数就直接编译报错 `too many arguments to function`。解法是编译时显式加 `-std=gnu17`（或其他 C23 之前的标准），实测加上就能正常编译过——这不是 xovi 项目本身的 bug，是"新版编译器默认标准变严格"和"项目代码写于旧标准年代"这种很常见的兼容性问题，但如果不知道原因，第一次遇到很容易卡住，值得记一笔。

1. 写好 `.xovi` 描述文件、跑 `xovigen.py` 生成胶水代码后，在 `init`（或者 hook 函数本体）里写实际逻辑——比如往语言列表模型里插一条新数据。
2. 对目标函数插入 trampoline——简单理解就是把原函数开头的几条指令替换成"跳到我自己的代码"，我的代码执行完该做的事，再跳回原函数继续执行（或者完全接管，视 hook 点而定）；这一步是 xovi 框架自动做的，写扩展的人不需要手写汇编，只要遵守 `override$` 命名约定。
3. 涉及到 QRC 资源变动（比如要往内嵌资源里加东西）的部分，用 `qt-resource-rebuilder` 处理。
4. 用 `rm-appload` 打包成 AppLoad 能识别加载的扩展包。
5. 通过 SSH 或 AppLoad 的推送机制部署到设备，用 `journalctl -u xochitl -f`（或 xovi 自己的日志输出）实时看有没有报错、crash。

```bash
# example.xovi —— 描述文件（仓库自带的例子，hook strdup）
# version 0.1.0
# import strdup
# export isDuck
# override strdup

python3 util/xovigen.py -o xovi.c -H xovi.h example.xovi   # 生成胶水代码，纯 Python，离线可跑

# -std=gnu17 是踩坑记录里那个 GCC 16 兼容性问题的解法，缺了这个 flag 编译会报错
$CC -std=gnu17 -shared -fPIC main.c xovi.c -o xovi-example.so

# 核对符号表：override$strdup（hook 函数）和 isDuck（自定义导出）应该都能查到
nm -D xovi-example.so | grep -E "override\$|isDuck"
```

### 4.3 固件升级兼容性与"自愈"设计

#### ⚠️ 不是"能不能升级"，是"升级后 hook 大概率要重新定位"——这条结论现在有实测数据支撑，不是推测

4.1 节实测确认 `setLanguageCode` 不在动态符号表里，xovi 那套按符号名 `dlsym` 插入的 hook 机制（`override$函数名`）对它不适用，只能靠"这份具体编译产物里的固定偏移地址"介入。对比 3.27.1.0 和 3.28.0.164 两个版本实测：函数入口地址从 `0xa0ff50` 变成了 `0x913c70`，差了约 1MB——**哪怕函数本身的代码逐字节没变，只要二进制里别的地方增删了内容，链接后的绝对地址就会跟着挪位置**。结论：**默认关闭自动升级，把已验证固件版本锁定为基线；确需升级时，先把新固件的 `xochitl` 二进制拉下来，重复一遍 4.1 节的定位流程比对偏移量是否变化，变了就先重新编译扩展、确认没问题再升级，不要边升级边指望旧 hook 还能用**。置信度：**高**（基于两个真实固件版本的直接反汇编对比，不是理论推测）。

xovi hook 的本质是"精确改写某个版本二进制里某个偏移的指令"，reMarkable 每次 OTA 升级都可能让这个偏移变化，导致 hook 失效甚至让 xochitl 崩溃。建议在 extension 的 `init` 阶段加一层**版本探测 + 优雅降级**——这一步是必须的，不是锦上添花，因为一旦偏移错位，hook 跳转的目标地址落在错误指令中间，大概率直接让 xochitl crash-loop（联动 6.1 节的重启节流限制）：

```c
// 伪代码示意
version = read_file("/etc/version");
if (version not in KNOWN_COMPATIBLE_VERSIONS) {
    log("固件版本未验证过，跳过 UI 级 hook，只加载语言包和独立的输入法 App");
    return SAFE_MODE;   // 不碰有风险的 hook 点，避免直接把 xochitl 弄崩
}
apply_hooks();
```

4.1 节的实测还带出一个更值得考虑的工程优化：既然两个版本里函数体本身**逐字节完全一致**（只是绝对地址变了），比起"每个已知版本号硬编码一个偏移量"这种笨办法，更稳健的做法是在 `init` 阶段做**字节特征码扫描**（signature/AOB scan，游戏外挂/反作弊领域的常见技术）——把函数开头那段稳定不变的机器码（比如 `paciasp; stp x29,x30,[sp,#-336]!; mov x29,sp; ...` 这几十字节）当作特征串，在运行时的 `.text` 段里现场搜索匹配位置，而不是依赖预先记录的固定偏移。这样只要编译器没有重新生成这个函数（哪怕整个二进制因为其他改动而重新链接、地址整体偏移），hook 依然能自动找到正确位置，不需要每次升级都人工重新定位——代价是扫描逻辑本身要写对、要处理"特征码在新版本里也变了"的兜底（这时才真正需要走一遍 4.1 节的人工反查流程）。

#### ✅ 已经把这条思路写成真代码并验证过（`xovi-extensions/cangjie-langhook/`）

读了 `asivery/xovi`（LGPL v3）源码确认——它的核心 hook 函数 `pivotSymbol()` 只在开头一行用 `dlsym()` 把符号名解析成地址，之后的 trampoline 构造、`mprotect`、写入逻辑全部只依赖解析出来的 `void*`，跟"地址是怎么来的"无关；`dynamiclinker.c` 也证实扩展只要能被 `dlopen()` 正常加载，不依赖 xovi 专属符号也不影响加载。据此写了一个不经过 `.xovi override` 声明的"裸"扩展：`__attribute__((constructor))` 里解析 `/proc/self/maps` 找到 xochitl 自身的可执行映射，在其中用手写的朴素字符串搜索扫描 48 字节特征码并**强制要求唯一命中**（0 次或 >1 次都直接放弃、不打 patch），命中后用照抄 `pivotSymbol` 编码方式（去掉 `dlsym` 那一步）写的 `cj_build_far_jump()` 生成远跳转指令，先把原函数被覆盖的字节另存到一段可执行的"调用桩"里（保证原始逻辑还能被完整调用），再覆盖目标——hook 处理函数目前只做验证性实现（记一条日志、原样调用原逻辑，不改变行为）。9 个宿主机单元测试全部通过，交叉编译产出合法 `aarch64` `.so`；**最有说服力的是把这套扫描逻辑直接跑在两个真实下载的 xochitl 二进制的 `.text` 段字节上**（不是合成测试数据）——3.28.0.164 和 3.27.1.0 都**唯一命中一次**，命中地址跟本节前面人工反汇编定位的结果分毫不差。

#### ✅ 已经在真机（3.28.0.164）上部署验证过，一次成功

不需要装完整的 xovi 框架——扩展自己在构造函数里做扫描和 patch，只要能被 `LD_PRELOAD` 进 xochitl 进程就行，所以直接用 systemd drop-in（`/etc/systemd/system/xochitl.service.d/cangjie-langhook.conf` 加一行 `Environment=LD_PRELOAD=...`，不改原始 `xochitl.service`，方便随时撤销）注入，没有另外安装 xovi.so 本身。重启后 `journalctl` 证据链完整：① 命中地址 **`0x913c70`，跟纯静态分析预测分毫不差**（这台设备主程序加载地址没有被 ASLR 打乱，运行时地址和链接时 vaddr 一致）；② xochitl 启动流程里自己会调用一次 `setLanguageCode` 激活默认语言，日志显示我的 handler 先触发（打出 `this`/参数指针），紧接着**原函数自己的日志也正常打出来了**（`Activated translation: en`，就是当初定位它时用的那行 `:49` 日志）——证明"调用桩"完整转调了原始逻辑，不是装了个空壳；③ xochitl 的另一个子进程也被同一个 `LD_PRELOAD` 注入了（环境变量会被子进程继承），但它不是 `/usr/bin/xochitl` 本身，日志显示 safe mode 正确拦下了它，没有误伤；④ 全程 `ActiveState=active`/`SubState=running`，没有崩溃重启，10 分钟 4 次的节流配额只用掉这一次主动重启。**这套"字节特征码扫描 + 自定义 trampoline"机制从设计到真机验证走完了完整闭环，是目前为止本方案里风险最高的技术不确定性（"能不能在没有导出符号的情况下稳定 hook 住内部函数"）第一次拿到真机层面的正面结果。**

#### ⚠️ Step G · 真正让 `availableLanguageCodes` 多一项 `"zh_CN"`——真机测试连续踩了三次崩溃

（根因和最终方案都值得记一笔，不只是"最后成功了"这一句话。）

**第一次崩溃**：我进入语言设置菜单直接触发 xochitl 重启，`journalctl` 里完全看不到任何 Step G 相关日志——先复现了一遍"删掉 drop-in 回到干净基线，同样菜单路径不崩"，确认崩溃确实是这个 hook 引入的，不是无关的固件问题；怀疑是 `fprintf(stderr,...)` 没有 `fflush()`，进程被信号杀死时缓冲区里还没落盘的日志跟着一起丢了，给每一条诊断日志都套上 `fflush` 后重新部署。

**第二次崩溃**：这次日志能看到崩在 `QString::fromUtf8()` 调用内部。查 Qt 官方源码 `qbytearrayview.h` 找到根因：`QByteArrayView` 的成员声明顺序是**版本相关的**——Qt 7+ 是 `{data; size;}`，但设备跑的是 **Qt 6.10.3**，实际顺序是 `{qsizetype m_size; const char *m_data;}`（size 在前），最初按 `{data,size}` 假设传参，size 被当成指针直接解引用崩溃。改对参数顺序后再次部署——**第三次崩溃，还是崩在同一个调用点**，说明字段顺序只是这条路上的一个坑，`fromUtf8` 经函数指针跨 ABI 调用还有别的细节没摸透（没有继续深挖是什么）。最终方案：**完全放弃调用 `QString::fromUtf8`**，只依赖一个在之前所有测试里都稳定成功的符号——`QArrayData::allocate`——用它额外分配一段 `char16_t` 缓冲区，把 `"zh_CN"`（纯 ASCII）逐字节手动零扩展成 UTF-16，不再需要真正的 UTF-8 解码器，整个追加逻辑只依赖一个已验证符号。12 个宿主机单元测试全部通过后重新部署。

**最终结果：真机确认成功，无崩溃。** `journalctl` 证据：`命中 availableLanguageCodes 的 ReadProperty 调用，准备追加 zh_CN` → `Step G：availableLanguageCodes 追加 "zh_CN"：成功`；我在设置菜单里肉眼确认**多出一项"简体中文"**，全程 xochitl 保持同一个 `MainPID`，`NRestarts=0`，没有发生任何重启。**一个没预料到的惊喜**：这一轮明确说好"暂不管显示名"，但"简体中文"这四个字还是正确显示了出来——大概率是因为 `getLanguageDisplayName`/`nativeLanguageName`/`languageName` 这几个辅助方法内部走的是 `QLocale`，而 `QLocale` 自带一份完整的标准区域数据库、不依赖 app 自己那份写死的语言白名单，只要传进去的是像 `"zh_CN"` 这样格式合法的 ISO 区域代码就能正确解析（置信度：中，逻辑自洽且结果吻合，但没有再专门反编译这三个方法逐一确认具体是哪个在生效）——省掉了原计划里"还要额外处理三个显示名函数"这一步。三次崩溃全程没有伤到原始 `this+0x10` 硬编码成员本身，回退流程（删 drop-in + daemon-reload + restart）在中途验证有效，符合设计时"失败安全"的预期。

> 源码：`xovi-extensions/cangjie-langhook/src/qstringlist_append.c`（`QArrayData::allocate` 追加方案）；安装点 `hook_init.c:504-514`

#### ✅ Step I · 追加 `zh_TW`/`zh_HK` 后修复重名，同一个 hook 上再加一段判断，已真机验证

4.1 节确认根因是 `nativeLanguageName`/`getLanguageDisplayName`（本地方法 id=1/3）对 `zh_TW`/`zh_HK` 只能解析到 script 粒度、给出同一个字符串。修复复用**已经装在 `FUN_009174d0` 上的同一个 hook**：先无条件调用原函数不变，再追加判断 `call_type==0 && (id==1 || id==3)`——读 Qt `InvokeMetaMethod` 参数数组里 `args[1]`（传入的 `code` 字符串），跟 `"zh_TW"`/`"zh_HK"` 逐 UTF-16 code unit 比较（新增只读辅助函数 `cj_qstring_equals_ascii()`），命中就把 `args[0]` 指向的返回值槽位覆盖成手工构造的 `"繁體中文（台灣）"`/`"繁體中文（香港）"`，不命中（含 `zh_CN`）一律不插手。新字符串构造方式**完全复用 Step G 定型的方案**——只用 `QArrayData::allocate`，新增姐妹函数 `build_utf16_literal_qstring()`（直接 `memcpy` 预先写好的 `u"..."` 字面量，不再逐字节零扩展 ASCII），明确不碰 `QString::fromUtf8`。新增 2 个宿主机单元测试（覆盖字面量构造和字符串比较逻辑），交叉编译、部署、`systemctl restart xochitl` 后 `NRestarts=0`，我确认两项繁体中文**不再重名**，`zh_CN`/其它语言显示不受影响。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c`（`cj_override_display_name_if_zh_variant` 定义在 365 行，调用点在 500 行）

同时，建议每次准备升级固件或改动 hook 逻辑前，先用 `codexctl download` 把当前能正常工作的固件版本存一份在本地，出问题时可以用 `codexctl install`/`restore` 类命令回退（具体子命令用法以 codexctl 当前文档为准）。

#### 📌 竞品借鉴（rmtool / rmkit-cn）·"启动前指纹核验、不认识就裸启 xochitl"——给本项目自愈加一层外层防线

> 来源：`pretenderlu/rmtool`（GPL-3.0）`_xovi_standalone.py:509-590 launcher()/shared_dropin()`；`boangs/rmkit`（GPL-3.0）`systemd/zz-rmkit-cn.conf`、`installer/install.sh:540 abort_safe()`。**源码研读结论，未在本项目验证。**

本节的自愈设计（特征码自定位 + `KNOWN_COMPATIBLE_VERSIONS` 版本门 + safe mode）解决的是"**hook 进去之后**定位不准怎么办"。两个竞品补上了一层更靠外的防线——**在把 `.so` 注入之前就做完整指纹核验，任何一项不符就根本不加载 `LD_PRELOAD`、直接裸启原生 xochitl**。这跟本项目"注入后各目标独立 safe mode"是互补关系，不是替代：

- **rmtool 的做法（fail-safe launcher）**：drop-in 的 `ExecStart` 不直接写 `LD_PRELOAD`，而是指向一个 launcher 脚本；脚本按顺序核验 `uname -m`（架构）→ `/sys/devices/soc0/machine`（Ferrari/Chiappa/…平台）→ `/etc/version`（固件号）→ **`sha256sum /usr/bin/xochitl` 与预期值逐字节相等** → 每个扩展 `.so`、每个 `.qmd` 的哈希与数量——**任何一步 `|| stock`**，`stock()` 会 `unset LD_PRELOAD XOVI_ROOT ...` 再 `exec /usr/bin/xochitl --system`。即"指纹对不上就当没装过一样正常开机"，把 crash-loop 的可能性堵在注入之前。它还有个 `startup_guard`：起来后 `clear_guard_when_stable` 观察 N 秒确认 xochitl 稳定才清除标记，否则下次启动自动回退。
- **rmkit-cn 的做法（fail-open drop-in + precheck）**：`LD_PRELOAD` 指向 `active/` 下的 symlink，由 `precheck.sh` 在每次启动前决定"建立还是摘除"；预检不过 / `/home` 未挂 / 脚本缺失 → symlink 不存在 → glibc `warn+skip` → xochitl 纯原生启动。同样是"缺省安全、绝不砖"。

**与本项目现状对照**：本项目的 drop-in（`deploy/install.sh`）目前是 `Environment=LD_PRELOAD=<xovi.so>:<cangjie-langhook.so>` 直接注入，靠 `.so` 内部的版本门/特征码兜底。借鉴点是把 drop-in 的 `ExecStart` 改成一个**前置校验 wrapper**：`sha256sum /usr/bin/xochitl` 不等于已验证基线就 `exec /usr/bin/xochitl`（不注入）。收益：即使某次固件大改让特征码扫描本身出问题（09 节自认"不保证大版本跳变"），也不会 crash-loop，最坏只是"中文没了但机器能正常用"。这与本项目健康检查（`is-active`/`MainPID`/`NRestarts`）是两端呼应——一个防启动崩溃，一个测启动结果。

**方案评估结论（2026-08-15 定案）·选 rmkit-cn 的 precheck+symlink，不选 rmtool 的 ExecStart wrapper**：两条路都能"指纹不符就裸启"，取舍是——symlink 式**不接管 `ExecStart`**（保留 systemd 原生的 `ExecStart=/usr/bin/xochitl` 不动），只加 `ExecStartPre=-` 前置钩子 + 把 `LD_PRELOAD` 从真实 `.so` 路径改指向一个由钩子管理的 symlink；钩子核验不过就删 symlink，glibc 对缺失的 `LD_PRELOAD` 条目是 warn+skip → xochitl 纯原生启动。wrapper 式要用 launcher 脚本**替换 `ExecStart`**，与出厂 `xochitl-service-override.conf` 叠加时风险面更大、改动更重。symlink 式改动最小、缺省安全，选它。

> **⚠️ 后续演进（2026-08-15 xovi 整合，取代下方方案）**：设备装 vellum 后，cangjie 重构成**合规 xovi 扩展**放 `extensions.d/`，precheck+symlink 这套 fail-safe 被 **`.so` 内建的 `_xovi_shouldLoad`** 取代——不再需要独立 `precheck.sh` / `active` symlink / LD_PRELOAD 注入 cangjie；`/usr/lib` drop-in 改名 `zz-cangjie-xovi.conf`、只 preload `xovi.so` + QML env。详见姊妹文档《拼音输入法白皮书》M7「整合进 vellum 的 xovi 体系」。下方 checklist 记录的是被取代前的方案，保留作历史。

**落实状态（已进 install.sh + 新增脚本，2026-08-15 Move .166 真机验证通过）**：

- [✅ 真机验证通过] `deploy/install.sh` 的 drop-in 从 `Environment=LD_PRELOAD=<真实 .so>` 改为指向 `active/` 下两个 symlink（`.../cangjie-ime/active/xovi.so`、`.../active/cangjie-langhook.so`），并加 `ExecStartPre=-/bin/sh .../cangjie-ime/precheck.sh`（`-` 前缀 = 钩子自身故障也不阻塞 xochitl 启动，双保险）。真机切换后 `systemctl` 健康（active / MainPID 24455→25766 变化 / NRestarts=0），且 `/proc/<pid>/maps` 硬证 `cangjie-langhook.so`+`xovi.so` 经 symlink 解析后**真的被加载**（各 4 条映射）——symlink 间接层不影响 `LD_PRELOAD` 注入；用户随后肉眼确认**打字与界面中文一切正常**（切换只改 `.so` 加载路径、未动引擎/词典/翻译，功能与切换前一致）。
- [✅ 真机验证通过] 新增 `deploy/cangjie-precheck.sh`（装到 `/home/root/.local/share/cangjie-ime/precheck.sh`，`/home` 分区 OTA 不冲）：每次启动前核 `uname -m`=aarch64 + `sha256sum /usr/bin/xochitl` ∈ `xochitl.sha256.known` 基线 + 两个 `.so` 存在 → 全过才建 symlink、任一不过就删 symlink 裸启。install.sh 安装时把当前 `xochitl` sha256 **幂等追加**进基线。**核心破坏测试（真机）**：故意把基线改成假哈希（模拟 OTA 后 xochitl 变了）→ restart → xochitl `active`、NRestarts=0（**无 crash-loop**）、active 链数=0（摘链）、`.so` 映射=0（**裸启未注入**）；恢复真基线 → restart → 链数=2、`.so` 映射=4（**自愈重新注入**）。fail-safe 三义（不砖 / 裸启 / 自愈）全部真机坐实。另有意外发现：**设备 `/bin/sh` 实为 bash**（`/bin/sh -> /usr/bin/bash.bash`），非 BusyBox ash，shell 兼容性顾虑消除（coreutils 仍是 BusyBox 版，脚本已核实无 `head -1` 类不兼容写法）。
- [✅ 已实现，逻辑离线验证；真机走查待完整卸载时] 卸载路径参考 rmtool `_native_chinese.py:696` 实现为新增的 `deploy/uninstall.sh`：卸载时若 `xochitl.conf` `[General] language` 仍是 `zh_*` **先 `sed` 改回 `en`** 再删 `translations` 下三份 zh `.qm`，避免"配置指向已删 .qm"的悬空状态；随后删 drop-in / qmd / `$DATADIR`（字体保留），重启 + 同套健康检查。（本轮设备只做了"切 fail-safe 防线"，未执行完整卸载，故 uninstall 真机全流程留待需要卸载时。）

> **本轮真机范围（用户确认：只切 fail-safe 防线，不重跑整个 install.sh）**：手工完成 drop-in→symlink 切换 + precheck/基线/active 部署 + 破坏测试，未触碰 reading-qol/字体/翻译（那些维持设备现状）。改前已 `cp` 备份原 drop-in 到 `$DATADIR/zz-cangjie-langhook.conf.bak.pre-failsafe`。此外，本项目 drop-in 放 `/usr/lib`（避开 `/etc` overlay 重启丢失）与 rmkit-cn 的 bind-mount 双写是两种规避手段，本项目已定选 `/usr/lib` 持久 drop-in + precheck symlink 外层防线，**不采用双写**（见姊妹文档《拼音输入法白皮书》10 节 bind-mount 借鉴块，那条"择一"悬念据此销掉）。

## 05｜里程碑与时间表

下面的周期是我**第一次做这类嵌入式逆向项目**时的现实预估（业余时间投入，非全职），不是"理想情况下最快能做完"的乐观估计——真实进度大概率会更慢。这里只列 UI 汉化这条线（M0-M2）；虚拟键盘/拼音输入法这条线（M3 及以后）见姊妹文档《拼音输入法开发方案》第 09 节，M3/M4 均已真机验证完成。

### M0 · 环境搭建 ✓ 已验证达标 · 1 周

验收：codexctl 能跑通下载/ls/cat（已验证）；交叉编译工具链能编出一个 aarch64 的 hello world 二进制（已验证，见 2.4 节）。

### M1 · 中文字体能正常显示 ✓ 已验证达标（含自定义字体注入）· 1–2 周

验收：字体 subset 完成，在设备任意能看到文字的地方看到汉字不是方框/乱码——最初客观达标是因为设备自带 Noto Sans SC 覆盖了 CJK，3.3 节设计的"裁剪 + 注入"这条路径当时确实没有被验证过。**这个状态已经改变**：3.6 节把 `rmfw/fonts/` 里的字体（汉仪铁线黑等）真机部署到 `/home/root/.local/share/fonts/` 并 `fc-cache` 生效，进一步用 fontconfig fallback 规则把系统界面（`reMarkable Sans`/`Serif Small` 的中文 fallback）从 Noto Sans SC 换成了汉仪铁线黑，`fc-match -s` 验证排序 + 真机肉眼确认笔画变化——**自定义字体真正注入并生效这件事，这一轮由我自己在 Paper Pro Move 上验证过了**，不再是"做好了但没用上"的状态。

### M2 · 语言列表出现中文选项 ✓ 已验证达标 · 1–2 周

验收：Settings 里能选中"简体中文"/"繁體中文（台灣）"/"繁體中文（香港）"三项且互不重名（4.1/4.3 节 Step G/I，已上机确认无崩溃）；点击后界面确认切换成中文（Step H 反编译证实点击无崩溃风险，Step J 补齐了 423 条里 161 条此前从未进入翻译目录的字符串，我实测 Accessibility/Account/Turn off 等页面已生效）——3.5 节 Step J 记录了剩余约 262 条因缺少 QML 文件名锚点这一轮跳过，覆盖率不是 100%，但机制本身、三个语言变体、多数常见页面均已验证生效。

### 当前实际进度

M0/M2 已验证达标，M1 客观达标但不是靠计划中的机制——UI 汉化这条线（M0-M2）基本落地。键盘输入法这条线（M3-M7）已经推进到 M4 完成、真机全链路验证通过，进度和细节见姊妹文档，不在本文档重复。

> **拆分记录**：这份文档原本包含拼音输入法（原第 9 节）的全部内容，评估过一次"暂缓拆分"（当时 UI 汉化已基本做完、拼音引擎只有几十行方案设计，且 hook 机制是两边共享的基础设施，拆开收益有限），并明确记录了触发拆分的时机——"等 M3/M4 键盘 hook 真正开始产生真机调试内容之后"。这个条件已经出现（键盘输入法这条线的真机调试记录篇幅远超本文档，且后续两边各自独立迭代），拆分已完成，见姊妹文档《拼音输入法白皮书》。

## 06｜风险清单与合规提醒

### 6.1 变砖与数据风险

- Developer Mode 开启本身会清空数据——不是本方案带来的风险，是官方机制，动手前先备份。
- 真正的风险是 hook 代码写崩导致 xochitl 反复 crash——只要 SSH 通道还在，哪怕 UI 起不来也能远程删掉/禁用有问题的扩展，所以**务必保证 SSH 访问始终可用**，不要把 hook 逻辑写进影响 SSH 服务本身的路径。
- 每次做有风险的改动前，用 `codexctl download` 确保本地存有一份当前能正常工作的固件版本，作为回退依据。
- **重启节流（已用真机 systemd 配置核实精确数值，比社区口口相传的"约 10 分钟"更准）**：`xochitl.service` 单元文件里写的是 `StartLimitIntervalSec=600` + `StartLimitBurst=4`——含义是 **600 秒（10 分钟）滚动窗口内最多允许重启 4 次**，超过这个次数 systemd 会拒绝继续重启（进入 start-limit-hit 状态），要么等窗口过去，要么手动 `systemctl reset-failed xochitl`。另外服务本身还挂了 **60 秒的运行时看门狗**（进程环境变量里能看到 `WATCHDOG_USEC=60000000`，对应 `/etc/systemd/system.conf` 的 `RuntimeWatchdogSec=1min`）——xochitl 如果 60 秒内没有正常"喂狗"，systemd 认为它卡死会主动重启它，这也会占用上面 4 次重启配额；`system.conf` 里还有 `RebootWatchdogSec=2min`，是系统级看门狗，真卡死到这个程度会触发整机重启。装字体、调试 hook 这类需要反复"改一次测一次"的环节要按这个 4 次/10 分钟的真实配额安排节奏，不要连续重启刷验证。
- **⚠️ dm-verity 铁律（2026-08-16 真机两次踩实，修正本文多处"/usr 持久"旧说法）**：rootfs（`/dev/mmcblk0p2` ext4 `ro`）+ persist 分区都是 **dm-verity 完整性保护**（启动日志 `Reached target Local Verity Protected Volumes`）。**任何对 `/usr` 的写入（改本体、或新增翻译 `.qm` / 插件 / drop-in——新增文件也改文件系统元数据块）+ 完整设备重启 → verity root hash 变 → 校验失败 → A/B slot 回滚**。`mount -o remount,rw /usr` 当次可写、进程级 `systemctl restart xochitl` 保留改动只是**假象**，完整设备重启就回滚。→ **界面翻译（`/usr/share`）、xovi drop-in（`/usr/lib`）都不是真正"重启不丢"，完整重启会被 verity 回滚；真正持久只有 `/home`（加密、非 verity）。** 本文 2.5 节"根分区只读、remount rw 才能写"仍对，但涉及 `/usr` 持久化的各处旧说法（如本节上方 fail-safe 块与姊妹文档 M7 续"`/usr/lib` drop-in 普通重启不丢"）据此收窄——`remount rw` 只在不触发完整重启时短暂有效。详细论述见姊妹文档《拼音输入法白皮书》「M7 续²」③。
- **⚠️ 变砖事故实录（2026-08-16，正是上面第 2 条"务必保证 SSH 始终可用、不要把 hook 逻辑写进影响 SSH 服务本身的路径"这条原则的血泪反例）**：为修一个"重启后 xovi.so（在加密 /home）preload 失败"的启动竞态，给 `xochitl.service` drop-in 加了 `After=home.mount` + `Wants=home.mount`——xochitl 卡等加密 `/home` 挂载超时 → `OnFailure=emergency.target` + 看门狗 → **启动失败重启循环变砖**。而 dropbear 的 SSH host key bind-mount 自 `/home`、`/home` 没挂载 → dropbear 起不来 → **SSH 全死、无法远程回退（死锁：回退那个坏文件恰恰需要 SSH）**。最终靠 recovery mode（开机态长按电源 25-30s → 松 1-2s → 单按 → 松，USB 出 `2edd:0140`）+ 官方 Linux 工具 `rm_recover restore`（`device-recovery.cloud.remarkable.com`，重刷系统分区**保用户数据**；`reset` 才清数据）救回。**红线：绝不轻易改核心服务（xochitl）的启动依赖，尤其涉及加密/延迟挂载单元；改前必纸面推演失败场景 + 回退路径的可达性——本次死锁的本质就是"回退依赖 SSH、而启动失败恰恰让 SSH 不可用"。**

### 6.2 法律与许可合规

- **逆向工程的地区差异**：出于互操作性目的对自己合法拥有设备做的逆向工程，欧盟《软件指令》和美国 DMCA 1201(f) 都有相应的互操作例外条款，但这是复杂且因地区而异的法律问题，本文不下定论，建议动手前自己查一下所在地区的相关规定。
- **我协议层面**：即便法律上站得住，reMarkable 的最终我许可协议仍可能包含禁止修改/逆向的条款，违反的后果通常是保修/服务失效这类民事层面的问题，建议完整读一遍协议。
- **字体许可**：思源黑体/Noto Sans CJK 是 SIL OFL 1.1，个人嵌入使用没问题，保留版权声明文件即可；如果以后打算公开发布我的扩展包给别人用，再仔细核对一次协议条款。
- **词库/双拼方案许可（详见姊妹文档《拼音输入法白皮书》第 04 节）**：拼音引擎依赖的 RIME 生态词库（`rime-luna-pinyin`/`rime-essay-simp` 是 LGPL v3，`rime-double-pinyin`/`rime-ice` 雾凇拼音是 GPL v3）不是本文档范围，这里不重复展开。
- **xovi 本身的许可**：`asivery/xovi` 是 **LGPL v3**（4.3 节 `cangjie-langhook` 扩展里 `trampoline_aarch64.c` 的指令编码方式抄自它的 `pivotSymbol()`，已在代码注释和 README 里注明出处）。
- **范围提醒**：本方案从头到尾都是"让自己的设备显示中文、能打中文字"，不涉及也不建议扩展到破解付费功能、绕过设备管控这类用途。

### 6.3 社区资源核实结论

动手前系统过了一遍 [reHackable/awesome-reMarkable](https://github.com/reHackable/awesome-reMarkable) 全部分类（APIs / Applications / Cloud Tools / Custom Templates / Device Tools / GUI Clients / Interface Customization / Screen Sharing 等），以及 Interface Customization 分类下全部 xovi 相关仓库（`rm-xovi-extensions`、`rm-hacks-xovi-qmd` 系列的五个分叉、各家 `xovi-qmd-extensions`）——**没有一个项目涉及语言包、输入法或 CJK**，包括看起来功能最杂的 `rm-hacks-xovi-qmd`（Samarkin 版），实测也只是清屏、手势、工具栏图标这类日常体验的小改进。这确认了本方案"没有现成中文化/输入法项目可以直接借用，需要自己做"的判断，不是调研时漏看了什么。

不过列表里几个和中文化无关的 xovi extension 仓库，作为"真实跑通的代码范式"仍值得在第 04 节动手前通读一两个——比如怎么在 `init` 里用 xovi API 取符号地址、怎么处理 QRC 资源，比单纯照抄官方 README 骨架更有实操参考价值：`qt-resource-rebuilder`（资源替换的标准做法，第 2.4 节已引用）、[rmitchellscott/xovi-qmd-extensions](https://github.com/rmitchellscott/xovi-qmd-extensions)。

## 07｜结项（2026-08-16）

整条中文化线（UI 汉化 + 拼音输入法 + 字体 + 设置页入口）**真机端到端跑通并收口**。设备固件 3.28.0.169（build 20260806095513，Qt6.10.3），走 vellum-xovi 体系，`cangjie-langhook.so`（md5 `345d5100…`，源码 `make aarch64` 可复现重建、零警告）+ `qt-resource-rebuilder` 各 4 段注入 xochitl，`systemctl is-active=active`、`NRestarts=0`。

**最终交付（均真机验证）**：

- **拼音输入法**：全拼/双拼 × 简/繁 4 模式 + 简拼 + 中英混输，逐字造句/分段提交/退格撤销；候选栏为 QMLDiff 注入的原生 `CjCandidateBar`；地球弹层切 4 模式。详见姊妹文档《拼音输入法白皮书》。
- **UI 界面汉化**：bind-mount 覆盖 `/usr/share/.../translations`（verity 安全，运行时 VFS 挂载不改 `/usr` 块），xochitl 原生加载 zh，中英双向切换正确；挂载走 xovi pre-start 脚本，每次 `xovi/start`/reenable 自动重放。
- **UI 字体**：**统一到 LXGW Neo XiHei Screen Full**（霞鹜新晰黑·屏幕阅读版·补全，单文件覆盖简繁英、e-ink 不发虚），UI 与候选栏共用一份，fontconfig 驱动。**此决定取代本文 6.x/CJK 预检段里"HarmonyOS TC 补繁体 / 观感用 HarmonyOS·汉仪"的旧描述**——结项后不再部署 HarmonyOS，`candidatebar.qmd`/`fontconfig`/`install.sh`/安装包已同步去除 HarmonyOS。**⚠️ 已再度演进（当前定案见 §3.3）**：晰黑单字重 e-ink 发虚 + 只覆盖到扩展 A 会让扩展 B 生僻字豆腐块，故 2026-08-23 主字体又从新晰黑换成**霞鹜新致宋（LXGW Neo ZhiSong Screen Full）+ 花园明朝 B（HanaMinB）扩展 B 兜底**；`candidatebar.qmd`(cjkFamily)/`fontconfig-cangjie.conf`/`install.sh`/安装包均已同步为致宋+HanaMinB，本条"统一到新晰黑"仅存历史。
- **设置页屏幕键盘中文入口**：纯 QMLDiff 补丁 `settings-keyboard-zh.qmd`（`?#keyboardDialog > SelectionComponent` + 3 REPLACE），设置→语言和键盘→屏幕键盘 出现"中文"项；qrr 设备日志 `Processing file .../LanguageAndKeyboard.qml` 证实 diff 应用。详见姊妹文档「M7 续²」①（含"qmldiff 能穿透 Component{}、方案 D 判死"的翻案）。
- **荧光笔汉字精确吸附（Step HL2）**：同一份 `cangjie-langhook.so` 里第三组 hook——拦 xochitl 手写命中区间→整行扩张函数 `FUN_00f05ad0`，让荧光笔划中文时不再"划一小段吸整行"（病根：xochitl 手写空格分词对 CJK 失效）。开关走 `reading-qol.json` 的 `hlSnapCjk`（默认开）。这是"块 4 系统增强"跨到"块 2 中文化"的特性、二进制不拆。真机 2026-08-23 通过，机理。

**部署与恢复（维护须知）**：

- 一键安装包 `chinese-ime/langhook/deploy/dist/cangjie-ime-installer.tar.gz`（`cangjie-ime/{install.sh,uninstall.sh,payload/}`）。设备已装整套时增量更新只需 `scp` 对应文件到 `xovi/exthome/qt-resource-rebuilder/` + 安全重启（隔离冒烟→systemd），无需重装。
- **持久性分层**：cangjie 扩展/词典/qmd/字体/翻译全在 `/home`（加密、非 verity），普通重启不丢；**引导配置在 /etc tmpfs，真机完整重启即清 → 需手动 `xovi/start`（或 `vellum reenable`）恢复**（无开机自动服务，见下"已知局限"）。**固件 OTA 冲掉 rootfs 后，重跑 `install.sh` 一键恢复。**

**已知局限 / 主动搁置（诚实记录，非缺陷遗漏）**：

1. **开机自动恢复未做**：/etc overlay 重启清空、无干净的开机自启方案，重启后需手动 `xovi/start`。真机多为休眠（tmpfs 不丢），此负担实际很小，判定为可接受。
2. **commitString 注入通道（B）默认关**：`cj_commit_via_ime` 已真机验证可用（插入+退格两条通道走公开 Qt 符号），但保留在 `CJ_COMMIT_VIA_IME` 开关后、暂不设默认——等与 qrr 一起做完整真机回归再考虑转正。见。
3. **设置页"中文"项的选中/切换/高亮**三点未逐项复测，靠"地球弹层设 `zh_CN_SP` 已工作、与 `keyboardLanguage` 同底层"高置信成立。
4. **输入法架构方向③（标准 Qt IM 插件替代 6-hook）已判死**（xochitl 要求安装私有具体类 `KeyboardInputContext` 本体），现有 hook 架构是正解、非待替换 hack。见。

## 参考来源

1. [asivery/xovi](https://github.com/asivery/xovi) —— reMarkable Paper Pro 系列的扩展 hook 框架，本方案的核心依赖
2. [Jayy001/codexctl](https://github.com/Jayy001/codexctl) —— 离线下载/解压/浏览 reMarkable 固件镜像，已验证支持 Paper Pro Move（`rmppm`）
3. [reMarkable 官方支持 · Developer mode](https://support.remarkable.com/s/article/Developer-mode) —— 官方开发者模式说明，开启会清空设备数据
4. [reMarkable 官方支持 · Keyboard Settings](https://support.remarkable.com/hc/en-us/articles/360002674938-Keyboard-Settings) —— 现有 Language/Keyboard 设置页结构
5. [KOReader Wiki · Installation on reMarkable](https://github.com/koreader/koreader/wiki/Installation-on-ReMarkable) —— Paper Pro / Move 上 xovi 安装流程的参照范例
6. OpenCC（BYVoid/OpenCC）—— 简繁转换库，3.2 节的核心依赖（姊妹文档拼音候选的繁体转换也复用同一份依赖）
7. SIL Open Font License 1.1（scripts.sil.org/OFL）—— 思源黑体/Noto Sans CJK 的授权协议
8. [chenhunghan · How to install Noto Sans CJK fonts for reMarkable Tablet](https://gist.github.com/chenhunghan/b9dbb6ad4095fa12c31838784c26073d) —— 3.3 节字体注入路径的社区实测来源（reMarkable 1 / 固件 3.27.3.0）
9. [remarkable.jms1.info · Fonts](https://remarkable.jms1.info/info/fonts.html) —— 3.6 节字体部署目录（`/home/root/.local/share/fonts/`）、`fc-cache` 重建缓存、rMPP `/var/cache` tmpfs 持久性问题的社区文档来源，本轮在 Paper Pro Move 上真机复核过
10. [reHackable/awesome-reMarkable](https://github.com/reHackable/awesome-reMarkable) —— 6.3 节核实"社区无现成中文化/输入法项目"结论所依据的项目列表
11. 《reMarkable Paper Pro Move 拼音输入法开发方案》（姊妹文档）—— 虚拟键盘 hook、拼音/双拼引擎、候选词典、镇纸 Paperweight 参考分析全部内容的出处，含独立的参考来源列表

### ⛔ 笔记打字文本换字体——判死（2026-08-22，真机双实验）

**诉求**：笔记打字默认中文致宋/英文 KF Readerly（参照阅读字体增强）。**判死证据链**：① 笔记本 `.content` 也有 `fontName` 字段，但真机两轮实验（KF Readerly、LXGW WenKai Screen，风格极易分辨）打字文本纹丝不动——`fontName` 只被 EPUB 管线消费；② 二进制字符串只有 `reMarkable Sans`/`reMarkable Serif Small` 两个家族、556 个解出 QML 无笔记文本字体设置点——字体钉死在 C++ 场景渲染层；③ fontconfig 捷径不通：英文换不了（内嵌 reMarkable Sans 拉丁齐全 fallback 不触发、应用字体优先于替换规则），中文 fallback 绑定管的是**全 UI** 不是单笔记。**剩余路径=逆向 hook QFont 构造点，除 Step S 级风险外有产品硬伤：手写批注锚定到打字文本字符度量（anchor_id+origin_x），换字体即老笔记手写错位**——判不立项。低风险替代（未做）：全 UI 中文 fallback 换宋/楷（一条 fontconfig 规则，全界面生效，非"笔记"开关）。工具沉淀：extract_qml.py 对 .169 二进制解出 556 QML 可复用。

### 📌 字体线三连（2026-08-22）：核查结论 + Readerly 宋体判死 + embolden 加粗

**核查**（用户问"全 UI 中文=黑体 Screen、阅读菜单=Screen 版+Readerly"是否属实）：全部属实——UI 中文 fallback=`LXGW Neo XiHei Screen Full`（配置在 `~/.config/fontconfig/fonts.conf`，/home 持久所以扛重启，本次首次归档进仓库 `chinese-ime/fonts/fonts.conf`）；阅读菜单三项=文楷 Mono GB Screen/致宋 Screen Full/KF Readerly。**⚠️ 时效更新（当前定案见 §3.3）**：本条记于 2026-08-22，次日（08-23）UI 中文主字体已改为**霞鹜新致宋 + 花园明朝 B 扩展 B 兜底**，此处"UI 中文 fallback=新晰黑"仅存当日快照；仓库 `chinese-ime/fonts/fonts.conf` 与 `deploy/fontconfig-cangjie.conf` 现均为"致宋 + HanaMinB"两级链。UI 英文改 Readerly 不可达（内嵌 reMarkable Sans 拉丁齐全，fallback 不触发、替换规则拦不住应用字体）。

**⛔ "选 Readerly 读中文书汉字回退致宋"判死（双实验+PDF 解剖实证）**：fc-match 层规则全部生效（按 family 定向 prepend_first 后 fc-match 三向验证过），但强制重渲后 `pdffonts` 实测 PDF 仍嵌晰黑——EPUB 渲染是 **xochitl fork 的 worker**（journal 里 rm.worker.unix 前缀+qmldiff/cangjie 日志实证，prgname 仍=xochitl），其 CJK 回退查询不带请求家族 → fontconfig 层既不能按家族也不能按进程定向。prgname 测试本身可用（拿 fc-match 当靶验证过）。**结论：读中文书要衬线直接选致宋（自带衬线拉丁）**；真要精确"英 Readerly+中宋"只剩 EPUB 优化器注入 `font-family: Readerly, 致宋` 双字体 CSS 栈一条路（未验证，需先测渲染器吃不吃 CSS 家族列表，未立项）。**fontconfig 语法坑**：`prepend` 在 test 命中同属性时插到命中值之前而非队头，占队头用 `prepend_first`。

**笔记文字发虚缓解（A 方案，实验中）**：晰黑 `<match target="font">` 开 `embolden`（FreeType 合成加粗），全 UI+笔记中文变粗，用户反馈"改善但不够"→ 后续 C 方案=验证 DeviceSceneView 的 mono 引擎是否天然覆盖笔记场景（快速黑白开关直测）。
