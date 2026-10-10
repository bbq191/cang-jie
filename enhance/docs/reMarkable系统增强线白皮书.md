# reMarkable 系统增强线（enhance）白皮书

> **这篇讲什么**：原理、决策、真机验证和踩坑。每个工具怎么装、怎么用，看各自的 README。
>
> **给谁看、先看哪几节**
>
> - 只想知道"现在有哪些工具、怎么开关"：读 §00b 一节就够。
> - 要改 xovi 扩展或 qmd 补丁：§02（扩展怎么生效）、§04（尤其「qmd 补丁怎么离线验证」「hook 安全性」）。
> - 关心整机耗电：§03j（设备空闲时谁在叫醒 CPU）。
> - 想知道最近改了什么、哪些还没手测：§03p（10-09 第六轮审计，含「已知风险」）、§03o、§03n 和 §05。
>
> **§ 编号固定不变**（代码注释和别的文档按编号引用），所以章节顺序和编号不完全对应：现状在 §00b，原理从 §02 起，各工具与历轮改动 §03a–§03p，踩坑 §04，待办 §05。
>
> **验证程度的写法**：「真机」= 在设备上跑过；「已部署」= 装上了设备、部署自检通过，但功能本身没手测；「开发机」= 只有 host 测试。
>
> **2026-09-30 起，手写优化（hw-stroke）和电池刺客（battop）已从仓库和设备上移除**（§03n）。§03b、§03c、§03e–§03g 是它们的历史记录，已压缩成要点；其中 §03c 的逆向结论（xochitl 怎么画笔画）本身仍然成立。

## 读这篇你能得到什么

| 你想知道 | 读 |
|---|---|
| 这条线有哪些工具、各自现在什么状态、怎么开关 | §00b |
| 几个术语（xovi、hook、特征码、trampoline…）是什么意思 | §00b 末尾「术语速查」 |
| 网页开关写到哪里、怎么确认扩展真的生效了 | §02 |
| 荧光笔"划哪吸哪"怎么修的 | §03a |
| 阅读器单击翻页怎么做的（日漫翻页规则 2026-10-07 删除） | §03i |
| 字体、壁纸服务和 lo-alias 在这条线里的位置 | §03h |
| 设备空闲时谁在定时把 CPU 叫醒（整套设备，不只本线） | §03j |
| 电池刺客、手写优化为什么没了、旧设备怎么清 | §03n |
| （历史）电池刺客与两次冻机；手写笔锋的逆向与实现 | §03b；§03c → §03e → §03f → §03g |
| 历轮审计（09-24 / 09-25 / 09-30 / 10-09）给本线改了什么 | §03k / §03l / §03m / §03p |
| 两个 C 扩展的三处隐患（10-09 已修并部署）怎么修的、真机验证到哪一步 | §03p「已知风险」 |
| 改 qmd 补丁后怎么离线验证（为什么不能用 `check-compatibility`） | §04「qmd 补丁怎么离线验证」 |
| 改了扩展为什么要整机重启、不能只重启 xochitl | §04「hook 安全性」末条 |
| 为什么单点工具要单独成线 | §00、§01 |

## 00b｜现状总览（先读这个）

![enhance 的工具怎么接到设备上](diagrams/enhance-overview.svg)

这条线是一组**互相独立的单点增强工具**，都不修改 xochitl（reMarkable 自带的阅读/笔记程序）本身。设备固件 3.28.0.172。现役的是下面六样：

| 工具 | 是什么 | 状态 | 开关 |
|---|---|---|---|
| **hl-snap**（[README](../hl-snap/README.md)） | xovi 扩展 `hl-snap.so`：荧光笔划中文时划哪高亮哪，不再整行吸附；hook 一个函数（§03a） | ✅ 真机通，日常在用；10-09 补了两处防御性修复（§03p），已部署，吸附功能本身未再手测 | 「管理 → 系统增强」，写 `hlSnapCjk`，**默认开**，下一次划线即生效 |
| **reader-page-turn**（§03i） | qmd 补丁 `reader-page-turn.qmd`（源码在 `shelf/xovi/`，随 book 服务安装）：单击屏幕左右各 7% 边缘翻页 | ✅ 离线 + 真机验证（09-24）；10-07 删掉日漫翻页规则：离线验证、已部署，功能未手测 | 「管理 → 系统增强」，写 `tapPageTurn`，**默认关**，重新打开书生效 |
| **ui-font**（[README](../ui-font/README.md)，§03o） | xovi 扩展 `ui-font.so` + qmd `ui-font-tokens.qmd` + font-serve 的界面字体仓库：把 xochitl **界面**的字体换成上传的字体，阅读器字体不变 | ✅ 真机目测通过（10-07：书库、设置、对话框、标题都换成更纱，阅读不变）；10-09 加的防递归检查已部署，当前固件触发不到 | 「其他 → xochitl → 界面字体」，整机重启生效 |
| **wallpaper-serve**（[README](../wallpaper-serve/README.md)） | Web 服务（`127.0.0.1:8793`）：上传即用的休眠壁纸，每次休眠后轮换；靠 xochitl 隐藏配置键 `SleepScreenPath` | ✅ 真机通 | 「其他 → 壁纸」 |
| **font-serve**（路由见 [本线 README](../README.md)；原理在书架白皮书） | Web 服务（`127.0.0.1:8792`）：上传字体即装进 fontconfig，维护中文回退链；10-07 起也管界面字体 | ✅ 真机通 | 「其他 → xochitl」，重开字体菜单即可选 |
| **lo-alias**（[README](../lo-alias/README.md)） | 小脚本：让 `10.11.99.1` 在不插 USB 时也可达（网关启动前调用） | ✅ 真机通（09-25 无 USB 冷启动核对） | 无 |

另有共用件 [`shared/`](../shared/PROVENANCE.md)：xovi 扩展用的特征码扫描 + trampoline 代码，编进 `hl-snap.so`，不单独部署（`ui-font.so` 不用这部分，改的是导入表，§03o）；10-10 起两个扩展读配置用的极简 JSON 读取 `minijson` 也在这里（§03q）。

**已移除**（§03n）：手写优化 hw-stroke（xovi 扩展，按笔尖角度和运笔速度调笔画粗细）、电池刺客 battop（按进程/唤醒源统计耗电的采样服务）。2026-09-30 用户要求移除，源码、部署脚本、网页开关和数据页一并删除，想看旧代码去 git 历史里找 2026-09-30 删除之前的版本。

**设备现状**（截至 2026-10-09）：

- 09-29 按用户要求卸载了 appload、KOReader、第三方 WeRead 和侧栏入口，本线各工具只服务 xochitl。
- 09-30 两次部署：第五轮审计版（§03m）、移除 hw-stroke / battop（§03n，`install-all.sh` 自动清掉设备残留）。
- 10-07 两次部署：删日漫翻页规则（§03i）、界面字体 ui-font（§03o），都整机重启、部署自检通过。
- 10-09 13:48 部署第六轮审计（§03p）：壁纸、字体两个服务，以及修了三处隐患的两个扩展；整机重启，部署自检 38✓ 1⚠（刚开机）0✗。真机 `/proc/<pid>/maps` 里 hl-snap 改过的那页已恢复成 `r-xp`（§04「多扩展共存」）；荧光笔吸附、壁纸、字体这些功能本身**没有手测**。
- `extensions.d/` 现在是 `hl-snap.so`、`ui-font.so` 和 `qt-resource-rebuilder.so`（10-09 16:33 `ls` 核对过）；**没有** `cangjie-langhook.so`。10-09 13:48 部署上去的 `hl-snap.so` md5 是 `0e1fe44b…`、`ui-font.so` 是 `21cda291…`（修隐患后重编；部署脚本推送时逐个核对 md5）；同日又补了调用桩分配失败时恢复页权限，`hl-snap.so` 变成 `f7261e7f…`，16:32 部署上机（整机重启后核对过设备上的 md5，xochitl 已载入、hook 安装日志正常）。09-25～10-07 的 `hl-snap.so` 是 `7ca1985b…`。

**还没做的**（详见 §05）：§03m、§03p、§03i（删日漫）里"部署后确认"的功能项还没逐项手测；§03p 三处 C 隐患的修复里，只有"代码页恢复 r-x"在真机上看到了，另两处只做过离线反汇编和 host 单测。

### 术语速查

| 术语 | 意思 |
|---|---|
| xochitl | reMarkable 自带的主程序（书库、阅读器、笔记本都是它） |
| xovi | 第三方扩展加载框架。靠 systemd drop-in 用 `LD_PRELOAD` 把 `xovi.so` 带进 xochitl，再加载 `~/xovi/extensions.d/` 下的每个文件 |
| xovi 扩展 | xovi 加载的 `.so`，导出 `_xovi_shouldLoad`（要不要加载）和 `_xovi_construct`（加载后做什么） |
| hook | 把某个函数的入口改成先跳到我们自己的代码（handler），再决定调不调原函数 |
| 特征码 | 目标函数开头的一串原始机器码。按"代码长什么样"找函数，而不是写死地址；换固件后找不到就自动不加载 |
| trampoline / 调用桩 | 被覆盖的原函数开头字节 + 一条跳回原函数的指令，放在新分配的内存里，让 handler 还能调用原函数 |
| `FUN_00xxxxxx` | Ghidra（逆向工具）给无符号函数起的名字，数字是地址 |
| qmd | qt-resource-rebuilder 的 QML 补丁文件，xochitl 启动时读一次，用来改界面 |
| `reading-qol.json` | `~/.local/share/cangjie-ime/reading-qol.json`，几个开关共用的配置文件（目录名是历史遗留） |
| 待换入区 | `~/.cangjie-stage/so-pending/`。部署时 xochitl 正在用旧版 `.so`，新版先放这里，整机重启时换进 `extensions.d/` |
| 整机重启 | 让扩展和 qmd 生效的唯一方式（2026-09-25 起）；单独重启 xochitl 有概率在它退出时崩溃（§04） |

## 00｜定位与原则

**起因**：2026-09-09 用户反馈"划线没有按 CJK 精确吸附"，查下去是完整中文化扩展 `cangjie-langhook.so` 整个从设备上消失了（§03a）。第一版修复在老项目 `chinese-ime/langhook` 里加运行期开关；用户纠正了三轮：不动老项目、另起一个能装下这类单点工具的顶层项目线、定名 `enhance/`，和 `shelf/`、`notes/` 并列。

**三条原则**（在纠正过程中形成）：

1. **单点工具不塞进已有大项目**：哪怕只有几十行，只要能独立部署、有独立生命周期，就独立成目录。
2. **不对接已移走的旧路径**：需要老项目的代码就拷贝一份独立维护（`shared/`、`lo-alias/` 都是这样来的）。
3. **一份产物两种用法，别搞两份构建**：要功能子集优先用运行期开关，而不是编译期 `#ifdef` 或构建变体。

## 01｜架构决策

**为什么 hl-snap 整个独立，而不是给 langhook 加开关？** 加开关技术上可行、也真机验证过，但源码仍绑在一个 4800 多行、混着拼音输入法等一堆 hook 的文件里，改老项目随时可能波及吸附这个独立诉求。`hl-snap/` 现在是独立的 `.c` + 元数据 + `Makefile` + 部署脚本，产物名和加载判据都不同。

