# enhance —— 系统增强线（新开的第七条线，2026-09-09）

跟 `shelf/`、`notes/` 一样是独立的顶层目录/项目线，不嵌进已有的六分块目录里。用户明确要求："CJK 精确吸附不动老项目，在新路径下工作，类似 shelf/"，"也不放在 cang-jie 路径［下随便一个已有目录］，应该是新路径，比如 enhance，里面不仅有精准吸附，还有笔锋和电池刺客等等"，"属于一条新线路，系统增强线"。

⚠️ **命名跟 工程纪律 六分块的「④系统增强」（`xovi-extensions/`）撞了**——那边指的是 reading-qol/font-menu 那一整套设备端 QML/UI 增强；这边（`enhance/`）目前专指"底层、跨块、原来散落各处的单点增强工具"，两者暂时并存，还没有正式在 工程纪律 里理顺关系，先各自独立记录，别混。

## 里面有什么

- **`hl-snap/`** —— 荧光笔 CJK 精确吸附，独立最小 xovi 扩展（一个 `.so`，只装一个 hook）。2026-09-09 从 `chinese-ime/langhook`（4800+ 行的完整中文化线，一堆互不相关的 hook 混一个文件）里逐字节拆出来，跟拼音输入法/UI 汉化完全脱钩。复用 `chinese-ime/langhook/src/{scan,pattern,trampoline_aarch64}.{c,h}` 这三个纯工具文件（特征码扫描 + ARM64 trampoline，路径引用不复制，跟 shelf 的 bookconv 被 reading/device-rs 路径引用是同一个先例），不依赖 `chinese-ime/langhook` 任何跟 IME 相关的代码。构建：`cd hl-snap && make aarch64 XOVI_DIR=<asivery/xovi clone 路径>`。部署：`deploy/install.sh`（vellum-xovi 结构，只碰 `/home`）。
- **`battop/`** —— 电池刺客，电量异常排查用的常驻采样诊断进程（Rust，纯 std 零依赖）。2026-09-09 从 `misc/battery-audit/battop/` 挪过来（`git mv`，历史一并带过来——`FINDINGS.md` 记着 2026-08-29 那次 cgroup/RCU 死锁事故+修复）；`misc/battery-audit/` 下另外几个诊断脚本（`bataudit*.sh`/`APP-DESIGN.md`）是那次事故调查过程的历史遗留，留在原处没跟着搬。
- **`handwriting-stroke/`** —— CJK 手写笔迹渲染优化（笔锋按中文书写习惯运笔粗细/顿挫，**跟 `cardhw` 那条 AI 转写完全无关**），目前只有一份研究现状记录，**没有任何实现代码**——全仓库搜索确认这个功能之前完全不存在，第一手线索（`Quill::strokev2` 光栅化引擎）见 `handwriting-stroke/README.md`；深挖需要 Ghidra，进度见 `ghidra-project-328/`（那个目录不属于 `enhance/`，是共享的逆向基座产物存放处，跟 `ghidra-project/` 并列）。

## 跟其它目录的关系

- `chinese-ime/langhook/` 完全没动——`hl-snap/` 只是路径引用它的三个工具文件，不修改、不移动。
- `misc/battery-audit/battop/` 已经不存在，`git mv` 到了这里；`misc/battery-audit/` 目录本身还在（剩下的诊断脚本历史记录）。
- `shelf/services/shelf-gateway/src/enhance/`（网页「管理→系统增强」面板的后端）是这条线的**消费方**——那边的 `battop.rs` 只是拿设备上已经装好的 `/home/root/battop/battop` 走 `systemctl`，不关心源码在仓库哪个位置；那边的模块注释这次已经把仓库路径引用从 `misc/battery-audit/battop/` 改成 `enhance/battop/`。同一个"enhance"名字是巧合还是有意的呼应——网页那边 2026-09-09 早先就已经在用这个名字组织这三个开关的面板代码，这次顶层目录顺着同一个名字延续下来，算是有意保持一致。
