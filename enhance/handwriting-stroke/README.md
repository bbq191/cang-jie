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

## 第二轮尝试：反查 vtable，没成功（2026-09-09，`ghidra-project-328/scripts/FindQuillStrokeRTTI.java`）

装了 Ghidra、真机拉了 3.28.0.172 的 `xochitl`、跑了 headless 完整分析（`ghidra-project-328/xochitl_328_analysis.gpr`，238 秒分析完成）。用 `cj_find_metaobject` 同款手法（直接在内存里搜"字面等于某地址的 8 字节指针值"，不依赖 Ghidra 自动 xref）反查：

- 四个 RTTI name 字符串（`FillPencil`/`FillBallpoint`/`FillMaskedEraser`/`FillSolid_Opaque` 四个模板实例化）**各自唯一命中一次**，地址在 `0x16d0818`~`0x16d0d80` 一带的 `.rodata`。
- 反查"谁指向这段字符串" → 各自唯一命中一次，候选 typeinfo 对象在 `0x16d0850`~`0x16d0db8` 一带。
- 再反查"谁指向这个 typeinfo 对象" → 各自也唯一命中一次，本以为是 vtable 起始，**结果 dump 出来的头几个"槽位"解出来是 ASCII 文本**（比如 `0x656b6f727473384e` 解出来是字面 `"N8stroke"`）——不是函数指针，是**另一段无关的字符串**，只是恰好挨在那个指针字段后面。

**结论**：`.data.rel.ro`/`.rodata` 里这些 typeinfo 相关结构挨得很紧，"指针字段后面紧跟的就是 vtable"这个假设在这里不成立——命中的那个指针字段更可能是别的结构（比如某个更外层类型 `__si_class_type_info`/`__vmi_class_type_info` 的 `base_type` 字段，指向这几个模板类当基类）里的一环，不是这几个类自己的 vtable。**没有找到这几个类的虚函数表，也就还没确认它们是不是走虚函数分发的**——甚至不能排除它们根本没有虚函数（RTTI/typeid 不一定要求多态，模板类被拿去 `typeid()` 比较、丢异常、塞进类型擦除容器都会生成 typeinfo，不一定意味着有 vtable）。

**这条自动化脚本路子先停在这**：继续往下靠"猜偏移+批量脚本扫"效率已经不高，需要换成 Ghidra GUI 交互式排查（打开 `ghidra-project-328/xochitl_328_analysis.gpr`，人工点开 `0x16d1af8`/`0x16d1ba0` 这几个候选地址周围的内存布局，看清楚它们实际挂在哪个更大的结构里，再决定这些类到底怎么被调用——是虚函数分发、还是直接模板实例化的静态调用，两种情况找"哪里画笔画宽度"的思路完全不同）。

## 下一步（没做，需要人工交互，不是脚本能继续扫出来的）

1. **GUI 打开** `ghidra-project-328/xochitl_328_analysis.gpr`（`ghidraRun` 或桌面启动器），人工浏览 `0x16d0800`~`0x16d1c00` 这段 `.data.rel.ro`，把这几个 typeinfo 相关结构的真实布局看清楚（Ghidra GUI 能交互式创建/调整数据类型定义，脚本猜偏移做不到这个）。
2. 确认这几个类到底有没有虚函数——如果有，vtable 在哪；如果没有，改成搜索"谁调用了这几个类的构造函数/方法"（直接函数调用，不经过虚表，得靠反编译读调用点）。
3. 找到 `LerpRaster` 具体在哪算"宽度沿路径插值"、参数怎么传进来（大概率来自 `.rm` 每个采样点自带的 `pressure`/`width` 字段——`reading/device-rs/vendor/remarkable_lines/src/other/point.rs`/`notes/crates/rmv6/src/v6/scene_item/point.rs` 已经在解析这些字段，只是目前只读不用来改渲染）。
4. 判断有没有稳定可 hook 的点（前 N 字节栈操作可安全 patch 的那种，参照 `enhance/hl-snap/src/hl_snap.c` 现有 hook 的判据）——这一步之前完全没做过，不能假设一定能找到。
5. 真有 hook 点了，这个目录才升级成有 `src/` 的实现——大概率也会照 `hl-snap/` 的样子做成独立最小 xovi 扩展，不塞进别的项目。

（内容跟这份文档同步，那边是给未来会话快速定位用的索引指针，细节以这份文档为准）。