**为什么 hl-snap 用自己的特征码当加载判据？** langhook 里所有 hook 能不能装，先看 `setLanguageCode` 的特征码在不在（"总闸"）。这对输入法合理，对只管吸附的扩展不合理：`hl-snap.so` 装不装只该取决于它要 patch 的 `FUN_00f05ad0` 还在不在。

**为什么字体、壁纸服务也归这里？** 概念上都是"跨块的单点增强工具"，只是历史上先存在于别处（`shelf/services/`）；已移除的 battop 当时也是从 `misc/` 搬来的。搬家不改运行时行为，见附录。

**命名遗留**：旧 `xovi-extensions/`（设备端 QML/UI 增强，reading-qol / font-menu）已移出仓库，和本线没有从属关系。若以后捞回来，设想按"它管 QML/UI 层、`enhance/` 管更底层的单点工具"分工——这只是设想，没拍板，动这条边界要先问用户。

## 02｜网页开关、xovi 加载机制与"开关≠已生效"

![网页开关 → 落到设备上的什么 → 什么时候生效](diagrams/enhance-switches.svg)

**分工**：`enhance/` 管工具本身怎么实现、怎么部署；[`gateway/src/enhance/`](../../gateway/src/enhance/) 管网页上怎么远程控制它们。两边没有代码依赖，只靠约定的文件路径和 systemd 单元名对接：

| 开关 | 网页位置 | 落到哪里 |
|---|---|---|
| CJK 荧光笔精确吸附 | 管理 → 系统增强 | `reading-qol.json` 的 `hlSnapCjk`（默认开） |
| 单击翻页 | 管理 → 系统增强 | `reading-qol.json` 的 `tapPageTurn`（默认关），`reader-page-turn.qmd` 每次打开书读一次（§03i）。`rtlPageTurn`（日漫翻页规则）2026-10-07 删除，旧键留在文件里无人读 |

同一个 `/api/enhance/*` 接口还管着「管理 → 实验室」里的「导入 md 文档」开关，它属于笔记线（「漫画页边距最小化」开关 2026-10-07 删除：带 sheng-ren 页边距标记的漫画加入 xochitl 后一律在首次打开时设页边距 1，不带标记的书不碰，见书架传书线架构 §3；旧 `comicMinMargin` 键原样留着、无人再读），见[网关白皮书](../../gateway/docs/reMarkable网关白皮书.md) §06。2026-09-30 前这里还有「电池刺客」启停和「CJK 手写笔迹优化」两个开关，随功能移除；旧设备 `reading-qol.json` 里的 `hwStroke*` 键会原样留着（见下一段"全量写回"），没有程序再读它们。

**全量写回**：`reading-qol.json` 是多方共享的文件（网关、旧原生设置页、C 扩展、qmd 都读，前三者会写）。`gateway/src/enhance/qol.rs` 把整份文件当不透明 JSON 读进来、只改要改的键，不认识的键原样写回，并在进程内串行化，避免冲掉别处写的开关。（`qol.rs` 头注说的"系统增强白皮书 §08「全量防覆盖」"是已移出仓库的旧系统增强白皮书，规则就是这一段。）

### xovi 扩展怎么生效

![xovi 扩展从加载到 hook 生效](diagrams/xovi-hook-lifecycle.svg)

扩展都走这套流程（现在只有 hl-snap），代码在 `shared/`：

1. xovi 在 xochitl 启动时加载 `extensions.d/` 下的每个文件——所以备份绝不能留在这个目录。
2. `_xovi_shouldLoad` 在 xochitl 代码段里搜特征码，恰好命中 1 处才加载；换了固件、函数变了，就不加载，xochitl 按原生行为跑。这是主要的 fail-safe。
3. `_xovi_construct` 再搜一次拿地址，用 trampoline 改写目标函数开头 20 字节。改写的前提是这 20 字节里没有分支和 PC 相对寻址（搬进调用桩后会算错），每个候选都要逐条反汇编核对（§04）；10-10 起 `cj_patch_target` 自己也会检查，命中就放弃 hook（§03q）。
4. 运行时每次调用先进 handler，读开关决定改不改；配置缺失或越界就用默认值。

### 开关≠已生效（2026-09-24 加）

开关只写配置；扩展根本没加载时，开了也没用。历史上两次"开关看着开了、其实没生效"：09-09 langhook 整个从设备上消失（§03a）；hw-stroke 因 GLIBC 版本不符静默加载失败（§04）。现在网页直接显示：

- 扩展开关旁有「已加载 / 未加载」徽章；「管理 → 基石」另列一行"xochitl 里生效的扩展"。
- 数据来自 `gateway/src/enhance/loaded.rs`：找 `comm == xochitl` 且父进程是 1 的主进程（排除渲染 PDF 时 fork 出的同名子进程），读它的 `/proc/<pid>/maps`，映射了哪个 `extensions.d/*.so` 就算已加载；映射了 `xovi.so` 说明 xovi 生效。
- **徽章的边界**：它只证明 `.so` 进了进程（走完了 `_xovi_shouldLoad`），不证明 hook 装上了；后者看 journal 里的"hook 安装完成"。
- 漫画页边距靠 qmd 补丁 `shelf-comic-margins.qmd`，不是 `.so`（2026-10-07 起实验室里的开关卡片删除，下面这套加载判定当时用于那张卡片的徽章）。判定：qt-resource-rebuilder.so 在主进程里、补丁文件在它的 exthome、且修改时间早于 xochitl 启动时间（`/proc/<pid>/stat` 第 22 列 + `/proc/stat` 的 btime）算已载入；文件比进程新报「待重启」。这是按加载机制推断，看不到补丁里的 LOCATE 是否全部命中。
- 「导入 md 文档」只控制网页子标签，标「网页功能」，没有加载这回事。

## 03a｜hl-snap：荧光笔 CJK 精确吸附（2026-09-09，真机通）

**病根**：xochitl 划线后调 `FUN_00f05ad0`（3.28.0.172 上在 `0xf03670`）把选区向两边扩张到整个"词"，分词靠空格；中文没有空格，于是一小段扩成整行。修法：hook 这个函数，选区第一个字是 CJK 时不调原函数。逻辑逐字节来自 langhook 里 2026-08-23 真机验证过的那段代码。

**怎么查出来的**：`journalctl -u xochitl` 搜 `cangjie` **零命中**——正常加载会打一串初始化日志，一行都没有说明扩展压根没被加载，而不是"装了但 hook 没生效"。`find` 全设备也找不到 `cangjie-langhook.so`，`~/.local/share/cangjie-ime/` 下 5 个词典也没了，只剩 `reading-qol.json`（"裸机恢复"类重置的典型后果，见 [`../../docs/INSTALL.md`](../../docs/INSTALL.md) OTA 一节）。

**顺带发现一条危险的过期记录**：当时的项目说明还写着往 `/usr/lib/systemd/system/xochitl.service.d/` 放 drop-in 做持久化，而这条路早在 2026-08-16 因两次 dm-verity A/B 回滚变砖放弃了。现行机制：持久源放 `~/xovi/services/xochitl.service/*.conf`，`xovi/start` 把它们拷进 `/etc` 下的 tmpfs 再重启 unit，全程不碰 `/usr`。

**方案两版**：第一版在 langhook 加 `CANGJIE_IME_HOOKS=0` 开关（真机通，但用户要求不动老项目，整段撤回）；第二版是现在的独立扩展（§01）。

**真机验证**（第二版）：备份 → 移除设备上第一版残留的 langhook（两者抢同一个目标）→ scp + md5 → 重启 → journal 出现 `_xovi_shouldLoad: 固件兼容(FUN_00f05ad0@0xf03670)` 与 `荧光笔EXPAND hook 安装完成 @ 0xf03670`；`/proc/<pid>/maps` 里 `hl-snap` 出现、`cangjie-langhook` 消失；`is-active` / `NRestarts` / `MainPID` 健康检查（含延迟复查）通过。地址与第一版一致，说明特征码在 3.28.0.172 上唯一命中。2026-09-24 只读复核：仍是同一地址、hook 装上。

**和 langhook 同时装会怎样**：按代码看是"先到先得"：两个扩展在 `_xovi_shouldLoad` 阶段都看到原始机器码、都同意加载；先执行 `_xovi_construct` 的那个改写了函数开头，后一个再扫时特征码对不上，静默放弃这个 hook。langhook 一侧源码已不在仓库，这一半**未核对**；两者同时装也没有真机测过。结论：两者不要同时装（langhook 自带同样的修复）。

## 03b｜（历史）battop 电池刺客：两次冻机与教训（2026-09-30 已移除）

battop 早于这条线存在（08-27 电池审计后建的长期耗电追踪工具），09-09 从 `misc/battery-audit/battop/` 搬进来。它留给这条线最重要的东西是教训：**一个诊断工具怎么差点变成故障源**。

- **08-29 第一次冻机**：屏幕冻住、ping/SSH 全不通、USB 链路仍在，长按电源 25–30 秒才恢复。内核日志坐实：systemd 启动 battop 的 oneshot 服务时把进程写进 cgroup，`synchronize_rcu` 撞上内核 RCU stall，连 PID 1 一起卡死。不是 battop 逻辑 bug，但 timer 每 10 分钟拉起一次 = 每天 144 次 cgroup 迁移，把罕见 stall 的暴露面放大了 144 倍。改成常驻服务，每次开机只迁一次。
- **09-20 定案"只 start、不 enable"**：原先 enable 链接在 `/etc` tmpfs、重启即清，"重启后不自启"一直是事实上的缓解，此后写成明确设计。
- **09-23 第二次冻机**（常驻模型下）：冻结前最后一条日志与 battop 每小时 fork 一次 `journalctl` 刷新唤醒源的时间精确重合到秒（只有时间吻合，没有内核栈证据）。唤醒源改读 `/dev/kmsg`，采样循环里不再创建子进程。
- 09-22 至 09-30 又经过三轮审计（拆模块、流式聚合、按文件缓存、唤醒时间按单调时钟换算等），都只在 host 验证。
- **09-30 移除**（§03n）。两次冻机的内核根因（RCU stall）至今没有排除。

**教训**：诊断工具自己也要当成"可能出事的代码"看——常驻、周期性、在内核敏感路径附近（cgroup、fork）的操作要尽量去掉；"重启后自然关闭"这种安全态要写成明确设计，而不是依赖 tmpfs 碰巧被清。旧的电池审计报告与冻机内核证据在 `enhance/battop/FINDINGS.md`，已随删除提交移出，看 git 历史。

## 03c｜（历史）hw-stroke 逆向：xochitl 怎么画笔画（2026-09-09，纯静态分析）

> 手写优化扩展 hw-stroke 2026-09-30 已移除（§03n）。本节的渲染链结论是对固件 3.28.0.172 的逆向结果，**本身仍然成立**，以后要改笔画渲染可以直接用；§03e–§03g 是在它上面做的实现，只留要点。

**目标**：让笔锋按中文书写习惯（粗细、顿挫）渲染。**和 AI 手写识别完全无关**，用户第一轮就澄清过。

**渲染链**（五层；真实类名来自 Qt 编译进二进制的调试字符串，与 Ghidra 反编译交叉验证）：

![笔画渲染链与 hook 位置（历史）](diagrams/handwriting-render-chain.svg)

