# handwriting-stroke —— CJK 手写笔迹渲染优化（研究阶段，零实现代码）

**这里没有任何代码**，只是研究现状记录。别指望这个目录能编译/部署——真要有实现代码了再补 `src/`。

## 要做的是什么

设备手写笔锋按中文书写习惯（运笔粗细/顿挫）做渲染优化。**跟 `cardhw`（笔迹→文字的 AI 视觉识别，见 `knowledge/pkm-semantic/handwriting/`）完全无关**——那条线是"认出你写的字"，这条线是"你写字这个动作本身，笔画画出来好不好看"。用户 2026-09-09 明确澄清过这个区分（第一轮理解错了，以为是 `cardhw`）。

## 现状（2026-09-09 GUI 排查后）：架构完整摸清，还没有实现代码

三轮排查下来，从"完全不知道从哪下手"走到"精确到一行 C 代码、一个函数地址"。完整调用链、每一步的判断依据、真机验证边界，见 `../docs/reMarkable系统增强线白皮书.md` §03c——这份 README 只放结论速查表，过程/踩坑看白皮书。

### 完整调用链（真实类名/函数名/地址，全部来自 GUI 反编译交叉验证，不是猜测）

```
ShapesOverlay（继承 QQuickPaintedItem，QML 类型 "com.remarkable"，源码路径
  /home/runner/work/xochitl/xochitl/src/xofm/libs/sceneview/src/shapesoverlay.cpp）
  │
  ├─ qt_static_metacall = FUN_0087c290（moc 生成的方法分派表）
  │    ├─ index 4 = saveStroke（调试导出，写 "LINE" 记录到文件，非渲染路径）
  │    │    └─ FUN_00f36d40 → FUN_00f36be0（逐点写文件，意外反解出点结构，见下）
  │    │
  │    └─ index 1 = updateImage = FUN_008bbb80（真正的渲染入口，往内部 QImage 画）
  │         ├─ 自由手写笔迹分支：QPainterPath::toFillPolygon() 重采样多边形，
  │         │    每个重采样点调 FUN_00f33180 打包回同款 14 字节点结构（从最近原始点抄
  │         │    宽度/压感字段）
  │         └─ FUN_00f3e8a0（"New StrokeRenderer," 调试日志坐实类名）
  │              └─ FUN_00f3dcf0（StrokeRenderer 构造函数，2456 字节大对象，组合
  │                   ~15 个内联多态成员，每个成员构造时直接装自己的 vtable 指针）
  │                   ├─ strokev2::CoverageBuffer          （像素覆盖率累加）
  │                   ├─ strokev2::IVaryingsGenerator<Quill::Varying2D>  （插值生成器接口，
  │                   │    至少 3 个实现：VaryingGenerator_AA /
  │                   │    VaryingGenerator_WidthLength / VaryingGenerator_ThresholdAndWidth）
  │                   └─ Quill::LerpRaster<strokev2::Fill*>（FillPencil/FillBallpoint[AA]/
  │                        FillSolid_Opaque[_AA]/FillSolid_Composed[_AA]/FillMaskedEraser/
  │                        FillAnts/FillShaderAA，按笔型最终像素混合策略）
  │
  └─ paint(QPainter*) = FUN_008b0120（纯虚函数覆写，只是把内部 QImage 整张 blit 上屏，
       不含任何逐点渲染逻辑——真正画像素发生在 updateImage，不是这里）
```

### 宽度插值本体（找到的具体函数）

`strokev2::VaryingGenerator_WidthLength::generate()`，vtable 地址 `0x016d2370`，方法地址 **`0x00f401f0`**：

```c
float FUN_00f401f0(float param_1, long param_2)
{
  return param_1 * *(float *)(param_2 + 8);
}
```

`param_1` 是点结构里的原始宽度值，`*(param_2+8)` 是生成器对象自带的可配置缩放系数（选笔工具时设定）。**就是个线性缩放，没有额外的曲线/顿挫逻辑**——这意味着：
- 想改"运笔粗细按 CJK 书写习惯变化"，直接改这个函数最省事（换成非线性曲线/速度相关项），但这只是缩放这一层。
- 真正的"顿挫感"（毛笔提按变化）如果要更精细，得往上游查——点结构里"原始宽度值"本身怎么算出来的（下一节地址）没有继续深挖，`param_1` 目前默认等于点结构里 offset `0x8` 那个字段（见下）经 `×0.25` 解出的原始值，还是路径上又做过别的处理，没有验证。

### 14 字节点结构（`FUN_00f36be0` 反解，独立验证过）

