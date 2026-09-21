# handwriting-stroke —— CJK 手写笔迹渲染优化

独立最小 xovi 扩展（`hw-stroke.so`）：在 xochitl 画笔迹的"变宽几何"函数上 hook，按**笔尖角度**和**运笔速度**调整笔画宽度，让手写更接近中文书写的粗细顿挫。默认**全关**（不改变任何行为），网页「管理 → 实验室」打开。

> **跟 AI 手写识别无关**：这条线是"你写字这个动作画出来好不好看"，不是"认出你写的字"。

**状态**：两个 hook 目标已真机验证（2026-09-10）——`FUN_00f47530`（书法笔专属）与 `FUN_00f4c8d0`（钢笔/铅笔/马克笔等日常工具，覆盖面是前者近 5 倍）。用户定性反馈"看上去还行"，参数是按当时真机数据校准的起点，未精细打磨。**最常用的钢笔/铅笔量级工具（`bVar16<4` 分支）仍然摸不到**，是当前最大缺口，见「未完成」。推导过程与踩坑见 [`../docs/reMarkable系统增强线白皮书.md`](../docs/reMarkable系统增强线白皮书.md) §03c–§03f、§04；本文只放结论与速查。

## 怎么用

**构建与部署**（仿 [`../hl-snap/`](../hl-snap/README.md)）：

```sh
make aarch64 XOVI_DIR=<asivery/xovi clone 路径>       # 产物 hw-stroke.so（已提交进仓库）
cd ../../packaging && sh deploy-handwriting-stroke.sh <host>   # host 侧一键：构建 → 推送 → 设备端安装
```

设备端 `deploy/install.sh [--no-restart]` 需要同目录的 `xovi-ext-install.sh` 与 `devlib.sh`（由部署脚本一起推送，只拷单个 `install.sh` 不够）；行为、备份与重启判定同 `hl-snap` README「部署」。装到 `extensions.d/hw-stroke.so`。通用 trampoline 安装代码（`cj_patch_target`）在 [`../shared/`](../shared/PROVENANCE.md)，两个 xovi 扩展共用。

**开关**：网页「管理 → 实验室」的"CJK 手写笔迹优化"是一个纯网页层派生开关——开 = 把 `hwStrokeNibMinRatio` 与 `hwStrokeSpeedMinRatio` 都写 `0.6`，关 = 都写 `1.0`（见 `gateway/src/enhance/qol.rs`）。不用重新部署 `.so`：扩展每次落笔时现读配置。角度/宽度/速度阈值这几个精调字段留给手改 `reading-qol.json`。

**`~/.local/share/cangjie-ime/reading-qol.json` 里的键**（`cangjie-ime` 是历史目录名；缺失/解析失败/越界的值都保持当前值，fail-safe）：

| 键 | 默认 | 含义 |
|---|---|---|
| `hwStrokeWidthFactor` | 1.0 | 整体宽度缩放（0~10；1.0=不变） |
| `hwStrokeNibAngleDeg` | 45 | 笔尖角度（度） |
| `hwStrokeNibMinRatio` | 1.0 | 笔尖角度效果强度；1.0=关闭，越小方向差越明显 |
| `hwStrokeNibWidthLow` / `hwStrokeNibWidthHigh` | 6 / 20 | 宽度渐变阈值（两个效果共用）：基础宽度低于 Low 效果趋近关闭，高于 High 满强度 |
| `hwStrokeSpeedMinRatio` | 1.0 | 提按（运笔速度代理）效果强度；1.0=关闭 |
| `hwStrokeSpeedLenLow` / `hwStrokeSpeedLenHigh` | 1 / 8 | 相邻两点距离阈值（像素/采样点）：短→粗（慢/顿笔），长→细（快/带过） |

> 白皮书 §03f 记过一次真机校准值（宽度阈值 5/20、速度阈值 1~10、强度 0.6），那是当时写进配置文件的值；上表是**代码里的默认值**，以代码常量（`src/hw_stroke.c`）为准。

## 三个 hook

`hw-stroke.so` 装三个 hook，各自独立 install，找不到目标只跳过自己、不拖累其它；`_xovi_shouldLoad` 只看主 hook 的特征码是否唯一命中，否则拒载（裸启原生）。

| 函数 | 作用 | 备注 |
|---|---|---|
| `FUN_00f47530` | 变宽几何生成器（书法笔专属），改宽度 | 主 hook；书法笔原生就带方向粗细（平头笔尖的真实物理效果，竖线是横线 2~4 倍粗），不是本扩展引入的 |
| `FUN_00f4c8d0` | 第二个几何生成器（`bVar16==5/6`：钢笔/铅笔/马克笔等） | 与主 hook 同一调用约定 `(float x, float y, ctx)`，宽度在 `ctx+4`；一次真机实测命中 10033 次（主 hook 2117 次） |
| `FUN_00f3f9d0` | 逐点渲染分派 | **只读诊断**，留着排查用，不改任何值 |

## 两个效果

```
笔尖角度：宽度 ×= min_ratio + (1-min_ratio) × |sin(运笔方向角 − 笔尖角度)|      （Illustrator/Inkscape 书法笔刷同款公式）
提按速度：宽度 ×= min_ratio + (1-min_ratio) × (1 − clamp((len−len_low)/(len_high−len_low), 0, 1))
```

`min_ratio=1.0` 时两个公式恒等于 1，即关闭。两个效果的强度都随**基础宽度**自动挂钩（细笔画趋近关闭、粗笔画满强度），不按笔型分支——真机发现钢笔与毛笔走的是同一条分支同一套公式，只是基础宽度不同。运笔方向用 hook 内部维护的"上一点坐标"（`ctx+0x48/0x4c`）现算，不依赖点结构里的方向字节。