- 入口：`ShapesOverlay`（继承 `QQuickPaintedItem`，源码路径字符串 `.../src/xofm/libs/sceneview/src/shapesoverlay.cpp`）的 `updateImage`（`FUN_008bbb80`）才真正画像素；`paint()` 只是把内部 `QImage` 整张贴上屏。
- `updateImage` 把手写笔迹用 `QPainterPath::toFillPolygon()` 重采样，每个点重新打包成 14 字节点结构，交给 `StrokeRenderer`（构造函数 `FUN_00f3dcf0`，把 `CoverageBuffer`、`IVaryingsGenerator`、`LerpRaster<Fill*>` 等十几个多态成员**内联组合**进自己）。
- `FUN_00f3f9d0` 逐点分派：按笔型标签 `bVar16` 分支各算宽度；再交给变宽几何生成器（`FUN_00f47530` 等：上一点半宽 + 当前点半宽 + 连线的垂直单位向量 → 梯形四角点）；然后四级 Sutherland-Hodgman 多边形裁剪（`FUN_00f37d30 → 00f378e0 → 00f376b0 → 00f374f0`）；最后经虚函数 `vtable+0x10` 交给多态像素消费者（`CoverageBuffer` / `Fill*`）。

**14 字节点结构**（`FUN_00f36be0` 反解，独立验证过；和 `.rm` 文件里的点字段**没做过对拍**）：

| 偏移 | 类型 | 换算 | 字段 |
|---|---|---|---|
| `0x0` / `0x4` | float | 原样 | x / y |
| `0x8` / `0xA` | u16 | ×0.25 | 两个同类宽度/速度定点字段 |
| `0xC` | u8 | ×2π/255 | 方向角 |
| `0xD` | u8 | ÷255 | 压感（0~1） |

**五轮排查**：

| 轮 | 手段 | 结论 |
|---|---|---|
| 1 | `strings` 侦察 | 真机 xochitl 有整套 `Quill::strokev2` 的 RTTI：按笔型分的光栅化策略类（`FillPencil` / `FillBallpoint[AA]` / `FillSolid_*_AA` / …），判断是"宽度沿路径插值"的实现层 |
| 2 | Ghidra 脚本反查 vtable | 失败：反查到的"vtable"槽位解出来是 ASCII 文本。当时误判"没有虚函数"，被第 3 轮推翻（§04） |
| 3 | Ghidra GUI 人工交互（用户截图回传） | 从 RTTI 字符串的真实引用追到 `saveStroke` / `ShapesOverlay` → `updateImage` → `StrokeRenderer` 构造函数。第 2 轮失败的原因：内联组合成员的 vtable 指针是构造时按值写入的，反查天生走不通 |
| 4 | headless 脚本接力 | 定位逐点分派 `FUN_00f3f9d0` 和变宽几何生成器 `FUN_00f47530`——**变宽笔画的几何能力本来就存在**。排除岔路：`FUN_00f33180` 只是通用 `QVector` 插入 |
| 5 | 挖到像素层 | 四级裁剪是固定代码，最后一级改虚函数调用交给像素消费者。静态分析到此为边界 |

**结论**：整条链是"固定几何算法（宽度插值 + 变宽四边形 + 裁剪，全部非虚函数）+ 最后一步虚函数分发给可插拔的像素填充策略"。宽度公式只和距离、压感、一条固定缓动曲线有关，没有"笔画方向"权重；改动点全在 `FUN_00f3f9d0` 这一层及其下游几何生成器。

**勘误（09-09 晚）**：第 3 轮曾把 `FUN_00f401f0`（`VaryingGenerator_WidthLength` 的业务方法）当"最具体候选"，反编译显示它只是一行线性缩放。按"反编译要交叉核实原始汇编"纪律补查：真实签名是 3 个 float 入参 + 1 个对象指针、产出一对 float；更关键的是它**在整个二进制里没有真实调用者**。断言收回，改用确认在调用链上的 `FUN_00f47530`。

## 03d｜Ghidra 环境（2026-09-09）

装法、`JAVA_HOME`（Ghidra 12.x 要 JDK 21）、怎么拉 xochitl 二进制、headless 怎么调，都在 [`../../defw/README.md`](../../defw/README.md)。取舍留一条：改用发行版包（`paru -S ghidra --assume-installed java-environment=21`，避免多装一份系统 JDK），每次调用用 `JAVA_HOME` 指定（`/opt/ghidra` 是 root 拥有，改不了 `launch.properties`）。Wayland 下 GUI 空白见 §04。

## 03e｜（历史）hw-stroke 第一版（2026-09-10）

- **hook 目标** `FUN_00f47530`：两个 float + 一个指针，入口第一件事就读"当前点宽度"（`ctx+4`）算半宽，在这里改值最简单可靠。先部署只读日志的诊断 hook 证明宽度随笔压连续变化，再加倍率开关，截图确认笔画明显变粗/变细。
- **笔尖角度模型**：采用 Illustrator/Inkscape 书法笔刷同款公式 `宽度 ×= min_ratio + (1-min_ratio)×|sin(运笔方向角−笔尖角度)|`，运笔方向用 hook 自己维护的"上一点坐标"（`ctx+0x48/0x4c`）现算。效果强度跟基础宽度挂钩：细笔画趋近关闭、粗笔画满强度。
- **撤回一次**：尝试读 `ctx+0x70` 当笔型标签排除钢笔，真机读出的值在 0~255 随机跳——这条路径上它是无关内存（§04「hook 安全性」第一条）。

## 03f｜（历史）压感撤回、运笔速度代理、第二个 hook（2026-09-10）

- **真实压感放弃**：压感字节是真数据，但 `FUN_00f3f9d0` 有 6 个调用点，实时预览和提交走不同路径，跨函数传值拿不到（诊断样本 2/3449 对得上）。改用 hook 内就能算的**运笔速度代理**：相邻两点距离短（慢、顿笔）→ 粗，长（快、带过）→ 细。
- **第二个 hook**：真机发现 `FUN_00f47530` **只有书法笔会走到**，钢笔、铅笔、马克笔完全摸不到。反编译 `bVar16==3/5/6` 三个分支：`FUN_00f4c8d0`（6 直接调、5 间接调）前 20 字节纯栈操作 → 采用；`FUN_00f4f430`（3）第 3 条指令是条件分支 → 不能安全 patch。部署后命中数近 5 倍，按真机数据校准到 `min_ratio=0.6`，用户反馈"看上去还行"。最常用的一档钢笔/铅笔（`bVar16<4`，走虚函数动态分发）到移除时也没摸到。

## 03g｜（历史）hw-stroke 降负载与"换 .so 后 restart → 整机重启"（2026-09-24）

- **降负载**：hook 原先每个点都 `fopen` 读一次配置、写一行 stderr（进 journal，又被别的进程逐行读），效果关着也照样执行。改为每一笔起点读一次配置、逐点日志和诊断 hook 默认关（`hwStrokeDebug`）；真机手写后相关日志 0 行。
- **部署时第二次踩到"换 .so 后 restart → 整机重启"**（第一次是 09-21 appload）：运行中的 xochitl 映射着旧 `.so`，换完文件再 `restart`，旧进程退出时 SEGV，`OnFailure=emergency` 触发整机重启。当天改成 stop → 换 → start；**09-25 又推翻**：停止 xochitl 本身就可能崩，与换没换 `.so` 无关，现在一律换入后整机重启（§04「hook 安全性」末条）。

## 03h｜字体、壁纸服务与 lo-alias（概要）

这三样 2026-09-11 才归入本线（概念归类，运行时行为不变），原理主要记在别处：

| 组件 | 是什么 | 详细记录 |
|---|---|---|
| `font-serve`（8792） | 上传字体装进 fontconfig 用户目录，重写 `fonts.json` 给字体菜单 qmd 读，动态维护中文回退链（全 `weak`，所选字体永远优先）；09-25 起开机时索引与字体目录一致就直接复用、`fonts.conf` 内容不变不重写（§03l）；10-07 起另管界面字体（`fonts/shelf-ui/` + `ui-font.json`，§03o）；10-09 起 `fc-cache` 只扫用户字体目录、`fc-scan`/`fc-cache` 带超时（§03p，已部署） | 书架白皮书第 F 章、§03k、§03bd |
| `wallpaper-serve`（8793） | 写 xochitl 隐藏键 `SleepScreenPath` 指向 `current.png`；xochitl 休眠时读完它就轮换（inotify，空闲零唤醒）；入池时等比放大后居中裁成 954×1696（10-09 起先裁再缩放，§03p）；旧 bind-mount 方案已退役 | [wallpaper-serve README](../wallpaper-serve/README.md)（机制与历次改动的权威描述）；书架白皮书 §03w / §03x / §03ab |
| `lo-alias.sh` | 给 `lo` 和 `usb1` 挂 `10.11.99.1`，让不插 USB 时 xochitl 的 :80 上传口仍可达；网关 `ExecStartPre` 调用 | [lo-alias README](../lo-alias/README.md) |

两个服务都依赖 [`../../rmsvc-core`](../../rmsvc-core/README.md)，由网关反向代理，随 `install-all.sh` 的 shelf 步安装。host 测试（2026-10-10 实跑）：font-serve 16 项、wallpaper-serve 16 项。

## 03i｜reader-page-turn：单击翻页（2026-09-24；日漫翻页规则 2026-10-07 删除）

![xochitl 阅读器翻页](diagrams/reader-page-turn.svg)

> **现状（2026-10-07 起，已部署并整机重启，部署自检通过，功能未手测）**：只剩**单击翻页**（`tapPageTurn`）。「日漫翻页规则」（`rtlPageTurn`）整个删掉——用户定书架只管入库、翻页方向交给书本身。xochitl 自己不看 OPF 的 `page-progression-direction`，所以**日漫在 xochitl 里一律从左往右翻**（用户已知悉）。删掉的有：qmd 里的 `cjRtl` 属性、开书查方向、`nextPageGesture`/`prevPageGesture` 的滑动对调；book-serve `GET /reading-direction/{uuid}`；shelf-conv `epub_is_rtl`/`spine_is_rtl`；网关与网页的开关（单独传 `rtlPageTurn` 回 400，`/api/enhance/status` 不再返回它）。`reading-qol.json` 里的旧 `rtlPageTurn` 键不清，无人再读。改了 qmd，部署时要整机重启一次（§04）；10-07 部署后 journal 有 `CJ-PAGE-TURN: loaded`、没有 qmd 解析报错。

**需求**：用户要"单击翻页"回来（08-14 做过 `tap-page-turn.qmd` 并真机验证，09-11 随 `xovi-extensions/reading-qol/` 移出仓库，设备上的 qmd 也已不在，只剩 `reading-qol.json` 里一个 `tapPageTurn`）；另外问漫画能不能"从左往右滑是下一页"——指 **xochitl**（KOReader 已于 2026-09-29 从设备卸载，现在只剩 xochitl 一个阅读器）。后一项就是 2026-09-24～10-07 的日漫翻页规则。

**做法**（现状）：一个 qmd 改三个 QML，锚点全部从设备 .172 的 xochitl 二进制里解出真实 QML 核对过：

