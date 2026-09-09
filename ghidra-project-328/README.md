# ghidra-project-328 —— xochitl 3.28.0.172 逆向工程（新开，不动旧的）

**跟 `ghidra-project/` 是两个独立项目，别混**：`ghidra-project/` 是固件 **.169** 时代的分析成果（`xochitl_analysis.gpr`/`.rep` + 一堆 `Decompile*`/`FindXrefs*`/`DumpVtable*` 脚本），函数地址早就对不上现在的固件版本，本目录**不复用、不覆盖**它——新固件重新导入分析，产物落这个新目录，两边各自独立、互不干扰（跟 `shelf/`/`notes/` 各自独立成目录同一个道理）。

## 起因

2026-09-09：研究"CJK 手写笔迹渲染优化"（笔锋按中文书写习惯运笔粗细/顿挫，跟 `cardhw` 那条 AI 转写完全无关）。`strings`/`c++filt` 在真机 `xochitl`（3.28.0.172）二进制上找到一套 C++ RTTI 名字，命名空间 `Quill::strokev2`，按笔型分光栅化策略（`FillPencil`/`FillBallpoint`/`FillBallpointAA`/`FillMaskedEraser`…），外层包 `LerpRaster`（插值光栅化）/`MonoRaster`——这只是字符串层面的侦察，**没有定位到实际代码地址、没验证能不能 hook**，要往下挖必须上 Ghidra。

## 目录约定（照抄 `ghidra-project/` 的成功模式）

- `xochitl_328_analysis.gpr`/`.rep` —— Ghidra 项目本体（`.rep` 是二进制项目库，**gitignore 掉**，只有 `.gpr` 标记文件入库；`.gitignore` 里的 `ghidra-project*/*.rep/`、`ghidra-project*/*.log` 这次顺手改成通配，同时覆盖新旧两个目录，不用每加一个新固件版本目录再补一行）。
- `scripts/` —— headless 分析脚本（Java，Ghidra Script API），一个脚本对应一次探索性任务，命名跟旧项目一个风格（`DecompileXxx.java`/`FindXrefsXxx.java`/`DumpVtableXxx.java`），产出的 `.log` 同样 gitignore、不入库。
- 发现即写文档：探索有结论就更新本 README 或专门的 `*反解发现.md`（照抄 `ghidra-project/SceneLink反解发现.md` 的先例）。

## 现状

2026-09-09：Ghidra 12.1.3（`~/.local/share/ghidra`，不进仓库——第三方工具装在 host 用户目录，不是项目内容）已装好，`analyzeHeadless`/`ghidraRun` 软链到 `~/.local/bin/`。**Ghidra 12.x 要求 JDK 21**（sdkman 默认是 17），已经在 `~/.local/share/ghidra/support/launch.properties` 里把 `JAVA_HOME_OVERRIDE` 指到 sdkman 的 `21.0.12-tem`，不改 sdkman 全局默认（`java -version` 命令行仍然是 17，只有 Ghidra 自己走 21）。

还没导入分析。下一步：`scp root@10.11.99.1:/usr/bin/xochitl` 拉当前固件二进制（不进仓库，太大且是 reMarkable 官方二进制，licensing 上也不该入库，跟 `ghidra-project/` 一直以来的做法一致——旧项目同样没把 .169 的 xochitl 本体提交进去，只提交分析脚本和发现文档），`analyzeHeadless <project_location> xochitl_328_analysis -import <xochitl路径>` 建库，再用 `scripts/` 里的脚本按 RTTI 名字反查 vtable/函数体（复用 `ghidra-project/scripts/DumpVtable*.java`/`FindXrefs*.java` 的套路，不是从零发明）。