| 偏移 | 类型 | 换算 | 字段 |
|---|---|---|---|
| `0x0` | float | 原样 | x 坐标 |
| `0x4` | float | 原样 | y 坐标 |
| `0x8` | u16 | `×0.25` | 宽度/速度（16 位定点，待进一步区分具体是哪个） |
| `0xA` | u16 | `×0.25` | 另一个同类字段 |
| `0xC` | u8 | `×2π/255` | 方向角（0-255 → 0~2π） |
| `0xD` | u8 | `÷255` | 压感（0-255 → 0~1，归一化） |

跟 `reading/device-rs/vendor/remarkable_lines/src/other/point.rs`、`notes/crates/rmv6/src/v6/scene_item/point.rs` 已经在解析的字段应该能对上，还没做过交叉对拍验证。

## 上一轮判断被推翻的地方（如实记录，别再犯）

第二轮（脚本反查 vtable 失败）曾经下结论"这几个 `Fill*` 类可能没有虚函数表"——**这个结论是错的**。这些类（`CoverageBuffer`/`VaryingGenerator_*` 等）确实有 vtable，只是它们是 `StrokeRenderer` 的**内联组合成员**（不是独立 `new` 出来的堆对象），vtable 指针是构造函数里按值直接塞进对象内存，没有任何地方存一个"指向这个成员 typeinfo 的裸指针"——这正是第二轮"反查谁指向 typeinfo"这条路天生走不通的原因，不是这些类没有虚函数。找到正确路径靠反过来走：QML 类型注册 → 构造函数工厂 → 构造函数体里的 vtable 安装序列 → 用 Ghidra `Search → Memory` 裸字节搜 typeinfo 地址，才在构造函数自己的字段里找到命中。

## 排查方法论（下次直接抄，别重新摸索）

1. **调试/异常字符串是金矿**：这次能挂上名字的每一步（`ShapesOverlay`/`saveStroke`/`updateImage`/`"New StrokeRenderer,"`/`shapesoverlay.cpp` 源码路径）都是从 Qt 编译进二进制的错误提示文案挖出来的，不是靠符号表（xochitl 符号被剥干净了，函数名全是 `FUN_xxxxx`，Symbol Tree → Functions 按名字搜是空的）。以后先扫这类字符串再决定往哪个函数深挖，效率比瞎猜地址高得多。
2. **多态类找 vtable 走"构造函数正向安装"，别走"typeinfo 反向反查"**：`ShapesOverlay` 这种独立堆对象/QML 注册类型，走"QML 工厂函数 → 构造函数开头 `str [x0]` 存 vtable 指针"最快；`StrokeRenderer` 内部那些内联组合成员，走"构造函数体里挨个 `param_1[N] = &PTR_FUN_xxx` 赋值"最快——两种都不需要反查 typeinfo。
3. **实在要反查裸指针，Ghidra `Show References To` 不够用就上 `Search → Memory`（按十六进制字节序列搜）**——前者依赖分析器已经把某处识别成指针类型，后者是纯字节扫描，跟本项目 `cj_find_metaobject` 手法等价，但 GUI 里能立刻看到命中上下文。
4. **Java Swing 在 Wayland（尤其平铺式合成器）下经典症状是整个窗口空白、菜单栏画不出来**——环境变量 `_JAVA_AWT_WM_NONREPARENTING=1` 能修，遇到"Ghidra 打开是白屏"先试这个，不用怀疑工程文件损坏。

## 下一步（没做，真要做需要真机验证）

1. **验证 `param_1` 的真实来源**：目前只确认它经点结构 `offset 0x8` 字段 `×0.25` 得到，中间有没有再被别的函数处理过没查——`FUN_00f33180`（重采样时打包点结构的那个函数）值得先看一遍。
2. **判断有没有稳定可 hook 的点**：`FUN_00f401f0` 本身只有一条指令，物理上极小，改起来风险也小，但要先确认这条路径每次画笔迹都会走到（还是只在特定条件下触发），以及改了会不会破坏别的依赖这个函数的调用方（`VaryingGenerator_WidthLength` 目前只在 `StrokeRenderer` 一处被组合，但要确认是不是唯一用途）。参照 `enhance/hl-snap/src/hl_snap.c` 现有 hook 的判据标准（特征码定位、fail-safe 开关）。
3. **真有 hook 点了，这个目录才升级成有 `src/` 的实现**——大概率也会照 `hl-snap/` 的样子做成独立最小 xovi 扩展，不塞进别的项目。**改设备渲染路径风险级别高，动手前必须先只读验证（打日志确认真实取值），不能直接写内存**，按项目工程纪律"先离线摸清楚再写"执行。

（内容跟这份文档同步，那边是给未来会话快速定位用的索引指针，细节以这份文档为准）。