- `DeviceSceneView.qml` 的 `FocusScope#root` 加 `cjTapPageTurn` 属性（这个对象就是手势文件里的 `view`，`view: root` 实例化）。
- `DocumentView.qml`：书一换，300 ms 后同步读 `reading-qol.json` 取 `tapPageTurn`。**每次打开书只读一次**——08 月旧版每 1.5 秒轮询一次配置，费电，这次不再轮询，代价是改开关要重新打开书。
- `SceneViewGestures.qml`：在单击 `touchClick.onClick` 函数体开头插入翻页分支（守卫沿用旧版：链接按下、文本编辑、缩放、笔记页不接管）。

**区域**（用户定）：左右各 7% 宽，纵向只在屏幕高度 45%–80% 之间；比 08 月旧版（10%、25%–85%）更窄更低，进一步避开顶部工具栏、底部进度条和握持的四角。

**已知限制**：改开关要重新打开书；只影响 xochitl；日漫不会从右往左翻（见上面现状）。

**历史：日漫翻页规则（2026-09-24～2026-10-07）**
- 做法：`DeviceSceneView` 另有 `cjRtl` 属性；`DocumentView` 开书时清掉 `cjRtl`，开了日漫开关就异步问 book-serve `GET /reading-direction/<uuid>`，书没换才生效；`SceneViewGestures` 在左滑 `nextPageGesture` / 右滑 `prevPageGesture` 的 `onActiveChanged` 开头插入"`cjRtl` 时反向翻页并 return"，单击左边缘也算下一页。
- 怎么判"从右往左"：EPUB OPF 的 `<spine page-progression-direction="rtl">`（book-serve 只读 container.xml 和 OPF 两个 zip 条目，按（大小, mtime）缓存；09-24 设备书库 58 本 EPUB 里 7 本带这个标记），或 uuid 在手动清单 `~/.local/state/shelf/books/rtl-overrides.json` 里。calibre 转出的漫画大多不写这个标记（《亂馬½ 典藏版》4–9 卷、《镖人》2–5 卷、東立版《火影》8–10 卷都没有，Kmoe 版都写了），所以 09-24 加了手动清单，当天写进 13 本；09-30 起清单只读。
- 母版库按书指定方向：09-25 加（优化时写进 OPF、已加入的书顺手写进/移出清单，真机《乱马》11/12 卷生效），09-30 用户定移除。
- 删除前（10-07）核对设备上的手动清单：里面 15 个 uuid 都已不在 xochitl 书库里，删掉没影响任何现有的书；文件还在设备上，无人再读。
- 真机：09-24 离线 + 真机验证通过，09-25《乱马》11 卷首次打开时日志 `CJ-PAGE-TURN: rtl book`。

**离线验证**（09-24 首版）：本机重编 qmldiff（`asivery/qmldiff`），把解出的三份 .172 QML 放到 hashtab 里的真实资源路径下跑 `apply-diffs`（不能用 `check-compatibility` 代替，见 §04）：三个 AFFECT 都应用，四处插入位置核对正确；`qmllint` 补丁前后报错数一致（DocumentView 原版就有 9 条"找不到设备私有模块"类报错）。

**离线验证**（10-07 删日漫分支后）：同样从设备 .172 xochitl 二进制解出真实 QML，`qmldiff apply-diffs` 三个 AFFECT 都应用；新旧补丁的输出对比，差异正好是删掉的日漫代码；qmllint 无语法错误，告警数下降（DocumentView 602→599、SceneViewGestures 180→173、DeviceSceneView 92→92）。未上真机。

**真机验证（09-24）**：部署后 journal 有 `CJ-PAGE-TURN: loaded`；打开书读到开关（`cfg tap=… rtl=…`）；《亂馬》《镖人》、Kmoe《火影》被识别为从右往左（`rtl book <uuid>`），普通书不识别；单击左右边缘翻页命中。之后（第三轮审计）去掉了单击/滑动命中时的日志——每写一行 journal 都会唤醒飞行记录仪之类的日志读者；关书时不再读配置。

## 03j｜设备常驻唤醒源一览（2026-09-30 按代码核对）

![设备常驻唤醒源一览](diagrams/wake-sources.svg)

用户关心整机耗电，所以把"设备空闲、不开网页时，谁会定时把 CPU 叫醒"集中列在这里。范围是**整套设备端组件**（不只本线），数字都取自代码常量，**没有在真机上实测唤醒次数**；用户态的 sleep/定时器用单调时钟，设备休眠时不走，所以下面都是"醒着时"的频率。

| 来源 | 属于 | 空闲时怎么醒 | 代码出处 |
|---|---|---|---|
| wifi-watch | packaging | 每 15 秒 `sleep` 一次看链路；链路好只读 sysfs、不 fork，每 40 轮（10 分钟）兜底复查一次频段/省电设置；09-28 起连上新网络后探一次外网，通了不再探，不通时每 10 分钟再探 | `packaging/wifi-watch/wifi-watch.sh` `INTERVAL`/`RECHECK`/`PROBE_URL` |
| 服务间事件流心跳 | rmsvc-core | 网关订阅 6 个有 `/events` 的服务（book、font、wallpaper、ink、transcribe、note；`gateway/src/manage.rs` 的 `MODULES` 里 `events: true` 的项），transcribe 再订阅 ink，共 7 条 loopback 流，各 120 秒一次心跳（09-30 前还订阅 koreader-serve，共 8 条） | `rmsvc-core/src/events.rs` `FOLLOW_KEEPALIVE_SECS` |
| shelf-mkdir-agent.qmd | shelf | 长轮询 `GET /mkdir/pending?wait=290`：空闲约 290 秒一次往返（book-serve 上限 300 秒；09-24 前 25 秒）；若真遇到 30 秒客户端超时自动退回 25 秒 | `shelf/xovi/shelf-mkdir-agent.qmd`；`book-serve` `MKDIR_WAIT_MAX_SECS` |
| 网关 mDNS | rmsvc-core | 09-25 起空闲零定时唤醒：改听内核 netlink 地址变化，地址增删才重扫（此前 60 秒一次）；netlink 打不开才退回 60 秒；局域网别的设备发 mDNS 查询另算 | `rmsvc-core/src/mdns.rs` `AddrWatch` / `RESCAN_INTERVAL` |
| wallpaper-serve | 本线 | inotify 等 xochitl 休眠时读完 `current.png`：空闲零唤醒，每次休眠醒一次（09-24 前常驻 `journalctl -f -u xochitl`，xochitl 每写一行日志就醒一次；10-10 起监听经 `rmsvc_core::fswatch::watch_with`，开发机实测空闲 30 秒该线程 0 次上下文切换，改前也是 0） | `enhance/wallpaper-serve/src/wake.rs` |
| hl-snap | 本线 | 没有定时器，只在划线时进 handler | `enhance/hl-snap/src/hl_snap.c` |
| reader-page-turn.qmd | 本线（源码在 shelf） | 打开书时单发 300 ms 读一次开关，不轮询 | `shelf/xovi/reader-page-turn.qmd` |
| 其余 qmd 与服务 | shelf / notes | comic-margins 换文档单发 1.5 秒；trash-agent 与 mkdir-agent 同为 290 秒长轮询（09-25 起；book-serve 不在时出错重试 15→30→60→120 秒封顶）；book-serve / ink-serve 用 inotify 防抖（8 秒 / 4 秒）；浏览器事件流 20 秒心跳只在网页开着时有 | 各自源码 |

**结论**：空闲时的定时唤醒主要是 wifi-watch（约 240 次/时）和 7 条事件流心跳（约 210 次/时）；mkdir-agent、trash-agent 两个长轮询各约 12 次/时。飞行记录仪不在设备上：它跑在宿主机，经 ssh 抓设备日志（接着电脑时才有），但本线仍尽量压低 xochitl 日志量（§03i 去掉命中日志）。已移除的 battop 原本默认不跑、开着时每 600 秒采样一次，现在这一项不存在了。

**mkdir-agent 放宽的依据**（09-24）：设备 Qt 6.10.3；qtdeclarative 6.10 的 `qqmlxmlhttprequest.cpp` 不设传输超时、XHR 也没有 timeout 属性；`QNetworkAccessManager` 缺省超时为 0（禁用）。xochitl 导入了 `setTransferTimeout`，但没有证据表明它作用在 QML 引擎的 NAM 上，所以 qmd 加了兜底：请求在 28–33 秒之间失败就当作客户端超时，退回 wait=25 并打一行 `SHELF-MKDIR: transfer timeout` 日志。**09-24 真机**：部署后与整机重启后各跑了数分钟，journal 里都没有这行，290 秒长等待生效。

## 03k｜第三轮审计给本线的改动（2026-09-24，当天部署）

当天已部署并真机验证：两个 `.so` 走 stop → 换 → start 换入成功（三个 hook 装上；这套换入方式 09-25 已改为整机重启，见 §04）；壁纸改监听休眠读图后，休眠那一刻即轮换。

| 改动 | 为什么 | 验证 |
|---|---|---|
| `shared/scan.c` 合并被 `mprotect` 切开的代码段 | 消除多扩展的加载顺序依赖（§04） | host 真内核单测复现切段 |
| 扩展 `_xovi_construct` 找不到映射或特征码时打日志 | 以前静默返回，网页徽章显示"已加载"却没 hook | 交叉编译零警告 |
| `shared/pattern.c` 精确匹配先 `memchr` 找首字节 | 扩展按 `-O0` 编译，加载时要扫整个代码段多遍；16 MB 扫描 39.6 → 3.2 ms | 与逐字节循环 400 组差分对拍，ASan/UBSan 通过；`memchr` 是 `GLIBC_2.17` 符号 |
| 扩展 Makefile：胶水只在缺失时生成，`make glue` 显式重生成 | 没有 xovi clone 也能编，不再悄悄退回旧 `.so`（§04） | 重编 `.so`，动态符号最高仍 `GLIBC_2.17` |
| wallpaper-serve 单图池唤醒不重写 `current.png` | 只传过一张是常态，每次唤醒白写 1–3 MB 闪存 | 新增单图池用例 |
| reader-page-turn.qmd 去掉单击/滑动命中日志、关书时不读配置 | 每行 journal 都会唤醒日志读者（§03j） | 只删日志、加一处提前返回，没有单独再验 |

同轮还给当时的 battop（summary 按文件缓存，34.7 → 4.0 ms/轮）和 hw-stroke（安装提示）改过，随它们 09-30 移除。

## 03l｜第四轮审计给本线的改动（2026-09-25）

写这节时全部只在 host 上测过（font-serve 10 项、wallpaper-serve 10 项 `cargo test`，`shared` `make test`）；这批代码此后已随 09-25 与 09-30 的部署上了设备，但下面「部署后确认」各项还没有逐项手测。扩展 `.so` 源码没改。

