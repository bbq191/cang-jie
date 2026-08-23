# qt-im-plugin —— 输入法架构重定向（白皮书方向③）离线工程

> 承接《reMarkable拼音输入法白皮书》§709–715「M7 续² · ③」。本目录把那次逆向的 scratch
> 前作（原只活在易失 `/tmp/rmqt`）抢救进仓库，并建立**离线可复现**的交叉编译。
> 现状一句话：**地基与"注入验证桩"就位、离线可编，但决定整个方向成立与否的"命门"仍未真机验证。**

## 0. 这套架构要替代什么

现行 `chinese-ime/langhook/`（6+ 处特征码 hook + trampoline 直接操纵 Qt 文本对象）能用、已真机验证，
但脆弱（固件动一下就要重定位）。方向③改用 **xochitl 自己就在走的标准 Qt6 输入法通道**：

- xochitl 启动执行 `qputenv("QT_IM_MODULES","xochitl")`（全二进制唯一一处 @vaddr `0x532c08`），
  Qt 据此加载 `Keys=["xochitl"]` 的 `platforminputcontext` 插件。**不是绕过 Qt IM，是用它。**
- 于是只需：**① 标准 Qt6 platforminputcontext 插件**（装拼音引擎）+ **② 1 处 hook 把 QT_IM_MODULES 改成我们的插件**。
  hook 面积 6+ → 1，引擎逻辑固件无关，不碰本体、不变砖。

## 1. 目录结构

```
qt-im-plugin/
├── src/
│   ├── cangjieinputcontext.cpp   # ② 插件本体（当前=注入验证桩,见 §3）
│   ├── cangjie.json              #   插件 metadata:{ "Keys": ["cangjie"] }
│   └── qputenv-hook.c            # ① 独立 LD_PRELOAD qputenv 拦截器
├── sdk/                          # 设备 Qt6.10.3 .so（链接用,抢救自 /tmp/rmqt,LGPL 可再分发）
│   ├── libQt6Core.so.6.10.3  libQt6Gui.so.6.10.3  libepaper.so
├── recon/                        # 逆向 scratch dump(create/metaobject 反汇编字节),仅存档
├── tools/patch_metadata.py       # .note.qt.metadata 段内 Qt 版本降版(唯一命中守卫)
└── Makefile                      # moc + 交叉编译 + metadata 降版,零警告
```

`make all` → `libcangjieinputcontextplugin.so`（插件）+ `cangjie-qputenv-hook.so`（hook）。
本机 Qt6.11.1 + `aarch64-linux-gnu-g++ 16.1` + qt6 `moc` 实测零警告通过。

## 2. 已钉死的机制（反汇编/头文件交叉核实，勿凭记忆改）

**qputenv ABI**（反汇编设备 `libQt6Core` @0x10ffd0 对拍 + 头文件双证）：
`qputenv(const char* name=x0, QByteArrayView view)`；6.10.3 `QByteArrayView` 走 `QT_VERSION<7`
分支布局 `{qsizetype m_size; const char* m_data}` → 按值传 **x1=size, x2=const char* data**。
反汇编铁证：`mov x19,x1` 后 `cmp x19,#0xf`（QByteArray SSO 15 字节阈值,只可能是 size）；
`cbz x2`/`mov x20,x2` 把 x2 当 data；`strb wzr,[x0,x19]` 在 data[size] 补 null。→ hook 签名
`char qputenv(const char*, long long size, const char* data)`。

**hook 走独立 LD_PRELOAD .so，不是折进 xovi 扩展**（关键，白皮书 §711 措辞含糊，此处钉死）：
- xovi 扩展是被 `xovi.so` **`dlopen`** 进来的，dlopen 库**不参与已绑定符号的插桩**，拦不到 xochitl
  自己对 qputenv 的调用。→ 必须让 hook 作为**真正 LD_PRELOAD 的独立 .so**（进全局查找作用域、
  优先于 libQt6Core），才能插桩成功。它也因此**不 patch qputenv 序言**——序言 offset `0x08` 是
  `adrp x3`（PC 相对），20 字节 trampoline 搬走即页地址错、SEGV（§703 方案 D 同类雷）。
  符号插桩天然避开这个坑。
- 代价：LD_PRELOAD 列表要加这个 .so（当前只有 xovi.so）。.so 必须放 `/home`（verity 铁律，见 §5）。

**插件 metadata 版本门**（Qt6.11 格式已变，此处更新白皮书旧描述）：
- Qt6.11 把插件 metadata 从旧 `.qtmetadata`/`QTMETADATA !` 魔数**挪进 NOTE 段 `.note.qt.metadata`**。
- 头 4 字节 = `{格式版=01, Qt主=06, Qt次=0b, arch_req}`。`patch_metadata.py` 把 `Qt次 0b→0a` 降到
  设备 6.10（段内唯一命中守卫）。
