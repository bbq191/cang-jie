# reMarkable 系统增强线（enhance）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。**§ 编号保持不变**（别处有交叉引用）；读现状先看 §00b，待办看 §05，踩坑看 §04。

## 读这篇你能得到什么

| 你想知道 | 读 |
|---|---|
| 这条线现在有什么、各自什么状态 | §00b（含总览图） |
| 为什么单点增强要单独成线、不塞进老项目 | §00、§01 |
| 网页开关怎么和这些工具对接 | §02 |
| 荧光笔"划哪吸哪"是怎么修的、怎么排查出来的 | §03a |
| 手写笔锋渲染：xochitl 怎么画笔画、我们 hook 在哪、做过什么又撤回了什么 | §03c（反解）→ §03e（第一版）→ §03f（速度代理 + 第二个 hook） |
| Ghidra 怎么装 | §03d（指向 [`../../defw/README.md`](../../defw/README.md)） |
| 逆向/hook 时踩过哪些坑 | §04 |
| 还有什么没做 | §05 |

> 这本白皮书写的是"系统增强线"（`enhance/`，2026-09-09 起的顶层项目线）。旧 `xovi-extensions/` 里那套设备端 QML/UI 增强（reading-qol / font-menu）早已搬出仓库，跟这里没有从属关系，别混。

## 00｜定位与原则

**起因**：2026-09-09 用户反馈"划线没有按 CJK 精确吸附"，排查发现根因是完整中文化扩展 `cangjie-langhook.so` 整个从设备上消失了（见 §03a）。第一版修复直接在老项目 `chinese-ime/langhook` 里加了个运行期开关（`CANGJIE_IME_HOOKS=0`）。用户用三轮话把方向纠正清楚：

1. "cjk 精确吸附，不动老项目，在新路径下工作，类似 shelf/"——第一版改动全部撤回，另起炉灶。
2. 不要只是"挪个单功能目录"，要建一个能装下这一类"跨块单点增强工具"的顶层项目线（"不仅有精准吸附，还有笔锋和电池刺客等等"）。
3. 定名 `enhance/`（系统增强线），跟 `shelf/`、`notes/` 并列。

**三条硬原则**（在纠正过程中形成，不是事先定好的）：

1. **单点增强工具不塞进已有大项目**——哪怕改动只有几十行，只要功能能独立部署、独立生命周期，就独立成目录。
2. **复用不复制**（后来因老项目搬出仓库，改为"剥离移植成独立副本、不再对接旧路径"，见 `../shared/PROVENANCE.md`）。
3. **一份产物两种用法，别搞两份构建**——需要功能子集时优先运行期开关，而不是编译期 `#ifdef`/构建变体（后者会让老项目里一批函数在特殊构建下变成死代码、还要记两份产物的差异）。最终体现为：`hl-snap/` 是**全新独立源码**（连 `_xovi_shouldLoad` 判据都不同：用自己的目标特征码，不借用 `setLanguageCode`）。

## 00b｜现状总览

![enhance 的五个工具怎么接到设备上](diagrams/enhance-overview.svg)

| 组件 | 类型 | 状态 |
|---|---|---|
| `hl-snap/` | xovi 扩展（C，ARM64） | ✅ 真机通。真机 journal 确认 hook 装上（`FUN_00f05ad0@0xf03670`），健康检查通过（`is-active` / `NRestarts` / `MainPID`，含延迟复查）（§03a） |
| `handwriting-stroke/` | xovi 扩展（C，ARM64） | ✅ 两个 hook 目标真机通：`FUN_00f47530`（书法笔专属）+ `FUN_00f4c8d0`（钢笔/铅笔/马克笔等日常工具，真机命中 10033 次，覆盖面是前者近 5 倍）。两个效果：笔尖角度模型（方向敏感）+ 提按速度代理（方向无关）。用户反馈"看上去还行"。**`bVar16<4`（最常用钢笔/铅笔量级）仍摸不到**（§03c–§03f） |
| `battop/` | Rust 二进制（采样诊断服务） | ✅ 真机通（历史更早，不是本线首创；2026-09-09 搬进本线）。**有意不随开机自启**，原因见其 `FINDINGS.md` 与 `battop/README.md` |
| `wallpaper-serve/`、`font-serve/` | Web 服务（挂网关） | ✅ 真机通。2026-09-11 从 `shelf/services/` 挪进来——用户判断"壁纸"/"字体"概念上更像系统增强、而非"书架内容管理"；运行时行为零变化，只是编译产物来源目录变了 |
| `shared/`、`lo-alias/` | 剥离移植的独立副本 | 2026-09-11 因 `chinese-ime/` 挪出仓库，原本路径引用它的三个工具文件（特征码扫描 + trampoline）和 `cangjie-lo-alias.sh` 都改成本目录下的独立副本（后者去掉了 `cangjie-` 前缀），来龙去脉见各自 `PROVENANCE.md` / `README.md` |

