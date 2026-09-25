# defw —— xochitl 3.28.0.172 逆向工程

> **读者与用途**：要给 xochitl 写新的 hook / qmd、或想复现"某个函数地址是怎么定位到的"的人。这里是**逆向工程的工作目录与方法说明**（Ghidra 项目 + headless 脚本），不是功能代码；用它支撑的成果在 [`../enhance/handwriting-stroke/`](../enhance/handwriting-stroke/README.md) 等处。
> 整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。原名 `ghidra-project-328`，2026-09-10 改名 `defw`。

**跟旧的 `ghidra-project/` 是两个独立项目，别混**：旧目录是固件 **3.28.0.169** 时代的分析成果，已随 2026-09-11 的仓库整理搬出 git 仓库，其函数地址对不上现在的固件，本目录不复用它。新固件重新导入分析，产物落这里。

## 目录里有什么

| 内容 | 说明 |
|---|---|
| `xochitl_328_analysis.gpr` | Ghidra 项目标记文件。`.rep`（二进制项目库）、`.lock`、`.log`（如本机的 `import.log`、`rtti_find*.log`）都 gitignore，**只有 `.gpr` 入库**（headless 建的 `.gpr` 是 0 字节也正常，元数据在 `.rep/` 里）。clone 下来只有 `.gpr` 和脚本，要自己重新导入分析 |
| `scripts/` | headless 分析脚本（Java，Ghidra Script API），见下表 |

`xochitl` 二进制本身**不进仓库**（体积大，也是 reMarkable 官方二进制，licensing 上不该入库）；用时 `scp root@10.11.99.1:/usr/bin/xochitl` 现拉。

## 脚本

`scripts/` 下是**可复用的通用工具**，不是一次性脚本：

| 脚本 | 做什么 |
|---|---|
| `DecompileTargets.java` | 改里面的地址数组重跑，就能反编译任意函数并列出调用者（xref） |
| `CheckFuncSizes.java` | 核对候选 hook 目标的字节大小与前 N 条**原始反汇编**——`patch_target` 需要 ≥ 20 字节可安全 patch 的序言，且用来交叉核实反编译结果、防止反编译器简化过头（这条真救过一次，见增强线白皮书 §04） |
| `FindQuillStrokeRTTI.java` | 在内存里搜"字面等于某地址的 8 字节指针值"，从 RTTI 名字字符串反查 typeinfo（对 stripped 二进制，自动 xref 是空的） |
| `FindGenerateCallSite.java` | 找 `VaryingGenerator_WidthLength::generate`（`FUN_00f401f0`）的直接引用与 vtable 槽位引用；这次的结果是"零真实调用者"，见白皮书 §03c 勘误 |
| `DecompileContaining.java` | 按**任意地址**（比如崩溃栈里的 PC）反编译**包含它的函数**，地址走脚本参数，不用改源码；打印入口、偏移、C 输出和调用者 |
| `ListRefs.java` | 列出指向给定地址的全部引用（READ / WRITE / PARAM / CALL…）及所在函数——回答"谁写这个全局变量""谁注册了这个任务函数" |

## 环境搭建（一次性）

Ghidra 走发行版包管理器（CachyOS/Arch 官方仓库有预编译包）：

```sh
paru -S ghidra --assume-installed java-environment=21
```

`--assume-installed` 跳过依赖里的虚拟包 `java-environment>=21`（已有 sdkman 的 JDK 21，不想再拉一份系统 JDK）。装完二进制在 `/usr/bin/ghidra`、`ghidra-analyzeHeadless`，主目录 `/opt/ghidra`。**Ghidra 12.x 要求 JDK 21**，而 `/opt/ghidra/support/launch.properties` 是 root 拥有、不能改，所以每次调用用 `JAVA_HOME` 环境变量指定（`launch.sh` 会读它）：

```sh
JAVA_HOME=~/.local/share/sdkman/candidates/java/21.0.12-tem ghidra-analyzeHeadless <project_location> xochitl_328_analysis -import <xochitl路径>
```

嫌每次都写，可以自己用 sudo 把 `JAVA_HOME_OVERRIDE` 写进 `launch.properties`（需要电脑上的 sudo 权限）。