## 笔画怎么变成像素：渲染链

![笔画渲染链与 hook 位置](../docs/diagrams/handwriting-render-chain.svg)

调用链（真实类名/函数名来自 Qt 编译进二进制的调试字符串与 Ghidra 反编译交叉验证；xochitl 符号被剥干净，函数名都是 `FUN_` 加地址）：

- `ShapesOverlay`（继承 `QQuickPaintedItem`，QML 类型在 `com.remarkable`，源码路径字符串 `.../src/xofm/libs/sceneview/src/shapesoverlay.cpp`）的 `updateImage`（`FUN_008bbb80`）才是真正画像素的入口；`paint()` 只是把内部 `QImage` 整张 blit 上屏。
- `updateImage` 里自由手写笔迹走 `QPainterPath::toFillPolygon()` 重采样，每个点重新打包成 14 字节点结构，再经 `StrokeRenderer`（构造函数 `FUN_00f3dcf0`，把 `CoverageBuffer` / `IVaryingsGenerator` / `LerpRaster<Fill*>` 十几个多态成员**内联组合**进自己）渲染。

**14 字节点结构**（`FUN_00f36be0` 反解，独立验证过）：

| 偏移 | 类型 | 换算 | 字段 |
|---|---|---|---|
| `0x0` / `0x4` | float | 原样 | x / y 坐标 |
| `0x8` / `0xA` | u16 | ×0.25 | 两个同类宽度/速度定点字段 |
| `0xC` | u8 | ×2π/255 | 方向角 |
| `0xD` | u8 | ÷255 | 压感（0~1） |

`.rm` 文件里解析的点字段应与此对得上，但**没做过交叉对拍**。

## 为什么没用真实压感

点结构里的压感字节是真实数据（正常书写力度下很快饱和到 255，专测轻重才能拿到 3~255 的宽分布），但**跨函数传给宽度 hook 的方案被真机数据证伪**：`FUN_00f3f9d0` 有 6 个调用点，宽度 hook 的绝大多数调用（一次采样 686 次 vs 仅 2/3449 落在锁定分支）根本不经过被锁定的那份，很可能"实时预览"和"提交进笔记本"走不同路径。所以改用完全在 hook 内部就能算的**运笔速度代理**。

## 未完成（没做，真要做需要更多真机验证/静态分析）

1. **`bVar16<4` 分支（最常用的钢笔/铅笔量级工具）仍摸不到**：它是虚函数动态分发（`(**(code**)(*plVar6+0x10))(x,y,width,plVar6,...)`，宽度直接当第 3 个参数传），运行时多态目标没确认。下一步思路：扩展诊断 hook 读 `*(void**)(*plVar6+0x10)` 打印函数地址，再拿地址反编译确认签名。
2. **`FUN_00f4f430`（`bVar16==3`）**：签名与宽度约定跟已验证的两个一致，但前 20 字节第 3 条指令是条件分支（`cbz`，PC 相对寻址），被 `memcpy` 进 call-through stub 后分支目标会算错，**不能安全 patch**；要用需要指令级搬移/重定位，或往后找更靠后的安全 patch 点。
3. 像素消费者虚函数（`vtable+0x10`）的真实目标、smoothstep 缓动曲线（存进 `plVar6+0xe`）的下游用途：静态分析到边界，需要动态分析。
4. 参数（角度 45°、宽度阈值、速度阈值、强度 0.6）只是一轮真机数据的校准起点，"看上去还行"到"效果好"之间还有调参空间。
5. 想换回真实压感，得先搞清 `FUN_00f47530` / `FUN_00f4c8d0` 各有哪些调用路径、`FUN_00f3f9d0` 这份实例覆盖了哪些。

## 排查方法论（下次直接抄）

1. **调试/异常字符串是金矿**：stripped 二进制里能挂上名字的每一步（`ShapesOverlay` / `saveStroke` / `updateImage` / `"New StrokeRenderer,"` / 源码路径）都来自 Qt 编译进去的错误提示文案，比符号表可靠，先扫它们再决定深挖哪个函数。
2. **找 vtable 走"构造函数正向安装"，别走"typeinfo 反向反查"**：内联组合成员的 vtable 指针是构造函数里按值写入的，没有任何地方存指向其 typeinfo 的裸指针，反查天生走不通（白皮书 §04 有这个教训的来龙去脉）。
3. Ghidra `Show References To` 依赖分析器已识别的 xref；找不到时退回 `Search → Memory`（十六进制字节序列搜索）。
4. **反编译要交叉核实原始汇编**：`FUN_00f401f0`（`VaryingGenerator_WidthLength` 的业务方法）曾被反编译显示成"1 个 float 入参、一行线性缩放"，原始汇编却是 3 个 float 入参 + 1 个对象指针、产出一对 float；而且它在整个二进制里**零真实调用者**——当初据此写的"最具体的候选改动点"结论已撤回，改用确认在真实调用链上的 `FUN_00f47530`。
5. 一旦有了具体地址，headless 脚本（[`../../defw/scripts/`](../../defw/README.md)）能接手反编译/查 xref/按字节搜内存，不必再靠 GUI 截图；前提是 GUI 已关闭工程（否则抢 `.lock`）。
6. Java Swing 在 Wayland 平铺式合成器下 Ghidra 窗口整片空白：设 `_JAVA_AWT_WM_NONREPARENTING=1`。