**未闭环**：集中在 `handwriting-stroke/`——`bVar16<4` 虚函数动态分发目标未确认、`FUN_00f4f430`（`bVar16==3`）因前 20 字节有条件分支不能安全 patch、当前参数只是初步校准起点。见 §05。

> `chinese-ime/` 挪出仓库后，`cangjie-langhook.so` 是否仍在设备上，取决于设备当前状态（2026-09-09 曾发现它从设备上整个消失）。它与 `hl-snap.so` **不能同时部署**（都 patch `FUN_00f05ad0`），见 §04。

## 01｜架构决策

**为什么不是"给 `chinese-ime/langhook` 加个开关"，而是整个独立出来？** 第一版运行期开关方案技术上可行、真机也验证通过——但它仍是"一份包含完整拼音输入法 + 荧光笔吸附的二进制，靠环境变量关掉不想要的部分"，源码层面两者还绑在一起，对老项目的任何改动都可能影响荧光笔吸附这个独立诉求的构建产物。用户要的是**源码层面的独立**：`hl-snap/` 现在是独立的 `.c` + `.xovi` 元数据 + `Makefile` + 部署脚本，产出跟 `cangjie-langhook.so` 不同名（`hl-snap.so`）、不同 `_xovi_shouldLoad` 判据的扩展。

**为什么 `_xovi_shouldLoad` 用自己的目标特征码，不借用 `setLanguageCode`？** 在老项目里 `setLanguageCode` 是"总闸"——所有 hook 能不能装，先看它的特征码在不在。这对完整输入法线合理（它本身也是要 hook 的目标），但对只关心荧光笔吸附的独立扩展不合理：`hl-snap.so` 装不装应只取决于它自己要 patch 的 `FUN_00f05ad0` 还在不在。

**为什么 `battop/` 搬进来？** 概念上它属于"跨块单点增强工具"，只是历史上先于这条线存在。搬家保留了 `FINDINGS.md`（那次事故调查是 battop 自己的历史）；早期诊断脚本 2026-09-11 也归档进 `battop/history/`。

**命名遗留**：`enhance/` 与旧 `xovi-extensions/`（设备端 QML/UI 增强）曾撞名，两者没有从属关系；后者已搬出仓库，撞车对象暂时物理上不存在，但这不算"已解决"——若它以后被捞回来，按"`xovi-extensions/` 管 QML/UI 层增强、`enhance/` 管更底层的单点工具"分工（这只是设想，非已拍板决定，要动这条边界得再问用户）。

## 02｜跟网关网页面板的关系

这条线的名字不是巧合——2026-09-09 更早些时候，网页「管理」页新增了「系统增强」二级 tab，把 `hlSnapCjk` 开关和 battop 启停做成网页可操作的面板（代码在 `gateway/src/enhance/{mod,qol,battop}.rs`，2026-09-11 前叫 `shelf/services/shelf-gateway/src/enhance/`）。分工：

- **`enhance/`（本白皮书）**管这些工具**本身怎么实现、怎么部署**；
- **`gateway/src/enhance/`** 管**网页上怎么远程控制它们**——只走 `systemctl` 和读写 `~/.local/share/cangjie-ime/reading-qol.json`（`hl-snap.so` / `hw-stroke.so` 读的是同一份文件），与仓库源码目录无代码依赖，两边靠约定的文件路径/systemd 单元名对接。

