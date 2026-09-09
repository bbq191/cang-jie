# reMarkable 系统增强线（enhance）白皮书

> 记"怎么决定、真机怎么验、踩了什么坑"。读现状先看 §00b；待办/已闭环看 §05；踩坑看 §04。
> ⚠️ **书名跟 `xovi-extensions/docs/reMarkable系统增强白皮书.md` 只差一个"线"字，内容完全不是一回事**——那本是设备端 QML/UI 增强全家桶（reading-qol/font-menu，工程纪律 六分块「④系统增强」）；这本是 2026-09-09 新开的顶层项目线 `enhance/`，专记"底层、跨块、原来散落各处的单点增强工具"。两者暂时并存，命名撞车是已知问题，见 §01。

## 00｜定位与原则

**起因**：2026-09-09 用户反馈"划线没有按 CJK 精确吸附"，排查发现根因是 `cangjie-langhook.so`（`chinese-ime/langhook` 那条完整中文化线的 xovi 扩展）整个从设备上消失了。第一版修复直接改在 `chinese-ime/langhook` 里加了个运行期开关（`CANGJIE_IME_HOOKS=0`）。用户用三轮话把方向纠正清楚：

1. "cjk 精确吸附，不动老项目，在新路径下工作，类似 shelf/"——第一版改动全部 revert，另起炉灶。
2. "也不放人〔在〕cang-jie 路径，应该是新路径，比如 enhance，里面不仅有精准吸附，还有笔锋和电池刺客等等"——不要只是"挪个单功能目录"，要建一个能装下这一类"跨块单点增强工具"的顶层项目线。
3. "属于一条新线路，系统增强线"——`enhance/` 定名，跟 `shelf/`、`notes/` 并列。

**三条硬原则**（用户纠正过程里体现出来的，不是提前定好再执行）：
1. **单点增强工具不该塞进已有的大项目**——哪怕改动量只有几十行，只要这个功能能独立部署/独立生命周期，就该独立成目录，不能因为"改动小"就图省事合并进老项目。
2. **复用不复制**：需要老项目里现成的通用基础设施（`chinese-ime/langhook` 的特征码扫描/trampoline 工具文件）时，走**路径引用**，不 `cp` 一份、不改老项目一个字节——跟 shelf 的 `bookconv` 被 `reading/device-rs` 路径引用是同一个已有先例。
3. **一份产物两种用法，别搞两份构建**：需要"只要功能子集"时优先选运行期开关而不是编译期 `#ifdef`/独立构建变体——后者要么让老项目里一批函数在特殊构建下变成死代码（触发 `-Wunused-function`，还得记两份构建产物的差异），要么真拆出一份新代码时又违反第 2 条。这条原则最终体现为：`hl-snap/` 是**全新独立源码**（不是从老项目 `#ifdef` 出来的变体），因为它连接口设计都不一样（`_xovi_shouldLoad` 判据从"借用 setLanguageCode"改成"用自己的目标特征码"）；但 `hl-snap/` 内部对 `chinese-ime/langhook` 的三个工具文件，用的是路径引用不是复制。

## 00b｜现状总览（2026-09-09，读本文其余历史节前先看这里）

**目录结构**：

```
enhance/
├── README.md                    目录一览 + 跟其它目录的关系
├── docs/reMarkable系统增强线白皮书.md   本文件
├── hl-snap/                     荧光笔 CJK 精确吸附，独立最小 xovi 扩展（C，ARM64）
├── battop/                      电池刺客，独立 Rust 二进制（诊断采样器，git mv 自 misc/battery-audit/）
└── handwriting-stroke/          CJK 手写笔迹渲染优化，研究阶段，零实现代码
```

**`hl-snap/`（§03a）**：✅ 真机通。逻辑逐字节抄自 `chinese-ime/langhook` 已验证过的 Step HL2 代码段，路径引用（不复制）它的 `scan.c`/`pattern.c`/`trampoline_aarch64.c` 三个工具文件；`_xovi_shouldLoad` 直接用自己的目标特征码当固件兼容性判据，不借用 `setLanguageCode`。真机 journal 确认 hook 装上（`FUN_00f05ad0@0xf03670`），健康检查通过（`is-active`/`NRestarts`/`MainPID`，含延迟复查）。设备上目前**只有这一个扩展在跑**，完整拼音输入法/UI 汉化没装。

**`battop/`（§03b）**：✅ 真机通（历史更早，不是本线首创）。`git mv` 自 `misc/battery-audit/battop/`（含 `FINDINGS.md`），4 处仓库文档引用同步改了路径。