已有具体地址之后，对已导入的程序跑脚本（**GUI 必须先关闭工程**，headless 和 GUI 不能同时开同一个工程，会抢 `.lock`）：

```sh
JAVA_HOME=... ghidra-analyzeHeadless <project_location> xochitl_328_analysis -process -scriptPath defw/scripts -postScript DecompileTargets.java
```

（`-process` 后需带程序名，具体参数以 `analyzeHeadless` 文档为准；本文不保证程序在项目里的名字。）

GUI 环境坑：Java Swing 在 Wayland 平铺式合成器下会整窗口空白，设 `_JAVA_AWT_WM_NONREPARENTING=1`。

## 方法论（怎么从"零线索"走到"两个 hook 目标真机部署"）

1. **字符串侦察**：`strings` / `c++filt` 在真机 `xochitl` 上找 RTTI 名字（这次是命名空间 `Quill::strokev2` 一整套笔画光栅化类）——只到字符串层面，不代表能 hook。
2. **GUI 探索式排查**：完全不知道往哪查的阶段，在 Ghidra CodeBrowser 里人工点、看截图。Qt 编译进二进制的调试字符串（源码路径、方法名）是把 `FUN_xxxx` 挂上名字的最可靠线索。
3. **headless 脚本接力**：一旦有了具体地址，反编译 / 查 xref / 按字节搜内存都能用 `scripts/` 批量做，不必再靠 GUI 截图。

完整的反解发现、踩坑与勘误记在 [`../enhance/handwriting-stroke/README.md`](../enhance/handwriting-stroke/README.md) 和 [`../enhance/docs/reMarkable系统增强线白皮书.md`](../enhance/docs/reMarkable系统增强线白皮书.md) §03c–§03f、§04；**本目录只管"怎么用这套工具"，不重复记发现内容**。发现即写：探索有结论就更新对应文档，别只留在脚本输出里。

## 调查记录：xochitl 退出时崩溃（2026-09-25）

设备上 memfault 存的崩溃栈（`~/.memfault/mar/*/stacktrace.json.gz`，按其中 `symbols` 表把 PC 换成"模块+偏移"；xochitl 的运行时基址是 `0x400000`，Ghidra 里的地址＝`0x400000 + 偏移`）有 3 份崩在同一个线程池里，这里用上面两个脚本查清了机制：

- 工作循环 `FUN_00a47fe0`：xochitl 自己的线程池，6 格环形任务队列，取出任务（可调用对象）就执行；由静态初始化里的 `FUN_0048bba0` 建出（`DAT_01aa0d00`、`DAT_01aa1eb0` 两个池）。
- 崩溃点在任务函数 `FUN_00a46190`（+0x628）和 `FUN_00a49c90`（+0x338）里：后者 `ldr x3,[DAT_01aa1880]` 后 `ldr x7,[x3,#0x28]` 就崩；两者都读全局 `DAT_01aa1880`。任务由 `FUN_00c0dba0` 注册，那里引用 `/sys/bus/i2c/drivers/g2194-regulator/0-0048`（屏幕电源稳压器）与 `enable_nowait`——**墨水屏刷新管线**。
- 静态初始化 `FUN_0048c010` 里 `__cxa_atexit` 的登记顺序：先两个线程池（析构 `FUN_00a45a50` / `FUN_00a45960`），后 `DAT_01aa1880`、`DAT_01aa18b8…1900` 这批数据（析构 `FUN_00a48330` / `FUN_00a41480`）。atexit 后登记先执行 → **退出时数据对象先析构、线程池后停**；那一刻池里恰有刷新任务在跑就读到已析构的对象 → SEGV。崩溃前 xochitl 日志正是 `shutdown: waiting for display to finish...`，对得上。
- 结论（置信度中高）：xochitl 自身的静态析构顺序问题，概率取决于退出瞬间屏幕是否在刷新；崩溃栈里没有我们扩展的帧。另外 2 份崩在 `libQt6Gui+0x495098`，没查。
- 我们这边的对策：部署生效一律主动整机重启（见 `packaging/README.md`「怎么让改动生效」）。理论上可以 hook `exit()` 改走 `_exit` 跳过析构，但 xochitl 可能在析构里落盘设置/文档状态，有丢数据风险，**不做**。