**网页现状**（以 `gateway/ui/app.js` 为准；网页那边后续又演进过几轮，纯网页层变化，`enhance/` 代码零变化，细节见 `shelf/docs/reMarkable书架白皮书.md` §03aj–§03an）：

| 开关 | 位置 | 写什么 |
|---|---|---|
| CJK 荧光笔精确吸附 | 「管理 → 系统增强」 | `hlSnapCjk`（默认开） |
| 电池刺客 | 「管理 → 系统增强」（2026-09-21 从「实验室」移来）；开了才出现「电池刺客」数据页 | `systemctl start/stop battop` |
| CJK 手写笔迹优化 | 「管理 → 实验室」 | 纯网页层派生开关：`hwStrokeNibMinRatio < 1.0` 视为已开；开写 `0.6`、关写 `1.0`（两个 min_ratio 字段同步写） |

写这个共享文件遵守**全量写回**铁律：`gateway/src/enhance/qol.rs` 把整份文件当不透明 JSON map 读进来、只覆盖要改的键，不知道的键原样写回，避免冲掉别处（旧原生设置页、C hook）写入的开关。

## 03a｜`hl-snap/` 诞生记（2026-09-09，真机通）

**起因排查**：用户反馈划线没有精确吸附。`journalctl -u xochitl` 搜 `cangjie` **零命中**——正常加载会打一串 `[cangjie]` 初始化日志，一行都没有，说明扩展压根没被 xovi 尝试加载，不是"装了但某个 hook 没生效"。查文件确认：`find / -iname "cangjie-langhook.so*"` 全设备零命中，连同 `~/.local/share/cangjie-ime/` 下 5 个词典 blob 一起消失，只有 `reading-qol.json` 幸存（"裸机恢复"类重置的典型后果，见 [`../../docs/INSTALL.md`](../../docs/INSTALL.md) OTA 一节）。

**顺带发现一处过期且危险的记录**：当时的项目说明里写的持久化机制是往 `/usr/lib/systemd/system/xochitl.service.d/` 放 drop-in，但那条路已在 2026-08-16 因**两次真机 dm-verity A/B 回滚变砖**而放弃；现行机制是持久源放 `~/xovi/services/xochitl.service/*.conf`（`/home`），`xovi/start` 遍历这些源目录拷进新挂载的 `/etc/.../xochitl.service.d/` tmpfs 再重启对应 unit，全程不碰 `/usr`。（文档指向"已因变砖而放弃的危险方案"，比普通的文档滞后风险级别高得多，发现要立刻改。）

**方案演进两版**：

1. 第一版（后来整个撤回）：在 `chinese-ime/langhook/hook_init.c` 加运行期开关 `CANGJIE_IME_HOOKS`，`=0` 时跳过拼音输入法/EF/KBS 那 8 个 hook、只留荧光笔扩张 hook。真机验证通过，但用户要求"不动老项目"，整段撤回。
2. 第二版（最终形态）：独立最小扩展 `enhance/hl-snap/`，见 §01。

**真机验证**（第二版）：完整备份 → 移除设备上第一版残留的 `cangjie-langhook.so` + 配置（两者不能共存，会抢同一个 hook 目标）→ scp + md5 校验部署 `hl-snap.so` → `xovi/start` → journal：

```
[hl-snap] _xovi_shouldLoad: 固件兼容(FUN_00f05ad0@0xf03670) → 加载
[hl-snap] 荧光笔EXPAND hook 安装完成 @ 0xf03670（neuter=1）
```

`/proc/<pid>/maps` 里 `hl-snap` 段数 0→4、`cangjie-langhook` 段数降回 0；`is-active` / `NRestarts` / `MainPID` 健康检查通过（含延迟复查排除慢速崩溃循环）。地址 `0xf03670` 与第一版真机验证时一致，确认特征码在 3.28.0.172 上仍唯一命中。

## 03b｜`battop/` 搬迁（2026-09-09，纯目录搬家）

把旧 `misc/battery-audit/battop/`（含 `.cargo/config.toml`、`Cargo.*`、`install.sh`、`src/`）与 `FINDINGS.md` 移进 `enhance/battop/`，同步更新了 4 处仓库文档引用和 `gateway` 里 `battop.rs` 的注释；设备端部署路径 `/home/root/battop` 不受影响。2026-09-11 清理旧顶层 `misc/` 时，剩下的诊断脚本历史也归档进了 `battop/history/`（当初没跟着搬是因为它属于"怎么发现该建 battop"的调查方法论历史，但 `misc/` 已无别的内容，跟唯一引用它的 battop 一起走更合适）。