**`handwriting-stroke/`（§03c）**：🔬 探路中，**零实现代码**，但完整调用链已经摸清（点数据→按笔型算宽度→变宽梯形几何→多边形裁剪→虚函数分发给像素消费者，五层全走完）。第一轮 `strings` 侦察找到 RTTI 类名；第二轮脚本化反查 vtable 失败（错误地下结论"可能没有虚函数表"，后纠正）；第三轮 Ghidra GUI 交互排查定位到 `ShapesOverlay::updateImage`→`StrokeRenderer`→`VaryingGenerator_WidthLength::generate()`；第四轮 headless 脚本接力（已知地址不用再靠人工截图），挖出逐点分派函数 `FUN_00f3f9d0`（含 smoothstep 缓动曲线+方向角 sincos）和变宽笔画的几何生成器 `FUN_00f47530`；第五轮继续挖穿四级多边形裁剪链，链路终点确认为虚函数分发（具体目标运行时决定，静态分析到此为边界）。

**未闭环**：`handwriting-stroke/` 研究部分基本闭环（架构+具体函数都已定位），**但完全没有做过任何 hook/真机验证**——连"改这个函数会不会真的改变渲染出来的笔迹宽度"都还没试过，见 §05。`hl-snap/` 和 `battop/` 都已经真机验证过，没有已知的未闭环项。

**命名遗留问题**：`enhance/` 跟 工程纪律 六分块「④系统增强」（`xovi-extensions/`）撞名，还没有正式理顺，见 §01。

## 01｜架构决策

**为什么不是"给 `chinese-ime/langhook` 加个开关"，而是整个独立出来？** 第一版方案（`CANGJIE_IME_HOOKS=0` 运行期开关）技术上是可行的、也真机验证通过了——但它仍然是"一份包含完整拼音输入法+荧光笔吸附的二进制，通过环境变量关掉不想要的部分"，源码层面两者还是绑在一起，任何对 `chinese-ime/langhook` 的改动（哪怕跟拼音输入法无关）理论上都可能影响到荧光笔吸附这个独立诉求的构建产物。用户要的是**源码层面的独立**：`hl-snap/` 现在是一份完全独立的 `.c` 文件+独立的 `.xovi` 元数据+独立的 `Makefile`+独立的部署脚本，产出一个跟 `cangjie-langhook.so` 完全不同名字（`hl-snap.so`）、完全不同 `_xovi_shouldLoad` 判据的扩展。两者除了"逻辑抄自同一处、复用同三个工具文件"之外，构建期/部署期没有任何耦合。

**为什么 `_xovi_shouldLoad` 改成用自己的目标特征码，不借用 `setLanguageCode`？** 原来在 `chinese-ime/langhook` 里，`setLanguageCode` 是"总闸"——**所有** hook（包括荧光笔吸附）能不能装，先看这个函数的特征码在不在。这个设计对完整输入法线合理（`setLanguageCode` 本身也是要 hook 的目标之一，天然适合当总闸），但对一个只关心荧光笔吸附的独立扩展来说不合理——`hl-snap.so` 装不装应该只取决于它自己要 patch 的 `FUN_00f05ad0` 还在不在，不该被一个它根本不用的函数（`setLanguageCode`）的存在与否连累。这是"源码独立"这条原则的自然推论，不是额外决定。

**为什么 `battop/` 要 `git mv` 过来而不是留在 `misc/`？** 用户明确说"里面不仅有精准吸附，还有笔锋和电池刺客等等"——battop 本身概念上就属于这条新线（跨块的单点增强工具），只是历史上先于这条线存在、暂居 `misc/battery-audit/`。搬家保留了 `FINDINGS.md`（那次事故调查是 battop 自己的历史，理应跟着走），没搬 `misc/battery-audit/` 下的诊断脚本（`bataudit*.sh` 等——那些是"怎么发现 battop 该建"这个更早期过程的遗留，属于调查方法论历史，不属于 battop 这个工具本身）。

**命名撞车怎么处理**：目前**刻意不处理**——`enhance/` 和 `xovi-extensions/`（工程纪律「④系统增强」）没有从属关系，也没打算合并。如果以后要理顺，大概率的方向是：`xovi-extensions/` 继续管"设备端 QML/UI 层面的增强"（阅读体验、设置面板），`enhance/` 管"更底层、不需要 QML 参与、原来又没有明确归属的单点工具"——但这只是猜测，不是已经拍板的决定，真要动这条边界得再问用户。这次先如实记录"两个东西都叫系统增强，读者自己留神"，不强行统一。

## 02｜跟 shelf 网页面板的关系（`shelf/services/shelf-gateway/src/enhance/`）

这条项目线的名字不是巧合——2026-09-09 更早些时候，shelf 网页「管理」页新增了一个「系统增强」二级 tab（`shelf/services/shelf-gateway/src/enhance/{mod,qol,battop}.rs`），把 `hlSnapCjk` 开关和 battop 启停做成了网页可操作的面板（细节见 `shelf/docs/reMarkable书架白皮书.md` §03aj）。**那次改动完全没有涉及"这些工具本身该放在仓库哪个位置"**——`shelf-gateway::enhance` 只是拿设备上已经装好的东西（`reading-qol.json` 文件、`battop.service` 单元）走 HTTP/`systemctl`，跟仓库源码目录结构没有代码依赖。这次给"CJK 精确吸附"独立成项目线时顺着同一个名字延续下来，是有意保持一致，不是重复发明。