| 改动 | 为什么 | 验证 |
|---|---|---|
| font-serve 开机时 `fonts.json` 与字体目录一致（文件集合相同、目录与各文件都不比索引新）就直接用，不再全量重扫；`fonts.conf` 内容不变不重写 | 此前每次开机把每个字体整文件读一遍、各 fork 一次 `fc-scan`（中文字体十几到几十 MB），正赶上 xochitl 起界面；无条件重写 `fonts.conf` 还会让 fontconfig 使用方重载配置 | 新增用例：复用 / 目录多文件 / 删文件 / 索引损坏各走对分支 |
| font-serve / wallpaper-serve 的上传暂存从运行时目录改到 `~/.local/state/shelf/upload`（/home），启动时清掉上次中途被杀留下的 `.part`；字体安装直接改名，不再拷贝 | 运行时目录可能落到 `/tmp`（tmpfs），几十 MB 的字体整份占内存（改动在 `rmsvc-core`，见基座白皮书 §02） | rmsvc-core 单测 |
| wallpaper-serve 同一次休眠去重改按含休眠的开机时长（`/proc/uptime`） | 原用单调时钟：休眠几小时后醒来、10 秒内又休眠，这次读图被当成重复读跳过 | 单测；监听出错的 5 秒→5 分钟退避在一次监听正常跑满 5 分钟后复位 |
| mkdir / trash 两个代理 qmd 出错指数退避（15→30→60→120 秒封顶）；comic-margins 建出 `EpubProperties` 后立即排 5 秒销毁 | book-serve 不在时此前固定 15 秒重试，每个代理每小时白醒 240 次；后者防异常时对象常驻 | `apply-diffs` 只在带同名锚点的**桩 QML** 上跑过，不是真实 .172 QML（§04） |

同轮给 battop 修过唤醒源时间换算（kmsg 时间戳不含休眠）和 dm-verity 下的安装器行为，随 battop 09-30 移除；讲换算的那张图也一并删了。

**部署后确认**（还没做）：`systemctl restart font-serve` 后 journal 里"索引 N 个家族"秒回、`~/.config/fontconfig/fonts.conf` 的 mtime 不变、`~/.local/state/shelf/upload/` 存在且上传字体后不留文件；xochitl 日志里 `SHELF-MKDIR` / `SHELF-TRASH` 行为照常（停掉 book-serve 时代理不再每 15 秒重试）。

## 03m｜第五轮审计给本线的改动（2026-09-30，已部署）

**09-30 14:10 已部署**（`deploy.sh` → 整机重启 → `verify-on-device.sh` 38✓ 1⚠（刚开机）0✗），部署自检通过，下表各项**功能待手测**。host 测试：font-serve 10 项、wallpaper-serve 11 项 `cargo test` 通过；`hl-snap.so` 源码没改，md5 不变。

| 改动 | 为什么 | 验证 |
|---|---|---|
| wallpaper-serve：监听到 `IN_IGNORED` 就返回错误，交给已有的退避重试循环重建目录和监听（`wake.rs::watch_once`） | 壁纸目录被删或所在文件系统被卸载时，内核自动撤掉监听、只发一条 `IN_IGNORED`，之后再无事件；旧代码照常 `continue`，轮换线程永远卡在一个死掉的监听上，之后再也不换壁纸，也不报错 | 新增真 inotify 用例：删掉目录后 `watch_once` 5 秒内返回"已失效" |
| hl-snap 的 `make clean` 只删 `.so`，不再删已提交的 `xovi_glue.{c,h}` | 删了之后本机没有 asivery/xovi clone 就再也编不出来（和 §04「提交进仓库的生成文件」同一类坑） | 读 Makefile 核对 |
| font-serve 上传回执去掉"KOReader 请到 KOReader 页单独上传" | KOReader 与 koreader-serve 已退役，网页里也没有 KOReader 页了 | — |
| `shelf-mkdir-agent.qmd` 的 `catch` 打一行 `SHELF-MKDIR: failed <错误>`；它和 `font-menu-dynamic.qmd` 的头注去掉退役引用（**选择器没改**） | 此前回复不是合法 JSON 或 `createCollection` 抛错时静默吞掉，journal 里看不出建文件夹为什么没成 | 只改注释和一行日志，没有另跑 qmldiff |
| 发现：`qmldiff check-compatibility` 基本不做校验 | 见 §04「qmd 补丁怎么离线验证」 | 开发机实测 |

同轮还有 battop「应用」视图归组、hw-stroke 的 `make clean`，随它们当天移除。

**部署后确认**（待手测）：壁纸照常在休眠时轮换；xochitl 里建文件夹失败时 journal 能看到 `SHELF-MKDIR: failed`。

## 03n｜移除手写优化与电池刺客（2026-09-30，已部署）

**起因**：用户要求"移除电池刺客/手写优化及其开关相关功能和模块"。

**删了什么**：

| 层 | 手写优化（hw-stroke） | 电池刺客（battop） |
|---|---|---|
| 源码 | `enhance/handwriting-stroke/`（`hw_stroke.c`、`hw-stroke.so`、xovi 胶水、安装包装） | `enhance/battop/`（Rust 采样服务、测试、`FINDINGS.md`、`history/` 旧脚本） |
| 网关 / 网页 | 实验室「CJK 手写笔迹优化」开关；`PUT /api/enhance/qol` 不再认 `hwStrokeEnabled`（单独传回 400），`/api/enhance/status` 不再返回它 | `gateway/src/enhance/battop.rs`、`/api/enhance/battop/*`；「系统增强」里的开关卡、运行时才出现的「电池刺客」数据页；中英语言包各 30 个键 |
| 安装 | `deploy-handwriting-stroke.sh`；步骤移出 `STEP_ORDER`，进 `STEP_RETIRED` | `deploy-battop.sh`；同左；`shelf/build.sh` 不再编它；CI 不再跑它的 `cargo test` |
| 核对 | `verify-on-device.sh` 不再当在位项；残留报 ⚠ 并给清理命令 | 同左 |

**没删的**：`shared/` 仍被 hl-snap 编进 `hl-snap.so`，没有只给 hw-stroke 用的文件；源码里提到 `hw_stroke.c` 的注释也不改——扩展带 `-g` 编译，动注释会改变调试行号，进而改变 `hl-snap.so` 的 md5（改完重编核对过：`7ca1985b…` 不变）。`reading-qol.json` 里的 `hwStroke*` 键不清（§02"全量写回"，留着无害）。

**旧设备怎么清**：

![重跑 install-all 时怎么自动清掉已移除功能的残留](diagrams/removal-cleanup.svg)

- **自动**：重跑 `sh packaging/install-all.sh <host>`。在最后一步 `xovi-apply` 之前，它对两样各跑一次清理（`packaging/removal.sh`，与 `uninstall-all.sh` 同一份代码）：
  - 电池刺客：删 `/usr` 的 `battop.service` 和旧版 `battop.timer`（走 devlib 的 dm-verity 门 + 限时读写窗口），删 `/home/root/battop` 整个目录；
  - 手写优化：从 `extensions.d/` 摘掉 `hw-stroke.so` 和 `.crashed` 标记，撤掉待换入区里的副本，清安装包目录 `/home/root/hw-stroke/`；xochitl 当时还加载着它就记一个待生效标记，`xovi-apply` 因此整机重启**一次**。
  - 全程不 stop / restart xochitl、xovi 已生效时不跑 `xovi/start`、不往 `extensions.d` 放备份；没有残留时什么都不动（不碰 systemd、不 remount）。`--skip battop` / `--skip handwriting-stroke` 可跳过。
- **手动（只清这两样）**：

  ```sh
  cd packaging
  sh uninstall-all.sh <host> --dry-run --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,shelf,sidebar-entry   # 计划里应只有 handwriting-stroke、battop
  sh uninstall-all.sh <host> --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,shelf,sidebar-entry
  ```

  然后在设备上整机重启一次（`reboot`）。这个 `--skip` 列表由 `lib.sh` 的 `uninstall_only_skip battop handwriting-stroke` 生成，`verify-on-device.sh` 报 ⚠ 时给的就是它。
- dm-verity 开着时：battop 单元删不掉，它的目录也保留（单元指向其中的程序；battop 从不开机自启），等可写后再跑一次；hw-stroke 只在 `/home`，照常清。
- 留意：`--skip handwriting-stroke` 时，待换入区里如果还有旧的 `hw-stroke.so` 副本，最后的 `xovi-apply` 会照常把它换进 `extensions.d`（跳过就是整样不碰）。只有 09-24 前后部署过 hw-stroke、之后一直没重启过的设备才会有这个副本。

**为什么选"重新部署时自动清"**：两样的清理都复用了已有、已真机走过的机制（卸 `/usr` 单元的读写窗口、摘 `.so` + 待生效标记 + 整机重启），没有新的高风险动作；让用户记得单独跑一条卸载命令，残留更可能一直留在设备上（hw-stroke 还会每次开机被加载）。sidebar-entry 没有并进自动清理：它会删 `cangjie-icons.rcc`，历史上别的 qmd 也用过这个文件名。

**验证**：

- 开发机：网关 `cargo test`、clippy、前端 node 测试与浏览器冒烟通过；`hl-snap.so` 重编 md5 不变；`packaging/tests/run_sim_tests.sh` 356 项全过（旧设备手动卸载、`install-all` 自动清并只整机重启一次、`--skip`、xovi 未生效、dm-verity、目录是符号链接、verify 报 ⚠ 等用例）。
- **真机（09-30 15:23 部署）**：`install-all.sh` 自动清掉了设备上的 battop（单元 + `/home/root/battop`）和 `hw-stroke.so`，整机重启一次（只一次）；`verify-on-device.sh` 36✓ 1⚠（刚开机）0✗，比上一次少的 2 项就是这两样不再检查；`/usr` 单元 12/12；xochitl 的 maps 里已没有 hw-stroke；日志无 warning 以上。原定的"部署后确认"各项都在这次部署自检里覆盖了。

## 03o｜界面字体 ui-font（2026-10-07，已部署）

![界面字体：只换 xochitl 界面，不碰阅读](diagrams/ui-font.svg)

**起因**：用户要求"按我电脑系统的中/英文字体设置改 xochitl 的系统字体，加前端入口，但不要把它用于阅读字体"。电脑（niri）的界面字体是 `gsettings font-name` = **Sarasa UI SC**（更纱黑体 UI：拉丁用 Inter、汉字用思源黑体），fontconfig 里 `sans-serif` 的中英文都落到它。

**xochitl 的界面字体从哪来**（从设备 .172 的 xochitl 解出全部 505 个 QML 核对）：
- **Ark 设计令牌**：`/ark-imports/ark/tokens/Style.qml`（源文件名 `rM-paper-tablet.json.qml`）是单例，36 个 QML 经 `root.typography.fontFamily` 取字体——正文 39 处写死 `"reMarkable Sans"`，标题（`title.lg/md_alt/x2l/xl` 两组）8 处 `"reMarkable Serif Small"`。
- **应用默认字体**：老界面（`device.ui.text` 等）的文字不写字体名，用 `QGuiApplication::font()`；xochitl 二进制导入了 `QGuiApplication::setFont`，`.rodata` 里 `reMarkable Sans` 只出现一次，字体本身是 qrc 里的 `:reMarkableSans-*.ttf`（`addApplicationFont` 注册，fontconfig 看不见，设备上 `fc-match "reMarkable Sans"` 落到 Noto Sans）。
- **阅读器**：EPUB 的字体由阅读器菜单（`FormatFont.qml` 的 `fontModel`：reMarkable Sans / reMarkable Serif Small / EB Garamond / Ebook default + font-serve 装的字体）选，`epubProperties.fontName` 传给渲染器，不读令牌。

**做法**（三块读同一个选择文件 `~/.local/share/shelf/ui-font.json`，只在 xochitl 启动时读一次）：

