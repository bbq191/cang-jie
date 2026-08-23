# cangjie-langhook

> **状态**：拼音输入法这条线（虚拟键盘 hook、拼音/双拼/繁体引擎、候选词典、逐字候选、中英混输、按语言的 HarmonyOS 字体）**已在真机全链路验证通过**（M3–M7）。本 README 上半部分讲这份 hook 的技术底子（内部用字节特征码扫描 + 自定义 trampoline，不用 xovi 的 override 机制；但外壳是合规 xovi 扩展，靠 xovigen 生成入口元数据、放 `extensions.d/` 被自动加载），下半部分「当前状态」+「一键安装」是最新交付状态；完整过程见《[reMarkable 拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》。

## 架构总览

`cangjie-langhook.so` 是一份 `.so` 打两组独立 trampoline hook：**① UI 汉化**（语言列表 hook →
中文语言包）与 **② 拼音输入法**（虚拟键盘按键处理 → 拼音缓冲 → 词典 → 候选栏）。完整逆向依据与
Step 记录见两本白皮书 §01，此处放两张总览图：

**① UI 汉化**（详见《[中文化白皮书](../docs/reMarkable中文化白皮书.md)》§01）：

![UI 汉化架构](../docs/architecture-ui.svg)

**② 拼音输入法**（详见《[拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》§01）：

![拼音输入法架构](../docs/architecture-ime.svg)

## 一键安装（新机 SSH 后照做）

一个打好的安装包放在 `deploy/dist/cangjie-ime-installer.tar.gz`（约 97MB，2026-08-15 真机 install.sh 重跑验证：含 **xovi 扩展版 `.so`**（`_xovi_construct`/`_xovi_shouldLoad` 入口 + xovigen 元数据）+ 5 个词典 blob + 3 个 HarmonyOS 字体 + 霞鹜文楷 `LXGWWenKai-Regular.ttf`(+OFL) + 候选栏 `candidatebar.qmd` + 阅读增强 `tap-page-turn.qmd`/`fast-mono-reading.qmd`(默认关)/`keyboard-mono.qmd` + 字体菜单 `add-lxgw-font.qmd` + **3 个界面翻译 `reMarkable_zh_{CN,TW,HK}.qm`** + 安装/卸载脚本）。fail-safe 内建在 `.so` 的 `_xovi_shouldLoad`（不再需要外部 precheck.sh）。

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

`install.sh` 会：**拷 `cangjie-langhook.so` 到 `/home/root/xovi/extensions.d/`（xovi 自动加载）**、拷 5 词典到 `/home/root/.local/share/cangjie-ime/`（`.so` 从这读，`CJ_DATA_DIR`）、拷 3 个 HarmonyOS 字体到 `~/.local/share/fonts/` 并装 fontconfig（**整个 xochitl UI** 中文字体按语言用 HarmonyOS Sans SC/TC）、**CJK 字体覆盖预检**（`fc-list :lang=zh-cn` 为空即中止，防豆腐块）、拷 `candidatebar.qmd`/reading-qol/字体菜单 qmd 到 qt-resource-rebuilder、**拷 `reMarkable_zh_{CN,TW,HK}.qm` 到 `/usr/share/remarkable/xochitl/translations/`（界面汉化）**、写 `/usr/lib` 持久 drop-in（`LD_PRELOAD=xovi.so` + `XOVI_ROOT` + **`QML_DISABLE_DISK_CACHE=1` 等 QML env**——qt-resource-rebuilder 硬需求）、`daemon-reload` + 重启 `xochitl` + 健康检查（`is-active`=active / `MainPID` 变化 / `NRestarts` 不增 / **cangjie 扩展是否真加载**）。fail-safe 内建在 `.so` 的 `_xovi_shouldLoad`（扫 xochitl 特征码兼容才加载、否则裸启原生），不再需要外部 precheck。脚本自带 xovi/qt-resource-rebuilder 前置检查。卸载走 `uninstall.sh`（先把系统语言从 `zh_*` 改回 `en` 再删翻译，避免配置悬空）。

装完：
- **输入法**：点开任意文本框弹出键盘，**点地球**在「简体全拼 / 繁體全拼 / 简体双拼 / 繁體双拼」间切换开始输入。
- **界面汉化**：Settings → General → Language 选「简体中文 / 繁體中文 / 香港繁體」。

**字体**：整个界面 UI 和候选栏的中文都用 **HarmonyOS Sans**——简体（`lang=zh-cn`）用 SC 字形、繁体/港（`lang=zh-tw`/`zh-hk`）用 TC 字形，靠 `/home/root/.config/fontconfig/fonts.conf`（`/home` 持久分区，重启不丢）。已知边界：xochitl 进程 `LANG` 恒为 `en_US`（汉化靠 Qt `.qm`、没改 locale），繁体 TC 字形能否在整个 UI 生效，取决于 Qt 渲染繁体文字时是否带上 `zh-tw` 语言提示；带不上时统一走 SC（跟原先 Noto Sans SC 行为一致，是可接受兜底）。候选栏因为直接读 `virtualKeyboard.language` 显式选 SC/TC，不受这个 locale 限制。

### 重启持久化（已解决那个坑）

drop-in 写在根分区 `/usr/lib/systemd/system/xochitl.service.d/zz-cangjie-xovi.conf`（不是会被 overlay 清掉的 `/etc`——vellum 的 `/etc/00-xovi.conf` 普通重启就丢、靠 reenable 手动恢复），内容是 `LD_PRELOAD=xovi.so` + `XOVI_ROOT` + **`QML_DISABLE_DISK_CACHE=1` 等 QML env**（qt-resource-rebuilder 硬需求，缺了它自检 abort 拖垮 xochitl）。cangjie 本身是 `extensions.d/` 里的 xovi 扩展、由 xovi 自己加载，**不在 LD_PRELOAD 里**。`/usr/lib` 在 `/dev/root` 持久分区、**普通重启不丢**（实测删掉 vellum 的 /etc 00-xovi 后仍独立支撑）。装脚本已自动处理（临时 remount rw 根分区写入、再 remount ro）。

- **vellum 的 xovi 配置（/etc 00-xovi）普通重启就丢**（overlay），靠 `vellum reenable` 手动恢复；我们的 `/usr/lib` drop-in 是独立一份、rootfs 持久、不受影响（cangjie 是 `extensions.d/` 扩展，不在任何 LD_PRELOAD 里，不会被"删掉那条"）。别主动跑官方 `xovi/start`。
- **OS 固件更新（OTA）后**会重置 rootfs（`/usr/lib` 的 drop-in + `/usr/share` 的界面翻译一起冲掉），扩展静默失效、界面回英文 → 那时重跑一次 `install.sh` 即可全部恢复。得益于韧性重构，`.so` 本身**不需要为新固件版本重新推导偏移**（函数体没大改就自动特征码定位），OTA 只影响 rootfs 上的加载配置和翻译文件。

### 回退

```sh
rm -f /home/root/xovi/extensions.d/cangjie-langhook.so   # 摘掉 cangjie 扩展
mount -o remount,rw / && rm -f /usr/lib/systemd/system/xochitl.service.d/zz-cangjie-xovi.conf && mount -o remount,ro /
rm -f /etc/systemd/system/xochitl.service.d/cangjie-langhook.conf   # 若还残留旧的 /etc 版
systemctl daemon-reload && systemctl restart xochitl
```

即恢复到没装输入法的状态（`.so`/词典/字体留在 `/home/root` 不影响，想清可手动删）。

### 重新打包（维护者）

安装包由本仓库现成产物组装，重建：

```bash
# .so:  cd chinese-ime/langhook && make aarch64 XOVI_DIR=<asivery/xovi clone>
#       （xovi 扩展需 xovigen 生成元数据胶水；.xovi 描述在本目录 cangjie-langhook.xovi）
# 词典: cd chinese-ime/pinyin-engine && python3 c/tools/gen_dict_blob.py && ...（各 gen_*_blob.py）
# 组装: tar 内 cangjie-ime/install.sh（+ uninstall.sh）在根，其余资源进 cangjie-ime/payload/：
#   payload = cangjie-langhook.so(xovi扩展版) + c/build/{dict,dict.zh_tw,dict_jianpin,dict_jianpin.zh_tw,english}.bin
#           + HarmonyOS_Sans_{,SC_,TC_}Medium.ttf + LXGWWenKai-Regular.ttf(+OFL)
#           + fontconfig-cangjie.conf + candidatebar.qmd + reading-qol/{tap-page-turn,fast-mono-reading,keyboard-mono}.qmd
#           + font-menu/add-lxgw-font.qmd + reMarkable_zh_{CN,TW,HK}.qm
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
deploy/                      一键安装资产：candidatebar.qmd（候选栏 QMLDiff，按语言选
                             HarmonyOS SC/TC）、fontconfig-cangjie.conf（整个 UI 字体）、
                             install.sh、dist/cangjie-ime-installer.tar.gz（打好的包）
```

## 当前状态（M3–M7 已真机全链路验证——远超本 README 早期记录）

顶部"状态说明"提到的"验证性 hook / 没部署真机 / 不改变行为"是项目最早期的记录，早已被真实进度覆盖。现在这份 `.so` 在真机上跑通并反复验证过：

- **语言切换器接入**：点地球的原生语言弹层里出现 4 个中文模式（简体全拼/繁體全拼/简体双拼/繁體双拼，Phase B），切换立即生效免重启；显示名订正、布局 `en_US` 兜底、崩溃事故均已修复。
- **按键拦截 + 拼音输入**：hook 按键统一入口拦截字符键，缓冲拼音、查 mmap 的 `dict.bin`/`dict.zh_tw.bin`/`dict_jianpin*.bin`，候选栏（QMLDiff 注入的 `CjCandidateBar`）逐字造句、分段提交、退格撤销（Phase A），双拼解码、繁体、简拼补充候选全部真机验证。
- **中英混输**（Phase C）：英文候选背后是 `english.bin`（SymSpell），"你好hello" 增量式补全。
- **字体**：候选栏 + 整个 UI 按语言用 HarmonyOS Sans SC/TC（见上方一键安装的字体说明）。

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