两条线的分工：`enhance/`（本白皮书）管**这些工具本身怎么实现、怎么部署到设备**；`shelf-gateway::enhance`（`shelf/docs/reMarkable书架白皮书.md` §03aj）管**网页上怎么远程控制它们**。改这些工具的行为来 `enhance/`，改网页控制面板去 `shelf/`，两边通过约定的文件路径/systemd 单元名对接，不是直接代码调用。

## 03a｜`hl-snap/` 诞生记（2026-09-09，真机通）

**起因排查**：用户反馈划线没有精确吸附效果。`journalctl -u xochitl` 搜 `cangjie` **零命中**——正常加载会打一串 `[cangjie]` 初始化日志（`_xovi_shouldLoad`/特征码定位/hook 安装完成），一行都没有说明扩展压根没被 xovi 尝试加载，不是"装了但某个 hook 没生效"。直接查文件确认：`find / -iname "cangjie-langhook.so*"` 全设备零命中，连同 `~/.local/share/cangjie-ime/` 下的 5 个词典 blob 一起，只有 `reading-qol.json` 幸存。

**顺带发现文档过期且有安全隐患**：工程纪律 当时记的持久化机制是"`/usr/lib/systemd/system/xochitl.service.d/zz-cangjie-xovi.conf`"，但 `chinese-ime/langhook/deploy/install.sh` 自己的头注写着这个方案**已经在 2026-08-16 因两次真机 dm-verity A/B 回滚变砖而放弃**，现行机制是持久源放 `~/xovi/services/xochitl.service/*.conf`（`/home`），`xovi/start` 遍历这些源目录拷进新挂载的 `/etc/.../xochitl.service.d/` tmpfs 再重启对应 unit——全程不碰 `/usr`。工程纪律 已经改过来（本地文件，未纳入版本控制，这次改动不体现在 git 历史里）。

**方案演进两版**：
1. 第一版（后来整个 revert）：在 `chinese-ime/langhook/hook_init.c` 加运行期开关 `CANGJIE_IME_HOOKS`，`=0` 时跳过拼音输入法/EF/KBS 那 8 个 hook 只留 `hl_expand`。真机验证通过（journal 确认三行关键日志），但用户要求"不动老项目"，整段 revert，`chinese-ime/langhook` 现在跟改动前一个字节不差。
2. 第二版（最终形态）：独立最小扩展 `enhance/hl-snap/`，见 §01 的架构决策。

**真机验证**（第二版）：完整备份 → 移除设备上第一版残留的 `cangjie-langhook.so`+`cangjie-langhook.conf`（两者不能共存，会抢同一个 hook 目标）→ scp+md5 校验部署 `hl-snap.so` → `xovi/start` → journal 确认：
```
[hl-snap] _xovi_shouldLoad: 固件兼容(FUN_00f05ad0@0xf03670) → 加载
[hl-snap] 荧光笔EXPAND hook 安装完成 @ 0xf03670（neuter=1）
```
`/proc/<pid>/maps` 里 `hl-snap` 段数 0→4、`cangjie-langhook` 段数降回 0；`is-active`/`NRestarts`/`MainPID` 健康检查通过（含延迟复查排除慢速崩溃循环）。地址 `0xf03670` 跟第一版真机验证时的地址一致，确认签名在 3.28.0.172 上仍唯一命中，不是固件迁移偏移漂移的问题。

**离线**：`cargo build -p shelf-gateway` 确认 `battop.rs` 注释改动不影响编译（无关但同批做的路径引用更新）；`enhance/hl-snap` 交叉编译零警告；`deploy/install.sh` 过 `shellcheck --severity=warning` 零告警。

## 03b｜`battop/` 搬迁（2026-09-09，纯目录搬家）

`git mv misc/battery-audit/battop enhance/battop`（含 `.cargo/config.toml`/`Cargo.{toml,lock}`/`install.sh`/`src/main.rs`，`target/` 构建产物本来就 gitignore 不用管）+ `git mv misc/battery-audit/FINDINGS.md enhance/battop/FINDINGS.md`。同步更新 4 处仓库路径引用（`xovi-extensions`/`shelf`/`notes` 三本白皮书 + `shelf-gateway::enhance::battop.rs` 模块注释）。`misc/battery-audit/` 下的诊断脚本历史（`bataudit*.sh`/`APP-DESIGN.md`/`battery-audit.sh`）留在原处，理由见 §01。设备端部署路径（`/home/root/battop`）不受影响——那是 `install.sh` 自己的固定拷贝目标，跟仓库里源码目录搬到哪无关。