## 03c｜`handwriting-stroke/` 研究：xochitl 怎么画笔画（2026-09-09，纯静态分析）

**目标**：CJK 手写笔迹渲染优化——笔锋按中文书写习惯（运笔粗细/顿挫）渲染。**与 AI 手写识别（笔迹→文字）完全无关**，用户第一轮就澄清过这个区分。全仓库关键词搜索确认这是全新功能：没有配置键、没有 hook、没有反编译记录。唯一沾边的先例——笔记页背景滤镜——是判死的（C++ `SceneView` tile 增量渲染够不到）。

**排查五轮，一张表**：

| 轮 | 手段 | 结论 |
|---|---|---|
| 1 | `strings` 侦察（没装 Ghidra） | 真机 `xochitl` 里有整套 `Quill::strokev2` 命名空间的 RTTI：按笔型分光栅化策略类（`FillPencil` / `FillBallpoint[AA]` / `FillSolid_*_AA` / `FillMaskedEraser` / `FillAnts` / `FillShaderAA`），外包 `LerpRaster` / `MonoRaster`。判断（未证实）：这是"宽度/透明度沿路径插值渲染"的实现层，比背景滤镜那层更底层、更对口 |
| 2 | 真上 Ghidra，脚本反查 vtable | 失败：反查到的"vtable"头几个槽位解出来是 ASCII 文本（`"N8stroke"`），不是函数指针。当时结论"这些类可能没有虚函数"——**后被第 3 轮推翻**（见 §04） |
| 3 | Ghidra GUI 人工交互（用户操作截图回传，脚本驱动不了 GUI） | 完整定位：从 RTTI 字符串的真实 xref 追到 `saveStroke` / `ShapesOverlay`（继承 `QQuickPaintedItem`）→ `updateImage`（`FUN_008bbb80`，真正画像素的入口）→ `StrokeRenderer` 构造函数 `FUN_00f3dcf0`（把 `Fill*` / `CoverageBuffer` / `IVaryingsGenerator` 等**内联组合**进自己，vtable 指针构造时按值写入）。第 2 轮失败的原因就在这里：内联组合成员没有"指向其 typeinfo 的裸指针"，反查天生走不通 |
| 4 | headless 脚本接力（已有具体地址后不必再靠 GUI 截图，前提是 GUI 关闭工程释放 `.lock`） | 定位逐点渲染分派函数 `FUN_00f3f9d0`：按笔型标签 `bVar16` 分支各自算宽度；最丰富的方向敏感分支有 smoothstep 三次缓动曲线 + 方向角 `sincosf`。再往下是变宽几何生成器 `FUN_00f47530`（上一点半宽 + 当前点半宽 + 垂直单位向量 → 梯形四角点，**变宽笔画的几何能力本来就存在**）。排除了一个岔路：`FUN_00f33180` 只是通用 `QVector` 插入函数 |
| 5 | 继续挖到像素层 | 梯形交给四级 Sutherland-Hodgman 多边形裁剪（`FUN_00f37d30 → 00f378e0 → 00f376b0 → 00f374f0`，固定代码），最后一级改虚函数间接调用（`vtable+0x10`）交给多态像素消费者（`CoverageBuffer` / `Fill*`）。静态分析到此为边界 |

**完整链路**（五层）与 hook 位置见下图；点结构、各分支细节见 [`../handwriting-stroke/README.md`](../handwriting-stroke/README.md)。

![笔画渲染链与 hook 位置](diagrams/handwriting-render-chain.svg)

**架构结论**：整条"笔画变像素"链路是"固定几何算法（宽度插值 + 变宽四边形 + 多边形裁剪，全部非虚函数）+ 最后一步虚函数分发给可插拔的像素填充策略"。**对诉求的结论**：所有宽度公式都只跟"距离/压感/一条固定缓动曲线"相关，没有"笔画方向（横竖撇捺）相关的权重"——最贴近诉求但没有现成代码可抄，需要新写逻辑；改动点全在 `FUN_00f3f9d0` 这一层及其下游几何生成器，裁剪与像素分发两层是通用管线、不需要动。

