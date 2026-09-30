# handwriting-stroke —— CJK 手写笔迹渲染优化

**一句话**：按**笔尖角度**和**运笔速度**调整手写笔画的粗细，让中文手写更有粗细顿挫。**默认关**，网页「管理 → 实验室」打开。

> **和 AI 手写识别无关**：这里管的是"你写的字画出来好不好看"，不是"认出你写了什么字"。

它是一个独立的 xovi 扩展（`hw-stroke.so`）：在 xochitl 画笔迹的"变宽几何"函数入口挂 hook，在宽度交给后续渲染之前按公式缩放一下。xovi 扩展怎么加载、hook 怎么装，见白皮书 §02 的流程图。

## 现状

| 项 | 状态 |
|---|---|
| 书法笔 | ✅ 真机通（hook `FUN_00f47530`）。书法笔原生就有方向粗细（平头笔尖，竖线是横线 2~4 倍粗），不是本扩展加的 |
| 钢笔 / 铅笔 / 马克笔等日常工具 | ✅ 真机通（hook `FUN_00f4c8d0`，一次实测命中 10033 次，是前者近 5 倍） |
| 最常用的钢笔/铅笔量级（xochitl 内部笔型标签 `bVar16<4`） | ❌ 还摸不到：走虚函数动态分发，目标没确认 |
| 真实压感 | ❌ 放弃，改用运笔速度代替（原因见白皮书 §03f） |
| 参数 | 一轮真机数据校准的起点，用户反馈"看上去还行"，未精细打磨 |
| 负载 | 2026-09-24 起每笔起点读一次配置，逐点日志默认关（白皮书 §03g） |

原理、逆向过程、渲染链、为什么这样设计：[白皮书 §03c–§03g](../docs/reMarkable系统增强线白皮书.md)；未完成项：白皮书 §05。本文只讲怎么用、怎么部署。

## 开关与参数

**网页开关**：「管理 → 实验室」→「CJK 手写笔迹优化」。它没有单独的布尔字段：开 = 把 `hwStrokeNibMinRatio` 和 `hwStrokeSpeedMinRatio` 都写成 `0.6`，关 = 都写 `1.0`；网页把 `hwStrokeNibMinRatio < 1.0` 显示为"已开"（`gateway/src/enhance/qol.rs`）。不用重新部署 `.so`，**从下一笔生效**（扩展在每一笔起点读一次配置，另每 1024 个点兜底读一次）。

开关旁的「已加载 / 未加载」徽章表示 `hw-stroke.so` 是否真的在 xochitl 进程里。

**精调参数**只能手改 `~/.local/share/cangjie-ime/reading-qol.json`（`cangjie-ime` 是历史目录名）。缺失、解析失败或越界的值都会被忽略、保持当前值：

| 键 | 默认 | 含义 |
|---|---|---|
| `hwStrokeWidthFactor` | 1.0 | 整体宽度缩放（0~10；1.0 = 不变） |
| `hwStrokeNibAngleDeg` | 45 | 笔尖角度（度） |
| `hwStrokeNibMinRatio` | 1.0 | 笔尖角度效果强度：1.0 = 关，越小横竖粗细差越明显 |
| `hwStrokeSpeedMinRatio` | 1.0 | 运笔速度（提按）效果强度：1.0 = 关 |
| `hwStrokeNibWidthLow` / `High` | 6 / 20 | 两个效果共用：基础宽度低于 Low 时效果趋近关闭，高于 High 时满强度（细笔不抖、粗笔才有顿挫） |
| `hwStrokeSpeedLenLow` / `High` | 1 / 8 | 相邻两点距离阈值（像素/采样点）：距离短（慢、顿笔）→ 粗，距离长（快、带过）→ 细 |
| `hwStrokeDebug` | false | 调试：开了才逐点写 `[hw-stroke:…]` 日志、才装只读的分派诊断 hook（后者只在加载时看这个键，改了要整机重启） |

两个效果的公式（`min_ratio = 1.0` 时恒等于 1，即关闭）：

```
笔尖角度：宽度 ×= min_ratio + (1-min_ratio) × |sin(运笔方向角 − 笔尖角度)|
运笔速度：宽度 ×= min_ratio + (1-min_ratio) × (1 − clamp((len−len_low)/(len_high−len_low), 0, 1))
```

> 白皮书 §03f 记过一次真机校准时写进配置文件的值（宽度阈值 5/20、速度阈值 1~10、强度 0.6）；上表是**代码默认值**，以 `src/hw_stroke.c` 为准。

## 装了哪些 hook

| 函数 | 作用 | 何时装 |
|---|---|---|
| `FUN_00f47530` | 变宽几何生成器（书法笔），改宽度 | 总是。它的特征码也是 `_xovi_shouldLoad` 的判据：找不到就整个扩展不加载 |
| `FUN_00f4c8d0` | 第二个几何生成器（钢笔/铅笔/马克笔等），改宽度 | 总是；找不到只跳过它自己 |
| `FUN_00f3f9d0` | 逐点渲染分派，**只读诊断**，不改任何值 | 仅 `hwStrokeDebug=true` 时 |

## 构建与部署

```sh
make aarch64                                                   # 产物 hw-stroke.so（已提交进仓库）；改了 hw-stroke.xovi 才要 make glue XOVI_DIR=<xovi clone>
                                                               # 09-25 起带 -ffile-prefix-map，调试信息不含本机路径（09-25 已部署真机，三个 hook 装上）
                                                               # make clean 只删 hw-stroke.so，不删已提交的 xovi_glue.{c,h}（09-30 起）
cd ../../packaging && sh deploy-handwriting-stroke.sh <host>     # host 侧一键：构建 → 推送 → 设备端安装
```

设备端 `deploy/install.sh [--no-restart]` 与 hl-snap 共用同一套流程（`packaging/xovi-ext-install.sh`），同目录需要 `xovi-ext-install.sh` 与 `devlib.sh`；备份、换文件（运行中正在用就放进待换入区）、生效方式（一律整机重启）都和 [hl-snap README「部署」](../hl-snap/README.md#部署) 一样。装到 `extensions.d/hw-stroke.so`。公共扫描/trampoline 代码在 [`../shared/`](../shared/PROVENANCE.md)。

**验证装上了**：`journalctl -u xochitl | grep hw-stroke` 应有 `变宽几何 hook 安装完成 @ 0xf47530` 与 `第二几何 hook 安装完成 @ 0xf4c8d0` 两行；若是 `_xovi_construct: … hook 未安装`，说明 `.so` 进了进程但 hook 没装上（2026-09-24 起才打这行）。

**构建时注意 GLIBC 版本**：交叉工具链的 glibc 比设备新得多，用到 `atan2f`/`sqrtf` 这类函数会链上设备没有的符号版本，整个 `.so` 静默加载失败。改完用 `aarch64-linux-gnu-objdump -T hw-stroke.so | grep GLIBC_` 确认最高仍是 `GLIBC_2.17`（详见白皮书 §04）。