## 03c｜`handwriting-stroke/` 研究：找到线索，反查 vtable 失败（2026-09-09）

**目标**：CJK 手写笔迹渲染优化——设备手写笔锋按中文书写习惯（运笔粗细/顿挫）渲染优化，**跟 `cardhw`（笔迹→文字 AI 视觉识别）完全无关**，用户第一轮就澄清过这个区分。

**现状确认为全新功能**：全仓库关键词搜索（笔锋/笔画/书写风格/stroke/taper/pen tip/ink width/pressure/brush/calligraphy/手写渲染等）确认没有配置键、没有 hook、没有反编译记录。唯一沾边的"够不够到 xochitl 原生渲染层"先例——笔记页想在背景滤镜层面接近同一层——是**判死**的（C++ `SceneView` tile 增量渲染够不到）。

**第一轮：`strings` 侦察**（没装 Ghidra）。真机 `xochitl`（3.28.0.172）二进制里有一整套 C++ RTTI mangled 名字，命名空间 `Quill::strokev2`——按笔型分光栅化策略类：`FillPencil`（铅笔）、`FillBallpoint`/`FillBallpointAA`（圆珠笔，AA=抗锯齿）、`FillSolid_Opaque_AA`/`FillSolid_Composed_AA`、`FillMaskedEraser`（橡皮擦）、`FillAnts`、`FillShaderAA`。外层包一层 `LerpRaster`（linear-interpolation raster，名字直接暗示"沿路径插值"）或 `MonoRaster`（单色光栅化，无插值）。**判断**（未证实）：这套结构像是"笔画宽度/透明度沿路径插值渲染"的实现层，比笔记页背景滤镜那次判死的层级更底层、也更直接对应这个诉求。

**第二轮：真上 Ghidra，反查 vtable 失败**。装 Ghidra（见 §03d）、真机拉 3.28.0.172 的 `xochitl`、`ghidra-project-328/xochitl_328_analysis.gpr` 完整分析 238 秒完成。写 `ghidra-project-328/scripts/FindQuillStrokeRTTI.java`，用跟 `cj_find_metaobject`（`chinese-ime/langhook/src/hook_init.c`）同一招——直接在内存里搜"字面等于某地址的 8 字节指针值"，不依赖 Ghidra 自动 xref（stripped 二进制，指向这些字符串的指针字段之前从没被识别/定型过，自动 xref 是空的）：

1. 四个候选类名字符串各自唯一命中一次，地址在 `0x16d0818`~`0x16d0d80` 一带的 `.rodata`。
2. 反查"谁指向这段字符串" → 各自唯一命中一次，候选 typeinfo 对象在 `0x16d0850`~`0x16d0db8` 一带。
3. 再反查"谁指向这个 typeinfo 对象" → 各自也唯一命中一次，本以为是 vtable 起始，**dump 出来的头几个"槽位"解出来是 ASCII 文本**（`0x656b6f727473384e` 解出来字面是 `"N8stroke"`，Python 解码核实过，不是误判）——不是函数指针，是**另一段无关的字符串**，只是恰好挨在那个指针字段后面。

**结论**（后来被第三轮推翻）：`.data.rel.ro`/`.rodata` 里这些 typeinfo 相关结构挨得很紧，"指针字段后面紧跟的就是 vtable"这个假设在这里不成立——命中的那个指针字段更可能是别的结构（比如某个更外层类型 `__si_class_type_info`/`__vmi_class_type_info` 的 `base_type` 字段，指向这几个模板类当基类）里的一环，不是这几个类自己的 vtable。**没有找到这几个类的虚函数表，也就还没确认它们是不是走虚函数分发的**——甚至不能排除它们根本没有虚函数（RTTI/typeid 不一定要求多态，模板类被拿去 `typeid()` 比较、丢异常、塞进类型擦除容器都会生成 typeinfo）。脚本化反查路子先停在这，需要转 Ghidra GUI 交互式排查。

**第三轮：Ghidra GUI 人工交互排查，完整定位到具体函数（2026-09-09，同一天）**。用户直接问"为何以前的所有研究都没用 GUI 排查"——如实回答：不是不想用，是没有屏幕/鼠标控制类工具，驱动不了 GUI，只能走 headless 脚本；GUI 阶段改成"远程指导、用户操作截图回传"的协作模式完成。

**起步先踩了个环境坑**：GUI 打开是空白页、没有菜单栏——不是工程损坏，是 Java Swing 在 Wayland 平铺式合成器（用户是 Hyprland/Sway 一类）下的经典渲染问题，`_JAVA_AWT_WM_NONREPARENTING=1` 环境变量修复，重开后正常。另外 Ghidra headless 建的工程 `.gpr` 文件本身是 0 字节属于正常现象（工程元数据实际存在 `.rep/` 目录里），不是文件损坏，走 `File → Open Project` 选中 `.gpr` 本身能正常打开，不用怀疑。