| 部件 | 怎么改 | 验证 |
|---|---|---|
| `shelf/xovi/ui-font-tokens.qmd` | `AFFECT REBUILD` 整份令牌文件：先 `REPLACE` 两个字面量为 `root.shelfUiSans` / `root.shelfUiSerif`，再在 `id: root` 后插入这两个属性（默认原生名）和 `Component.onCompleted` 同步 XHR 读选择文件 | 离线 `apply-diffs`：39 + 8 处替换；补丁后的单例在本机 Qt 6.11 实跑：有配置读到、没文件保持原生 |
| `enhance/ui-font`（xovi 扩展） | 改 xochitl 导入表（`.got.plt`）里 `QGuiApplication::setFont` 那一格；收到的家族名是 `reMarkable Sans` 且选了 `sans` 时，拷一份 QFont、`setFamily` 后再调原函数（字重字号保留），否则原样放行 | host：导入表改写在懒绑定 / BIND_NOW × PIE / 非 PIE 四种链接下都通过；在本机真实 Qt 6 上走 setFont 替换（含非 ASCII 家族名、没选、没文件、别的家族） |
| font-serve 界面字体仓库 | `fonts/shelf-ui/` 子目录 + `shelf/ui-fonts.json` 清单；`GET/POST /ui`、`DELETE /ui/{family}`、`PUT /ui/select`；只能选已装的界面字体；回执带"是否待整机重启"（与本次开机时读到的选择比较） | 13 项 `cargo test`（新增界面仓库隔离、保底规则、选择校验） |

**为什么不 patch setFont 本体**：它开头第 3 条就是 PC 相对的 `adrp`（`libQt6Gui.so.6` 6.10.3 反汇编），`shared/` 的跳板会把这条搬走、地址算错。改导入表一格不搬指令；槽位运行时按符号名从 `.rela.plt` 找（.172 上在 `0x1a60b68`，不在 RELRO 里，xochitl 没开 BIND_NOW）。原函数用 `dlsym` 地址调，不能用槽里的旧值：懒绑定时那是 PLT 解析桩，调一次就会被解析器写回真实地址、把我们挤掉。C 直接调 Qt 的 C++ 函数：QFont 16 字节在栈上留 64 字节；QString 24 字节返回值走 x8（AAPCS64 对超 16 字节结构体与 C++ 非平凡返回值同样走 x8）；`QString::fromUtf8(QByteArrayView)` 的 `{m_size, m_data}` 按值放 x0/x1。

**为什么要保底规则**（阅读不变的关键）：界面字体放在 fontconfig 扫得到的位置，xochitl 才能按名字用它；但阅读的中文缺字回退（阅读字体是 reMarkable Serif 这类不带汉字的字体时）由 fontconfig 排序。设备上用独立的临时 fontconfig 配置实测（不影响运行中的 xochitl，测完删除）：只多一个 Sarasa UI SC，`fc-match -s "reMarkable Serif Small:lang=zh-cn"` 与 `:charset=4e2d` 都把它排第一——等于成了阅读的中文字体。加一条 `<match target="pattern"><edit name="family" mode="append" binding="weak">Noto Sans SC</edit></match>` 后：无 lang 时 Noto Sans → Noto Sans SC → Sarasa；有 zh-cn 或汉字字符集时 Noto Sans SC 第一；明确点名 Sarasa UI SC 时仍是它。append 不动前面已有的顺序，所以拉丁字、你选的阅读字体、已有的中文回退链都不受影响。font-serve 只在界面字体目录里有字体时写这条规则，删光就撤。顺带的变化：没传 lang 的汉字回退原来排在 Noto Sans JP（日文字形）前面，现在是 Noto Sans SC。

**界面字体文件**：Sarasa 发行包是 TTC（每个字重一个约 85MB 的集合）；用 fontTools 从 Regular / SemiBold / Bold 三个 TTC 里取出 `Sarasa UI SC`（第 7 个字面）存成三个 TTF，各约 45MB。令牌里的 `Font.Medium` 由 Qt 就近选字重。

**生效**：两处都只在启动时读选择，改完要整机重启（§04）；网页改完显示"整机重启后生效"。

**部署**（10-07 已部署并整机重启，部署自检 38✓ 1⚠（刚开机）0✗）：`deploy.sh --only font`（网关、font-serve、两个字体 qmd）→ `DEFER_XOVI_START=1 deploy-ui-font.sh` → 经 SSH 隧道直连 font-serve 上传三个 TTF（约 135MB，14 秒）并 `PUT /ui/select` 正文与标题都选 `Sarasa UI SC`（用户定：与电脑界面一致）→ `deploy-xovi-apply.sh`。设备上核对：
- xochitl 日志：`[ui-font] 安装完成（setFont 导入槽 0x1a60b68）`、`[ui-font] setFont(reMarkable Sans) → Sarasa UI SC`（扩展赶在 xochitl 调 setFont 之前装好了）、`SHELF-UI-FONT: sans=Sarasa UI SC serif=Sarasa UI SC`；xochitl `NRestarts=0`；没有新的 QML 报错（唯一一行 `Experimental.qml:71: ReferenceError: Values` 上次开机就有）。
- xochitl 拉起的短命子进程也会被 xovi 加载扩展，两个扩展都在 `_xovi_shouldLoad` 拒绝（hl-snap 打"找不到 xochitl 映射"，ui-font 打"不导入 setFont"）——正常。ui-font 这句原来写成"（未知固件）"，不准确，之后改了措辞；10-07 部署的 `.so` 是 `7199073b…`，新措辞随 10-09 那次部署（连同 §03p 的防递归检查）上了设备。
- 阅读侧：`fonts.json` 仍为空（菜单里没有更纱）；真实配置下 `fc-match -s "reMarkable Serif Small:lang=zh-cn"` 第一是 Noto Sans SC、更纱排第三；`fonts.conf` 末尾有保底规则。
- xochitl 部署后 VmRSS 约 224MB（没有部署前的对照数，字体是 mmap，按用到的页计）。

**真机验证**（10-07，用户目测）：书库、设置、对话框、标题都已是更纱黑体，没有报告漏掉的界面文字；阅读时书里的中文仍是原来的字体（阅读不受影响）。

## 03p｜第六轮审计给本线的改动与已知风险（2026-10-09，已部署）

**10-09 13:48 已部署**（`install-all`，整机重启，部署自检 38✓ 1⚠（刚开机）0✗），下表各项功能**待手测**。开发机：font-serve 13 项、wallpaper-serve 12 项 `cargo test` 通过，`shared` / `ui-font` 的 C 单测通过。下表是两个服务的改动；两个 xovi 扩展同日另修了三处隐患，见本节「已知风险」。

| 改动 | 为什么 | 验证（开发机） |
|---|---|---|
| wallpaper-serve：cover 模式**先在原图上裁出落在屏幕里的那块，再一次缩放**到 954×1696；contain 改用整块贴到黑底 | 旧实现把整张图缩放到放大后的尺寸再裁掉两边，横图要白做一半以上的重采样，中间图也大一倍多。12MP 横图 1.72s → 1.01s，中间图 15MB → 6.5MB | 对拍测试：contain 与旧实现逐字节相同；cover 取景一致（平滑渐变图最大通道差 ≤3） |
| wallpaper-serve：壁纸池只认普通文件 | 池目录里的子目录或上传半成品可能被轮换选中、读失败报错 | 单测 |
| wallpaper-serve：找 xochitl 主进程读 `/proc`（`rmsvc_core::proc::find_process`） | 此前每次查状态都 fork 一个 `systemctl show xochitl -p MainPID`，网页刷新一次要查两三遍 | 单测 |
| font-serve：`fc-cache` 只扫用户字体目录（`fc-cache -f ~/.local/share/fonts`，fc-cache 对目录递归，含 `shelf-ui/`） | 此前不带目录参数，每装/删一个字体都把没变过的系统字体目录也强制重扫一遍（开发机 CPU 0.7s → 0.006s） | 单测 |
| font-serve：一次上传多个字体只跑一遍 `fc-cache` | 此前每个文件各跑一遍 | 单测 |
| font-serve：`fc-scan` 30 秒、`fc-cache` 120 秒超时（`rmsvc_core::proc::run_timeout`） | 坏文件或 IO 挂起时不能把上传请求一直挂着；`fc-scan` 超时退回自己解析 name 表 | 单测 |
| font-serve：跨分区装字体不留半截文件；字体目录只认普通文件（符号链接不再扫入）；家族名带 `&` 时 `fontconfigRef` 能对上 | 基座 `fs::move_into` 先复制到临时名再改名（基座白皮书 §02）；其余为扫描与转义的边界修正 | 单测 |
| 两个服务的通用代码迁到 rmsvc-core（`Paths::sandbox` 测试沙箱、`formats::mime_of`、`run_or_exit` 等），删掉私有副本 | 去重；测试不再落到开发机真实 XDG 目录 | 测试照常通过 |

**部署后确认**（待手测）：传一张横图壁纸，休眠屏取景正常、上传明显变快；一次上传两三个字体，回执很快回来、阅读字体菜单照常出现新字体；界面字体照常生效。

### 已知风险（审计发现，同日已修并部署）

两个 xovi 扩展是改 xochitl 进程内存的高危代码。审计先只记录，同日按"先离线反汇编钉死再写"修掉了三处（两个 `.so` 因此都变了），随 13:48 那次部署整机重启生效。三处**之前都没有在真机上出过问题**。真机验证到哪一步：

- **#2 已在真机看到**：部署后 xochitl 的 `/proc/<pid>/maps` 里，hl-snap 改过的那页 `0xf03000` 是 `r-xp`，不再是 `rwxp`。
- **#1、#3 只有离线证据**：#1 的上界在正常划线时本来就不会越过，#3 在当前固件上触发不到，真机上没法直接观察到"拦住了"；靠的是反汇编、ELF 头核对和 host 单测。部署后两个扩展照常加载（部署自检通过），但荧光笔吸附本身没有手测。

| # | 在哪 | 隐患 | 怎么修的 |
|---|---|---|---|
| 1 | `hl-snap/src/hl_snap.c` 的 `cj_hl_glyph_is_cjk` | 拿 `subs[0]` 当下标读 glyph 数组时只检查了 `idx < 0`，**没有上界检查**；越界会读到数组外的内存 | 从 3.28.0.172 的反汇编确认 glyph 数组是 Qt 6 的 QList：`scene+8` 是元素基址、`scene+0x10` 是元素个数（兄弟函数 `0xf03750` 开头 `ldp x5, x3, [x21, #8]` 一次取两者，`sub x8, x3, #1` 做上界）。下标 ≥ 个数时按"不是汉字"处理、交回原生扩张，不再读数组外的内存。xochitl 自己的 `0xf02e90` 也不查上界，所以这只是多一层保险 |
| 2 | `shared/trampoline_patch.c` 的 `cj_patch_target` | 改写目标函数开头时把那一页改成**可读可写可执行（RWX）**，改完一直留着 | 改之前从 `/proc/self/maps` 记下原权限，写完跳转后恢复：原本 `r-x` 的代码页恢复成 `r-x`；原本就可写（例如别的扩展先改过）、横跨的两页权限不一致或查不到时保持 RWX（旧行为）；恢复失败只打日志、保持 RWX。别的扩展以后要改同一页，照样会自己先 `mprotect`。调用桩分配失败、放弃 hook 时同样恢复（同日补的口子：以前这条罕见路径会把页留在 RWX）。host 单测覆盖页内、跨页、原本 RWX、调用桩分配失败（测试链接时包一层 `mmap` 让它故意失败）四种情形；真机上这条失败路径不会走到，只靠单测 |
| 3 | `ui-font/src/ui_font.c` | 原函数地址用 `dlsym(RTLD_DEFAULT, setFont)` 取。**非 PIE** 的主程序如果取过 setFont 的地址，dlsym 拿到的是主程序自己的"规范 PLT 桩"，调它又经改过的导入槽回到 handler，**无限递归** | 3.28.0.172 的 xochitl 确实是非 PIE（`ELF Type: EXEC`），但 `.dynsym` 里 setFont 的 `st_value` 是 0（没取过地址），dlsym 拿到的是 Qt 里的真函数，当前不会触发。现在 `_xovi_shouldLoad` / `_xovi_construct` 检查 dlsym 结果是否落在主程序的加载段里，落在就拒绝加载。host 测试的非 PIE 两组改成真正非 PIE 编译（加 `-fno-pie`），并复现了"取地址后 dlsym 落在主程序"这一情形 |

