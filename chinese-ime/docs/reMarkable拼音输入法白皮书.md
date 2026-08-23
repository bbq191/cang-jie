# reMarkable Paper Pro Move 拼音输入法开发方案白皮书

> 开发方案白皮书 · v1.0 · 拼音输入法分册

虚拟键盘 Hook、拼音/双拼引擎、候选词典与候选栏——这是 M3-M7 里程碑（键盘输入法这条线）的独立记录。UI 界面本地化（简体/繁体菜单、字体、翻译文件）是完全独立、已经基本收工的另一条线，见姊妹文档《[reMarkable Paper Pro Move 中文化开发方案](reMarkable中文化白皮书.md)》——那份文档也是本方案共享的 xovi hook 基础设施（第 04 节）、开发环境搭建（第 02 节）的出处，本文不重复展开，遇到需要交叉参考的地方会明确标注"见姊妹文档"。

**拆分说明：** 这两份文档原本是一份，拼音输入法这条线随着 M3/M4 陆续在真机上跑出大量调试记录，篇幅超过了 UI 汉化那部分的两倍且还在继续增长，遂拆分成两份独立文档，各自可以完整阅读，不需要来回切换。

## 本册目录

1. [01 · 总体架构图解](#总体架构图解)
2. [02 · 虚拟键盘 Hook 实现全过程](#虚拟键盘-hook-实现全过程)
3. [03 · 拼音引擎核心设计](#拼音引擎核心设计)
4. [04 · 双拼](#双拼复用同一个引擎只加一层键位转换)
5. [05 · 繁体输入](#繁体输入同样复用简体拼音引擎)
6. [06 · 候选词库](#候选词库)
7. [07 · 简拼与中英混输](#简拼与中英混输均已接入设备并真机验证)
8. [08 · e-ink 候选栏渲染要点](#e-ink-候选栏渲染要点)
9. [09 · 里程碑与时间表](#里程碑与时间表)
10. [10 · 风险清单与合规提醒](#风险清单与合规提醒)

附：[▶ 完整复现路径（从新机到当前进度）](#完整复现路径从拿到新机到当前进度) · [▤ 项目目录结构](#项目目录结构cang-jie)

## ▶ 完整复现路径：从拿到新机到当前进度

这一节把整条线拉成一条能从头照着走的主线——从我拿到一部全新的 Paper Pro Move，到现在 M3/M4/M6 真机跑通的状态，每步都链到下面对应的深入小节（踩坑细节都在那里，这里只列主干）。

1. **拿到新机 → 打开开发者模式 → 建立连接**：设备设置里开开发者模式（会清空数据），走 USB gadget 网卡 `10.11.99.x` 网段 SSH 进 `root@10.11.99.1`。交叉编译工具链、xovi 安装等环境搭建详见姊妹文档《中文化白皮书》02 节。
2. **装 xovi + `LD_PRELOAD` 注入**：手动维护合并的 systemd drop-in，把 `cangjie-langhook.so` 加进 `LD_PRELOAD`（不跑官方 `start` 脚本——它会覆盖我合并的配置）。`/etc` 是 overlay、真机重启会清掉，重启后需重建。
3. **离线反编译定位 hook 点**（Ghidra，`ghidra-project/`）：钉死 `VirtualKeyboard` 的按键统一入口 `FUN_00697610`、语言分派 `FUN_00695a60`、布局加载等的偏移与签名——先只读打日志确认真实内存布局，再写内存。见 02 节。
4. **拼音引擎：Python 原型 + 差分测试 → C 移植**：`pinyin-engine/src/` 先把音节切分/候选生成/双拼/简拼写通、穷举差分对拍，再逐字节移植成 `pinyin-engine/c/src/` 的 C。见 03/04 节。
5. **词库：下载快照 → 生成 blob**：rime-ice（8105/base/41448）+ iorest（pypinyin 注音）+ SymSpell（英文）下载进 `pinyin-engine/data/`，`c/tools/gen_*_blob.py` 生成 `dict.bin`/`dict.zh_tw.bin`/`dict_jianpin*.bin`/`english.bin`；简体侧过一遍 8105 白名单守卫的繁体归一化。见 06/07 节。
6. **编译 + 部署 + 健康检查**：`make aarch64` 零警告出 `.so`；部署前备份、`scp` `.so`+各 `.bin` 到 `/home/root/`、核对本地/设备 md5 一致、`systemctl restart xochitl`、确认 `is-active`=active / `MainPID` 变化 / `NRestarts` 不增。
7. **真机逐项验证**：地球键语言弹层 4 模式切换 → 逐字候选/分段提交/退格撤销 → 双拼免重启 → 繁体候选 → 中英混输。每步独立验证、出问题立即回退。
8. **当前进度**：M3（键盘弹出/按键拦截）、M4（候选可用/逐字造句/中英混输）、M6（双拼+繁体）均真机验证通过；M5/M7（打磨/长期维护）收尾中。详见 09 节里程碑。

## ▤ 项目目录结构（cang-jie）

> **目录已重构（2026-08-15）**：中文化核心交付归拢进 **`chinese-ime/`**（本白皮书在 `chinese-ime/docs/`）。文中出现的 `pinyin-engine/`、`xovi-extensions/cangjie-langhook/` 等旧路径，一律对应 `chinese-ime/pinyin-engine/`、`chinese-ime/langhook/`；`ghidra-project/`/`rmfw/`(固件镜像)/`weread-client/` 仍在顶层。

整个仓库的文件夹作用如下，**加粗的是拼音输入法这条线直接相关的**；UI 汉化那条线（字体等）见姊妹文档《中文化白皮书》。

- **`pinyin-engine/`** — **拼音引擎（这条线的离线核心）**——下面 6 个子目录。
  - `pinyin-engine/src/` — Python 参照实现：`segment`/`dictionary`/`jianpin`/`shuangpin`/`traditional`/`mixed_input`/`engine`/`syllables`。先在这里把算法写通、写满测试。
  - **`pinyin-engine/c/src/`** — **C 移植（编进 `.so` 的引擎）**：`segment`/`syllables`/`dictionary`/`jianpin`/`shuangpin` + 自动生成的 `*_data.h`。
  - `pinyin-engine/c/tools/` — blob 生成脚本：`gen_dict_blob`/`gen_english_blob`/`gen_jianpin_blob`/`gen_*_zh_tw_blob`/`blob_format`——从 Python 词典导出运行时 mmap 用的二进制。
  - `pinyin-engine/c/tests/` — C 单测 + 差分测试（`diff_check_dict`/`diff_check_jianpin`/`diff_check_shuangpin`）：Python 参照与 C 逐字节对拍。
  - **`pinyin-engine/data/`** — **词库数据快照**：`8105`/`base`/`41448`/`iorest` 词典 + `english/`(SymSpell) + 双拼 schema + 各 `LICENSE.*`/`PROVENANCE.*` 留痕。GPL/无许可数据只做独立文件、不编译进 `.so`。
  - `pinyin-engine/tools/` — 构建期工具：`annotate_iorest.py`（pypinyin 给 iorest 注音）。
  - `pinyin-engine/tests/` · `ui/` — Python 单测；`ui/` 是候选栏 QML 原型（`CandidateBar.qml` + PySide6 离屏渲染验证）。
- **`chinese-ime/langhook/`** — **设备端 hook**：`src/hook_init.c`（全部拦截/候选逻辑 + `_xovi_construct`/`_xovi_shouldLoad` 入口）+ `scan`/`pattern`/`trampoline`/`qstringlist_append` + `cangjie-langhook.xovi`（xovi 扩展描述），交叉编译成 `cangjie-langhook.so`——**合规 xovi 扩展**，放 `xovi/extensions.d/` 被自动加载（不再靠 LD_PRELOAD）。直接引用 `../pinyin-engine/c/src` 的引擎源码（不复制，避免两份漂移）。
- **`ghidra-project/`** — **反编译工程**：Ghidra 项目 + headless 脚本，定位 hook 点/偏移/参数签名的离线侦查产物。
- `rmfw/` — 固件/字体资源（`fonts/`、`out/`）——主要服务 UI 汉化那条线，见姊妹文档。
- `weread-client/` — 独立子项目（微信读书方案），跟 reMarkable 中文化无关，见《微信读书方案白皮书》，本文不展开。
- 工程纪律 · 三份白皮书 — 工程纪律；本册 + 《中文化白皮书》+ 《微信读书方案白皮书》。

## 01｜总体架构图解

虚拟键盘是全局命名空间的 `VirtualKeyboard` 类（`QQuickPaintedItem` 子类），xochitl 自己纯 QML + C++ 后端从零实现，不是 Qt 官方 `QtQuick.VirtualKeyboard` 模块，也不是系统级 `QPlatformInputContext` 输入法插件（这两条更省事的路线都已经用真机证据排除，见 02 节）。`cangjie-langhook.so` 用 `LD_PRELOAD`（通过一个 systemd drop-in，不改 xochitl 本体）注入进程后，对这个类打两条独立的 trampoline hook 链：一条改语言切换器（让"简体中文"/"繁体中文"出现在键盘自带的语言选择弹层里），一条改按键处理本体（拦截字符键、缓冲拼音、查词典、展示候选）。两条链共享同一份 `.so`，但触发条件和落地效果完全不同，下图分开画。

![总体架构 · 拼音输入法（两条 hook 链：语言切换器 + 按键处理→拼音缓冲→词典→候选栏）](architecture-ime.svg)

> 图注：`dict.bin` 是运行时才加载的外部数据文件，其余都在 `cangjie-langhook.so` 内；候选栏组件本身是 QMLDiff 运行时注入进 xochitl 资源系统的独立 QML 文件，不是编译进 `.so` 的代码。

> 📌 **竞品架构对照**：同赛道竞品 `boangs/rmkit`（rmkit-cn）在此处走的是**进程外**路线——拼音引擎是常驻 Go 服务、文本靠 uinput 模拟按键注入，而非本图这样进程内直接改道。两种架构的完整取舍与一条可留存的降级容错路径记在 10 节的 📌 借鉴块。

## 02｜虚拟键盘 Hook 实现全过程

**方案 A（移植 Qt Virtual Keyboard）vs 方案 B（自研轻量引擎）**

| 维度                      | 方案 A · 移植 Qt Virtual Keyboard                        | 方案 B · 自研 + 移植开源分词引擎           |
| ------------------------- | -------------------------------------------------------- | ------------------------------------------ |
| 起步速度                  | **快**——PinyinIME 现成代码，交叉编译进去就有基础可用版本 | 慢——UI 和引擎都要从零搭                    |
| 可控性/可定制             | 较低——要在别人的模块架构里改                             | **高**——候选栏样式、e-ink 适配都能按需定制 |
| 体积/资源占用             | 较大——整个模块一起搬                                     | **可控**——只取用到的部分                   |
| 和 xochitl 自定义键盘共存 | 需要处理两套键盘 UI 的切换/替换关系                      | **可以直接设计成"替换"而非"共存"**         |
| 推荐用途                  | **M3/M4 阶段快速验证可行性、跑出 demo**                  | **中长期正式版本的方向**                   |

**建议路径：** 先走方案 A 花两三周跑出一个"能用但粗糙"的 demo，验证 xovi hook 输入控件这条链路整体走得通；验证通过后再逐步往方案 B 迁移，把 UI 和引擎换成自己可控的实现。

### ✅ 架构侦查：真正的虚拟键盘实现是全局类 `VirtualKeyboard`，不需要自己造一套 QWERTY 布局

对解压出的 552 个 QML 文件做关键词扫描（`Keyboard`/`InputPanel`/`inputMethod`），命中 57 个文件，定位到虚拟键盘的真实实现——**模块 `xofm.modules.virtualkeyboard`，QML 类型 `VirtualKeyboard`**，是 xochitl 自己纯 QML + 一个 C++ 后端对象从零实现的，**不是** Qt 官方 `QtQuick.VirtualKeyboard` 模块，也**不是**系统级 `QPlatformInputContext` 输入法插件（原计划要验证的低风险路线，证据上不成立，排除）；键盘弹出/收起走标准 `Qt.inputMethod.show()`/`hide()`/`visible`，跟 Type Folio 物理键盘连接状态联动隐藏。**最有价值的发现**：`virtualKeyboard` 暴露了 `languages`/`language`/`languageName(code)` 这组 API，跟姊妹文档《reMarkable中文化白皮书》4.1 节 `LanguageSettings.availableLanguageCodes`+`getLanguageDisplayName()` 是同一种设计模式；插入文字统一走 `insertText(text, start, len)`，删除是独立的 `triggerBackspace()`。**意味着不需要自己实现一套 QWERTY 布局**——拼音敲的是拉丁字母，现有英文键盘布局直接能用，真正要做的是在 `insertText` 这条已有提交路径上插一层。

**反编译确认**（跟姊妹文档 4.1 节 Step F 反编译 `LanguageSettings` 同一套方法：字符串 xref → `qt_metacast` → vtable 找 `metaObject()` → 解码 `QMetaObject` 字符串表），纠正了一处此前基于类名相似的错误猜测——`xofm::libs::keyboardsettings::VirtualKeyboardControl`（`staticMetaObject`@`0x1255650`）几乎是空类，只有一个信号 `keyboardLanguageChanged`，跟 `languages`/`language`/`insertText` 这组 API 无关；`xofm::libs::keyboardinfo::KeyboardInfo`（@`0x10d4840`）只管物理键盘连接状态。**真正的宿主类是全局命名空间的 `VirtualKeyboard`**（`QQuickPaintedItem` 子类，`staticMetaObject`@`0x12677e8`，~14 属性/35 方法），方法表跟 QML 侧用法完全对得上。`insertText` 精确签名从汇编级确认：`qt_metacall` 里 method index 22/23/24 三个连续 case 是 moc 给带默认参数的 `Q_INVOKABLE` 生成的典型模式，真实声明是 `Q_INVOKABLE void insertText(const QString &text, int replaceFrom = 0, int replaceLength = 0)`，`replaceFrom` 是相对光标的偏移（`insertText("", -1, 1)`＝删除光标前 1 个字符）；`triggerBackspace()` 是独立零参方法（method index 25 → `FUN_00695820`）。记录一个当时不处理的已知风险：`languageName(code)`（method index 32）内部用 `QLocale` 做本地化名称解析，跟 `LanguageSettings::getLanguageDisplayName` 同一族逻辑，日后往 `languages` 追加 `zh_TW`/`zh_HK` 会重现姊妹文档 Step I 的重名 bug（后来 Step R 确实处理了，见下文）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2507`（`VK_STATICMETAOBJECT_VADDR`，Step K 反编译确认的地址）

### ⚠️ 按键 hook 点定位：两次假设被真机数据推翻，第三次找到真正入口 `FUN_00697610`

先按 Step F 的思路 hook `qt_metacall` 本地分派函数 `FUN_00695a60`——真机诊断日志显示其它方法 id 都正常命中，唯独 `insertText`/`triggerBackspace` 对应的 22-25 敲字全程未命中，说明普通按键根本没走 `qt_metacall` 分派（大概率是 QML AOT 编译器直接生成了对实现函数的调用）。改成直接 hook 实现函数本体 `FUN_00695970`/`FUN_00695820`——反汇编纠正了反编译器算错的参数个数（`mov x20,x1`/`w21,w2`/`w22,w3` 确认 `insertText` 真实是 4 参数：`this`/文本指针/`replaceFrom`/`replaceLength`），真机部署后 `triggerBackspace` 命中，但 `insertText` 敲字仍未命中——普通字符键根本不调用这个公开方法。回头查 `VirtualKeyboard` 覆盖的 `QQuickItem` 触摸/鼠标虚函数（`touchEvent`/`mousePressEvent`，vtable slot 26/33 找到），两者命中键位后都调用同一个函数 `FUN_00697610(this, key指针, isPress)`——反编译确认按键 `type` 字段（key指针+0x70）在这里分支：**0=字符/1=空格直接内联构造 `QInputMethodEvent`+`setCommitString`+`sendEvent`，完全绕开公开的 `insertText()`**；2=退格转调已验证过的 `FUN_00695820`；3/4/6=CapsLock/符号切换/Shift。真机部署验证字符键、退格键全部按预期命中，press/release 配对清晰，xochitl 全程健康。

**结论**：`FUN_00697610`（VADDR `0x697610`）是唯一、完整覆盖所有按键类型的 hook 点，不管触摸还是鼠标触发都会经过。代码见 `hook_init.c` Step L（insertText/triggerBackspace 实现函数）+ Step M（按键统一分发函数）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:740-741`（`VK_INSERTTEXT_IMPL_VADDR`/`VK_TRIGGERBACKSPACE_IMPL_VADDR`）、`:1169`（`VK_KEYHANDLER_VADDR`）、安装函数 `:1118-1130`/`:1132-1144`/`:3548-3560`

### ✅ Step N：字符键文本字段偏移——反编译+反汇编交叉核实，真机逐字符验证 100% 命中

反编译 `FUN_00697610` 字符键分支显示的取值写法没有直接采信（Step L 阶段反编译器算错过参数个数），改看原始反汇编：`ldr x1,[x20,#0x8]` 确认是真正的内存加载，`key_ptr+0x08` 存的是**指向 QString 的指针**，不是内嵌结构体。三个候选文本来源：普通态 `key_ptr+0x08`（有效性看 `+0x10`）、shift 态（基址整体 `+0x18`，等效 `key_ptr+0x20`，看 `+0x28`）、数字/符号覆盖态 `key_ptr+0x38`（看 `+0x40`，数字密码框专用）。反编译中一处"疑似重复的 `type==2` 判断"核实后确认是死代码（退格在更前面已经 `return`）。真机验证：三个候选槽位全部只读打印，我依次敲小写/大写（Shift）/数字符号键，日志与实际输入**逐一精确匹配**（`"a"/"A"`、`"3"/"{"` 等，符号层同样对上），数字/符号态候选全程 `guard=0`，符合预期（普通打字不触发）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:1175-1197`（`cj_log_vk_key_text_candidates`）

### M3/M4 现状速查

上面几段侦查/hook 定位过程的最终结论汇总，实现时直接查这张表。

| 项目                                                | 结论                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | 置信度                                                      |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| 虚拟键盘宿主类                                      | `VirtualKeyboard`（全局命名空间，`QQuickPaintedItem` 子类，`staticMetaObject`@`0x12677e8`）——不是 `VirtualKeyboardControl`                                                                                                                                                                                                                                                                                                                                                                                                           | 高（反编译+字符串表解码）                                   |
| 按键统一入口                                        | `FUN_00697610(this, key指针, isPress)`，VADDR `0x697610`——touch/mouse 触发都经过这里，**不是** `insertText()`                                                                                                                                                                                                                                                                                                                                                                                                                        | 高（反编译+真机部署验证命中）                               |
| 按键类型字段                                        | `key_ptr+0x70`（int）：0=字符、1=空格、2=退格、3=CapsLock、4=符号切换、5=回车、6=Shift                                                                                                                                                                                                                                                                                                                                                                                                                                               | 高（反编译+真机验证）                                       |
| 字符键文本（普通态）                                | `key_ptr+0x08` 存 `QString*`（需解引用），有效性看 `key_ptr+0x10`≠0                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | 高（反汇编 `ldr` 指令确认+真机逐字符验证）                  |
| 字符键文本（shift 态）                              | 基址整体 `+0x18`，等效 `key_ptr+0x20` 存 `QString*`，有效性看 `key_ptr+0x28`≠0                                                                                                                                                                                                                                                                                                                                                                                                                                                       | 高（同上，含符号层验证）                                    |
| 字符键文本（数字覆盖态）                            | `key_ptr+0x38` 存 `QString*`，有效性看 `key_ptr+0x40`≠0——数字密码框专用，普通打字不触发                                                                                                                                                                                                                                                                                                                                                                                                                                              | 中（反编译确认，真机未命中过，没法用真实输入验证）          |
| 退格实现                                            | `FUN_00695820(this)`，VADDR `0x695820`，零参，真机验证命中                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | 高                                                          |
| `insertText()` 签名                                 | `Q_INVOKABLE void insertText(const QString &text, int replaceFrom=0, int replaceLength=0)`——但普通打字**不经过**这个方法，只有 QML 主动调用（如长按删字、候选点选）才会走它                                                                                                                                                                                                                                                                                                                                                          | 高（反汇编+真机验证：装了 hook 但打字全程未命中）           |
| 已装 hook（观察类）                                 | `FUN_00695970`（insertText 实现，只打日志转调；Step T 复用它的调用桩来提交拼音缓冲区）——代码见 `hook_init.c` Step L                                                                                                                                                                                                                                                                                                                                                                                                                  | —                                                           |
| 已装 hook（改变行为类）                             | `FUN_00695a60`（`VirtualKeyboard` 本地方法/属性分派，Step R+S：`languageName()` 重名修复 + `languages` 追加 `zh_CN`/`zh_TW`）、`FUN_00695820`+`FUN_00697610`（退格/按键分发，Step T：中文模式下拦截拼音字符/空格/回车/退格，Step V：接入真词典候选+分页）、`FUN_00c6f3c0`（键盘布局加载，Step U：中文语言代码换成 `en_US` 兜底布局查找）                                                                                                                                                                                             | 高（真机反复验证）                                          |
| `KeyPopup` 复用（候选栏载体，**已废弃，见下一行**） | `virtualKeyboard.childItems()` 取到 `KeyPopup` 实例指针 → `setProperty("keys"/"visible"/"targetRect", ...)` 在 `call-through 之后`写入才能稳定跨按键存活；翻页后原生 `onReleased` 会同步覆盖，改用 `QMetaMethod::invoke`+`QueuedConnection` 延迟到下一个事件循环重新展示（Step V-2）；内部自带的 `autoHideTimeout` 原生 Timer 已找到并直接停用（Step W）——**双拼真机测试暴露出候选快速变化时的叠影问题，四轮修复尝试均未解决根因，最终判定 `KeyPopup` 组件本身不适合做常驻候选条，Step BB 起改用自定义组件，本行内容降级为历史记录** | 高（真机反复验证；但 Step BB 之后已不再是当前实现）         |
| `CjCandidateBar`（Step BB 起的现行候选栏组件）      | 装 `xovi`+`qt-resource-rebuilder` 扩展，用 QMLDiff 语言在 `KeyboardPanel.qml` 里注入一个全新 `Rectangle`（不挂 `parent: virtualKeyboard`，避免其异步重挂载导致锚定失效）；C 端用 `QQuickItem::parentItem()`+`childItems()`+`objectName` 匹配定位实例，读写机制复用既有 `QObject::setProperty()`；自带 `expanded`/`previewText` 属性，不受原生 `autoHideTimeout` 影响（组件本身没有这个机制）                                                                                                                                         | 高（真机反复验证：可见性、点击提交、多轮 UI 迭代）          |
| `languages` 属性内存布局                            | `call_type==1(ReadProperty) id==0`——**不是** `id==1`（那是单个字符串属性 `language`，size 是字符数不是元素数，误判导致过一次真机崩溃，已修复并重新验证）                                                                                                                                                                                                                                                                                                                                                                             | 高（真机 hex dump 逐字节确认，非猜测）                      |
| 键盘布局按语言加载                                  | `FUN_00c6f3c0(this, code)` 按语言代码查 JSON 布局资源，查不到就让行/键容器保持空——`zh_CN`/`zh_TW` 天然查不到，这是切中文后按键网格消失的根因                                                                                                                                                                                                                                                                                                                                                                                         | 高（反编译+真机验证，Step U）                               |
| 候选词典查询                                        | `dict.bin` 独立文件 mmap 只读（不编译进 `.so`，GPL v3 数据隔离取舍，见 06 节），排序数组+二分查找，精确匹配+前缀匹配合并去重，真候选替换掉早期版本的拼音音节展示                                                                                                                                                                                                                                                                                                                                                                     | 高（差分测试 19/19 一致+真机真汉字候选验证，见下方 Step V） |

### ⚠️ 候选栏 + 语言切换器接入：复用原生 `KeyPopup` 组件，含一次真机崩溃事故

**拿到 `KeyPopup` 指针**：候选栏复用键盘自带的长按重音弹窗 `KeyPopup`（给字符串列表、渲染可点候选、选中调 `insertText` 提交，结构跟拼音候选栏几乎一样）。`QObject::children()`（内存归属树）返回 0 个结果——QML `parent:` 绑定的是 `QQuickItem` 视觉父级，不是 `QObject` 归属父级，两棵树不同；改用 `QQuickItem::childItems()` 命中。**写入验证**：`QObject::setProperty()` 确认能写 QML 编译期合成的属性，但写完后下一次按键就被原生的长按检测预备逻辑覆盖——把写入从 `call-through` 之前挪到之后，连续多次按键值能稳定存活。

**方向确认**：我要求走"原生语言切换器"接入（跟切换西班牙语键盘一样的路径），推翻了此前"不新增语言代码，重名 bug 不会触发"的判断。反编译 `FUN_00695a60`（`VirtualKeyboard` 本地方法/属性统一分派函数）一次解决两个目标：`languageName()`（`call_type==0 id==0x20`）+ `languages` 属性（`call_type==1`，一开始误判成 `id==1`）。

**真机崩溃事故**：部署后点击搜索框弹出键盘触发崩溃（`xochitl` 被 systemd 自动拉起）。诊断：`id==1` 实际是单个字符串属性 `language`（如 "en_US"，`size=5` 是字符数），追加逻辑把它当成 5 个 QString 元素遍历，读到的"引用计数指针"其实是字符串字节，解引用直接写坏内存。**立即回退**，加一版纯只读诊断 hook（只 dump 十六进制字节，不解引用）真机核实：`id==0` 才是真正的 `languages`（size=17，元素预览 "da_DK"/"nl_NL"/"en_US"）。改用 `id==0` 后重新部署，不再崩溃。**经验教训**：看到"数据形状像 QStringList"就直接采信 case 编号写追加代码，跳过了"先只读 dump 确认真实布局"这一步——本项目从姊妹文档 Step F 开始反复强调的纪律这次漏掉了，直接导致真机崩溃；往后任何新的写属性场景，形状再眼熟也要先过一遍只读验证。

**显示名微调**：真机确认新增语言项后，`zh_CN` 显示成英文 "Chinese"、`zh_TW` 带地区括注"繁體中文（台灣）"——原因是复用了专为 `LanguageSettings`（zh_CN/zh_TW/zh_HK 三项共存，需要地区括注区分）设计的显示名函数；`VirtualKeyboard` 只有两项，没有撞名问题，也需要显式覆盖 `zh_CN`。新增专用的 `cj_vk_override_language_name()`，真机最终确认干净显示"简体中文"/"繁体中文"。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:566`（`VK_DISPATCH_VADDR`）、安装函数 `:615-625`；`:659`（`VK_LOADLAYOUT_VADDR`，Step U 布局兜底 hook，安装 `:700-710`）

### ✅ 拼音缓冲状态机（Step T）真机全项验收通过——过程中额外发现并修复两个真机专属 bug

**实现**：中文模式判断用 `QObject::property()`+`QVariant::toString()`；重新逐分支核对 `FUN_00697610` 确认三个关键事实：① release 事件在绘制/发信号之后必定提前 `return`，只有 press 需要处理；② 退格分支直接调用已有 hook 点 `FUN_00695820`，不需要改按键分发函数本身；③ 空格/回车在这个函数里内联直接处理，必须在这里拦截。拦截逻辑：中文模式下字符键把字母缓冲进静态 buffer、调用 `cj_segment()` 切分、写入 `KeyPopup.keys`、跳过 call-through；退格吸收缓冲区最后一个字符；空格/回车在缓冲区非空时把内容通过 `insertText` 调用桩原样提交，缓冲区为空时行为完全原生。

**真机发现：切到中文后按键网格整体消失**——不是 Step T 引入的，是语言切换器接入的设计遗漏：反编译确认 `language` 属性写入会转调 `FUN_00c6f3c0(this, code)` 按语言代码加载键盘布局 JSON，查不到就直接跳到函数末尾 `return`，行/键容器保持空——`zh_CN`/`zh_TW` 从来不是原生支持的语言，天然查不到布局资源。**修复**：hook `FUN_00c6f3c0`，传入语言代码是中文时换成确认真实存在的 `en_US` 再调用原函数，布局查找成功、按键网格正常渲染；不碰 `this` 上任何字段，`language` 属性依然正确汇报中文。

**真机发现：空列表写入静默失败**——真机 `QArrayData::allocate(capacity=0)` 返回空指针，被误判成分配失败而放弃写入（`visible=false` 本身写成功，候选框正确隐藏，只是内部 `keys` 没真的清空）；本机单测的假 `allocate` 对 `capacity=0` 碰巧返回非空指针，掩盖了这个差异——再一次印证真机行为不能只靠 host 单测替代验证。修复：`count==0` 时不调用 `allocate`，直接写 `{d=NULL,ptr=NULL,size=0}`。

**真机验收清单（全部通过）**：切中文打字不直接落字、候选栏随打字刷新音节切分；退格只回退缓冲区不误删已提交内容；空格/回车在缓冲区非空时提交、为空时行为完全原生；切回英文后字母键完全恢复原生输入；全程无崩溃。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c`（按键分发 `cj_vk_keyhandler_handler`:3046-3169、退格 `cj_vk_trigger_backspace_handler`:1063-1116、提交 `cj_step_t_commit_buffer`:2946-2976、刷新主流程 `cj_step_t_refresh_candidates`:2738-2875）

### ✅ Step V：候选词典真正接入——真汉字候选替换拼音音节展示，过程中发现并修复三个真机专属 bug

**接入方式**：`dict.bin` 存放在 `/home/root/`（跟 `cangjie-langhook.so` 同一目录，见 06 节"磁盘空间调研"——`/home` 是独立加密分区，45.8GB 可用，25MB 的 `dict.bin` 完全不是问题），运行时只读 `mmap`，**不编译进 `.so` 本体**（架构取舍见 `pinyin-engine/c/src/dictionary.h` 顶部说明，法律背景见 10 节）。查询用最优切分（`cj_best_segmentation`，不是全部候选切法）：完整音节走精确匹配（`cj_dict_lookup`），"打到一半"的最后一段走前缀匹配（`cj_dict_lookup_prefix`）追加，按词去重合并——跟 `pinyin-engine` 的 `engine.py::query()` 同一个设计思路，只是这次是 C 版、直接查 `mmap` 好的 `dict.bin`。真候选是真汉字，需要真正的 UTF-8→UTF-16 解码（新增 `cj_build_utf8_qstring`，支持 4 字节序列/代理对——41448 大字表含 Unicode 扩展区生僻字，此前拼音音节展示阶段只需要 ASCII 零扩展，不够用了）。

**真机 bug 1·候选优先级**：我实测报告"打完 `pin` 再按 `y`，候选框无变化，感觉 `y` 没按中"。根因：最初设计是"完整音节精确匹配优先，pending 前缀匹配只在还有剩余槽位时才补"——"pin"单独精确匹配就有 50 个真候选（品/拼/贫/频/聘…），把展示槽位直接占满，之后不管 pending 后缀怎么变（y/yi/yin…），精确匹配部分纹丝不动，候选栏对后续按键完全没有视觉反馈——这不是极端情况，是绝大多数≤4 字母声母音节都会踩到的常见路径。**修复**：pending 存在时前缀匹配优先，完整音节精确匹配退化成补位（前缀匹配填不满展示槽位时才用来填充剩余部分），没有 pending 时行为不变。

**真机 bug 2·候选太多选不到**：我原话"pin 如此多候选词，我无法选到看不到的字"——只展示前 5 个候选，剩下 45 个字完全没法选中。**修复**：一次查询多缓存一批（`CJ_STEP_V_CACHE_CAP=64`，8 页），按页切片展示（`CJ_STEP_V_PAGE_SIZE=8`，在真机验证过的 `KeyPopup` 安全上限 18-20 个内留足余量），超过一页时末尾追加固定翻页标记 `">>"`（内容跟任何真候选都不会撞，词典候选全部是真汉字）。点击翻页标记不提交文本，而是在 `cj_vk_insert_text_handler` 里识别出这个标记，翻到下一页（翻到底绕回第一页）并重新刷新。已知边界：候选数超过 `CJ_STEP_V_CACHE_CAP=64` 的极端音节，翻到第 8 页之后的字选不到，是这一版明确接受的边界，不是 bug。空格/回车提交时永远提交 `g_step_v_cached_candidates[0]`（第一名，不随翻页变化），符合主流拼音输入法"空格=确认最佳候选，无需先翻页"的习惯。

**真机 bug 3·翻页与闲置计时器不同步**：我连续翻了 8 页停下来看最后一页，10+ 秒后按字母键，触发了 `cj_step_t_expire_buffer_if_stale()` 把缓冲区当"闲置超时"清空（日志证据：`缓冲区 "ni" 闲置超过 10 秒...静默清空`）。根因：`g_pinyin_buffer_last_activity` 只在字母键/退格时更新（在 `cj_vk_keyhandler_handler` 里），翻页点击走的是完全不同的函数（`cj_vk_insert_text_handler`），从未更新过这个时间戳。**修复**：翻页也算活跃操作，在翻页处理逻辑里同步刷新时间戳，跟字母键/退格用同一个变量、同一个 10 秒阈值。

**候选提交与外部同步**（这一段描述的是 Step V 当时的行为，真机 bug 4 之后已经拆开，见下方新增 callout）：我点选 `KeyPopup` 候选时 QML 侧 `onKeySelected` 会直接调用 `insertText`，这次调用不经过我自己的调用桩（trampoline 直调原函数体，不重新触发 patch 过的入口），据此判定是"外部触发"——此时若拼音缓冲区还非空，判定状态已不可信，清空缓冲区+隐藏候选栏重新同步（fail-safe：宁可丢弃一次不确定状态，不在错误基础上继续构建）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2026-2124`（词典/handle 选择）、`:2321-2410`（候选生成 `cj_step_v_generate_dict_candidates`）、`:2452-2472`（展示 `cj_step_v_display_page`）、`:2946-3044`（提交 `cj_step_t_commit_buffer`/`_by_index`）、`:937-1037`（外部 insertText 同步）；UTF-8 真解码见 `qstringlist_append.c:290-338`

### ✅ 真机 bug 4·回车被 Step V 误合并成"确认候选"，跟空格没了区别（05 节繁体双 blob 真机验证时顺带发现）

我实测反馈"按回车不是输入英文而是输入第一候选字"——Step V 接入真候选那版，把"空格/回车提交缓冲区"这段代码改成"优先提交第一名真候选词"时，没有区分空格和回车，两个键从此共用同一段逻辑，导致回车也变成了确认候选。但主流拼音输入法的通用约定是**空格＝确认默认候选，回车＝放弃候选、原样提交刚敲的拉丁字母**（常见场景：打到一半发现其实想打英文单词，不想经过候选栏）——这个约定在 Step T 最初的设计文字里其实有提到，只是 Step V 加真候选的时候两个键的分支被合并到了一起，这个区别在合并时丢掉了，是真机测试才暴露出来的疏漏，不是新需求。

**修复**：`cj_step_t_commit_buffer()` 新增 `force_raw_pinyin` 参数——为真（只有回车这一处传）时无条件提交缓冲区原样拼音字母，不管候选栏当前有没有真候选；为假（空格、以及退格之外几处"先冲掉悬空缓冲区"的安全网调用点）维持原行为。三处调用点里只有空格/回车共享的那一处需要按 `key_type` 区分，其它两处（普通态文本异常兜底、CapsLock 等其它按键的安全网）跟空格/回车无关，保持 `force_raw_pinyin=0` 不变。

**真机验收**：敲一段拼音，候选栏出现真候选后按回车——插入原样字母（比如 `zhongguo`），不是"中国"；同样内容改按空格——插入候选字"中国"，行为不变；退格/切换语言不受影响；全程 `xochitl` 无崩溃。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2946-2976`（`cj_step_t_commit_buffer` 定义）；三处调用点在 `cj_vk_keyhandler_handler`:3046-3169 内

### ⚠️ Step V-2：候选栏一点翻页就消失——根因是原生 `onReleased` 必然在我写入之后同步执行

候选词典接入后翻页可以点、也能翻，但**点完候选框立刻消失**。重新用 zstd 解压出真实的 `KeyPopup.qml` 源码（跟定位其它 QML 文件同一套工具链）核实到第 117 行：

```qml
onReleased: { keySelected(symbol.text); root.visible = false; }
```

不管点的是候选栏哪一格（包括我自己放的翻页标记 `">>"`），点完都会自动关闭弹窗——`keySelected(symbol.text)` 同步发射信号，会一路同步执行到我的 `insertText` hook 返回，然后 `root.visible = false` 才执行。也就是说这行代码在时间顺序上**必然**发生在我的 hook 写完 `visible=true` 之后——不是时序偶然，无法靠"写快一点"赢过它，是确定性的调用链顺序问题。

**解法**：让我的写入不再发生在这次 `onReleased` 处理的同一个调用链里——用 Qt 官方的 `QMetaMethod::invoke`+`Qt::QueuedConnection` 给自己排一个"稍后处理"的 `insertText` 调用（带专属标记内容，跟点击翻页标记的语义区分开），会在当前事件循环这一轮完全处理完（含原生 `root.visible=false` 已跑完）之后才真正触发。关键构件：`VirtualKeyboard::staticMetaObject` 地址（早在反编译阶段就确认，这是第一次真正在运行期使用）；枚举方法表确认 `methodCount=100`，找到 3 个 `insertText` 重载，全局 index=87/88/89——**这跟反编译阶段记录的"method index 22/23/24"是完全不同的两套编号**（前者是分派函数内部经继承链累加过的本地 id，后者是 `QMetaObject::method()` 的全局 index），靠方法名重新枚举、不硬编码固定数字，避免固件小版本更新后编号漂移导致悄悄调错方法。

**两阶段验证纪律**：先部署一版只枚举、只打日志、不真正调用的诊断版本，真机确认 `methodCount`/找到的 index 符合预期后，才加上真正的 `invoke()` 调用——跟 Step R+S 崩溃事故之后重新确立的"先只读验证形状，再写"纪律一致。

**真机验证结果**：延迟刷新机制每次翻页点击都精确成对出现"排队延迟刷新调用成功"→"延迟刷新调用到达，重新展示候选栏"，日志里没有一次失败，全程无崩溃。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2507`（`VK_STATICMETAOBJECT_VADDR`）、方法枚举 `cj_step_v2_ensure_inserttext_method`:2585-2617、`cj_step_v2_queue_refresh`:2633-2657；QueuedConnection 标记处理在 `cj_vk_insert_text_handler`:937-1037 内

### ✅ Step W：候选栏停顿 10 秒后自动消失——根因是 `KeyPopup` 自带独立原生 Timer，排除二进制 patch 方案，改为直接找到并停用它

Step V-2 解决了"点了就消失"，但我继续测试发现候选栏还会在**停顿一会儿（不是紧接着点）之后**消失——而且暴露出一个更细致的连锁问题：打"pi"后停顿，候选框自己隐身，但内部拼音缓冲区没有同步清空，这时候再打一个字母会拼接成"pii"而不是重新开始。根因是 `KeyPopup.qml` 自带的 `readonly property int autoHideTimeout: 10000`——背后是一个独立的原生 QML Timer，跟我自己的缓冲区状态完全不同步：候选框"看起来没了"，但缓冲区逻辑上还没到期，才会让人误以为上一次输入已经结束。我明确要求：不是"接受现状、只保证恢复路径顺畅"，而是**彻底消灭这个自动隐藏**，候选栏只要有候选就应该一直显示，隐藏的唯一权威改成我自己的缓冲区状态机。

**排除的方案：直接 hook `QQuickItem::setVisible`**——反汇编 `libQt6Quick.so.6.10.3` 发现这个库是用 **BTI（Branch Target Identification）编译**的（`readelf -n` 确认 `AArch64 feature: BTI, PAC`），`QQuickItem::setVisible` 函数开头是一条 `bti c` 落点指令——它是跨库导出符号，会被间接跳转触达（反汇编也证实它本身就是读虚表再 `br x16` 跳到 `QQuickItemPrivate::setVisible`），覆盖掉这个落点会让所有从别处间接跳过来调用它的地方触发 Branch Target Exception 直接崩溃。这个函数在整个界面里任何控件的显隐变化都会经过，风险是全局性的，跟此前所有 hook 目标（只被 xochitl 自己代码直接调用、天生不需要 BTI 落点的内部未导出函数）完全不是一个风险量级，予以排除。

**采用的方案：找到并直接停用那个原生 Timer 本身**，全程走已验证成熟的 `QObject::setProperty()`，不涉及任何二进制 patch，没有 BTI 问题。Timer 是非可视对象，不出现在 `childItems()`（视觉树）里，只能走 `QObject` 归属树——此前第一次尝试 `qt_qFindChildren_helper` 从错误对象（`virtualKeyboard`）出发返回 0 个结果，这次**反向复用同一个教训**：从已经正确找到的 `keyPopup` 自己出发查询它的 QObject 子对象（父子关系是对的）。精确签名（本机 Qt6.11 头文件核对，真机 `nm` 核实 `libQt6Core.so.6.10.3` 导出）：

```c
void qt_qFindChildren_helper(const QObject *parent, QAnyStringView name,
                             const QMetaObject &mo, QList<void*> *list,
                             Qt::FindChildOptions options);
```

"空名字"位模式 `{0, 0x8000000000000000}` 复用此前已验证过、调用不报错的编码；`metaObjectFilter` 传 `&QObject::staticMetaObject` 匹配任意 QObject 派生类；`options` 传 1（`Qt::FindChildrenRecursively`）。**第一次部署即精确命中**：`qFindChildren` 返回 11 个 QObject 子项，第一个就是唯一的 `QQmlTimer`，其余是 `QQuickGrid`/`QQuickRepeater`/`QQmlDelegateModel`/`QQuickRectangle`/`QQuickPen` 等候选渲染内部对象。读它的 `interval` 属性核实身份：**精确等于 10000**，跟 `autoHideTimeout` 声明值完全对上。

**停用**：`QObject::setProperty(timer, "running", false)`——持续每次候选栏更新（唯一写入 `KeyPopup` 属性的入口）都顺带重新 assert 一次，不假设"停一次就永久生效"（跟此前"`keys`/`visible`/`targetRect` 每次按键都要重新写"是同一个道理，因为不确定原生逻辑会不会在看不到的时机重新 `start()` 它）。**结果**：隐藏候选栏的唯一权威变成我自己的空闲过期判断（缓冲区真正过期才清空+隐藏），视觉状态和逻辑状态严格同步，"pii"拼接错觉随之消失。真机验证：日志确认 `Timer.interval = 10000`（身份核实成功）+ 每次候选栏更新 `running=false 写入成功`（持续生效），全程 `MainPID` 不变、`active`，我测试确认"已生效"。

**M3/M4 候选栏行为闭环**：真汉字候选（Step V 词典）+ 分页（Step V）+ 翻页后重新展示（Step V-2 `QueuedConnection`）+ 停顿后不再消失（Step W），四层机制组合起来，候选栏行为达到"只要有候选就应该一直显示，直到真正提交/放弃/切换语言"的预期。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:1355-1403`（排除 hook `setVisible` 的 BTI 论证）、`:1689-1715`（`cj_step_w_stop_autohide_timer` 身份核实+停用）、接入唯一写入入口在 `cj_step_t_update_popup`:1860-1927 内

### ✅ Step Y：双拼（小鹤）真正接入设备——C 移植 + 键盘按键事件接入，跟全拼共用同一套下游流水线

**C 移植**：04 节"复用同一个引擎，只加一层键位转换"这句话，落地时没有把 RIME 官方 schema 的 algebra 正则替换规则链搬进 C（新增一个正则引擎依赖，风险类比 05 节排除"把 OpenCC 静态链接进 `.so`"的顾虑）。验证过双拼解码是逐 2 键分组独立解码、组间互不影响之后，改成穷举全部 26×26=676 种按键组合、把 Python 参照实现（`src/shuangpin.py`）的解码结果整张打印成 C 静态查找表（`shuangpin_data.h`，生成脚本 `gen_shuangpin_table.py`），运行时纯查表，零正则、零动态解析。差分测试穷举全部 676 种组合 + 几个多音节/半音节边界场景，**682/682 逐字节完全一致**。

**接入方式**：双拼开启时，`g_pinyin_buffer` 里存的是双拼按键原文（不是拼音字母），`cj_step_t_refresh_candidates()` 一开始先用 `cj_shuangpin_keys_to_pinyin()` 转换成标准拼音字符串（音节间用 `'` 分隔），之后不管是查词典还是拼音切分兜底展示，全都统一用转换后的结果（`seg_src`）——04 节设计"不需要单独的引擎"就是这样落地的，转换只做这一次，下游代码不需要为双拼单独分支。

**开关机制（Phase B 起改为语言弹层实时切换）**：早期用一个约定路径的标记文件 `/home/root/.cangjie_shuangpin_enabled`（惰性检查一次、切换需重启 `xochitl` 才生效）做过渡开关；Phase B 把四个中文模式（简体全拼/繁體全拼/简体双拼/繁體双拼）各编码成一个伪语言代码追加进 `virtualKeyboard.languages`，我点地球从原生语言弹层里选（跟切西班牙语同一路径），双拼状态编码进 `language` 属性、每次按键读取得出，**切换立即生效、无需重启**——标记文件机制已被取代删除。详见下方 Phase B。

> 源码：`pinyin-engine/c/src/shuangpin.c`/`shuangpin.h`、`pinyin-engine/c/tools/gen_shuangpin_table.py`、`pinyin-engine/c/tests/test_shuangpin.c`、`diff_check_shuangpin.py`；设备接入见 02 节 Phase B——双拼开关随 Phase B 改为语言弹层实时切换（真源表 `xovi-extensions/cangjie-langhook/src/hook_init.c` 的 `CJ_VK_MODES`:411-436、解析 `cj_step_t_language_mode_ex`:2892）

### ⚠️ 候选栏叠影排查——四轮低风险实验全部失败，最终定位到 `KeyPopup` 组件本身不适合做常驻候选条

双拼真机测试暴露了一个 Step V/V-2/W 都没解决的新问题：候选数量变化快时（尤其双拼下相邻两次按键经常在"完整音节+一大批真候选"和"半个音节+寥寥几个候选"之间跳变），候选栏会出现多份候选文字叠在同一屏幕位置的视觉重影（真机截图证实：连续敲同一个字母时能看到好几份候选互相压着）。依次尝试了四轮修复，思路一次比一次更接近底层，但**全部没有真正解决**：

1. **显示节流**：内部候选状态每次按键照常重新计算，只节流"多久真正刷新一次可见候选栏"（不到 80ms 跳过这次可见刷新，复用 Step V-2 的延迟刷新机制补一次）——日志确认节流逻辑本身生效，但复现叠影时相邻按键间隔往往并不短，说明"按键太快"不是主因。
2. **缩小每页候选数量**：从 8 降到 5（跟拼音切分兜底路径的单行安全值统一，避免两条路径在"单行"和"两行"之间来回切换）——依然复现。
3. **动态撑满 `targetRect` 宽度**：读一次 `virtualKeyboard.width` 撑满候选栏定位框，真机日志确认写入生效（`w=954`），但候选栏视觉上没变宽——**targetRect 根本不控制 `KeyPopup` 自己的可见尺寸**，大概率只是原生长按逻辑用来定位箭头指向的辅助信息。
4. **直接写 `KeyPopup` 自己的 `width`/`height`**（标准 `QQuickItem` 属性，不是 `targetRect` 那种自定义 property）——这次真机确认视觉上真的变宽变高了（写入+读回校验完全对得上），但叠影依然存在。

**结论**：四轮实验分别验证了时机/数量/宽度/尺寸四个维度都不是根因，说明卡住的地方不是这些表面参数，而是 `KeyPopup` 内部 `Repeater` 在候选数量变化时的重建机制本身有问题——`KeyPopup` 从设计上就是"长按弹出重音符号"用的浮动小弹窗，不是为"常驻候选条、内容频繁变化"设计的组件，继续在它身上打补丁大概率还会有下一个坑。决定不再调 `KeyPopup`，转向改造/注入新组件，见下方 Step BB。

### ✅ Step BB：放弃 `KeyPopup`，改用 QMLDiff 注入全新候选条组件——装 `xovi`/`qt-resource-rebuilder`、学 QMLDiff 语言、真机验证全过程

**调研纠正了一个最初的误判**：早前反复用 zstd 从 `xochitl` 二进制里解压 QML 文件做只读分析，一度让人以为"改 UI 资源"必然要动 `xochitl` 二进制本身，风险类别会跳到"改写磁盘上的可执行文件"。实际调查 `asivery/rm-xovi-extensions` 仓库下的 `qt-resource-rebuilder` 子项目（正确地址：白皮书曾经引用过的独立仓库地址已经 404，真实位置是这个 monorepo 的子目录）才发现：它是一个**运行时拦截**的 `xovi` 扩展（override `qRegisterResourceData`），在 `xovi` 启动时实时"重建" Qt 的资源数据库，**完全不需要碰 `xochitl` 二进制文件**——跟 `cangjie-langhook.so` 是同一个风险类别（注入进程、运行时生效），不是本项目此前唯一没试过的"改写磁盘可执行文件"。

**安装踩的坑**：官方 `xovi/start` 脚本会把 `/etc/systemd/system/xochitl.service.d/` 整个挂一个 tmpfs、只写入它自己生成的 `LD_PRELOAD=/home/root/xovi/xovi.so`——如果照官方步骤跑，**会把已有的 `cangjie-langhook.conf` 直接覆盖没了**。改成手动写一个合并了两个库的 drop-in（`LD_PRELOAD=/home/root/xovi/xovi.so:/home/root/.local/share/cangjie-ime/cangjie-langhook.so`，外加 `XOVI_ROOT`/`QML_DISABLE_DISK_CACHE` 等环境变量），不用官方脚本。`cangjie-langhook.so` 与 5 个词典 blob 现遵循 XDG Base Directory 布局统一放在 `/home/root/.local/share/cangjie-ime/`，不再散落 `/home/root/` 根目录。`rebuild_hashtable`（首次装好后必须跑一次，给 `.qmd` 用的版本专属哈希表）本身会临时停掉正在跑的 `xochitl`、用一份不含 `cangjie-langhook.so` 的纯净 `xovi` 单独跑一次，且脚本原文要求"设备提示时输入密码"——这一步不是纯后台操作，需要我在设备旁边配合。跑完之后 `xovi.so`+`cangjie-langhook.so` 一起加载真机验证过零冲突（各自的 hook 互不干扰，pinyin 全部功能正常）。

**QMLDiff 语言踩的坑（跟官方文档字面意思不完全一致的地方）**：① 树选择器 `TRAVERSE Item > Item[#id]` 的第一个 token 匹配的是**当前 TRAVERSE 根自己**，不是它的第一个子级——裸 `TRAVERSE Item` 会成功匹配到文件根节点本身（报错从"Cannot locate"变成"Cursor not set"证实了这一点）；② **id 过滤器要用 `#id`**（对象 ID），`:id`（"对象在父级中的名称"）是另一个不同的概念，一开始按官方 README 示例搞反了；③ **`INSERT` 前必须先 `LOCATE`** 设置光标位置，不能直接 `INSERT`（报错 "Cursor not set! Use the LOCATE or REPLACE directive first."）。这些都是靠"部署→读 `[qmldiff]:` 日志里的具体报错→改一处→再部署"这个低风险循环（qmldiff 解析失败只是记日志、不影响 `xochitl` 正常运行，不会崩）逐条试出来的，不是一次读文档就写对的。

**真机反复验证才找到正确的组件挂载方式**：最初仿照 `KeyPopup` 写 `parent: virtualKeyboard` 并锚定它的边（`anchors.left/right/top: virtualKeyboard.left/right/top`），部署后候选条完全看不见——排查了颜色对比度（红→白→纯黑，我"仔细努力看了"才勉强确认看到过一次红色）都没有实质进展，直到换成**不写 `parent:`、绝对坐标定位**的极端测试版本才第一次确认可见，反推出真正的根因：`virtualKeyboard` 是在 `KeyboardPanel.qml` 的 `Component.onCompleted` 里才被**异步**重新挂载/铺满父级（`virtualKeyboard.parent = keyboardLayout; virtualKeyboard.anchors.fill = keyboardLayout;`）——我的兄弟节点如果在声明时就锚定它的边，绑定时机对不上。改成默认隐式挂在 `keyboardLayout` 下（不写 `parent:`），锚定改成相对 `parent`（`keyboardLayout` 自己，同步绑定的 `width`/`height`，不涉及那个坑），第一次真机验证即成功——一条贴着键盘顶部、跟键盘等宽的横条稳定显示。

**C 端怎么找到这个新组件**：`CjCandidateBar`（QMLDiff 里定义的普通 `Rectangle`，`objectName: "CjCandidateBar"`）挂在 `keyboardLayout` 下面，是 `virtualKeyboard` 的"上级"（`virtualKeyboard.parent = keyboardLayout` 这行代码本身就是把 `virtualKeyboard` 变成 `keyboardLayout` 的子项），从 `virtualKeyboard` 出发用 `childItems()` 天然找不到它。反过来：先用 `QQuickItem::parentItem()`（真机 `nm` 核实的导出符号，跟 `childItems()` 同一个函数指针形状）拿 `virtualKeyboard` 自己的视觉父级，再对这个父级调用 `childItems()`，在返回的兄弟节点里按 `objectName`（不是类名前缀——普通 `Rectangle` 没有独立的 C++ 类名）匹配。写属性（`keys`/`visible`）复用 Step P 已经验证成熟的 `QObject::setProperty()` 机制，不需要新写入手法；**不再需要 `targetRect`/`width`/`height` 手动定位那一整套代码**（宽高位置全部交给 QML 声明式 `anchors` 处理），也不再需要 Step W 那套"停用原生 `autoHideTimeout`"逻辑——`CjCandidateBar` 是自己定义的 `Rectangle`，没有任何原生自动隐藏行为需要对抗，这是这次改造顺带获得的简化，两段旧代码标记 `__attribute__((unused))` 保留完整踩坑记录，不删除。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:1646-1680`（`cj_find_candidatebar`）、`:1860-1927`（`cj_step_t_update_popup` 改造后）；QMLDiff 补丁文件部署在设备 `/home/root/xovi/exthome/qt-resource-rebuilder/candidatebar.qmd`；`xovi` LGPL-3.0、`rm-xovi-extensions`（含 `qt-resource-rebuilder`）GPL-3.0，两份都是运行时加载的独立 `.so`、不编译进 `cangjie-langhook.so` 本体，法律定性同 06 节 rime-ice 的取舍逻辑，10 节需要同步补一条

### ✅ Step CC：贪心多词拼句——把 Python 版 `engine.py::_compose()` 早就有的能力补进 C 移植版

双拼真机测试暴露了 Step V 阶段就明确标注过的已知限制："只用整段精确匹配词典查一次，查不到直接放弃"——我实测反馈打"这是一句话"这种五个字的完整小句（词典里不会整条收录这样的短语）打到一半就从真候选退化回拼音字母展示。移植 Python 端早就写好、单测覆盖过的算法：`dp[i]` 表示"从第 `i` 个音节开始能拼出的最佳整句"，从右往左填表，每个位置优先尝试更长的整词（最长匹配优先），命中且后半段也有解就立即拼接、`break`（贪心，不是全局最优，跟 Python 版同一个 MVP 取舍）。

**真机发现：这个兜底一开始只接进了"没有 pending"那条分支**——但双拼下差不多一半按键都处于 pending 状态（每 2 键才凑成一个完整音节，奇数键数必然 pending），我实测反馈"这是一句话"打到第 9 个键（进入第 5 个音节、pending 态）就从"这是一句"这样的真候选又退化回字母显示。**修复**：pending 分支查不到东西时，也对"已经打完的那部分音节"跑一次贪心拼句，展示结果后面接上正在打的这半个音节的原始字母（还没定型，继续显示字母是合理的）。

> 源码：`pinyin-engine/src/engine.py:52-75`（`_compose` 原始 Python 实现）；C 移植 `xovi-extensions/cangjie-langhook/src/hook_init.c:2159-2216`（`cj_step_v_compose`）、`:2321-2410`（`cj_step_v_generate_dict_candidates`，pending 分支贪心拼句在其内）

### ✅ Step DD：双拼单敲声母键，翻译成真实声母再查词典——跟简拼"敲声母出常用字"体验对齐

我实测反馈：双拼下单独敲一个声母键（比如小鹤方案里 `u` 代表整体认读声母 `sh`，还没按韵母键）应该像全拼下敲 "sh" 一样直接弹出 sh 声母的常用字，而不是把这个双拼按键字母原样当成拼音前缀去查词典（词典里没有以字母 `u` 开头的合法拼音音节，查了也是白查）。**修复**：双拼模式下，pending 只有 1 个字节（刚打完声母键）时，先把这个字节翻译回它代表的声母字符串（小鹤方案只有 `v`→`zh`、`i`→`ch`、`u`→`sh` 三个压缩声母键需要翻译，其它声母键本来就是字面值原样透传），再喂给已经在用的 `cj_dict_lookup_prefix`——不需要新查询机制，复用的是全拼路径本来就有的"敲 sh 就弹出 sh 声母常用字"前缀匹配能力。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2224-2231`（`cj_shuangpin_initial_key_to_pinyin_initial`）

### ✅ 候选条最终 UI 打磨——参照 iOS 拼音候选栏的实际交互（我提供真机截图对照），加预览行、展开/收起、e-ink 对比度调整

**展示方式演进**：最初照搬 `KeyPopup` 分页+">>"点击翻页的交互，我反馈"候选太少，学一下 iOS 键盘的展示方式"——iOS 参考截图显示的是单行紧凑排列尽量多显示候选、右上角一个"▾"展开按钮，不是固定几个+点按钮翻页。尝试过横向 `ListView`（划着看更多）——横向拖动手势在这块 e-ink 触屏上的实际支持情况不确定，我反馈体验依然像"点击翻页"，不是顺滑拖动。最终改成参照 iOS 截图的方案：`Flow` 自动换行 + 固定收起高度只留一行（`Flow` 自己设置 `height`+`clip:true`，不能只在外层 `Rectangle` 裁切——那样裁切边界会切在下一行中间，露出半行内容）+ 右上角"▾"/"▴"展开按钮切换收起/展开两种高度，候选内容一变（每次按键）自动收起展开态，避免陈旧的展开状态挂在那不合逻辑。

**预览行（Step EE）**：我要求候选条下方能看到自己实际打的拼音字母，**双拼要显示自动转换后的标准拼音，不是双拼按键原文**（比如双拼敲 `ulpb` 应该显示转换出来的拼音，不是 "ulpb" 字面值）。新增 `previewText` 属性（单个 `QString`，跟 `keys` 那种 `QStringList` 要用不同的 `QVariant` 构造函数 `QVariant::QVariant(const QString&)`），C 端缓存每次刷新用到的 `seg_src`（全拼模式下就是原样字母，双拼模式下已经是转换结果，两种情况统一处理，不需要再判断一次是不是双拼）。

**e-ink 对比度教训（这个项目里反复踩到的同一类坑）**：预览行文字最初用小字号+浅灰色（`#666666`），我反馈"又淡又小"——这是本项目第三次在 e-ink 对比度上判断失误（此前红色候选条、白色候选条都出现过"颜色选错、几乎看不见"的问题）。调大字号、颜色直接改成纯黑，不再尝试任何"柔和一点"的灰阶——e-ink 屏幕上，非黑即白的高对比度选择比看起来更精致的中间色调更可靠，这条经验值得写进后续任何新增 UI 元素的默认选择里。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c`（`g_step_v_preview_text`:898；缓存逻辑在 `cj_step_t_refresh_candidates`:2738-2875 内，写入在 `cj_step_t_update_popup`:1860-1927 内）；QMLDiff 候选条定义见设备 `/home/root/xovi/exthome/qt-resource-rebuilder/candidatebar.qmd`

### ✅ Phase A：逐字候选 + 分段提交 + 退格撤销——把"能选到词"补成真正 iOS 式"逐字可选、中途可改"的完整输入法

（我对照 iOS 截图提出，三种模式全部真机验证通过。）

**暴露的缺口**：此前多音节输入（如 `shenme`）候选栏只给整段词 `什么` + 贪心拼句兜底，**不出首音节单字**（神/什/深/身…），想先选"神"根本选不到；而且点候选/空格/回车一律清空整个缓冲区，没有"提交一部分、保留剩下继续选"。我对照 iOS（打 shenme 同时给出"什么"和"神/什/深…"、点第一个字后剩下的音节继续选）指出这是隐藏的范围缺口——当初 M4 验收标准"点选后正确插入汉字"没覆盖"逐字造句"。分三步补齐：

- **A-i 前缀候选生成**：新增 `cj_add_prefix_candidates`——对切分出的音节序列 s1…sk，除了整段（j=k）精确匹配，再对递减前缀 j=k-1…1 各查一次词典，j=1 即首音节单字。配额刻意保底：整段占一半、首音节单字填满剩下、中间前缀少量，防止"整段词一多就把单字挤出候选栏"（06 节真机+差分测试验证过"候选一多会挤掉尾部结果"，这里反过来给单字保底）。新增平行数组 `g_step_v_cached_consumed` 记录每个候选点选后要从缓冲区裁掉多少个原始按键字节——全拼直接用音节 offset/len，双拼每个完整音节固定 2 键（consumed=2j）。同时移除了早前 Step FF 注入的原始字母英文候选，候选栏改成 iOS 干净版（只展示词候选），输入内容只留在预览行。
- **A-ii 分段提交**：候选点选不再传候选文字本身，改传 `"\x02"+index`（`candidatebar.qmd` delegate 的 `index`，跟 `">>"`/`"\x01"` 是同类控制标记）；C 端 `cj_vk_insert_text_handler` 解码索引 → `cj_step_t_commit_candidate_by_index()`：提交该候选文字 → `memmove` 从缓冲区裁掉它消耗的前缀 → 对剩余重新切分刷新候选。空格走同一路径提交第一名。真机验证：`shenme` 点"什"（消耗 4/6 字节）→ 上屏"什"、候选栏变成 `me` 的候选 → 点"么"（消耗 2/2）→ 得"什么"。
- **A-iii 退格撤销 / 中途改字**：解决"逐字造句时中途一个字选错怎么回退"。退格从右往左剥整个 composition——新增"已提交撤销栈"（每次提交压入：消耗掉的原始拼音字节 + 该词在文本框占的 UTF-16 码元数，用 `cj_utf8_to_utf16_units` 从 UTF-8 数，非 BMP 扩展区字算 2 个码元）：① 缓冲区有拼音 → 删最后一个字母（原有）；② 缓冲区空但栈非空 → `insertText("", -units, units)` 从文本框删掉上一个已选词（跟原生长按删键 `insertText("", -1, 1)` 同机制）、把它的拼音还回缓冲区、重新出候选可重选；③ 都空 → 原生退格。composition 结束（空格/回车确认、外部插入、闲置超时、缓冲区空时敲原生空格/回车）时清空撤销栈，之后退格转原生，堵住"空格后再退格却去撤字"的错位。真机验证：打"什么"退格→删"么"、候选回到 me；再退到底→删"什"、候选回到 shen；打"你好"退格→**一次删掉两个字**（`insertText("", -2, 2)`，多码元删除参数正确）；双拼打 `ulpb` 选"双""拼"后退格分别还原 `pb`/`ul`（2 键/音节还原正确）。全程无崩溃、`MainPID` 稳定。

**已知限制（如实记录）**：撤销用 `insertText("", -units, units)` 删的是**光标前**的字——composition 进行中若我手动把光标点到别处，撤销会删错位置；真实交互里很少这么做，且焦点变化会触发外部插入的 finalize，我 hook 不到纯光标移动事件，这一版接受这个边界。中英混输的"智能组合候选"（iOS 的 `你好hello`：拼音部分转汉字+英文部分保留）已由 07 节 Phase C 完成（词典驱动的前缀补全，增量式：分两次输入 `nihao`→选→`hello`→选）；仅"连打 `nihaohello` 一次成句"的联合切分仍后置。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c`——`cj_add_prefix_candidates`:2273-2319/`cj_consumed_bytes_for_prefix`:2237-2249（A-i）、`cj_step_t_commit_candidate_by_index`:2983-3044 与 `cj_vk_insert_text_handler`:937-1037 里的 `\x02` 索引解码（约 959-997 行）（A-ii）、`g_compose_stack`:804/`cj_vk_trigger_backspace_handler`:1063-1116 撤销分支/`cj_compose_stack_reset`:808 各 finalize 点（A-iii）；`candidatebar.qmd` delegate 传 `index`

### ✅ Phase B：4 输入法（简体全拼 / 繁體全拼 / 简体双拼 / 繁體双拼）+ 地球键切换——复用原生语言弹层，双拼顺带做成免重启实时切换

（真机 a–g 逐项验证通过。）

**目标与决策**：我要一套完整 iOS 式输入法，需要 4 个可切模式 = {简体, 繁体} × {全拼, 双拼}。缺口有二——简/繁本就能切（Step R+S+U 语言弹层，真机验证过），但全拼/双拼当时是标记文件开关、改了要重启、键盘上没入口。切换机制取**"扩展原生语言弹层到 4 项"**而非 hook 地球键单击循环：反编译+QML 证据确认地球键原生行为就是弹出 `KeyboardLanguageSelect` 语言列表（没有单击循环的原生逻辑），把 4 个中文项放进这个列表、像切西班牙语一样点选，是**复用已验证机制、零新增 hook、零反编译侦查**的最低风险路线；双拼状态编码进 `language` 属性后**顺带变成免重启实时切换**。代价（我知悉接受）：交互是"点地球→弹层→点选"两步，不是单击循环。

**核心设计：4 个伪语言代码 → 统一模式，`language` 属性是唯一真源**。`zh_CN`=简体全拼、`zh_TW`=繁體全拼、`zh_CN_SP`=简体双拼、`zh_TW_SP`=繁體双拼，追加进 `virtualKeyboard.languages`；我点选后代码被写进原生 `language` 属性，我读回解析出 (简/繁 base, 双拼位) 两个轴。伪代码安全性：写进 `language` 后只有我消费；原生 `languageName()` 内部 `QLocale(code)` 对未知代码容错、且结果被我的显示名覆盖；布局加载被 Step U 强制换 `en_US`。`zh_CN`/`zh_TW` 本就是原生不支持的代码、Step S 阶段已在弹层正常工作，再加两个同性质代码无新风险。

**落地：一张真源表驱动四处，只改 `hook_init.c`**（不动 QML/`candidatebar.qmd`/拼音引擎）。把此前分散在 Step R/S/U 三处的 zh_CN/zh_TW 列表合并成单一 `CJ_VK_MODES[4]` 表（`{code, 显示名, 简/繁 base, 双拼位}`），避免多份列表日后漂移，同时驱动：① Step S 往 `languages` 追加 4 个代码；② Step R 显示名覆盖（4 代码 → "简体全拼/繁體全拼/简体双拼/繁體双拼"，去地区括注）；③ Step U 布局兜底把 4 个代码都强制换 `en_US`，保证键盘网格正常渲染；④ 模式解析 `cj_step_t_language_mode_ex(this_ptr, &shuangpin)` 读一次 `language` 属性同时得出简/繁与双拼两个轴，`_SP` 变体映射到其简/繁 base。**双拼免重启随之自动达成**：`cj_step_t_refresh_candidates` 里的 `shuangpin_active` 从"标记文件+一次性缓存"改为每次按键读 `language` 属性得出，切模式立即生效；原 `cj_shuangpin_is_enabled()`/标记文件机制整段删除。双拼解码（Step Y `cj_shuangpin_keys_to_pinyin`）下游逻辑一字未改，只换了 `shuangpin_active` 的来源。

**真机验证（a–g 全部通过）**：点地球弹层出现 4 个中文项、显示名正确 → 选"简体全拼"打 `nihao` 出简体候选 → "繁體全拼"出繁体候选 → 选"简体双拼"**不重启**即双拼解码生效 → "繁體双拼"双拼+繁体 → 4 模式反复来回切每次立即生效、无残留双拼状态 → 切回 English 完全恢复原生输入。部署健康：交叉编译零警告、本地/设备 md5 一致、重启后 `MainPID` 变化、`is-active`=active、`NRestarts` 不增、无崩溃。

**已知取舍 / 未做**：每次按键读一次 `language` 属性的小引用计数"泄漏"沿用既有取舍（量级不变，不手写引用计数）；显示名"全拼/双拼"是可调展示细节；**不做**地球键单击循环（本轮走弹层路线，需要单击循环得另开一轮反编译侦查地球键 `KeyType` 值/触发路径）。（Phase C 中英混输已在 07 节完成，非本节范围。）

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c`——真源表 `CJ_VK_MODES`+`CjLanguageMode` 枚举:411-436、Step R 显示名覆盖 `cj_vk_override_language_name`:438、Step S 追加 `cj_vk_dispatch_handler`:570、Step U 布局兜底 `cj_vk_loadlayout_handler`:667、模式解析 `cj_step_t_language_mode_ex`:2892、接入点 `cj_step_t_refresh_candidates`:2738-2875；标记文件机制已删除

## 03｜拼音引擎核心设计

- **音节切分**：把连续的拉丁字母输入切成一个个合法拼音音节（声母表 + 韵母表 + 合法组合规则），比如 `xianggang` 要能切成 `xiang + gang` 而不是 `xi + ang + gang` 之类的错误组合——这是拼音引擎最基础但也最容易出 bug 的一步，建议先写清楚的单元测试覆盖常见边界情况（连续声母、隔音符号 `ang'gang` 之类）。
- **候选生成**：音节序列 → 候选词，起步用简单的词频表（unigram）排序就够用，MVP 阶段不用一上来就做复杂的语言模型。
- **候选排序进阶**（M6 之后的可选优化）：二元词频（bigram，结合上一个已上屏的词调整排序）、模糊音（如 `zh/z`、`an/ang` 不分的容错）。

### ✅ 已实测跑通（本地目录 `pinyin-engine/`，代码 + 25 个单元测试）

音节表没有手写——声韵母拼合规则例外太多，手写错的代价是切分全错，改成直接从 06 节的 RIME 词库反推"实际出现过的全部音节"（429 个），权威且和候选词典同源。切分算法：隔音符号 `'` 是硬边界（`xi'an` 强制切 `xi+an`，不带符号的 `xian` 按正词法惯例整体算一个音节）；没有歧义时贪心最长匹配（`xianggang` → `xiang+gang`，本节点名的例子实测验证通过）；"最后一个音节还没打完"单独处理成 pending 前缀（打到 `zho` 时识别成"确定前缀为空 + pending='zho'"交给前缀匹配，不报错）。候选生成实测 `nihao → 你好`、`zhongwenshurufa → 中文输入法`、`woshizhongguoren → 我是中国人` 都能正确产出，细节和踩过的坑（比词典准备阶段还多，见 06 节）在 `pinyin-engine/README.md`。

### ⚠️ M3/M4 真正动手前发现的架构空白：这套引擎现在只在开发机上跑，没接进 xochitl 进程，接入方式还没定

`pinyin-engine/` 是纯 Python（`segment.py`/`dictionary.py`/`engine.py`），依赖 `data/` 下的词典文件，02 节的 hook 是 C 写的、编译成 ARM64 `.so` 用 `LD_PRELOAD` 挂进 xochitl——Python 代码没法直接被 C 调用，两条线目前是断开的。候选方案：① 引擎作为设备上的**独立进程**跑，hook 用本地 socket/管道发按键、收候选（前提是 reMarkable 系统自带 Python3，这一步需要设备 SSH 确认，本轮设备连不上，待补）；② 把 `segment.py`/`dictionary.py` 核心逻辑**移植成 C**，静态编译进 hook 的 `.so`（工作量大得多——词典加载、音节切分、候选排序全部要重写+重新写测试，但没有 IPC 延迟和"设备装不装 Python"这个不确定性）。

**一个不能忽略的许可证约束**：`pinyin-engine/README.md` 自己写明数据来源里 `rime-double-pinyin`（双拼 schema，只有双拼功能用到）是 **GPL v3**，核心的 `luna_pinyin`/`essay-zh-hans` 是 LGPL v3——README 原话提醒"GPL v3 的 copyleft 会扩散到链接它的整个程序"。如果双拼相关代码/数据被静态编译进 `LD_PRELOAD` 到 xochitl 进程里的同一个 `.so`，算不算"链接"、要不要担心 copyleft 扩散到整个 hook 程序，是真实的法律边界问题，不该我一个人下结论。规避选项：MVP 先只做全拼（完全不碰 `rime-double-pinyin` 数据），把这个问题彻底往后推；或者把双拼部分隔离成独立进程用 IPC 通信，不跟 hook 的 `.so` 编译在一起。

候选栏 UI 这一半的接入方式找到了一个有希望的现成机制（复用长按弹出的 `KeyPopup` 组件，不用注入全新 QML），详见 08 节。

### ✅ 引擎执行环境已经查清楚——设备没有 Python3，排除 IPC 路线，确定走 C 移植

```
$ which python3 python            // 无输出
$ python3 --version               // -sh: line 1: python3: command not found
$ ls /usr/bin/python* /usr/local/bin/python*   // 无输出
$ df -h /                         // 452.2M 总量，339.8M 已用，只剩 83.7M 可用（80%）
$ free -h                         // 2GB 总内存，1.5GB 空闲
```

真机 SSH 实测：**标准路径下没有任何 Python 解释器**，根分区只剩 **83.7MB** 可用空间——内存本身够用（2GB，1.5GB 空闲），但磁盘这一项就算想自己塞一个精简解释器上去也很紧张（Python3 解释器+标准库至少几十 MB 起步，词典数据 `essay-zh-hans.txt` 等还要再占一块）。**"独立进程 + IPC"这条路在当前设备状态下不现实，排除。**

**结论：拼音引擎执行环境走 C 移植**——`segment.py`/`dictionary.py`/`engine.py` 核心逻辑需要重写成 C，静态编译进 hook 的 `.so`。附带好处：GPL v3 许可证的顾虑也基本消解——既然决定要重写成 C，`rime-double-pinyin`（双拼 schema，GPL v3）这份 Python 代码本身不会被直接编译/链接进去，双拼功能如果要做，是"参照 GPL v3 schema 文件里的转换规则重新实现"而不是"链接 GPL 代码"，法律边界比直接编译含 GPL 代码的 `.so` 清晰得多（仍建议真正做双拼那一步时单独确认一次，这里不代替法律意见）。

这个结论意味着实际工作量比最初设想大不少——不是"接一层拼音缓冲逻辑"这么简单，而是要把整个拼音引擎核心（音节切分+候选生成+排序）用 C 重写一遍。建议的验证顺序：先把音节切分这一层（`segment.py`，逻辑相对独立、不依赖大词典数据）单独移植成 C 作为试点，验证"Python 逻辑移植成 C 静态编译进 hook"这条路整体走不走得通，再决定要不要继续搬候选词典这个更大的部分。

### ✅ 试点已完成——`segment.py`/`syllables.py` 移植成 C，跟 Python 原版差分测试 40/41 逐字节完全一致

新增 `pinyin-engine/c/`（跟 Python 版平级的独立目录）。音节表数据不在 C 里重新解析 YAML——用一个代码生成脚本（`c/tools/gen_syllables_header.py`）从 Python 端 `SYLLABLES` 集合导出成静态 C 数组（429 个音节），真相来源仍然是同一份 `luna_pinyin.dict.yaml`。切分算法的核心改动：Python 版靠 `lru_cache` 天然避免重复计算和指数爆炸，C 版没做等价的 memoization（内存代价太大——要缓存到 32 个候选的完整枚举结果，一张 64×64 的表估算要 64MB，不现实），改用两道更保守的硬上限：① 求"最少音节数"的最优切分用 DP 直接算（`O(len × 6)`，不需要先枚举全部候选，不会指数爆炸）；② 只有需要暴露多个候选给上层排序的最后一段，才做有界的递归全枚举，加了候选数量上限（8 个）和递归工作量预算（4000 次）两道防线，专门堵住"长按同一个字母"这类病态输入可能触发的指数级枚举——比如连续打 22 个字母不停顿，理论上可能出现上百种歧义切法。

**验证（比原计划更彻底，多加了一层差分测试）**：① 单元测试——`test_syllables.c`/`test_segment.c` 逐条对照 Python 版 `test_syllables.py`/`test_segment.py` 的用例，全部通过；② **差分测试**（新工具 `diff_check.py` + `diff_cli.c`）——41 个测试字符串（单音节/常见词/歧义/隔音符号/pending/大小写/超长重复/非法输入）分别跑 Python 原版和 C 版，逐行比较输出，**40/41 逐字节完全一致**，唯一的差异是一个刻意构造的极端用例（`jintiantianqizenmeyang`，22 字母一次性打完不停顿，Python 枚举出 121 种歧义切法）触发了候选数量上限被截断到 8 个——但排第一的推荐结果两边一致，C 版返回的 8 个候选也全部是 Python 121 个里真实存在的项（不是算错，是截断），符合设计预期，不是 bug；③ ARM64 交叉编译（跟 `cangjie-langhook` 同一个交叉编译器）零警告通过，确认这批代码能在目标架构上编译。

**结论：Python → C 移植这条路径本身是可行的**，算法能正确复刻、交叉编译没有障碍，验证方法（单测+差分测试+交叉编译）本身也可以直接复用到候选词典那个更大的移植任务上。这批 C 代码后来（02 节 Step T）已经接进 `xovi-extensions/cangjie-langhook` 并真机验证通过。**候选词典**（`dictionary.py`）后来也移植完了 C 查询层（离线数据管线+mmap 查询，验证方法跟这里完全一致），细节和数据源变更见 06 节——02 节 Step V 记录了它真正接入设备的真机验证过程。

## 04｜双拼——复用同一个引擎，只加一层键位转换

双拼的本质是"用固定的单键组合代表一个完整拼音音节"，**不需要单独的引擎**：在我按键输入和拼音引擎之间加一个"双拼键位表"预处理层，把双拼按键序列先转换成完整拼音字符串，再喂给 03 节同一套引擎。工作量集中在维护多套键位映射表（自然码、小鹤双拼等常见方案）和一个可以切换方案的设置项，而不是重新做引擎。

### ✅ 已实测跑通（`pinyin-engine/src/shuangpin.py`），顺带纠正了一处旧例子的错误

此前这里举的例子"小鹤双拼里 `vg` 代表 `ang`"是**错的**——实测 `vg` 解码出来是 `zheng`（v=声母 zh，g=韵母 eng），"昂"（零声母 ang）实际按键是 `ah`。双拼按键映射不是凭记忆手写的表格：同一个按键在不同上下文代表的韵母会变（比如 `k` 键在 zh/ch/sh/r/z/c/s 这些整体认读声母后面代表 `uai`，其它声母后面代表 `ing`），手写错一条的代价是那一批音节全部解码错。改成直接复用 RIME 官方 `rime-double-pinyin` 仓库的 schema 文件（`double_pinyin_flypy.schema.yaml` = 小鹤双拼，`double_pinyin_natural.schema.yaml` = 自然码）——这些 schema 里 `translator.preedit_format` 那一段正好就是"双拼按键 → 完整拼音"的正则替换规则链（RIME 自己的 algebra DSL），照搬这条规则链跑一遍，不是重新发明。协议注意：`rime-double-pinyin` 是 **GPL v3**（不是 luna_pinyin/essay 那两个词库用的 LGPL v3，实测下载的 LICENSE 文件确认了这个区别，10 节已同步）。验证方式：单元测试里直接对拍——同一个词分别走全拼 `query("zhongguo")` 和双拼 `query_shuangpin("vsgo", scheme="flypy")`，候选列表完全一致，证明双拼层只是纯粹的预处理转换，没有绕开 03 节那套引擎另起一套逻辑。

### 📌 范围收窄（本轮明确）

正式方案只提供**小鹤双拼（flypy）**一种双拼方案，不做自然码或其它 schema 的可选项——`shuangpin.py`/后续 C 移植版本仍然保留"按 schema 名字加载"的通用能力（代码结构不因此变复杂，删掉现成能力反而要额外处理"只剩一种方案还要不要留切换开关"这种没意义的分支），但产品/UI 层面只暴露"开启双拼（小鹤）"一个开关，不做方案选择菜单。09 节 M6 验收标准已同步：原"双拼键位表可切换使用"改为"双拼固定为小鹤双拼，整体开关"。

## 05｜繁体输入——同样复用简体拼音引擎

不建议为繁体单独做注音或仓颉引擎（工作量是完全独立的一个项目）。推荐做法：我仍然用拼音打字、看到的候选字是简体，选中后如果当前系统语言是繁体模式，**用 OpenCC 对最终选中的候选词做一次转换再插入文本框**——复用姊妹文档 3.2 节提到的同一个 OpenCC 依赖。如果未来有精力覆盖习惯注音/仓颉的输入方式，那是一个独立的扩展方向，不在本方案 MVP 范围内。

### ✅ 已实测跑通（`pinyin-engine/src/traditional.py` + `engine.commit()`）

候选栏本身（`query()` 的返回值）保持简体不变，只有 `commit(word, region)` 这个转换点才生效——单元测试验证过候选栏候选里"网络"还是简体，`region=None` 不转换，`region="tw"` 转出"網路"，`region="hk"` 转出"網絡"，和姊妹文档 3.2 节实测过的词汇级转换差异（s2twp vs s2hk）完全对得上，说明这一层接的确实是同一套配置，没有退化成容易踩坑的纯字形转换。06 节发现的"词库本身偏繁体、候选生成要做简繁归一"是另一个更底层的坑，这里的转换层解决的是"最终呈现给我"这一步，两者不是一回事，不要混为一谈。

### 📌 设计修订（已实现，见下方）——正式设备端方案改成候选生成阶段整体转换，不是选中后再转

上面这套"候选栏保持简体、只在 `commit()` 转换"是 Python 原型阶段的最小实现，能跑通、有单测，但不满足这次"功能与简体一致"的要求——体验上是"选的时候看着简体，插入后突然变成繁体"，跟雾凇拼音这类参照对象的真实体验（繁体 schema 下候选栏本身就显示繁体字）不一致。正式方案改成：**在候选生成阶段就整体转换，候选栏显示的直接是目标字形**，选中即所见即所得。

**实现方式——offline 双 blob，不在设备端跑 OpenCC**：不打算把 OpenCC（C++ 库）移植/静态链接进 `cangjie-langhook.so`——那是一个新的重量级 C++ 依赖，会显著扩大注入进程里的攻击面/复杂度，跟本项目一贯"能离线做完就不留到运行时"的取舍不符。改成在生成 `dict.bin`（06 节）的同一个离线管线里，**额外生成一份 `dict.zh_tw.bin`**：对词典每个候选词条，用现有 `pinyin-engine/src/traditional.py` 的 OpenCC 词汇级转换（`s2twp`，跟姊妹文档 3.2 节验证过的配置一致）离线转换成繁体形式，转换后按 `(音节, 转换后的词)` 做跨词条去重合并权重（同一简体词转换后可能撞成同一个繁体词，也可能不同简体词转换后撞在一起，都要在生成阶段解决，不能留到运行时——这是 06 节 41448 大字表合并时已经用过的同一套去重手法，不是新技巧）。设备端 C 引擎完全不变，只是根据 `virtualKeyboard.language`（已经在用的判断信号）决定 `mmap` 哪份文件——语言是 `zh_CN` 就开 `dict.bin`，`zh_TW` 就开 `dict.zh_tw.bin`，查询/分页/`KeyPopup` 展示逻辑一行都不用改。

**为什么这样能做到"功能与简体一致"**：分页、翻页、`KeyPopup` 复用、07 节的简拼/中英混输设计，全都建立在"C 引擎查一份 mmap 好的候选池"这个统一接口上，跟候选池里装的是简体字还是繁体字无关——两份 blob 只是内容不同、结构完全一样，天然保证所有已验证/设计过的机制在繁体模式下逐条对等生效，不需要为繁体模式单独维护一套逻辑分支。唯一的额外成本是生成阶段多跑一次转换+去重，运行时零额外开销、零新增依赖。

**许可证**：OpenCC（BYVoid/OpenCC）是 **Apache License 2.0**（比 rime-ice 的 GPL v3 宽松很多），且这里只是开发机上生成数据用的离线工具依赖，不进入设备端产物，风险类别比 06 节词典数据本身更低，10 节风险清单需要补一条区分记录。

09 节 M6 验收标准已同步：原"繁体模式下候选正确转换为繁体"改为"繁体模式下候选栏本身显示繁体字（生成阶段转换），跟简体模式功能完全对等"。

### ✅ 离线部分已实现——`dict.zh_tw.bin` 生成完成，C 端零改动打开验证通过

`pinyin-engine/src/traditional.py` 新增 `build_traditional_table()`：把一份候选表整体跑一遍 OpenCC `s2twp`，同一个音节 key 下如果多个候选转换后撞成同一个词（真实碰撞用例，不是构造的：雾凇拼音词库里"谙"和词库自带的繁体异体"諳"转换后都是"諳"），权重相加、重新按权重降序排序。生成工具 `gen_dict_zh_tw_blob.py` 用它产出 `dict.zh_tw.bin`（457,368 个 key、586,048 条候选，25.5MB，跟简体版 589,221 条相比因合并少了约 3,173 条）——打包逻辑抽成 `blob_format.py`，跟 `gen_dict_blob.py`（简体版）共用，避免维护两份几乎一样的偏移量计算代码，抽取前后用 sha256 核对过简体版输出逐字节不变，不是"重构顺便改了行为"。

**验证了这个设计最核心的主张——C 引擎真的不用改**：`c/src/dictionary.c` 一行代码没动，新增的 `test_dictionary_zh_tw.c` 直接用现有的 `cj_dict_open()` 打开 `dict.zh_tw.bin`，查 `zhong guo` 拿到"中國"（不是"中国"）、查 `wang luo` 拿到"網路"、查 `an` 验证"諳"合并权重正确（6174，两条候选之和，不是丢了一条），3 个 C 测试全过。Python 侧新增 3 个单测（含上面"谙/諳"这个真实碰撞用例、一个用假转换器精确控制"合并后反超原排名第一"边界场景的测试）。全部 71 个 Python 单测 + 全部 C 单测 + 差分测试 + ARM64 交叉编译，一次跑完全部通过。

**还没做的（这一版）**：没有生成配套的 `dict_jianpin.zh_tw.bin`（简拼索引本身还没接入设备，繁体简拼要等两边都上真机才有意义，不提前做）；没有生成 `dict.zh_hk.bin`（设备语言切换器目前只有简体/繁体两项，没有单独的香港繁体选项，`region` 参数已经支持 `"hk"`，以后要加不需要改代码）。接入设备已经完成，见下方。

> 源码：`pinyin-engine/src/traditional.py`（`build_traditional_table`）、`pinyin-engine/c/tools/blob_format.py`、`gen_dict_zh_tw_blob.py`、`pinyin-engine/c/tests/test_dictionary_zh_tw.c`、`pinyin-engine/README.md` 第 8 条

### ✅ 已接入 `xovi-extensions/cangjie-langhook`，真机验证通过

`dict.zh_tw.bin` 部署到 `/home/root/`（跟 `dict.bin` 同一目录）；新增 `cj_dict_zh_tw_ensure_open()`，跟已有 `cj_dict_ensure_open()` 同一个"惰性打开、成功/失败都不重试"模式，只是换一个路径、换一个静态 handle。**该查哪份词典**由新增的 `cj_dict_select_handle_for_mode(mode)` 决定（07 节接入简拼时改成接收调用方已经算好的 `mode`，不再自己读一次 `this_ptr`，见下方说明）：是 `zh_TW` 且繁体版打开成功就返回繁体 handle，其它情况一律退化用简体 handle——跟 `cj_dict_ensure_open()` 一样的 fail-safe 原则，两份词典都没打开成功时上层 `cj_step_v_generate_dict_candidates()` 已有"返回 0 个候选、退化成拼音切分展示"的处理，不是新失败模式。`cj_step_t_is_chinese_mode()` 顺手重构成从共用的 `cj_step_t_language_mode()`（新枚举 `CjLanguageMode`：`CJ_LANG_OTHER`/`CJ_LANG_ZH_CN`/`CJ_LANG_ZH_TW`）派生，避免"是否中文模式"和"该查哪份词典"分别各读一次 `language` 属性，把 Step T 已经记录、接受的"有界小引用计数泄漏"的量级维持不变，不因为这次改动翻倍。`cj_step_v_generate_dict_candidates()`/`cj_step_t_refresh_candidates()` 都改成接收/传递选好的 `CjDictHandle*`，不再内部硬编码简体 handle。

**真机验收（我操作确认）**：切"简体中文"打字，候选栏正常显示简体候选（回归确认没改坏原有功能）；切"繁體中文"打字，候选栏显示的是繁体字（"中國"而不是"中国"）；切回简体能正常切回，不会卡在繁体状态。`xochitl` 全程健康，部署前后 `MainPID` 正常更新、`NRestarts=0`，ARM64 交叉编译零警告。

**验证过程中额外发现并修复了一个真机专属 bug（真机 bug 4）**：回车键被 Step V 误合并成"确认候选"，跟空格没了区别——详见 02 节对应小节（不是这次繁体接入引入的问题，是 Step V 阶段遗留、这次真机测试才暴露出来，顺手修的，不属于繁体双 blob 设计本身）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2026-2040`（`cj_dict_zh_tw_ensure_open`）、`:2051-2059`（`cj_dict_select_handle_for_mode`）、`:2892-2929`（`cj_step_t_language_mode`/`cj_step_t_is_chinese_mode`）

## 06｜候选词库

推荐用 **RIME 输入法框架**生态的开源词库，社区维护多年、质量有保障，明确在我的项目里保留来源和协议声明。不建议使用来源不明的"某某输入法词库导出"，版权风险不可控。最初选的是 `rime/rime-luna-pinyin` 仓库的 `luna_pinyin.dict.yaml`（LGPL v3，实测下载 LICENSE 文件确认），移植 C 查询层这一轮换成了[雾凇拼音（iDvel/rime-ice）](https://github.com/iDvel/rime-ice)，换源理由和细节见下方。

### ⚠️ 真的接上这份词库做候选生成后，踩到两个不看代码、光读文档想不到的坑

① **"你好"这种最基础的词，词库里竟然没有**——`luna_pinyin.dict.yaml` 本身只收单字全集 + 少量手工维护的固定词组，frontmatter 里 `use_preset_vocabulary: true` 才是关键：常用词组要靠另一份"preset vocabulary"（只有"词+出现次数"、不带拼音的频率表）跟单字拼音实时组合拼出来。对应做法是另外拉取 `rime/rime-essay-simp` 仓库的 `essay-zh-hans.txt`（同样是 LGPL v3），逐字取该字权重最高读音拼出拼音 key，词库没有的词组从这里补，"你好""中文输入法""我是中国人"这类才补得出来。

② **词库里的百分比权重列，语义比想象中窄很多**——那一列（比如"的 de 99.97% / di 0.03%"）是"同一个字的多个读音里选哪个"，只在字符内部有意义，**不能跨字符比较**！实测踩到的反例：单独查 `guo` 这个音节，"掴"字（guai/guo 两个读音里 guo 占 97.33%，但这只是它自己内部的读音占比）如果直接拿这列排序会排到真正常用的"国"（因为国只有一个读音，没有百分比可标）前面——生僻字排到常用字前面，候选体验直接不可用。

连带发现的第三个问题：这份词库的字/词源头（萌典等）本身以繁体字形为主，同一个字常常繁简变体都收（"麼"权重远高于"么"），照抄权重列排序会拼出"今天天气怎麼樣"这种简繁混排的错误结果。三个坑的修正方法是同一个：跨字符/跨词的排序统一换成 essay 语料的真实使用频次，词库百分比权重只留在"同一个字选主读音"这层内部用——这也把简繁混排问题"顺带"解决了（因为 essay 语料本身是简体语料），但这不是系统性根治，05 节设计的 OpenCC 转换层实际上需要覆盖到候选生成这一层，不能只当"翻译 UI 字符串"和"上屏后再转繁体"两处用，这是本次动手实现才补出来的认知，此前的方案设计遗漏了这点。三个坑各自的实测复现和回归测试见 `pinyin-engine/README.md` 和 `tests/test_dictionary.py`。

### ✅ 换源雾凇拼音 + C 查询层落地——离线数据管线/格式设计/差分测试全部完成，已真机接入验证（详见 02 节 Step V）

**为什么换源**：我提出雾凇拼音（`iDvel/rime-ice`）持续维护、本机桌面输入法同款，以后有机会做词库同步。核实后确认换源本身合理，且格式比 `luna_pinyin` 更干净——主词库 `cn_dicts/base.dict.yaml`（钉 commit `569ff3bc`）三列 tab 分隔（词/拼音/整数词频），词频已经是跨字符可比的排序权重，**不再需要** `essay-zh-hans.txt` 语料频次表兜底、也不再有"百分比权重只在同字内部有意义"那个坑（上方记录的三个坑，实测扫过全部 55 万+行数据，第三列没有出现过百分比格式）。**动手后才发现的数据缺口**：`base.dict.yaml` 只收词/短语，完全不含单字（"国"/"的"这类最基础的单字一条都查不到，`engine.py` 逐字拼句兜底逻辑也依赖单字候选）——雾凇拼音把单字表拆到独立的 `8105.dict.yaml`（8105 常用字表，115KB），默认改成两个文件合并加载。规模：45.7 万 key、55.2 万条候选（luna_pinyin+essay 组合约 29 万条候选，新数据源接近翻倍）。**许可证**：**GPL v3**（不是 luna_pinyin/essay 的 LGPL v3，实测下载 LICENSE 文件确认），词典数据本身就是交付内容、没有"重写算法绕开链接"这个选项（03 节双拼那次的规避思路在这里不适用），因此**存储架构改成独立二进制文件、运行时只读 mmap，不编译进 `cangjie-langhook.so` 本体**——法律定性更接近"程序读取一份数据文件"而非"GPL 代码编译链接进同一个二进制"（不代替法律意见，只记录架构选择本身降低了风险类别）。

**二进制格式设计**：不是指针式 trie，是**排序字符串数组 + 二分查找**——README 记录的已知瓶颈是 `lookup_prefix` 的线性扫描，不是"数据结构不够聪明"，二分查找就能把它从 O(n) 降到 O(log n)；trie 需要在反序列化时重建指针，是过去真机崩溃教训明确要避免的"不确定内存布局细节时手写内存操作"，排序数组可以整体 mmap、查询时只做只读偏移量运算，不需要在加载时重建任何指针。文件布局：header + key 表（音节序列排序）+ 候选表（按 key 表顺序连续排列，组内按权重降序）+ 两块紧凑的原始字节区。前缀匹配（"打到一半"的场景）在排序数组上是天然连续的区间，二分查找区间边界即可，不需要真正的 trie；**唯一需要注意的陷阱**：字节前缀相同不等于音节数相同（比如 `"ni ha"` 是 `"ni hao a"` 这个 3 音节词字节上的前缀，但语义上应该只匹配 2 音节的候选），额外做了空格计数过滤解决，真实碰撞用例已写进单元测试。字段宽度做过一次针对性优化：一开始用 `uint64_t` 存偏移量，生成出来的 `dict.bin` 是 30.6MB（比原始 16.7MB YAML 还大，词条数乘以固定结构体开销是大头），改成 `uint32_t` 后降到约 25.0MB。

**验证**：跟 03 节切分层试点同一套方法论——① Python 侧 `Dictionary` 类改造后 39 个单元测试全部通过（含换源后重新验证的"生僻字不能排到常用字前面"回归测试）；② C 侧新增 10 个单元测试全部通过；③ **差分测试**（新工具 `diff_check_dict.py` + `diff_cli_dict.c`）——19 条查询逐条比对 Python 参照实现和 C 版，**19/19 逐字节完全一致**；④ ARM64 交叉编译零警告。

**磁盘空间复核：之前担心的是错误的分区**——我提到 [toltec](https://github.com/toltec-dev/toltec) 能不能扩容，查证后发现 toltec 目前根本不支持 Paper Pro/Paper Pro Move（官方 Discussion 里维护者原话"没人有这台设备"），且包管理器本身也不是存储扩容工具。但顺着这条线查到 [Vellum](https://github.com/vellum-dev/vellum)（明确支持 `rmppm`）的设计说明提到"把内容放 `/home/root/` 而非受限的根文件系统"，跟 reMarkable 官方开发者文档"`/home` 是独立分区"的表述相互印证——真机 `df -h` 实测确认：根分区（`/`）确实只有 83.7MB 可用，但 `cangjie-langhook.so`/`dict.bin` 实际部署路径 `/home/root/` 所在的 `/home` 是完全独立的 LUKS 加密分区，**45.8GB 可用**（约是根分区的 560 倍）——此前"25MB 的 `dict.bin` 放不下"的顾虑到此解除，测的一直是错误的分区。

**加入 41448 大字表 + 一个靠差分测试抓出来的真实 C bug**：空间确认充裕后，评估了 `rime-ice` 剩下四个扩展词库，发现障碍跟空间无关——`ext.dict.yaml`（11.9MB）权重全是常量 100、内容偏专有名词，暂缓；`tencent.dict.yaml`（17.4MB，本该最有价值）**没有拼音列**，需要先实现 Rime 那套"单字读音自动拼词组拼音+多音字消歧"算法才能用，是独立子项目，这轮不做；`others.dict.yaml` 是配合 corrector 插件用的错音提示表，故意收录错误读音，语义跟候选表不同，不接入。只加了 `41448.dict.yaml`（大字表，46019 字，跟 8105 有 7888 字重叠，加载时按 `(音节, 词)` 去重，8105 真实权重优先，41448 独有生僻字权重按 0 兜底、自然排到候选末尾）——总候选数 55.2 万涨到 58.9 万。**规模变化顺带暴露了一个潜伏的 C 端 bug**：`cj_dict_lookup_prefix` 早期实现用固定 512 大小的缓冲区收集候选、攒满就不再看后面的 key、再统一排序——数据规模变大后，像前缀 `"sh"` 这种命中大量单字的查询，真正权重最高的一批候选因为遍历顺序靠后被直接挤掉，差分测试当场测出 19 条里 5 条不一致。修复为不设固定缓冲区的**流式 top-k**（只维护 `max_out` 个"当前最优"，不管候选总数多大都不会漏掉真正的头部结果）。修复后 Python 39 + C 10 单测、差分测试 19/19、ARM64 交叉编译全部重新验证通过，`dict.bin` 涨到约 25.6MB（远小于 45.8GB 可用空间）。

> 源码：`pinyin-engine/c/src/dictionary.h`（架构设计说明，头部）、`dictionary.c:157-184`（流式 top-k）；真机接入见 02 节 Step V，`hook_init.c:2321-2472`

### ✅ 扩展词库：Iorest/rime-dict + pypinyin 自动注音——180 万词并入简体全拼，真机验证通过（洛天依 等长尾词可查）

**为什么能做上面那次做不了的事**：上一段（41448）里 `tencent.dict.yaml` 因"没有拼音列、需要自动注音"被判为独立子项目暂缓；这次我要更大的覆盖、指定 [Iorest/rime-dict](https://github.com/Iorest/rime-dict)（`luna_pinyin.extended` 导入 23 个专题词库，约 180 万词，钉 commit `a2057ba`）。它**绝大多数词条同样没有拼音列**（README 自夸"极简标音"，靠 Rime `use_preset_vocabulary` 部署时自动注音），补上"注音"这一步是关键。方案**不是**重造 Rime 的字组合启发式，而是用 **pypinyin 短语级注音**（`lazy_pinyin(word, Style.NORMAL)`，短语词典对常见词多音字基本正确：银行=hang、行走=xing、重庆=chong qing），一词一拼音、候选干净。pypinyin 只在**开发机构建期**跑（MIT/数据 CC），产出 `data/iorest.dict.yaml` 提交进仓库，**不装进设备/`.so`**——日常构建不需要它。注音边界两个坑实测钉死：① 我的音节约定用 `v` 表 ü（女=nv、略=lve），pypinyin `Style.NORMAL` 默认就输出 `v`，天然满足；② 每个输出音节过 `is_valid_syllable` 校验，非汉字/占位/不可映射的丢弃（180 万行只丢 358 条）。

**许可证事实（实测，非印象）**：Iorest 仓库**没有 LICENSE 文件**、GitHub API `license` 为 `null`、README 与 Pages 站均无来源声明，导入列表含 `sougou` 等、出处不明——这跟 rime-ice 的 GPL v3（有明确协议）不是一回事，是"默认保留所有权利、授权状态不明"。我判断**个人设备自用、不对外分发**，据此接受该法律不确定性；此处只记录事实与取舍、不代替法律意见（详见 `pinyin-engine/data/PROVENANCE.iorest.md`）。架构上仍与一贯做法一致：数据不编译进 `.so`，离线注音成 `iorest.dict.yaml` → 由独立 mmap 的 `dict.bin` 承载。

**接入范围（刻意收窄）**：iorest 只进**简体全拼** `dict.bin`（新增 `FULL_DICT_PATHS` = 默认 + iorest，只给 `gen_dict_blob` 及其差分测试用）；**不进简拼索引**（`dict_jianpin.bin` 仍从 `DEFAULT_DICT_PATHS` 精简构建，不把 180 万长尾编进"简拼快速输入"、blob 小、候选不变噪）。**繁体 `dict.zh_tw.bin` 后来也接入了 iorest**（`gen_dict_zh_tw_blob.py` 改用 `FULL_DICT_PATHS`、离线 s2twp 整体转换成繁体形式，已部署到设备、93MB，**真机验证通过**：切繁體全拼打 `luotianyi` → 出「洛天依」，繁体全拼同样能查到 iorest 长尾词——不再是本段最初写的"留作后续"）；简拼索引仍精简（不含 iorest）。权重统一低常量（=1），排在 rime-ice base 真实词频之后、只做覆盖兜底。规模：候选 58.9 万 → **226 万**（键 45.7 万 → 152.9 万）；`dict.bin` 25.6MB → 98.9MB，经 8105 白名单守卫的繁体归一化去重后回落到 **97.4MB**（`/home` 独立分区 45GB 可用，mmap 按需分页无压力）。

**240 万规模又靠差分测试抓出一个确定性 bug（跟 41448 那次同源）**：前缀查询在两边都截断到 `limit=50`，但 iorest 灌入大量同音词后前缀池远超 50、截断落在**同权重（weight=1）并列区**——Python 参照按插入序、C 按候选词字节序 tie-break，导致截断到的**成员集合**不同（`ni h`/`zhong g` 两条不一致）。修复：把 Python `lookup_prefix` 的 tie-break 对齐成跟 C 一样的**字节序**（`-weight, word.encode("utf-8")`），两边截断到的 50 个成员完全一致。修复后 Python 71 + C 单测、差分测试 19/19、ARM64 交叉编译全部通过。

**真机验证**：切"简体全拼"打 `luotianyi` → 候选出 **洛天依**（离线已确认 base 查不到、full 能查到）；日志确认 `dict.bin key_count=1528738 cand_count=2260034` 正常 mmap 加载、`dict_jianpin.bin key_count=167492` 仍是精简未含 iorest；常用输入不退化、无崩溃、`MainPID` 稳定、`NRestarts`=0。**已知限制（如实记录）**：pypinyin 对长尾专名的姓氏多音字有个别错（"曾"姓注 ceng 应 zeng、"解"姓注 jie 应 xie、"仇"姓注 chou 应 qiu）——名字是小子集，属选用 pypinyin（候选干净、不过度生成）的可接受 MVP 取舍。

> 源码：`pinyin-engine/tools/annotate_iorest.py`（pypinyin 注音）、`src/dictionary.py`（`FULL_DICT_PATHS` + `lookup_prefix` 字节序 tie-break）、`c/tools/gen_dict_blob.py`/`c/tests/diff_check_dict.py`（改用 `FULL_DICT_PATHS`）、`c/src/dictionary.c`（`cj_dict_cand_ranks_before` tie-break 注释订正）；数据与许可证留痕 `pinyin-engine/data/PROVENANCE.iorest.md`

### ✅ 真机报"简体模式出繁体候选"——已修，用 8105 白名单守卫的字符级归一化（真机验证通过）

**根因**：iorest 那 23 个专题词库大量源自台湾繁体 IME（小麦注音/新酷音），实测 **128 万**词条含繁体字形（劍網三/絕對領域/傲嬌），pypinyin 注音后进了简体 `dict.bin`，简体模式就冒出繁体候选。当初只验了"洛天依可查"、没查繁体污染。

**为什么不能直接 OpenCC t2s（踩坑）**：t2s 假设输入是**纯繁体**，喂给已简体/混合内容会**误转合法简体字**——实测 `坏→坯`（"暗中破坏"变"暗中破坯"，胡话）、`夥→伙`。盲目全量 t2s 会改坏一批简体词。

**正确修法（字符级 + 8105 白名单守卫）**：以 8105 通用规范汉字表为"保留白名单"，逐字处理——**在白名单里的字（坏/夥/乾/於/車…）一律原样保留**，只对不在白名单的真繁体独有字（嬌/劍/網/鰕…）调 t2s。这样**保证零改坏**。同一 key 内按转换后的词去重、保留最高权重。放在 `Dictionary(normalize_simplified=True)` 构造 flag 里，简体侧消费方（`gen_dict_blob`/`gen_jianpin_blob` 及各自差分测试）统一开启、保持 Python 参照与 C blob 一致；host 单测默认不开、不受影响。繁体 `dict.zh_tw.bin` 走各自 s2twp、天然不经过这里。

**效果与残留（如实记录）**：非白名单繁体字 **100% 清除**（128 万 iorest 明显繁体词全部转简，含 base 里鱼类专名的 鰕→𫚥）；**已知残留**：26 个字（車/於/卻/徵/祕…）是 rime-ice 自己的 8105 字表里带的繁体形，落在白名单里被保护、未转，且多埋在低权重候选位——属"繁简边界判断题"（如 `於` 在书面简体也用），不是 bug；要强制转这几个需逐字确认别误伤（`夥`/`乾` 不能转），留作可选后续。**验证**：Python 71 + dict 差分 19/19 + jianpin 差分 11/11 全过；真机切简体打之前出繁体的词已全简体、`破坏` 未被改坏、`MainPID` 稳定无崩溃。

> 源码：`pinyin-engine/src/dictionary.py`（`Dictionary._normalize_to_simplified` + `normalize_simplified` flag）、`c/tools/gen_dict_blob.py`/`gen_jianpin_blob.py`、`c/tests/diff_check_dict.py`/`diff_check_jianpin.py`（消费方统一开 flag）

### 📌 待办记录：`Iorest/rime-dict`——现在不引入，主要障碍是许可证缺失（比之前排除的四个文件更根本）

约 180 万条词条、30+ 个专题词库，已做跨文件去重，标准 `dict.yaml` 格式。核实结论：**没有 `LICENSE` 文件，GitHub API 返回 `license: null`**，README 也完全没有提许可证或数据来源——这比排除的 `ext`/`tencent`/`others` 三个文件的障碍更根本：那三个是"格式不兼容"或"语义不匹配"，理论上花功夫能解决；这个是**法律上默认"保留所有权利"，没有明确授权他人使用/修改/再分发**，不确定这批数据本身是不是从有 EULA 限制的来源转换来的。数据格式技术上兼容，不是技术障碍。**明确记录：现在不引入**，以后要考虑的话第一步不是看格式/规模，是先想办法联系作者确认许可条款或找到明确的数据来源声明。

> 注：这条待办后来被上面"扩展词库"一节推翻——按"个人设备自用、不对外分发"的判断纳入了 iorest，本条保留作决策演进记录。

### ✅ KeyPopup 候选数量安全上限——为翻页设计钉一个数字，02 节 Step V 的分页参数就是照这个定的

候选过多时怎么翻页，此前完全没设计过——`KeyPopup` 原生是"长按一个字母键弹出 3-6 个重音变体"的组件，从没测过更大数量下的真实渲染效果。用临时诊断 hook（中文模式下每按一次字符键切换到下一个测试数量，候选内容用中文数字词"一/二/.../二十"而不是 ASCII 数字——CJK 字符实际渲染宽度明显大，ASCII 测出来的上限会高估）真机连续测试：3/5 个单行显示清晰；**7 个开始换行到两行，仍可读**；18 个还能正常显示（两行内挤下）；**20 个直接挤出屏幕，不可用**。结论：安全上限在 18-20 之间；02 节 Step V 的分页方案选用每页 8 个，在安全范围内留足余量，也符合主流拼音输入法的常见做法。诊断代码验证完已完整回退，真机重新部署确认 Step T 正常拼音功能未受影响。

## 07｜简拼与中英混输——均已接入设备并真机验证

本节最初是设计规划，对应产品要求"拼音要支持全拼、简拼、中英混输"（全拼已经是 03/06/08 节验证过的现状，本节只补简拼和中英混输两块空白）。参照的是雾凇拼音背后 RIME 生态里"简拼/中英混输是引擎侧的通用惯例，不是词库数据"这个事实——简拼的声母缩写规则、中英混输的触发方式，都是公开的拼音输入法编码惯例，不是受版权保护的词库内容，用自己的 C 代码重新实现不涉及 04 节双拼那种"是否构成链接"的顾虑，风险类别比引入新的 GPL 数据更低。**简拼这部分已经走完 Python 原型→C 移植→接入设备→真机验证的全流程**（见下方）；**中英混输也已接入设备并真机验证，但设备实现没有照搬 Python 原型的 Shift 直通状态机，改用了更贴近 iOS 的"候选式"方案**（见下方"中英混输"小节）。

### 简拼——声母首字母倒排索引

#### 📌 索引设计

简拼字母表沿用 03 节从 06 节词库反推出的 23 个声母（`b p m f d t n l g k h j q x zh ch sh r z c s y w`）——多字母声母 `zh/ch/sh` 的简拼字母取首字母（`z/c/s`），这会跟单字母声母 `z/c/s` 产生天然歧义（比如按 `z` 既可能对应 `z` 声母也可能对应 `zh` 声母的字），这是几乎所有主流拼音输入法公认接受的行为，靠词频排序消歧，不打算特殊处理消除。零声母音节（`an`/`ou` 这类，03 节音节表里本就存在）没有声母，简拼字母退化成韵母首字母。对词典里每个多音节词条计算"简拼 key"＝各音节简拼字母顺序拼接（"拼音引擎" pin+yin+yin+qing → key=`pyyq`），生成一个独立的倒排索引 blob（暂定 `dict_jianpin.bin`），复用 06 节已经验证过的"排序 key 数组 + 二分查找区间"格式——只是 key 的计算方式从"完整拼音"换成"简拼字母"，不是重新设计数据结构。候选文字/权重复用主 `dict.bin` 已有的候选池（用偏移量引用，不重复存储文字），额外体积只有 key 表本身。

**查询逻辑——跟 03 节全拼路径并行，不是替代**：简拼输入天然"一个字母对应一个完整音节"，不需要 03 节 `segment.c` 那套带隔音符号/歧义切分的逻辑，直接按字符定长切分即可。真正的难点是缓冲区还没输入完时无法确定我在打全拼还是简拼（`"px"` 既可能是简拼、也可能是全拼音节 `p` 还没打完+新音节 `x` 刚开始）——设计成两条路径并行查询、结果合并展示：全拼路径＝03 节 `segment.c` 现有逻辑不变，简拼路径＝新的定长切分+简拼索引查询，按统一词频排序合并去重，简拼候选给一个可调的降权系数（具体数值留给真机试验调，这里只定"全拼精确匹配优先于简拼模糊匹配"这个方向）——避免简拼天然更大的候选量把全拼精确结果挤到分页后面。

**跟 05 节繁体双 blob 设计的关系**：简拼索引本身只依赖发音（音节），不随字形变化——理论上一份索引可以同时服务简体/繁体两份候选池（只要两份 `dict.bin` 用同一词条顺序生成，索引里存的候选偏移对两边同时有效），这是"值得做但非必须"的优化。MVP 允许先各自独立生成一份完整简拼索引，更简单但有少量数据重复，压缩空间不是当前设备的实际约束（06 节磁盘空间调研已确认 `/home` 分区 45.8GB 可用）。

**明确不做（MVP 范围）**：不支持"同一个词里全拼、简拼混用"（比如一个音节打全拼、下一个音节打简拼，部分输入法支持这种更灵活的模式，这次不做，非必要复杂度）；不做拼音容错/模糊音（跟 06 节排除 `others.dict.yaml` 是同样理由——语义超出候选查询本身的范围）。

#### ✅ Python 原型已落地（`pinyin-engine/src/jianpin.py` + 10 个单元测试全部通过）

索引设计比上面写的更简单一步——不需要真的建一张 23 个声母的查表，`jianpin_letter(syllable)` 恒等于 `syllable[0]`（`zh`/`ch`/`sh` 这三个双字母声母本身的首字符就是 `z`/`c`/`s`，跟单字母声母/零声母音节在字符层面天然统一，是拼音正字法本身的巧合，不是需要额外编码的规则）。索引只收多音节词——单音节词的简拼字母跟全拼首字母是同一个东西，查询效果已经被 `segment.py` 的 pending 前缀匹配覆盖，重复收录没有收益。`Dictionary.items()` 新增一个公开遍历接口供索引反推整份词典用，不直接暴露内部 `_table`。`PinyinEngine.query_jianpin()` 接上了一个独立入口（跟全拼 `query()` 不合并，上面"查询逻辑"提到的加权合并留给真机试验阶段）。

**接入时被自己的单元测试当场打脸一次，值得记一笔**：最初 `query_jianpin()` 抄了 `query()` 的分层模式——"精确匹配优先，不够 limit 再前缀匹配补位"。写 `test_query_jianpin_prefix_fills_in_when_exact_is_sparse` 才发现：`"py"` 精确匹配的 2 音节词有 213 个（"朋友"/"培养"/"便宜"……权重都不低），随便一个 limit 都会被精确匹配自己占满，"拼音输入法"（`pysrf`，只有前缀匹配才能覆盖）永远排不到补位机会——精确匹配结果集本来就是前缀匹配结果集的子集，拆成两段查询是画蛇添足，还引入了真实 bug。改成直接用一次 `lookup_prefix()`、纯按权重排序，问题消失。这个坑跟 06 节 `dictionary.c` 那次"攒够 512 个候选就不再看后面的 key"是同一类型的错误，区别是这次在单测阶段就被抓到，不是等接了真实数据规模才在真机上暴露——"先写差分/边界测试，再信任实现"这条项目纪律又一次起了作用。

**还没做的（这一版）**：跟全拼路径的加权合并（上面"查询逻辑"一节已经说明留给真机试验）；接入 `xovi-extensions/cangjie-langhook`/碰设备。C 移植已经完成，见下方。

> 源码：`pinyin-engine/src/jianpin.py`、`pinyin-engine/src/engine.py`（`query_jianpin`）、`pinyin-engine/tests/test_jianpin.py`、`pinyin-engine/README.md` 第 7 条

#### ✅ C 移植已完成（`pinyin-engine/c/src/jianpin.c` + `jianpin.h`），差分测试 11/11 完全一致

跟 06 节 `dictionary.c` 同一套模式——排序 key 数组 + 二分查找 + mmap 只读、流式 top-k 前缀匹配（不设固定缓冲区，避免重演 06 节"攒够 512 个候选就漏掉后面更高权重结果"那次真机+差分测试都验证过的 bug）。生成工具 `gen_jianpin_blob.py` 从同一份 `Dictionary`+`JianpinIndex` 导出 `dict_jianpin.bin`（167,492 个 key、542,948 条候选、15.0MB）。11 个 C 单元测试全过，差分测试（`diff_check_jianpin.py`，覆盖常见词、单音节 key 应为空、前缀命中更长词、高频前缀、未知 key、空前缀）**11/11 逐字节一致**，ARM64 交叉编译零警告。

**跟本节最初设计的一处偏离，明确记录**：上面"索引设计"一段原话是"候选文字/权重复用主 `dict.bin` 已有的候选池，用偏移量引用，不重复存储文字"——真正实现时改成了自成一体的独立 blob（`dict_jianpin.bin` 自己存一份候选词文字+权重，不引用 `dict.bin`）。原因：偏移量引用要求两份 blob 的候选表生成顺序/下标严格对齐，一旦 `dict.bin` 以后换数据源/加扩展词库导致内部顺序变化，`dict_jianpin.bin` 里存的下标会悄悄失效却不报错——mmap 场景下这是最不想要的一类 bug（不崩溃，读到无意义数据）。改成两份数据各自独立生成、独立校验、运行时互不依赖，代价只是多占约 15MB 磁盘（06 节磁盘空间调研已确认 45.8GB 可用），跟 05 节繁体双 blob"允许各自独立生成、接受少量数据重复"是同一个取舍方向。

**还没做的（这一版）**：跟全拼路径的真正加权合并（这一版用"全拼先填、简拼补位"简化，见下方）。接入设备已经完成，见下方。

> 源码：`pinyin-engine/c/src/jianpin.c`、`jianpin.h`、`pinyin-engine/c/tools/gen_jianpin_blob.py`、`pinyin-engine/c/tests/test_jianpin.c`、`diff_check_jianpin.py`、`diff_cli_jianpin.c`

#### ✅ 已接入 `xovi-extensions/cangjie-langhook`，真机验证通过

落地方式比最初"查询逻辑"一节设想的"两条路径加权合并排序"更简单——`cj_step_v_generate_dict_candidates`（06 节全拼路径）先把候选填进缓存数组，缓存槽位（`CJ_STEP_V_CACHE_CAP`=64）还有空间时，新增的 `cj_step_v_generate_jianpin_candidates` 用当前拼音缓冲区（本身已经是纯小写字母，天然就是合法简拼 key/前缀，不需要像全拼路径那样从切分结果重新拼）查一次简拼索引，结果按候选文字去重后追加在全拼候选之后——天然满足"全拼精确匹配优先于简拼模糊匹配"这个方向，代价是简拼候选的权重数字本身在跨路径排序里不生效（只在简拼内部互相比较时有意义），这一版明确接受这个简化。只调用一次 `cj_jianpin_lookup_prefix`（不再重演 Python 原型阶段"exact 和 prefix 分两次查、短 exact 结果抢占配额"那个坑）。词典 handle 选择复用 05 节 `cj_dict_select_handle_for_mode` 同一套"惰性打开、成功/失败都不重试"模式，新增 `cj_jianpin_select_handle` 按繁简选对应索引文件；两个选择器现在共用调用方（`cj_step_t_refresh_candidates`）算好的同一个 `CjLanguageMode`，顺手把"词典读一次语言属性、简拼再读一次"合并回一次，语言属性只读一次。

**真机验证时依次发现并修复的两个真实 bug**：① **切换繁体后简拼补充候选依然是简体字**——根因是最初只生成了 `dict_jianpin.bin`（从简体词典反推），繁体模式下全拼路径已经正确走 `dict.zh_tw.bin`，但简拼补充没有对应的繁体版本可选；全拼路径命中 0 个候选时（缓冲区还没形成合法完整音节），简体简拼候选甚至会顶替成"第一名"，空格/回车直接提交出简体字，不是只在候选栏靠后位置才出现的小瑕疵。修复：照抄 05 节繁体双 blob 的思路，新增 `gen_jianpin_zh_tw_blob.py`——`JianpinIndex.__init__` 只依赖传入对象有 `.items()` 方法（鸭子类型），`build_traditional_table()` 返回的 `dict[tuple, list[Candidate]]` 天然满足，不需要改 `jianpin.py` 一行代码，生成 `dict_jianpin.zh_tw.bin` 并接入 `cj_jianpin_select_handle`。② **"m"/"n" 两个字母打不出任何简拼候选**——两个问题叠加：一是简拼补充函数最初写了"缓冲区长度 < 2 就跳过"的判断，理由是"简拼索引只收多音节词，单字母不可能命中"——这个理由只对*精确匹配*成立，对*前缀匹配*是错的（单字母完全可以是多字母简拼 key 的合法前缀，比如 "m" 是 "mn"=每年、"mh" 等词的前缀），错误地把这条经验套用到了前缀匹配上；二是 "m"/"n" 恰好本身也是合法的完整拼音音节（叹词"呒"/"嗯"），06 节全拼路径的切分逻辑一敲完这一个字母就判定"音节已经打完"（`has_pending=false`），不会像 "b" 这类本身不成词的声母字母那样触发全拼路径自己的前缀匹配兜底——两条路径在这个边界情况上同时失效，候选栏才会完全没反应。修复：去掉简拼补充函数里那条错误的长度下限，缓冲区非空时一律正常做前缀查询。两次都是真机实测直接发现，不是代码审查推演出来的。

**还没做的**：跟全拼路径的真正加权合并（见上面"还没做的（这一版）"）；`KeyPopup` 点选提交跟"这是简拼命中的第几条候选"没有索引式对接（限制跟 06 节 M4 里程碑记录的一致，提交的是候选文字本身，够用）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:2051-2059`（`cj_dict_select_handle_for_mode`）、`:2072-2124`（`cj_jianpin_ensure_open`/`cj_jianpin_zh_tw_ensure_open`/`cj_jianpin_select_handle`）、`:2413-2449`（`cj_step_v_generate_jianpin_candidates`）、`:2738-2875`（`cj_step_t_refresh_candidates` 接入点）；`pinyin-engine/c/tools/gen_jianpin_zh_tw_blob.py`、`pinyin-engine/c/tests/test_jianpin_zh_tw.c`

### 中英混输——iOS 候选式（点选英文候选上屏，取代 Python 原型的 Shift 直通）

#### 📌 触发信号

复用 02 节 Step N 已经真机验证过的 Shift 态文本读取能力——如果当前拼音缓冲区为空、且这次按下的字符键处于 Shift（大写）态，整个输入会话切换进"西文直通"模式，不再走 03/07 节的任何拼音/简拼解析，缓冲区原样存放我敲的字符（大小写都保留），作为唯一候选实时展示在 `KeyPopup`（02 节已验证的候选栏载体）里；空格/回车直接把缓冲区原文提交（不追加真空格/不发送真回车，跟 02 节 Step T 现有的"提交占位"机制完全一致，只是提交内容从拼音换成原始英文），提交后清空缓冲区、退出西文直通模式，回到"下一次按键决定模式"的初始状态。

**为什么不用启发式自动检测（考虑过，明确排除）**：曾设想"如果缓冲区在任意时刻既不能构成合法拼音前缀、也不能匹配任何简拼路径，就自动判定为英文"——但英文字母序列几乎总能匹配上某个简拼路径的合法声母/韵母前缀（比如 `hello` 里的每个字母单独看都是合法声母或韵母开头），自动回退条件几乎不会触发，等于没有这个功能。改用显式的 Shift 触发信号，判定 100% 确定，不依赖猜测——这也是主流拼音输入法（含雾凇拼音的推荐配置）实际采用"大写字母触发西文直通"惯例的原因，不是这里独创。

**已知限制（MVP 范围）**：不支持"句子中途插入英文单词但不换大小写"这种更智能的检测（真实产品也很少做，风险/收益不成比例）；不支持在同一次提交里输出"你好 hello 世界"这种一次性混合结果——中文候选和西文直通内容分属两次独立的提交动作，靠切换 Shift 状态分开，这跟大多数拼音输入法的实际交互一致。

**跟繁体模式的关系**：西文直通内容不经过任何简繁转换（05 节双 blob 设计只影响中文候选文字），这是"功能对等"要求里天然满足的一项，不需要额外设计。

#### ✅ Python 原型已落地（`pinyin-engine/src/mixed_input.py` + 15 个单元测试全部通过）

状态机（`MixedInputState`）只处理"要不要进入西文直通模式、缓冲区怎么变、什么时候提交"这几件事，不依赖 `segment.py`/`dictionary.py` 任何一行——设计本身决定的（Shift 触发的西文直通是纯粹的按键状态机，跟拼音引擎的候选生成完全无关），不是这轮偷懒省事。四个方法对应四类按键：`handle_char`（缓冲区空+大写进入直通，空+小写返回 `PASSTHROUGH_KEY` 交给拼音模式；已在直通模式则原样追加，大小写不做规范化）、`handle_space_or_enter`（缓冲区非空原样提交，为空正常打空格/回车）、`handle_backspace`（弹出最后一个字符，弹空自动退出直通模式）、`handle_other_key`（CapsLock 等其它按键，缓冲区非空先提交一次再照常处理，不留悬空缓冲区）——后三个方法命名和"先提交/清空、再正常处理"的模式跟 `hook_init.c` Step T 对拼音缓冲区的安全网特意保持对称，以后翻译成 C 的时候少一层"这两处到底是不是同一个逻辑"的判断。

测试覆盖全部状态转移，包括一个容易漏掉的边界：`test_full_round_trip_scenario` 验证"提交完一个英文词（比如 `Hello`）后紧接着敲一个小写字母，应该被重新判定为交给拼音模式，不是残留在直通状态里"；`test_mixed_case_word_preserved_verbatim` 验证大小写混合的词（模拟中途松开 Shift 继续打小写字母的真实打字习惯）原样保留，不做任何规范化。

**还没做的**：C 移植、接入 `hook_init.c`/碰设备。这一层比 `segment.c`/`dictionary.c`/`jianpin.c` 都更依赖真机验证——Shift 态检测本身、这个状态机假设的"缓冲区为空时才判断进入哪个模式"在真实按键时序下是否站得住，都需要跟 `hook_init.c` 已有的 Step T 状态机对照验证，不是照抄这份 Python 逻辑直接翻译就完事，风险类别比前三次 C 移植（纯算法/数据结构，行为在 Python 阶段就能钉死）更高，真正动手前应该单独确认。

> 源码：`pinyin-engine/src/mixed_input.py`、`pinyin-engine/tests/test_mixed_input.py`、`pinyin-engine/README.md` 第 9 条

#### ⚠️ Step FF（已被 Phase A A4 取消，保留作过程记录）：接入设备时先试了 iOS"可点英文候选"，后来做逐字输入法时把它移除了

**结论先行**：Step FF 曾把"我实际敲的原始字母"作为一个可点候选塞进候选栏（点它上屏整串英文）。但随后做 Phase A 逐字候选时，我对照 iOS 指出 iOS 候选栏**只有词候选、不显示原始输入**（输入内容在预编辑区/预览行）——于是 A4 把这个英文字母候选**移除**，候选栏改成 iOS 干净版。**当前中英混输现状**：整串英文上屏靠回车（原样提交拉丁字母，02 节 Step T）、首字母大写英文靠 Shift 直接透传上屏；iOS 那种"你好hello"候选需要设备端英文词典——**这一步已由下方 Phase C 完成**（词典驱动的前缀补全，取代 Step FF 的原始字母候选；仅"连打 nihaohello 一次成句"的联合切分仍后置）。下面这段是 Step FF 当时的实现记录，机制（候选点选复用现成提交闭环）在 Phase A 里被继承下来用作逐字提交的地基，所以保留。

**为什么不照搬 Python 原型**：动手前重新对照了 iOS 拼音键盘的实际交互——iOS **不需要 Shift** 切模式，我正常敲小写字母，候选栏里会有一个候选就是"我实际敲的那串字母"，点它就上屏英文。这跟上面 Python 原型的"Shift 触发西文直通模式"是两套不同设计。我明确要求"参考 iOS 逻辑"，经确认走 iOS 候选式，Python 那套 Shift 状态机作为被取代的原型保留、不移植。

**为什么这条路几乎零新增管线（关键）**：设备端拼音缓冲区 `g_pinyin_buffer` 里始终存的是"我实际敲的原始拉丁按键"——全拼/简拼下是拼音字母、双拼下是双拼按键原文（只在查词典时才解码成拼音）。而候选点选早就有完整闭环：点候选 → QML `onKeySelected` → `insertText(候选文字)` → `cj_vk_insert_text_handler` 判定为外部提交 → 清空缓冲区+隐藏候选栏 → 原生插入该文本。**所以只要把 `g_pinyin_buffer` 这串原始字母作为一个候选塞进候选栏 `keys` 列表，点它就自动复用这条现成路径提交英文并清空状态，不需要写任何新的提交/模式逻辑。** 回车键本来也已经是"原样提交拉丁字母"（`force_raw_pinyin=1`），键盘路径的"提交我敲的"早就有，缺的只是 iOS 那个"可点"的候选。

**实现**：新增 `cj_step_t_update_popup_with_english()`，把英文候选插到展示第 1 位之后（`count>0` 时下标 1，即紧跟第一名中文候选——收起单行里始终可见；无候选时下标 0，成为唯一候选），两条候选展示路径（真词典候选 `cj_step_v_display_page`、词典没命中的拼音切分兜底）都改用它。英文候选是纯 ASCII（缓冲区只收 a-z），ASCII ⊂ UTF-8，无论真汉字（`is_utf8=1`）还是拼音切分（`is_utf8=0`）路径都能被现有编码函数正确处理。**只影响候选栏展示，不碰 `g_step_v_cached_candidates`**——空格键仍提交第一名中文候选、翻页/滚动逻辑一律不受影响。**天然支持三种模式**：注入的就是 `g_pinyin_buffer`，跟全拼/简拼/双拼无关（双拼下预览行显示解码后拼音、英文候选显示原始按键，两者用途不同、各自正确）。

**范围（小写 MVP）**：英文候选覆盖小写串（承重的"只缓冲小写字母"逻辑不动）；大写维持现状——Shift+字母直接原样透传上屏英文，不进候选。首字母大写的英文（iPhone）这一版不做单条候选，留待后续。

**真机验收（三种模式逐项，日志+肉眼双确认）**：① 双拼：敲 `hello`，候选栏第 2 位出现 `hello`，点它上屏英文；日志"展示 64 个候选 → 更新 count=65"正好差 1，证明注入生效、64+1=65 的容量边界无溢出。② 全拼：`nihao` 第一名"你好"、第 2 位 `nihao`；`hello` 点选上屏；空格提交"你好"、回车原样提交 `world`（回归确认英文候选没打乱空格/回车语义）。③ 简拼：`bj`→"北京"、`zg`→"中国"，第 2 位英文候选同样在。全程 `xochitl` 无崩溃、`MainPID` 稳定、`NRestarts=0`，ARM64 交叉编译零警告。

**还没做的**：首字母大写/驼峰英文的单条候选（小写 MVP 之外）；iOS 那种提交后自动加空格；英文候选在候选栏里的视觉区分（这一版跟中文候选同样式，纯文本，零 QML 改动）。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:1934-1964`（`cj_step_t_update_popup_with_english`，A4 后已标记 unused 保留）、`cj_step_v_display_page` 与 `cj_step_t_refresh_candidates` 拼音切分兜底两处调用点；点选提交复用 `cj_vk_insert_text_handler` 的外部提交同步逻辑

#### ✅ Phase C 已完成并真机验证——中英混输改成"词典驱动的 iOS 式前缀补全"，取代 Step FF 的原始字母候选

**结论先行**：Step FF 把"我实际敲的原始字母"当候选（打 `shenme` 也会把 `shenme` 塞进候选栏，是噪音）。Phase C 改成**只在缓冲区命中真实英文词时**才给英文候选，背后是一份设备端英文词典。效果：打 `nihao`→选"你好"→再打 `hello`→候选栏（第一名中文候选之后）出现 `hello`→点它上屏，得"你好hello"；打 `hel` 就按词频补出 `help/held/helpful/hello`（iOS 式前缀自动补全）。我真机逐项确认全拼/简拼/双拼三种模式英文候选均正常、常用中文不退化。

**几乎零新增管线（关键复用）**：英文词典 `english.bin` 用**跟 `dict.bin` 完全一样的 CJDICT01 格式**（key=英文词本身，单候选=该词，weight=缩放后词频），`cj_dict_open`/`cj_dict_lookup_prefix` 一行 C 不改就能打开——`cj_dict_lookup_prefix` 的"按权重 top-k + 空格数过滤"恰好就是"按词频排序的英文自动补全"（英文词 0 空格、缓冲区 0 空格，过滤天然通过）。英文候选注入现有 `g_step_v_cached_candidates`/`consumed` 平行数组后，展示（`cj_step_v_display_page`）、点选（candidatebar `\x02<idx>`）、分段提交（`cj_step_t_commit_candidate_by_index`，`consumed=`整段缓冲区→裁掉整串+上屏英文词）三条现成路径全部自动生效——**不改 QML、不改点选逻辑、不改查询层**。**与模式无关**：查的是 `g_pinyin_buffer` 里的原始拉丁按键（全拼=拼音字母/简拼=声母/双拼=双拼按键原文），我想打英文时敲的就是字面 h-e-l-l-o，三种模式命中一致。

**词典来源与许可证（实测）**：SymSpell `frequency_dictionary_en_82_765.txt`（~8.3 万高频词，`词 词频` 两列，钉 commit `c239062`），MIT 分发、派生自 Google Books Ngram（CC BY 3.0，需署名）∩ SCOWL——比 iorest 干净得多。词频十亿级（"the"=231 亿）超 uint32，生成时按常量除数 6 等比缩放（只用于相对排序、单调保序）。数据**不编译进 `.so`**，`english.bin`（3.5MB）运行时 mmap 只读，跟 rime-ice/iorest 同一架构取舍（留痕 `data/PROVENANCE.english.md`）。

**范围（增量式 MVP，如实记录）**：交互是"分两次输入"（`nihao`→选→`hello`→选），**不做**一口气连打 `nihaohello` 自动切成"你好hello"的联合拼音/英文边界切分——那需要边界打分模型、歧义大、是独立的更大工程。小写 MVP；大写英文仍靠 Shift 直接透传。候选栏无视觉区分（纯文本，零 QML 改动）。

**顺带修掉一个 Step W 遗留 bug（真机报出）**：候选栏停留一会儿再按空格**丢字不上屏**。根因：每次按键先跑 10 秒空闲静默清空（`cj_step_t_expire_buffer_if_stale`，autoHideTimeout 时代的遗留），跟 Step W"候选栏只要有候选就一直显示、直到显式操作"直接矛盾——停顿后按空格这次先触发过期把缓冲区悄悄丢了，空格看到空缓冲区什么都不提交。修复：**整个去掉空闲过期**，composition 真正持续到提交/回车/退格清空/焦点切换/切语言才结束。真机确认停顿后按空格正常提交、"pi 停顿后打 i 变 pii"的拼接错觉也一并消除。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c` `cj_english_dict_ensure_open`（≈2023）、`cj_step_c_add_english_candidates`（≈2462）、`cj_step_t_refresh_candidates` 注入点（≈2896）、空闲过期移除（≈3175）；生成器 `pinyin-engine/c/tools/gen_english_blob.py`；数据/许可证 `pinyin-engine/data/english/` + `PROVENANCE.english.md`

## 08｜e-ink 候选栏渲染要点

候选栏是全键盘里刷新最频繁的部分（每敲一个字母都要重绘一次），墨水屏的刷新特性和普通 LCD 完全不同：

- **局部刷新**：只重绘候选栏所在的矩形区域，不要触发整屏刷新，否则每敲一个字母都黑闪一下，体验会很差。
- **防重影**：候选栏这种高频局部刷新区域，隔一段时间（比如每输入完一个词）触发一次该区域的"深度刷新"清除残影，纯局部快速刷新容易积累重影。

### ✅ 已实测跑通——不再是伪代码骨架

`pinyin-engine/ui/CandidateBar.qml` + `render_demo.py` 用 PySide6 把 03 节的真实引擎、06 节的真实词库候选、姊妹文档 3.3 节裁剪出来的真实字体接到一起，`QT_QPA_PLATFORM=offscreen` 离屏渲染成截图验证过效果：`nihao` 输入正确渲染出"你好"（加粗边框标出默认上屏项）+ 后续候选，`zhongwenshurufa` 正确渲染"中文输入法"，`zho` 这种"打到一半"的 pending 输入也正确显示。桌面 Qt Quick 阶段能验证的是"数据链路通不通、候选栏排版逻辑对不对"，e-ink 局部刷新/防重影这两条实际效果已经在真机的 `KeyPopup` 复用方案里得到印证（02 节），这版原型当初只覆盖前者。

```qml
// pinyin-engine/ui/CandidateBar.qml 的候选栏核心片段（已跑通，非伪代码）
Repeater {
    model: bridge.candidates   // bridge 是 Python 侧暴露的 QObject，candidates 来自真实 PinyinEngine.query()
    Rectangle {
        width: candidateText.width + 24
        height: 44
        border.color: "black"
        border.width: index === 0 ? 2 : 1   // 默认上屏项加粗边框
        Text {
            id: candidateText
            text: modelData
            font.family: fontLoader.name   // 绑定姊妹文档 3.3 节裁剪出来的真实 subset 字体
        }
    }
}
```

### ✅ 候选栏怎么真正接进 xochitl——复用键盘自带的 `KeyPopup` 组件，全部真机验证通过

（02 节有最终结论汇总，这里是候选栏这一侧的完整推导+验证过程。）

反编译按键分发函数时读过的宿主 QML（`KeyboardPanel.qml`）里，有一个专门给"长按字母键弹出重音符号"用的 `KeyPopup` 组件实例：`parent: virtualKeyboard`、`onKeySelected: (symbol) => virtualKeyboard.insertText(symbol, 0, 0)`。`KeyPopup.qml` 本体结构是 `property alias keys: keyRepeater.model`（纯字符串列表喂给 `Repeater`）+ `signal keySelected(string)`——跟拼音候选栏要做的事在结构上几乎是同一件事：给一串字符串、渲染成可点候选、选中后提交，唯一差别是外观和触发场景。它已经活在真实场景树里，显隐/定位/点击提交这套管线是既有机制，不需要凭空造一个新组件。

**拿到 `KeyPopup` 实例指针，一次假设被真机数据推翻**：先尝试 `QObject::children()`（内存归属树）——这个 Qt 6.10 构建里它被内联，没有独立导出符号，改用 xochitl 自己代码里真实调用过的 Qt 内部辅助函数 `qt_qFindChildren_helper`（反编译真实调用点确认参数编码）。真机部署：符号解析成功、调用不报错，但**返回 0 个子对象**——排查后发现假设本身错了：QML `parent: virtualKeyboard` 控制的是 `QQuickItem` **视觉父级**，跟 `QObject::parent()`（`children()` 遍历的那棵树）是两回事，`keyPopup` 真正的 QObject 归属父级不是 `virtualKeyboard`。改用视觉树自己的访问器 `QQuickItem::childItems()`（真机 `nm` 确认是导出符号）——这个函数按值返回 `QList`，AArch64 ABI 下超过 16 字节的聚合类型走"隐藏指针"约定，没有手写汇编摆弄 X8 寄存器，而是把 C 端返回类型声明成一个 24 字节的普通结构体，让编译器自己按 ABI 规则生成调用代码（部署前反汇编确认生成了 `mov x8, <sret地址>` 再 `blr`，跟 Qt 的期望完全匹配——AAPCS64 是硬件调用约定，C/C++ 两边编译器必须遵守同一套规则，风险比反编译细节确定的私有偏移低得多）。真机部署：`virtualKeyboard.childItems()` 返回 5 个视觉子项，其中就有 `KeyPopup_QMLTYPE_2293`（QML 运行时类型名带编号后缀，只能前缀匹配）。这个"从错误对象出发查询返回 0 个结果、改用另一条树"的教训后来在 02 节 Step W 里被**反向复用**了一次——找 `autoHideTimeout` 的 Timer 时同样先踩了"从哪个对象出发"的坑。

**写入验证**：`QObject::setProperty()` 对 QML 编译期合成的 `property alias` 确认畅通无阻（返回 `true`，读回值正确）。`autoHideTimeout`（10 秒无操作自动隐藏）这个只读属性最终在 02 节 Step W 里通过找到并直接停用背后的原生 `QQmlTimer` 解决，不需要改 `.qml` 资源本身。

> 源码：`xovi-extensions/cangjie-langhook/src/hook_init.c:1265-1287`（`cj_resolve_children_search_symbols`）、`:1567-1613`（`cj_resolve_keypopup_write_symbols`）

### 📌 竞品借鉴（rmtool / rmkit-cn）·用固件自带 Mono 屏幕模式压低候选栏刷新延迟

> 来源：`pretenderlu/rmtool`（GPL-3.0）`fast-mono-reading/qmd-src/fast-mono-reading-3.28.qmd`，其屏幕模式机制上游是 `boangs/rmkit`。本条为**源码研读结论，未在本项目真机验证**，落地前须按本项目纪律先只读核实 Move .166 的真实 QML 布局。

**核到的事实**（交叉印证：rmtool QMD + `rmkit-cn` 附带的 `qml-dump/qml/device/view/main/MainView.qml:671`）：xochitl 的墨水屏刷新模式在 QML 层就可控，不需要 hook C++、更不用碰 framebuffer/waveform：

- `import xofm.libs.epaper as Epaper` 暴露组件 `Epaper.ScreenModeItem`，取值有 `Mono` / `UI` 等；`MainView.qml` 里有 `id: globalScreenMode` 的实例，文档视图通过 `documentView.item.globalScreenMode` 决定当前刷新模式，默认 `Epaper.ScreenModeItem.UI`。
- rmtool 的做法就是把文档视图那个 ScreenModeItem 的 `mode` 改绑成"启用快速黑白时 = `Epaper.ScreenModeItem.Mono`"。彩色墨水屏刷一次彩色要多遍 waveform，锁成单色即跳过彩色通道，翻页/局部重绘明显变快。
- 残影用固件自带的 `root.ghostBuster.forceClearNow(...)` 周期全刷清除（rmtool 每 N 页触发一次，N 可选 5/10/20/30 或从不）。

**对本项目候选栏的价值**：候选栏是全键盘刷新最频繁的区域（本节开头已述"每敲一字母重绘一次""高频局部刷新积累残影"）。如果在**拼音组词态**把候选栏所在场景切到 `Mono`、**提交/切回英文后恢复**原模式，候选刷新的黑闪与延迟感有望明显下降；配合"每输入完一个词触发一次深度刷新"正好复用 `ghostBuster` 路径清残影——这与本节 572 行早就写下的"防重影"设想是同一思路，rmtool 证明了它在 QML 层可实现。

#### ✅ 离线可行性预研（2026-08-15，基于本地 `.164` 二进制解包，比原假设更简单）

原清单假设"本地无解包源、只能真机 dump"，这一条已过时——`reading-qol/tools/extract_qml.py`（3.5 Step J zstd 全局扫的可复用落盘版）已能离线解 xochitl 内嵌 QML。用它解 `rmfw/xochitl_3.28.0.164.bin`（556 个 QML）认领键盘场景文件后，三项清单的结论如下：

- **[✅ 第 1 项 · 已由 reading-qol 在 .166 证实]** `Epaper.ScreenModeItem` / `.Mono` 枚举在 .166 存在——阅读增强那条线已用 .166 真实 QML（`DocumentView.qml:489` + 设备 hashtab）+ 真机部署坐实（见姊妹文档《中文化白皮书》3.6 / `reading-qol/fast-mono-reading.qmd`）；`.164` 上 `KeyboardPanel.qml` 自身 `import xofm.libs.epaper as Epaper`，同族。
- **[✅ 第 2 项 · 结论修正，比原假设好]** 原以为"键盘浮层与 `DocumentView` 不在同一子树、够不到 `globalScreenMode`"是障碍——**实际不需要够到它**。`.164` `KeyboardPanel.qml`（候选栏 `CjCandidateBar` 注入的同一个文件）里 `keyboardContainer` **自带一个键盘区专属的 `Epaper.ScreenModeItem { id: screenMode; objectName: "keyboard"; mode: Epaper.ScreenModeItem.Animation }`**（默认 `Animation`，多遍 waveform 换流畅）。要压候选栏刷新，正确做法是把这个 `#screenMode` 的 `mode` 在**拼音组词态换成 `Mono`、提交/切回英文后恢复 `Animation`**，作用域天然够得着、且不牵动阅读页的 `#content`——比阅读那条要协调三个 QML 文件、还踩过"改错 ScreenModeItem 对象"的坑简单一级。
- **[⚠️ 第 3 项 · 键盘场景不需要它]** `.164` `KeyboardPanel.qml` 内 `ghostBuster` **不可达**（无引用）。但键盘区是局部小面板、`Animation↔Mono` 模式切换本身即触发刷新，全屏 `forceClearNow` 用在这里反而突兀——键盘 Mono 方案**不依赖 ghostBuster**（这跟阅读整页需要周期全刷清残影的场景不同），故不构成阻塞。

**结论（置信度：高，基于 `.164`；`.166` 结构复核见下）**：候选栏 Mono 加速**可行且实现路径干净**——单文件（`KeyboardPanel.qml`）REPLACE `#screenMode` 的 `mode` 绑定成"组词态 Mono / 否则 Animation"，组词态由已存在的缓冲区状态机驱动。`.166` 复核前提：`candidatebar.qmd` 已在 .166 真机正常注入 `KeyboardPanel.qml`，证明该文件与注入点在 .166 有效；剩余需设备复核的只是"`#screenMode`/`objectName:"keyboard"` 在 .166 行号/结构一致"。

#### ✅ 已落地并真机验证通过（2026-08-15，`keyboard-mono.qmd`）

预研之后按用户要求落地。实现与设计一致——单文件 `keyboard-mono.qmd`（放 `reading-qol/`，与 tap/fast-mono 同族），REPLACE `#screenMode` 的 `mode` 绑成 `cjCandidateBar.visible ? Epaper.ScreenModeItem.Mono : Animation`，组词态信号取 `candidatebar.qmd` 注入的候选栏可见性，**零 C 改动**。全流程：

- **`.166` 结构核对**：scp `.166` xochitl（md5 `5215ef7ab…`）→ `extract_qml.py` 解 556 QML → 认领 `KeyboardPanel.qml`，`#screenMode`（id/objectName:"keyboard"/mode:Animation/anchors.fill/所在 keyboardContainer）与 `.164` **逐行一致**，预研选择器直接适用。
- **qmldiff 离线 apply-diffs（关键：双 qmd 共存）**：`candidatebar.qmd` + `keyboard-mono.qmd` **一起** apply 到 `.166` 真实 `KeyboardPanel.qml`（顺序同设备字母序，candidatebar 先）→ `2 diff(s) applied`、emit 里 `cjCandidateBar` id（candidatebar 注入）与 `#screenMode.mode` 引用**共存且合法**、`INSERT{}` 注释无 `/* */` 污染。两 qmd 同改一文件的合并风险离线排除。
- **真机部署**：scp 到 qt-resource-rebuilder exthome（**XDG 核查**：qmd 归第三方扩展 exthome，`cangjie-ime` 的 XDG data 目录不含 qmd，符合布局）→ 重启健康检查绿（active/MainPID 变化/NRestarts=0）、`.so` 注入仍在 → journalctl `Loading file keyboard-mono.qmd` + `Processing … KeyboardPanel.qml` **无解析错误**。
- **功能验证（`KBD-MONO` 日志硬证 mode 真在切）**：用户打字触发键盘/组词，日志证 **mode 精确跟随组词态**——`candbarVisible=true`（组词）→ `mode=1`（Mono）、`false`（收起/提交）→ `mode=2`（Animation），两个完整来回。`onModeChanged` 绑定正确响应候选栏可见性。

**已并入 `deploy/install.sh`**（reading-qol qmd 同段）+ 重打包 `dist/cangjie-ime-installer.tar.gz`。**可观测性诚实标注**：候选栏纯黑白，Mono 化不改静态观感，"快多少"需 A/B 慢动作对比、收益可能微妙——但 `mode` 属性层面机制已由日志客观证实工作。回退：删 `keyboard-mono.qmd` 重启即恢复（独立文件、不碰 `.so`/candidatebar）。

> 备注：rmtool 的 QMD 是 GPL-3.0，不可直接搬运；机制（属性名/组件名）是公开事实，本项目若采用应参照机制**自行编写** QMD，沿用 06/10 节"数据/补丁独立文件、不编译进 `.so`"的隔离取舍。

## 09｜里程碑与时间表

下面的周期是我**第一次做这类嵌入式逆向项目**时的现实预估（业余时间投入，非全职），不是"理想情况下最快能做完"的乐观估计——真实进度大概率会更慢。M0-M2（UI 汉化）已经落地，见姊妹文档；这里只列 M3 及以后（键盘输入法这条线）。

### M3 · 自定义虚拟键盘能弹出 ✓ 已完成

验收：xovi hook 打通按键处理入口，复用现有英文 QWERTY 布局输入中文（不需要自绘新键盘）——02 节确认 hook 点是 `FUN_00697610`，按键文本偏移全部真机验证；候选栏组件从早期复用的 `KeyPopup` 迁移为 Step BB 注入的自定义 `CjCandidateBar`（原因见 02 节"候选栏叠影排查"）；语言切换器里新增"简体中文"/"繁体中文"两项且键盘布局正常渲染（含一次真机崩溃事故+一次布局消失事故的定位与修复，见 02 节）；拼音缓冲拦截逻辑真机确认：切中文打字不直接落字、候选栏随打字刷新、退格/空格/回车行为正确、切回英文完全恢复原生。

### M4 · 拼音候选可用 ✓ 已完成

验收：键入拼音能看到候选字列表，点选后正确插入汉字——已在真机全链路验证通过（02 节 Step V/V-2/W，候选栏载体后续由 Step BB 的 `CjCandidateBar` 取代 `KeyPopup`，分页/翻页机制随之简化为单行+展开按钮，见 02 节）：词典（雾凇拼音 rime-ice，8105+41448 字表，58.9 万条候选）真正接入 `cangjie-langhook`，候选栏显示的是真汉字而不是拼音音节；候选栏下方新增预览行实时显示当前输入的拼音（双拼下自动转换成标准拼音展示，见 02 节 Step EE）。

**Phase A 起进一步补成真正 iOS 式"逐字可选、中途可改"**（见 02 节 Phase A，全拼/简拼/双拼三种模式真机验证通过）：打 `shenme` 候选栏同时给出"什么"和"神/什/深…"首音节单字（A-i 前缀候选）；点单字只提交它、剩下的音节重新切分继续选（A-ii 分段提交，候选点选改成传候选**索引**、C 端据此按消耗音节数裁缓冲区——补上了此前"点选无索引式对接"这个已知限制）；退格从右往左剥整个 composition，缓冲区空时撤销上一个已选字并还原它的拼音重选（A-iii 退格撤销/中途改字）。

**如实标注仍未做的**：简拼已走完 Python 原型→C 移植→接入设备→真机验证全流程（见 07 节，含"繁体模式下简拼候选依然是简体字""m/n 两个字母查不到简拼"两个真机 bug 的定位与修复）；**中英混输**——早前 Step FF 的"原始字母候选"在 Phase A A4 移除后，**Phase C 已完成词典驱动的 iOS 式前缀补全并真机验证**（见 07 节 Phase C）：英文候选背后是设备端 SymSpell 词典（`english.bin`，复用 `dict.bin` 同格式），打 `nihao`→选"你好"→打 `hello`→点英文候选得"你好hello"，全拼/简拼/双拼三模式统一；仅"连打 `nihaohello` 一次成句"的联合切分仍后置。

### M5 · 全局可用 + 原生入口 ◑ 核心已达成，收尾中

验收：不再是独立测试模块，笔记搜索框等真实场景里都能正常唤出并使用。**核心已达成**：hook 打在系统级 `VirtualKeyboard` 上（`LD_PRELOAD` 注入），天然全局；M3/M4 全程在真实搜索框/笔记输入场景里直接验证（不是独立测试模块）。**原生入口**走的是键盘自带的语言切换弹层（点地球选中文，Phase B），不是 Settings App 里单开一页——那条"接入 Settings App"是早期计划措辞，被键盘语言弹层这条更原生的路线取代、不需要做（这也是本条标题从"接入原生 Settings"改过来的原因）。**真正还剩的**只是长期日常使用的打磨：极端场景兜底、性能优化，跟 M7 高度重叠，暂未单独动手。

### M6 · 双拼（小鹤）+ 繁体支持 ✓ 已完成

验收：双拼固定为小鹤双拼，整体开关（范围本轮收窄，不做方案选择菜单，见 04 节）；繁体模式下候选栏本身显示繁体字（生成阶段离线转换双 blob，见 05 节设计修订），跟简体模式功能完全对等——**繁体部分**：`dict.zh_tw.bin`/`dict_jianpin.zh_tw.bin` 都已部署到设备、`cj_dict_select_handle_for_mode()`/`cj_jianpin_select_handle()` 接入 `cangjie-langhook`，切简体/繁体候选栏（含全拼候选和简拼补充候选）都正确显示对应字形，切换不卡死，见 05/07 节；**双拼部分现已完成 C 移植+接入设备+真机验证**（见 02 节 Step Y）：穷举 26×26 全部按键组合生成静态查找表、差分测试 682/682 逐字节一致，接入方式是把双拼按键原文先转换成标准拼音字符串再喂给下游同一套引擎（跟 04 节设计的"不需要单独引擎"完全对应）；开关机制已随 Phase B 从"标记文件+需重启"升级为语言弹层实时切换（四模式简体全拼/繁體全拼/简体双拼/繁體双拼编码进伪语言代码追加进 `languages`，点地球从原生弹层选、切换立即生效免重启，见 02 节 Phase B）；真机测试中额外修复了 pending 分支贪心拼句缺口（Step CC）和双拼单敲声母键显示真实声母候选（Step DD），见 02 节。

### M7 · 打磨与长期维护 ◑ 进行中（.166 韧性重构已落地）

验收：性能优化、bug 收尾；简拼已接入设备并真机验证；逐字候选/分段提交/退格撤销（Phase A）三种模式真机验证通过；中英混输（Phase C，增量式词典补全）已接入设备真机验证，仅联合连打切分后置；**"每次固件更新后先跑一遍兼容性检查"这条已经从习惯变成实战**：设备 OTA 从 3.28.0.164 升到 **3.28.0.166**，各 hook 目标函数地址整体位移（+0x240/−0xc0/−0x220/−0x340 各不相同），旧的"一个锚点 + .164 固定偏移 + `offset==0x513c70` 版本门"整体进 safe mode、中文全丢——据此做了韧性重构（见下方），已真机恢复。本质仍是持续性质。

#### ✅ 韧性重构（.166 适配，已真机验证）——从"一个锚点推全部"改成"每个目标各自特征码自定位"

根因：旧设计只扫 `setLanguageCode` 一条锚点算出模块基址，其余 8 个函数 + 2 个 metaobject 全用"锚点 + 在 .164 上测得的固定偏移"推地址，还用 `offset==0x513c70` 这道版本门卡"是不是 .164"。OTA 到 .166 后各函数位移幅度不一致，从单一锚点推出来的地址全错，版本门也不再匹配 → 整个扩展 safe mode，输入法和界面中文一起消失。

改法：① 删掉版本门；② 9 个代码目标各自用自己 40 字节函数体 prologue 在运行期扫描自定位（函数搬家自动适配），其中退格 `FUN_00695820`、按键分发 `FUN_00697610` 体内各有一条 `BL` 相对跳转、callee 重定位会让这 4 字节变化，用逐字节掩码通配掉即跨版本唯一命中；③ 2 个 metaobject 是数据、没有 prologue 可扫，但其结构体 +0x18 字段是 `static_metacall` 函数指针（非 PIE、绝对值直接存盘），而那个函数已被特征码定位——于是扫镜像里"唯一等于该函数地址的 8 字节指针"、减 0x18 即 metaobject，完全版本无关。每个目标独立判定：某个没唯一命中只跳过它自己（safe mode），核心输入不受单个非核心目标缺失影响。

上机前的安全门：全部特征码 + metaobject 定位在 `.164`/`.166` 两版二进制离线对拍确认唯一命中，且真机每个运行期地址与离线预测逐一精确吻合（9 代码目标 + VK/EF 两个 metaobject 全部命中，无一失败）。界面汉化则是把 `reMarkable_zh_{CN,TW,HK}.qm` 放回 xochitl 原生翻译目录 `/usr/share/remarkable/xochitl/translations/`，复用它自己的 `setLanguageCode` 加载流程（该函数正是主 hook）。**关于"固件无关"的诚实边界**：真正的 OTA-proof 做不到——让 `.so` 被加载的 `LD_PRELOAD` drop-in 在 rootfs（`/usr/lib`），OTA 必冲，界面翻译（`/usr/share`）同理；届时无论如何都要重跑 `install.sh`。韧性重构真正省掉的是"为新固件版本重新反编译推导偏移"——只要函数体没大改，同一份 `.so` 直接在新版本上自定位生效。

> 源码：`xovi-extensions/cangjie-langhook/src/pattern.c`（`cj_*_masked` 逐字节掩码）、`src/hook_init.c`（`cj_resolve_target`/`cj_find_metaobject` + `cj_hook_init` 各目标自定位）；离线对拍脚本 `verify_patterns.py`/`find_metaobjects2.py`

#### ✅ M7 续：整合进 vellum 的 xovi 体系 + 持久化重构（2026-08-15，新固件真机验证通过）

设备后来装了 **vellum**（apk 包管理 + oxide 生态），它用官方 xovi 机制（`/etc/00-xovi.conf` preload `xovi.so` → 扫 `extensions.d/` 自动加载扩展）。原来 cangjie 靠独立 systemd drop-in 手动 `LD_PRELOAD` 注入，跟 vellum 互相覆盖；加上固件又更新到 `20260806095513`，系统分区回滚导致输入法失效。据此重构，均真机验证通过：

- **cangjie 重构成合规 xovi 扩展**：写最小 `cangjie-langhook.xovi`（只 version，不 import/export/override——内部仍是自定义 trampoline）+ xovigen 生成元数据胶水；`cj_hook_init` 从 `__attribute__((constructor))` 改由导出的 `_xovi_construct` 调用；fail-safe 从外部 `precheck.sh` 改成 `.so` 内建的 `_xovi_shouldLoad`（扫 setLanguageCode 特征码兼容才加载、否则裸启原生）。放 `xovi/extensions.d/`（/home 分区，**固件回滚/重启不丢**）被 vellum 的 xovi 自动加载、跟 qt-resource-rebuilder 并列。新固件上 9 个特征码全命中，hook 逻辑不需重反编译。
- **持久化重构**：xovi 启动配置从 vellum 那份易失的 `/etc/00-xovi.conf`（overlay，普通重启就清）改放自己的 `/usr/lib/.../zz-cangjie-xovi.conf`（ext4 rootfs，**普通重启不丢**，只固件 OTA 冲）。含 `LD_PRELOAD=xovi.so` + `XOVI_ROOT` + **`QML_DISABLE_DISK_CACHE=1` 等 QML env**——最后这几个是 qt-resource-rebuilder 硬需求，缺了它自检 abort 拖垮 xochitl（排错时一度误判成 cangjie 崩溃，靠 qtrr 日志才定位）。实测删掉 vellum 的 /etc 00-xovi 后靠 /usr/lib drop-in 独立支撑。

**层次**：cangjie 扩展（/home）OTA 不丢；xovi 启动配置 + 翻译（rootfs）普通重启不丢、OTA 冲 → 重跑 `install.sh` 恢复。install.sh/uninstall.sh 已重写为新架构、真机一键跑通。**留作后续**：设置 App「屏幕键盘」的中文入口（地球弹层已能切 4 模式，这是锦上添花）。**→ 已用纯 QMLDiff 解决（`settings-keyboard-zh.qmd`），进展见下方「M7 续²」①**：先绕了一段弯路——误判"QMLDiff 不穿透 Component、需 C hook"去写方案 D，后经离线实证翻案（qmldiff 其实能穿透 `Component{}`、方案 D 那条 metacall 路本就拦不到），最终一个 QMLDiff 补丁搞定，离线 apply-diffs 门槛已过、真机待验；这段弯路同时牵出输入法架构可重定向 + dm-verity 铁律。

> 源码：`chinese-ime/langhook/`（`cangjie-langhook.xovi` + hook_init.c 末尾 `_xovi_construct`/`_xovi_shouldLoad` + Makefile xovigen 步骤）、`deploy/install.sh`（extensions.d 部署 + /usr/lib drop-in + QML env）。

#### ⚠️ M7 续²：设置页中文入口 → 变砖事故 → 输入法架构重定向探索（2026-08-16，均未完成落地，但关键认知已钉死）

上一小节末尾那条"留作后续（设置页屏幕键盘中文入口）"真动手后，一条线牵出三件大事——从一个锦上添花的设置项，意外摸清了整个输入法**可以不用逆向 hack** 的架构，也用变砖的代价踩实了 **dm-verity** 这条最硬的部署约束。三件事都**未完成落地**，但结论全部沉淀进记忆（`qt-im-plugin-direction.md`、`device-boot-dependency-brick.md`），此处存档。

**① 设置页「屏幕键盘」中文入口——先走逆向+方案 D（后判死），最终纯 QMLDiff 解决（离线门槛过 + 真机验证通过）**

- 真实 QML（`device/view/settings/LanguageAndKeyboard.qml` 的 `keyboardDialog`，从当前固件二进制 zstd 解出核实）：`model: KeyboardSettings.availableLayouts` + `display: qsTr(languageSettings.languageName(modelData))`。前者不含中文伪代码 → 缺项；后者被 QLocale 泛化成 "Chinese"。QMLDiff 不穿透 `Component{}` 走不通，落 C hook。
- 逆向逐字节钉死（写 Python metaobject 解析器，用已验证锚点 `availableLanguageCodes`=prop0 / `nativeLanguageName`=m1 / `getLanguageDisplayName`=m3 自校验）：`availableLayouts` = `KeyboardSettingsAttached` 的 property id **1**（QStringList）；`languageName` = `LanguageSettings` 的 method id **2**（走已 hook 的 `FUN_009174d0`，加个 case 即可让 zh\_\* → "中文"）。
- **SEGV 教训（Step S 重演）**：第一版照 Step EF `static_metacall` 模板用 `patch_target` 打 `KeyboardSettingsAttached::qt_static_metacall`，真机 `status=11/SEGV` 崩溃循环。根因——trampoline（`make_call_through_stub`）直接 memcpy 目标前 20 字节且**不重定位 PC 相对指令**，而 KBS 序言 offset `0x0C` 处就是 `cbnz w1`（EF 的 cbnz 在 `0x14`、20 字节之外才安全）。改用**方案 D：改 metaobject 结构体 +0x18 的 `static_metacall` 函数指针**指向 wrapper（不碰任何指令字节、wrapper 用绝对地址直调原函数，PC 相对指令原地正常），三次上机零 SEGV。这是"形状看着像不等于对"的又一活教材：新的写内存场景，形状再眼熟也先过一遍只读验证。

- **【2026-08-16 结论翻案：方案 D 整条路是弯路，纯 QMLDiff 才是正解】** 后续两处离线实证推翻了上面"QMLDiff 不穿透 `Component{}` 走不通 → 落 C hook（方案 D）"的前提，方案 D 的逆向、SEGV 修复、metaobject patch 全部可以不要：
  - **① 方案 D 即便不崩也无效**：`CJ_KBS_DIAG` 诊断（打日志过 `KeyboardSettingsAttached::static_metacall` 的 call_type/id）显示设置页读 `availableLayouts` 时**只有 call_type=2/5、从不触发 ReadProperty**——QML 编译后的绑定走直接 getter、绕过 metacall，所以在 static_metacall 里拦 ReadProperty 追加 zh_CN **永远拦不到**。方案 D 判死。
  - **② "QMLDiff 不穿透 `Component{}`"是错的**：核对 `asivery/qmldiff` 源码——大写 `Component { }` 在 qmldiff 里就是 `name=="Component"` 的**普通 Object 节点**，遍历/匹配代码对它无任何特判（`parser.rs` 走 `parse_simple_assignment`、`processor.rs` 的 `locate_in_tree` 对所有 `ObjectChild::Object` 一视同仁）。当初的错误判断很可能是把大写对象 `Component{}` 与 Qt6 小写内联组件声明 `component Name: Base{}`（那个才是 qmldiff 里的特殊变体 `ObjectChild::Component`）搞混了。**真正的坑不是 Component、而是"非通配选择器只匹配直接子节点、不递归"**：要下潜到深埋的对象得用通配 `?#id`（递归全树搜索）或写全 `>` 路径。
  - **正解 = 纯 QMLDiff 补丁 `settings-keyboard-zh.qmd`**（走 candidatebar 同管线，不改 C、不动 xovi 扩展）：`AFFECT /qml/device/view/settings/LanguageAndKeyboard.qml` → `TRAVERSE ?#keyboardDialog > SelectionComponent` → 三个 `REPLACE`（`model` 追加 `.concat(["zh_CN"])`；`display` 对 `zh_*` 返回 "中文"；`selectedIndex` 把任意 `zh_*` 归一高亮到 zh_CN 项）。运行时安全性无需担心：`onSelected` 就是 `Settings.keyboardLanguage = selection`，而 `Settings.keyboardLanguage` 与 `virtualKeyboard.language` 同底层存储（xochitl.conf），**地球弹层已经在把它设成 zh_CN_SP 等伪码且整套输入法真机正常工作**——zh_CN 无对应 `:/misc/keyboards/` 布局时回退默认 QWERTY，正是拼音所需，故**不需要**往 QRC 注入任何布局资源（一度以为要注入，属过度设计）。
  - **AFFECT 路径逐字节核实**：解析 xochitl 3.28.0.169 二进制的 Qt 资源 name 表（UTF-16BE）+ struct 树，确认 `LanguageAndKeyboard.qml` 在 `/qml/` qrc 根下 `device/view/settings/`（与真机验证过的 tap-page-turn `/qml/device/view/documentview/…` 同根同规约、`documentview` 与 `settings` 是 `view` 下兄弟目录）。
  - **离线门槛通过**：clone+build `asivery/qmldiff`，把提取的真实 QML 放到 AFFECT 路径 `apply-diffs` 实跑——`1 diff(s) applied`、退出码 0；输出核对：**只有 keyboardDialog 被改**（另 3 个 SelectionComponent 原样未动），三处绑定正确 emit、`onSelected` 保留、是合法 QML。`install.sh`/`uninstall.sh` 已把 `settings-keyboard-zh.qmd` 纳入部署/清理。
  - **真机验证通过（2026-08-16）**：设备此时已非裸机（item3 恢复后 xovi+cangjie+qrr 全在、`.so`=`345d5100…`），故无需重装——只 `scp settings-keyboard-zh.qmd` 到 qrr `exthome/` 补上差量。安全部署：先按 的隔离手动拉起（复刻 xovi 环境 `systemctl stop`→`setsid env -i … LD_PRELOAD=xovi.so XOVI_ROOT=… /usr/bin/xochitl --system`）冒烟——qrr 无 qmldiff 解析报错、cangjie+qrr 各 4 段、xochitl 健康起来（**先隔离验证再交 systemd，避开 §706 的 `OnFailure=emergency` 崩溃链**）→ 杀隔离实例 → `systemctl reset-failed && start`（`is-active=active`/`NRestarts=0`/4+4 段）。**真机确认：设置→语言和键盘→屏幕键盘 列表出现"中文"项**（用户目视确认）。选中/地球切 4 模式/高亮态未逐项复测，但选"中文"仅 `Settings.keyboardLanguage="zh_CN"`、与地球弹层已工作的 `zh_CN_SP` 同底层，高置信可用。→ 至此该功能离线+真机全绿，可宣称完成。

**② 变砖事故 + dm-verity 铁律（血换的部署约束，红线）**

- 为修"重启后 `xovi.so`（在加密 `/home`）preload 失败"的竞态，给 `xochitl.service` drop-in 加了 `After=home.mount` + `Wants=home.mount`。结果 xochitl **卡等加密盘挂载超时 → `OnFailure=emergency.target` + 看门狗 → 启动失败重启循环变砖**。更致命的是回退死锁：dropbear 的 SSH host key bind-mount 自 `/home`，`/home` 没挂载 → dropbear 起不来 → SSH 全死 → **回退那个坏文件恰恰需要 SSH**。靠 recovery mode（开机态长按电源 25-30s → 松 1-2s → 单按 → 松，USB 出 `2edd:0140`）+ 官方 Linux 工具 `rm_recover restore`（`device-recovery.cloud.remarkable.com`，重刷系统分区**保用户数据**、`reset` 才清数据）救回。**红线：绝不轻易改核心服务（xochitl）的启动依赖，尤其涉及加密/延迟挂载单元；改前必纸面推演失败场景 + 回退路径的可达性（这里回退依赖 SSH、而启动失败正让 SSH 不可用，是死锁）。**
- **dm-verity 铁律（真机两次踩实）**：rootfs（`/dev/mmcblk0p2` ext4 ro）+ persist 分区都是 **dm-verity 完整性保护**（启动日志 `Reached target Local Verity Protected Volumes`）。**任何对 `/usr` 的写入（改本体 xochitl，或新增插件 / hook.so / drop-in——新增文件也改文件系统元数据块）+ 完整设备重启 → verity root hash 变 → 校验失败 → A/B slot 回滚**（两次：patch 本体、往 /usr/lib 放插件+hook+drop-in，都回滚到 restore 的 3.27.1.0 slot）。**假象**：`remount rw /usr` 当次可写、进程级 `systemctl restart xochitl` 保留改动（曾误判"新增文件 verity 放行"），但**完整设备重启就回滚**。→ **铁律：持久化定制只能放 `/home`（加密、非 verity、重启不丢，但挂载晚有竞态），绝不放 `/usr`。** 这也修正了本文前面（M7 续）"/usr/lib drop-in 普通重启不丢"的说法——那只在"进程级 restart / 不触发完整重启"时成立，完整重启会 verity 回滚。

**③ 输入法架构重定向：标准 Qt IM 插件替代逆向 hook（方向坐实，注入最后一环待落地）**

- **关键发现**：反查当前固件 xochitl，**它的输入法本身就是标准 Qt6 `platforminputcontext` 插件**——启动时执行 `qputenv("QT_IM_MODULES", "xochitl")`（`QT_IM_MODULES` 复数；全二进制**唯一**一处引用 @vaddr `0x532c08`，值串 "xochitl" @fileoff `0x106d3e8`），Qt 据此加载 `Keys=["xochitl"]` 的插件。**不是绕过 Qt IM，而是用它。**
- **意味着可以**：写标准 Qt6 `platforminputcontext` 插件（拼音引擎，已有 C 移植）+ **hook 这 1 处 qputenv**（LD_PRELOAD wrap 导出符号 `_Z7qputenvPKc14QByteArrayView`，把值改成让 cangjie 生效）+ **装饰器**包原生 IC（`QT_IM_MODULES` 按 `;` 分割、`create(QStringList)` 返回第一个有效的，故共存需我们的 `CangjieInputContext` 内部 `create("xochitl")` 拿原生 IC 转发、保留 xochitl 键盘）。**hook 面积从现有 6+ 处特征码 → 1 处，引擎逻辑固件无关、不碰本体不变砖。**
- **真机验证进度（诚实）**：插件被 Qt 扫到、读 metadata、`Keys` 匹配、版本兼容性可控（设备 Qt 6.10.3；本机 Qt 6.11 编译后 hex patch metadata `06 0b`→`06 0a` 降为 6.10 即过版本门），qputenv hook 也编好并验证 systemd 能注入 LD_PRELOAD。**但注入最后一环（`QInputMethodEvent` 实际把中文送进文本框）没跑通**——不是方案错，是 hook/插件放在 `/usr` 被 ② 的 verity 回滚清掉了。要落地须放 `/home` + 走 vellum/xovi 或自建等价的"/home 持久 + 引导"机制。
- **由此回答"是否还需要 vellum"**：**需要**。vellum/xovi 最根本的价值**不是 QML 增强，而是绕开 dm-verity 做持久化**（`xovi.so` + `extensions.d` 放 /home 持久，`00-xovi` 引导配置放 /etc 靠 `vellum reenable` 恢复）。新方案的插件/hook 同样必须放 /home、同样面临"持久引导入口放哪"（/usr 回滚、/etc overlay 清）这个 verity 架构固有难题，故绕不开 vellum 或等价机制。这也终于说清了本项目当初选 vellum 的根本理由。

> Qt6 语义（分隔符 `;`、`create(QStringList)` 返回首个有效项、`EpaperIntegration::inputContext()` 返回构造时缓存的成员而 `initialize()` 里调 `QPlatformInputContextFactory::create()`）均反汇编自设备 `libQt6Gui.so.6.10.3`。恢复常识：`rm_recover restore` 保数据、`reset` 清数据；开发者模式 SSH 密码 recovery 后会重置（GPLv3 页显示的可能是旧值，以退出重进后的实时值为准）。

### 当前实际进度

M3/M4/M6 已经真机全链路验证通过，键盘输入法这条线不再是"桌面离屏验证过设计思路"的阶段——语言切换器接入、按键拦截、拼音/双拼缓冲、真汉字候选、繁体双 blob、简拼、候选栏显示时序问题全部在真机上跑通并反复验证过。候选栏组件本身经历过一次架构级替换：早期复用原生 `KeyPopup`（长按重音弹窗）能跑通基础读写，但双拼真机测试暴露出候选快速变化时的视觉叠影，四轮修复尝试（显示节流/缩小分页/动态撑宽/直接改尺寸）均未解决根因，最终判定是组件本身的 `Repeater` 重建机制不适合做常驻候选条——Step BB 改为安装 `xovi`+`qt-resource-rebuilder` 扩展，用 QMLDiff 语言注入一个全新的 `CjCandidateBar` 组件，彻底解决叠影问题，顺带把候选展示从"KeyPopup 分页+点击翻页"改成参照 iOS 拼音候选栏的"单行紧凑+展开按钮"交互，并新增候选栏下方的拼音预览行（Step EE）。这轮真机测试还顺手挖出并修复了一个 Step V 遗留的真实 bug（回车被误合并成"确认候选"，见 02 节真机 bug 4）。

双拼（04 节）现已完成 C 移植+接入设备+真机验证（Step Y），过程中额外修复了 pending 分支贪心拼句和单敲声母键显示真实声母候选两个问题（Step CC/DD）；双拼开关已随 Phase B 做成语言弹层实时切换——4 个中文模式（简体全拼/繁體全拼/简体双拼/繁體双拼）编码进伪语言代码追加进 `languages`，点地球从原生弹层里选、切换立即生效免重启（02 节 Phase B），标记文件机制已删除。07 节的简拼已经接入设备并真机验证通过（全拼路径优先、简拼补位填充剩余候选槽位；过程中发现并修复了"繁体模式下简拼候选依然是简体字""m/n 两个字母查不到简拼候选"两个真机 bug），候选逻辑随后按我对照 iOS 的要求做成真正的逐字输入法（Phase A，02 节）：前缀候选让首音节单字可选、分段提交让"点第一个字→剩下继续选"、退格撤销让长句中途能改字，全拼/简拼/双拼三种模式真机验证通过。

中英混输早前 Step FF 的可点英文候选在 Phase A A4 移除（候选栏改 iOS 干净版）后，Phase C 已完成词典驱动的 iOS 式前缀补全并真机验证（`english.bin`=SymSpell 82k 词频表、复用 `dict.bin` 格式，打 `nihao`→选你好→`hello`→点英文候选得"你好hello"，三模式统一；仅连打 `nihaohello` 一次成句的联合切分后置），同批修掉了 Step W 遗留的"停顿后按空格丢字"（去掉 10 秒空闲静默清空）；并修复了我真机报的"简体模式出繁体候选"——iorest 128 万繁体词条经 8105 白名单守卫的字符级归一化转简（保证不改坏 `坏`/`夥` 等合法简体字），详见 06/07 节。

M5 的核心（全局可用 + 原生入口）已经达成——hook 打在系统级 `VirtualKeyboard` 上天然全局，M3/M4 全程在真实搜索框/笔记场景验证，原生入口走键盘语言弹层（Phase B）而非 Settings App 单开页；剩下的只是长期日常使用的打磨（兜底/性能），跟 M7 高度重叠、不强行区分先后，尚未单独动手。

> **拆分记录**：这份文档从原本合并在《reMarkable中文化白皮书》里的第 9 节拆分而来——UI 汉化那份文档曾评估过"暂缓拆分，等 M3/M4 键盘 hook 真正开始产生真机调试内容之后再拆"，现在触发条件已经出现（这条线的真机调试记录篇幅远超 UI 汉化部分，且两边后续各自独立迭代、互不阻塞），拆分已完成，即本文档。

## 10｜风险清单与合规提醒

变砖/数据风险、重启节流配额、逆向工程的地区法律差异、xovi 本身的 LGPL v3 许可这些跟 UI 汉化那条线共用的通用风险，见姊妹文档《reMarkable中文化白皮书》第 06 节，此处不重复，只列拼音输入法这条线特有的许可证问题。

- **词库许可证，三批数据分属两种协议，不要混为一谈**：`rime-luna-pinyin` + `rime-essay-simp`（06 节候选词库，早期版本）是 **LGPL v3**；`rime-double-pinyin`（04 节双拼按键映射）、`rime-ice`（雾凇拼音，06 节候选词库现用数据源）都是 **GPL v3**——都实测下载 LICENSE 文件确认过。保留来源、协议声明和原始 `LICENSE`/`AUTHORS` 文件（`rime-ice` 上游没有独立 `AUTHORS` 文件，归属信息保留在数据文件自身的注释头里，见 `pinyin-engine/data/PROVENANCE.rime-ice.md`）；来源不明的词库导出文件默认"保留所有权利"、授权状态不明，一般不建议用于会对外分发的场景——`Iorest/rime-dict`（无 LICENSE、`license: null`、出处未交代）就是这类；本项目后来按我"个人设备自用、不对外分发"的明确判断纳入了它（用 pypinyin 离线注音、数据不编译进 `.so`，只记录事实与取舍、不代替法律意见，见 06 节 + `pinyin-engine/data/PROVENANCE.iorest.md`）。
- **GPL v3 数据的 copyleft 应对**：GPL/LGPL v3 都对"修改后再分发"有源码可获取的要求。`rime-double-pinyin`（双拼 schema）的应对是"参照规则重新用 C 实现，不链接 GPL 代码本身"；`rime-ice` 词典数据的应对是"改成独立文件运行时 mmap、不编译进 `cangjie-langhook.so` 本体"（架构设计见 06 节 + `pinyin-engine/c/src/dictionary.h`）——两种应对方式针对的风险点不同（前者是"代码链接"，后者是"数据分发"），如果以后要把裁剪/组装过的词典数据单独发布，建议重新过一遍条款细则，本文不代替法律意见。
- **字体许可**：思源黑体/Noto Sans CJK 候选栏渲染用到的字体是 SIL OFL 1.1，跟姊妹文档 3.3 节 UI 界面用的是同一份协议、同一批裁剪产物，不重复说明。
- **OpenCC 许可（05 节繁体双 blob 设计新增）**：**Apache License 2.0**，比 rime-ice/rime-double-pinyin 的 GPL v3 宽松得多，且只作为开发机生成 `dict.zh_tw.bin` 的离线工具依赖，不进入设备端 `.so` 产物，风险类别是这份清单里最低的一项。
- **xovi + qt-resource-rebuilder 许可（Step BB 候选栏重构新增，02 节）**：候选栏组件从 `KeyPopup` 迁移到自定义 `CjCandidateBar` 之后，注入链路多了两个运行时依赖——`asivery/xovi`（基础扩展框架）是 **LGPL v3**，`asivery/rm-xovi-extensions`（含 `qt-resource-rebuilder`，QMLDiff 补丁的运行时载体）是 **GPL v3**。两者都以独立 `.so` 形式通过同一个 `LD_PRELOAD` 链跟 `cangjie-langhook.so` 一起注入 xochitl 进程，运行时同进程但各自是单独编译的目标文件，不静态链接进彼此——法律定性上更接近 06 节 rime-ice"数据隔离存放、不编译进同一个二进制"的取舍逻辑（不代替法律意见）。`qt-resource-rebuilder` 运行时读取的 QMLDiff 补丁文件（`candidatebar.qmd`）是本项目自己编写的纯文本 DSL 脚本，不包含任何上游代码，只是描述"往哪个 QML 文件的哪个位置插入什么内容"。
- **范围提醒**：跟 UI 汉化那条线一样，本方案自始至终是"让自己的设备能打中文字"，不涉及也不建议扩展到破解付费功能、绕过设备管控这类用途。

### 📌 竞品借鉴（rmkit-cn）·进程内 hook vs 进程外 uinput 的架构取舍，及一条降级容错路径

> 来源：`boangs/rmkit`（"rmkit-cn"，GPL-3.0）`ime-go/injector.go`、`intercept/ime_hook.cpp`。**源码研读结论，未在本项目验证。**

同赛道竞品 rmkit-cn 也做 reMarkable（主测 Paper Pro Move）中文输入法、同样站在 xovi 上，但文本注入走了跟本项目相反的路线，值得记一笔作对照与备用：

| 维度     | rmkit-cn（进程外）                                                                                                                                                                   | 本项目 cang-jie（进程内）                           |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------- |
| 引擎形态 | Go 常驻服务（`ime-go`），systemd 拉起                                                                                                                                                | C 引擎编译进 `cangjie-langhook.so`，随 xochitl 进程 |
| 文本注入 | **uinput 内核级模拟按键**：`injector.go` 用 Ctrl+Shift+U 触发 Unicode 输入，逐个敲候选字的十六进制码点再回车                                                                         | 进程内直接操纵 Qt 文本对象（02 节 hook 链）         |
| 拦截层   | `intercept/ime_hook.cpp`：LD_PRELOAD hook `QInputMethodEvent::setCommitString` 与 `QGuiApplicationPrivate::processKeyEvent`，把字母改道，经 `/tmp` 下 socket/flag 文件与 Go 服务通信 | 直接在按键分发函数内改道（`FUN_00697610` 等）       |
| 候选 UI  | Go 侧 overlay（`overlay.go`，rM2 走 framebuffer；Paper Pro 的 DRM 分支源码里仍是 TODO）                                                                                              | QMLDiff 注入的原生 `CjCandidateBar`                 |

**取舍结论**（置信度：高）：进程外 uinput 路线的优势是**与 xochitl 内部解耦、崩溃隔离、固件升级抗性强**——正是本项目在 .166 上被迫做特征码自定位重构（09 节）付过学费的那类痛，它天然免疫；代价是每个汉字要一长串按键事件（慢）、候选 UI 非原生、且依赖解码端配合。本项目的进程内路线换来的是原生体验、低延迟、无常驻进程，代价就是"崩溃共命运 + 强耦合固件布局"，这也正是本项目"先只读后写内存、一步一确认"工程纪律存在的根本原因。两条路没有绝对优劣，是"延迟/原生度"与"隔离/抗升级"之间的取舍。

**可留存的降级容错路径**：万一未来某次固件大改让进程内 hook 短期无法自定位（09 节的特征码方案覆盖小变动，但不保证大版本跳变），rmkit-cn 的 uinput 注入可作为**临时降级通道**存档——牺牲体验换"至少能把字打进去"。当前不需要实现，仅记录为 Plan B。

### 📌 竞品借鉴（rmkit-cn）·bind-mount 双写解决 /etc overlay 重启丢失 drop-in

> 来源：`boangs/rmkit`（GPL-3.0）`installer/install.sh`、`systemd/zz-rmkit-cn.conf`。**源码研读结论，未在本项目验证。**

本工程纪律 记的痛点"`/etc` 是 overlay、真机重启会清掉 systemd drop-in、扩展静默失效"，rmkit-cn 给了一个工程解法：安装时 `mount --bind / /tmp/lc` 拿到 overlay 的 lowerdir（ext4 持久层），把 drop-in 与 wants symlink **同时写进 `/etc/...`（upperdir/tmpfs）和 `/tmp/lc/etc/...`（lowerdir/ext4）双份**，重启后 lowerdir 那份仍在。这与本项目现在把 drop-in 放 `/usr/lib/systemd/system/xochitl.service.d/`（根分区、避开 overlay）是两种不同规避手段——本项目的做法更简单，但 OTA 必冲；rmkit-cn 的双写扛重启但不扛 OTA。二者都不是 OTA-proof，可按维护偏好择一——**本项目已定选 `/usr/lib` 持久 drop-in（避开 `/etc` overlay）+ precheck symlink 外层防线（4.3 节 fail-safe），不采用 bind-mount 双写**（"择一"悬念据此销掉）；详细的固件升级自愈设计见姊妹文档《reMarkable中文化白皮书》4.3 节的 📌 借鉴块。

## 参考来源

1. [Qt Virtual Keyboard · PinyinIME Attribution](https://doc.qt.io/qt-6/qtvirtualkeyboard-attribution-pinyin.html) —— 02 节方案 A（移植路线）参考的现成拼音引擎来源
2. [rime/rime-luna-pinyin](https://github.com/rime/rime-luna-pinyin) —— 03/06 节拼音引擎早期版本使用的候选词库，LGPL v3，代码仍保留供对比/回退
3. [rime/rime-essay-simp](https://github.com/rime/rime-essay-simp) —— 06 节"常用词组哪来的"用到的 preset vocabulary 频率表（早期版本），LGPL v3
4. [rime/rime-double-pinyin](https://github.com/rime/rime-double-pinyin) —— 04 节双拼引擎实测使用的按键映射 schema（小鹤双拼/自然码），GPL v3
5. [iDvel/rime-ice](https://github.com/iDvel/rime-ice)（雾凇拼音）—— 06 节候选词库现用数据源，持续维护，GPL v3，钉版本 commit `569ff3bc`
6. [asivery/xovi](https://github.com/asivery/xovi) —— 02 节 Step BB 用到的基础扩展框架，LGPL v3
7. [asivery/rm-xovi-extensions](https://github.com/asivery/rm-xovi-extensions)（含 `qt-resource-rebuilder`）—— 02 节 Step BB 用到的 QMLDiff 运行时载体，GPL v3
8. [toltec-dev/toltec](https://github.com/toltec-dev/toltec) —— 06 节磁盘空间调研中核实过目前不支持 Paper Pro Move 的第三方包管理器
9. [vellum-dev/vellum](https://github.com/vellum-dev/vellum) —— 06 节磁盘空间调研中确认支持 `rmppm` 且印证 `/home` 分区独立性的同类工具
10. [koreader/koreader · zh_pinyin_data.lua](https://github.com/koreader/koreader/blob/master/frontend/ui/data/keyboardlayouts/zh_pinyin_data.lua) —— 06 节提到的相邻参考，AGPL 协议，仅供参照数据结构，不可直接搬运代码
11. 《reMarkable Paper Pro Move 中文化开发方案》（姊妹文档）—— xovi hook 基础设施、开发环境搭建、UI 汉化模块、`LanguageSettings` hook 全部内容的出处

### ✅ 架构侦查：真正的虚拟键盘实现是全局类 `VirtualKeyboard`，不需要自己造一套 QWERTY 布局

对解压出的 552 个 QML 文件做关键词扫描（`Keyboard`/`InputPanel`/`inputMethod`），命中 57 个文件，定位到虚拟键盘的真实实现——**模块 `xofm.modules.virtualkeyboard`，QML 类型 `VirtualKeyboard`**，是 xochitl 自己纯 QML + 一个 C++ 后端对象从零实现的，**不是** Qt 官方 `QtQuick.VirtualKeyboard` 模块，也**不是**系统级 `QPlatformInputContext` 输入法插件（原计划要验证的低风险路线，证据上不成立，排除）；键盘弹出/收起走标准 `Qt.inputMethod.show()`/`hide()`/`visible`，跟 Type Folio 物理键盘连接状态联动隐藏。**最有价值的发现**：`virtualKeyboard` 暴露了 `languages`/`language`/`languageName(code)` 这组 API，跟姊妹文档《reMarkable中文化白皮书》4.1 节 `LanguageSettings.availableLanguageCodes`+`getLanguageDisplayName()` 是同一种

### ✅ Step FF · 用户词频自动调节（自适应调频）——真机验证通过（2026-08-22）

**做什么**：记录用户在候选栏实际点选/空格确认的 `(查询key, 词)` 计数，下次输入时把选过的词提前。此前候选排序是纯静态词频（rime-ice 烤进 dict.bin），没有任何随使用调节的机制（2026-08-22 核查确认，连白皮书待办清单里都没有——M6 后可选项只有 bigram/模糊音，跟自适应调频不是一回事）。

**架构（跟静态词频彻底解耦）**：GPL 的 `dict.bin` 一字节不动；用户计数是独立数据、独立文件 `CJ_DATA_DIR/userfreq.tsv`（`count\tkey\tword` 文本，原子写 tmp+rename），查询时在 hook 层融合。融合规则按候选生成的**分组结构**分两种（不做全局重排，不破坏"整词组→中间前缀→首音节单字"的既有布局）：

- **精确 key 组**（`cj_add_prefix_candidates` 各组 / 简拼精确 key / 完整音节贪心拼句）：`cj_uf_words_for_key()` 把该 key 下用户选过的词按 count 降序**强制召回插到组头**（每组最多 `CJ_UF_GROUP_MAX=3` 个，避免挤掉静态头部），天然解决"用户常选词掉出词典 top-k 池"的召回边界；后续词典结果照旧、对组内去重。
- **前缀预测池 / 简拼池**（候选的完整 key 未知）：`cj_uf_order()` 按 **word 聚合计数**（`word_total`，跨 key 求和）做组内**稳定重排**（count 降序、并列保持原序、全 0 恒等）。

**记账点**：`cj_step_t_commit_candidate_by_index`（候选点选与空格确认第一名都走这里）——新增平行数组 `g_step_v_cached_key[]` 记录每个缓存候选的来源 key（英文候选/pending 态拼句 key_len=0 不参与），提交后 `cj_uf_record` + 立刻 `cj_uf_save`。⚠ 两个已识别并处理的坑：① 强制召回的候选 `word` 指针指向 userfreq 内部条目，record 会 memmove——提交路径先拷本地再 record，record 后必经缓存重建；② 英文候选插入的 memmove 必须同步平行 key 数组。

**数据规则全部钉死成确定性**（`src/userfreq.py` docstring 是唯一规格源，C 版逐条对齐）：容量 2048 条、key≤64B、word≤48B、count 饱和 1e6；满员淘汰=最小 count 并列取字节序最大者，新条目以自身 count 参与比较；`words_for_key` 并列按 word 字节序；`order` 稳定排序。**没有任何"容许差异"条款**——不同于 dict/jianpin 差分测试的同权重顺序豁免，userfreq 的差分要求逐字节完全一致。

**验证（全部离线门槛过，真机未上）**：Python 参照 `src/userfreq.py` + 16 单测；C 移植 `c/src/userfreq.c` + 7 组单测；差分 `diff_check_userfreq.py` **5470 条操作全一致**（含灌满 2048 条后的淘汰风暴——规模边界正是 06/07 节两次抓出 bug 的同类位置）；Python 全量 87 测试绿；`make aarch64` 零警告出 `.so`。**真机验证（2026-08-22，固件 3.28）**：部署走两步纪律（先重启验证旧 .so 干净加载，再上新构建；备份进 `/home/root/cangjie-backups/`、md5 核对、PID 换新、NRestarts=0、cangjie 4 段映射）。已验证：① 点选"倪浩"（ni hao 下的靠后候选）→ `userfreq.tsv` 原子写出 `1\tni hao\t倪浩`；② 重打 nihao 用户确认"倪浩"升到组头（强制召回生效）；③ 点第一名候选（你好）同样记账、重复点选计数递增（终态 `你好=1、倪浩=2`，文件按 (key,word) 字节序稳定排列）；④ 用户未报任何提交延迟。**未逐项验证**（低风险留观）：简拼/双拼/繁体模式的记账（同一条 commit 代码路径，仅 key 来源不同）；重启后计数保持（文件在 /home 持久分区，无理由丢失）。

**为何不做衰减/时间因子（这版刻意收窄）**：count 单调递增+饱和已覆盖"常用词恒强"；错选一次的词会被后续正确选择的更高 count 自然压回。留到真机体验后再决定是否要 recency 因子，不预先复杂化。

### ✅ Step GG · 快捷输入（text replacement，iOS 同款）——真机验证通过，含设置页 CRUD 管理界面（2026-08-22）

**做什么**：`CJ_DATA_DIR/snippets.tsv`（每行 `缩写\t短语`）定义"缩写→短语"；原始字母缓冲区**精确等于**缩写时短语注入为**第 0 位候选**（打缩写+空格即上屏，同 iOS）。缩写限小写字母 ≤32B（拼音缓冲区只收字母，别的字符配了也匹配不上，parse 直接拒收）；短语任意单行 UTF-8 ≤256B（地址/邮箱/英文都行）；同缩写多行全注入按文件序；容量 256 条。输入法对文件**只读**+**mtime 热重载**（`cj_sn_refresh` 每次候选刷新 stat 一次，变了才重解析——host 改完下一次打字即生效，不重启）。注入点在 refresh 尾部（英文候选之后）整体前插，四个平行缓存数组同步右移；`key_len=0` 不参与 Step FF 调频（显式意图无需学习）。与模式无关（匹配的是原始按键字面，同英文候选路径的理由）。

**验证**：Python 参照（`src/snippets.py`，规格唯一源）6 单测 + C（`c/src/snippets.c`）5 组单测（含 mtime 热重载）+ 差分 `diff_check_snippets.py` parse+510 条 lookup 全一致 + `make aarch64` 零警告；部署健康检查过。**真机验证通过（2026-08-22）**：设置页管理界面全链（增/删/改/保存后去输入框打缩写立即出候选=热重载闭环）用户确认正常。

**设置页 CRUD 管理界面（settings-reading-enhance.qmd 扩展，真机通）**：「系统增强→阅读增强」页底部加「快捷输入（文本替换）」入口行 → 二级页（哨兵 990002，`payloadLoader` 早返回再加一条、`cjSnippetsPage` 用独立 TRAVERSE/INSERT 块——不赌 qmldiff 对单 INSERT 多兄弟节点的支持）。二级页=列表（Flickable+Repeater，行内 改/删）+ 新增/编辑子视图（TextInput 字段靠上避键盘、缩写框 onTextChanged 过滤只留小写字母、保存前校验 `^[a-z]{1,32}$` 与短语 ≤256B 无 tab/换行）。数据直读写 `snippets.tsv`：读=同步 GET file://，写=**异步 PUT**（同步 PUT 到 file:// 只截断不写体，reading-qol.json 同一个坑）；`items` 数组每次整体换新（var 子属性变更不触发重算）。三语表（简/繁/英）沿用 qsTranslate 探测配方。**连带修复**：阅读增强页开关涨到 8 项超出一屏且原 Column 不可滚——包一层 `Flickable`（contentHeight=cjCol.height，clip）恢复下滑（2026-08-22 用户报障后真机验证过）。返回键=`root._selectedPage=990001`（Component 内 id 动态作用域解析到 Settings 根，真机成立）。侧栏高亮条件扩到 990001||990002。

### ✅ A-iv · 焦点守卫——"跨输入框的陈旧 composition 泄漏" bug 真机修复（2026-08-22）

**用户报障**：笔记里 `nihao`+空格上屏→关笔记→搜索框按退格→候选栏弹回"你好"。**根因**：拼音缓冲区+撤销栈（A-iii"空格后可退格撤回"特性）是全局单例，finalize 只覆盖"外部 insertText/再次产字键"，**漏了焦点切换**——陈旧栈条目在新输入框被退格弹出，除了显示错乱还会**往新输入框发删码元**（比表象更危险）。

**修法（两层）**：① C 端 `cj_ime_focus_guard()`——写缓冲区/压撤销栈时记录 `QGuiApplication::focusObject()`（dlsym 公开符号；⚠ 不能复用 `CJ_UI_TRANSLATION` 条件块里那份解析，该宏不开时整块编译不进来，独立惰性解析），退格与按键分发两个入口先比对焦点，变了整体 finalize（清缓冲区+清栈+收候选栏）、按键放行原生；fo==NULL 按"变了"处理（宁可少一次撤销不冒删错框的险）。② QML 端 candidatebar.qmd 加 `Connections{target: Qt.inputMethod}`——键盘隐藏即自清（收栏+清 keys/preview），补上"焦点切走时没有按键、C 端没机会收栏"的显示残留。

**真机验证（4/4）**：①原 bug 路径退格转原生不再弹旧候选；②同框撤销（A-iii）未误伤；③逐字造句未误伤（坐实**点候选栏不偷焦点**——守卫依赖的这个前提成立）；④半截拼音不再跨框泄漏（状态+显示都干净）。qmd 语法防线用设备 `/usr/lib/qt6/bin/qmllint` 独立包装校验（host qmldiff 工具链已不在，明文 qmd 失配只 log 不崩仍是兜底）。

### ✅ Step FF v2 · recency 近因因子（2026-08-22，真机迁移验证通过）

**为什么**：纯累计计数让旧习惯永远压住新习惯（选过 100 次的词要被追平才让位）。**设计（全整数、确定性、可差分——不用墙钟）**：逻辑时钟 `gen`（每次成功记账 +1）+ 每条目 `last`（最后使用时的 gen），排序/召回/淘汰一律用**有效分** `effective = count >> min((gen−last)/HALF_LIFE=128, 31)`——每闲置 128 次全局选择影响力减半；命中时"触碰即结算"（count=衰减后有效分+n、last=gen）。`cj_uf_count` 保留原始存储值（诊断），新增 `cj_uf_effective`；**hook 层 API 形状不变、一行不用改**。

**文件格式 v2**：首行 `#gen\t<gen>`，条目行 `count\tkey\tword\tlast`。**迁移矩阵**：新读旧（3 字段）=last=0、gen=0 → 不衰减、计数全额保留（真机验证：5 条 v1 数据无损读入，首次记账后落盘 v2）；旧读新=头部与 4 字段行都被 v1 解析器跳过 → 空表（回滚丢学习数据、不崩，v1 数据有备份 `userfreq.tsv.bak.v1`）。

**验证**：规格唯一源仍是 `src/userfreq.py` docstring；Python 19 测 + C 7 组测 + 差分 5470 条（新增 E 有效分操作、D dump 含 gen/last）全一致 + 全套引擎差分回归 + 零警告 `.so`；真机迁移+记账+落盘 v2 验证过。**测试坑（记档）**：满员表上用"新 key 打点"推进时钟会被淘汰规则丢弃、gen 根本不走——推进时钟必须**命中已有条目**；铺满员定态用 load 显式 last（绕开 record 推进时钟造成的不均匀衰减），Python/C 两边测试同一手法。