**排查路径**（完整过程/每步截图判断见 `handwriting-stroke/README.md`，这里只记结论）：
1. 从 RTTI name 字符串 `DAT_016d0810` 的**真实 Ghidra xref**（不是脚本裸扫，是分析器自动识别的两处引用）追出第一个具体类：函数 `FUN_00f36d40` 反编译后直接读到 Qt 编译进二进制的源码路径字符串 `/home/runner/work/xochitl/xochitl/src/xofm/libs/sceneview/src/shapesoverlay.cpp` 和方法名 `saveStroke`——**这批调试/异常字符串是 stripped 二进制里比符号表更可靠的类名/函数名来源**，全程靠它们一路挂上名字，不是猜出来的。
2. 顺着 `saveStroke` 所在的 `qt_static_metacall`（moc 生成的方法分派表，`FUN_0087c290`）反查出 `ShapesOverlay`（继承 `QQuickPaintedItem`，QML 类型注册在模块 `com.remarkable` 下）完整类结构；`paint(QPainter*)` 覆写（`FUN_008b0120`）确认只是把内部 `QImage` 缓冲整张 blit 上屏，不含逐点渲染逻辑。
3. `qt_static_metacall` 另一个分派项 `updateImage`（`FUN_008bbb80`）才是真正画像素的入口——反编译里能看到自由手写笔迹分支用 `QPainterPath::toFillPolygon()` 重采样多边形，每个重采样点重新打包回一个 14 字节点结构（字段：x/y 浮点 + 两个 u16×0.25 定点 + 方向角字节 + 压感字节），这个点结构跟前面 `FUN_00f36be0`（`saveStroke` 调试导出用的逐点写文件函数）反解出来的完全一致，两条独立路径互相印证。
4. `updateImage` 调 `FUN_00f3e8a0`——反编译里有条调试日志字符串 `"New StrokeRenderer,"`，直接坐实这是 `StrokeRenderer` 的构造工厂；真正的构造函数 `FUN_00f3dcf0` 是个 2456 字节大对象初始化，内部逐个安装十几个 `&PTR_FUN_016d2xxx` vtable 指针——**这些地址恰好落在第二轮找到的 RTTI 字符串同一片 `.data.rel.ro` 区域**，证实 `StrokeRenderer` 把 `Fill*`/`CoverageBuffer`/`IVaryingsGenerator` 这些类当**内联组合成员**（不是独立 `new` 出来的堆对象）逐个塞进自己内存，vtable 指针构造时直接按值写入。
5. 反过来验证：Ghidra `Show References To` 对这些组合成员的 typeinfo 地址是空的（跟第二轮撞见的问题同类——没有人存一个指向组合成员 typeinfo 的裸指针）。改用 `Search → Memory`（十六进制字节序列搜索，等价于本项目 `cj_find_metaobject` 手法但在 GUI 里能立刻看到命中上下文）直接搜 typeinfo 地址的字面字节，命中了构造函数自己的字段——**这就是第二轮结论错误的原因**：不是这些类没有 vtable，是"反查谁指向 typeinfo"这条路对内联组合成员天生找不到，得反过来从构造函数正向找。
6. 沿着这个新方法，逐层验证出 `strokev2::CoverageBuffer`（像素覆盖率累加）、`strokev2::IVaryingsGenerator<Quill::Varying2D>`（插值生成器接口，用同样的 typeinfo→字节搜索法确认有 `VaryingGenerator_AA`/`VaryingGenerator_WidthLength`/`VaryingGenerator_ThresholdAndWidth` 三个具体实现，`base_type` 字段都指回接口自己的 typeinfo，继承关系交叉验证成立）等真实类名。
7. **定位到 `VaryingGenerator_WidthLength` 的业务方法地址 `0x00f401f0`**（vtable 只有 3 个槽位：析构×2 + 1 个业务方法）。**⚠️ 2026-09-09 晚间勘误**：这里当时只信了反编译 C 代码（`return param_1 * *(float*)(param_2+8);`，看起来是一行线性缩放），没有交叉核实原始汇编——事后按项目"反编译要交叉核实"纪律补查，发现真实签名是 **3 个 float 入参 + 1 个对象指针、产出一对 `{float,float}`**（用了 `[x0+8]` 和 `[x0+0xc]` 两个配置字段，不是一个），反编译器把这个函数简化错了；而且 `getReferencesTo` 显示 `FUN_00f401f0` **从没有被真实调用点引用过**，`FUN_00f3f9d0` 里的宽度公式看起来是内联计算不是在调它——**这个函数是否真的会被真机执行到，静态反编译从来没有坐实过**。"这是最具体的候选改动点"这句断言收回，详见 `handwriting-stroke/README.md`「勘误」一节，改动目标改回确认会执行的 `FUN_00f3f9d0` 内联公式。

