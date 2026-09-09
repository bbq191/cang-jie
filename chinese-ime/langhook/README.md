# cangjie-langhook

> **状态**：拼音输入法这条线（虚拟键盘 hook、拼音/双拼/繁体引擎、候选词典、逐字候选、中英混输、霞鹜新致宋主字体 + 花园明朝 B 扩展 B 兜底）**已在真机全链路验证通过**（M3–M7）。本 README 上半部分讲这份 hook 的技术底子（内部用字节特征码扫描 + 自定义 trampoline，不用 xovi 的 override 机制；但外壳是合规 xovi 扩展，靠 xovigen 生成入口元数据、放 `extensions.d/` 被自动加载），下半部分「当前状态」+「一键安装」是最新交付状态；完整过程见《[reMarkable 拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》。

## 架构总览

`cangjie-langhook.so` 是一份 `.so` 打三组独立 trampoline hook：**① UI 汉化**（语言列表 hook →
中文语言包）、**② 拼音输入法**（虚拟键盘按键处理 → 拼音缓冲 → 词典 → 候选栏），外加 **③ 荧光笔汉字精确吸附（Step HL2）**——hook `FUN_00f05ad0`（xochitl 手写命中区间→整行扩张函数），让荧光笔划中文时不再"划一小段吸整行"（真机 2026-08-23 通过；这是"块 4 系统增强"跨到"块 2 中文化"的特性，二进制不拆、随本 `.so` 一起装）。完整逆向依据与
Step 记录见两本白皮书 §01，此处放两张总览图（HL2 机理）：

**① UI 汉化**（详见《[中文化白皮书](../docs/reMarkable中文化白皮书.md)》§01）：

![UI 汉化架构](../docs/architecture-ui.svg)

**② 拼音输入法**（详见《[拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》§01）：

![拼音输入法架构](../docs/architecture-ime.svg)

## 一键安装（新机 SSH 后照做）

一个打好的安装包放在 `deploy/dist/cangjie-ime-installer.tar.gz`（约 124MB，2026-08-18 重打包；花园明朝 B 单文件约 30MB 是体积主因）：含 **xovi 扩展版 `.so`**（`_xovi_construct`/`_xovi_shouldLoad` 入口 + xovigen 元数据）+ 5 个词典 blob + CJK/阅读字体（霞鹜系列：新致宋 Screen Full=UI/候选栏主字体、新晰黑 Screen Full、文楷、文楷 Mono GB Screen + 花园明朝 B 扩展 B 兜底 + KF Readerly ×4，**零 HarmonyOS**）+ 候选栏 `candidatebar.qmd` + 阅读增强 / 字体菜单 / 设置门户等一组 qmd（含 `add-reading-fonts.qmd`）+ **3 个界面翻译 `reMarkable_zh_{CN,TW,HK}.qm`** + 安装/卸载脚本。**payload 权威清单以 `deploy/install.sh` 的拷贝段为准**（此处不逐一枚举、免漂移）。fail-safe 内建在 `.so` 的 `_xovi_shouldLoad`（不再需要外部 precheck.sh）。

### 前置（安装包不负责，需先自己装好）

1. **开发者模式 + SSH**：设备"设置 → 通用 → 平板电脑 → 软件 → 高级 → 开发者模式"打开（**会恢复出厂、清空数据，先备份笔记**）；开启后 SSH 密码在"设置 → 通用 → 帮助 → 关于 → 版权与许可"页显示。USB 连接走 gadget 网卡，设备侧固定 `10.11.99.1`；开发机侧把这张网卡 IP 配到 `10.11.99.0/24`（如 `sudo ip addr add 10.11.99.2/24 dev <iface>`）即可 `ssh root@10.11.99.1`。
2. **xovi + qt-resource-rebuilder**：按 [asivery/xovi](https://github.com/asivery/xovi) 和 [qt-resource-rebuilder](https://github.com/asivery/rm-xovi-extensions) 官方文档装好（候选栏靠 qt-resource-rebuilder 的 QMLDiff 注入，是硬前置）。所有 xovi 扩展都以这套为基座，本包不重复 bootstrap 它。

### 三步安装

```bash
# 1. 把安装包传到设备
scp deploy/dist/cangjie-ime-installer.tar.gz root@10.11.99.1:/home/root/

# 2. 在设备上解包
ssh root@10.11.99.1 'cd /home/root && tar -xzf cangjie-ime-installer.tar.gz'

# 3. 跑安装脚本（幂等，可重复跑）
ssh root@10.11.99.1 '/home/root/cangjie-ime/install.sh'
```

`install.sh` 会：**拷 `cangjie-langhook.so` 到 `/home/root/xovi/extensions.d/`（xovi 自动加载）**、拷 5 词典到 `/home/root/.local/share/cangjie-ime/`（`.so` 从这读，`CJ_DATA_DIR`）、拷字体（霞鹜新致宋 + 花园明朝 B + 新晰黑/文楷/文楷 Mono + KF Readerly，见 install.sh 拷贝段）到 `~/.local/share/fonts/` 并装 fontconfig（**整个 xochitl UI** 与候选栏中文字体统一走**霞鹜新致宋（LXGW Neo ZhiSong Screen Full），扩展 B 生僻字字形级回退花园明朝 B（HanaMinB）**）、**CJK 字体覆盖预检**（`fc-list :lang=zh-cn` 为空即中止，防豆腐块）、拷 `candidatebar.qmd`/reading-qol/字体菜单 qmd 到 qt-resource-rebuilder、**拷 `reMarkable_zh_{CN,TW,HK}.qm` 到 `~/.local/share/cangjie-ime/translations/`（`/home` 持久）+ 装一个 pre-start bind-mount 脚本把它盖到 `/usr/share/.../translations/` 上（界面汉化，运行时 VFS 挂载、不改 `/usr` 块本身、无 verity 回滚风险，2026-08-16 起改用这套，见下）**、写 `~/xovi/services/xochitl.service/` 下的持久源 `.conf`（`LD_PRELOAD=xovi.so` + `XOVI_ROOT` + **`QML_DISABLE_DISK_CACHE=1` 等 QML env**——qt-resource-rebuilder 硬需求，`xovi/start` 会把它拷进 `/etc` tmpfs）、`daemon-reload` + 重启 `xochitl` + 健康检查（`is-active`=active / `MainPID` 变化 / `NRestarts` 不增 / **cangjie 扩展是否真加载**）。fail-safe 内建在 `.so` 的 `_xovi_shouldLoad`（扫 xochitl 特征码兼容才加载、否则裸启原生），不再需要外部 precheck。脚本自带 xovi/qt-resource-rebuilder 前置检查。卸载走 `uninstall.sh`（先把系统语言从 `zh_*` 改回 `en` 再删翻译，避免配置悬空）。

装完：
- **输入法**：点开任意文本框弹出键盘，**点地球**在「简体全拼 / 繁體全拼 / 简体双拼 / 繁體双拼」间切换开始输入。
- **界面汉化**：Settings → General → Language 选「简体中文 / 繁體中文 / 香港繁體」。

**字体**（2026-08-23 定案）：整个界面 UI 和候选栏的中文统一走 **霞鹜新致宋（LXGW Neo ZhiSong Screen Full）**——宋体书卷气、BMP+扩展 A 全覆盖、Screen 版 e-ink 不发虚；打到 CJK 扩展 B 生僻字（词典雾凇 41448 大字表含约 1.3 万个，如 `hang` 尾部 𠡊）时字形级回退 **花园明朝 B（HanaMinB）**，不再豆腐块。靠 `/home/root/.config/fontconfig/fonts.conf`（`/home` 持久分区，重启不丢）把 `sans-serif` 及 zh-* 指向"致宋 + HanaMinB"两级链，另对致宋开 `embolden` 合成加粗补偿宋体笔画在低对比 e-ink 下发淡。候选栏字体由 `candidatebar.qmd` 的 `cjkFamily` 显式设为致宋（简/繁字形已由 `dict.bin`/`dict.zh_tw.bin` 决定，字体只管渲染、不按语言切族）。**不再部署 HarmonyOS / 按 SC/TC 切族的旧方案。**字体演进与扩展 B 兜底诊断 及《中文化白皮书》§3.3。

### 重启持久化（2026-08-16 改用 vellum-xovi 官方机制，彻底弃掉 /usr drop-in）

**早期方案（已放弃，别再照抄）**：drop-in 写根分区 `/usr/lib/systemd/system/xochitl.service.d/zz-cangjie-xovi.conf`。真机踩实两次：写 `/usr` 后完整重启触发 dm-verity root hash 变化 → A/B 回滚——**正是设备被搞成裸机的原因**。这条路线 2026-08-16 起彻底不用了。

**现行机制**：cangjie 的 env drop-in（`LD_PRELOAD=xovi.so` + `XOVI_ROOT` + `CANGJIE_IME_HOOKS`（若配了的话）等）写成一份 `.conf` 放进 `~/xovi/services/xochitl.service/`（**持久源，在 `/home`**，跟 `qt-resource-rebuilder.conf` 并列）。`/home/root/xovi/start` 遍历这些持久源目录，把内容整份拷进新挂载的 `/etc/systemd/system/xochitl.service.d/` **tmpfs**（overlay，普通重启就清）、再生成 `00-xovi.conf`、`daemon-reload`、重启对应 unit——**全程不碰 `/usr`，无 verity 回滚风险**。装脚本已自动处理这一步，正常装完不用手跑。

- **真机重启后（不是休眠唤醒）需要手动跑一次 `/home/root/xovi/start`**（等价于 `vellum reenable`，两者殊途同归，都是重建 `/etc` tmpfs）——`/etc` 是 overlay，真重启即清空，这是接受的已知负担（真机大多是休眠不是重启，tmpfs 休眠不丢，负担很小）。**不要理解成"别跑官方 xovi/start"**——现在 cangjie 自己的持久化就是靠 `xovi/start` 生效的，跑它不会冲掉 cangjie，反而是让 cangjie 生效的正确步骤。
- **界面汉化的 bind-mount**同样是 `xovi` pre-start 脚本（`cangjie-xlate-bindmount.sh`）负责，每次 `xovi/start` 自动重新挂载，跟 langhook 扩展本身的持久化是两回事、互不影响。
- **OS 固件更新（OTA）后**会整个换新 `/usr`（`/home` 不受影响）——`extensions.d/` 里的 `.so`、`~/.local/share/cangjie-ime/` 的词典/配置本身**理论上不受 OTA 影响**（不在 `/usr`），但 `/etc` tmpfs 配置会因为重启而清空，**需要重新跑一次 `/home/root/xovi/start`**（不一定要整个重装 `install.sh`，除非 `/home` 下的 payload 本身也丢了——2026-09-09 真机踩过 payload 整个从设备上消失、只有配置文件残留的情况，原因不明，遇到这种"重跑 xovi/start 没用、journal 里根本没有 `[cangjie]` 日志"的情况先 `find / -iname "cangjie-langhook.so*"` 确认二进制还在不在，不在就得整个重跑 `install.sh`）。得益于韧性重构，`.so` 本身**不需要为新固件版本重新推导偏移**（函数体没大改就自动特征码定位）。

### 回退

```sh
rm -f /home/root/xovi/extensions.d/cangjie-langhook.so                          # 摘掉 cangjie 扩展
rm -f /home/root/xovi/services/xochitl.service/cangjie-langhook.conf            # 摘掉持久 env 源（若装过）
/home/root/xovi/start                                                          # 重建 /etc tmpfs，不再含 cangjie，顺带重启 xochitl
```

即恢复到没装输入法的状态（`.so`/词典/字体留在 `/home/root` 不影响，想清可手动删）。**不要用 `mount -o remount,rw / && rm .../usr/lib/...`那种写法**——现在没有任何东西写在 `/usr`，这条命令本身除了无谓地 remount 根分区之外什么也做不到。

### 重新打包（维护者）

安装包由本仓库现成产物组装，重建：

```bash
# .so:  cd chinese-ime/langhook && make aarch64 XOVI_DIR=<asivery/xovi clone>
#       （xovi 扩展需 xovigen 生成元数据胶水；.xovi 描述在本目录 cangjie-langhook.xovi）
# 词典: cd chinese-ime/pinyin-engine && python3 c/tools/gen_dict_blob.py && ...（各 gen_*_blob.py）
# 组装: tar 内 cangjie-ime/install.sh（+ uninstall.sh）在根，其余资源进 cangjie-ime/payload/：
#   payload = cangjie-langhook.so(xovi扩展版) + c/build/*.bin(词典) + 字体 ttf + fontconfig
#           + candidatebar.qmd + reading-qol/*.qmd + font-menu/add-reading-fonts.qmd
#           + reMarkable_zh_{CN,TW,HK}.qm
#   ——具体 ttf/qmd 清单以 deploy/install.sh 的拷贝段为准（勿在此手抄，会漂移）。
# 注意: deploy/uninstall.sh 必须一并纳入 tar。fail-safe 已内建在 .so 的 _xovi_shouldLoad,
#       不再有独立 precheck.sh。reading-qol/font-menu 的 qmd 留在 xovi-extensions/,打包时跨目录拷。
```

---

验证性 xovi 扩展：不通过 xovi 原生的按符号名 `override` 机制（`.xovi` 描述文件 +
`xovigen.py` + `pivotSymbol()`），而是**运行时字节特征码扫描 + 自定义 trampoline**
来 hook `xofm::libs::localization::LanguageSettings::setLanguageCode`。

## 为什么不用 xovi 原生的 override 机制

读了 `asivery/xovi`（LGPL v3）源码后确认：`pivotSymbol()` 只在开头一行用
`dlsym(RTLD_DEFAULT/NEXT, symbol)` 把符号名解析成地址，之后的 trampoline 构造/
`mprotect`/写入逻辑全部只依赖那个解析出来的 `void*`。但白皮书 10.1 节实测过——
`setLanguageCode` 在两个固件版本的 `.dynsym`（动态符号表）里都不存在（只有
401/367 条 defined 符号，全是 libc/Qt 的跨模块符号），`dlsym` 根本找不到它，所以
`pivotSymbol` 这条路走不通。

同一次实测还发现：两个版本（3.27.1.0 / 3.28.0.164）里这个函数的**入口地址不一样**
（`0xa0ff50` vs `0x913c70`，差了约 1MB），但函数体开头 **48 字节逐字节完全相同**。
这正是字节特征码扫描（AOB scan，游戏外挂/反作弊圈子的常规技术）的典型适用场景：
不管链接后地址挪到哪，只要这段字节没变，运行时扫描就能自动找到它，不需要给每个
固件版本号维护一份硬编码偏移表。

## 目录结构

```
src/
  scan.{h,c}                解析 /proc/self/maps，定位 xochitl 自己的可执行映射
  pattern.{h,c}             特征码搜索 + 唯一性校验（0 次或 >1 次都视为失败）
  trampoline_aarch64.{h,c}  AArch64 远跳转 trampoline 指令编码（MOVZ/MOVK x16 + BR x16，
                            编码方式抄自 xovi 的 pivotSymbol，去掉了开头的 dlsym 解析）
  qstringlist_append.{h,c}  用 QArrayData::allocate 手工构造/追加 QString / QStringList
                            （语言列表追加、候选栏 keys 写入、候选词 UTF-8→UTF-16 编码）
  hook_init.c                全部拦截/候选逻辑的入口（3000+ 行）：constructor 里装多条
                              trampoline hook——语言切换器（追加中文项+改显示名+布局兜底）、
                              按键统一入口 FUN_00697610（拦截字符键→拼音缓冲→查词典→
                              写候选栏 CjCandidateBar），直接 include 引擎源码：
                              ../../pinyin-engine/c/src/{segment,syllables,dictionary,
                              jianpin,shuangpin}.c（不复制，避免两份漂移）
tests/                       宿主机单元测试（pattern/scan/trampoline）
deploy/                      一键安装资产：candidatebar.qmd（候选栏 QMLDiff，cjkFamily=
                             霞鹜新致宋）、fontconfig-cangjie.conf（整个 UI 字体：致宋 +
                             HanaMinB 兜底）、
                             install.sh、dist/cangjie-ime-installer.tar.gz（打好的包）
```

## 当前状态（M3–M7 已真机全链路验证——远超本 README 早期记录）

顶部"状态说明"提到的"验证性 hook / 没部署真机 / 不改变行为"是项目最早期的记录，早已被真实进度覆盖。现在这份 `.so` 在真机上跑通并反复验证过：

- **语言切换器接入**：点地球的原生语言弹层里出现 4 个中文模式（简体全拼/繁體全拼/简体双拼/繁體双拼，Phase B），切换立即生效免重启；显示名订正、布局 `en_US` 兜底、崩溃事故均已修复。
- **按键拦截 + 拼音输入**：hook 按键统一入口拦截字符键，缓冲拼音、查 mmap 的 `dict.bin`/`dict.zh_tw.bin`/`dict_jianpin*.bin`，候选栏（QMLDiff 注入的 `CjCandidateBar`）逐字造句、分段提交、退格撤销（Phase A），双拼解码、繁体、简拼补充候选全部真机验证。
- **中英混输**（Phase C）：英文候选背后是 `english.bin`（SymSpell），"你好hello" 增量式补全。
- **字体**：候选栏 + 整个 UI 统一用霞鹜新致宋、扩展 B 生僻字回退花园明朝 B（见上方一键安装的字体说明）。
- **荧光笔汉字吸附（Step HL2）**：hook `FUN_00f05ad0` 让荧光笔划中文时精确吸附、不再"划一小段吸整行"，开关经 `reading-qol.json` 的 `hlSnapCjk`（默认开），真机 2026-08-23 通过。

早期用的**字节特征码扫描（AOB scan）定位 hook 点**这套方法本身仍然成立、贯穿始终——不管固件版本把函数地址挪到哪，只要函数体开头字节没变，运行时扫描就能自动找到（当年对 `xochitl_3.28.0.164.bin`/`xochitl.bin` 两个版本各约 12MB 的 `.text` 段做过唯一命中验证，命中地址跟 objdump 人工反汇编分毫不差）。完整的 hook 定位/踩坑/真机验证记录见《[拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》02 节。

### 韧性重构（.166 适配，已真机验证）

设备 OTA 从 3.28.0.164 升到 **3.28.0.166**，各 hook 目标函数地址整体位移（`+0x240`/`-0xc0`/`-0x220`/`-0x340` 各不相同）。旧设计只扫 `setLanguageCode` 一条锚点算出基址、其余全用"锚点 + .164 固定偏移"，还用 `offset==0x513c70` 版本门卡"是不是 .164"——OTA 后各函数位移不一致，从单一锚点推出来的地址全错、版本门也不匹配 → 整个扩展 safe mode，输入法和界面中文一起消失。

据此重构成**每个目标各自特征码自定位**：

- **删掉版本门**；9 个代码目标各自用自己 40 字节函数体 prologue 运行期扫描（函数搬家自动适配），退格 `FUN_00695820`、按键分发 `FUN_00697610` 体内各有一条 `BL` 相对跳转会随 callee 重定位变化，用**逐字节掩码**（`pattern.c` 新增的 `cj_*_masked`）通配掉那 4 字节即跨版本唯一命中。
- 2 个 metaobject 是数据、无 prologue，但结构体 `+0x18` 是 `static_metacall` 函数指针（非 PIE，绝对值直接存盘），那个函数已被特征码定位——于是扫镜像里"唯一等于该函数地址的 8 字节指针"、减 `0x18` 即 metaobject，**完全版本无关**。
- **每个目标独立 safe mode**：某个没唯一命中只跳过它自己，核心输入不受单个非核心目标缺失影响。

上机前用离线脚本把全部特征码 + metaobject 定位在 `.164`/`.166` 两版二进制对拍确认唯一命中；真机每个运行期地址与离线预测逐一精确吻合。**关于"固件无关"的诚实边界**：真正 OTA-proof 做不到——cangjie 扩展本身在 `extensions.d/`（/home，OTA 不丢），但让 xovi 启动的 `/usr/lib` drop-in + 界面翻译（`/usr/share`）都在 rootfs、OTA 会冲掉，届时重跑 `install.sh` 恢复。韧性重构真正省掉的是"为新固件版本重新反编译推导偏移"：只要函数体没大改，同一份 `.so` 直接在新版本自定位生效（新固件 `20260806095513` 已实测 9 个特征码全命中）。

## 使用

```bash
make test        # 宿主机单元测试（pattern/scan/trampoline），不需要交叉编译器
make aarch64     # 交叉编译出 cangjie-langhook.so（需要 aarch64-linux-gnu-gcc，零警告）
# 部署到设备见上方「一键安装」，或 deploy/install.sh
```

## 许可与出处

`trampoline_aarch64.c` 里的指令编码方式抄自 `asivery/xovi`
（`src/trampolines/aarch64/aarch64.c` 的 `pivotSymbol()`），该项目为 LGPL v3，
详见白皮书 12.2 节许可合规记录。