- 第 4 字节 `arch_req`（`QPluginMetaData::archRequirements`）：aarch64 下 x86 特性位全 0，
  **bit7(0x80)=debug 构建标志**（`#ifndef QT_NO_DEBUG` 才置位）。debug 插件会被设备 release 版 Qt
  的 debug/release 门拒绝。→ **正确做法是 `-DQT_NO_DEBUG` release 编译让它自然=0x00，不是 hex patch**。
- 最终头 `01 06 0a 00`，与设备验证过的前作产物**逐字节一致**。

## 3. 现状：插件是"注入验证桩"，不是真输入法（诚实）

`cangjieinputcontext.cpp` 现在只做一件事：`setFocusObject` 时向焦点对象 `sendEvent` 一个
`QInputMethodEvent`，`commitString="你好"`。它是用来回答**唯一命门问题**的最小探针：

> **标准 Qt IM 通道的 `commitString` 到底能不能把中文送进 xochitl 的 TextInput？**

白皮书 §712 诚实标注这一环"没跑通"——不是方案被证伪，是当时 hook/插件放 `/usr` 被 verity 回滚清掉、
没跑到验证。**在这个"你好"被真机证实能上屏之前，往上叠装饰器/拼音引擎都是建在沙上**，故本轮到此打住、
不盲目往上堆。

**桩还缺三件**（命门验证通过后才做）：
1. **装饰器**：hook 若把 QT_IM_MODULES 改成 `cangjie` 单值，原生 `xochitl` IC 就不加载 → 丢原生键盘。
   需 `CangjieInputContext` 内部 `create("xochitl")` 拿原生 IC 转发（保留键盘），自己只加中文
   preedit/commit。（Qt `create(QStringList)` 返回**首个有效项**、分隔符 `;`——共存必须靠装饰器,
   不能靠 `cangjie;xochitl` 并列。）
2. **拼音引擎接线**：复用 `chinese-ime/pinyin-engine/c/`（segment/syllables/dict 查询已 C 移植）。
   注入路径从现行"直接改 Qt 文本对象"整体改为 `QInputMethodEvent` preedit（拼音串）/commit（汉字）。
3. **候选栏**：现行 `CjCandidateBar`（QMLDiff 注入）与新通道怎么接，待定。

## 4. 真机验证清单（我无设备权限，需你在设备旁执行）

命门验证（最小、先只读后写、一步一确认，出问题立即回退）——**务必保证 SSH 全程可用**（§5 死锁教训）：

1. 备份：`cp cangjie-qputenv-hook.so /home/root/cangjie-backups/…`，两个 .so 放 `/home`（非 verity）。
2. hook 加进 LD_PRELOAD（与 xovi.so 并列，配置放 rootfs drop-in 或 xovi 引导；**只进程级 restart，先别整机重启**）。
3. `QT_PLUGIN_PATH` 指向插件所在 `/home` 目录（platforminputcontexts 子目录）。
4. `systemctl restart xochitl`；`is-active`=active、MainPID 变、NRestarts 不增。
5. 看 `/tmp/cangjie-hook.log`（拦到 QT_IM_MODULES）+ `/tmp/cangjie-im.log`（插件构造 + create + setFocusObject）。
6. **命门判定**：点开任意文本框 → 是否自动出现"你好"。
   - 出 → ③ 通道成立，进 §3 装饰器/引擎。
   - 不出但日志有 setFocusObject → commit 路径不对（可能 xochitl 的 TextInput 不吃标准
     QInputMethodEvent，得换 preedit 或走 IC 的 `QInputMethodQueryEvent` 协商）。
   - 插件根本没构造 → 版本门/plugin path/hook 改值没生效，逐条回查日志。

## 5. dm-verity 铁律 + 部署（血换的红线，勿犯）

- rootfs(`/dev/mmcblk0p2`)+persist 都是 **dm-verity 完整性保护**。**任何对 `/usr` 的写入（改本体/
  新增插件/hook/drop-in）+ 完整设备重启 → root hash 变 → 校验失败 → A/B slot 回滚**（2026-08-16 两次真机踩实）。
  假象：`remount rw /usr`+进程级 restart 当次保留改动，但**完整重启就回滚**。
- → 铁律：**插件 .so + hook.so 只能放 `/home`（加密、非 verity、重启不丢，但挂载晚有竞态）**。
  引导入口（LD_PRELOAD/QT_PLUGIN_PATH 指向 /home）本身的持久化是 verity 固有难题——这正是仍需
  vellum/xovi（或自建等价"/home 持久+引导"机制）的根本理由，**不是为了 QML 增强**。