**跟第二轮结论的关系，明确写清楚**：§04 原有的"RTTI typeinfo 存在 ≠ 有 vtable"这条踩坑本身没错（判断方法论是对的），错的是第二轮**把这条通用原则套用到具体案例上得出的结论**——没找到 vtable不等于没有 vtable，只能说明当时那个反查手法找不到（详见新增的 §04 踩坑条目）。

**第四轮：headless 脚本接力，往上下游继续挖，定位到完整链路（同一天，用户问"能不能自己挖"之后）**。用户直接问"能不能自己挖"——如实说明：GUI 探索式排查（不知道该往哪查）还是不行，没有屏幕/鼠标控制工具；但**一旦有了具体地址，反编译指定函数/查 xref/按字节搜内存这些操作 headless 脚本 API 都能做**，等价于 GUI 里手动点，不用再靠人工截图——前提是先请用户在 GUI 里 `File → Close Project` 释放 `.lock`（headless 和 GUI 不能同时开同一个工程）。

用这个方式独立挖了三层：
1. `FUN_00f33180`（曾以为是 `param_1` 上游的候选）反编译出来是个通用 `QVector` 插入函数，只做数据搬运不做计算——**排除，不是需要的东西**，如实记录这个岔路而不是悄悄跳过。
2. 改查真正的逐点渲染分派函数 **`FUN_00f3f9d0`**：按笔型标签字节（`*(byte*)(lVar7+0x70)`）分派到不同分支，各自算一遍宽度。最丰富的一支（方向敏感笔型）有两处新发现：**smoothstep 三次缓动曲线**（`(1-(3-2t)t²)*0.2+0.8`，`t` 由点结构宽度字段映射而来，宽度超过阈值后平滑衰减，是目前找到的唯一非线性宽度处理）+ **方向角→`sincosf`**（仅特定标志位开启时计算，笔锋朝向的 sin/cos 参与渲染，典型各向异性笔刷特征）。
3. 顺着这支分支的绘制调用挖到 **`FUN_00f47530`**——变宽笔画的几何生成器：拿"上一点半宽 + 当前点半宽 + 两点连线的垂直方向单位向量"，构造一个梯形的四个角点（两端各自沿垂直方向偏移半宽）。**变宽笔画的几何能力本来就存在（两端半宽天然独立）**。

**第五轮：继续挖到像素填充层，链路彻底打通（同一天，用户要求"直到出结果"）**。梯形算出来之后交给 `FUN_00f37d30`，往下是**四级级联的 Sutherland-Hodgman 多边形裁剪**（`FUN_00f37d30→FUN_00f378e0→FUN_00f376b0→FUN_00f374f0`）：逐条边跟脏矩形/裁剪区做交点线性插值，把梯形切成落在裁剪区内的三角形，全程纯几何、不碰颜色、全是固定代码（非虚函数）。裁剪链**最后一级不再调下一个裁剪函数，改成虚函数间接调用**（`(**(code**)(*(long*)*param_1+0x10))(...)`，`vtable+0x10` 跟全程反复见到的槽位模式一致）——**几何裁剪到此为止，交棒给多态的像素消费者（`CoverageBuffer`/`Fill*`，具体是哪个实例取决于当前笔型选中了哪个组合成员）**，静态反编译单函数看不出运行时指针具体指向谁，这是这条链能挖到的边界，往下需要动态分析。

**完整链路（五层，全部走完）**：`.rm` 点结构 → `FUN_00f3f9d0` 按笔型算宽度（含 smoothstep 缓动曲线+方向角 sincos）→ `FUN_00f47530` 构造变宽梯形几何 → 四级多边形裁剪（固定代码）→ 虚函数分发给像素消费者。具体代码/公式见 `handwriting-stroke/README.md`「宽度插值本体」。

**架构结论**：整条"笔画变像素"的链路是"固定几何算法（宽度插值+变宽四边形+多边形裁剪，全部非虚函数）+ 最后一步虚函数分发给可插拔的像素填充策略"——跟 `StrokeRenderer` 构造函数"组合十几个多态成员、按需切换"的整体设计首尾呼应。

**对诉求本身的结论**：想改"CJK 书写习惯的运笔粗细/顿挫"，现状所有宽度公式都只跟"距离/压感/一条固定缓动曲线"相关，**没有笔画方向（横竖撇捺）相关的权重**——这是最贴近诉求、但也没有现成代码可抄的一层，真要做需要新写逻辑，不是调现有参数就够；改动点全在 `FUN_00f3f9d0` 这一层，几何裁剪+像素分发两层是通用管线，跟笔型无关，不需要动。

**这轮排查（含 GUI 和 headless 两部分）完全没有写过一行实现代码、没有碰过真机**——纯静态反编译分析，虚函数分发的具体目标、smoothstep 曲线值的下游用途、`bVar16` 每个取值对应哪个具体 `Fill*`/`VaryingGenerator_*` 类都还没交叉验证，见 §05。