## 03q｜第七轮审计给本线的改动（2026-10-10，**未部署**）

两个 `.so` 都重编了：`hl-snap.so` `f7261e7f…` → `261606f0…`，`ui-font.so` `21cda291…` → `c294752f…`（交叉编译零警告，动态符号最高版本不变：hl-snap `GLIBC_2.17`、ui-font `GLIBC_2.34`）。

| 改动 | 为什么 | 验证（开发机） |
|---|---|---|
| 两个扩展各自的极简 JSON 扫描收进 `shared/minijson.{c,h}`（以 ui-font 那份为准）：只认**顶层对象里后面跟冒号的字符串**为键；符号设为 hidden | 旧实现用 `strstr` 找 `"键名"`：某个值恰好等于键名时先撞上它、把真键遮住（`{"last":"hlSnapCjk","hlSnapCjk":false}` 旧 hl-snap 读成"开"）；嵌套对象里的同名键被当成顶层键；hl-snap 那份还不要求键后有冒号。两个 `.so` 装进同一进程，函数不导出，免得一个扩展调到另一个扩展里的同名函数 | 把旧 hl-snap 函数逐字复制到临时程序跑出三种错法；`shared` 单测新增布尔/字符串两组用例，ui-font 单测补"值等于键名"一例 |
| `cj_patch_target` 搬运前逐条检查被覆盖的指令，PC 相对寻址（`adr`/`adrp`、`b`/`bl`、`b.cond`/`bc.cond`、`cbz`/`cbnz`、`tbz`/`tbnz`、`cb<cc>`、`ldr`/`ldrsw`/`prfm` literal）一律放弃 hook，不改字节、页恢复原权限 | 以前只靠调用方注释保证；以后新增 hook 时漏核一条就会在调用原函数时跳飞。宁可误判放弃，不漏判（`br`/`blr`/`ret` 不算：`blr` 的返回地址落在调用桩里，返回后接着跑桩里的指令，结果正确） | 单测里 21 条 PC 相对、17 条非 PC 相对的编码都由 GNU as 2.47 汇编得到；hl-snap 现有目标 `0xf03670` 开头 5 条（`paciasp`/`stp`/`mov`/`str`/`mov`，用 3.28.0.172 的 xochitl objdump 再核过）不会被误判 |

**行为变化**：hl-snap、ui-font 读配置的结果只在上面那几种不规范/罕见的 JSON 上不同；网关与设置页写出的 `reading-qol.json`、font-serve 写出的 `ui-font.json` 都是扁平对象，结果与旧版相同。hl-snap 的 hook 目标不触发新检查，加载与安装行为不变。

**部署后确认**（待部署）：journal 里仍有 `荧光笔EXPAND hook 安装完成` 与 `[ui-font] 安装完成`、没有 `PC 相对寻址，…放弃 hook`；网页「系统增强」关掉再打开吸附开关，各划一段中文看效果跟着变；界面字体照常。

## 03r｜重构第二阶段给两个服务的改动（2026-10-10，**未部署**）

只动 font-serve、wallpaper-serve 两个 Rust 服务，把它们迁到基座第一阶段新加的接口；两个 `.so` 不变。开发机：两个服务各 16 项 `cargo test --locked`、`cargo clippy --all-targets -- -D warnings` 零告警。

| 改动 | 为什么 | 验证（开发机） |
|---|---|---|
| wallpaper-serve 休眠读图监听改用 `rmsvc_core::fswatch::watch_with`（`WatchSpec{mask: CLOSE_NOWRITE, debounce: None}`），删掉自己那份 inotify + 退避重挂（审计 CORE-7） | 两份同构代码；原先自己写只因基座掩码写死、强制防抖 | 真 inotify 用例：别人读 `current.png` 才轮换、自己写不触发；壁纸目录整个删掉、上传把它建回来后照常轮换；开局目录不在先建再监听；按文件名筛事件。沙箱里起服务空闲 30 秒、以及读过一次 `current.png` 之后空闲 20 秒，监听线程上下文切换都是 0（改前同样是 0）：改前阻塞在 inotify `read`，改后阻塞在无超时 `poll` |
| 两个服务的仓库改实现 `UploadTarget`，`list`/`remove` 改成固有方法（审计 CORE-6） | 上传流程用不到列表和删除；font-serve 那个只转发 `remove_family` 的 `remove` 没人调，删掉 | 原有用例 |
| 错误分级（审计 CORE-1）：wallpaper-serve 名字不合法 400、池里没有 404、删正在用的那张 409、写 `current.png` / 状态文件 / `xochitl.conf` 失败 500；font-serve 没有这个家族 404、选没装的界面字体 400、删字体文件 / 重写索引 / 存选择失败 500 | 此前一律 400（预览缺图是 404），设备侧写盘故障也显示成"请求不对"。前端只显示 `message`、不按状态码分支，提示文字不变 | 路由抽成 `router()`，用 `TestRequest` 补路由级用例；写盘失败用"目标位置被目录占住"造出来 |
| `/events` 改用 `sse_reply_for`（审计 CORE-5） | 心跳间隔直接读本请求的 `?ka=`，不靠路由分发时的线程局部 | 路由用例 |

**与旧监听的差别**（都在 `wake.rs` 头注里）：壁纸目录被删后，旧代码重试时自己把目录建回来，现在等别人建（上传入池会建、服务重启也会建；目录不在时池是空的，本来没东西可轮换），fswatch 按 1 秒起翻倍、5 分钟封顶的间隔查目录回来没有；目录被挪走时旧代码没订 `IN_MOVE_SELF`、会一直守着挪走后的目录，现在会重挂到原路径；新接口不区分 `IN_ISDIR`，只有目录恰好叫 `current.png` 才会撞上。上传请求整体出错（基本是 multipart 解析失败）仍报 400：基座 `AssetUploadFlow::run` 只返回字符串，分不出"暂存目录建不起来"这种 500，留给基座下一步。

**部署后确认**（待部署）：网页「壁纸」上传、切换、删除照常；休眠一次后壁纸轮换（journal 有 `休眠屏已读 → 轮换到 …`）；「xochitl」页上传 / 删除字体、改界面字体选择照常；网页事件流照常推送（改完列表自动刷新）。

## 04｜踩坑

**逆向方法论**

- **RTTI 存在 ≠ 有 vtable；"反查找不到 vtable" ≠ "没有 vtable"**。类被 `typeid()` 用到就会生成 RTTI，不代表多态；反过来，内联组合成员的 vtable 指针是容器对象构造函数里按值写入的，没有地方存指向它 typeinfo 的指针，反查天生走不通。§03c 第 2 轮两头都错过。反查走不通时，先判断目标是"独立堆对象"（能反查）还是"别的对象的组合成员"（从容器的构造函数正向找）。
- **stripped 二进制里，指向字符串的指针不会自动有 xref**：Ghidra `Show References To` 为空不代表没人引用，退回 `Search → Memory` 按字节搜。但搜到一个指针值不等于它在你期望的结构体字段里，`.data.rel.ro` 里紧挨着的对象会命中无关邻居，**要验证内容合理**。
- **Qt 编译进二进制的调试/异常字符串比符号表可靠**：沿已知函数读下去常能撞见写死的源码路径、方法名、日志文案（`shapesoverlay.cpp` / `saveStroke` / `updateImage` / `"New StrokeRenderer,"`），是确认"这是哪个类"最快的办法，应优先找。
- **反编译结果"干净利落"要留心眼，关键结论交叉核实原始汇编**：`FUN_00f401f0` 被简化成一行线性缩放，实际多了两个入参和一个返回值；信了简化结论就宣称"最具体候选"，没先查它有没有被调用（零调用者早该是暂停信号）。产出多个值的方法只有一行，往往是反编译器漏看了寄存器。
- **反编译里 `undefined2*` 指针的偏移是"2 字节单位"**：`FUN_00f4c8d0` 的 `param_3 + 2` 实际是字节偏移 4。核对关键偏移先确认指针类型宽度。
- 一旦有了具体地址，headless 脚本（[`../../defw/scripts/`](../../defw/README.md)）就能接手反编译、查引用、按字节搜内存，不必再靠 GUI 截图；前提是 GUI 已关闭工程，否则抢 `.lock`。

**hook 安全性**

- **多条调用路径汇到同一函数时，"参数在路径 A 里是什么"不能套到路径 B**：钢笔走虚函数调用，没验证过它传给 `FUN_00f47530` 的 `ctx` 就是 `FUN_00f3f9d0` 自己的对象，照抄得到的 `ctx+0x70` 是随机数据。hook 目标**自己**直接读写的字段安全，借用上游解读的字段不安全。
- **"A 调用 B"不等于"所有对 B 的调用都来自 A"**：`FUN_00f3f9d0` 有 6 个调用点，从它入口设全局变量再传给下游，真机证明下游绝大多数调用不经过这一份。除非引用或运行时数据证明 B 只有唯一入口。
- **"前 20 字节可安全 patch"必须逐个候选验证**：`FUN_00f4f430` 签名和已验证的两个完全一样，但第 3 条指令是条件分支，搬进调用桩会跳错地方——靠 `CheckFuncSizes.java` 逐条反汇编提前拦下。签名相似不代表二进制布局相似。
- **两个扩展抢同一个 hook 目标：先到先得，后到的静默放弃**。`hl-snap.so` 与 `cangjie-langhook.so` 都 patch `FUN_00f05ad0`（§03a）。凡是"从老项目拆出功能子集"，都要检查新旧产物会不会同时部署、目标有无重叠，并在部署文档里写清互斥关系。
- **多扩展共存曾依赖加载顺序（已修、已部署）**：`shared/scan.c` 的 `cj_find_exec_module` 原先只返回 xochitl **第一个**可执行段。每装一个 hook，`mprotect` 都会把那一页切成独立的段，之后"第一段"只到最低的已 patch 页为止，后装的扩展找不到更高地址的目标。09-24 真机 maps 里 xochitl 代码段确实被切成了 7 段；当时 hl-snap 和 hw-stroke 都装上，是因为顺序恰好是高地址目标先装。**修法**（09-24）：把紧接其后的同文件、首尾相接、可读可执行的续段一并算进扫描范围；`_xovi_construct` 找不到映射或特征码时改为打日志。host 单测用真内核 `mprotect` 复现了切段。现在装的两个扩展之间**没有加载顺序依赖**（2026-10-09 按代码核实）：ui-font 只改 xochitl 导入表（数据段 `.got.plt`）里的一格，不扫描、不改代码段；hl-snap 扫描并改代码段，读不到也不改导入表。谁先谁后，对方看到的内存都一样。10-09 真机日志里是 hl-snap 先、ui-font 后，maps 里被改的那页 `0xf03000` 已恢复成 `r-xp`、代码段被切成 3 段，扫描照常合并。以后再加**改代码段**的扩展时，仍要把两个 `.so` 改名调换顺序验一次（§05）。
- **换了运行中 xochitl 正映射着的 `.so` 再 restart → 整机重启**（09-21 appload、09-24 hw-stroke 两次）：09-24 改为 stop → 换 → start（§03g）。**09-25 更正**：真正的问题是停止 xochitl 本身有概率崩溃（同日一次只换 qmd 的普通 restart 也崩了，memfault 栈与 09-21 同一处），与换没换 `.so` 无关；现在部署生效一律主动整机重启，见 `packaging/README.md`「怎么让改动生效」。反编译坐实的机制（xochitl 退出时 `atexit` 先析构墨水屏刷新任务用的全局数据、后停线程池）见 `defw/README.md`「调查记录」，与本项目扩展无关。

