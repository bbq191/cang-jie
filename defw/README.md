# defw —— xochitl 3.28.0.172 逆向工程（新开，不动旧的；原名 `ghidra-project-328`，2026-09-10 改名）

**跟 `ghidra-project/` 是两个独立项目，别混**：`ghidra-project/` 是固件 **.169** 时代的分析成果（`xochitl_analysis.gpr`/`.rep` + 一堆 `Decompile*`/`FindXrefs*`/`DumpVtable*` 脚本），函数地址早就对不上现在的固件版本，本目录**不复用、不覆盖**它——新固件重新导入分析，产物落这个新目录，两边各自独立、互不干扰（跟 `shelf/`/`notes/` 各自独立成目录同一个道理）。

## 起因

2026-09-09：研究"CJK 手写笔迹渲染优化"（笔锋按中文书写习惯运笔粗细/顿挫，跟 `cardhw` 那条 AI 转写完全无关，详见 `enhance/handwriting-stroke/README.md`）。`strings`/`c++filt` 在真机 `xochitl`（3.28.0.172）二进制上找到一套 C++ RTTI 名字，命名空间 `Quill::strokev2`，按笔型分光栅化策略（`FillPencil`/`FillBallpoint`/`FillBallpointAA`/`FillMaskedEraser`…），外层包 `LerpRaster`（插值光栅化）/`MonoRaster`——这只是字符串层面的侦察，**没有定位到实际代码地址、没验证能不能 hook**，要往下挖必须上 Ghidra。

## 目录约定（照抄 `ghidra-project/` 的成功模式）

- `xochitl_328_analysis.gpr`/`.rep` —— Ghidra 项目本体（`.rep` 是二进制项目库，**gitignore 掉**，只有 `.gpr` 标记文件入库；`.gitignore` 里原来 `ghidra-project*/*.rep/`、`ghidra-project*/*.log` 那组通配是给 `ghidra-project/`（.169）+ 改名前的这个目录共用的，改名成 `defw/` 后通配符不再匹配，2026-09-10 补了 `defw/*.rep`/`defw/*.lock`/`defw/*.log` 单独一组，不是覆盖旧规则）。
- `scripts/` —— headless 分析脚本（Java，Ghidra Script API），一个脚本对应一次探索性任务，命名跟旧项目一个风格（`DecompileXxx.java`/`FindXrefsXxx.java`/`DumpVtableXxx.java`），产出的 `.log` 同样 gitignore、不入库。
- 发现即写文档：探索有结论就更新本 README 或专门的 `*反解发现.md`（照抄 `ghidra-project/SceneLink反解发现.md` 的先例）。

## 环境搭建（2026-09-09，一次性，供以后重建环境参考）

Ghidra 走 **`paru -S ghidra`**（CachyOS/Arch 官方仓库有预编译包，不是手动装 zip 那条路；手动装过一次 `~/.local/share/ghidra`，用户装好 pacman 版后已删掉，避免两份并存）。装的时候用 `--assume-installed` 跳过依赖里的 `java-environment>=21`（已有 sdkman 的 JDK 21，不需要 pacman 再装一份系统 JDK）：

```sh
paru -S ghidra --assume-installed java-environment=21
```

装完二进制在 `/usr/bin/ghidra`/`/usr/bin/ghidra-analyzeHeadless`/`/usr/bin/pyghidra`（PATH 直接能用），主目录 `/opt/ghidra`。**Ghidra 12.x 要求 JDK 21**，sdkman 默认还是 17，`/opt/ghidra/support/launch.properties` 是 root 拥有（pacman 包管理，没有 sudo 权限改不了），不像手动装那版能直接改 `JAVA_HOME_OVERRIDE`——改用 `JAVA_HOME` 环境变量每次调用时指定（`launch.sh` 会读这个变量，优先级仅次于 PATH 上的 `java`）：

```sh
JAVA_HOME=~/.local/share/sdkman/candidates/java/21.0.12-tem ghidra-analyzeHeadless ...
```

嫌每次都要写这串，想要一劳永逸可以自己 `sudo sed -i 's/^JAVA_HOME_OVERRIDE=.*/JAVA_HOME_OVERRIDE=\/home\/afu\/.local\/share\/sdkman\/candidates\/java\/21.0.12-tem/' /opt/ghidra/support/launch.properties`（这一步需要 sudo，这边没有 host sudo 权限，得自己跑）。

`xochitl` 二进制不进仓库（太大且是 reMarkable 官方二进制，licensing 上也不该入库，跟 `ghidra-project/` 一直以来的做法一致——旧项目同样没把 .169 的 xochitl 本体提交进去，只提交分析脚本和发现文档），`scp root@10.11.99.1:/usr/bin/xochitl` 现拉，`JAVA_HOME=~/.local/share/sdkman/candidates/java/21.0.12-tem ghidra-analyzeHeadless <project_location> xochitl_328_analysis -import <xochitl路径>` 建库。

## 现状（2026-09-10 更新：已经不是"还没导入分析"了）

这个目录**已经真机验证跑通过完整闭环**——不只是建了库，是用它支撑了 `enhance/handwriting-stroke/` 从"零线索"到"两个 hook 目标真机部署验证"的全过程（笔尖角度模型+提按速度代理，见 `enhance/handwriting-stroke/README.md`）。方法论：字符串侦察（`strings`/`c++filt` 找 RTTI 名字）→ GUI 探索式排查（Ghidra CodeBrowser 人工点，用于"完全不知道往哪查"的阶段）→ headless 脚本接力（一旦有具体地址，反编译/查 xref/搜内存都能用 `scripts/` 下的脚本批量做，不用继续截图）。`scripts/` 下现在是**可复用的通用工具**（`DecompileTargets.java` 改个地址数组重跑就能反编译任意函数+查调用者、`CheckFuncSizes.java` 核对反编译结果跟原始反汇编是否一致，防止反编译器简化过头——这条真的救过一次，见系统增强白皮书 §04），不是一次性脚本用完就扔。完整的反解发现记录（架构结论/踩坑/勘误）在 `enhance/handwriting-stroke/README.md` 和 `enhance/docs/reMarkable系统增强线白皮书.md` §03c-§03f，**这个目录本身只管"怎么用这套工具"，不重复记录发现内容**。
