# handwriting-stroke —— CJK 手写笔迹渲染优化（研究阶段，零实现代码）

**这里没有任何代码**，只是研究现状记录。别指望这个目录能编译/部署——真要有实现代码了再补 `src/`。

## 要做的是什么

设备手写笔锋按中文书写习惯（运笔粗细/顿挫）做渲染优化。**跟 `cardhw`（笔迹→文字的 AI 视觉识别，见 `knowledge/pkm-semantic/handwriting/`）完全无关**——那条线是"认出你写的字"，这条线是"你写字这个动作本身，笔画画出来好不好看"。用户 2026-09-09 明确澄清过这个区分（第一轮理解错了，以为是 `cardhw`）。

## 现状：全新功能，零地基

全仓库关键词搜索（笔锋、笔画、书写风格、stroke、taper、pen tip、ink width、pressure、brush、calligraphy、手写渲染等）确认：**没有配置键、没有 hook、没有反编译记录**。唯一沾边的"够不够到 xochitl 原生渲染层"先例——笔记页想在背景滤镜层面接近同一层——是**判死**的（C++ `SceneView` tile 增量渲染够不到）。

## 第一手线索（2026-09-09，`strings`/`c++filt`，没上 Ghidra）

真机 `xochitl`（3.28.0.172）二进制里有一整套 C++ RTTI mangled 名字，命名空间 **`Quill::strokev2`**——按笔型分光栅化策略类：

- `FillPencil`（铅笔）
- `FillBallpoint` / `FillBallpointAA`（圆珠笔，AA=抗锯齿）
- `FillSolid_Opaque_AA` / `FillSolid_Composed_AA`
- `FillMaskedEraser`（橡皮擦）
- `FillAnts`、`FillShaderAA`

外层包一层 **`LerpRaster`**（linear-interpolation raster，插值光栅化——名字直接暗示"沿路径插值"）或 `MonoRaster`（单色光栅化，无插值，大概率对应橡皮擦这类不需要渐变的场景）。

这些字符串是 C++ RTTI typeinfo 名字（`dynamic_cast`/异常机制需要，`strip` 过的二进制通常还留着），**只证明这些类存在、有名字，没有定位到它们方法体的实际代码地址、没有验证能不能 hook**。

**判断**：这套结构看起来正是"笔画宽度/透明度沿路径插值渲染"的实现层，是运笔粗细该扎进去的地方，比笔记页背景滤镜那次判死的层级更底层、也更直接对应这个诉求——但只是判断，没有证据支撑到能动手写 hook 的程度。

## 下一步（没做，需要单独立项）

1. 装 Ghidra（已装，`~/.local/share/ghidra`，见 `ghidra-project-328/README.md`）+ 把 3.28.0.172 的 `xochitl` 导进新建的 Ghidra 项目（复用 `ghidra-project-328/`，别跟 `ghidra-project/` 那个 .169 项目混）。
2. 靠这些 RTTI typeinfo 字符串反查 vtable/函数体（`ghidra-project/scripts/DumpVtable*.java`/`FindXrefs*.java` 这套脚本本来就是干这个的，照搬套路，落 `ghidra-project-328/scripts/`）。
3. 找到 `LerpRaster` 具体在哪算"宽度沿路径插值"、参数怎么传进来（大概率来自 `.rm` 每个采样点自带的 `pressure`/`width` 字段——`reading/device-rs/vendor/remarkable_lines/src/other/point.rs`/`notes/crates/rmv6/src/v6/scene_item/point.rs` 已经在解析这些字段，只是目前只读不用来改渲染）。
4. 判断有没有稳定可 hook 的点（前 N 字节栈操作可安全 patch 的那种，参照 `enhance/hl-snap/src/hl_snap.c` 现有 hook 的判据）——这一步之前完全没做过，不能假设一定能找到。
5. 真有 hook 点了，这个目录才升级成有 `src/` 的实现——大概率也会照 `hl-snap/` 的样子做成独立最小 xovi 扩展，不塞进别的项目。

（内容跟这份文档同步，那边是给未来会话快速定位用的索引指针，细节以这份文档为准）。
