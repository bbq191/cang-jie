# shared —— 来源与维护约定

`scan.c`/`scan.h`/`pattern.c`/`pattern.h`/`trampoline_aarch64.c`/`trampoline_aarch64.h`
是特征码扫描 + ARM64 远跳转 trampoline 三个纯工具文件，2026-09-11 **剥离移植**
（copy，不是路径依赖）自 `chinese-ime/langhook/src/`。

`trampoline_patch.c`/`trampoline_patch.h`（通用 trampoline 安装：`cj_patch_target`，
mmap+mprotect+调用 `cj_build_far_jump` 拼跳转指令+`__builtin___clear_cache`）不是从
`chinese-ime/langhook/` 移植的——那边的 `hook_init.c` 从没拆出过这一层，是
`enhance/hl-snap/src/hl_snap.c`/`enhance/handwriting-stroke/src/hw_stroke.c` 两边各自
"逐字节抄自 hook_init.c"独立复制的产物，2026-09-15 全量代码审查发现两份代码逐字节相同后
收进这里，来源是这两个文件各自当时的副本（内容一致，选哪份复制过来都一样）。

## 为什么从路径引用改成拷贝

原先 `enhance/hl-snap/`、`enhance/handwriting-stroke/` 两个 xovi 扩展的 Makefile 用
`LANGHOOK_SRC_DIR = ../../chinese-ime/langhook/src` **路径引用**（不复制）这三个文件——
用意是单一事实源，改一处两边都同步。2026-09-11 全仓库大规模归档整理，`chinese-ime/` 挪出了
仓库（本身仍是现役——`cangjie-langhook.so` 还在设备上跑，只是不再是仓库里"随时会改、需要
联动"的活跃开发目标），继续路径引用会让 `enhance/` 这条线的构建绑死在一个仓库外的目录上，
不符合本项目"新代码不对接旧路径、只许剥离移植"的工程原则（同 `notes/crates/rmv6` 的既有先例）。
故改成本目录下的独立副本。

## 维护后果

`chinese-ime/langhook/src/` 那份原件如果再改，**不会自动同步到这里**——两边从此各自独立
维护。目前两份逻辑均为特征码扫描/trampoline 这类稳定基础设施，预期变动很少；真要同步改动，
两边手动对拍。

## 使用方

`enhance/hl-snap/Makefile`、`enhance/handwriting-stroke/Makefile` 的 `LANGHOOK_SRC_DIR`
指向这里。