- **绝不给 xochitl.service 加加密/延迟挂载依赖**（`After/Wants=home.mount`）：会卡等挂载超时 →
  看门狗重启循环变砖，且回退依赖 SSH 而 dropbear host key 在 /home 未挂载 → SSH 死 → 死锁。
  （2026-08-16 靠 recovery + `rm_recover restore` 救回。）

## 6. 许可证

- `sdk/` 下 `libQt6*.so` = Qt LGPL v3，作为链接依赖再分发合规（保留来源说明）。
- 插件/hook 为本项目原创。引擎复用见 `pinyin-engine/` 各 PROVENANCE（rime-ice=GPLv3 数据、运行时 mmap 不编译进 .so）。

## 7. 真机验证结论（2026-08-16，决定性负面结果，方向③"替换式插件"判不通）

命门验证清单真机跑完（安全法=停服务→`setsid env -i ... LD_PRELOAD=hook QT_PLUGIN_PATH=... xochitl --system` 手动隔离启动，手动进程崩溃不触发 `OnFailure=emergency.target`；busybox 清理用 `kill $(pidof)`；memfault 回收删崩溃 json 拿不到栈；xochitl stripped 无符号、本机无 aarch64 gdb，故靠变量对照实验而非回溯定位）。

**③ 注入链在真机上确实执行**（§712 的"注入最后一环"到达）：hook 拦 qputenv 把 `QT_IM_MODULES xochitl→cangjie` → Qt factory 从 /home 发现并加载动态插件（版本门 6.10 patch 过）→ `CangjieInputContext 构造成功` → `commitString=你好` 执行。**但装不了我们自己的 IC**：

| 变体（插件 create 返回什么） | 真机结果 |
|---|---|
| 裸桩 CangjieInputContext（直接替换，无原生） | 崩（QPA init 期，加载 imageformats 时） |
| 装饰器 CangjieInputContext（`create("xochitl")` 拿原生 KeyboardInputContext 包起来、15 虚函数全转发） | **仍崩**（更晚，xochitl 模块 setup 期，localization/front-light 之后） |
| 对照:原生手动启动（无 hook/插件） | 稳定存活 |
| 诊断:`create("cangjie")` 直接返回原生 KeyboardInputContext 本体（不包装） | **稳定存活** |

**根因确证（置信度高）**：xochitl 的输入法 IC 是它自己的私有具体类 **`KeyboardInputContext`**（`KeyboardInputContextPlugin`，源 `src/devicekeyboard/src/keyboardinputcontext.cpp`，RTTI `20KeyboardInputContext`）。xochitl 键盘模块在 setup 时**依赖"当前安装的 platform IC 就是 KeyboardInputContext 具体实例"**（`qApp->inputMethod()` 侧取回后按具体类型 cast/接线）。只要安装的 IC 不是 KeyboardInputContext 本体（换成我们的子类、哪怕转发全部虚函数），那次 cast 落空 → 崩。返回原生本体就一切正常。

**对方向③的判决**：白皮书 §709–713"用标准 Qt IM 插件 + hook 1 处替代 6+ 处特征码 hook"的**乐观前提不成立**。"xochitl 走标准 Qt IM 通道"这句对**加载**为真（factory/QT_IM_MODULES/版本门全被真机证实），但对**替换**为假——xochitl 与其私有 `KeyboardInputContext` 具体类型强耦合，无法用一个标准 `QPlatformInputContext` 子类顶替。要让我们的 IC 被接受，只能：① 子类化 `KeyboardInputContext`（私有类、在 stripped 主程序里，无法链接/干净复制 ABI → 退化回 trampoline）；或 ② 返回原生本体再 hook 它的按键函数（**这正是现有 `chinese-ime/langhook/` 的做法**）。两条都回到"hook KeyboardInputContext 方法"，③ 省不掉那 6 处。**故：现有 6-hook langhook 是本 app 的正确架构，不是待替换的 hack。**

**保留的真实价值**：
- 真机证明 `QInputMethodEvent::commitString` 这条注入通道可用、能到达焦点对象——**可能用于简化现有 langhook 的"文本注入"这一步**（现在是直接操纵 Qt 文本对象），是现架构内的增量优化，非整体替换。
- 钉死了 xochitl IC 的耦合本质，避免今后再往"替换式插件"方向空耗。
- 本目录（可复现构建 + qputenv hook ABI + 版本门/metadata 格式）作为过程存档与"commitString 注入"实验床保留。