**⚠️ 2026-09-09 晚间勘误**：第 3 轮曾把 `VaryingGenerator_WidthLength` 的业务方法 `FUN_00f401f0` 当作"最具体的候选改动点"（反编译显示 `return param_1 * *(float*)(param_2+8);`）。按项目"反编译要交叉核实原始汇编"纪律补查后发现真实签名是 **3 个 float 入参 + 1 个对象指针、产出一对 `{float,float}`**，用了 `[x0+8]` 与 `[x0+0xc]` 两个配置字段——反编译器把多寄存器传参/HFA 返回简化丢了信息；更关键的是 `getReferencesTo` 显示它**从没被真实调用点引用过**。这个函数是否真会被执行，静态分析从没坐实过，"最具体候选"断言收回，hook 目标改用确认在真实调用链上的 `FUN_00f47530`（§03e）。

这轮排查全程没写过实现代码、没碰真机。

## 03d｜Ghidra 环境搭建（2026-09-09）

Ghidra 装法、`JAVA_HOME` 处理（Ghidra 12.x 要 JDK 21）、`xochitl` 二进制怎么拉、headless 怎么调，都记在 [`../../defw/README.md`](../../defw/README.md)（该目录本身就是这套工具的工作目录），这里不重复。当时的取舍留一条：第一版手动装 zip，第二版改用发行版包（`paru -S ghidra --assume-installed java-environment=21` 跳过虚拟依赖，避免拉一份多余的系统 JDK），用 `JAVA_HOME` 环境变量每次调用时指定（pacman 装的 `/opt/ghidra` 是 root 拥有，没 sudo 改不了 `launch.properties`）。GUI 环境坑（Wayland 下 Swing 空白）见 §04。

## 03e｜`handwriting-stroke/` 第一版实现：真机部署验证（2026-09-10）

落地前先纠正了 §03c 里对 `FUN_00f401f0` 的分析，改用确认在真实调用链上、签名简单的 `FUN_00f47530`（变宽梯形几何生成器）当 hook 目标：两个 float（s0/s1）+ 一个指针（x0），标准调用约定，入口第一件事就是读"当前点宽度"（`(char*)ctx+4`）算半宽。

- **Step 1/2 验证机制可行**：先部署纯诊断 hook（只读日志、call-through 不改值），真机手写时 `w` 数值合理且随笔压连续渐变；再加浮点开关 `hwStrokeWidthFactor`（默认 1.0），改成 2.5/0.5 后真机截图肉眼确认笔画明显变粗/变细——证实"改这条渲染路径能真实影响笔迹粗细"。
- **笔尖角度模型**：用户反馈"CJK 顿挫不是单纯粗细问题"，调研西式书法笔工具的经典公式（Illustrator/Inkscape 的 Calligraphic Brush 同款）`宽度 ×= min_ratio + (1-min_ratio)×|sin(运笔方向角−笔尖固定角度)|`。运笔方向角不依赖点结构方向字节（真机诊断过：日常"中粗钢笔"走 `bVar16<4` 纯线性分支，不碰方向字段），改用 `FUN_00f47530` 自己维护的"上一点坐标"现算，所有笔型统一走到。踩了 GLIBC 符号版本坑（§04），用三角恒等式绕开 `atan2f`、`__builtin_sqrtf` + `-fno-math-errno` 绕开 `sqrtf`。
- **效果强度按宽度自动挂钩**：真机反馈"钢笔效果不好、毛笔还行"，诊断发现两者走同一条分支同一套公式，纯粹是基础宽度差异导致同比例摆动观感不同——改成效果强度跟 `w` 挂钩，细笔画趋近关闭、粗笔画满强度。
- **⚠️ 撤回一次**：尝试直接读点结构笔型标签字节（`bVar16`）强制排除钢笔，真机测大号 paintbrush 时数据自相矛盾（`w` 恒定但 `fillType` 在 0~255 随机跳，`x`/`y` 却是真实连续运笔坐标）而撤回，根因见 §04（跨路径身份假设）。退回纯宽度渐变方案。

