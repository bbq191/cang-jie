# appload 3.28 兼容补丁——来源与许可留痕

`appload-qmd-v0.5.3.orig.qmd`、`appload-qmd-3.28.qmd`、`appload_patch_328.py` 三个文件
2026-09-16 从"本机 `oldbak/xovi-extensions/reading-qol/tools/` 里有、但没回到 git 版本
控制"这条已知缺口里正式回收——`oldbak/` 在写这份文档的这台机器上已经不存在了（见项目记忆
`appload-smoketest-and-device-runtime`），这次不是照抄那份旧文件，是重新从上游 GitHub
按 commit 精确取源、重新写的工具（逻辑/思路与旧版一致：等长字节回填一个编译好的 `.so` 里
内嵌的 NUL 结尾 C 字符串，不重新编译整个 `appload.so`）。

## 上游来源

`asivery/rm-appload`（reMarkable 第三方 App 加载框架，https://github.com/asivery/rm-appload ，
`LICENSE = GPL-3.0`，全文见同目录 `appload-qmd-LICENSE`——按 SPDX `GPL-3.0` 标签取的标准文本，
2026-09-16 从上游 `master` 分支原样下载，未改动）：

- **`appload-qmd-v0.5.3.orig.qmd`**（7895 字节）——`xovi/template/appload.qmd` 在 tag `v0.5.3`
  （2026-05-21 发布，https://github.com/asivery/rm-appload/releases/tag/v0.5.3 ）的原始内容。
  这是当前 `vellum add appload` 装的官方发行版 `appload.so` 里**实际内嵌**的那份 qmd 原文——
  patch 工具靠在目标 `.so` 二进制里搜这段字节序列来定位注入点（同 `enhance/shared/pattern.c`
  "唯一命中才动手"的安全原则，见下）。
- **`appload-qmd-3.28.qmd`**（7427 字节）——同一份文件在 PR #59（`rmitchellscott/3.28` →
  `master`，https://github.com/asivery/rm-appload/pull/59 ，标题"support for 3.28"）合并
  提交 `3b42440e369a82535fb93df4a9155de9199cf487` 那一刻的内容——**不是**当前 `master` HEAD
  的最新内容（HEAD 在 2026-09-16 已经是 9496 字节，比原始 7895 字节还长，装不进等长回填的
  空间；用 PR #59 合并时那一刻的版本，是因为
  记录过"新 qmd 7427B → 等长回填 + NUL 补齐"这个精确长度，确认这就是当初真机验证过、且
  7427 ≤ 7895 装得下的那一版）。**PR #59 本身已在 2026-09-07 合并进上游 `master`**——比
  项目记忆/`deploy-sidebar-entry.sh` 注释里"上游 PR #59（未合并）"的说法要新，但上游至今
  （2026-09-16）没有发布包含这个修复的新 tag（最新仍是 `v0.5.3`），`vellum add appload`
  装的官方发行版因此仍然没有这个修复——等长回填补丁依然是当前唯一可用的解法，直到上游发布
  新版本。

## 为什么是"等长字节回填"而不是重新编译 appload.so

`rm-appload` 是 Qt/QML 项目（`appload.pro`），从源码重新交叉编译整个 `.so` 需要完整 Qt6
aarch64 交叉工具链——超出这个仓库现有的构建依赖范围，也偏离了这个补丁工具"只改一段被编译
进二进制的静态字符串数据、不碰任何代码逻辑"的最小改动原则。qmd 在 `.so` 里是一段普通的
NUL 结尾 C 字符串常量（编译器不会对它做任何代码生成，纯数据），只要新内容不超过原长度，
覆盖这段字节、剩余位置补 `\x00` 补齐终止符，就能在不重新链接、不改变文件里任何其它偏移量的
前提下完成替换。

## 安全性

`appload_patch_328.py` 的定位逻辑照搬 `enhance/shared/pattern.c` 的唯一性校验原则："在目标
`.so` 里搜 `appload-qmd-v0.5.3.orig.qmd` 这段字节，命中且只命中一次才动手；0 次或 >1 次都
视为失败、不碰文件"——理由跟 `pattern.c` 头注一致：宁可放弃也不要在不确定的位置上写。

## 已知局限（如实记录，未真机验证）

这台写代码的机器上没有真机可连（`ping 10.11.99.1` 不通），也没有一份真实的 `appload.so`
二进制可供试跑——`packaging/appload_patch_328.py` 的字节替换/填充/唯一性校验逻辑只用构造出
的假二进制片段跑过 host 单测（见 `packaging/tests/test_appload_patch_328.py`），**没有验证
过对真实 `appload.so` 文件能不能找到且只找到一次这段 qmd 字节**——真机验证需要用户在装了
`vellum add appload`（v0.5.3）的设备上跑一遍，见 `packaging/README.md`「验证现状」。
