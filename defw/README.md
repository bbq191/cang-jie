# defw —— xochitl 3.28.0.172 逆向工程

> **读者与用途**：要给 xochitl 写新的 hook / qmd、查 xochitl 崩溃栈，或想复现"某个函数地址是怎么定位到的"的人。
> 这里是**逆向工程的工作目录与方法说明**（Ghidra 项目 + headless 脚本），不是功能代码，也不上设备；用它支撑的成果在 [`../enhance/handwriting-stroke/`](../enhance/handwriting-stroke/README.md) 等处。
> 整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。原名 `ghidra-project-328`，2026-09-10 改名 `defw`。

**跟旧的 `ghidra-project/` 是两个独立项目，别混**：旧目录是固件 3.28.0.169 时代的分析成果，已随 2026-09-11 的仓库整理搬出 git 仓库，函数地址对不上现在的固件，本目录不复用它。

## 目录里有什么

| 内容 | 说明 |
|---|---|
| `xochitl_328_analysis.gpr` | Ghidra 项目标记文件（headless 建的是 0 字节，正常，元数据在 `.rep/` 里）。**只有它入库**；`.rep/`（项目库）、`.lock`、`*.log` 都被 gitignore。clone 下来要自己重新导入分析 |
| `scripts/` | headless 分析脚本（Java，Ghidra Script API），见下表 |

项目里的程序名是 **`xochitl-3.28.0.172`**（headless `-process` 要用它）。`xochitl` 二进制本身**不进仓库**（体积大，也是 reMarkable 官方二进制，不该入库）；用时 `scp root@10.11.99.1:/usr/bin/xochitl` 现拉，导入时把文件名改成上面的程序名，后面的命令就能照抄。

## 脚本

`scripts/` 下都是**可复用的通用工具**：

| 脚本 | 做什么 | 地址从哪给 |
|---|---|---|
| `DecompileContaining.java` | 反编译**包含某地址的函数**（地址可以落在函数中间，比如崩溃栈里的 PC）；打印入口、偏移、签名、C 输出和调用者 | 脚本参数 |
| `ListRefs.java` | 列出指向某地址的全部引用（READ / WRITE / DATA / CALL…）及所在函数——回答"谁写这个全局变量""谁注册了这个任务函数" | 脚本参数 |
| `DecompileTargets.java` | 反编译一组**函数入口**并列出调用者 | 改源码里的地址数组 |
| `CheckFuncSizes.java` | 核对候选 hook 目标的字节大小与前 N 条**原始反汇编**：`patch_target` 需要 ≥ 20 字节可安全改写的序言；也用来交叉核实反编译结果，防止反编译器简化过头（真救过一次，见增强线白皮书 §04） | 改源码 |
| `FindQuillStrokeRTTI.java` | 在内存里搜"字面等于某地址的 8 字节指针"，从 RTTI 名字字符串反查 typeinfo（stripped 二进制的自动 xref 是空的） | 改源码 |
| `FindGenerateCallSite.java` | 找 `VaryingGenerator_WidthLength::generate`（`FUN_00f401f0`）的直接引用与 vtable 槽位引用；结果是"零真实调用者"，见增强线白皮书 §03c 勘误 | 固定 |

**地址怎么换算**：Ghidra 里的地址 = xochitl 运行时基址 `0x400000` + 偏移。崩溃栈里写的 `xochitl+0x6467b8`，在 Ghidra 里就是 `00a467b8`。脚本参数用不带 `0x` 的十六进制。

## 环境搭建（一次性）

Ghidra 走发行版包管理器（CachyOS/Arch 官方仓库有预编译包）：

```sh
paru -S ghidra --assume-installed java-environment=21
```

- `--assume-installed` 跳过虚拟依赖 `java-environment>=21`（已有 sdkman 的 JDK 21，不想再拉一份系统 JDK）。装完命令在 `/usr/bin/ghidra`、`/usr/bin/ghidra-analyzeHeadless`，主目录 `/opt/ghidra`。
- **Ghidra 12.x 要求 JDK 21**。`/opt/ghidra/support/launch.properties` 归 root 所有、改不了，所以每次调用用 `JAVA_HOME` 指定（`launch.sh` 会读它）。嫌麻烦可以自己用 sudo 把 `JAVA_HOME_OVERRIDE` 写进 `launch.properties`。
- GUI 在 Wayland 平铺式合成器下会整窗口空白：设 `_JAVA_AWT_WM_NONREPARENTING=1`。

## 常用命令

下面 `<proj>` 是放 `.gpr` 的目录（即本目录），`J` 是 JDK 21 路径。**先关掉 GUI 里的这个工程**——headless 和 GUI 不能同时打开，会抢 `.lock`。