## 03f｜压感预研撤回 + 运笔速度代理 + 第二个 hook 目标（2026-09-10 下午）

**压感**：用户问"有力度吗"——headless 反编译 `FUN_00f3f9d0` 核实：点结构 offset `0xD` 硬件压感字节确实被读出转成 0~1 浮点，但存进 `plVar6+0x74`，不是我们 hook 收到的 `ctx`（`ctx=plVar6[1]`，另一个对象）。改用不依赖它们的独立算法（`当前点地址 = param_2[3] + (param_2[4]*0xe − 0xe)`）读压感，两轮真机证实**压感字节本身是真实数据**：第一次（轻触→使劲按）只在接触瞬间爬坡（12→255）、之后饱和；第二次专测才拿到宽分布（3~255）。但**跨函数传值到 `FUN_00f47530` 失败**：诊断样本里只有 2/3449 落在会调用它的分支，而它自己触发了 686 次——大部分调用根本不经过被锁定的那份 `FUN_00f3f9d0`（它有 6 个调用点，可能"实时预览"和"提交进笔记本"走不同路径）。放弃真实压感，改用完全在 hook 内部就能算的**运笔速度代理**（复用"上一点坐标"算距离：慢/顿笔→粗，快/带过→细，与笔尖角度模型共用"强度按基础宽度挂钩"逻辑）。

**第二个 hook 目标**：真机测试发现 `FUN_00f47530` **只有"书法笔"这一个工具会走到**，且书法笔原生就有方向敏感宽度（平头笔尖真实物理效果：竖线是横线 2~4 倍粗，两个效果全关也存在，不是 bug）——在书法笔上叠加效果是重复造轮子，而钢笔/铅笔/马克笔完全摸不到这个 hook（之前"钢笔效果不好"很可能是没生效）。用户拍板"另开调查，找钢笔/铅笔自己的写入点"。headless 反编译 `FUN_00f3f9d0` 里 `bVar16==3/5/6` 三个分支各自调用的绘制原语，核实真实签名：

- **`FUN_00f4c8d0`**（`bVar16==6` 直接调、`5` 经 `FUN_00f4d190` 间接调）：`(x, y, ctx)`，宽度在 `ctx+4`（`undefined2*` 的 `param_3+2` 是 2 字节单位，换算正好 4），与 `FUN_00f47530` 同一调用约定；前 20 字节纯栈操作，patch 安全 → 采用。
- **`FUN_00f4f430`**（`bVar16==3`）：签名一样，但**前 20 字节第 3 条指令是条件分支**（`cbz`，PC 相对寻址），被 `memcpy` 进 call-through stub 后分支目标会算错 → 不能安全 patch，留作已知候选。

部署 `FUN_00f4c8d0` hook（诊断优先，先保持效果关闭确认真机加载正常）后真机命中 **10033 次**（`FUN_00f47530` 仅 2117 次），`w` 范围 3~36，真实触及日常常用的多种笔，覆盖面近 5 倍。按真机数据重新校准阈值，两个效果开到中等强度（`min_ratio=0.6`），用户反馈"看上去还行"。

## 04｜踩坑

**逆向方法论**