## 03d｜Ghidra 环境搭建（2026-09-09）

**第一版：手动装 zip**。下载 Ghidra 12.1.3 官方 release 到 `~/.local/share/ghidra`（不进仓库），`analyzeHeadless`/`ghidraRun` 软链进 `~/.local/bin/`。Ghidra 12.x 要求 JDK 21（sdkman 默认是 17），改 Ghidra 自己 `support/launch.properties` 的 `JAVA_HOME_OVERRIDE` 指到 sdkman 的 `21.0.12-tem`，不碰 sdkman 全局默认。

**第二版（最终）：改用 `paru -S ghidra`**。用户已有 sdkman 的 JDK 21，问"paru 如何安装 ghidra 而不安装 java"——CachyOS/Arch 官方仓库有预编译 `ghidra` 包（不是 AUR 源码构建），依赖里的 `java-environment>=21` 是虚拟包，pacman 看不到 sdkman 装的 JDK（不在 pacman 数据库里），会强行拉一份系统 JDK。用 `--assume-installed java-environment=21` 跳过这条依赖检查（`pacman -S --assume-installed java-environment=21 ghidra --print` 空跑确认只装 `ghidra` 本身，不拉 `jdk21-openjdk`）。用户装完后，手动那份 `~/.local/share/ghidra` 删掉，避免两份并存。

**遗留的小麻烦**：pacman 装的 `ghidra` 主目录在 `/opt/ghidra`（root 拥有），`support/launch.properties` 没有 host sudo 权限改不了，不能像手动装那版直接写 `JAVA_HOME_OVERRIDE`。改用 `JAVA_HOME` 环境变量每次调用时指定：
```sh
JAVA_HOME=~/.local/share/sdkman/candidates/java/21.0.12-tem ghidra-analyzeHeadless ...
```
想一劳永逸可以自己跑一次 `sudo sed -i 's/^JAVA_HOME_OVERRIDE=.*/JAVA_HOME_OVERRIDE=.../' /opt/ghidra/support/launch.properties`（需要 sudo，这次没跑，这边没有 host sudo 权限）。

## 04｜踩坑

