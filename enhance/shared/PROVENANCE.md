# shared —— 两个 xovi 扩展共用的 C 代码

`hl-snap` 和 `handwriting-stroke` 都靠"找到 xochitl 里的某个函数 → 改写它的开头跳到自己的代码"来工作，这套基础设施放在这里，两边的 `Makefile` 用 `LANGHOOK_SRC_DIR` 指向本目录。整个流程的图解见白皮书 §02（[`../docs/diagrams/xovi-hook-lifecycle.svg`](../docs/diagrams/xovi-hook-lifecycle.svg)）。

| 文件 | 做什么 |
|---|---|
| `scan.c/.h` | `cj_find_exec_module`：读 `/proc/self/maps`，找到 `/usr/bin/xochitl` 的可执行段 |
| `pattern.c/.h` | `cj_find_unique_pattern`：在段里搜特征码（目标函数开头的一串原始机器码），**必须恰好命中 1 处** |
| `trampoline_aarch64.c/.h` | `cj_build_far_jump`：拼一条跳到任意 64 位地址的 ARM64 远跳转（20 字节） |
| `trampoline_patch.c/.h` | `cj_patch_target`：mprotect 目标页 → 把开头 20 字节抄进新分配的"调用桩"并接上跳回原函数的远跳转 → 把目标开头改写成跳到 handler → 刷指令缓存。任一步失败返回 0、不改任何字节 |
| `tests/` | host 单测：`make test` |

## 已知限制：只扫第一个匹配的可执行段

`cj_find_exec_module` 找到**第一行**匹配 `/usr/bin/xochitl` 的可执行映射就返回。xochitl 刚启动时这是一整段，没问题；但每装一个 hook，`mprotect` 都会把目标所在的那一页从大段里切出来，之后"第一段"只到最低的那个已 patch 页之前为止。于是：**后装 hook 的扩展，只能找到地址低于已 patch 页的目标。**

2026-09-24 真机上两个扩展都装上了，是因为加载顺序恰好是 hw-stroke（目标 `0xf47530`、`0xf4c8d0`）先、hl-snap（目标 `0xf03670`，更低）后。顺序反过来时，按代码推断 hw-stroke 会在 `_xovi_construct` 里静默找不到目标（这条路径不打日志），网页徽章仍显示"已加载"。这个推断**没有真机复现过**；xovi 按什么顺序加载扩展也没核实。修法（扫描所有匹配段而不是第一段）需要改代码并重新真机验证，记在白皮书 §05。

## 来源

- `scan`、`pattern`、`trampoline_aarch64` 三组：2026-09-11 从 `chinese-ime/langhook/src/` **拷贝**过来。原先两个扩展的 `Makefile` 直接路径引用那边，`chinese-ime/` 移出仓库后改成本目录下的独立副本（本项目的原则：新代码不对接已移走的旧路径，只拷贝）。
- `trampoline_patch`：不是从 langhook 拷的——那边从没拆出这一层。`hl_snap.c` 和 `hw_stroke.c` 曾各自从 langhook 的 `hook_init.c` 抄了一份，2026-09-15 代码审查发现两份逐字节相同，收进这里。

## 维护约定

从此和 langhook 原件各自独立维护，不会自动同步。这些都是很少变动的基础设施；真要同步改动，手动对拍。