**qmd 补丁怎么离线验证**

![qmd 离线验证：check-compatibility 不可信，要用 apply-diffs](diagrams/qmd-offline-verify.svg)

- **`qmldiff check-compatibility` 不能用来验证 qmd**（2026-09-30 第五轮审计发现）：这个子命令基本不做校验，给它一份语法都不对的垃圾 qmd 也报"无错误"，查不出 `({})` 对象字面量这类会让整份补丁在设备上不应用的错误，也不核对 LOCATE 锚点能不能命中。它过了什么都不说明。
- **要用 `apply-diffs` 实跑**：把从固件解出的真实 QML 放到 hashtab 里对应的资源路径下，用 `asivery/qmldiff` 的 `apply-diffs` 把 qmd 真正应用一遍，看每个 AFFECT 是否应用、插入位置对不对，再用 `qmllint` 比补丁前后报错数（§03i 就是这么验的）。桩 QML 只能证明语法，证明不了锚点在真实 QML 上命中（§03l 的教训）。
- 本机新版 qmldiff 可能比设备上的宽松（例如新版对 `({})` 不再报错、旧设备版会），所以 qmd 里照旧**不写 `({})`、handler 一律用 `{ … }` 块**。

**构建与环境**

- **交叉工具链的 glibc 比设备新，`.so` 会在符号解析这步静默加载失败**：`atan2f` / `sqrtf` 在本地链接时被打上 `GLIBC_2.43`，设备的 `libm` 没这么新，整个 `hw-stroke.so` 加载失败，还连累同一次扫描里的 `hl-snap.so`。用 `objdump -T` 检查，最高应是 `GLIBC_2.17`。规避：能内联的用 `__builtin_xxx` + `-fno-math-errno`，不能的用三角恒等式改写。多扩展场景下要确认**所有**扩展都正常，不能只看目标扩展的日志（现在网页徽章能直接看到）。
- **提交进仓库的生成文件，Makefile 规则别依赖它的 mtime，`clean` 也别删它**：`xovi_glue.{c,h}` 已提交，但旧规则依赖 `.xovi` 的修改时间——checkout 后 `.xovi` 恰好较新就会去跑 xovigen，本机没有 asivery/xovi clone 时构建失败，部署脚本随即退回仓库里已提交的（可能是旧源码编出的）`.so`，悄悄部署了旧版。现在胶水只在缺失时生成，改了 `.xovi` 用 `make glue XOVI_DIR=<clone>` 显式重生成（09-24）；`make clean` 09-30 起只删 `.so`。
- **构建产物里带开发机绝对路径**：`-g` 的调试信息记着编译目录，同一份源码在两处编出不同 md5，核对"设备上的 `.so` 是不是仓库这份"时容易误判。09-25 `Makefile` 加 `-ffile-prefix-map=$(CURDIR)=.`，反汇编不变；当天已部署真机，设备上 md5 与仓库一致。
- **Java Swing 在 Wayland 平铺合成器下整窗口空白**：设 `_JAVA_AWT_WM_NONREPARENTING=1`；先查 `$XDG_SESSION_TYPE`，别怀疑程序或工程坏了。Ghidra headless 建的 `.gpr` 是 0 字节也正常（元数据在 `.rep/`）。
- **过期文档可能指向"已经放弃的危险方案"**（§03a 的 `/usr` drop-in）：这类要立刻改，不能当一般文档债。
- **诊断工具自己也会出事**（§03b）：常驻、周期性、在 cgroup/fork 这类内核敏感路径附近的操作要尽量去掉。
- **PDF 渲染子进程里的"拒绝加载"日志是正常的**：xochitl 渲染 PDF 时 fork 出的子进程（journal 里常带 `rm.worker.unix` 字样）也会走一遍 `_xovi_shouldLoad`，因为找不到 `/usr/bin/xochitl` 映射而打"拒绝加载"，不代表主进程没装上。

## 05｜真机待办

**未闭环**

| 项 | 现状 | 下一步 |
|---|---|---|
| 第七轮审计改动（§03q，10-10） | 两个 `.so` 已重编，**未部署** | 部署后按 §03q「部署后确认」核对 |
| 第六轮审计改动（§03p，10-09） | 10-09 13:48 已部署，部署自检 38✓，功能未手测 | 按 §03p「部署后确认」手测 |
| 三处 C 扩展隐患（§03p「已知风险」） | 10-09 已修并随同一次部署上机；代码页恢复 `r-xp` 已在真机 maps 看到；glyph 上界、非 PIE 拒加载只有离线反汇编与 host 单测 | 划几段中文确认荧光笔吸附照常；journal 里仍有 `荧光笔EXPAND hook 安装完成` 与 `[ui-font] 安装完成` |
| 删日漫翻页规则（§03i，10-07） | 10-07 已部署并整机重启，部署自检通过，功能未手测；qmd 离线验证过，部署后 journal 有 `CJ-PAGE-TURN: loaded`、无 qmd 解析报错 | 看：「系统增强」只剩「单击翻页」开关；单击左右边缘照常翻页；日漫滑动不再对调；journal 里 `cfg tap=…` 正常 |
| 第五轮审计改动（§03m） | 09-30 14:10 已部署，部署自检通过，功能待手测 | 按 §03m「部署后确认」手测 |
| 第四轮审计改动（§03l） | 已随后续部署上设备，功能项没逐项核对 | 按 §03l「部署后确认」手测 |
| 多扩展共存的反序加载（§04） | 修法已部署；现装的 hl-snap（改代码段）与 ui-font（只改导入表一格）互不读写对方改动的内存，10-09 按代码核实无顺序依赖 | 以后再加**改代码段**的扩展时，把两个 `.so` 改名调换加载顺序验一次 |
| wallpaper-serve 监听休眠读图 | 09-24 真机验证：休眠那一刻轮换、只轮换一次 | 充电状态（内核不挂起）下还没试 |

**已闭环（真机）**：界面字体 ui-font（§03o，10-07：界面换成更纱、阅读字体不变，用户目测）；hl-snap 精确吸附（§03a）；网页"已加载"徽章（§02）；xochitl 单击翻页 + 日漫翻页规则（§03i；日漫翻页 10-07 删除）；换入后整机重启的部署流程（§04，09-25 首次走通有变化的 `.so`）；wallpaper-serve 监听休眠读图轮换；mkdir-agent 290 秒长轮询（§03j）；lo-alias 不插 USB 冷启动（09-25 两次无 USB 整机重启后核对：`10.11.99.1` 同时挂上 `lo` 与 `usb1`，xochitl :80 已绑定）；移除 hw-stroke / battop 的旧设备清理（§03n，09-30 部署自检）。

**随功能移除不再跟进**：hw-stroke 的 `bVar16<4`（最常用钢笔/铅笔量级）、`FUN_00f4f430` 不能安全 patch、像素消费者的真实目标、参数调优、真实压感；battop 两次冻机的内核根因（RCU stall 本身仍未排除）。

## 附录｜迁移沿革

| 时间 | 事件 |
|---|---|
| 2026-09-09 | `battop/` 从旧 `misc/battery-audit/battop/` 搬进本线；`hl-snap/` 新建 |
| 2026-09-10 | `defw/`（3.28.0.172 固件逆向产物）由 `ghidra-project-328` 改名而来；**不属于**本线，是共享的逆向基座，hw-stroke 的研究用它 |
| 2026-09-11 | 旧 `chinese-ime/` 移出仓库 → `shared/`（扫描 + trampoline）、`lo-alias/` 由路径引用改成独立副本 |
| 2026-09-11 | `wallpaper-serve/`、`font-serve/` 从 `shelf/services/` 挪进本线（运行时行为不变）；bind-mount 壁纸脚本归档进 `wallpaper-serve/legacy-bind-mount/` |
| 2026-09-11 | `packaging/` 成为本线工具的 host 侧编排方（`deploy-<name>.sh`），设备端 `install.sh` 仍可单独跑 |
| 2026-09-11 | 网关由 `shelf/services/shelf-gateway` 搬到顶层 `gateway/`，`gateway/src/enhance/` 是本线的网页控制面 |
| 2026-09-15 | 代码审查：两个扩展各约 50 行逐字节重复的 trampoline 安装代码收进 `shared/trampoline_patch.c` |
| 2026-09-20 | 扩展的设备端安装流程收进 `packaging/xovi-ext-install.sh`（数据驱动） |
| 2026-09-24 | 网页"已加载"徽章（§02）；第三轮审计（§03k，当天部署） |
| 2026-09-25 | 部署生效一律整机重启（§04）；`Makefile` 加 `-ffile-prefix-map`；lo-alias 无 USB 冷启动真机通过；第四轮审计（§03l） |
| 2026-09-29 | 设备上卸载 KOReader、第三方 WeRead、appload 和侧栏入口（按用户要求） |
| 2026-09-30 | 第五轮审计（§03m）；撤掉母版库按书设阅读方向，`rtl-overrides.json` 改为只读（§03i）；14:10 部署 |
| 2026-09-30 | **移除手写优化（`handwriting-stroke/`）与电池刺客（`battop/`）**：源码、部署脚本、网页开关与数据页一并删除；15:23 部署，`install-all.sh` 自动清掉设备残留（§03n） |
| 2026-10-07 | **删日漫翻页规则**（用户定：书架只管入库，方向交给书本身）：`reader-page-turn.qmd` 只留单击翻页，网关删 `rtlPageTurn` 开关，book-serve 删 `GET /reading-direction`（§03i；已部署并整机重启，部署自检通过，功能未手测） |
| 2026-10-07 | **界面字体 ui-font**（§03o）：新 xovi 扩展 `ui-font/` + `shelf/xovi/ui-font-tokens.qmd` + font-serve 界面字体仓库与网页卡片；`packaging` 加 `ui-font` 步（`deploy-ui-font.sh`）；已部署并整机重启，部署自检 38✓ 1⚠（刚开机）0✗，正文与标题选更纱黑体 UI SC |
| 2026-10-09 | 第六轮审计（§03p）：壁纸先裁再缩放、`fc-cache` 只扫用户字体目录等；同日修掉审计记下的三处 C 扩展隐患；13:48 部署（自检 38✓，功能未手测）；仓库公开 |