```sh
J=~/.local/share/sdkman/candidates/java/21.0.12-tem

# 首次导入并自动分析（耗时较长）
JAVA_HOME=$J ghidra-analyzeHeadless <proj> xochitl_328_analysis -import <路径>/xochitl-3.28.0.172

# 崩溃栈里的 PC → 所在函数的 C 代码与调用者（只读，不重新分析）
JAVA_HOME=$J ghidra-analyzeHeadless <proj> xochitl_328_analysis -process xochitl-3.28.0.172 -noanalysis -readOnly \
  -scriptPath defw/scripts -postScript DecompileContaining.java 00a467b8 00a49fc8

# 谁引用了某个全局变量
JAVA_HOME=$J ghidra-analyzeHeadless <proj> xochitl_328_analysis -process xochitl-3.28.0.172 -noanalysis -readOnly \
  -scriptPath defw/scripts -postScript ListRefs.java 01aa1880
```

`DecompileTargets.java` 等"改源码给地址"的脚本同理，把 `-postScript` 换掉、去掉末尾参数即可。

## 方法：怎么从"零线索"走到"hook 目标真机部署"

1. **字符串侦察**：`strings` / `c++filt` 在真机 `xochitl` 上找 RTTI 名字（手写笔迹那次是命名空间 `Quill::strokev2` 一整套笔画光栅化类）。这只到字符串层面，不代表能 hook。
2. **GUI 探索**：完全不知道往哪查时，在 Ghidra CodeBrowser 里人工点、看。Qt 编译进二进制的调试字符串（源码路径、方法名）是给 `FUN_xxxx` 挂上名字最可靠的线索。
3. **headless 脚本接力**：一旦有了具体地址，反编译、查引用、按字节搜内存都用 `scripts/` 批量做，不必再靠 GUI 截图。
4. **反编译要交叉核实**：关键偏移和参数签名用原始反汇编（`CheckFuncSizes.java`）核对，别只信反编译出来的 C。

完整的发现、踩坑与勘误记在 [`../enhance/handwriting-stroke/README.md`](../enhance/handwriting-stroke/README.md) 和 [`../enhance/docs/reMarkable系统增强线白皮书.md`](../enhance/docs/reMarkable系统增强线白皮书.md) §03c–§03f、§04。**本目录只管"怎么用这套工具"，不重复记发现内容**；唯一例外是下面这条不属于任何功能线的崩溃调查。

## 调查记录：xochitl 退出时崩溃（2026-09-25）

**结论（置信度中高）**：xochitl 退出时自己的静态析构顺序有问题——先销毁了墨水屏刷新任务要用的全局数据，后停线程池；退出那一刻屏幕恰好在刷新，就读到已销毁的对象而崩溃。崩溃栈里没有我们扩展的帧，与本项目无关。

**证据来源**：设备上 memfault 存的 5 份崩溃栈（`~/.memfault/mar/*/stacktrace.json.gz`，用其中的 `symbols` 表把 PC 换成"模块+偏移"）。3 份崩在同一个线程池里，用 `DecompileContaining.java` 与 `ListRefs.java` 查清了机制：

- **线程池**：工作循环 `FUN_00a47fe0`，6 格环形任务队列，取出任务（可调用对象）就执行；由静态初始化里的 `FUN_0048bba0` 建出（`DAT_01aa0d00`、`DAT_01aa1eb0` 两个池）。
- **崩溃点**：任务函数 `FUN_00a46190`（+0x628）和 `FUN_00a49c90`（+0x338）。后者 `ldr x3,[DAT_01aa1880]` 后 `ldr x7,[x3,#0x28]` 就崩；两者都读全局 `DAT_01aa1880`。任务由 `FUN_00c0dba0` 注册，那里引用 `/sys/bus/i2c/drivers/g2194-regulator/0-0048`（屏幕电源稳压器）与 `enable_nowait`——也就是**墨水屏刷新管线**。
- **析构顺序**：静态初始化 `FUN_0048c010` 里 `__cxa_atexit` 的登记顺序是先两个线程池（析构 `FUN_00a45a50` / `FUN_00a45960`），后 `DAT_01aa1880`、`DAT_01aa18b8…1900` 这批数据（析构 `FUN_00a48330` / `FUN_00a41480`）。atexit 是后登记的先执行 → **退出时数据先析构、线程池后停**。崩溃前 xochitl 日志正是 `shutdown: waiting for display to finish...`，对得上。
- **未查**：另外 2 份崩在 `libQt6Gui+0x495098`。

**我们的对策**：让改动生效一律主动整机重启，不再停止或重启 xochitl（见 [`../packaging/README.md`「怎么让改动生效」](../packaging/README.md#怎么让改动生效2026-09-25-起一律整机重启)）。理论上可以 hook `exit()` 改走 `_exit` 跳过析构，但 xochitl 可能在析构里落盘设置或文档状态，有丢数据风险，**不做**。