- **RTTI typeinfo 存在 ≠ 有 vtable**：类被 `typeid()` 用到就会生成 RTTI 元数据，不代表它是多态类型（有虚函数、有 vtable）。反查 vtable 前应该先确认目标类到底有没有虚函数（比如看调用点是不是用了虚函数调用指令，或者干脆看 typeinfo 对象本身的 vtable_ptr 字段指向的是 `__class_type_info`〔无继承，不太可能是这种模式〕还是 `__si_class_type_info`〔单继承〕还是 `__vmi_class_type_info`〔多继承/虚继承〕——这本身就需要先看清楚周围内存布局，鸡生蛋蛋生鸡，说明这类反查天生就不是纯脚本能一遍搞定的，需要人工先建立假设）。
- **stripped 二进制里，指向字符串的指针字段不会自动有 xref**——Ghidra 的引用分析靠"已经被定型为指针的数据"才能建立 xref，没被分析器识别/定型过的原始字节即使内容上是一个合法指针，也不会出现在 `getReferencesTo()` 里。这种情况下退回到"直接在内存里搜字节序列"（本项目 `cj_find_metaobject` 已经验证过的手法）比依赖 Ghidra 自动分析更可靠，但也更容易走偏——"扫到一个指针值"不等于"这个指针值就在我期望的那个结构体字段里"，`.data.rel.ro` 里紧密排列的多个对象会让"反查上一层"这种操作命中一个完全无关的邻居。**扫到命中不代表布局假设是对的，必须验证内容合理性**（这次是靠"dump 出来的槽位内容能不能解出人话"这个笨办法戳破了错误假设——早一点做这个校验能少走一层弯路）。
- **工程纪律 记录会过期，而且过期的可能是"已经放弃的危险方案"**：这次踩到的不是"文档没跟上最新进展"这种常见滞后，而是文档还在推荐一条**已经因为真机变砖两次而被放弃**的路线。这种"过期文档指向危险操作"比"过期文档只是不够新"风险级别高得多，发现了要立刻改，不能当一般的文档债务处理。
- **两个 xovi 扩展抢同一个 hook 目标会冲突**：`hl-snap.so` 和 `chinese-ime/langhook` 的 `cangjie-langhook.so` 都会 patch `FUN_00f05ad0`，同时部署行为未定义。凡是"从老项目里独立拆出一个功能子集"的场景，都要检查新旧两份产物有没有可能同时部署、目标有没有重叠，部署脚本/文档里要把这条互斥关系写清楚（已经在 `hl-snap/README.md` 里记了）。
- **"反查 vtable 找不到"不等于"没有 vtable"，内联组合成员是反查思路的盲区**：§03c 第二轮曾错误地下结论"这几个类可能没有虚函数表"，第三轮证明它们确实有 vtable，只是作为另一个类（`StrokeRenderer`）的内联组合成员出现——vtable 指针是构造函数里按值直接写入对象内存，**没有任何地方存一个指向组合成员 typeinfo 的裸指针**，所以"反查谁指向 typeinfo"这条路对这类结构天生走不通，不是分析深度不够，是方法论本身对不上目标结构的内存布局。遇到反查走不通，先确认目标是不是"独立堆对象"（能反查）还是"别的对象的组合成员"（得反过来从容器对象的构造函数正向找）——这是比第二轮那次更早该做的判断。
- **Ghidra `Show References To` 依赖分析器已识别的 xref，命中为空不代表真的没有引用**——`Show References To` 只查数据库里已经建立的 xref 记录，取决于分析器有没有把引用处认成"指针"类型；分析器没识别到的，即使内存里字面上就是那个地址的字节，也不会出现在结果里。Ghidra `Search → Memory`（十六进制字节序列搜索）是纯字节扫描，不依赖分析器识别，找不到 xref 时应该退回到这个而不是断定"没人引用"。
- **stripped 二进制里，Qt 编译进二进制的调试/异常字符串（源码路径、断言文案、`QMessageLogger::warning` 里的类名/方法名字面量）是比符号表更可靠的类名/函数名来源**——本项目 xochitl 的函数符号表被剥得只剩 `FUN_xxxxx`，`Symbol Tree → Functions` 按类名/方法名搜是空的，但沿着任意一个已知函数反编译读下去，经常能撞见字面写死的源码路径/方法名/日志文案（这次连续撞见 `shapesoverlay.cpp`/`saveStroke`/`updateImage`/`"New StrokeRenderer,"` 四处），是最快的"确认这是哪个类"的手段，应该优先找这类线索，而不是先尝试反查 RTTI/vtable。
- **Java Swing 在 Wayland 平铺式合成器下容易整窗口空白、没有菜单栏**——`_JAVA_AWT_WM_NONREPARENTING=1` 环境变量修复，遇到"GUI 程序打开是白屏"先检查 `$XDG_SESSION_TYPE` 是不是 `wayland`，不用怀疑程序本身或工程文件损坏。Ghidra headless 建的工程 `.gpr` 文件是 0 字节也是正常现象（元数据实际在 `.rep/` 目录里），同理不是文件损坏的信号。
- **反编译 C 代码简化过头、必须交叉核实原始汇编——这次真撞见了，不是纪律走过场**：`VaryingGenerator_WidthLength::generate()`（`FUN_00f401f0`）反编译显示成"1 个 float 入参、一行线性缩放"，原始汇编显示真实签名是"3 个 float 入参+1 个对象指针、产出一对 `{float,float}`"，用了两个配置字段不是一个——反编译器把多寄存器传参/HFA 返回值简化丢了信息。**更严重的次生错误**：当时信了简化后的错误结论，直接写进"最终结论"里当成"最具体的候选改动点"宣称完成，没有先确认这个函数在真实调用链上有没有被引用（`getReferencesTo` 其实早就显示零真实调用者，这个信号当时没重视）。**教训**：反编译输出看着越"干净利落"（比如一行代码）越要留一个心眼——真实硬件计算很少这么巧合地简单，尤其是产出多个值的接口方法，简单到只有一行往往是反编译器漏看了寄存器；关键结论落地前，`getReferencesTo` 显示零调用者这个事实本身就该是暂停信号，不该被"反正找到了对的 vtable"这种部分正确掩盖过去。

## 05｜真机待办

**未闭环**：`handwriting-stroke/` ——完整调用链（`.rm` 点结构 → `FUN_00f3f9d0` 按笔型算宽度〔含 smoothstep 缓动曲线+方向角 sincos〕→ `FUN_00f47530` 构造变宽梯形几何 → 四级多边形裁剪 → 虚函数分发给像素消费者）五层已经全部走完（§03c 第三、四、五轮），**完全没有做过任何 hook/真机验证**，改动会不会真的影响渲染出来的笔迹都还没试过，且虚函数分发的具体目标（哪个 `Fill*`/`CoverageBuffer` 实例）静态反编译到边界了、需要动态分析才能确认。具体待办清单见 `handwriting-stroke/README.md`「下一步」——按项目"先离线摸清楚再写"纪律，动手前还要先打日志只读验证一轮，不能直接写内存。

**已闭环（真机）**：`hl-snap/` 精确吸附 hook（§03a，journal 三行关键日志+健康检查+地址一致性交叉验证）；`battop/` 目录搬迁（§03b，纯文件系统操作，不涉及设备行为变化，不需要真机验证，`cargo build` 确认引用它的 `shelf-gateway` 仍能编译）。

**已放弃**：无（这条线刚开，还没有放弃过的分支）。
