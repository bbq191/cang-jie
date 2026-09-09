# ghidra-project-328 —— xochitl 3.28.0.172 逆向工程（新开，不动旧的）

**跟 `ghidra-project/` 是两个独立项目，别混**：`ghidra-project/` 是固件 **.169** 时代的分析成果（`xochitl_analysis.gpr`/`.rep` + 一堆 `Decompile*`/`FindXrefs*`/`DumpVtable*` 脚本），函数地址早就对不上现在的固件版本，本目录**不复用、不覆盖**它——新固件重新导入分析，产物落这个新目录，两边各自独立、互不干扰（跟 `shelf/`/`notes/` 各自独立成目录同一个道理）。

## 起因

2026-09-09：研究"CJK 手写笔迹渲染优化"（笔锋按中文书写习惯运笔粗细/顿挫，跟 `cardhw` 那条 AI 转写完全无关）。`strings`/`c++filt` 在真机 `xochitl`（3.28.0.172）二进制上找到一套 C++ RTTI 名字，命名空间 `Quill::strokev2`，按笔型分光栅化策略（`FillPencil`/`FillBallpoint`/`FillBallpointAA`/`FillMaskedEraser`…），外层包 `LerpRaster`（插值光栅化）/`MonoRaster`——这只是字符串层面的侦察，**没有定位到实际代码地址、没验证能不能 hook**，要往下挖必须上 Ghidra。

## 目录约定（照抄 `ghidra-project/` 的成功模式）

- `xochitl_328_analysis.gpr`/`.rep` —— Ghidra 项目本体（`.rep` 是二进制项目库，**gitignore 掉**，只有 `.gpr` 标记文件入库；`.gitignore` 里的 `ghidra-project*/*.rep/`、`ghidra-project*/*.log` 这次顺手改成通配，同时覆盖新旧两个目录，不用每加一个新固件版本目录再补一行）。
- `scripts/` —— headless 分析脚本（Java，Ghidra Script API），一个脚本对应一次探索性任务，命名跟旧项目一个风格（`DecompileXxx.java`/`FindXrefsXxx.java`/`DumpVtableXxx.java`），产出的 `.log` 同样 gitignore、不入库。
- 发现即写文档：探索有结论就更新本 README 或专门的 `*反解发现.md`（照抄 `ghidra-project/SceneLink反解发现.md` 的先例）。

## 现状

2026-09-09：Ghidra 装好了——**改用 `paru -S ghidra`**（CachyOS/Arch 官方仓库有预编译包，不是手动装 zip 那条路了；手动装过一次 `~/.local/share/ghidra`，用户装好 pacman 版后已删掉，避免两份并存）。装的时候用 `--assume-installed` 跳过依赖里的 `java-environment>=21`（已有 sdkman 的 JDK 21，不需要 pacman 再装一份系统 JDK）：

```sh
paru -S ghidra --assume-installed java-environment=21
```

装完二进制在 `/usr/bin/ghidra`/`/usr/bin/ghidra-analyzeHeadless`/`/usr/bin/pyghidra`（PATH 直接能用），主目录 `/opt/ghidra`。**Ghidra 12.x 要求 JDK 21**，sdkman 默认还是 17，`/opt/ghidra/support/launch.properties` 是 root 拥有（pacman 包管理，没有 sudo 权限改不了），不像手动装那版能直接改 `JAVA_HOME_OVERRIDE`——改用 `JAVA_HOME` 环境变量每次调用时指定（`launch.sh` 会读这个变量，优先级仅次于 PATH 上的 `java`）：

```sh
JAVA_HOME=~/.local/share/sdkman/candidates/java/21.0.12-tem ghidra-analyzeHeadless ...
```

嫌每次都要写这串，想要一劳永逸可以自己 `sudo sed -i 's/^JAVA_HOME_OVERRIDE=.*/JAVA_HOME_OVERRIDE=\/home\/afu\/.local\/share\/sdkman\/candidates\/java\/21.0.12-tem/' /opt/ghidra/support/launch.properties`（这一步需要 sudo，这边没有 host sudo 权限，得自己跑）。

还没导入分析。下一步：`scp root@10.11.99.1:/usr/bin/xochitl` 拉当前固件二进制（不进仓库，太大且是 reMarkable 官方二进制，licensing 上也不该入库，跟 `ghidra-project/` 一直以来的做法一致——旧项目同样没把 .169 的 xochitl 本体提交进去，只提交分析脚本和发现文档），`JAVA_HOME=~/.local/share/sdkman/candidates/java/21.0.12-tem ghidra-analyzeHeadless <project_location> xochitl_328_analysis -import <xochitl路径>` 建库，再用 `scripts/` 里的脚本按 RTTI 名字反查 vtable/函数体（复用 `ghidra-project/scripts/DumpVtable*.java`/`FindXrefs*.java` 的套路，不是从零发明）。