- **RTTI typeinfo 存在 ≠ 有 vtable，"反查找不到 vtable" ≠ "没有 vtable"**：类被 `typeid()` 用到就会生成 RTTI，不代表多态；但反过来，反查谁指向 typeinfo 走不通，也不代表没有 vtable——**内联组合成员**的 vtable 指针是容器对象构造函数里按值直接写入的，没有任何地方存指向其 typeinfo 的裸指针。§03c 第 2 轮先后犯了两头的错。遇到反查走不通，先判断目标是"独立堆对象"（能反查）还是"别的对象的组合成员"（得从容器对象的构造函数正向找）。
- **stripped 二进制里，指向字符串的指针字段不会自动有 xref**：Ghidra 的引用分析只认已被定型为指针的数据，`Show References To` 命中为空**不代表**真的没人引用；退回 `Search → Memory`（十六进制字节序列搜索，纯字节扫描）。但"扫到一个指针值"不等于"它在我期望的结构体字段里"——`.data.rel.ro` 里紧密排列的对象会让"反查上一层"命中无关邻居，**必须验证内容合理性**（这次靠"dump 出的槽位能不能解出人话"戳破了错误假设）。
- **Qt 编译进二进制的调试/异常字符串是比符号表更可靠的类名/函数名来源**：符号表被剥得只剩 `FUN_xxxxx`，但沿任意已知函数反编译读下去，常能撞见字面写死的源码路径/方法名/日志文案（这次连续撞见 `shapesoverlay.cpp` / `saveStroke` / `updateImage` / `"New StrokeRenderer,"`），是最快的"确认这是哪个类"的手段，应优先找它们。
- **反编译"干净利落"要留心眼、关键结论必须交叉核实原始汇编**：`FUN_00f401f0` 被反编译成"一行线性缩放"，真实是 3 float 入参 + 对象指针、产出一对 float；更严重的次生错误是信了简化结论就宣称"最具体候选"，没先确认它有没有被真实引用（`getReferencesTo` 零调用者早就是暂停信号）。产出多个值的接口方法简单到只有一行，往往是反编译器漏看了寄存器。
- **反编译里的 `undefined2*` 指针，偏移数字是"2 字节单位"不是字节**：`FUN_00f4c8d0` 的 `param_3 + 2` 实际是字节偏移 4、`param_3 + 0x3c` 是 `0x78`。核对宽度字段这类关键偏移要先确认指针实际类型宽度。

**hook 安全性**

- **多条调用路径汇聚到同一函数时，"某参数在路径 A 里等于什么"不能套到路径 B**：钢笔（`bVar16<4`）走的是 `plVar6` 虚函数调用，没验证过它传给 `FUN_00f47530` 的 `ctx` 就是 `FUN_00f3f9d0` 自己的 `lVar7`；直接照抄"参数就是 `lVar7`"得到的 `ctx+0x70` 读出来是随机数据。`ctx+4/+0x48/+0x4c/+0x5a`（hook 目标自己直接读写的字段）安全，借用上游解读的字段（`ctx+0x70`）不安全。
- **同一函数有多个调用点，不代表某个 hook 实例能看到所有调用**：`FUN_00f3f9d0` 有 6 个调用点，从它入口设全局变量再传给下游的方案，在单个调用序列内逻辑无懈可击，但真机数据证明下游绝大多数调用不经过这一份。"A 调用 B"的静态事实不能默认成"所有 B 的调用都来自 A"，除非 xref 或运行时数据证明 B 只有唯一入口。
- **"前 N 字节纯栈/寄存器操作、可安全 patch"必须对每个候选逐个验证**：`FUN_00f4f430` 签名/宽度约定与已验证的两个完全一致（同一族"变宽几何生成器"），但前 20 字节第 3 条指令是条件分支，PC 相对偏移搬进 stub 会指向错误目标——靠 `CheckFuncSizes.java` 逐条反汇编检查提前拦下，没有真机验证就发现了。签名相似不代表二进制布局相似。
- **两个 xovi 扩展抢同一个 hook 目标会冲突**：`hl-snap.so` 与 `cangjie-langhook.so` 都会 patch `FUN_00f05ad0`，同时部署行为未定义。凡是"从老项目里独立拆出功能子集"，都要检查新旧产物是否可能同时部署、目标有无重叠，并在部署文档里把互斥关系写清楚（已写进 `hl-snap/README.md`）。

**构建与环境**

- **交叉编译工具链的 glibc 可能远新于设备，`dlopen` 在符号解析这步静默失败**：`atan2f` / `sqrtf` 在本地 `aarch64-linux-gnu-gcc` 链接时被打上 `GLIBC_2.43`，设备 `libm.so.6` 没这么新，整个 `.so` `dlopen` 失败。`nm -D` / `objdump -T` 里 `GLIBC_2.17` 是常见符号的老基线，出现高很多的版本号都要警惕。规避：能避开的用 `__builtin_xxx` + `-fno-math-errno` 让编译器内联成硬件指令；避不开的用三角恒等式改写。**连带效应**：`hw-stroke.so` 加载失败还拖累同一次扫描里的 `hl-snap.so` 没加载成功——多扩展场景下要检查所有扩展是否都正常，不能只看目标扩展自己的日志。
- **Java Swing 在 Wayland 平铺式合成器下整窗口空白、没菜单栏**：`_JAVA_AWT_WM_NONREPARENTING=1` 修复；先查 `$XDG_SESSION_TYPE`，不要怀疑程序或工程损坏。Ghidra headless 建的 `.gpr` 是 0 字节也是正常现象（元数据在 `.rep/` 里）。
- **项目说明里记录的做法会过期，而且过期的可能是"已经放弃的危险方案"**（§03a 记的 `/usr` 持久化 drop-in）：这种"过期文档指向危险操作"要立刻改，不能当一般文档债务。

