# shared —— xovi 扩展的 hook 基础设施（C）

`hl-snap` 靠"找到 xochitl 里的某个函数 → 改写它的开头跳到自己的代码"来工作，这套基础设施放在这里，它的 `Makefile` 用 `LANGHOOK_SRC_DIR` 指向本目录。原先 `handwriting-stroke`（手写优化，`hw-stroke.so`）也用这一份；它 **2026-09-30 已移除**，下面提到它的段落是历史记录。这里的源码仍被 hl-snap 编进 `hl-snap.so`，没有只给 hw-stroke 用的文件，所以一个也没删（源码里提到 `hw_stroke.c` 的注释也保留不改：扩展带 `-g` 编译，动注释会改变行号调试信息，进而改变 `hl-snap.so` 的 md5）。整个流程的图解见白皮书 §02（[`../docs/diagrams/xovi-hook-lifecycle.svg`](../docs/diagrams/xovi-hook-lifecycle.svg)）。

| 文件 | 做什么 |
|---|---|
| `scan.c/.h` | `cj_find_exec_module`：读 `/proc/self/maps`，找到 `/usr/bin/xochitl` 的可执行段（连同被 `mprotect` 切开的续段，见下节） |
| `pattern.c/.h` | `cj_find_unique_pattern`：在段里搜特征码（目标函数开头 32~40 字节的原始机器码），**必须恰好命中 1 处**。精确匹配先用 `memchr` 跳到首字节候选再整段比较（2026-09-24，16 MB 扫描 39.6 → 3.2 ms，与逐字节循环差分对拍一致） |
| `trampoline_aarch64.c/.h` | `cj_build_far_jump`：拼一条跳到任意 64 位地址的 ARM64 远跳转（20 字节） |
| `trampoline_patch.c/.h` | `cj_patch_target`：mprotect 目标页 → 把开头 20 字节抄进新分配的"调用桩"并接上跳回原函数的远跳转 → 把目标开头改写成跳到 handler → 刷指令缓存。任一步失败返回 0、不改任何字节 |
| `tests/` | host 单测：`make test` |

## 多扩展共存：合并被 mprotect 切开的代码段（2026-09-24 修；hw-stroke 09-30 移除后只剩 hl-snap 一个扩展，这个修复仍保留）

每装一个 hook，`mprotect` 都会把目标所在的那一页从 xochitl 的代码段里切出来（`r-xp` / `rwxp` / `r-xp`）。`cj_find_exec_module` 原先找到**第一行**匹配的可执行映射就返回，于是"第一段"只到最低的已 patch 页之前为止：**后装 hook 的扩展只能找到地址低于已 patch 页的目标**。09-24 真机两个扩展都装上了，是因为顺序恰好是 hw-stroke（`0xf47530`、`0xf4c8d0`）先、hl-snap（`0xf03670`）后；反过来 hw-stroke 会在 `_xovi_construct` 里静默装不上。

现在找到第一段后，把紧随其后、同一路径、地址首尾相接、可读可执行（`r-xp` 或 `rwxp`）的续段一并算进来。host 单测用真内核复现了切段（私有映射一个文件再 `mprotect` 中间一页，旧实现只返回第一段、新实现返回整段）；`_xovi_construct` 找不到目标时也改为打日志。**两个扩展反序加载的场景仍没在真机上跑过**（xovi 的加载顺序依据也没核实），见白皮书 §05。

## 来源

- `scan`、`pattern`、`trampoline_aarch64` 三组：2026-09-11 从 `chinese-ime/langhook/src/` **拷贝**过来。原先两个扩展的 `Makefile` 直接路径引用那边，`chinese-ime/` 移出仓库后改成本目录下的独立副本（本项目的原则：新代码不对接已移走的旧路径，只拷贝）。
- `trampoline_patch`：不是从 langhook 拷的——那边从没拆出这一层。`hl_snap.c` 和 `hw_stroke.c` 曾各自从 langhook 的 `hook_init.c` 抄了一份，2026-09-15 代码审查发现两份逐字节相同，收进这里。

## 维护约定

从此和 langhook 原件各自独立维护，不会自动同步。这些都是很少变动的基础设施；真要同步改动，手动对拍。