## 05｜真机待办

**未闭环**（全在 `handwriting-stroke/`）：

- `bVar16<4`（最常用的钢笔/铅笔量级工具）仍摸不到——虚函数动态分发，运行时多态目标未确认；
- `FUN_00f4f430`（`bVar16==3`）因前 20 字节有条件分支不能安全 patch；
- 像素消费者虚函数分发目标 / smoothstep 曲线下游用途，静态分析到边界，没深挖；
- 当前参数是初步校准起点，未精细打磨；
- 想换回真实压感，需先搞清 `FUN_00f47530` / `FUN_00f4c8d0` 各有哪些调用路径。

具体待办清单见 [`../handwriting-stroke/README.md`](../handwriting-stroke/README.md)「未完成」。

**已闭环（真机）**：`hl-snap/` 精确吸附 hook（§03a，journal 三行关键日志 + 健康检查 + 地址一致性交叉验证）；`battop/` 目录搬迁（§03b，纯文件系统操作，不涉及设备行为，`cargo build` 确认引用它的网关仍能编译）；`handwriting-stroke/` 笔尖角度模型 + 提按速度代理，两个 hook 目标均真机验证（§03e/§03f，journal 日志 + 真机截图 + 用户定性反馈，与 `hl-snap` 共存不冲突）。

**已放弃**：按 `bVar16`（笔型标签）精确排除钢笔的方案（§03e / §04，真机数据证伪了"`FUN_00f47530` 的 `ctx` 在钢笔路径下等于 `FUN_00f3f9d0` 的 `lVar7`"）；真实硬件压感跨函数传值方案（§03f，压感数据本身是真的，但"入口设全局变量 → hook 内读"被真机数据证伪，改用运笔速度代理）。

## 附录｜迁移沿革（原 `enhance/README.md`「跟其它目录的关系」，2026-09-20 迁入并压缩）

| 时间 | 事件 |
|---|---|
| 2026-09-09 | `battop/` 本体从旧 `misc/battery-audit/battop/` 搬进本线 |
| 2026-09-10 | `defw/`（3.28.0.172 固件逆向产物）由 `ghidra-project-328` 改名而来；**不属于**本线，是共享的逆向基座，`handwriting-stroke/` 的研究用它 |
| 2026-09-11 | 旧 `chinese-ime/` 挪出仓库（其设备端产物是否仍在跑取决于设备现状）→ `shared/`（特征码扫描 + trampoline）、`lo-alias/` 由路径引用改成剥离移植的独立副本 |
| 2026-09-11 | 旧 `misc/battery-audit/` 剩下的诊断脚本归档进 `battop/history/`，`misc/` 删空 |
| 2026-09-11 | `wallpaper-serve/`、`font-serve/` 从 `shelf/services/` 挪进本线（概念归类，运行时行为不变，仍靠网关代理托管）；历史 bind-mount 壁纸方案脚本归档进 `wallpaper-serve/legacy-bind-mount/` |
| 2026-09-11 | `packaging/` 成为 `hl-snap` / `handwriting-stroke` / `battop` 的 host 侧编排方（各有 `deploy-<name>.sh`），三个工具原有的设备端 `install.sh` 仍可脱离它单独跑 |
| 2026-09-11 | 网关本体由 `shelf/services/shelf-gateway` 正名搬到顶层 `gateway/`，`gateway/src/enhance/` 是本线开关的网页控制面（消费方） |
| 2026-09-15 | 全量代码审查：`hl-snap` / `handwriting-stroke` 各自约 50 行逐字节重复的 trampoline 安装代码收进 `shared/trampoline_patch.c` |
