/* cangjie-langhook —— 验证性 xovi 扩展：不通过符号名，而是靠运行时字节特征码
 * 扫描定位 xofm::libs::localization::LanguageSettings::setLanguageCode，然后
 * 用一个不依赖 xovi override 机制的自定义 trampoline hook 住它。
 *
 * 背景见白皮书 10.1/10.3 节：这个函数在两个固件版本（3.27.1.0/3.28.0.164）里
 * 都不在 .dynsym 导出符号表里，xovi 那套按符号名 dlsym 解析的 override 机制
 * 对它不适用；但反汇编发现两个版本函数体开头 48 字节逐字节相同，只是链接后的
 * 绝对地址不同——这正是字节特征码扫描的适用场景：不管地址在哪个版本挪到哪，
 * 只要这段字节没变，扫描就能找到它，不需要每个版本号硬编码一个偏移量。
 *
 * 范围声明：这一版仍然是“验证 hook 机制本身能生效”——命中目标、打 patch、
 * 原样调用原函数，不改变任何实际行为（真正让语言列表显示新选项需要另外定位
 * UI 填充列表的函数，见 imperative-petting-bentley.md Step F）。2026-08-09
 * 已经在真机（3.28.0.164）部署验证成功过一次：命中地址跟静态分析完全一致，
 * hook 正确触发并完整转调了原始逻辑，xochitl 全程健康无崩溃，详见白皮书
 * 10.3 节。Step E 在此基础上加了两段纯只读的内存转储（vtable + QString 参数
 * 原始字节），用来给 Step F 的反汇编工作提供真实经验数据，不用再靠猜。
 *
 * Step F 反编译 + 解码 QMetaObject 之后确认 availableLanguageCodes 是
 * Q_PROPERTY（不是方法），读取实现在 FUN_009174d0，成员在 this+0x10。
 *
 * Step G（第一次真正改变行为，不再是"只读/原样转调"）：hook FUN_009174d0，
 * 在它读取 availableLanguageCodes 之后，往返回的 QStringList 副本里追加新
 * 语言代码——原始成员（this+0x10）本身永远不碰，只用 Qt 自己导出的
 * QArrayData::allocate（放弃了最初设计里的 QString::fromUtf8，真机连崩
 * 三次才定位到问题，改成手动 ASCII→UTF-16 零扩展，详见 qstringlist_append.c
 * 和计划文件 Step G 的完整踩坑记录）。真机验证过：新条目的显示名不需要额外
 * 处理——getLanguageDisplayName 等方法内部走 QLocale，自己就能正确解析出
 * "简体中文"这类原生名称，不用碰这三个方法。
 *
 * Step G+（本次追加）：一次 ReadProperty 里连续追加三个语言代码——
 * "zh_CN"/"zh_TW"/"zh_HK"（简体、繁体台湾、繁体香港），对应白皮书 8.2 节
 * 已经产出并验证过的三份 .qm（都已放到设备的 translations 目录）。
 * cj_stringlist_append_utf8 本身设计成"读当前 {d,ptr,size} 三元组、原地
 * 产出新三元组"，天然支持在同一次调用里连续追加多项，不需要改动这个函数，
 * 只是把调用点从一次改成循环三次。
 *
 * Step I（追加 zh_TW/zh_HK 之后发现的问题）：LanguageSettings 的三个
 * Q_INVOKABLE 显示名方法也走同一个 FUN_009174d0 分派——反编译确认
 * nativeLanguageName(id=1)/getLanguageDisplayName(id=3) 内部只按 QLocale
 * 的 script（简体/繁体）取名，不看 territory，所以 zh_TW/zh_HK 会重名
 * （zh_CN 因为 script 不同才没暴露这个问题）。同一个 hook 里追加一段
 * call_type==0 && (id==1||id==3) 的处理：先无条件调用原函数（不变），再
 * 判断传入的 code 是不是 "zh_TW"/"zh_HK"，是的话把返回值槽位覆盖成手工
 * 订正的带地区显示名，构造方式复用 Step G 定型的"只用 QArrayData::allocate、
 * 不碰 fromUtf8"方案。
 *
 * Step L（M3 第一版 hook，纯验证，不改变任何行为，真机验证过程中经历过一次
 * 方向调整）：白皮书 9.1 节 Step K 已经用跟 Step F 同样的方法反编译确认——
 * 真正的虚拟键盘宿主类是全局命名空间的 VirtualKeyboard（不是名字很像但其实
 * 几乎是空类的 xofm::libs::keyboardsettings::VirtualKeyboardControl）。
 * 第一版尝试 hook VirtualKeyboard 自己 qt_metacall（FUN_00697510）转调的
 * 私有分派函数 FUN_00695a60——原理上对应 Step F 选中 FUN_009174d0 而不是
 * LanguageSettings 自己 qt_metacall 入口的思路。装上真机后打开一版不限定
 * local_id 的诊断日志，敲字/搜索框输入都测过：其它 local_id（0/1/3/6/11/
 * 12/20/32 等）全部正常命中，唯独 insertText/triggerBackspace 对应的
 * 22-25 一次都没出现过——说明真正敲键盘触发的调用根本没走 qt_metacall 分派，
 * 大概率是 Qt6 QML 的 AOT 编译器（qmlcachegen）在类型编译期已知的调用点上
 * 直接生成了对 C++ 实现函数的调用，绕过了整个元对象调用机制。这是 hook
 * qt_metacall 分派层这条路径本身的真实局限，改成直接 hook 真正的实现函数
 * FUN_00695970（insertText）/FUN_00695820（triggerBackspace）——不管调用方
 * 走哪条路径，最终都必须落到这两个函数。反编译器给 FUN_00695970 生成的 C
 * 签名只显示 3 个参数，跟调用点传的 4 个参数对不上，没有直接采信，改看原始
 * 反汇编：prologue 里 `mov x20,x1`/`mov w21,w2`/`mov w22,w3` 确认了完整的
 * `(this, text 指针, replaceFrom, replaceLength)` 4 参数签名。两个函数
 * 开头 20 字节都是纯寄存器/栈操作，没有 PC 相关寻址，能安全用跟
 * setLanguageCode 同一套 trampoline 机制打 patch。这一版只做原样转调 + 打印
 * 参数（text 内容预览、replaceFrom/replaceLength），不修改任何参数或返回
 * 值，目的是先在真机上确认这两个函数确实是敲键盘时的必经之路，拼音缓冲/
 * 候选逻辑要等这一步验证过再加。
 *
 * Step M（真机验证 Step L 之后，再一次方向调整，最终命中）：真机装上 Step L
 * 之后敲字实测，triggerBackspace 命中了，insertText 敲字全程一次没命中——
 * 说明普通字符键根本不调用公开的 insertText()。回头查 VirtualKeyboard 覆盖
 * 的 QQuickItem 触摸/鼠标虚函数（touchEvent=FUN_00698270、
 * mousePressEvent=FUN_00698bb0，从 vtable slot 26/33 找到，跟找
 * qt_metacast/qt_metacall/event 等 QObject 虚函数是同一手法，只是这次数到了
 * QQuickItem 自己新增的虚函数区），两者命中键位后都调用同一个函数
 * FUN_00697610(this, key指针, isPress)——这才是真正、唯一、完整覆盖所有
 * 按键类型的分发点，不管触摸还是鼠标触发都会经过。真机部署验证：字符键/
 * 退格键全部按预期命中（type=0/type=2 清晰的 press/release 配对）。
 *
 * Step N（解析 key 结构体文本字段偏移）：反编译 + 原始反汇编交叉核实
 * FUN_00697610 的 type==0（字符键）分支，确认文本不是内嵌 24 字节 QString，
 * 而是指针（`ldr x1,[x20,#0x8]` 真做了内存加载，不是 `add` 取地址）。三个
 * 候选来源：普通态 key_ptr+0x08（有效性看 +0x10）、shift 态 key_ptr+0x20
 * （满足 this 上几个状态位条件时基址整体 +0x18）、数字/符号覆盖态
 * key_ptr+0x38（有效性看 +0x40）。没有复刻"该用哪个候选"的选择逻辑（要读
 * this 一堆状态位，容易抄错也没必要），三个候选全部打印，靠真机上用户实际
 * 敲的字符对照哪个槽位命中来验证。反编译中还发现一处"疑似重复的 type==2
 * 判断"，核实后确认是死代码（type==2 退格在更前面已经 return，不是真的第二
 * 条逻辑）。
 *
 * Step T（拼音缓冲状态机，第一次真正拦截/改变按键行为）：动手前把
 * FUN_00697610 完整反编译重新逐分支核对了一遍（缓存在
 * ghidra-project/decompile_keyhandler2_stdout.log，不是重新猜测），确认了
 * 三个关键事实：① release（is_press==0）事件在绘制/发信号之后必定提前
 * return，永远不会走到任何文本提交逻辑，只有 press 需要处理；② type==2
 * （退格）分支直接调用 FUN_00695820，也就是 Step L 已经装好 hook 的
 * triggerBackspace 实现——拦截退格不需要改这个函数，改
 * cj_vk_trigger_backspace_handler 就够了；③ type==1（空格）/type==5（回车）
 * 是在这个函数里内联直接处理（分别构造空格 QString 提交、构造
 * QKeyEvent(Key_Enter) 发送），没有可复用的下层 hook 点，必须在这里拦截。
 * 中文模式判断复用 Step P/Q 已经验证过的 QObject::property()+QVariant
 * 转换手法（这次换成 QVariant::toString()，本机 nm 核实过设备 Qt 库导出
 * 这个符号），"提交缓冲区"复用 Step L 已经装好的 insertText 调用桩
 * （g_orig_vk_insert_text_call_through）。缓冲区/候选切分/展示全部实现见
 * 下面 "Step T" 标记的代码段，设计依据的完整版见计划文件
 * imperative-petting-bentley.md Step T 段落（已知限制、取舍也写在那里，
 * 这里不重复）。
 */
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdint.h>
#include <dlfcn.h>
#include <stdbool.h>
#include <time.h> /* Step T 追加：autoHideTimeout 同步用的空闲计时，见下方说明 */

#include "scan.h"
#include "pattern.h"
#include "trampoline_aarch64.h"
#include "qstringlist_append.h"
#include "segment.h" /* Step T：pinyin-engine/c，Makefile 里 -I 引入的相对路径 */
#include "dictionary.h" /* Step V：候选词典查询层，同一个 -I 路径 */
#include "jianpin.h" /* Step X：简拼索引查询层，同一个 -I 路径 */
#include "shuangpin.h" /* Step Y：双拼（小鹤）键位转换层，同一个 -I 路径 */
#include "userfreq.h" /* Step FF：用户词频（自适应调频），同一个 -I 路径 */
#include "snippets.h" /* Step GG：快捷输入（text replacement），同一个 -I 路径 */

#define TARGET_MODULE_SUFFIX "/usr/bin/xochitl"

/* 白皮书 10.1 节反汇编实测得到的 setLanguageCode 函数入口序言，两个固件版本
 * （3.27.1.0 / 3.28.0.164）逐字节相同，12 条指令共 48 字节：
 *   paciasp
 *   stp  x29, x30, [sp, #-336]!
 *   mov  x29, sp
 *   stp  x21, x22, [sp, #32]
 *   add  x21, x0, #0x28
 *   ldr  x3,  [x1, #16]
 *   stp  x19, x20, [sp, #16]
 *   mov  x19, x1
 *   ldp  x1,  x2,  [x21, #8]
 *   mov  x20, x0
 *   cmp  x2,  x3
 *   ldr  x3,  [x19, #8]
 * 字节顺序已经按 AArch64 小端展开（跟 objdump 打印的 32 位大端风格指令字相反）。
 */
static const uint8_t SET_LANGUAGE_CODE_PROLOGUE[] = {
    0x3f, 0x23, 0x03, 0xd5, /* paciasp                          */
    0xfd, 0x7b, 0xab, 0xa9, /* stp  x29, x30, [sp, #-336]!      */
    0xfd, 0x03, 0x00, 0x91, /* mov  x29, sp                     */
    0xf5, 0x5b, 0x02, 0xa9, /* stp  x21, x22, [sp, #32]         */
    0x15, 0xa0, 0x00, 0x91, /* add  x21, x0, #0x28              */
    0x23, 0x08, 0x40, 0xf9, /* ldr  x3,  [x1, #16]              */
    0xf3, 0x53, 0x01, 0xa9, /* stp  x19, x20, [sp, #16]         */
    0xf3, 0x03, 0x01, 0xaa, /* mov  x19, x1                     */
    0xa1, 0x8a, 0x40, 0xa9, /* ldp  x1,  x2,  [x21, #8]         */
    0xf4, 0x03, 0x00, 0xaa, /* mov  x20, x0                     */
    0x5f, 0x00, 0x03, 0xeb, /* cmp  x2,  x3                     */
    0x63, 0x06, 0x40, 0xf9, /* ldr  x3,  [x19, #8]              */
};

/* ============================================================================
 * 韧性重构（.166 适配）：其余 8 个 hook 目标各自的 prologue 特征码
 *
 * 背景：以前只扫这一条 setLanguageCode 锚点算出 base，其余目标全用“锚点 +
 * .164 固定偏移”。但 OTA 到 .166 后各函数位移不一致（+0x240/-0xc0/-0x220/
 * -0x340…），从锚点推出来的地址全错，safe mode 整体拒绝 → 中文全丢。
 *
 * 现在每个目标由自己 40 字节 prologue 运行期自定位，函数搬家自动适配。全部
 * 在 .164/.166 两版二进制离线对拍确认唯一命中（scratchpad/verify_patterns.py）：
 * 7 个精确命中，2 个（退格 FUN_00695820、按键处理 FUN_00697610）体内各有一条
 * BL 相对跳转，callee 重定位使这 4 字节变化，用配套 _MASK 通配掉即跨版本唯一。
 *
 * 字节从 xochitl_3.28.0.164.bin 对应文件偏移（vaddr-0x400000）机器提取，非手抄。
 * ========================================================================== */

static const uint8_t PROLOGUE_FUN_009174D0[] = { /* Step G：LanguageSettings 本地分派 */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xb5, 0xa9, 0xe4, 0x03, 0x02, 0x2a,
    0xfd, 0x03, 0x00, 0x91, 0xf3, 0x53, 0x01, 0xa9, 0xf3, 0x03, 0x03, 0xaa,
    0xe1, 0x03, 0x00, 0x35, 0x5f, 0x08, 0x00, 0x71, 0xc0, 0x0a, 0x00, 0x54,
    0x4c, 0x12, 0x00, 0x54,
};

static const uint8_t PROLOGUE_VK_DISPATCH[] = { /* Step R+S：VirtualKeyboard 本地分派 */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xac, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf4, 0x03, 0x00, 0xaa, 0xf3, 0x03, 0x03, 0xaa,
    0xf7, 0x63, 0x03, 0xa9, 0xf9, 0x6b, 0x04, 0xa9, 0xfb, 0x73, 0x05, 0xa9,
    0x41, 0x01, 0x00, 0x35,
};

static const uint8_t PROLOGUE_VK_LOADLAYOUT[] = { /* Step U：键盘布局加载 */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xa5, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf3, 0x03, 0x00, 0xaa, 0xf5, 0x5b, 0x02, 0xa9,
    0xf7, 0x63, 0x03, 0xa9, 0xf7, 0x03, 0x01, 0xaa, 0xf9, 0x6b, 0x04, 0xa9,
    0xfb, 0x73, 0x05, 0xa9,
};

static const uint8_t PROLOGUE_VK_INSERTTEXT[] = { /* Step L：insertText 实现 */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xb4, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf4, 0x03, 0x01, 0xaa, 0xf3, 0x03, 0x00, 0xaa,
    0xe0, 0x83, 0x01, 0x91, 0xf5, 0x5b, 0x02, 0xa9, 0xf5, 0x03, 0x02, 0x2a,
    0xf6, 0x03, 0x03, 0x2a,
};

static const uint8_t PROLOGUE_VK_TRIGGERBACKSPACE[] = { /* Step L：triggerBackspace 实现 */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xb7, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf4, 0x03, 0x00, 0xaa, 0xa3, 0x06, 0xf7, 0x97,
    0x00, 0x03, 0x00, 0xb4, 0xf3, 0x03, 0x00, 0xaa, 0x62, 0x00, 0x80, 0x52,
    0x02, 0x20, 0xa0, 0x72,
};
static const uint8_t PROLOGUE_VK_TRIGGERBACKSPACE_MASK[] = { /* +0x14 一条 BL 通配 */
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01,
};

static const uint8_t PROLOGUE_VK_KEYHANDLER[] = { /* Step M：按键统一分发（核心） */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xac, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf4, 0x03, 0x01, 0xaa, 0xf3, 0x03, 0x00, 0xaa,
    0xf5, 0x5b, 0x02, 0xa9, 0x55, 0x1c, 0x00, 0x12, 0xf7, 0x63, 0x03, 0xa9,
    0xe3, 0xf1, 0xff, 0x97,
};
static const uint8_t PROLOGUE_VK_KEYHANDLER_MASK[] = { /* +0x24 一条 BL 通配 */
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x00, 0x00, 0x00, 0x00,
};

static const uint8_t PROLOGUE_EF_STATICMETACALL[] = { /* Step EF：EpubProperties::qt_static_metacall */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xa9, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf3, 0x03, 0x00, 0xaa, 0xa1, 0x01, 0x00, 0x35,
    0x5f, 0x30, 0x00, 0x71, 0xa9, 0x00, 0x00, 0x54, 0xf3, 0x53, 0x41, 0xa9,
    0xfd, 0x7b, 0xd7, 0xa8,
};

static const uint8_t PROLOGUE_EF_SETTEXTFORMAT[] = { /* Step EF：setTextFormat */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xb4, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf7, 0x1b, 0x00, 0xf9, 0xf7, 0x03, 0x00, 0xaa, 0x20, 0x00, 0x40, 0xf9,
    0xa0, 0x11, 0x00, 0xb4, 0xf5, 0x5b, 0x02, 0xa9, 0xf5, 0x03, 0x01, 0xaa,
    0xf6, 0x03, 0x02, 0xaa,
};

/* Step KBS：设置 App「语言和键盘 → 屏幕键盘」中文入口。
 * KeyboardSettingsAttached::qt_static_metacall 序言（栈帧 0xC0，cbnz w1 分派）。
 * 离线在当前固件(sha256 6361610…)与 .164 两版二进制对拍：40 字节完全一致、
 * 各唯一命中（scratchpad/kbd）。rmfw/xochitl.bin(4646e0ae) 那版函数整体布局
 * 不同（栈帧 0x100、分支偏移全变），非精确/掩码可覆盖——但那不是当前/目标
 * 固件，该版会 0 命中 → 该 hook 自跳过 safe mode，不影响核心输入。 */
static const uint8_t PROLOGUE_KBS_STATICMETACALL[] = { /* KeyboardSettingsAttached::qt_static_metacall */
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xb4, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xa1, 0x01, 0x00, 0x35, 0x5f, 0x08, 0x00, 0x71, 0xa0, 0x00, 0x00, 0x54,
    0xec, 0x32, 0x00, 0x54, 0x62, 0x2f, 0x00, 0x34, 0x5f, 0x04, 0x00, 0x71,
    0xa1, 0x2b, 0x00, 0x54,
};

/* Step HL2：荧光笔"吸整行"元凶——命中区间向两边扩张的函数（.169 FUN_00f05ad0，
 * 中文实测走这条：对每个子区间调 FUN_00f052f0 把 start/end 向 ±方向扩张到边界）。
 * 前 20 字节纯栈/寄存器可安全 patch；offset 32 的 CBZ 在 patch 区外、同版本固定，精确匹配。
 * x0=scene，x1=range 向量。 */
static const uint8_t PROLOGUE_HL_EXPAND[] = {
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xbd, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf5, 0x13, 0x00, 0xf9, 0xf5, 0x03, 0x00, 0xaa, 0x20, 0x00, 0x40, 0xf9,
    0xf3, 0x53, 0x01, 0xa9, 0xf4, 0x03, 0x01, 0xaa, 0x80, 0x00, 0x00, 0xb4,
    0x01, 0x00, 0x40, 0xb9,
};

/* 韧性重构：各 hook 目标运行期自定位结果（0=未唯一命中，对应 hook 跳过 safe
 * mode）。setLanguageCode 主 hook 仍在 cj_hook_init 里就地解析，不进这张表。 */
static uintptr_t g_addr_fun9174d0 = 0;         /* Step G */
static uintptr_t g_addr_vk_dispatch = 0;       /* Step R+S（同时是 VK metaobject 的 static_metacall） */
static uintptr_t g_addr_vk_loadlayout = 0;     /* Step U */
static uintptr_t g_addr_vk_inserttext = 0;     /* Step L */
static uintptr_t g_addr_vk_triggerbackspace = 0; /* Step L */
static uintptr_t g_addr_vk_keyhandler = 0;     /* Step M */
static uintptr_t g_addr_ef_staticmetacall = 0; /* Step EF（同时是 EF metaobject 的 static_metacall） */
static uintptr_t g_addr_ef_settextformat = 0;  /* Step EF */
static uintptr_t g_addr_kbs_staticmetacall = 0; /* Step KBS：设置页屏幕键盘中文入口 */
static uintptr_t g_vk_metaobject = 0;          /* Step V-2：扫 static_metacall 唯一 qword-0x18 */
static uintptr_t g_ef_metaobject = 0;          /* Step EF */
static uintptr_t g_kbs_metaobject = 0;         /* Step KBS：靠 static_metacall 特征码地址反查 */
static uintptr_t g_addr_hl_expand = 0;         /* Step HL2：荧光笔命中区间→整行扩张（CJK 修复目标） */

/* 已知版本的入口相对基址偏移，只用来做“命中地址是否落在已知版本范围内”的
 * 诊断日志，不参与实际定位逻辑——真正定位靠上面的特征码扫描，这里只是
 * 锦上添花的核对信息，扫到的偏移不在这个列表里不代表出错，可能只是遇到了
 * 没分析过的新版本。 */
struct known_offset { const char *fw_version; uintptr_t offset; };
static const struct known_offset KNOWN_OFFSETS[] = {
    { "3.28.0.164", 0x913c70 },
    { "3.27.1.0",   0xa0ff50 },
};

/* 覆盖目标函数开头的字节数——跟 cj_build_far_jump() 生成的跳转指令长度一致
 * （5 条指令 = 20 字节），要求 <= 特征码长度（48 字节），否则会踩进后面还没
 * 校验过的指令。 */
#define PATCH_LEN CJ_FAR_JUMP_LEN

/* 保存原函数被覆盖那部分的“可调用副本”：mmap 出来的可执行内存，前 PATCH_LEN
 * 字节是原始指令原样拷贝（这几条指令本身不含 PC 相对寻址，挪到别的地址执行
 * 是安全的——都是纯寄存器/栈操作），后面紧跟一段跳回原函数 PATCH_LEN 偏移处
 * 的远跳转，让原始逻辑从这里开始继续往下执行，行为上等价于完全没有被 patch
 * 过。这是标准的 detour/trampoline 手法。 */
typedef void (*orig_fn_t)(void *this_ptr, void *qstring_arg);
static orig_fn_t g_orig_call_through = NULL;

/* --- 以下两个 dump 函数是这一轮（imperative-petting-bentley.md Step E）新加的，
 * 纯只读——不修改任何内存，只是把已经合法存在的对象内容打印出来，用来经验性
 * 摸清楚 LanguageSettings 的 vtable 布局和这个 Qt 6 build 里 QString 的真实
 * 内存布局，不用去猜/查文档。风险等同于"这个对象能不能正常被调用虚函数"本身
 * ——C++ 对象的第一个字保证是 vtable 指针（Itanium ABI），如果读这个都不安全，
 * 调用方自己调用虚函数时也早就崩了，不存在额外风险。 */

static void cj_dump_vtable(const void *this_ptr, int n_slots) {
    void *const *vptr = *(void *const *const *)this_ptr;
    fprintf(stderr, "[cangjie] vtable @ %p (this=%p):\n", (const void *)vptr, this_ptr);
    for (int i = 0; i < n_slots; i++) {
        fprintf(stderr, "[cangjie]   slot[%2d] = %p\n", i, vptr[i]);
    }
}

static void cj_dump_bytes(const char *label, const void *addr, size_t len) {
    const uint8_t *p = (const uint8_t *)addr;
    char buf[256];
    size_t pos = 0;
    for (size_t i = 0; i < len && pos + 3 < sizeof(buf); i++) {
        int n = snprintf(buf + pos, sizeof(buf) - pos, "%02x ", p[i]);
        if (n <= 0) break;
        pos += (size_t)n;
    }
    fprintf(stderr, "[cangjie] %s @ %p (%zu 字节): %s\n", label, addr, len, buf);
}

#ifdef CJ_UI_TRANSLATION
static int  cj_install_zh_translation(const char *code); /* #2 前向声明,定义在 cj_resolve_qt_symbols 之后 */
static void cj_qml_retranslate(void *qobject);
#endif

static void cj_hook_handler(void *this_ptr, void *qstring_arg) {
    fprintf(stderr,
            "[cangjie] setLanguageCode 被触发：this=%p arg=%p"
            "（不解读结构语义，只做原始只读转储，见下面几行）\n",
            this_ptr, qstring_arg);
    cj_dump_vtable(this_ptr, 20);
    cj_dump_bytes("QString 参数原始内存", qstring_arg, 64);
#ifdef CJ_UI_TRANSLATION
    /* #2 UI 汉化:读出语言码(QString {d,ptr@8,size@16},UTF-16 ASCII),zh_* 则装 /home 翻译器。
     * 先装(call-through 前),原生 setLanguageCode 跑完后再强制 retranslate 刷新已显示界面。 */
    int cj_zh_active = 0;
    if (qstring_arg) {
        void *dp = NULL; long long sz = 0;
        memcpy(&dp, (const uint8_t *)qstring_arg + 8, sizeof(void *));
        memcpy(&sz, (const uint8_t *)qstring_arg + 16, sizeof(long long));
        if (dp && sz > 0 && sz < 16) {
            char code[16]; const uint16_t *ch = (const uint16_t *)dp; long long i;
            for (i = 0; i < sz; i++) code[i] = (ch[i] < 0x80) ? (char)ch[i] : '?';
            code[i] = '\0';
            cj_zh_active = cj_install_zh_translation(code);
        }
    }
#endif
    if (g_orig_call_through) {
        g_orig_call_through(this_ptr, qstring_arg);
    } else {
        fprintf(stderr, "[cangjie] 严重：原函数调用桩未初始化，跳过调用（这不应该发生）\n");
    }
#ifdef CJ_UI_TRANSLATION
    if (cj_zh_active) cj_qml_retranslate(this_ptr);   /* 补触发 QML 全局重译,刷新当前界面 */
#endif
}

static void *make_call_through_stub(const uint8_t *original_bytes, void *jump_back_target) {
    size_t stub_len = PATCH_LEN + CJ_FAR_JUMP_LEN;
    void *stub = mmap(NULL, stub_len, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (stub == MAP_FAILED) return NULL;

    memcpy(stub, original_bytes, PATCH_LEN);

    uint32_t jump_instrs[5];
    cj_build_far_jump(jump_instrs, jump_back_target);
    memcpy((uint8_t *)stub + PATCH_LEN, jump_instrs, CJ_FAR_JUMP_LEN);

    if (mprotect(stub, stub_len, PROT_READ | PROT_EXEC) != 0) {
        munmap(stub, stub_len);
        return NULL;
    }
    __builtin___clear_cache((char *)stub, (char *)stub + stub_len);
    return stub;
}

/* 通用版本：给定目标地址和我们自己的 handler，装好 hook，把可调用原函数的
 * "调用桩"地址写进 *out_stub。原来只服务 setLanguageCode 一个目标，这次
 * （Step G）需要同时 hook 第二个函数，抽成通用函数避免重复这段 mprotect/
 * 调用桩/写跳转的逻辑。 */
static int patch_target(void *target_addr, void *handler, void **out_stub) {
    long pagesize = sysconf(_SC_PAGESIZE);
    if (pagesize <= 0) pagesize = 4096;

    uintptr_t page_base = (uintptr_t)target_addr & ~((uintptr_t)pagesize - 1);
    size_t region_len = (size_t)pagesize;
    /* 如果目标要写入的 PATCH_LEN 字节跨了页边界，扩大保护范围覆盖到下一页 */
    if ((((uintptr_t)target_addr - page_base) + PATCH_LEN) > region_len) {
        region_len += (size_t)pagesize;
    }

    if (mprotect((void *)page_base, region_len, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
        fprintf(stderr, "[cangjie] mprotect 失败，放弃 hook（safe mode）：%s\n", strerror(errno));
        return 0;
    }

    /* 先用原始字节生成“调用桩”，再覆盖目标——顺序不能反，覆盖之后原始字节
     * 就没了，没法再拷贝。 */
    void *jump_back_target = (uint8_t *)target_addr + PATCH_LEN;
    void *stub = make_call_through_stub((const uint8_t *)target_addr, jump_back_target);
    if (!stub) {
        fprintf(stderr, "[cangjie] 调用桩分配失败，放弃 hook（safe mode）\n");
        return 0;
    }
    *out_stub = stub;

    uint32_t jump_to_handler[5];
    cj_build_far_jump(jump_to_handler, handler);
    memcpy(target_addr, jump_to_handler, CJ_FAR_JUMP_LEN);
    __builtin___clear_cache((char *)target_addr, (char *)target_addr + CJ_FAR_JUMP_LEN);

    return 1;
}

/* 韧性重构：在映射镜像 [base, base+size) 里用 prologue 特征码（mask=NULL 为精确
 * 匹配）唯一定位一个 hook 目标，返回运行期地址；0 次或 >1 次都返回 0（对应 hook
 * 走 safe mode 跳过，不在唯一性不确定时打 patch）。 */
static uintptr_t cj_resolve_target(uintptr_t base, size_t size,
                                   const uint8_t *pat, const uint8_t *mask,
                                   size_t len, const char *name) {
    uintptr_t addr = 0;
    size_t hits = cj_count_pattern_masked((const uint8_t *)base, size, pat, mask, len, &addr);
    if (hits != 1) {
        fprintf(stderr, "[cangjie] 特征码定位 %s 命中 %zu 次（要求恰好 1 次），"
                        "该 hook 跳过（safe mode）\n", name, hits);
        return 0;
    }
    fprintf(stderr, "[cangjie] 特征码定位 %s @ %p\n", name, (void *)addr);
    return addr;
}

/* 韧性重构：metaobject 是数据、没有 prologue 可扫，但其结构体 +0x18 字段是
 * static_metacall 函数指针（非 PIE ET_EXEC，绝对值直接存盘），而那个函数我们
 * 已经用特征码定位到了。于是在镜像里扫“等于该函数运行期地址的 8 字节指针”，
 * 减 0x18 即 metaobject。离线在 .164/.166 两版确认过：该 qword 恰好出现 1 次、
 * 正好落在 metaobject+0x18（scratchpad/find_metaobjects2.py）。完全版本无关。 */
static uintptr_t cj_find_metaobject(uintptr_t base, size_t size,
                                    uintptr_t static_metacall_addr, const char *name) {
    if (!static_metacall_addr) return 0;
    uint8_t needle[sizeof(uintptr_t)];
    memcpy(needle, &static_metacall_addr, sizeof(needle));
    uintptr_t hit = 0;
    size_t hits = cj_count_pattern((const uint8_t *)base, size, needle, sizeof(needle), &hit);
    if (hits != 1) {
        fprintf(stderr, "[cangjie] metaobject %s：static_metacall 指针命中 %zu 次"
                        "（要求 1），跳过（safe mode）\n", name, hits);
        return 0;
    }
    uintptr_t mo = hit - 0x18;
    fprintf(stderr, "[cangjie] metaobject %s @ %p（经 static_metacall 指针 @ %p 反查）\n",
            name, (void *)mo, (void *)hit);
    return mo;
}

/* =====================================================================
 * Step G：让 availableLanguageCodes（Q_PROPERTY，属性索引 0，QStringList，
 * 读取实现在 FUN_009174d0 里）返回值多一项 "zh_CN"。
 *
 * 设计依据见白皮书 10.1/10.3 节 + 计划文件 imperative-petting-bentley.md
 * Step G——这里只放实现，不重复背景。核心取舍：不碰 this+0x10 那个原始
 * 硬编码成员，只在"读取"这个具体调用（callType==1 ReadProperty，本地属性
 * 索引==0）触发时，对已经被 Qt 自己正确拷贝到返回值位置的那份独立副本
 * 追加一项——原始数据永远不被触碰，就算这里出错，原始状态也完好。
 * ===================================================================== */

#define FUN_009174D0_VADDR 0x9174d0UL
#define VADDR_BIAS 0x400000UL /* 见白皮书 10.1 节：vaddr = 文件偏移 + 0x400000 */

/* xochitl 模块基址缓存——cj_hook_init 里 cj_find_exec_module 找过一次。
 * 韧性重构后各 hook 目标地址不再由“base+固定偏移”算出（改成各自特征码
 * 自定位，结果存 g_addr_ 系列 + g_vk_metaobject / g_ef_metaobject），base 现在
 * 只作诊断/个别运行期代码复用，不再是定位主力。 */
static uintptr_t g_xochitl_base = 0;

typedef void (*orig_dispatch_fn_t)(void *this_ptr, int call_type, int id, void **args);
static orig_dispatch_fn_t g_orig_dispatch_call_through = NULL;

static cj_qarraydata_allocate_fn g_qarraydata_allocate = NULL;
static int g_qt_symbols_resolve_attempted = 0;

/* 惰性解析——不在扩展刚加载（构造函数最早期）时就调用 dlsym，等到这个
 * handler 真正被触发（此时 xochitl/Qt 肯定已经完全初始化好）才做，避免对
 * "这个时间点调用 dlsym 找 Qt 符号是否安全"做未经验证的假设。
 *
 * 只解析一个符号（QArrayData::allocate）——原本还解析 QString::fromUtf8，
 * 但真机测试连续崩了两次，最后定位到问题就出在调用 fromUtf8 这一步（先是
 * QByteArrayView 参数顺序搞反，改对了还是崩，说明这条路的 ABI 细节没摸透），
 * 干脆改成只依赖这一个已经被反复验证可靠的符号，见 qstringlist_append.h/.c
 * 里的完整踩坑记录。 */
static int cj_resolve_qt_symbols(void) {
    if (g_qt_symbols_resolve_attempted) {
        return g_qarraydata_allocate != NULL;
    }
    g_qt_symbols_resolve_attempted = 1;

    g_qarraydata_allocate = (cj_qarraydata_allocate_fn) dlsym(
        RTLD_DEFAULT, "_ZN10QArrayData8allocateEPPS_xxxNS_16AllocationOptionE");

    if (!g_qarraydata_allocate) {
        fprintf(stderr,
                "[cangjie] Step G：dlsym 解析 QArrayData::allocate 失败，"
                "放弃追加 zh_CN，只做原样转调（safe mode）\n");
        return 0;
    }
    return 1;
}

/* 排查真机崩溃时加固：每一步都 fflush，避免 stderr 缓冲区在崩溃时把还没
 * 写盘的日志一起丢掉（第一次真机测试崩溃后 journalctl 完全没有 Step G 相关
 * 日志，怀疑就是这个原因）。 */
#define CJ_LOG(...) do { fprintf(stderr, __VA_ARGS__); fflush(stderr); } while (0)

/* 依次追加的语言代码——顺序即追加顺序（简体、繁体台湾、繁体香港），对应
 * 设备 translations 目录里已经放好的三份 .qm。全部是纯 ASCII、长度都是 5，
 * cj_stringlist_append_utf8 不做真正的 UTF-8 解码，长度必须精确等于
 * strlen()。 */
static const struct { const char *code; long long len; } CJ_EXTRA_LANGUAGES[] = {
    { "zh_CN", 5 },
    { "zh_TW", 5 },
    { "zh_HK", 5 },
};
#define CJ_EXTRA_LANGUAGES_COUNT (sizeof(CJ_EXTRA_LANGUAGES) / sizeof(CJ_EXTRA_LANGUAGES[0]))

#ifdef CJ_UI_TRANSLATION
/* =====================================================================
 * #2 UI 界面汉化（verity 安全，2026-08-16）：股票固件 translations 目录只有
 * de/en/es/fr、没有 zh，且在 /usr（verity 只读，加不进）。切到 zh_* 时用**公开 Qt
 * 符号（QTranslator ctor/load/installTranslator，全 dlsym RTLD_DEFAULT、固件无关）
 * 装一个从 /home 读 .qm 的 QTranslator——不碰 /usr、无 §707 回滚。xochitl 自己那次
 * QTranslator::load(zh) 因 /usr 无 zh .qm 失败、无害，我们额外叠上；装在 call-through
 * 之前，让 xochitl setLanguageCode 后的 retranslate 拾取。.qm 放下面这个目录（必须与
 * install.sh 部署路径、CJ_DATA_DIR 一致）。 */
#define CJ_TRANS_DIR "/home/root/.local/share/cangjie-ime/translations"

typedef void (*cj_qtranslator_ctor_fn)(void *self, void *parent);
typedef unsigned char (*cj_qtranslator_load_fn)(void *self, const void *fn, const void *dir,
                                                const void *delim, const void *suffix);
typedef unsigned char (*cj_install_translator_fn)(void *translator);

static cj_qtranslator_ctor_fn   g_qtr_ctor = NULL;
static cj_qtranslator_load_fn   g_qtr_load = NULL;
static cj_install_translator_fn g_install_translator = NULL;
static int   g_trans_symbols_attempted = 0, g_trans_symbols_ok = 0;
static unsigned char g_cj_translator_buf[64] __attribute__((aligned(16))); /* 单例 QTranslator 存储 */
static void *g_cj_translator = NULL;              /* 指向上面的缓冲，构造一次 */
static int   g_cj_translator_installed = 0;
static char  g_cj_trans_loaded_base[8] = {0};     /* 已加载的 base，避免重复装同一语言 */

static int cj_resolve_trans_symbols(void) {
    if (g_trans_symbols_attempted) return g_trans_symbols_ok;
    g_trans_symbols_attempted = 1;
    g_qtr_ctor = (cj_qtranslator_ctor_fn) dlsym(RTLD_DEFAULT, "_ZN11QTranslatorC1EP7QObject");
    g_qtr_load = (cj_qtranslator_load_fn) dlsym(RTLD_DEFAULT,
                     "_ZN11QTranslator4loadERK7QStringS2_S2_S2_");
    g_install_translator = (cj_install_translator_fn) dlsym(RTLD_DEFAULT,
                     "_ZN16QCoreApplication17installTranslatorEP11QTranslator");
    g_trans_symbols_ok = g_qtr_ctor && g_qtr_load && g_install_translator && cj_resolve_qt_symbols();
    CJ_LOG("[cangjie] #2 UI翻译符号解析%s（ctor=%p load=%p install=%p）\n",
           g_trans_symbols_ok ? "成功" : "失败（safe mode，跳过UI汉化）",
           (void *)g_qtr_ctor, (void *)g_qtr_load, (void *)g_install_translator);
    return g_trans_symbols_ok;
}

/* code 可能是 zh_CN / zh_CN_SP / zh_TW / zh_TW_SP / zh_HK 等 → 映射到三个 base 之一。 */
static const char *cj_zh_base(const char *code) {
    if (strncmp(code, "zh_TW", 5) == 0) return "zh_TW";
    if (strncmp(code, "zh_HK", 5) == 0) return "zh_HK";
    if (strncmp(code, "zh", 2) == 0)    return "zh_CN";   /* zh_CN* 及兜底 */
    return NULL;
}

/* qmlEngine(this)->retranslate()：强制 QML 全局重新求值 qsTr 绑定，刷新已显示界面。
 * 符号从 libQt6Qml dlsym（RTLD_DEFAULT，xochitl 是 QML app 必已加载）。 */
typedef void *(*cj_qmlengine_fn)(const void *obj);
typedef void  (*cj_retranslate_fn)(void *engine);
typedef void *(*cj_focusobj2_fn)(void);
static cj_qmlengine_fn   g_qml_engine = NULL;
static cj_retranslate_fn g_qml_retranslate = NULL;
static cj_focusobj2_fn   g_ui_focusobj = NULL;
static int g_retrans_symbols_attempted = 0;
static void cj_resolve_retrans_symbols(void) {
    if (g_retrans_symbols_attempted) {
        return;
    }
    g_retrans_symbols_attempted = 1;
    g_qml_engine = (cj_qmlengine_fn) dlsym(RTLD_DEFAULT, "_Z9qmlEnginePK7QObject");
    g_qml_retranslate = (cj_retranslate_fn) dlsym(RTLD_DEFAULT, "_ZN10QQmlEngine11retranslateEv");
    g_ui_focusobj = (cj_focusobj2_fn) dlsym(RTLD_DEFAULT, "_ZN15QGuiApplication11focusObjectEv");
    CJ_LOG("[cangjie] #2 retranslate 符号：qmlEngine=%p retranslate=%p focusObj=%p\n",
           (void *)g_qml_engine, (void *)g_qml_retranslate, (void *)g_ui_focusobj);
}

static void cj_qml_retranslate(void *qobject) {
    cj_resolve_retrans_symbols();
    if (!g_qml_engine || !g_qml_retranslate) return;
    /* LanguageSettings(this)是 C++ 单例、不在 QML 树 → qmlEngine(this)=NULL。改从
     * focusObject(设置页聚焦的 QQuickItem,在 QML 树里)取 engine;this 兜底。 */
    void *engine = NULL;
    if (g_ui_focusobj) { void *fo = g_ui_focusobj(); if (fo) engine = g_qml_engine(fo); }
    if (!engine && qobject) engine = g_qml_engine(qobject);
    if (!engine) { CJ_LOG("[cangjie] #2 拿不到 QQmlEngine(focusObj/this 都 NULL)，无法 retranslate\n"); return; }
    g_qml_retranslate(engine);
    CJ_LOG("[cangjie] #2 已触发 QQmlEngine::retranslate（全局重译）\n");
}

static int cj_install_zh_translation(const char *code) {
    const char *base = cj_zh_base(code);
    if (!base) return 0;                                  /* 非中文，不管 */
    if (!cj_resolve_trans_symbols()) return 0;
    if (g_cj_translator_installed && strcmp(g_cj_trans_loaded_base, base) == 0) return 1; /* 已是该语言，仍可重译 */

    if (!g_cj_translator) {
        g_cj_translator = g_cj_translator_buf;            /* 静态缓冲，免 malloc/stdlib 依赖 */
        memset(g_cj_translator, 0, sizeof(g_cj_translator_buf));
        g_qtr_ctor(g_cj_translator, NULL);                /* QTranslator(parent=NULL) */
    }

    char path[256];
    snprintf(path, sizeof(path), "%s/reMarkable_%s.qm", CJ_TRANS_DIR, base);
    uint8_t fn[24], empty[24];
    memset(empty, 0, sizeof(empty));
    if (!cj_build_ascii_qstring(fn, path, (long long)strlen(path), g_qarraydata_allocate)) {
        CJ_LOG("[cangjie] #2 路径 QString 构造失败，跳过\n"); return 0;
    }
    unsigned char loaded = g_qtr_load(g_cj_translator, fn, empty, empty, empty);
    if (!loaded) { CJ_LOG("[cangjie] #2 QTranslator::load 失败 %s（文件不在?）\n", path); return 0; }

    if (!g_cj_translator_installed) {
        g_install_translator(g_cj_translator);            /* 只装一次；换语言靠 load 换内容 */
        g_cj_translator_installed = 1;
    }
    strncpy(g_cj_trans_loaded_base, base, sizeof(g_cj_trans_loaded_base) - 1);
    CJ_LOG("[cangjie] #2 UI汉化：已装/换 %s 翻译器（%s）\n", base, path);
    return 1;
}
#endif /* CJ_UI_TRANSLATION */

/* Step I：zh_TW/zh_HK 在原始 nativeLanguageName/getLanguageDisplayName 里
 * 重名（白皮书/计划文件 Step I 已经反编译确认根因：两者在 Qt/CLDR 里都是
 * Hant script，native name 只到 script 粒度，不到 territory 粒度）——这里
 * 手工订正成带地区的显示名，跟原始代码里已经有的 "es"->"es_US"、
 * "en"->"English" 特判是同一种性质的订正，只是换成我们需要的 case。
 * C11 u"" 字面量，-std=gnu17 支持。 */
static const uint16_t CJ_DISPLAY_NAME_ZH_TW[] = u"繁體中文（台灣）";
static const uint16_t CJ_DISPLAY_NAME_ZH_HK[] = u"繁體中文（香港）";
#define CJ_DISPLAY_NAME_ZH_TW_LEN ((long long)(sizeof(CJ_DISPLAY_NAME_ZH_TW) / sizeof(uint16_t)) - 1)
#define CJ_DISPLAY_NAME_ZH_HK_LEN ((long long)(sizeof(CJ_DISPLAY_NAME_ZH_HK) / sizeof(uint16_t)) - 1)

static void cj_override_display_name_if_zh_variant(uint8_t *dest, const uint8_t *code_qstring) {
    const char *matched_label = NULL;
    const uint16_t *replacement = NULL;
    long long replacement_len = 0;

    if (cj_qstring_equals_ascii(code_qstring, "zh_TW", 5)) {
        matched_label = "zh_TW";
        replacement = CJ_DISPLAY_NAME_ZH_TW;
        replacement_len = CJ_DISPLAY_NAME_ZH_TW_LEN;
    } else if (cj_qstring_equals_ascii(code_qstring, "zh_HK", 5)) {
        matched_label = "zh_HK";
        replacement = CJ_DISPLAY_NAME_ZH_HK;
        replacement_len = CJ_DISPLAY_NAME_ZH_HK_LEN;
    } else {
        return; /* zh_CN、en、es、de、fr 等一律不插手，保留原函数已经写好的结果 */
    }

    if (!cj_resolve_qt_symbols()) {
        return; /* 已经在 cj_resolve_qt_symbols 里打过日志 */
    }

    uint8_t new_name[24];
    int ok = cj_build_utf16_literal_qstring(new_name, replacement, replacement_len, g_qarraydata_allocate);
    CJ_LOG("[cangjie] Step I：为 \"%s\" 构造订正显示名：%s\n", matched_label,
           ok ? "成功" : "失败（保留原函数的重名结果，不影响其它功能）");
    if (!ok) {
        return;
    }
    memcpy(dest, new_name, 24);
    CJ_LOG("[cangjie] Step I：\"%s\" 显示名已覆盖为订正后的字符串\n", matched_label);
}

/* Phase B（4 输入法）统一真源表：简/繁 × 全拼/双拼，各编码成一个伪语言
 * 代码，追加进 virtualKeyboard.languages 供原生语言弹层点选（跟切西班牙语
 * 同一条路径）。用户点选后代码被写进原生 language 属性，我们读回解析出
 * (简/繁, 全拼/双拼) 两个轴。一张表驱动四处——Step S 追加 / Step R 显示名
 * 覆盖 / Step U 布局兜底 / 模式解析 cj_step_t_language_mode——避免多份列表
 * 日后漂移。伪代码安全性：写进 language 后只有我们消费；原生 languageName()
 * 内部 QLocale(code) 对未知代码容错、且结果被我们覆盖；布局加载被 Step U
 * 强制换 en_US。zh_CN/zh_TW 本就是原生不支持的代码、今天已在弹层正常工作，
 * 再加两个同性质代码无新风险。显示名去掉地区括注（这里没有 zh_HK 撞名问题，
 * 跟姊妹文档 Step I 的 LanguageSettings 三选一场景不同，不共用常量）。
 * CjLanguageMode 枚举定义前倒到这里（原在文件后段），让本表的 base 字段和
 * 下面 Step R/S/U 都能引用它。详见白皮书 02 节 Phase B。 */
typedef enum {
    CJ_LANG_OTHER = 0,
    CJ_LANG_ZH_CN,
    CJ_LANG_ZH_TW,
} CjLanguageMode;

static const uint16_t CJ_VK_LABEL_ZH_CN[]    = u"简体全拼";
static const uint16_t CJ_VK_LABEL_ZH_TW[]    = u"繁體全拼";
static const uint16_t CJ_VK_LABEL_ZH_CN_SP[] = u"简体双拼";
static const uint16_t CJ_VK_LABEL_ZH_TW_SP[] = u"繁體双拼";
#define CJ_VK_LBL_LEN(a) ((long long)(sizeof(a) / sizeof(uint16_t)) - 1)

static const struct {
    const char *code;
    long long code_len;
    const uint16_t *label;
    long long label_len;
    CjLanguageMode base; /* _SP 变体映射到其简/繁 base，双拼只体现在 shuangpin 位 */
    int shuangpin;
} CJ_VK_MODES[] = {
    { "zh_CN",    5, CJ_VK_LABEL_ZH_CN,    CJ_VK_LBL_LEN(CJ_VK_LABEL_ZH_CN),    CJ_LANG_ZH_CN, 0 },
    { "zh_TW",    5, CJ_VK_LABEL_ZH_TW,    CJ_VK_LBL_LEN(CJ_VK_LABEL_ZH_TW),    CJ_LANG_ZH_TW, 0 },
    { "zh_CN_SP", 8, CJ_VK_LABEL_ZH_CN_SP, CJ_VK_LBL_LEN(CJ_VK_LABEL_ZH_CN_SP), CJ_LANG_ZH_CN, 1 },
    { "zh_TW_SP", 8, CJ_VK_LABEL_ZH_TW_SP, CJ_VK_LBL_LEN(CJ_VK_LABEL_ZH_TW_SP), CJ_LANG_ZH_TW, 1 },
};
#define CJ_VK_MODES_COUNT (sizeof(CJ_VK_MODES) / sizeof(CJ_VK_MODES[0]))

static void cj_vk_override_language_name(uint8_t *dest, const uint8_t *code_qstring) {
    const char *matched_label = NULL;
    const uint16_t *replacement = NULL;
    long long replacement_len = 0;

    for (size_t i = 0; i < CJ_VK_MODES_COUNT; i++) {
        if (cj_qstring_equals_ascii(code_qstring, CJ_VK_MODES[i].code, CJ_VK_MODES[i].code_len)) {
            matched_label = CJ_VK_MODES[i].code;
            replacement = CJ_VK_MODES[i].label;
            replacement_len = CJ_VK_MODES[i].label_len;
            break;
        }
    }
    if (!replacement) {
        return; /* 其它语言一律不插手，保留原函数结果 */
    }

    if (!cj_resolve_qt_symbols()) {
        return;
    }

    uint8_t new_name[24];
    int ok = cj_build_utf16_literal_qstring(new_name, replacement, replacement_len, g_qarraydata_allocate);
    CJ_LOG("[cangjie] Step R：为 \"%s\" 构造订正显示名：%s\n", matched_label,
           ok ? "成功" : "失败（保留原函数结果，不影响其它功能）");
    if (!ok) {
        return;
    }
    memcpy(dest, new_name, 24);
    CJ_LOG("[cangjie] Step R：\"%s\" 显示名已覆盖为订正后的字符串\n", matched_label);
}

/* Step KBS：设置 App「屏幕键盘」把简繁全双拼收成一个"中文"入口——列表项
 * 显示名 + 状态行都走 LanguageSettings::languageName(code)（method id==2），
 * 原生经 QLocale 把 zh_* 泛化成 "Chinese"，这里对我们的 4 个中文伪代码统一
 * 覆盖成"中文"。与地球弹层的 VirtualKeyboard::languageName（Step R，显示
 * "简体双拼"等具体模式）是两个不同的方法/数据源，互不影响：设置页只呈现
 * 一个"中文"总入口，进去选中即 keyboardLanguage=zh_CN，模式再靠键盘地球键切。 */
static const uint16_t CJ_DISPLAY_NAME_ZH[] = u"中文";
#define CJ_DISPLAY_NAME_ZH_LEN ((long long)(sizeof(CJ_DISPLAY_NAME_ZH) / sizeof(uint16_t)) - 1)

static void cj_ls_override_language_name_to_zh(uint8_t *dest, const uint8_t *code_qstring) {
    int is_zh = 0;
    for (size_t i = 0; i < CJ_VK_MODES_COUNT; i++) {
        if (cj_qstring_equals_ascii(code_qstring, CJ_VK_MODES[i].code, CJ_VK_MODES[i].code_len)) {
            is_zh = 1;
            break;
        }
    }
    if (!is_zh) {
        return; /* 非中文伪代码，保留原函数结果（en/es/de/fr… 一律不插手） */
    }
    if (!cj_resolve_qt_symbols()) {
        return;
    }
    uint8_t new_name[24];
    int ok = cj_build_utf16_literal_qstring(new_name, CJ_DISPLAY_NAME_ZH,
                                            CJ_DISPLAY_NAME_ZH_LEN, g_qarraydata_allocate);
    CJ_LOG("[cangjie] Step KBS：设置页 languageName 覆盖为\"中文\"：%s\n",
           ok ? "成功" : "失败（保留原结果，不影响其它功能）");
    if (!ok) {
        return;
    }
    memcpy(dest, new_name, 24);
}

static void cj_metacall_dispatch_handler(void *this_ptr, int call_type, int id, void **args) {
    CJ_LOG("[cangjie] metacall dispatch 被触发：this=%p call_type=%d id=%d args=%p\n",
           this_ptr, call_type, id, (void *)args);

    if (g_orig_dispatch_call_through) {
        CJ_LOG("[cangjie] 调用原函数（调用桩）...\n");
        g_orig_dispatch_call_through(this_ptr, call_type, id, args);
        CJ_LOG("[cangjie] 原函数返回完成\n");
    } else {
        CJ_LOG("[cangjie] 严重：metacall 调用桩未初始化，跳过调用（这不应该发生）\n");
        return;
    }

    if (args == NULL) {
        return;
    }

    /* call_type==1 (QMetaObject::ReadProperty)，id==0（本地属性索引，对应
     * availableLanguageCodes——白皮书 10.1 节已经从 QMetaObject 数据结构里
     * 逐字节核实过）。 */
    if (call_type == 1 && id == 0) {
        CJ_LOG("[cangjie] 命中 availableLanguageCodes 的 ReadProperty 调用，准备追加语言代码\n");

        uint8_t *dest = (uint8_t *)args[0];
        CJ_LOG("[cangjie] dest(args[0]) = %p\n", (void *)dest);
        if (dest == NULL) {
            return;
        }

        if (!cj_resolve_qt_symbols()) {
            return; /* 已经在 cj_resolve_qt_symbols 里打过日志 */
        }
        CJ_LOG("[cangjie] Qt 符号解析成功：allocate=%p\n", (void *)g_qarraydata_allocate);

        for (size_t i = 0; i < CJ_EXTRA_LANGUAGES_COUNT; i++) {
            int ok = cj_stringlist_append_utf8(dest, CJ_EXTRA_LANGUAGES[i].code,
                                                CJ_EXTRA_LANGUAGES[i].len, g_qarraydata_allocate);
            CJ_LOG("[cangjie] Step G：availableLanguageCodes 追加 \"%s\"：%s\n",
                   CJ_EXTRA_LANGUAGES[i].code,
                   ok ? "成功" : "失败（保留当前列表状态，不影响其它功能，停止继续追加）");
            if (!ok) break; /* 一次失败就停手，已经成功追加的部分保留，不重试、不回滚 */
        }
        return;
    }

    /* Step I：call_type==0 (InvokeMetaMethod)，id==1(nativeLanguageName) 或
     * id==3(getLanguageDisplayName)——反编译确认两者都走同一个共享 helper，
     * 输出只到 script 粒度，zh_TW/zh_HK 因此重名。args[0]=返回值槽位指针
     * （跟上面 availableLanguageCodes 是同一种约定），args[1]=传入的 code
     * QString 指针本身（反编译里 FUN_00c3ec80(&local_20,param_4[1]) 的用法
     * 已确认，不需要再解一层指针）。 */
    if (call_type == 0 && (id == 1 || id == 3)) {
        uint8_t *dest = (uint8_t *)args[0];
        const uint8_t *code_qstring = (const uint8_t *)args[1];
        if (dest == NULL || code_qstring == NULL) {
            return;
        }
        cj_override_display_name_if_zh_variant(dest, code_qstring);
    }

    /* Step KBS：languageName()（method id==2）——设置 App「屏幕键盘」列表项
     * 显示名 + 状态行都走它。args 约定与 id==1/id==3 同构（metaobject 逐字节
     * 核实三者都是 QString method(QString code)、argc==1 flags==0x102，共用这个
     * 本地分派 helper）：args[0]=返回值槽位，args[1]=传入 code QString。 */
    if (call_type == 0 && id == 2) {
        uint8_t *dest = (uint8_t *)args[0];
        const uint8_t *code_qstring = (const uint8_t *)args[1];
        if (dest == NULL || code_qstring == NULL) {
            return;
        }
        cj_ls_override_language_name_to_zh(dest, code_qstring);
    }
}

static void cj_install_availablelanguagecodes_hook(void) {
    uintptr_t target = g_addr_fun9174d0;
    if (!target) return; /* 特征码没唯一命中，cj_resolve_target 已记录，跳过 */

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_metacall_dispatch_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step G：FUN_009174d0 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_dispatch_call_through = (orig_dispatch_fn_t)stub;
    fprintf(stderr, "[cangjie] Step G：FUN_009174d0 hook 安装完成 @ %p\n", (void *)target);
}

/* =====================================================================
 * Step R+S：VirtualKeyboard 自己的本地方法/属性统一分派函数
 * （FUN_00695a60，`VirtualKeyboard::qt_metacall`=FUN_00697510 会把
 * call_type+本地 id 转发到这里）：
 *   - languageName()：call_type==0(InvokeMetaMethod) id==0x20(32)，
 *     args[1]=传入的 code QString，args[0]=返回值槽位（swap 写回）。跟
 *     LanguageSettings::getLanguageDisplayName（Step I 已修）是同一族
 *     QLocale 脚本粒度逻辑，zh_TW/zh_HK 重名 bug 会重现，逐指令确认过。
 *   - languages 属性：call_type==1(ReadProperty) **id==0**（不是 1！）。
 *
 * 崩溃事故记录（计划文件 Step R+S 段落有完整过程）：最初反编译时把
 * `languages` 误判成 id==1，真机测试点搜索框时崩溃——事后用纯只读诊断
 * hook（只 dump 字节，不解引用）查清楚：id==1 读到的其实是单个 QString
 * "en_US"（`language`，当前激活语言，size=5 是字符数不是元素数）；id==0
 * 才是真正的 `languages`（size=17，元素预览确认是 "da_DK"/"nl_NL"/
 * "en_US" 这样的语言代码，标准 24 字节 QString 数组，形态确实跟
 * availableLanguageCodes 一致）。编号错了一位，数据格式本身没问题——已经
 * 真机验证过修正后的编号，不是重新猜测。
 *
 * 签名 (this, call_type, local_id, args) 复用已有的 orig_dispatch_fn_t
 * 类型。
 * ===================================================================== */

#define VK_DISPATCH_VADDR 0x695a60UL

static orig_dispatch_fn_t g_orig_vk_dispatch_call_through = NULL;

static void cj_vk_dispatch_handler(void *this_ptr, int call_type, int id, void **args) {
    (void)this_ptr;
    /* 无条件先转调原函数——原始逻辑先把数据正确算好/拷贝到 args[0]，我们
     * 只在其基础上后处理，不碰对象内部字段，跟 Step G 同样的取舍。 */
    if (g_orig_vk_dispatch_call_through) {
        g_orig_vk_dispatch_call_through(this_ptr, call_type, id, args);
    } else {
        CJ_LOG("[cangjie] 严重：VirtualKeyboard 本地分派调用桩未初始化，跳过调用（不应该发生）\n");
        return;
    }

    if (args == NULL) {
        return;
    }

    /* Step R：languageName()，call_type=0(Invoke) id=0x20(32)。 */
    if (call_type == 0 && id == 0x20) {
        uint8_t *dest = (uint8_t *)args[0];
        const uint8_t *code_qstring = (const uint8_t *)args[1];
        if (dest && code_qstring) {
            cj_vk_override_language_name(dest, code_qstring); /* Step R 专用，见上方说明 */
        }
        return;
    }

    /* Step S：languages 属性，call_type=1(Read) id=0（真机诊断确认，不是
     * 最初反编译时假设的 1——那个是崩溃的直接原因）。 */
    if (call_type == 1 && id == 0) {
        uint8_t *dest = (uint8_t *)args[0];
        if (dest == NULL) {
            return;
        }
        if (!cj_resolve_qt_symbols()) { /* 复用 Step G 的 QArrayData::allocate 解析 */
            return;
        }
        for (size_t i = 0; i < CJ_VK_MODES_COUNT; i++) {
            int ok = cj_stringlist_append_utf8(dest, CJ_VK_MODES[i].code,
                                                CJ_VK_MODES[i].code_len, g_qarraydata_allocate);
            CJ_LOG("[cangjie] Step S：languages 追加 \"%s\"：%s\n", CJ_VK_MODES[i].code,
                   ok ? "成功" : "失败（保留当前列表状态，停止继续追加）");
            if (!ok) break;
        }
    }
}

static void cj_install_virtualkeyboard_dispatch_hook(void) {
    uintptr_t target = g_addr_vk_dispatch;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_vk_dispatch_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step R+S：FUN_00695a60 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_vk_dispatch_call_through = (orig_dispatch_fn_t)stub;
    fprintf(stderr, "[cangjie] Step R+S：FUN_00695a60 hook 安装完成 @ %p\n", (void *)target);
}

/* =====================================================================
 * Step U（真机发现的紧急修复，Step T 部署后用户实测暴露）：切到语言切换器
 * 里新增的"简体中文"/"繁体中文"之后，收起键盘按钮以下的按键网格整体消失。
 *
 * 根因（反编译确认，不是猜测——用跟 Step K/M/N 一致的方法论：先读
 * FUN_00695a60 的 WriteProperty 分支，call_type==2 (Write) id==1
 * (language) 转调 FUN_00692410(this, code)；该函数把 code 写进
 * this+0xf0/+0xf8/+0x100（跟已确认的 language 属性存储一致）、发
 * languageChanged 信号（signal index 5）之后，若 code 不是 "numeric"，
 * 会调用 FUN_00c6f3c0(this, code)——反编译这个函数确认它就是键盘布局加载
 * 逻辑（调试字符串字面量 "Loading layout for "），一进来先清空
 * this+0x20/+0x38/+0x50 这几个行/键容器（FUN_0069ae50），再调用
 * FUN_00c6b830(code, &jsonObj) 按 code 查找布局 JSON 资源——**查不到就
 * 直接跳到函数末尾 return**（decompile_layoutload_stdout.log 第134/830
 * 行，中间几百行重建布局的代码全部被跳过），容器从此保持空。zh_CN/zh_TW
 * 从来不是这个虚拟键盘原生支持的语言，天然查不到对应的布局资源，这是
 * Step R+S 设计时的一个遗漏——只验证过"选项能不能出现、点了会不会崩溃"，
 * 没有验证"选中之后键盘本身是否还能正常渲染"，这次真机测试才第一次真正
 * 暴露。
 *
 * 修复：hook FUN_00c6f3c0 本身（入口 20 字节 `paciasp/stp/mov x29,sp/
 * stp/mov x19,x0`，纯寄存器栈操作，无 PC 相关寻址，反汇编核实过，跟
 * Step L 的 insertText/triggerBackspace 同款安全）——调用它之前检查传入
 * 的语言代码，如果是 zh_CN/zh_TW，就换成一个真机确认过真实存在、有对应
 * 布局资源的语言代码（"en_US"——Step S 诊断阶段读到的真实 languages 列表
 * 元素预览里就有这一项）再调用原函数，让布局资源查找成功、按键网格照常用
 * 英文 QWERTY 渲染。这个函数的入参是"这次要用哪个语言的布局"，跟
 * this+0xf0 存的"当前 language 属性值"是两回事——FUN_00692410 在调用这个
 * 函数之前已经把 this+0xf0 设成真实的 zh_CN/zh_TW 了，这里只换传给
 * FUN_00c6f3c0 的参数，不碰 this 的任何字段，所以 virtualKeyboard.language
 * 依然正确汇报 zh_CN/zh_TW，Step T 的中文模式判断（读 language 属性）不受
 * 影响。 */
#define VK_LOADLAYOUT_VADDR 0xc6f3c0UL

typedef void (*orig_vk_loadlayout_fn_t)(void *this_ptr, const uint8_t *code_qstring);
static orig_vk_loadlayout_fn_t g_orig_vk_loadlayout_call_through = NULL;

static const char CJ_FALLBACK_LAYOUT_LANG[] = "en_US";
#define CJ_FALLBACK_LAYOUT_LANG_LEN 5

static void cj_vk_loadlayout_handler(void *this_ptr, const uint8_t *code_qstring) {
    const uint8_t *effective_code = code_qstring;
    uint8_t substitute[24];

    int is_our_chinese = 0;
    if (code_qstring) {
        for (size_t i = 0; i < CJ_VK_MODES_COUNT; i++) {
            if (cj_qstring_equals_ascii(code_qstring, CJ_VK_MODES[i].code, CJ_VK_MODES[i].code_len)) {
                is_our_chinese = 1;
                break;
            }
        }
    }
    if (is_our_chinese) {
        int substituted = 0;
        if (cj_resolve_qt_symbols() &&
            cj_build_ascii_qstring(substitute, CJ_FALLBACK_LAYOUT_LANG, CJ_FALLBACK_LAYOUT_LANG_LEN,
                                    g_qarraydata_allocate)) {
            effective_code = substitute;
            substituted = 1;
        }
        CJ_LOG("[cangjie] Step U：语言代码是中文，键盘布局改用 \"%s\" 兜底：%s\n",
               CJ_FALLBACK_LAYOUT_LANG,
               substituted ? "成功" : "失败（回退到原始 code，布局可能仍是空的）");
    }

    if (g_orig_vk_loadlayout_call_through) {
        g_orig_vk_loadlayout_call_through(this_ptr, effective_code);
    } else {
        CJ_LOG("[cangjie] 严重：键盘布局加载函数调用桩未初始化，跳过调用（不应该发生）\n");
    }
}

static void cj_install_virtualkeyboard_loadlayout_hook(void) {
    uintptr_t target = g_addr_vk_loadlayout;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_vk_loadlayout_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step U：键盘布局加载 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_vk_loadlayout_call_through = (orig_vk_loadlayout_fn_t)stub;
    fprintf(stderr, "[cangjie] Step U：键盘布局加载 hook 安装完成 @ %p\n", (void *)target);
}

/* =====================================================================
 * Step L：VirtualKeyboard::insertText/triggerBackspace 的第一版 hook，
 * 纯验证性质——见上面文件头注释里的完整背景。这一版故意不改变任何行为，
 * 只在命中目标方法时打印参数，用真机日志核实反编译结论。
 *
 * 真机验证过程中的一次重要纠正：最初 hook 在 FUN_00695a60（qt_metacall
 * 分派到的本地方法路由函数）——装上之后打开一个诊断版本（不限定 local_id，
 * 把所有 flag==0 的调用都打出来），真机上敲字/搜索框输入都验证过，local_id
 * 命中了 0/1/3/6/11/12/20/32 等一大堆值，唯独 22-25（insertText 的三个
 * 重载 + triggerBackspace）一次都没出现过。结论：真正敲键盘触发的 insertText
 * 调用根本没有经过 qt_metacall 分派——大概率是 Qt6 QML 的 AOT
 * 编译器（qmlcachegen）在 QML 侧 `virtualKeyboard.insertText(...)` 这种
 * 类型在编译期已知的调用点上，直接生成了对 C++ 实现函数的调用，绕过了整个
 * 元对象调用机制。这是 hook qt_metacall 分派层这条路径本身的一个真实局限，
 * 不是我们代码写错了。
 *
 * 应对：改成直接 hook insertText/triggerBackspace 的真正实现函数
 * （FUN_00695970/FUN_00695820）——不管调用方是走 qt_metacall 还是 QML AOT
 * 直调，最终都必须落到这两个函数上，这里才是真正兜底的 hook 点。反编译器
 * 生成的 FUN_00695970 C 签名（只显示 3 个参数）跟原始调用点传的 4 个参数
 * 对不上，没有直接相信反编译结果，改看原始反汇编——prologue 里
 * `mov x20,x1`/`mov w21,w2`/`mov w22,w3` 三条指令确认了完整的 4 参数签名
 * `(this, text 指针, replaceFrom, replaceLength)`，跟调用点的用法一致。
 * FUN_00695820（triggerBackspace）反编译签名本身就只有一个参数，没有这个
 * 问题。两个函数开头 20 字节都是纯寄存器/栈操作，没有 PC 相关寻址，可以
 * 安全用跟 setLanguageCode 同一套 trampoline 机制打 patch。
 * ===================================================================== */

#define VK_INSERTTEXT_IMPL_VADDR 0x695970UL       /* FUN_00695970，insertText 真实实现 */
#define VK_TRIGGERBACKSPACE_IMPL_VADDR 0x695820UL /* FUN_00695820，triggerBackspace 真实实现 */

typedef void (*orig_vk_insert_text_fn_t)(void *this_ptr, const void *text_qstring,
                                          int replace_from, int replace_length);
static orig_vk_insert_text_fn_t g_orig_vk_insert_text_call_through = NULL;

typedef void (*orig_vk_trigger_backspace_fn_t)(void *this_ptr);
static orig_vk_trigger_backspace_fn_t g_orig_vk_trigger_backspace_call_through = NULL;

/* 只读预览一个 QString 参数的内容——跟 Step E 的 cj_dump_bytes 一样是纯
 * 只读诊断，不修改任何内存。非 ASCII 字符先用 '?' 占位（拼音输入阶段敲的
 * 都是拉丁字母，这一步只需要确认"参数传的是不是我们期望的那段拉丁文字/
 * 空字符串"，不需要完整还原中文候选词，真要看中文内容再升级成 UTF-16
 * 多字节输出）。size/data 指针明显不合理时退化成打印原始 24 字节，不猜。 */
static void cj_log_qstring_preview(const char *label, const void *qstring_ptr) {
    if (!qstring_ptr) {
        CJ_LOG("[cangjie]   %s: (null)\n", label);
        return;
    }
    const uint8_t *p = (const uint8_t *)qstring_ptr;
    void *data_ptr = NULL;
    long long size = 0;
    memcpy(&data_ptr, p + 8, sizeof(void *));
    memcpy(&size, p + 16, sizeof(long long));

    if (size < 0 || size > 200 || (size > 0 && data_ptr == NULL)) {
        CJ_LOG("[cangjie]   %s: size=%lld data=%p（超出安全预览范围，改打印原始字节）\n",
               label, size, data_ptr);
        cj_dump_bytes(label, qstring_ptr, 24);
        return;
    }

    char buf[256];
    size_t pos = 0;
    const uint16_t *chars = (const uint16_t *)data_ptr;
    for (long long i = 0; i < size && pos + 4 < sizeof(buf); i++) {
        uint16_t c = chars[i];
        buf[pos++] = (c < 0x80) ? (char)c : '?';
    }
    buf[pos] = '\0';
    CJ_LOG("[cangjie]   %s: size=%lld data=%p 内容预览=\"%s\"\n", label, size, data_ptr, buf);
}

/* Step T：进程内拼音缓冲区——重启即清空，不持久化，跟 M3 一贯的原则一致。
 * 全局状态放在这里（比 Step T 大段代码本身更靠前）是因为下面的
 * cj_vk_insert_text_handler（Step V 新增的缓冲区同步逻辑）和后面的
 * cj_vk_trigger_backspace_handler 都马上要用，C 不允许用还没声明的全局。 */
static char g_pinyin_buffer[CJ_SEG_MAX_INPUT];
static int g_pinyin_buffer_len = 0;

/* Phase A（A-iii 退格撤销/中途改字）：本次 composition 的"已提交撤销栈"。
 * 每次点选提交一个候选，就把它消耗掉的原始拼音字节 + 它在文本框占的
 * UTF-16 码元数压栈；退格在缓冲区空时弹栈——从文本框删掉上一个已选词、
 * 把它的拼音还回缓冲区，实现"从右往左剥整个 composition、撤销已选字并
 * 恢复拼音重选"。composition 结束（空格/回车确认、焦点切换、闲置超时、
 * 缓冲区空时敲原生空格/回车）时清空这个栈，之后退格转原生。声明放这里
 * （跟 g_pinyin_buffer 一起）是因为退格/提交/外部同步几处都要用。 */
#define CJ_COMPOSE_STACK_MAX 64
typedef struct {
    char pinyin[CJ_SEG_MAX_INPUT]; /* 这次提交消耗掉的原始按键字节（撤销时还回缓冲区） */
    int pinyin_len;
    int textfield_units;           /* 提交的词在文本框占几个 UTF-16 码元（撤销时删这么多） */
} CjComposeEntry;
static CjComposeEntry g_compose_stack[CJ_COMPOSE_STACK_MAX];
static int g_compose_stack_len = 0;

/* 一次性清空撤销栈——composition 结束的统一入口。 */
/* A-iv（焦点守卫，2026-08-22 修 bug）：拼音缓冲区/撤销栈都是全局单例，
 * 但 composition 语义上属于"某一个输入框"——用户在笔记里空格上屏后
 * 关笔记、去搜索框按退格，栈里的陈旧条目会把"你好"的拼音还原进候选栏、
 * 甚至往新输入框发删码元（真机踩过）。修法：记录 composition 归属的
 * focusObject（写缓冲区/压撤销栈时更新），退格拦截前比对当前焦点，变了
 * 就整体 finalize（清缓冲区+清栈+收候选栏）、退格放行原生。 */
static void *g_ime_focus_obj = NULL;

/* A-iv：当前聚焦的输入对象（QGuiApplication::focusObject()，公开符号，
 * #2 retranslate 路径真机验证过同一符号）。独立惰性解析——不复用
 * CJ_UI_TRANSLATION 条件块里那套（那个宏不开时会整块编译不进来）。
 * 拿不到返回 NULL（调用方按"焦点已变"处理，fail-safe 方向是放行原生）。 */
typedef void *(*cj_aiv_focusobj_fn)(void);
static cj_aiv_focusobj_fn g_aiv_focusobj = NULL;
static int g_aiv_focusobj_attempted = 0;

static void *cj_current_focus_object(void) {
    if (!g_aiv_focusobj_attempted) {
        g_aiv_focusobj_attempted = 1;
        g_aiv_focusobj = (cj_aiv_focusobj_fn) dlsym(
            RTLD_DEFAULT, "_ZN15QGuiApplication11focusObjectEv");
        CJ_LOG("[cangjie] A-iv：focusObject 符号=%p\n", (void *)g_aiv_focusobj);
    }
    return g_aiv_focusobj ? g_aiv_focusobj() : NULL;
}

static void cj_compose_stack_reset(void) {
    g_compose_stack_len = 0;
}

/* UTF-8 字节串占多少个 UTF-16 码元：BMP 字符（<= U+FFFF）1 个，增补平面
 * （代理对，41448 大字表里的扩展区生僻字）2 个。撤销时 insertText 要删的
 * 就是这个数。只数首字节（0x80-0xBF 是续接字节，跳过）。 */
static int cj_utf8_to_utf16_units(const char *utf8, int len) {
    int units = 0;
    for (int i = 0; i < len; ) {
        unsigned char c = (unsigned char)utf8[i];
        int seq;
        if (c < 0x80) {
            seq = 1;
        } else if ((c & 0xE0) == 0xC0) {
            seq = 2;
        } else if ((c & 0xF0) == 0xE0) {
            seq = 3;
        } else if ((c & 0xF8) == 0xF0) {
            seq = 4;
            units++; /* 4 字节 UTF-8 → 非 BMP → 代理对，多占 1 个码元 */
        } else {
            seq = 1; /* 非法字节，当 1 字节容错，不卡死 */
        }
        units++;
        i += seq;
    }
    return units;
}

/* 缓冲区最后一次被修改（追加字母/退格/Step V 翻页）的时间戳。
 *
 * 历史（Phase C 真机 bug 修复后废弃了它的原用途）：早先这里配一个 10 秒
 * 空闲计时器（CJ_PINYIN_IDLE_TIMEOUT_SEC），跟 KeyPopup 原生 autoHideTimeout
 * 对齐——停顿 10 秒就静默清空缓冲区。但 Step W 已经彻底禁掉了原生自动隐藏、
 * 让候选栏"只要有候选就一直显示，直到真正提交/放弃/切换语言"；这个 10 秒
 * 静默清空跟那个目标直接矛盾：用户打完 "pin" 停一会儿再按空格，这次空格
 * 先触发过期把 "pin" 悄悄丢了，空格分支看到空缓冲区就什么都不提交（真机
 * 报出来的"停留一段时间再按没有字上屏"）。所以空闲过期整个去掉了，
 * composition 现在真正持续到显式操作（提交/回车/退格清空/焦点切换/切语言）。
 * 时间戳本身保留（写而不读，几处仍在更新），留作以后可能的按键节流/诊断
 * 用途，不再驱动任何自动清空。 */
static time_t g_pinyin_buffer_last_activity = 0;

/* Step V：空格/回车提交要用的"当前候选栏第一名真候选"缓存（UTF-8 字节，
 * 从 mmap 的 dict.bin 拷贝出来，不持有指针引用）。声明放在这里（跟
 * g_pinyin_buffer 一起）是因为 cj_step_t_expire_buffer_if_stale 马上要用。
 * len==0 表示当前没有真候选（缓冲区空，或者词典没命中、候选栏正展示
 * 拼音切分兜底），这时候空格/回车应该走"提交原样拼音"的 Step T 老路径。 */
#define CJ_STEP_V_TOP_CANDIDATE_CAP 64
static char g_step_v_top_candidate_utf8[CJ_STEP_V_TOP_CANDIDATE_CAP];
static int g_step_v_top_candidate_len = 0;

/* Step V：候选翻页（多缓存、按页切片、末尾 ">>" 翻页标记，点标记切页不提交，拦截见 cj_vk_insert_text_handler）——详见白皮书 02 节 Step V。 */
#define CJ_STEP_V_PAGE_MARKER ">>"
#define CJ_STEP_V_PAGE_MARKER_LEN 2
/* Step V-2：延迟刷新专用标记，区别于上面用户点击触发的翻页标记——这个
 * 是我们自己代码用 QueuedConnection 排队触发的，不是真人点击，语义也
 * 不一样（只重新展示当前页，不翻页）。声明放在这里（跟 PAGE_MARKER
 * 一起）是因为 cj_vk_insert_text_handler 马上要用，完整设计说明见文件
 * 后面 Step V-2 代码段。 */
#define CJ_STEP_V_REFRESH_MARKER "\x01"
#define CJ_STEP_V_REFRESH_MARKER_LEN 1
/* Phase A（A-ii 分段提交）：候选点选标记——CjCandidateBar 的 delegate 点了
 * 之后不再传候选文字本身，而是传 "\x02<十进制索引>"（STX 控制符 + 缓存
 * 数组下标）。C 端 cj_vk_insert_text_handler 解码出索引，查 consumed 长度，
 * 提交候选文字 + 从缓冲区裁掉已消耗前缀 + 对剩余重新切分，实现逐字造句。
 * 用 0x02 是因为它不可能出现在真候选文字（真汉字）或翻页/刷新标记里。 */
#define CJ_STEP_A_INDEX_MARKER_CH 0x02
/* 每页展示的真候选个数——双拼叠影排查时从 8 降到 5（单行安全值，跟拼音切分兜底路径统一，避免行数横跳）。详见白皮书 02 节候选栏叠影排查。 */
#define CJ_STEP_V_PAGE_SIZE 5
#define CJ_STEP_V_CACHE_CAP 64   /* 一次查询缓存的候选总数上限——8 页，够用；候选数比这个还多的极端音节，翻到第 8 页之后的字选不到，是这一版的已知边界，不是 bug */
static CjDictCandidate g_step_v_cached_candidates[CJ_STEP_V_CACHE_CAP];
/* Phase A（逐字候选+分段提交）：跟 g_step_v_cached_candidates 平行的数组，
 * 记录每个候选点选提交后应该从 g_pinyin_buffer 裁掉多少个原始按键字节
 * （consumed bytes）——首音节单字候选只消耗它对应音节那几个字节、整段
 * 词消耗全部、前缀预测/贪心拼句消耗整串。A-ii 的分段提交靠它裁缓冲区、
 * 对剩余部分重新切分，实现"点第一个字、剩下的继续选"的 iOS 逐字造句。 */
static int g_step_v_cached_consumed[CJ_STEP_V_CACHE_CAP];
/* Step FF（用户词频）：再一条平行数组——记录产生每个候选的词典查询 key
 * 原文（全拼="ni hao"/前缀预测="ni h"/简拼=原始字母），提交时用它记
 * (key, word) 计数。key_len==0 表示"这个候选不参与调频"（英文候选、
 * pending 态的贪心拼句等）。生成函数们只往 g_step_v_cached_* 这一组
 * 全局缓存里填（refresh_candidates 是唯一填充链），所以跟 consumed 一样
 * 按下标平行维护，不改函数签名。 */
static char g_step_v_cached_key[CJ_STEP_V_CACHE_CAP][CJ_UF_KEY_MAX];
static uint8_t g_step_v_cached_key_len[CJ_STEP_V_CACHE_CAP];
static int g_step_v_cached_count = 0;
static int g_step_v_current_page = 0;

static void cj_step_v_set_cached_key(int idx, const char *key, size_t key_len) {
    if (idx < 0 || idx >= CJ_STEP_V_CACHE_CAP) {
        return;
    }
    if (!key || key_len == 0 || key_len > CJ_UF_KEY_MAX) {
        g_step_v_cached_key_len[idx] = 0; /* 不参与调频 */
        return;
    }
    memcpy(g_step_v_cached_key[idx], key, key_len);
    g_step_v_cached_key_len[idx] = (uint8_t)key_len;
}

/* Step EE：候选栏下方"预览行"——用户要求能看到自己实际打的拼音字母，
 * 双拼要显示自动转换后的拼音（不是双拼按键原文）。缓存这份文本（而不是
 * 每次显示都重新算）是因为 Step V-2 的延迟刷新（cj_vk_insert_text_handler
 * 里补的那次"重新展示"）、翻页点击等几处调用点手上没有 seg_src 可用，
 * 只能读缓存——跟 g_step_v_cached_candidates 是同一个道理。 */
static char g_step_v_preview_text[CJ_SEG_MAX_INPUT + 1];
static int g_step_v_preview_len = 0;

/* Step T：前向声明——这几个函数的真正定义在文件后面（依赖 Step P/Q 已经
 * 建好的 KeyPopup 写入基础设施，那部分定义顺序更靠后），下面的
 * cj_vk_insert_text_handler（Step V 缓冲区同步逻辑）和 triggerBackspace
 * hook 都提前需要用到。用跟 Step S 诊断阶段同样的手法（当时给
 * cj_log_qstring_preview 加过一次前向声明），不是新模式，见头文件顶部
 * Step T 说明。 */
static int cj_step_t_is_chinese_mode(void *this_ptr);
static void cj_step_t_refresh_candidates(void *this_ptr);
/* Phase A（A-ii）：按缓存下标提交候选——真正定义在文件后面（跟
 * cj_step_t_commit_buffer 一起），cj_vk_insert_text_handler 解码点选索引后
 * 要调它，所以提前声明。 */
static void cj_step_t_commit_candidate_by_index(void *this_ptr, int idx);
#ifdef CJ_COMMIT_VIA_IME
static int cj_commit_via_ime(const void *qstr_ref, int replace_from, int replace_len); /* B：前向声明,commit_buffer 早于定义处 */
#endif
/* 05 节繁体双 blob：跟 cj_step_t_is_chinese_mode 共用同一次
 * virtualKeyboard.language 属性读取，真正定义见下面 cj_step_t_language_mode
 * 附近说明。前向声明原因跟上面这几个一样——cj_dict_select_handle_for_mode（马上要
 * 定义，给 Step V 词典查询选 handle 用）比这个函数的真正定义位置更靠前。
 * CjLanguageMode 枚举本身已随 Phase B 前倒到 Step R 真源表处定义。 */
static CjLanguageMode cj_step_t_language_mode(void *this_ptr);
/* Phase B：一次读取 language 属性同时解析简/繁 base 与双拼位，供
 * cj_step_t_refresh_candidates 用；cj_step_t_language_mode 是传 NULL 的薄封装。 */
static CjLanguageMode cj_step_t_language_mode_ex(void *this_ptr, int *out_shuangpin);
/* Step V：is_utf8 选 candidates 里每个元素的编码——0=ASCII 拼音字母（零
 * 扩展，Step T 原有用法），1=UTF-8 真候选词（Step V 新增，词典查询结果，
 * 需要真正的 UTF-8->UTF-16 解码，不能零扩展）。count==0 时这个参数不影响
 * 结果（两条路径对空列表的处理一样），调用方随便传都行。 */
static void cj_step_t_update_popup(void *this_ptr, const char *const *candidates, const long long *lens, int count,
                                    int visible, int is_utf8);
/* Step V：从 g_step_v_cached_candidates 按 g_step_v_current_page 截取一页
 * 刷进候选栏，真正定义在文件后面（Step V 大段代码，靠后）。this_ptr
 * （Step AA 新增）只用来读 virtualKeyboard 宽度算 targetRect，NULL 时
 * 退化用旧的固定经验值，见 cj_step_t_compute_target_rect。 */
static void cj_step_v_display_page(void *this_ptr);
/* Step V-2：给自己排一个延迟的 insertText 调用，用来在原生代码把
 * KeyPopup 关掉之后重新打开它——真正定义在文件后面（Step V-2 代码段）。 */
static void cj_step_v2_queue_refresh(void *this_ptr);

/* A-iv（焦点守卫，2026-08-22 修 bug）：composition（拼音缓冲区+撤销栈）
 * 语义上属于"某一个输入框"，但状态是全局单例——焦点切走后必须整体
 * finalize，否则陈旧状态会泄漏进新输入框（真机踩过：笔记里"你好"空格
 * 上屏→关笔记→搜索框按退格→候选栏弹回"你好"、还往搜索框发删码元）。
 * 在退格拦截和按键分发两个入口各调一次：焦点没变/无陈旧状态时一次
 * focusObject() 调用即返回；fo==NULL（拿不到焦点）按"变了"处理——宁可
 * 少一次撤销（良性），不冒往错误输入框删码元的险。 */
static void cj_ime_focus_guard(void *this_ptr) {
    if (g_pinyin_buffer_len == 0 && g_compose_stack_len == 0) {
        return;
    }
    void *fo = cj_current_focus_object();
    if (fo && fo == g_ime_focus_obj) {
        return;
    }
    CJ_LOG("[cangjie] A-iv：焦点已切换（%p→%p），finalize 陈旧 composition\n",
           g_ime_focus_obj, fo);
    g_pinyin_buffer_len = 0;
    g_step_v_top_candidate_len = 0;
    g_step_v_cached_count = 0;
    cj_compose_stack_reset();
    cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
}

static void cj_vk_insert_text_handler(void *this_ptr, const void *text_qstring,
                                       int replace_from, int replace_length) {
    CJ_LOG("[cangjie] Step L：insertText 实现被调用，this=%p replaceFrom=%d replaceLength=%d\n",
           this_ptr, replace_from, replace_length);
    cj_log_qstring_preview("text", text_qstring);

    /* Step V-2：我们自己排队的延迟刷新调用到达——这不是用户操作，是
     * cj_step_v2_queue_refresh() 之前用 QueuedConnection 排的队，此刻
     * 原生 KeyPopup.qml 第 117 行 root.visible=false 早就执行完了（这次
     * 调用根本不在那次 onReleased 的调用链里），现在重新写一遍
     * visible=true 不会再被谁追着覆盖。只重新展示当前页，不改缓冲区/
     * 页码状态，也不 call-through（不真的插入这个控制字符）。 */
    if (text_qstring &&
        cj_qstring_equals_ascii((const uint8_t *)text_qstring, CJ_STEP_V_REFRESH_MARKER, CJ_STEP_V_REFRESH_MARKER_LEN)) {
        CJ_LOG("[cangjie] Step V-2：延迟刷新调用到达，重新展示候选栏（不改缓冲区/页码）\n");
        if (g_step_v_cached_count > 0) {
            cj_step_v_display_page(this_ptr);
        }
        return;
    }

    /* Phase A（A-ii）：候选点选——CjCandidateBar delegate 传来 "\x02<十进制
     * 索引>"（见 CJ_STEP_A_INDEX_MARKER_CH 说明）。解码出缓存下标，走分段
     * 提交（提交候选文字 + 裁缓冲区 + 对剩余重新切分），不 call-through
     * （不真的插入这个标记字符串）。QString 布局：ptr@+8、size@+16、
     * 每个字符 uint16。 */
    if (text_qstring) {
        void *idx_ptr = NULL;
        long long idx_size = 0;
        memcpy(&idx_ptr, (const uint8_t *)text_qstring + 8, sizeof(void *));
        memcpy(&idx_size, (const uint8_t *)text_qstring + 16, sizeof(long long));
        if (idx_ptr && idx_size >= 2) {
            const uint16_t *chars = (const uint16_t *)idx_ptr;
            if (chars[0] == CJ_STEP_A_INDEX_MARKER_CH) {
                int idx = 0;
                int ok = 1;
                for (long long i = 1; i < idx_size; i++) {
                    if (chars[i] < (uint16_t)'0' || chars[i] > (uint16_t)'9') {
                        ok = 0;
                        break;
                    }
                    idx = idx * 10 + (int)(chars[i] - (uint16_t)'0');
                }
                if (ok) {
                    CJ_LOG("[cangjie] Phase A：候选点选，索引=%d（分段提交）\n", idx);
                    cj_step_t_commit_candidate_by_index(this_ptr, idx);
                    return;
                }
            }
        }
    }

    /* Step V：候选翻页标记被点击——不是真的要往文本框插入内容，是"翻到
     * 下一页"的信号。判断依据：插入文本内容精确等于我们自己放进候选栏的
     * 翻页标记 ">>"（真词典候选全部是真汉字，不会跟这个 ASCII 符号撞）。
     * 命中时：翻页（翻到底了绕回第一页）、重新刷新候选栏，直接 return——
     * 不 call-through（不真的插入 ">>" 这两个字符），不做下面的缓冲区
     * 同步逻辑（缓冲区/候选缓存都要原样保留，用户还在选同一次输入的
     * 候选，不是提交完了）。 */
    if (g_pinyin_buffer_len > 0 && text_qstring &&
        cj_qstring_equals_ascii((const uint8_t *)text_qstring, CJ_STEP_V_PAGE_MARKER, CJ_STEP_V_PAGE_MARKER_LEN)) {
        /* 🔴 真机 bug：翻页点击走 cj_vk_insert_text_handler、不更新活跃时间戳，被闲置超时误清空——翻页也算活跃、在这里同步刷新时间戳。详见白皮书 02 节 Step V。 */
        g_pinyin_buffer_last_activity = time(NULL);
        g_step_v_current_page++;
        if (g_step_v_current_page * CJ_STEP_V_PAGE_SIZE >= g_step_v_cached_count) {
            g_step_v_current_page = 0; /* 翻到底了，绕回第一页，不是卡死在最后一页 */
        }
        CJ_LOG("[cangjie] Step V：翻页标记被点击，切到第 %d 页（共缓存 %d 个候选）\n",
               g_step_v_current_page + 1, g_step_v_cached_count);
        cj_step_v_display_page(this_ptr);
        /* Step V-2：这次同步写入马上会被原生 root.visible=false 覆盖
         * （见文件后面 Step V-2 代码段的完整说明），排一个延迟调用在那
         * 之后重新展示一遍，才是真正让候选栏在翻页后保持可见的关键。 */
        cj_step_v2_queue_refresh(this_ptr);
        return;
    }

    /* Step V：外部 insertText（如候选点选）与拼音缓冲区同步——我们自己的提交走 trampoline 不回这个入口，故能走到这里的都是外部调用；缓冲区非空即清空+隐藏候选栏重新同步（fail-safe）。详见白皮书 02 节 Step V。 */
    if (g_pinyin_buffer_len > 0) {
        CJ_LOG("[cangjie] Step V：insertText 被外部调用时拼音缓冲区非空（内容=\"%.*s\"），"
               "判定为候选点选提交或其它外部提交，清空缓冲区+隐藏候选栏同步状态\n",
               g_pinyin_buffer_len, g_pinyin_buffer);
        g_pinyin_buffer_len = 0;
        g_step_v_top_candidate_len = 0;
        g_step_v_cached_count = 0;
        cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
    }
    /* A-iii：能走到这里的都是真正的外部 insertText（我们自己的提交/撤销走
     * trampoline 不回这个入口，候选点选/翻页/延迟刷新标记都在上面提前
     * return 了）——外部往文本框写了内容，说明 composition 上下文已经变了，
     * finalize 撤销栈（哪怕缓冲区本来就空、栈里还留着刚点完的字）。 */
    cj_compose_stack_reset();

    /* 无条件原样转调——这次调用本身的内容我们不修改，只是顺带同步了缓冲区
     * 状态，见上面 Step V 说明；原始的"不改变行为"取舍见文件头 Step L 说明。 */
    if (g_orig_vk_insert_text_call_through) {
        g_orig_vk_insert_text_call_through(this_ptr, text_qstring, replace_from, replace_length);
    } else {
        CJ_LOG("[cangjie] 严重：insertText 调用桩未初始化，跳过调用（不应该发生）\n");
    }
}

static void cj_vk_trigger_backspace_handler(void *this_ptr) {
    CJ_LOG("[cangjie] Step L：triggerBackspace 实现被调用，this=%p\n", this_ptr);

    /* Step T：中文模式且缓冲区非空时，退格只回退缓冲区里打到一半的拼音
     * 字母，不删除已经提交到文本框的内容——反编译确认 FUN_00697610 的
     * type==2 分支就是直接调用这个函数（本文件顶部 Step T 大段注释有完整
     * 证据），在这里拦截等价于在按键分发函数里拦截，不用改那边的控制流。 */
    /* A-iii：退格从右往左剥整个 composition。只在"有东西可撤"（缓冲区非空
     * 或撤销栈非空）且中文模式时拦截，读一次 language 属性。 */
    /* A-iv（焦点守卫）：陈旧 composition 先 finalize——守卫命中后下面的
     * 拦截条件自然不成立，退格落到原生 call-through 在新输入框正常删字。 */
    cj_ime_focus_guard(this_ptr);

    if ((g_pinyin_buffer_len > 0 || g_compose_stack_len > 0) && cj_step_t_is_chinese_mode(this_ptr)) {
        if (g_pinyin_buffer_len > 0) {
            /* 缓冲区还有正在打的拼音 → 删最后一个字母（Step T 原有行为）。 */
            g_pinyin_buffer_len--;
            g_pinyin_buffer_last_activity = time(NULL);
            CJ_LOG("[cangjie] Step T：退格吸收，缓冲区回退到 \"%.*s\"（跳过原生退格，不删除已提交文本）\n",
                   g_pinyin_buffer_len, g_pinyin_buffer);
            cj_step_t_refresh_candidates(this_ptr);
            return; /* 跳过 call-through：不触发真实退格 */
        } else {
        /* 缓冲区空但撤销栈非空 → 撤销上一个已选词：从文本框删掉它、把它的
         * 拼音还回缓冲区、重新出候选，可重选（A-iii 的核心，逐字纠错）。
         * （A-iv：焦点守卫命中时不会进这里——上面第一分支已 finalize 并
         * 落到函数末尾的原生 call-through。） */
        CjComposeEntry e = g_compose_stack[--g_compose_stack_len];
        if (g_orig_vk_insert_text_call_through && cj_resolve_qt_symbols() && e.textfield_units > 0) {
            /* insertText 空串 + (replaceFrom=-units, replaceLength=units)：删掉
             * 光标前 units 个码元，跟原生长按删键 insertText("", -1, 1) 同一
             * 机制。空 QString 直接用清零的 24 字节 {d,ptr,size}=0（Qt 默认
             * 构造的空串就是这个，避开 QArrayData::allocate(0)==NULL 的坑）。 */
            uint8_t empty_qstr[24];
            memset(empty_qstr, 0, sizeof(empty_qstr));
#ifdef CJ_COMMIT_VIA_IME
            /* B：退格撤销的删除也走 commitString（空串 + replaceFrom/Length 删光标前
             * units 码元，标准 Qt IM 协议 QInputMethodEvent 支持），失败回退 insertText。
             * 注意:原生退格走 triggerBackspace(FUN_00695820) 而非 commitString-replace,
             * 故此路真机验证过才算数(见 device-bare-state 记忆)。 */
            if (!cj_commit_via_ime(empty_qstr, -e.textfield_units, e.textfield_units))
                g_orig_vk_insert_text_call_through(this_ptr, empty_qstr, -e.textfield_units, e.textfield_units);
#else
            g_orig_vk_insert_text_call_through(this_ptr, empty_qstr, -e.textfield_units, e.textfield_units);
#endif
        }
        int restore = e.pinyin_len;
        if (restore > CJ_SEG_MAX_INPUT) {
            restore = CJ_SEG_MAX_INPUT; /* 防御，正常不会（pinyin 本就是former缓冲区前缀） */
        }
        memcpy(g_pinyin_buffer, e.pinyin, (size_t)restore); /* 缓冲区此刻为空，直接铺回 */
        g_pinyin_buffer_len = restore;
        g_pinyin_buffer_last_activity = time(NULL);
        CJ_LOG("[cangjie] A-iii：退格撤销已选词（删 %d 个码元），还原拼音 \"%.*s\"，重新出候选\n",
               e.textfield_units, g_pinyin_buffer_len, g_pinyin_buffer);
        cj_step_t_refresh_candidates(this_ptr);
        return;
        }
    }

    if (g_orig_vk_trigger_backspace_call_through) {
        g_orig_vk_trigger_backspace_call_through(this_ptr);
    } else {
        CJ_LOG("[cangjie] 严重：triggerBackspace 调用桩未初始化，跳过调用（不应该发生）\n");
    }
}

static void cj_install_virtualkeyboard_insert_hook(void) {
    uintptr_t target = g_addr_vk_inserttext;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_vk_insert_text_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step L：insertText 实现 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_vk_insert_text_call_through = (orig_vk_insert_text_fn_t)stub;
    fprintf(stderr,
            "[cangjie] Step L：insertText 实现 hook 安装完成 @ %p（只打日志，不改行为）\n",
            (void *)target);
}

static void cj_install_virtualkeyboard_triggerbackspace_hook(void) {
    uintptr_t target = g_addr_vk_triggerbackspace;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_vk_trigger_backspace_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step L：triggerBackspace 实现 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_vk_trigger_backspace_call_through = (orig_vk_trigger_backspace_fn_t)stub;
    fprintf(stderr,
            "[cangjie] Step L：triggerBackspace 实现 hook 安装完成 @ %p（只打日志，不改行为）\n",
            (void *)target);
}

/* =====================================================================
 * Step M（真机验证 Step L 之后追加）：Step L 装完之后真机测试发现
 * triggerBackspace 命中了，insertText 敲字全程一次没命中——顺着 VirtualKeyboard
 * 覆盖的 QQuickItem 触摸/鼠标虚函数（touchEvent=FUN_00698270、
 * mousePressEvent=FUN_00698bb0，vtable slot 33/26，跟 event/eventFilter 等
 * QObject 基类虚函数是同一批手法找到的）往下追，两者命中键位之后都会调用同一个
 * 分发函数 FUN_00697610(this, key指针, isPress)——反编译确认按键 type 字段
 * （key指针+0x70 处的 int，跟 QML 里 VirtualKeyboardKey.Shift 这种用法对应的
 * 同一个枚举）在这里分支：0=普通字符键/1=空格 直接内联构造
 * QInputMethodEvent+setCommitString+sendEvent（不经过公开的 insertText()，
 * 这就是 Step L 打普通字母没命中的真正原因）；2=退格 调用
 * FUN_00695820（也就是 Step L 已经在真机验证过命中的 triggerBackspace 实现，
 * 印证了这条链路的正确性）；3/4/6 是 CapsLock/符号键盘切换/Shift。
 *
 * 这才是 M3 真正应该 hook 的点——不管触摸还是鼠标触发，所有按键最终都走这里，
 * 是唯一、完整覆盖普通字符输入的入口。这一版仍然只做验证：无条件原样转调，
 * 只读打印 this/key 指针/isPress/type 字段，不解析 key 结构体里的具体文本
 * （文本字段随 Shift/符号态在结构体里的偏移会变，Step K/L 阶段没必要啃透，
 * 等确认这里确实是真机上"每次点键都会经过"的点之后，下一轮再深入）。函数开头
 * 20 字节（paciasp/stp/mov x29,sp/stp/mov x20,x1）纯寄存器栈操作，无 PC 相关
 * 寻址，用同一套 trampoline 机制安全打 patch。
 * ===================================================================== */

#define VK_KEYHANDLER_VADDR 0x697610UL /* FUN_00697610，触摸/点击命中按键后的统一分发函数 */

typedef long (*orig_vk_keyhandler_fn_t)(void *this_ptr, void *key_ptr, int is_press);
static orig_vk_keyhandler_fn_t g_orig_vk_keyhandler_call_through = NULL;

/* Step N：字符键文本是指针不是内嵌 QString——普通态 key_ptr+0x08（有效性看 +0x10）、shift 态 +0x20（看 +0x28）、数字覆盖态 +0x38（看 +0x40）；三个候选全打出来靠真机对照验证。反汇编交叉核实过程详见白皮书 02 节 Step N。 */
static void cj_log_vk_key_text_candidates(const void *key_ptr) {
    struct { const char *label; long ptr_off; long guard_off; } candidates[] = {
        { "普通态(+0x08)", 0x08, 0x10 },
        { "shift态(+0x20)", 0x20, 0x28 },
        { "数字/符号态(+0x38)", 0x38, 0x40 },
    };
    const uint8_t *base = (const uint8_t *)key_ptr;
    for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]); i++) {
        long guard = 0;
        memcpy(&guard, base + candidates[i].guard_off, sizeof(long));
        if (guard == 0) {
            CJ_LOG("[cangjie] Step N：  候选[%s] guard=0，视为无效，跳过\n", candidates[i].label);
            continue;
        }
        void *text_ptr = NULL;
        memcpy(&text_ptr, base + candidates[i].ptr_off, sizeof(void *));
        if (!text_ptr) {
            CJ_LOG("[cangjie] Step N：  候选[%s] guard=%ld 但指针为空，跳过\n", candidates[i].label, guard);
            continue;
        }
        cj_log_qstring_preview(candidates[i].label, text_ptr);
    }
}

/* =====================================================================
 * Step O：M4 前置侦查——KeyPopup 复用方案真机验证第一步："怎么拿到这个
 * QML 运行时动态生成的实例指针"。
 *
 * 第一次尝试：QObject::children()（QObject 归属树）——`QObject::children()`
 * 这个 Qt 6.10 构建里被内联掉了，没有独立导出符号，改用 xochitl 自己代码
 * 里真实调用过的 Qt 内部辅助函数 qt_qFindChildren_helper（反编译
 * FUN_007f1b20 这个真实调用点确认了参数编码）。真机部署后：符号解析
 * 成功、调用没有报错，但 <strong>返回 0 个子对象</strong>——排查后确认是
 * 假设本身错了：QML 里 `parent: virtualKeyboard` 这种写法控制的是
 * `QQuickItem` 的**视觉父级**（渲染树），跟 `QObject::parent()`（内存
 * 归属树，`children()`/`findChildren` 遍历的正是这棵树）是两回事——
 * `keyPopup` 在 QML 源码里实际嵌套声明在 `keyboardLayout` 内部，它真正的
 * QObject 归属父级大概率是 `keyboardLayout` 或更上层，不是
 * `virtualKeyboard`，所以从 `virtualKeyboard` 这个指针出发遍历 QObject
 * 树找不到它。
 *
 * 第二次尝试（当前这版）：改用 `QQuickItem::childItems()`——这是视觉树
 * 自己的访问器，才真正对应 QML `parent:` 属性的语义。真机上 `nm` 核实
 * 过 `libQt6Quick.so.6.10.3` 确认它是导出符号（非虚函数，直接 dlsym，
 * 不需要 vtable 查找）：`QList<QQuickItem*> childItems() const`。返回值
 * 是"按值返回的 QList"，在 AArch64 ABI 下超过 16 字节的聚合类型走
 * "隐藏指针"约定（调用方分配好返回值空间的地址放 X8 寄存器，编译器
 * 自动处理）——这里没有手写汇编去摆弄 X8，而是让 C 编译器自己按 ABI
 * 规则生成调用代码：只要把函数指针类型声明的"返回值"设成一个 24 字节的
 * 普通 C 结构体（跟 QString/QStringList 那套 {d,ptr,size} 布局一样大），
 * GCC/Clang 在 aarch64 上对"返回值超过 16 字节的结构体"这条 ABI 规则
 * 是 C/C++ 通用的，不需要知道调用的是 C++ 函数还是 C 函数，两边编译器
 * 生成的调用约定必须一致才能在同一个进程里协同工作，这是硬件调用约定
 * （AAPCS64）保证的，不是这个具体 Qt 版本的实现细节，比之前反复出现的
 * "反编译器把参数个数/偏移算错"那类风险更可控。
 *
 * className() 走跟 Step O 第一版一样的公开符号 `QMetaObject::className()`，
 * metaObject() 依然是虚函数，用跟 Step K/M 反复验证过的技巧（vtable[0]）
 * 调用。这一版仍然只做只读遍历+打印子项类名，不读写任何属性。 */

typedef struct { void *d; void *ptr; long long size; } CjPtrList24;
typedef CjPtrList24 (*qquickitem_childitems_fn)(const void *this_ptr);
typedef const char *(*qmetaobject_classname_fn)(const void *metaobject);
typedef const void *(*generic_metaobject_getter_fn)(void *this_ptr);
/* Step Q 追加（用户要求候选框贴着收键盘按钮定位，需要真实几何数据，不能
 * 瞎猜坐标）：QQuickItem::x()/y()/width()/height() 都是简单的 const 方法，
 * 返回单个 double，真机 nm 核实过是导出符号——用同一个函数指针类型即可，
 * 不需要为每个方法单独定义类型。 */
typedef double (*qquickitem_getter_fn)(const void *this_ptr);
/* Step BB：QMLDiff 注入的 CjCandidateBar 挂在 keyboardLayout 下面（不是
 * virtualKeyboard 下面——特意避开 virtualKeyboard 那套异步 Component.
 * onCompleted 重新挂载的时序坑，见 candidatebar.qmd 设计注释），
 * virtualKeyboard.childItems() 找不到它，得反过来先拿 virtualKeyboard
 * 自己的视觉父级（reparent 之后就是 keyboardLayout）再去它的 childItems()
 * 里找。QQuickItem::parentItem() const 是导出符号，真机 nm 核实过
 * （libQt6Quick.so.6.10.3），跟 childItems() 同一个函数指针形状（单个
 * this 参数、返回一个指针，不涉及 ABI 隐藏指针那套规则，比 childItems()
 * 返回值处理更简单）。 */
typedef void *(*qquickitem_parentitem_fn)(const void *this_ptr);
typedef char (*qquickitem_boolgetter_fn)(const void *this_ptr);

static qquickitem_childitems_fn g_qquickitem_childitems = NULL;
static qmetaobject_classname_fn g_qmetaobject_classname = NULL;
static qquickitem_getter_fn g_qquickitem_x = NULL;
static qquickitem_getter_fn g_qquickitem_y = NULL;
static qquickitem_getter_fn g_qquickitem_width = NULL;
static qquickitem_getter_fn g_qquickitem_height = NULL;
static qquickitem_parentitem_fn g_qquickitem_parentitem = NULL;
static qquickitem_boolgetter_fn g_qquickitem_isvisible = NULL;
static int g_children_search_symbols_attempted = 0;
static int g_children_search_symbols_ok = 0;

static int cj_resolve_children_search_symbols(void) {
    if (g_children_search_symbols_attempted) {
        return g_children_search_symbols_ok;
    }
    g_children_search_symbols_attempted = 1;

    g_qquickitem_childitems = (qquickitem_childitems_fn) dlsym(RTLD_DEFAULT,
        "_ZNK10QQuickItem10childItemsEv");
    g_qmetaobject_classname = (qmetaobject_classname_fn) dlsym(RTLD_DEFAULT, "_ZNK11QMetaObject9classNameEv");
    g_qquickitem_x = (qquickitem_getter_fn) dlsym(RTLD_DEFAULT, "_ZNK10QQuickItem1xEv");
    g_qquickitem_y = (qquickitem_getter_fn) dlsym(RTLD_DEFAULT, "_ZNK10QQuickItem1yEv");
    g_qquickitem_width = (qquickitem_getter_fn) dlsym(RTLD_DEFAULT, "_ZNK10QQuickItem5widthEv");
    g_qquickitem_height = (qquickitem_getter_fn) dlsym(RTLD_DEFAULT, "_ZNK10QQuickItem6heightEv");
    g_qquickitem_parentitem = (qquickitem_parentitem_fn) dlsym(RTLD_DEFAULT, "_ZNK10QQuickItem10parentItemEv");
    g_qquickitem_isvisible = (qquickitem_boolgetter_fn) dlsym(RTLD_DEFAULT, "_ZNK10QQuickItem9isVisibleEv");

    g_children_search_symbols_ok = g_qquickitem_childitems && g_qmetaobject_classname;
    CJ_LOG("[cangjie] Step O：符号解析%s（childItems=%p classname=%p x=%p y=%p width=%p height=%p parentItem=%p）\n",
           g_children_search_symbols_ok ? "成功" : "失败（safe mode，跳过遍历）",
           (void *)g_qquickitem_childitems, (void *)g_qmetaobject_classname,
           (void *)g_qquickitem_x, (void *)g_qquickitem_y,
           (void *)g_qquickitem_width, (void *)g_qquickitem_height, (void *)g_qquickitem_parentitem);
    return g_children_search_symbols_ok;
}

/* 只在真机上跑前几次——遍历本身是只读的，但避免每次敲字都做一遍这个
 * 相对更重的操作（涉及一次 Qt 内部函数调用+分配），够用来确认结论就
 * 收手，不是常驻诊断。 */
static int g_children_search_attempts = 0;
#define CJ_CHILDREN_SEARCH_MAX_ATTEMPTS 3

/* Step P：缓存第一次找到的 KeyPopup 指针，供后面 setProperty 测试用。QML
 * 运行时类型名带 "_QMLTYPE_<编号>" 后缀，只能前缀匹配，不能精确比较——见
 * 计划文件 Step O 记录。只存第一次找到的，不覆盖（同一个 xochitl 进程里
 * 这个实例应该是稳定的单例）。 */
static void *g_keypopup_ptr = NULL;
#define CJ_KEYPOPUP_CLASSNAME_PREFIX "KeyPopup"

static void cj_log_children_classnames(void *parent) {
    if (g_children_search_attempts >= CJ_CHILDREN_SEARCH_MAX_ATTEMPTS) {
        return;
    }
    if (!cj_resolve_children_search_symbols()) {
        return;
    }
    g_children_search_attempts++;

    CjPtrList24 result = g_qquickitem_childitems(parent);
    CJ_LOG("[cangjie] Step O：childItems 返回 %lld 个视觉子项（parent=%p）\n", result.size, parent);

    if (g_qquickitem_x && g_qquickitem_y && g_qquickitem_width && g_qquickitem_height) {
        CJ_LOG("[cangjie] Step Q：  parent 几何 = {x=%.1f y=%.1f w=%.1f h=%.1f}\n",
               g_qquickitem_x(parent), g_qquickitem_y(parent),
               g_qquickitem_width(parent), g_qquickitem_height(parent));
    }

    if (result.size < 0 || result.size > 200 || (result.size > 0 && result.ptr == NULL)) {
        CJ_LOG("[cangjie] Step O：size/ptr 看起来不合理，放弃遍历（不崩，只是跳过）\n");
        return;
    }

    void **arr = (void **)result.ptr;
    for (long long i = 0; i < result.size; i++) {
        void *child = arr[i];
        if (!child) {
            continue;
        }
        void **vtable = *(void ***)child;
        generic_metaobject_getter_fn get_mo = (generic_metaobject_getter_fn)vtable[0];
        const void *mo = get_mo(child);
        const char *classname = mo ? g_qmetaobject_classname(mo) : NULL;
        CJ_LOG("[cangjie] Step O：  子项[%lld] = %p 类名=%s\n", i, child, classname ? classname : "(null)");

        /* Step Q 追加：顺带打印每个子项自己的几何（相对 parent 的局部坐标，
         * 跟 targetRect 应该是同一套坐标系——之前观察到原生逻辑写的
         * targetRect y 值恒为 96、x 值随按键变化，量级上跟这里的子项坐标
         * 应该能对上，用来确认坐标系假设。 */
        if (g_qquickitem_x && g_qquickitem_y && g_qquickitem_width && g_qquickitem_height) {
            CJ_LOG("[cangjie] Step Q：    几何 = {x=%.1f y=%.1f w=%.1f h=%.1f}\n",
                   g_qquickitem_x(child), g_qquickitem_y(child),
                   g_qquickitem_width(child), g_qquickitem_height(child));
        }

        if (!g_keypopup_ptr && classname &&
            strncmp(classname, CJ_KEYPOPUP_CLASSNAME_PREFIX, sizeof(CJ_KEYPOPUP_CLASSNAME_PREFIX) - 1) == 0) {
            g_keypopup_ptr = child;
            CJ_LOG("[cangjie] Step P：缓存到 KeyPopup 指针 = %p（类名=%s）\n", child, classname);
        }
    }
}

/* =====================================================================
 * Step W：找 KeyPopup 内部自带的 autoHideTimeout 计时器——只读诊断，不写
 * 任何内存。
 *
 * 起因：候选栏在用户停顿超过 10 秒后被原生逻辑自动隐藏（KeyPopup.qml 里
 * `readonly property int autoHideTimeout: 10000` 背后必然有一个 QML Timer
 * 在计时），用户要求彻底消灭这个行为。最初考虑直接 hook
 * `QQuickItem::setVisible`（这个 Timer 到期最终就是靠它把 visible 置
 * false），但反汇编 libQt6Quick.so 发现这个库是用 BTI（Branch Target
 * Identification）编译的——`readelf -n` 确认 `AArch64 feature: BTI, PAC`，
 * `QQuickItem::setVisible` 函数开头是一条 `bti c` 落点指令，因为它是跨库
 * 导出符号，会被间接跳转（反汇编也证实它本身就是读虚表再 `br x16` 跳到
 * `QQuickItemPrivate::setVisible`）——覆盖掉这个落点会让所有从别处间接跳
 * 过来调用它的地方直接崩溃（Branch Target Exception），而这个函数在整个
 * 界面里任何控件的显隐变化都会经过，是全局性风险，跟之前 hook 的、只被
 * xochitl 自己代码直接调用（不需要 BTI 落点）的内部未导出函数完全不是
 * 一个风险量级。改成找到这个 Timer 对象本身，直接
 * `QObject::setProperty(timer, "running", false)` 停掉它——复用 Step P
 * 已经验证成熟的读写机制，不涉及任何二进制 patch，没有 BTI 问题。
 *
 * Step O 第一次尝试用 qt_qFindChildren_helper 从 virtualKeyboard 出发找
 * keyPopup，查错了对象（`parent: virtualKeyboard` 只建立视觉父级，
 * keyPopup 真正的 QObject 归属父级不是它）导致返回 0 个结果，后来改用
 * childItems()（视觉树）才找到 keyPopup 本身。这次反过来：Timer 不是
 * QQuickItem（没有可视化表现），不会出现在 childItems() 里，只能走
 * QObject 归属树——但这次从 keyPopup 自己出发查它的 QObject 子对象（不是
 * 从 virtualKeyboard 出发），父子关系是对的，理论上能找到 QML 里内部声明
 * 的 Timer。
 *
 * 精确签名（本机 Qt6.11 头文件 qobject.h 核对参数类型，跟设备 Qt6.10.3
 * 同大版本 ABI 稳定；真机 nm 核实 libQt6Core.so.6.10.3 存在这个导出符号）：
 *   void qt_qFindChildren_helper(const QObject *parent, QAnyStringView name,
 *                                 const QMetaObject &mo, QList<void*> *list,
 *                                 Qt::FindChildOptions options);
 * QAnyStringView 按值传参在 AArch64 ABI 下是 16 字节（两个寄存器字）。
 * "空名字"（不做名字过滤，匹配任意名字）对应的位模式
 * {0, 0x8000000000000000} 是 Step O 阶段已经真机验证过、调用不报错的
 * 编码（当时返回 0 个结果是因为查错了父对象，不是因为这个编码本身有问题
 * ——调用本身是良构的），这次原样复用，不重新猜测。metaObjectFilter 传
 * `&QObject::staticMetaObject`（本机 nm 核实的导出数据符号，不是函数）
 * 匹配"任意 QObject 派生类"，不预先假设 Timer 具体是哪个 C++ 类（QML
 * Timer 类型背后大概率是 QQmlTimer，但没必要提前假设，宽松过滤直接看真实
 * 类名更可靠）。options 传 1（头文件核实的 Qt::FindChildrenRecursively 值，
 * 不是猜的）——即使 Timer 不是 keyPopup 的直接子对象也能找到。
 *
 * classname 读取复用 Step O 已经验证过的手法（vtable[0] 是 QObject::
 * metaObject()，对任意 QObject 派生类都成立，Itanium C++ ABI 保证）。
 * 限流到最多 3 次，够确认结论，不常驻。
 * ===================================================================== */

typedef void (*qt_qfindchildren_helper_fn)(const void *parent, uint64_t name_word0, uint64_t name_word1,
                                            const void *metaobject_filter, void *out_list, int options);
static qt_qfindchildren_helper_fn g_qfindchildren_helper = NULL;
static const void *g_qobject_static_metaobject = NULL;
static int g_keypopup_children_search_attempted2 = 0;
static int g_keypopup_children_search_ok2 = 0;

static int cj_resolve_keypopup_children_search_symbols(void) {
    if (g_keypopup_children_search_attempted2) {
        return g_keypopup_children_search_ok2;
    }
    g_keypopup_children_search_attempted2 = 1;
    g_qfindchildren_helper = (qt_qfindchildren_helper_fn) dlsym(RTLD_DEFAULT,
        "_Z23qt_qFindChildren_helperPK7QObject14QAnyStringViewRK11QMetaObjectP5QListIPvE6QFlagsIN2Qt15FindChildOptionEE");
    g_qobject_static_metaobject = dlsym(RTLD_DEFAULT, "_ZN7QObject16staticMetaObjectE");
    g_keypopup_children_search_ok2 = g_qfindchildren_helper && g_qobject_static_metaobject
        && cj_resolve_children_search_symbols(); /* 复用 Step O 已解析的 classname 符号 */
    CJ_LOG("[cangjie] Step W：符号解析%s（helper=%p staticMetaObject=%p）\n",
           g_keypopup_children_search_ok2 ? "成功" : "失败（safe mode，跳过）",
           (void *)g_qfindchildren_helper, (const void *)g_qobject_static_metaobject);
    return g_keypopup_children_search_ok2;
}

static int g_keypopup_children_search_attempts2 = 0;
#define CJ_KEYPOPUP_CHILDREN_SEARCH_MAX_ATTEMPTS 3
static void *g_keypopup_autohide_timer_ptr = NULL;
/* QML Timer 类型的元对象类名/QML 运行时合成类名里大概率含 "Timer" 这个
 * 词——用宽松包含匹配（不是精确相等，参考 Step P 阶段"_QMLTYPE_<编号>"
 * 后缀的教训），真机日志会打印全部候选类名，最终认定哪个是它需要人工核实
 * 一次，这里只是"打个标记，供参考"，不代表已经确认。 */
#define CJ_TIMER_CLASSNAME_HINT "Timer"

/* Step BB 之后不再调用——CjCandidateBar 自己没有原生 autoHideTimeout，
 * 找 KeyPopup 内部 Timer 这件事本身也就没有意义了；标记 unused 保留
 * 完整的踩坑记录（BTI 风险分析等），不删除。 */
__attribute__((unused))
static void cj_log_keypopup_qobject_children(void) {
    if (!g_keypopup_ptr) return;
    if (g_keypopup_children_search_attempts2 >= CJ_KEYPOPUP_CHILDREN_SEARCH_MAX_ATTEMPTS) return;
    if (!cj_resolve_keypopup_children_search_symbols()) return;
    g_keypopup_children_search_attempts2++;

    CjPtrList24 result;
    memset(&result, 0, sizeof(result));
    g_qfindchildren_helper(g_keypopup_ptr, 0, 0x8000000000000000ULL,
                            g_qobject_static_metaobject, &result, 1 /* Qt::FindChildrenRecursively */);
    CJ_LOG("[cangjie] Step W：qFindChildren 返回 %lld 个 QObject 子项（parent=keyPopup=%p）\n",
           result.size, g_keypopup_ptr);

    if (result.size < 0 || result.size > 200 || (result.size > 0 && result.ptr == NULL)) {
        CJ_LOG("[cangjie] Step W：size/ptr 看起来不合理，放弃遍历（不崩，只是跳过）\n");
        return;
    }

    void **arr = (void **)result.ptr;
    for (long long i = 0; i < result.size; i++) {
        void *child = arr[i];
        if (!child) continue;
        void **vtable = *(void ***)child;
        generic_metaobject_getter_fn get_mo = (generic_metaobject_getter_fn)vtable[0];
        const void *mo = get_mo(child);
        const char *classname = mo ? g_qmetaobject_classname(mo) : NULL;
        CJ_LOG("[cangjie] Step W：  子项[%lld] = %p 类名=%s\n", i, child, classname ? classname : "(null)");

        if (!g_keypopup_autohide_timer_ptr && classname && strstr(classname, CJ_TIMER_CLASSNAME_HINT)) {
            g_keypopup_autohide_timer_ptr = child;
            CJ_LOG("[cangjie] Step W：疑似找到 autoHideTimeout 对应的 Timer 指针 = %p"
                   "（类名=%s，此函数只负责发现+核实，真正停用 running 在"
                   " cj_step_w_stop_autohide_timer 里）\n",
                   child, classname);
        }
    }
}

/* =====================================================================
 * Step P：往 KeyPopup.keys 属性写一个测试用的候选列表——第一次真正改变 UI
 * 状态的验证（此前 Step L-O 全部是只读观察/原样转调）。设计依据、符号来源
 * 见计划文件 imperative-petting-bentley.md Step P 段落，这里只放实现。
 *
 * 核心取舍：全部基于真机 nm 核实过存在的 Qt 导出符号，不猜测/不手写内存
 * 布局细节——QVariant(const QStringList&) 构造函数、~QVariant()、
 * QObject::setProperty/property 都是 Qt 公开、稳定导出的符号；QStringList
 * 内部 24 字节 {d,ptr,size} 布局复用 Step G 已经反复验证过的
 * cj_build_utf16_literal_qstring()。sizeof(QVariant)==32 字节这一点来自
 * 本机 Qt6.11 头文件（跟设备 Qt6.10.3 同大版本，ABI 稳定）逐字段核对，不是
 * 猜的——C 端只需要声明一个 32 字节的普通结构体类型当"QVariant"用，不需要
 * 解释内部字段，构造/析构都通过调用真实的 Qt 符号完成。
 *
 * 这里只保留符号解析部分（下面的 typedef/dlsym）——实际的写入触发逻辑
 * 已经被 Step Q 取代（Step P 的一次性写入被验证会被下一次按键覆盖，
 * 见计划文件 Step P 追加验证结果），本文件里搜 "Step Q" 看当前实现。 */

typedef struct { uint8_t raw[32]; } CjVariant32;

typedef void (*qvariant_ctor_stringlist_fn)(void *this_variant, const uint8_t *stringlist24);
typedef void (*qvariant_dtor_fn)(void *this_variant);
typedef unsigned char (*qobject_setproperty_fn)(void *obj, const char *name, const void *variant32);
typedef CjVariant32 (*qobject_property_fn)(const void *obj, const char *name);
typedef CjPtrList24 (*qvariant_tostringlist_fn)(const void *variant32); /* 复用 Step O 的 24 字节结构定义 */

static qvariant_ctor_stringlist_fn g_qvariant_ctor_stringlist = NULL;
static qvariant_dtor_fn g_qvariant_dtor = NULL;
static qobject_setproperty_fn g_qobject_setproperty = NULL;
static qobject_property_fn g_qobject_property = NULL;
static qvariant_tostringlist_fn g_qvariant_tostringlist = NULL;
static int g_keypopup_write_symbols_attempted = 0;
static int g_keypopup_write_symbols_ok = 0;

/* 追加（用户确认后）：visible/targetRect 写入用到的两个额外 QVariant 构造
 * 符号。QML `rect` 类型在 C++ 侧对应 QRectF——本机 Qt6.11 头文件
 * （qrect.h）核实过 QRectF 私有成员就是 `qreal xp,yp,w,h`（4 个 double，
 * 32 字节，无虚函数，Q_RELOCATABLE_TYPE），构造函数
 * `QRectF(qreal left, qreal top, qreal width, qreal height)` 顺序就是
 * x/y/width/height，跟 C 端声明的 CjRectF32 逐字段对应。部署前 objdump
 * 反汇编核实过：QRectF 全部由 4 个同类型 double 字段组成，属于 AAPCS64
 * 的"齐次浮点聚合"（HFA，≤4 个同类型成员），传参/返回值都走 d0-d3 寄存器，
 * **不是**走 >16 字节聚合默认的隐藏指针（X8）规则——这一点最初想当然按
 * "看大小"猜错了，靠反汇编纠正，不是靠记忆 ABI 细节。C 端只要把结构体
 * 类型声明对（4 个 double 字段），编译器会自动按同一套 HFA 规则处理，跟
 * Step O 的方法论一致（不用手写汇编，但必须部署前反汇编验证）。 */
typedef struct { double x, y, w, h; } CjRectF32;
typedef void (*qvariant_ctor_rectf_fn)(void *this_variant, CjRectF32 rect);
typedef void (*qvariant_ctor_bool_fn)(void *this_variant, bool value);
typedef bool (*qvariant_tobool_fn)(const void *variant32);
typedef CjRectF32 (*qvariant_torectf_fn)(const void *variant32);

static qvariant_ctor_rectf_fn g_qvariant_ctor_rectf = NULL;
static qvariant_ctor_bool_fn g_qvariant_ctor_bool = NULL;
static qvariant_tobool_fn g_qvariant_tobool = NULL;
static qvariant_torectf_fn g_qvariant_torectf = NULL;

/* Step T：判断 virtualKeyboard.language 是不是 zh_CN/zh_TW 需要的转换——
 * QVariant::toString() 返回 QString（24 字节，超过 16 字节走隐藏指针，跟
 * 上面 toStringList()/Step O 的 childItems() 是同一个 ABI 处理手法，直接
 * 复用 CjPtrList24 这个结构定义，反正都是 {d,ptr,size} 24 字节布局）。
 * 本机 nm 核实过设备真机拉下来的 libQt6Core.so.6.10.3 确认导出这个符号
 * （`_ZNK8QVariant8toStringEv`），不是猜的。 */
typedef CjPtrList24 (*qvariant_tostring_fn)(const void *variant32);
static qvariant_tostring_fn g_qvariant_tostring = NULL;

/* Step W：核实找到的 QQmlTimer 是不是 autoHideTimeout 那个——读它的
 * interval 属性，预期应该是 10000（跟 KeyPopup.autoHideTimeout 只读属性的
 * 声明值一致）。`int QVariant::toInt(bool *ok = nullptr) const`，本机 nm
 * 核实过设备 libQt6Core.so.6.10.3 导出这个符号，ok 传 NULL（不关心转换
 * 是否"精确"，这里只是核对读到的数值本身）。 */
typedef int (*qvariant_toint_fn)(const void *variant32, void *ok_out_or_null);
static qvariant_toint_fn g_qvariant_toint = NULL;

/* Step AA（已废弃，KeyPopup 时代）：targetRect 不控制 KeyPopup 可见宽度，改直接写 width/height 属性+读回核对——详见白皮书 02 节候选栏叠影排查。 */
typedef void (*qvariant_ctor_double_fn)(void *this_variant, double value);
static qvariant_ctor_double_fn g_qvariant_ctor_double = NULL;

/* Step EE：candidatebar.qmd 新增 previewText 属性（用户要求"候选栏下方
 * 显示实际打的拼音字母，双拼自动转成对应的拼音字母"）——这是单个
 * QString 属性，不是 QStringList（keys 那种），要用不同的 QVariant
 * 构造函数：`QVariant::QVariant(const QString&)`，参数是引用（传指针，
 * 不是按值传整个 24 字节结构体），本机拉取真机 libQt6Core.so.6.10.3
 * nm 核实存在这个符号，不是猜的。配合已有的 cj_build_ascii_qstring
 * （运行时 ASCII 内容构造 QString）就能拼出要写入的字符串。 */
typedef void (*qvariant_ctor_qstring_fn)(void *this_variant, const void *qstring_ref);
static qvariant_ctor_qstring_fn g_qvariant_ctor_qstring = NULL;

static int cj_resolve_keypopup_write_symbols(void) {
    if (g_keypopup_write_symbols_attempted) {
        return g_keypopup_write_symbols_ok;
    }
    g_keypopup_write_symbols_attempted = 1;

    g_qvariant_ctor_stringlist = (qvariant_ctor_stringlist_fn) dlsym(RTLD_DEFAULT,
        "_ZN8QVariantC1ERK5QListI7QStringE");
    g_qvariant_dtor = (qvariant_dtor_fn) dlsym(RTLD_DEFAULT, "_ZN8QVariantD1Ev");
    g_qobject_setproperty = (qobject_setproperty_fn) dlsym(RTLD_DEFAULT,
        "_ZN7QObject11setPropertyEPKcRK8QVariant");
    /* 读回旧值是锦上添花，不是必须——这两个符号缺失不阻止写入测试。 */
    g_qobject_property = (qobject_property_fn) dlsym(RTLD_DEFAULT, "_ZNK7QObject8propertyEPKc");
    g_qvariant_tostringlist = (qvariant_tostringlist_fn) dlsym(RTLD_DEFAULT, "_ZNK8QVariant12toStringListEv");
    /* visible/targetRect 是否可写不影响 keys 写入本身，缺失只跳过这两项。 */
    g_qvariant_ctor_rectf = (qvariant_ctor_rectf_fn) dlsym(RTLD_DEFAULT, "_ZN8QVariantC1E6QRectF");
    g_qvariant_ctor_bool = (qvariant_ctor_bool_fn) dlsym(RTLD_DEFAULT, "_ZN8QVariantC1Eb");
    /* 读回诊断用——排查"setProperty 返回 true 但屏幕没变化"是不是因为
     * QML 绑定表达式紧接着又把值算回去了（常见的 QML 陷阱：直接赋值只是
     * 临时覆盖绑定的求值结果，绑定依赖项一旦重新求值就会把值改回来）。 */
    g_qvariant_tobool = (qvariant_tobool_fn) dlsym(RTLD_DEFAULT, "_ZNK8QVariant6toBoolEv");
    g_qvariant_torectf = (qvariant_torectf_fn) dlsym(RTLD_DEFAULT, "_ZNK8QVariant7toRectFEv");
    /* Step T：language 属性判断要用，缺失只影响"能不能判断中文模式"，不
     * 影响 keys/visible/targetRect 写入本身，跟其它几个可选符号一个待遇。 */
    g_qvariant_tostring = (qvariant_tostring_fn) dlsym(RTLD_DEFAULT, "_ZNK8QVariant8toStringEv");
    /* Step W：核实 Timer 身份用，缺失只影响"能不能核对 interval"，不影响
     * keys/visible/targetRect 写入本身，跟其它几个可选符号一个待遇。 */
    g_qvariant_toint = (qvariant_toint_fn) dlsym(RTLD_DEFAULT, "_ZNK8QVariant5toIntEPb");
    /* Step AA：直接写 KeyPopup.width/height 用，缺失只影响"能不能撑宽
     * 候选栏"，不影响 keys/visible/targetRect 写入本身，跟其它几个可选
     * 符号一个待遇。 */
    g_qvariant_ctor_double = (qvariant_ctor_double_fn) dlsym(RTLD_DEFAULT, "_ZN8QVariantC1Ed");
    /* Step EE：写 previewText（单个 QString 属性）用，缺失只影响"能不能
     * 显示实际按键预览"，不影响 keys/visible 写入本身，跟其它几个可选
     * 符号一个待遇。 */
    g_qvariant_ctor_qstring = (qvariant_ctor_qstring_fn) dlsym(RTLD_DEFAULT, "_ZN8QVariantC1ERK7QString");

    g_keypopup_write_symbols_ok = g_qvariant_ctor_stringlist && g_qvariant_dtor && g_qobject_setproperty;
    CJ_LOG("[cangjie] Step P：写属性符号解析%s（ctor=%p dtor=%p setProperty=%p "
           "property=%p toStringList=%p ctor_rectf=%p ctor_bool=%p toString=%p toInt=%p ctor_double=%p）\n",
           g_keypopup_write_symbols_ok ? "成功" : "失败（safe mode，跳过写入）",
           (void *)g_qvariant_ctor_stringlist, (void *)g_qvariant_dtor,
           (void *)g_qobject_setproperty, (void *)g_qobject_property, (void *)g_qvariant_tostringlist,
           (void *)g_qvariant_ctor_rectf, (void *)g_qvariant_ctor_bool, (void *)g_qvariant_tostring,
           (void *)g_qvariant_toint, (void *)g_qvariant_ctor_double);
    return g_keypopup_write_symbols_ok;
}

/* ===================================================================
 * Step BB：找到 QMLDiff 注入的 CjCandidateBar 实例指针——跟 Step O/P
 * 找 KeyPopup 是同一大类问题（"怎么拿到 QML 运行时动态生成的实例"），
 * 但这次不能直接用 virtualKeyboard.childItems()：CjCandidateBar.qmd
 * 里特意没有写 `parent: virtualKeyboard`（早期几版这样写过，真机测试
 * 死活看不见任何内容，最后定位到是 virtualKeyboard 那套异步 Component.
 * onCompleted 里才重新挂载/铺满的时序坑——我们的兄弟节点如果显式锚定
 * 它的边，绑定时机对不上，元素要么零尺寸要么位置不对；换成不写 parent、
 * 直接用 QMLDiff INSERT 时的隐式默认父级（keyboardLayout，同步绑定的
 * width/height，不涉及那个坑）之后，真机验证一次成功）。
 *
 * 也就是说 CjCandidateBar 现在挂在 keyboardLayout 下面，是
 * virtualKeyboard 的"上级"（KeyboardPanel.qml 里 `virtualKeyboard.parent
 * = keyboardLayout` 这行代码本身就是把 virtualKeyboard 变成 keyboardLayout
 * 的子项），从 virtualKeyboard 出发用 childItems() 天然找不到它。反过来：
 * 先用 QQuickItem::parentItem()（真机 nm 核实的导出符号，见符号声明处
 * 说明）拿 virtualKeyboard 自己的视觉父级（reparent 之后就是
 * keyboardLayout），再对这个父级调用 childItems()，在返回的兄弟节点里
 * 按 objectName 匹配——不是按类名前缀匹配（CjCandidateBar 是普通
 * Rectangle，没有独立的 C++ 类名，QML `objectName: "CjCandidateBar"`
 * 是唯一稳定可读的身份标记，跟 Step O 用类名前缀匹配 KeyPopup 是同一个
 * "找到能唯一标识这个实例的属性"思路，只是这次换了一种属性）。
 *
 * 只存第一次找到的，不重复搜索（同一个 xochitl 进程里这个实例应该是
 * 稳定单例，跟 g_keypopup_ptr 同样的假设）。 */
static void *g_candidatebar_ptr = NULL;
#define CJ_CANDIDATEBAR_OBJECTNAME "CjCandidateBar"
static int g_candidatebar_search_attempts = 0;
#define CJ_CANDIDATEBAR_SEARCH_MAX_ATTEMPTS 5 /* 比 KeyPopup 的 3 次多留一点余量——
                                                  这条路径这次才第一次接入真机验证 */

static void cj_find_candidatebar(void *virtualkeyboard_this_ptr) {
    if (g_candidatebar_ptr) return;
    if (g_candidatebar_search_attempts >= CJ_CANDIDATEBAR_SEARCH_MAX_ATTEMPTS) return;
    if (!cj_resolve_children_search_symbols() || !g_qquickitem_parentitem) return;
    if (!cj_resolve_keypopup_write_symbols() || !g_qobject_property || !g_qvariant_tostring || !g_qvariant_dtor) return;
    g_candidatebar_search_attempts++;

    void *keyboard_layout = g_qquickitem_parentitem(virtualkeyboard_this_ptr);
    CJ_LOG("[cangjie] Step BB：virtualKeyboard.parentItem() = %p\n", keyboard_layout);
    if (!keyboard_layout) return;

    CjPtrList24 result = g_qquickitem_childitems(keyboard_layout);
    CJ_LOG("[cangjie] Step BB：keyboardLayout.childItems() 返回 %lld 个视觉子项\n", result.size);
    if (result.size < 0 || result.size > 200 || (result.size > 0 && result.ptr == NULL)) {
        CJ_LOG("[cangjie] Step BB：size/ptr 看起来不合理，放弃遍历（不崩，只是跳过）\n");
        return;
    }

    void **arr = (void **)result.ptr;
    for (long long i = 0; i < result.size; i++) {
        void *child = arr[i];
        if (!child) continue;

        CjVariant32 v = g_qobject_property(child, "objectName");
        CjPtrList24 name = g_qvariant_tostring(&v);
        g_qvariant_dtor(&v);

        if (cj_qstring_equals_ascii((const uint8_t *)&name, CJ_CANDIDATEBAR_OBJECTNAME,
                                     sizeof(CJ_CANDIDATEBAR_OBJECTNAME) - 1)) {
            g_candidatebar_ptr = child;
            CJ_LOG("[cangjie] Step BB：找到 CjCandidateBar 指针 = %p\n", child);
            break;
        }
    }
}
/* ================= Step BB 发现逻辑代码段结束 ================= */

/* Step W 续：核实 Timer 身份（interval≈10000）+ 每次候选栏更新都重新 assert running=false（不假设停一次永久生效），全程用 QObject::setProperty()、无二进制 patch、无 BTI 问题。详见白皮书 02 节 Step W。 */
static int g_keypopup_timer_interval_logged = 0;

/* Step BB 之后不再调用——见 cj_step_t_update_popup 顶部说明，
 * CjCandidateBar 没有原生自动隐藏行为需要对抗。标记 unused 保留记录。 */
__attribute__((unused))
static void cj_step_w_stop_autohide_timer(void) {
    if (!g_keypopup_autohide_timer_ptr) {
        return;
    }
    if (!cj_resolve_keypopup_write_symbols()) {
        return;
    }

    if (!g_keypopup_timer_interval_logged && g_qobject_property && g_qvariant_toint && g_qvariant_dtor) {
        g_keypopup_timer_interval_logged = 1;
        CjVariant32 v = g_qobject_property(g_keypopup_autohide_timer_ptr, "interval");
        int interval = g_qvariant_toint(&v, NULL);
        g_qvariant_dtor(&v);
        CJ_LOG("[cangjie] Step W：核对 Timer.interval = %d（预期约 10000 才是 autoHideTimeout 本尊，"
               "不是就说明找错了对象，需要回头重新排查）\n", interval);
    }

    if (g_qvariant_ctor_bool && g_qobject_setproperty) {
        CjVariant32 v;
        memset(&v, 0, sizeof(v));
        g_qvariant_ctor_bool(&v, false);
        unsigned char ok = g_qobject_setproperty(g_keypopup_autohide_timer_ptr, "running", &v);
        g_qvariant_dtor(&v);
        CJ_LOG("[cangjie] Step W：停用原生 autoHideTimeout Timer，running=false 写入%s\n",
               ok ? "成功" : "失败");
    }
}

/* Step P 的一次性写入函数（cj_test_keypopup_keys_write/
 * cj_test_keypopup_visibility_write）已被 Step Q 取代——Step P 追加验证
 * 发现写入会被下一次按键覆盖，Step Q 改成"call-through 之后写、连续多次
 * 按键都写"，验证了"写入时序"假设成立（结果见下面这个函数的调用历史）。
 * Step Q 自己的测试候选常量/一次性写入函数（cj_step_q_write_test_values，
 * 写死的"你好/拼音/测试"）在 Step T 落地后已经被真正的候选逻辑
 * （cj_step_t_refresh_candidates，见文件后面）取代并删除——两者都会在
 * call-through 之后写 KeyPopup.keys，留着 Step Q 那份只会互相打架，删掉。
 * 这里只保留还在用的通用读回函数：Step T 的诊断日志复用它。 */
static void cj_log_keypopup_all_properties(const char *when_label) {
    if (!g_keypopup_ptr || !g_qobject_property || !g_qvariant_dtor) {
        return;
    }
    CJ_LOG("[cangjie] Step Q：== 属性读回（%s） ==\n", when_label);

    if (g_qvariant_tostringlist) {
        CjVariant32 v = g_qobject_property(g_keypopup_ptr, "keys");
        CjPtrList24 list = g_qvariant_tostringlist(&v);
        CJ_LOG("[cangjie] Step Q：  keys 元素个数 = %lld\n", list.size);
        g_qvariant_dtor(&v);
    }
    if (g_qvariant_tobool) {
        CjVariant32 v = g_qobject_property(g_keypopup_ptr, "visible");
        bool visible = g_qvariant_tobool(&v);
        CJ_LOG("[cangjie] Step Q：  visible = %s\n", visible ? "true" : "false");
        g_qvariant_dtor(&v);
    }
    if (g_qvariant_torectf) {
        CjVariant32 v = g_qobject_property(g_keypopup_ptr, "targetRect");
        CjRectF32 r = g_qvariant_torectf(&v);
        CJ_LOG("[cangjie] Step Q：  targetRect = {x=%.1f y=%.1f w=%.1f h=%.1f}\n", r.x, r.y, r.w, r.h);
        g_qvariant_dtor(&v);
    }
}

/* =====================================================================
 * Step T：拼音缓冲状态机——最小可行试点。设计依据、拦截点选择的完整反编译
 * 证据见文件头的 Step T 段落 + 计划文件 imperative-petting-bentley.md
 * Step T（已知限制、取舍也写在那边，这里只放实现）。
 *
 * cj_step_t_is_chinese_mode/cj_step_t_refresh_candidates 的声明已经在上面
 * cj_vk_trigger_backspace_handler 之前出现过（前向声明，那边先用到）；
 * g_pinyin_buffer/g_pinyin_buffer_len 也已经在那里定义过，这里不重复定义。
 * ===================================================================== */

/* 判断 key_ptr 普通态（Step N 已确认的 key_ptr+0x08/+0x10）文本是不是
 * "恰好一个 a-z 小写字母"——拼音只关心这个，不关心 shift/大小写（shift 态
 * 文本在另一个偏移，我们压根不读，等于天然忽略大小写状态，这也是刻意的
 * 简化，见计划文件"已知限制"）。是则写入 *out_letter 并返回 1，否则返回 0，
 * 不修改 *out_letter。 */
static int cj_step_t_extract_single_letter(const void *key_ptr, char *out_letter) {
    long guard = 0;
    memcpy(&guard, (const uint8_t *)key_ptr + 0x10, sizeof(long));
    if (guard == 0) {
        return 0;
    }
    void *text_ptr = NULL;
    memcpy(&text_ptr, (const uint8_t *)key_ptr + 0x08, sizeof(void *));
    if (!text_ptr) {
        return 0;
    }
    void *data_ptr = NULL;
    long long size = 0;
    memcpy(&data_ptr, (const uint8_t *)text_ptr + 8, sizeof(void *));
    memcpy(&size, (const uint8_t *)text_ptr + 16, sizeof(long long));
    if (size != 1 || !data_ptr) {
        return 0;
    }
    uint16_t c;
    memcpy(&c, data_ptr, sizeof(uint16_t));
    if (c < (uint16_t)'a' || c > (uint16_t)'z') {
        return 0;
    }
    *out_letter = (char)c;
    return 1;
}

/* 把一份 cj_segment() 候选（音节 span 数组 + 可能的"打到一半"前缀）渲染成
 * 一行"音节 音节 ... 前缀"的展示字符串（空格分隔），写进调用方提供的 buf
 * （容量 cap，不含结尾 NUL）。返回实际写入的字符数。这一轮只做拼音音节
 * 切分展示，不是真汉字候选——词典没移植，如实展示这轮能做到什么，见计划
 * 文件 Step T"已知限制"。 */
static long long cj_step_t_render_segmentation(const CjSegmentation *seg, const char *lowercased,
                                                char *buf, size_t cap) {
    size_t pos = 0;
    for (int i = 0; i < seg->syllable_count && pos < cap; i++) {
        if (i > 0 && pos < cap) {
            buf[pos++] = ' ';
        }
        const CjSyllableSpan *sp = &seg->syllables[i];
        for (size_t j = 0; j < sp->len && pos < cap; j++) {
            buf[pos++] = lowercased[sp->offset + j];
        }
    }
    if (seg->has_pending) {
        if (seg->syllable_count > 0 && pos < cap) {
            buf[pos++] = ' ';
        }
        for (size_t j = 0; j < seg->pending.len && pos < cap; j++) {
            buf[pos++] = lowercased[seg->pending.offset + j];
        }
    }
    return (long long)pos;
}

/* 把 KeyPopup.keys/visible/targetRect 更新成给定的候选列表——count==0 时
 * 传 NULL/NULL 即可，会写一个空列表 + visible=false（隐藏）。targetRect
 * 复用 Step Q 已经调过的经验坐标，这轮不重新设计定位（用户已经明确要求
 * "先这样吧，主要要先完善输入法的正确输入"，见计划文件 M3 方向确认）。
 * 任何一个必需符号没解析成功就安静放弃（不崩，不重试），fail-safe。
 * is_utf8 见上面前向声明处的说明——Step V 新增，选 candidates 的编码。 */
/* Step AA（已废弃）：候选栏定位框读 virtualKeyboard.width 撑满键盘宽度，读不到退回经验值（fail-safe）——详见白皮书 02 节候选栏叠影排查。 */
/* 保留但不再调用——真机反复验证过 targetRect 根本不控制 KeyPopup 自己
 * 的可见尺寸（Step AA 的完整踩坑记录），后来发现即便直接写 width/height
 * 也治不了候选栏叠影的根因（KeyPopup 内部 Repeater 重建机制的问题，不是
 * 尺寸问题）。Step BB 换成 QMLDiff 注入的 CjCandidateBar 之后，宽高定位
 * 全部交给 QML 自己的 anchors 声明式处理（见 candidatebar.qmd），这个
 * 函数和它代表的整条"手动算/写尺寸"思路彻底不需要了，标记 unused 维持
 * 零警告编译，不删除——完整的踩坑过程本身是有价值的记录。 */
__attribute__((unused))
static CjRectF32 cj_step_t_compute_target_rect(void *this_ptr) {
    CjRectF32 fallback = { 140.0, 90.0, 500.0, 10.0 };
    if (!this_ptr || !g_qobject_property || !g_qvariant_toint || !g_qvariant_dtor) {
        return fallback;
    }
    CjVariant32 v = g_qobject_property(this_ptr, "width");
    int width = g_qvariant_toint(&v, NULL);
    g_qvariant_dtor(&v);
    if (width <= 0) {
        return fallback;
    }
    CjRectF32 r = { 0.0, 90.0, (double)width, 10.0 };
    return r;
}

/* Step BB：候选栏写入目标从 g_keypopup_ptr 换成 g_candidatebar_ptr——
 * CjCandidateBar 是我们自己用 QMLDiff 定义的组件，宽度/高度/位置全部由
 * candidatebar.qmd 里的 anchors 声明式绑定（跟着 keyboardLayout 走），
 * 不需要像 KeyPopup 那样每次刷新还要手动算/写 targetRect/width/height
 * ——这是这次改造顺带获得的简化，不是遗漏。同理也不需要 Step W 那套
 * "停用原生 autoHideTimeout"逻辑：CjCandidateBar 是我们自己定义的
 * Rectangle，没有任何原生自动隐藏行为需要对抗，可见性 100% 由我们自己
 * 通过 visible 属性决定。 */

/* Step CB-FMT：笔记本候选栏避让格式行（vkb-format-menu）。
 * 缺陷：候选栏 anchors.bottom=keyboardLayout.top、向上 94px、z:999；笔记本的格式行
 * vkb-format-menu（有序/无序/任务列表、撤销/重做，真机实测 absY≈1166 h≈82，紧贴键盘
 * 顶边上方）正落在候选栏这条带里，被 z:999 盖住；搜索页无此格式行故正常。
 * 修复：候选栏显示时递归找 vkb-format-menu，存在则置 hasFormatRow=true，qmld 里
 * anchors.bottomMargin: hasFormatRow?82:0 把候选栏底边上移一个格式行高度让位。
 * 真机几何定位见白皮书候选栏节。 */
/* 读 item 的 objectName 到 buf（UTF-16→ASCII，非 ASCII 记 '?'）。 */
static void cj_geom_objname(void *item, char *buf, int cap) {
    buf[0] = '\0';
    if (!item || !g_qobject_property || !g_qvariant_tostring || !g_qvariant_dtor) return;
    CjVariant32 v = g_qobject_property(item, "objectName");
    CjPtrList24 s = g_qvariant_tostring(&v);
    int n = 0;
    if (s.ptr && s.size > 0 && s.size < 200) {
        const uint16_t *u = (const uint16_t *)s.ptr;
        for (long long i = 0; i < s.size && n < cap - 1; i++) {
            uint16_t c = u[i];
            buf[n++] = (c >= 32 && c < 127) ? (char)c : '?';
        }
    }
    buf[n] = '\0';
    g_qvariant_dtor(&v);
}
/* 从 root 递归找【当前真正显示】的 vkb-format-menu（笔记本格式行）。判据三合一：
 * isVisible()（考虑父链，排除搜索页里隐藏的残留元素）+ height>10 + 绝对 y 落在候选栏
 * 覆盖区[1040,1300]（紧贴键盘上方，排除屏外/别处的同名元素）。累加 abs_y。
 * 深度/宽度剪枝防跑飞；只在候选栏显示时调（打字才走，非每帧）。 */
static int cj_has_format_menu(void *item, double abs_y, int depth) {
    if (!item || depth > 13 || !g_qquickitem_childitems) return 0;
    double y = abs_y + (g_qquickitem_y ? g_qquickitem_y(item) : 0);
    char nm[24];
    cj_geom_objname(item, nm, sizeof(nm));
    if (strcmp(nm, "vkb-format-menu") == 0) {
        double h = g_qquickitem_height ? g_qquickitem_height(item) : 0;
        char vis = g_qquickitem_isvisible ? g_qquickitem_isvisible(item) : 1;
        if ((vis & 1) && h > 10 && y >= 1040 && y <= 1300) return 1;
        /* 不满足则继续找（可能有多个同名元素，别的才是活跃那个） */
    }
    CjPtrList24 ch = g_qquickitem_childitems(item);
    if (ch.size > 0 && ch.size < 80 && ch.ptr) {
        void **arr = (void **)ch.ptr;
        for (long long i = 0; i < ch.size; i++)
            if (cj_has_format_menu(arr[i], y, depth + 1)) return 1;
    }
    return 0;
}

static void cj_step_t_update_popup(void *this_ptr, const char *const *candidates, const long long *lens, int count,
                                    int visible, int is_utf8) {
    (void)this_ptr; /* Step AA 的 targetRect/width/height 手动计算已经不需要了，
                        this_ptr 这个参数留着只是为了不用改一遍所有调用点的签名 */
    if (!g_candidatebar_ptr || !cj_resolve_keypopup_write_symbols() || !cj_resolve_qt_symbols()) {
        return;
    }

    unsigned char ok_keys = 0;
    uint8_t new_list[24];
    int list_ok = is_utf8
        ? cj_build_qstringlist_from_utf8_array(new_list, candidates, lens, count, g_qarraydata_allocate)
        : cj_build_qstringlist_from_ascii_array(new_list, candidates, lens, count, g_qarraydata_allocate);
    if (list_ok) {
        CjVariant32 v;
        memset(&v, 0, sizeof(v));
        g_qvariant_ctor_stringlist(&v, new_list);
        ok_keys = g_qobject_setproperty(g_candidatebar_ptr, "keys", &v);
        g_qvariant_dtor(&v);
    }

    unsigned char ok_visible = 0;
    if (g_qvariant_ctor_bool) {
        CjVariant32 v;
        memset(&v, 0, sizeof(v));
        g_qvariant_ctor_bool(&v, visible ? true : false);
        ok_visible = g_qobject_setproperty(g_candidatebar_ptr, "visible", &v);
        g_qvariant_dtor(&v);
    }

    /* Step CB-FMT：候选栏显示时检测笔记本格式行（vkb-format-menu），置 hasFormatRow
     * 让候选栏底边上移让位（避免 z:999 盖住格式行）。搜索页无格式行 → false → 不偏移。 */
    if (visible && g_qquickitem_parentitem && g_qvariant_ctor_bool) {
        void *it = g_candidatebar_ptr, *root = g_candidatebar_ptr;
        for (int i = 0; i < 14 && it; i++) { root = it; it = g_qquickitem_parentitem(it); }
        int has_fmt = cj_has_format_menu(root, 0.0, 0);
        CjVariant32 v;
        memset(&v, 0, sizeof(v));
        g_qvariant_ctor_bool(&v, has_fmt ? true : false);
        g_qobject_setproperty(g_candidatebar_ptr, "hasFormatRow", &v);
        g_qvariant_dtor(&v);
    }

    /* Step EE：候选栏下方的"预览行"——用户要求能看到实际打的拼音字母，
     * 双拼下显示自动转换出来的拼音（不是双拼按键原文）。读全局缓存
     * g_step_v_preview_text/len（cj_step_t_refresh_candidates 每次刷新
     * 都会更新，这里不重新计算），不管 visible 是 true 还是 false 都写
     * （隐藏候选栏时预览行本身也该跟着清空/隐藏，QML 侧 previewText 为
     * 空字符串时不显示这行就行，不需要额外一个开关）。
     *
     * len==0 时不走 cj_build_ascii_qstring（06 节已经踩过的坑：真机
     * QArrayData::allocate(capacity=0) 会返回空指针，被当成分配失败，
     * cj_build_ascii_qstring 内部没有对 len==0 做特判），直接构造一个
     * 空 QString（{d=NULL,ptr=NULL,size=0} 就是 Qt 隐式共享容器"空/
     * 默认构造"状态本身，不需要真的分配 0 大小内存，qstringlist_append.c
     * 里 count==0 的特判是同一个原理）。 */
    unsigned char ok_preview = 0;
    if (g_qvariant_ctor_qstring) {
        uint8_t qstr[24];
        int qstr_ok;
        if (g_step_v_preview_len == 0) {
            memset(qstr, 0, sizeof(qstr));
            qstr_ok = 1;
        } else {
            qstr_ok = cj_build_ascii_qstring(qstr, g_step_v_preview_text,
                                              g_step_v_preview_len, g_qarraydata_allocate);
        }
        if (qstr_ok) {
            CjVariant32 v;
            memset(&v, 0, sizeof(v));
            g_qvariant_ctor_qstring(&v, qstr);
            ok_preview = g_qobject_setproperty(g_candidatebar_ptr, "previewText", &v);
            g_qvariant_dtor(&v);
        }
    }

    CJ_LOG("[cangjie] Step BB：候选栏更新 count=%d visible=%d keys写入=%s visible写入=%s "
           "预览=\"%.*s\"写入=%s\n",
           count, visible, ok_keys ? "true" : "false", ok_visible ? "true" : "false",
           g_step_v_preview_len, g_step_v_preview_text, ok_preview ? "true" : "false");
}

/* Step FF（A4 后已弃用，保留）：iOS 候选式中英混输——把原始拉丁字母作为可点候选插进 keys 列表第 1 位、复用候选点选闭环、零新增管线；三种模式统一（双拼下是原始按键）。后来做逐字输入法时按 iOS 惯例移除。详见白皮书 07 节 Step FF。 */
/* Phase A（A4）之后不再调用——候选栏改成 iOS 干净版（只展示词候选），
 * 原始字母候选取消。保留完整实现+踩坑记录（Step FF），不删除；等 Phase C
 * 做智能混输时可能以不同形式复用。 */
__attribute__((unused))
static void cj_step_t_update_popup_with_english(void *this_ptr, const char *const *cand_ptrs,
                                                 const long long *cand_lens, int count, int is_utf8) {
    if (g_pinyin_buffer_len <= 0) {
        cj_step_t_update_popup(this_ptr, cand_ptrs, cand_lens, count, count > 0 ? 1 : 0, is_utf8);
        return;
    }
    if (count < 0) {
        count = 0;
    }
    if (count > CJ_STEP_V_CACHE_CAP) {
        count = CJ_STEP_V_CACHE_CAP; /* 防御：调用方最多传 CJ_STEP_V_CACHE_CAP 个，多的截掉，给英文候选留位 */
    }

    const char *aug_ptrs[CJ_STEP_V_CACHE_CAP + 1];
    long long aug_lens[CJ_STEP_V_CACHE_CAP + 1];
    int insert_pos = count > 0 ? 1 : 0; /* 紧跟第一名中文候选之后 */
    int n = 0;
    for (int i = 0; i <= count; i++) {
        if (i == insert_pos) {
            aug_ptrs[n] = g_pinyin_buffer;
            aug_lens[n] = (long long)g_pinyin_buffer_len;
            n++;
        }
        if (i < count) {
            aug_ptrs[n] = cand_ptrs[i];
            aug_lens[n] = cand_lens[i];
            n++;
        }
    }
    cj_step_t_update_popup(this_ptr, aug_ptrs, aug_lens, n, 1, is_utf8);
}

/* 只在真机上跑前几次——避免长时间打字把日志刷爆，够确认结论就收手，跟
 * Step O 的 CJ_CHILDREN_SEARCH_MAX_ATTEMPTS 是同一个节奏。 */
static int g_step_t_diag_log_count = 0;
#define CJ_STEP_T_DIAG_LOG_MAX 20

#define CJ_STEP_T_MAX_DISPLAY_CANDIDATES 5 /* 屏幕宽度有限，只显示前几个（cj_segment 最多能给 CJ_SEG_MAX_CANDIDATES=8 个） */

/* ===================================================================
 * Step V：候选词典接入——真汉字候选，不再只是拼音切分展示
 *
 * dict.bin 存放在 CJ_DATA_DIR（= /home/root/.local/share/cangjie-ime/，
 * 遵循 XDG Base Directory 布局，跟 cangjie-langhook.so 同一个目录，见白皮书
 * 9.5 节"磁盘空间调研"——/home 是独立的加密分区，45.8GB 可用，25MB 的
 * dict.bin 不是问题），运行时只读 mmap，不编译进
 * .so 本体，见 pinyin-engine/c/src/dictionary.h 顶部的架构说明（GPL v3
 * 数据/存储架构的完整取舍记录在那里，这里不重复）。
 *
 * 查询用最优切分（cj_best_segmentation，不是 cj_segment 的全部候选）：
 * 完整音节走精确匹配（cj_dict_lookup），"打到一半"的最后一段走前缀匹配
 * （cj_dict_lookup_prefix）追加，按词去重后合并——跟 pinyin-engine 的
 * engine.py::query() 是同一个设计思路，只是这次是 C 版、直接查 mmap
 * 好的 dict.bin，不是 Python 版内存里的 dict。
 *
 * 已知限制（这轮明确不做，不是漏掉）：只用"最优切分"一种切法查词典，
 * 没有做 engine.py 里的贪心多词拼接兜底（比如很长一段没有直接词条、需要
 * 拼几个词才能凑出候选的输入）——常见的短词/短语（打字时最高频的场景）
 * 已经能命中，更复杂的整句拼接是独立的后续工作，退化路径是显示拼音
 * 切分（下面兜底逻辑），不是空白/报错。 */
/* 词典 blob 全部收在 XDG data 目录下（/home/root/.local/share/cangjie-ime/），
 * 与 cangjie-langhook.so 同处，不再散落在 /home/root/ 根目录；改路径只需改这一处。
 * reMarkable 上 xochitl 是 systemd root 服务、进程环境未必设 XDG_DATA_HOME，
 * 故直接硬编码符合 XDG 布局的绝对路径，不运行时读环境变量。 */
#define CJ_DATA_DIR "/home/root/.local/share/cangjie-ime"
#define CJ_DICT_BIN_PATH CJ_DATA_DIR "/dict.bin"
static CjDictHandle g_dict_handle;
static int g_dict_open_attempted = 0;

/* 只在第一次真正需要查词典时尝试打开一次（成功/失败都不重试）。失败时
 * （文件不存在/格式不对/mmap 失败）词典功能整体不生效，候选栏自动退化
 * 回纯拼音切分展示，不影响任何其它功能——fail-safe，跟这份 hook 一贯的
 * 取舍一致。 */
static int cj_dict_ensure_open(void) {
    if (g_dict_open_attempted) {
        return g_dict_handle.base != NULL;
    }
    g_dict_open_attempted = 1;
    if (cj_dict_open(CJ_DICT_BIN_PATH, &g_dict_handle)) {
        CJ_LOG("[cangjie] Step V：词典打开成功 %s（key_count=%u cand_count=%u）\n",
               CJ_DICT_BIN_PATH, g_dict_handle.key_count, g_dict_handle.cand_count);
        return 1;
    }
    CJ_LOG("[cangjie] Step V：词典打开失败 %s——候选栏退化为纯拼音切分展示（不影响其它功能）\n",
           CJ_DICT_BIN_PATH);
    return 0;
}

/* ===================================================================
 * 05 节繁体双 blob 接入：dict.zh_tw.bin 跟 dict.bin 是完全一样的
 * CJDICT01 格式（拼音输入法白皮书 05 节"设计修订"+"离线部分已实现"
 * 两个 callout 有完整背景），cj_dict_open/cj_dict_lookup*() 一行代码
 * 不用改——这段只是照抄上面 cj_dict_ensure_open 的"惰性打开、成功/
 * 失败都不重试"模式，换一个路径、换一个静态 handle，没有新逻辑。 */
#define CJ_DICT_ZH_TW_BIN_PATH CJ_DATA_DIR "/dict.zh_tw.bin"
static CjDictHandle g_dict_handle_zh_tw;
static int g_dict_zh_tw_open_attempted = 0;

static int cj_dict_zh_tw_ensure_open(void) {
    if (g_dict_zh_tw_open_attempted) {
        return g_dict_handle_zh_tw.base != NULL;
    }
    g_dict_zh_tw_open_attempted = 1;
    if (cj_dict_open(CJ_DICT_ZH_TW_BIN_PATH, &g_dict_handle_zh_tw)) {
        CJ_LOG("[cangjie] 05 节：繁体词典打开成功 %s（key_count=%u cand_count=%u）\n",
               CJ_DICT_ZH_TW_BIN_PATH, g_dict_handle_zh_tw.key_count, g_dict_handle_zh_tw.cand_count);
        return 1;
    }
    CJ_LOG("[cangjie] 05 节：繁体词典打开失败 %s——繁体模式退化用简体词典（不影响其它功能，"
           "跟 cj_dict_ensure_open 同一个 fail-safe 原则）\n",
           CJ_DICT_ZH_TW_BIN_PATH);
    return 0;
}

/* ===================================================================
 * Phase C 中英混输：english.bin 跟 dict.bin 是完全一样的 CJDICT01 格式
 * （key = 英文词本身，单候选 = 该词，weight = 缩放后词频，见
 * pinyin-engine/c/tools/gen_english_blob.py + data/PROVENANCE.english.md），
 * cj_dict_open/cj_dict_lookup_prefix 一行不改就能打开——cj_dict_lookup_prefix
 * 的"按权重 top-k + 空格数过滤"直接就是"按词频排序的英文自动补全"。
 * 照抄上面 ensure_open 的"惰性打开、成功/失败都不重试"模式。 */
#define CJ_ENGLISH_BIN_PATH CJ_DATA_DIR "/english.bin"
static CjDictHandle g_english_dict;
static int g_english_open_attempted = 0;

static int cj_english_dict_ensure_open(void) {
    if (g_english_open_attempted) {
        return g_english_dict.base != NULL;
    }
    g_english_open_attempted = 1;
    if (cj_dict_open(CJ_ENGLISH_BIN_PATH, &g_english_dict)) {
        CJ_LOG("[cangjie] Phase C：英文词典打开成功 %s（key_count=%u cand_count=%u）\n",
               CJ_ENGLISH_BIN_PATH, g_english_dict.key_count, g_english_dict.cand_count);
        return 1;
    }
    CJ_LOG("[cangjie] Phase C：英文词典打开失败 %s——中英混输的英文候选整体不生效（不影响其它功能）\n",
           CJ_ENGLISH_BIN_PATH);
    return 0;
}

/* 根据已经判断好的语言模式选出这次查询该用哪份词典——zh_TW 且繁体版
 * 打开成功才返回繁体 handle，其它情况一律退化用简体 handle。返回 NULL
 * 表示两份词典都没打开成功，调用方（cj_step_v_generate_dict_candidates）
 * 已经有"handle 为空就返回 0 个候选、退化成拼音切分展示"的处理，不是
 * 新增的失败模式。参数是 mode 而不是 this_ptr——Step X 简拼补充候选
 * 接入时改的：调用方（cj_step_t_refresh_candidates）现在只读一次
 * virtualKeyboard.language，同一个 mode 同时喂给词典和简拼两个选择器，
 * 不是各自重复读一次属性（避免"有界小引用计数泄漏"随接入的功能变多
 * 越滚越大）。 */
/* Step FF：用户词频（自适应调频）——独立文件、独立数据（不碰 GPL 的
 * dict.bin），规则/格式见 pinyin-engine/src/userfreq.py docstring 与
 * c/src/userfreq.h。惰性加载（文件不存在=空表照常工作）；提交点记
 * (key, word) 计数后立刻原子保存（tmp+rename，全表 ≤ ~240KB、常态几 KB，
 * 单线程 UI 路径里同步写一次的代价可忽略，换来崩溃不丢计数）。 */
#define CJ_USERFREQ_PATH CJ_DATA_DIR "/userfreq.tsv"
static CjUserFreq g_userfreq; /* ~250KB bss，定长无 malloc */
static int g_userfreq_load_attempted = 0;

static void cj_userfreq_ensure_loaded(void) {
    if (g_userfreq_load_attempted) {
        return;
    }
    g_userfreq_load_attempted = 1;
    cj_uf_load(&g_userfreq, CJ_USERFREQ_PATH);
    CJ_LOG("[cangjie] Step FF：用户词频加载完成（%d 条，%s）\n",
           g_userfreq.count, CJ_USERFREQ_PATH);
}

/* 强制召回条数上限——每个查询组最多把几个"用户选过的词"插到组头。
 * 组内候选位本就有限（整词组=max_out/2、中间前缀=4），3 个足够覆盖
 * "我常打的那几个词"，又不至于把静态词频的头部全挤出可视区。 */
#define CJ_UF_GROUP_MAX 3

/* Step GG：快捷输入（text replacement）——用户在 snippets.tsv 里定义
 * "缩写→短语"，原始字母缓冲区精确等于缩写时短语注入为第 0 位候选
 * （打缩写+空格即上屏短语，iOS 同款）。文件只读+mtime 热重载（host 侧
 * 改完下一次打字即生效）；bss 零初始化恰好就是 cj_sn_init 后的状态，
 * 不需要显式 init。 */
#define CJ_SNIPPETS_PATH CJ_DATA_DIR "/snippets.tsv"
#define CJ_SN_INJECT_MAX 4 /* 同一缩写最多注入几条短语到候选头部 */
static CjSnippets g_snippets; /* ~74KB bss，定长无 malloc */

static CjDictHandle *cj_dict_select_handle_for_mode(CjLanguageMode mode) {
    if (mode == CJ_LANG_ZH_TW && cj_dict_zh_tw_ensure_open()) {
        return &g_dict_handle_zh_tw;
    }
    if (!cj_dict_ensure_open()) {
        return NULL;
    }
    return &g_dict_handle;
}

/* ===================================================================
 * Step X：简拼索引接入——07 节"简拼——声母首字母倒排索引"设计里"跟全拼
 * 路径并行查询、结果合并"这部分，这次真正接入设备。dict_jianpin.bin 是
 * 独立的自成一体 blob（不是引用 dict.bin 候选池的偏移量，见 jianpin.h
 * 顶部注释里记录的设计偏离），打开方式照抄上面 cj_dict_zh_tw_ensure_open
 * 同一个"惰性打开、成功/失败都不重试"模式，只是换一个类型（CjJianpinHandle）
 * 和一个静态 handle，没有新逻辑。 */
#define CJ_JIANPIN_BIN_PATH CJ_DATA_DIR "/dict_jianpin.bin"
static CjJianpinHandle g_jianpin_handle;
static int g_jianpin_open_attempted = 0;

static int cj_jianpin_ensure_open(void) {
    if (g_jianpin_open_attempted) {
        return g_jianpin_handle.base != NULL;
    }
    g_jianpin_open_attempted = 1;
    if (cj_jianpin_open(CJ_JIANPIN_BIN_PATH, &g_jianpin_handle)) {
        CJ_LOG("[cangjie] Step X：简拼索引打开成功 %s（key_count=%u cand_count=%u）\n",
               CJ_JIANPIN_BIN_PATH, g_jianpin_handle.key_count, g_jianpin_handle.cand_count);
        return 1;
    }
    CJ_LOG("[cangjie] Step X：简拼索引打开失败 %s——简拼补充候选直接跳过（不影响全拼候选，"
           "fail-safe 原则跟 cj_dict_ensure_open 一致）\n",
           CJ_JIANPIN_BIN_PATH);
    return 0;
}

/* Step X 真机 bug：繁体模式下简拼补充候选仍是简体字（dict_jianpin.bin 只从简体反推），全拼命中 0 个时甚至成第一名被提交——照抄 05 节双 blob 生成 dict_jianpin.zh_tw.bin + cj_jianpin_select_handle 按繁简选。详见白皮书 07 节。 */
#define CJ_JIANPIN_ZH_TW_BIN_PATH CJ_DATA_DIR "/dict_jianpin.zh_tw.bin"
static CjJianpinHandle g_jianpin_handle_zh_tw;
static int g_jianpin_zh_tw_open_attempted = 0;

static int cj_jianpin_zh_tw_ensure_open(void) {
    if (g_jianpin_zh_tw_open_attempted) {
        return g_jianpin_handle_zh_tw.base != NULL;
    }
    g_jianpin_zh_tw_open_attempted = 1;
    if (cj_jianpin_open(CJ_JIANPIN_ZH_TW_BIN_PATH, &g_jianpin_handle_zh_tw)) {
        CJ_LOG("[cangjie] Step X：繁体简拼索引打开成功 %s（key_count=%u cand_count=%u）\n",
               CJ_JIANPIN_ZH_TW_BIN_PATH, g_jianpin_handle_zh_tw.key_count, g_jianpin_handle_zh_tw.cand_count);
        return 1;
    }
    CJ_LOG("[cangjie] Step X：繁体简拼索引打开失败 %s——繁体模式下简拼补充候选退化用简体索引"
           "（不影响全拼候选，fail-safe 原则跟 cj_dict_select_handle_for_mode 一致）\n",
           CJ_JIANPIN_ZH_TW_BIN_PATH);
    return 0;
}

/* mode 由调用方（cj_step_t_refresh_candidates）传入——复用它已经调用过
 * 一次 cj_step_t_language_mode(this_ptr) 的结果，不在这里重复读属性
 * （道理跟 cj_dict_select_handle_for_mode 的调用点一致：避免同一次刷新里把"有界
 * 小引用计数泄漏"翻倍）。zh_TW 且繁体简拼索引打开成功才返回繁体 handle，
 * 其它情况一律退化用简体 handle；两份都没打开成功返回 NULL，调用方
 * （cj_step_v_generate_jianpin_candidates）已经有"handle 为空直接跳过"
 * 的处理，不是新失败模式。 */
static const CjJianpinHandle *cj_jianpin_select_handle(CjLanguageMode mode) {
    if (mode == CJ_LANG_ZH_TW && cj_jianpin_zh_tw_ensure_open()) {
        return &g_jianpin_handle_zh_tw;
    }
    if (!cj_jianpin_ensure_open()) {
        return NULL;
    }
    return &g_jianpin_handle;
}

/* g_step_v_top_candidate_utf8/g_step_v_top_candidate_len 已经在文件前面
 * （跟 g_pinyin_buffer 放一起）声明过，这里不重复定义，直接用。 */
/* 精确匹配补位用的内部缓冲区大小——跟 CJ_STEP_V_CACHE_CAP 保持一致，
 * 避免"前缀匹配没填满 max_out、想用精确匹配补位，但补位缓冲区本身比
 * max_out 还小"这种不一致（Step V 加入分页缓存后 max_out 从 5 涨到 64，
 * 这个缓冲区也要跟着涨，否则补位补不满）。 */
#define CJ_STEP_V_PREFIX_SCRATCH CJ_STEP_V_CACHE_CAP

/* 用最优切分结果查词典，取出真候选词（UTF-8 字节，指向 mmap 区域，
 * 生命周期只到下一次 cj_dict_* 调用之前，调用方必须在这次刷新内用完，
 * 不能跨刷新持有）。返回写入 out 的候选个数，0 表示没有真候选（调用方
 * 应该退化成拼音切分展示）。
 *
 * 🔴 真机验证时发现并修复的真实 bug（不是理论推演）：最初版本是"完整
 * 音节精确匹配优先，pending 前缀匹配的结果只在还有剩余槽位时才补上"——
 * 用户实测报告"打完 pin 再按 y，候选栏没反应，感觉 y 没按中"。根因：
 * 很多音节的精确匹配候选数远超展示上限（"pin" 单独精确匹配就有 50 个
 * 真候选，品/拼/贫/频/聘...），这些候选把 CJ_STEP_T_MAX_DISPLAY_CANDIDATES
 * （5 个）槽位直接占满，之后不管 pending 后缀怎么变（y/yi/yin...），
 * 精确匹配部分纹丝不动，pending 前缀匹配永远排不上号，候选栏对用户
 * 后续的按键完全没有视觉反馈——这不是极端情况，是绝大多数声母 4 个字母
 * 以内的音节都会踩到的常见路径。
 *
 * 修复：**pending 存在时，前缀匹配优先**——用户还在继续敲一个音节，
 * 说明当前"已经打完的部分"还不是最终意图，这个信号比"完整音节本身
 * 有很多同音字"更有参考价值；完整音节的精确匹配退化成补位（前缀匹配
 * 填不满 max_out 时才用来填充剩余槽位），不再无条件优先。没有 pending
 * 时行为不变（只有精确匹配，因为这时候没有"还在继续输入"这个信号）。 */
/* Step CC：贪心最长匹配分词+逐段拼字实现"整句候选"，移植自 engine.py::_compose()——dp[i] 从右往左、每位最长匹配优先、命中即用第一名拼接并 break（贪心 MVP，非全局最优）；音节数超上限直接放弃、退回拼音切分兜底。详见白皮书 02 节 Step CC / 03 节。 */
#define CJ_STEP_V_COMPOSE_MAX_SYLLABLES (CJ_SEG_MAX_INPUT / 2 + 1)
#define CJ_STEP_V_COMPOSE_TEXT_CAP 128
static char g_step_v_composed_text[CJ_STEP_V_COMPOSE_TEXT_CAP];

static int cj_step_v_compose(const CjDictHandle *dict_handle, const CjSegmentation *best_seg,
                              const char *lowercased, char *out_text, int out_cap) {
    int n = best_seg->syllable_count;
    if (n <= 1 || n >= CJ_STEP_V_COMPOSE_MAX_SYLLABLES) {
        /* n<=1：单音节的话跟"整段精确匹配"是同一次查询，重复做没有意义
         * （精确匹配已经失败过一次，compose 也不会有别的结果）。
         * n 太大：超出这个函数愿意处理的范围，放弃，不强凑。 */
        return 0;
    }

    static char dp_text[CJ_STEP_V_COMPOSE_MAX_SYLLABLES][CJ_STEP_V_COMPOSE_TEXT_CAP];
    static int dp_len[CJ_STEP_V_COMPOSE_MAX_SYLLABLES];
    static int dp_ok[CJ_STEP_V_COMPOSE_MAX_SYLLABLES];

    dp_ok[n] = 1;
    dp_len[n] = 0;

    for (int i = n - 1; i >= 0; i--) {
        dp_ok[i] = 0;
        for (int j = n; j > i; j--) {
            char key[CJ_SEG_MAX_INPUT * 2];
            size_t key_len = 0;
            for (int k = i; k < j && key_len < sizeof(key); k++) {
                if (k > i && key_len < sizeof(key)) {
                    key[key_len++] = ' ';
                }
                const CjSyllableSpan *sp = &best_seg->syllables[k];
                for (size_t x = 0; x < sp->len && key_len < sizeof(key); x++) {
                    key[key_len++] = lowercased[sp->offset + x];
                }
            }

            CjDictCandidate cand;
            int found = cj_dict_lookup(dict_handle, key, key_len, &cand, 1);
            if (found <= 0 || !dp_ok[j]) {
                continue;
            }

            int total_len = (int)cand.word_len + dp_len[j];
            if (total_len > CJ_STEP_V_COMPOSE_TEXT_CAP - 1) {
                continue; /* 拼出来太长装不下，试更短的切法（j 继续往小走） */
            }

            memcpy(dp_text[i], cand.word, cand.word_len);
            memcpy(dp_text[i] + cand.word_len, dp_text[j], (size_t)dp_len[j]);
            dp_len[i] = total_len;
            dp_ok[i] = 1;
            break;
        }
    }

    if (!dp_ok[0] || dp_len[0] <= 0) {
        return 0;
    }
    int copy_len = dp_len[0] < out_cap ? dp_len[0] : out_cap;
    memcpy(out_text, dp_text[0], (size_t)copy_len);
    return copy_len;
}

/* dict_handle 由调用方（cj_step_t_refresh_candidates）通过
 * cj_dict_select_handle_for_mode() 事先选好——简体还是繁体，这个函数不关心，
 * 只管拿到的 handle 查，NULL 表示两份词典都没打开成功，直接返回 0
 * 走原有的"退化成拼音切分展示"路径，是既有 fail-safe 行为的自然
 * 延伸，不是新失败模式。 */
/* Step DD：双拼单敲声母键（pending 1 字节）时翻译回声母字面值（小鹤 v/i/u→zh/ch/sh，其它原样）再喂 cj_dict_lookup_prefix，像全拼敲 "sh" 一样弹 sh 声母常用字。详见白皮书 02 节 Step DD。 */
static const char *cj_shuangpin_initial_key_to_pinyin_initial(char key, size_t *out_len) {
    switch (key) {
        case 'v': *out_len = 2; return "zh";
        case 'i': *out_len = 2; return "ch";
        case 'u': *out_len = 2; return "sh";
        default: *out_len = 1; return NULL; /* 原样透传，调用方直接用 key 本身 */
    }
}

/* Phase A：某个候选覆盖"前 j 个完整音节"时，点选提交后要从 g_pinyin_buffer
 * 裁掉多少个原始按键字节。全拼下 seg_src==g_pinyin_buffer，音节 offset/len
 * 直接就是缓冲区里的字节位置；双拼下缓冲区是原始按键、每个完整音节恰好
 * 2 个键（小鹤方案，见 04 节），consumed = 2*j。 */
static int cj_consumed_bytes_for_prefix(const CjSegmentation *best_seg, int shuangpin_active, int j) {
    if (j <= 0) {
        return 0;
    }
    if (j > best_seg->syllable_count) {
        j = best_seg->syllable_count;
    }
    if (shuangpin_active) {
        return 2 * j;
    }
    const CjSyllableSpan *sp = &best_seg->syllables[j - 1];
    return (int)(sp->offset + sp->len);
}

/* Phase A：把"前 j 个完整音节"的空格拼接 key 写进 keybuf，返回长度。 */
static size_t cj_build_prefix_key(const CjSegmentation *best_seg, const char *lowercased,
                                   int j, char *keybuf, size_t cap) {
    size_t kl = 0;
    for (int i = 0; i < j && i < best_seg->syllable_count && kl < cap; i++) {
        if (i > 0 && kl < cap) {
            keybuf[kl++] = ' ';
        }
        const CjSyllableSpan *sp = &best_seg->syllables[i];
        for (size_t m = 0; m < sp->len && kl < cap; m++) {
            keybuf[kl++] = lowercased[sp->offset + m];
        }
    }
    return kl;
}

/* Phase A（iOS 逐字候选核心）：对递减前缀 j=k..1 各查一次词典，让"整段词"
 * 和"首音节单字"同时出现在候选栏——打 shenme 既有"什么"也有"神/什/深…"，
 * 才能逐字选。配额：整段(j==k)占一半，单字(j==1)填满剩下，中间前缀少量，
 * 保证首音节单字永远有位置、不被整段词挤掉（06 节真机+差分测试验证过
 * "候选一多就挤掉尾部结果"，这里刻意反过来给单字保底）。每个候选记录
 * consumed 字节数，供 A-ii 分段提交裁缓冲区。返回追加后的 n。 */
static int cj_add_prefix_candidates(const CjSegmentation *best_seg, const char *lowercased,
                                     CjDictHandle *dict_handle, int shuangpin_active,
                                     CjDictCandidate *out, int *out_consumed, int n, int max_out) {
    int k = best_seg->syllable_count;
    for (int j = k; j >= 1 && n < max_out; j--) {
        char keybuf[CJ_SEG_MAX_INPUT * 2];
        size_t kl = cj_build_prefix_key(best_seg, lowercased, j, keybuf, sizeof(keybuf));
        if (kl == 0) {
            continue;
        }
        int consumed = cj_consumed_bytes_for_prefix(best_seg, shuangpin_active, j);

        /* Step FF：强制召回——用户在这个精确 key 下选过的词插到组头
         * （count 降序）。词文字指向 g_userfreq 内部条目，指针在下一次
         * cj_uf_record/load 前有效：提交路径先拷本地再 record，且 record
         * 后必经 refresh 重建缓存，满足生命周期约束。 */
        cj_userfreq_ensure_loaded();
        const CjUfEntry *ufw[CJ_UF_GROUP_MAX];
        int un = cj_uf_words_for_key(&g_userfreq, keybuf, kl, ufw, CJ_UF_GROUP_MAX);
        for (int i = 0; i < un && n < max_out; i++) {
            int dup = 0;
            for (int p = 0; p < n; p++) {
                if (out[p].word_len == ufw[i]->word_len &&
                    memcmp(out[p].word, ufw[i]->word, out[p].word_len) == 0) {
                    dup = 1;
                    break;
                }
            }
            if (dup) {
                continue;
            }
            out[n].word = ufw[i]->word;
            out[n].word_len = ufw[i]->word_len;
            out[n].weight = 0; /* 展示不用 weight，排序地位由插入位置决定 */
            out_consumed[n] = consumed;
            cj_step_v_set_cached_key(n, keybuf, kl);
            n++;
        }

        int cap;
        if (j == 1) {
            cap = max_out - n;           /* 首音节单字：填满剩下所有位置 */
        } else if (j == k) {
            cap = max_out / 2;           /* 整段词：占一半，给单字留位 */
        } else {
            cap = 4;                     /* 中间前缀：少量 */
        }
        if (cap > max_out - n) {
            cap = max_out - n;
        }
        if (cap <= 0) {
            continue;
        }
        CjDictCandidate scratch[CJ_STEP_V_PREFIX_SCRATCH];
        int want = cap < CJ_STEP_V_PREFIX_SCRATCH ? cap : CJ_STEP_V_PREFIX_SCRATCH;
        int cn = cj_dict_lookup(dict_handle, keybuf, kl, scratch, want);
        for (int i = 0; i < cn && n < max_out; i++) {
            int dup = 0;
            for (int p = 0; p < n; p++) {
                if (out[p].word_len == scratch[i].word_len &&
                    memcmp(out[p].word, scratch[i].word, out[p].word_len) == 0) {
                    dup = 1;
                    break;
                }
            }
            if (dup) {
                continue;
            }
            out[n] = scratch[i];
            out_consumed[n] = consumed;
            cj_step_v_set_cached_key(n, keybuf, kl);
            n++;
        }
    }
    return n;
}

static int cj_step_v_generate_dict_candidates(const CjSegmentation *best_seg, const char *lowercased,
                                               CjDictHandle *dict_handle, int shuangpin_active,
                                               CjDictCandidate *out, int *out_consumed, int max_out) {
    if (!dict_handle || max_out <= 0) {
        return 0;
    }

    int n = 0;
    int whole_consumed = g_pinyin_buffer_len; /* 前缀预测/贪心拼句：点了就消耗整串 */

    if (best_seg->has_pending) {
        /* pending 存在（正在打某个音节的一半）：先做前缀预测（complete
         * syllables + pending 声母），命中的是"整串还没打完的预测词"，点选
         * 消耗整串；配额占一半，给下面完整音节的逐字单字留位。 */
        char prefix_key[CJ_SEG_MAX_INPUT * 2];
        size_t prefix_len = cj_build_prefix_key(best_seg, lowercased,
                                                 best_seg->syllable_count, prefix_key, sizeof(prefix_key));
        if (best_seg->syllable_count > 0 && prefix_len < sizeof(prefix_key)) {
            prefix_key[prefix_len++] = ' ';
        }
        if (shuangpin_active && best_seg->pending.len == 1) {
            /* Step DD：单敲声母键——翻译成声母字面值再拼进前缀。 */
            size_t initial_len = 0;
            const char *initial = cj_shuangpin_initial_key_to_pinyin_initial(
                lowercased[best_seg->pending.offset], &initial_len);
            if (initial) {
                for (size_t j = 0; j < initial_len && prefix_len < sizeof(prefix_key); j++) {
                    prefix_key[prefix_len++] = initial[j];
                }
            } else if (prefix_len < sizeof(prefix_key)) {
                prefix_key[prefix_len++] = lowercased[best_seg->pending.offset];
            }
        } else {
            for (size_t j = 0; j < best_seg->pending.len && prefix_len < sizeof(prefix_key); j++) {
                prefix_key[prefix_len++] = lowercased[best_seg->pending.offset + j];
            }
        }

        int pred_cap = max_out / 2;

        /* Step FF：先强制召回——用户以前打到这个前缀就选过的词（key=
         * 前缀原文），插在预测组头。 */
        cj_userfreq_ensure_loaded();
        const CjUfEntry *ufw[CJ_UF_GROUP_MAX];
        int un = cj_uf_words_for_key(&g_userfreq, prefix_key, prefix_len, ufw, CJ_UF_GROUP_MAX);
        for (int i = 0; i < un && n < pred_cap; i++) {
            out[n].word = ufw[i]->word;
            out[n].word_len = ufw[i]->word_len;
            out[n].weight = 0;
            out_consumed[n] = whole_consumed;
            cj_step_v_set_cached_key(n, prefix_key, prefix_len);
            n++;
        }

        /* 词典前缀池 → scratch，按 word 聚合计数（use_word_total）组内
         * 稳定重排后去重追加——池内候选的完整 key 未知（"ni h" 命中的
         * 是 "ni hao"/"ni hong" 等不同 key 的词），只能按词聚合。 */
        CjDictCandidate scratch[CJ_STEP_V_PREFIX_SCRATCH];
        int want = pred_cap < CJ_STEP_V_PREFIX_SCRATCH ? pred_cap : CJ_STEP_V_PREFIX_SCRATCH;
        int pn = cj_dict_lookup_prefix(dict_handle, prefix_key, prefix_len, scratch, want);
        if (pn > 0) {
            const char *uf_words[CJ_STEP_V_PREFIX_SCRATCH];
            size_t uf_lens[CJ_STEP_V_PREFIX_SCRATCH];
            int uf_order[CJ_STEP_V_PREFIX_SCRATCH];
            for (int i = 0; i < pn; i++) {
                uf_words[i] = scratch[i].word;
                uf_lens[i] = scratch[i].word_len;
            }
            cj_uf_order(&g_userfreq, prefix_key, prefix_len, /*use_word_total=*/1,
                        uf_words, uf_lens, pn, uf_order);
            for (int oi = 0; oi < pn && n < pred_cap; oi++) {
                const CjDictCandidate *c = &scratch[uf_order[oi]];
                int dup = 0;
                for (int p = 0; p < n; p++) {
                    if (out[p].word_len == c->word_len &&
                        memcmp(out[p].word, c->word, c->word_len) == 0) {
                        dup = 1;
                        break;
                    }
                }
                if (dup) {
                    continue;
                }
                out[n] = *c;
                out_consumed[n] = whole_consumed;
                cj_step_v_set_cached_key(n, prefix_key, prefix_len);
                n++;
            }
        }

        /* 完整音节的逐字候选（首音节单字等），consumed 按前缀长度。 */
        n = cj_add_prefix_candidates(best_seg, lowercased, dict_handle, shuangpin_active,
                                      out, out_consumed, n, max_out);

        /* 都查不到 → 贪心拼句兜底（Step CC），接上正在打的半个音节的字母，
         * 点选消耗整串。 */
        if (n == 0 && best_seg->syllable_count > 0 && max_out > 0) {
            int compose_len = cj_step_v_compose(dict_handle, best_seg, lowercased,
                                                 g_step_v_composed_text, CJ_STEP_V_COMPOSE_TEXT_CAP);
            if (compose_len > 0) {
                int total_len = compose_len;
                for (size_t j = 0; j < best_seg->pending.len && total_len < CJ_STEP_V_COMPOSE_TEXT_CAP; j++) {
                    g_step_v_composed_text[total_len++] = lowercased[best_seg->pending.offset + j];
                }
                out[0].word = g_step_v_composed_text;
                out[0].word_len = (uint32_t)total_len;
                out[0].weight = 0;
                out_consumed[0] = whole_consumed;
                /* Step FF：pending 态的拼句尾巴挂着没打完的字母，key 无法
                 * 稳定重现，不参与调频。 */
                cj_step_v_set_cached_key(0, NULL, 0);
                n = 1;
            }
        }
    } else if (best_seg->syllable_count > 0) {
        /* 没有 pending：完整音节序列确定。逐字候选（含整段匹配 j=k + 首
         * 音节单字 j=1）——这是 iOS 逐字造句的关键：打 shenme 同时给出
         * "什么"和"神/什/深…"，点第一个字提交后剩下的音节继续选。 */
        n = cj_add_prefix_candidates(best_seg, lowercased, dict_handle, shuangpin_active,
                                      out, out_consumed, n, max_out);

        /* 整段和各前缀都没命中（多音节完整小句词典通常不整条收录）→ 贪心
         * 拼句兜底，点选消耗整串。 */
        if (n == 0 && max_out > 0) {
            int compose_len = cj_step_v_compose(dict_handle, best_seg, lowercased,
                                                 g_step_v_composed_text, CJ_STEP_V_COMPOSE_TEXT_CAP);
            if (compose_len > 0) {
                out[0].word = g_step_v_composed_text;
                out[0].word_len = (uint32_t)compose_len;
                out[0].weight = 0;
                out_consumed[0] = whole_consumed;
                /* Step FF：完整音节的贪心拼句 key 稳定（整段音节 key），
                 * 参与调频——选过一次的整句下次强制召回直达。 */
                {
                    char full_key[CJ_SEG_MAX_INPUT * 2];
                    size_t fkl = cj_build_prefix_key(best_seg, lowercased,
                                                      best_seg->syllable_count,
                                                      full_key, sizeof(full_key));
                    cj_step_v_set_cached_key(0, full_key, fkl);
                }
                n = 1;
            }
        }
    }

    return n;
}

/* Step X：简拼补充候选——全拼路径填完后把简拼前缀查询结果追加在其后（全拼优先；简拼权重不参与跨路径排序，接受简化）；缓冲区本身已是合法简拼 key，单次 cj_jianpin_lookup_prefix 即可；按候选文字去重。🔴 真机 bug：曾误加 "buffer_len<2 跳过"，导致 "m"/"n" 查不到简拼（单字母也可能是多字母 key 的前缀），已去掉长度下限。详见白皮书 07 节。 */
static int cj_step_v_generate_jianpin_candidates(const char *lowercased, size_t buffer_len,
                                                  const CjJianpinHandle *jianpin_handle,
                                                  CjDictCandidate *out, int *out_consumed,
                                                  int start_index, int max_out) {
    if (!jianpin_handle || buffer_len == 0 || start_index >= max_out) {
        return 0;
    }

    int scratch_cap = max_out - start_index;
    if (scratch_cap > CJ_STEP_V_PREFIX_SCRATCH) {
        scratch_cap = CJ_STEP_V_PREFIX_SCRATCH;
    }
    CjJianpinCandidate scratch[CJ_STEP_V_PREFIX_SCRATCH];
    int scratch_n = cj_jianpin_lookup_prefix(jianpin_handle, lowercased, buffer_len, scratch, scratch_cap);

    int n = start_index;

    /* Step FF：强制召回——用户在这串简拼字母（精确 key）下选过的词插到
     * 简拼段头（对全部已有候选去重，跟下面词典结果同一套防线）。 */
    cj_userfreq_ensure_loaded();
    const CjUfEntry *ufw[CJ_UF_GROUP_MAX];
    int un = cj_uf_words_for_key(&g_userfreq, lowercased, buffer_len, ufw, CJ_UF_GROUP_MAX);
    for (int i = 0; i < un && n < max_out; i++) {
        int dup = 0;
        for (int j = 0; j < n; j++) {
            if (out[j].word_len == ufw[i]->word_len &&
                memcmp(out[j].word, ufw[i]->word, out[j].word_len) == 0) {
                dup = 1;
                break;
            }
        }
        if (!dup) {
            out[n].word = ufw[i]->word;
            out[n].word_len = ufw[i]->word_len;
            out[n].weight = 0;
            out_consumed[n] = (int)buffer_len;
            cj_step_v_set_cached_key(n, lowercased, buffer_len);
            n++;
        }
    }

    /* 简拼池按 word 聚合计数组内稳定重排（跟全拼前缀池同一个理由：池内
     * 候选的完整拼音 key 未知）。 */
    int uf_order[CJ_STEP_V_PREFIX_SCRATCH];
    if (scratch_n > 0) {
        const char *uf_words[CJ_STEP_V_PREFIX_SCRATCH];
        size_t uf_lens[CJ_STEP_V_PREFIX_SCRATCH];
        for (int i = 0; i < scratch_n; i++) {
            uf_words[i] = scratch[i].word;
            uf_lens[i] = scratch[i].word_len;
        }
        cj_uf_order(&g_userfreq, lowercased, buffer_len, /*use_word_total=*/1,
                    uf_words, uf_lens, scratch_n, uf_order);
    }

    for (int oi = 0; oi < scratch_n && n < max_out; oi++) {
        const CjJianpinCandidate *c = &scratch[uf_order[oi]];
        int dup = 0;
        for (int j = 0; j < n; j++) {
            if (out[j].word_len == c->word_len &&
                memcmp(out[j].word, c->word, out[j].word_len) == 0) {
                dup = 1;
                break;
            }
        }
        if (!dup) {
            out[n].word = c->word;
            out[n].word_len = c->word_len;
            out[n].weight = c->weight;
            /* Phase A：简拼候选是声母缩写命中的整词，点选消耗整个缓冲区
             * （不像全拼那样能逐字裁——简拼没有逐音节的字节边界）。 */
            out_consumed[n] = (int)buffer_len;
            cj_step_v_set_cached_key(n, lowercased, buffer_len);
            n++;
        }
    }
    return n - start_index;
}

/* Phase C（中英混输，增量式）：把原始拉丁缓冲区当英文前缀查 english.bin，
 * 命中的英文词（按词频 top-N）插进候选缓存第一名中文候选之后。iOS 式：
 * 打 nihao→选你好→再打 hello→候选栏出现 hello→点它上屏英文，得"你好hello"。
 *
 * 与模式无关——查的是 g_pinyin_buffer 里的原始按键（全拼=拼音字母/简拼=
 * 声母/双拼=双拼按键原文），用户想打英文时敲的就是字面 h-e-l-l-o，无论
 * 哪种中文模式原始缓冲区都是 "hello"，英文命中一致，不需要按模式分支。
 *
 * 复用现成管线：英文候选注入 g_step_v_cached_candidates/consumed 后，展示
 * （display_page）、点选（candidatebar \x02<idx>）、分段提交
 * （commit_candidate_by_index，consumed=raw_len 裁整段+上屏英文词）三条路径
 * 全部自动生效——不改 QML、不改点选逻辑、不改查询层。fail-safe：词典没
 * 打开就原样返回，中文候选照常。 */
#define CJ_STEP_C_ENGLISH_MAX 3 /* 英文候选最多插几个（可调；打拼音时的噪音靠这个 + "排在中文之后"控制） */

static int cj_step_c_add_english_candidates(const char *raw, size_t raw_len,
                                            CjDictCandidate *cands, int *consumed,
                                            int cur_count, int max_out) {
    if (raw_len == 0 || max_out <= 0) {
        return cur_count;
    }
    if (!cj_english_dict_ensure_open()) {
        return cur_count; /* 词典没打开：英文候选整体跳过，中文照常 */
    }

    /* 按词频 top-N 取前缀命中（cj_dict_lookup_prefix 已是权重 top-k）。 */
    CjDictCandidate hits[CJ_STEP_C_ENGLISH_MAX];
    int hit_n = cj_dict_lookup_prefix(&g_english_dict, raw, raw_len, hits, CJ_STEP_C_ENGLISH_MAX);
    if (hit_n <= 0) {
        return cur_count; /* 不是英文词前缀（打纯拼音时的常态，无噪音） */
    }

    /* 对已有候选去重（中文候选是 UTF-8 汉字、英文是 ASCII，正常不会撞，
     * 保一道廉价防线）；攒进 ins[]。 */
    CjDictCandidate ins[CJ_STEP_C_ENGLISH_MAX];
    int ins_n = 0;
    for (int i = 0; i < hit_n; i++) {
        int dup = 0;
        for (int j = 0; j < cur_count; j++) {
            if (cands[j].word_len == hits[i].word_len &&
                memcmp(cands[j].word, hits[i].word, hits[i].word_len) == 0) {
                dup = 1;
                break;
            }
        }
        if (!dup) {
            ins[ins_n++] = hits[i];
        }
    }
    if (ins_n == 0) {
        return cur_count;
    }

    /* 插在第一名中文候选之后（cur_count>0 时 pos=1，收起单行里可见）；
     * 中文候选为空（纯英文输入）时 pos=0，英文成为唯一/首个候选。 */
    int insert_pos = cur_count > 0 ? 1 : 0;
    if (insert_pos > cur_count) {
        insert_pos = cur_count;
    }
    if (insert_pos + ins_n > max_out) {
        ins_n = max_out - insert_pos; /* 极端：连插入位都快满了，能插几个插几个 */
        if (ins_n <= 0) {
            return cur_count;
        }
    }

    /* 把 [insert_pos, cur_count) 这段右移 ins_n；超过 max_out 的尾部丢弃
     * （丢的是最低优先级的长尾中文候选）。两个平行数组同步移动。 */
    int tail_len = cur_count - insert_pos;
    int keep_tail = max_out - insert_pos - ins_n;
    if (keep_tail > tail_len) {
        keep_tail = tail_len;
    }
    if (keep_tail > 0) {
        memmove(&cands[insert_pos + ins_n], &cands[insert_pos], (size_t)keep_tail * sizeof(cands[0]));
        memmove(&consumed[insert_pos + ins_n], &consumed[insert_pos], (size_t)keep_tail * sizeof(consumed[0]));
        /* Step FF：平行 key 数组同步右移（这个函数实际只被 refresh 链用
         * 全局缓存调用，cands==g_step_v_cached_candidates 恒成立）。 */
        memmove(&g_step_v_cached_key[insert_pos + ins_n], &g_step_v_cached_key[insert_pos],
                (size_t)keep_tail * sizeof(g_step_v_cached_key[0]));
        memmove(&g_step_v_cached_key_len[insert_pos + ins_n], &g_step_v_cached_key_len[insert_pos],
                (size_t)keep_tail * sizeof(g_step_v_cached_key_len[0]));
    }
    for (int i = 0; i < ins_n; i++) {
        cands[insert_pos + i] = ins[i];
        consumed[insert_pos + i] = (int)raw_len; /* 英文提交消耗整段缓冲区 */
        cj_step_v_set_cached_key(insert_pos + i, NULL, 0); /* 英文不参与中文调频 */
    }
    return insert_pos + ins_n + keep_tail;
}

/* 🔴 真机 bug：候选多时只展示前几个、其余选不到（用户"选不到看不到的字"）。KeyPopup 时代用分页+">>" 标记；Step BB 的 CjCandidateBar 改成一次性展示全部缓存候选（最多 CJ_STEP_V_CACHE_CAP=64），">>" 翻页拦截逻辑保留成死分支不删。详见白皮书 02 节 Step V。 */
static void cj_step_v_display_page(void *this_ptr) {
    int total = g_step_v_cached_count;
    if (total <= 0) {
        cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
        return;
    }

    const char *ptrs[CJ_STEP_V_CACHE_CAP];
    long long lens[CJ_STEP_V_CACHE_CAP];
    for (int i = 0; i < total; i++) {
        ptrs[i] = g_step_v_cached_candidates[i].word;
        lens[i] = (long long)g_step_v_cached_candidates[i].word_len;
    }
    /* Phase A（A4）：候选栏改成 iOS 干净版——只展示词候选（含首音节单字），
     * 不再注入原始字母候选（输入内容只在预览行 Step EE 显示）。原 Step FF 的
     * 可点英文候选被取消，英文整串提交暂时靠回车；等后置的 Phase C 做智能
     * 混输。这样展示下标 = 缓存下标，A-ii 的点选索引化直接对齐。 */
    cj_step_t_update_popup(this_ptr, ptrs, lens, total, 1, 1); /* is_utf8=1：候选词是真汉字 */

    CJ_LOG("[cangjie] Step BB：候选栏一次性展示全部 %d 个候选（横向可滚动，不再分页）\n", total);
}
/* ================= Step V 候选词典接入代码段结束 ================= */

/* =========== Step V-2：验证 QMetaMethod::invoke + Qt::QueuedConnection ===========
 * 背景（真机日志+重新解压 KeyPopup.qml 源码确认的真实根因，不是猜测）：
 * 原生 KeyPopup.qml 第 117 行——
 *     onReleased: { keySelected(symbol.text); root.visible = false; }
 * ——不管点的是候选栏哪一格（包括我们的翻页标记 ">>"），点完都会自动把
 * 弹窗关掉。`keySelected(symbol.text)` 是同步信号发射，会一路同步执行到
 * 我们的 insertText hook 返回，然后 `root.visible = false` 才执行——也
 * 就是说这行代码，在时间顺序上必然发生在我们的 hook 写完 visible=true
 * 之后，我们在 hook 内部怎么写都会被这行覆盖，不是时序偶然，是确定的
 * 执行顺序问题，没法靠"写快一点"赢过它。
 *
 * 出路：让我们的写入不再发生在这次 onReleased 处理的同一个调用链里——
 * 用 Qt 官方的 QueuedConnection 给自己排一个"稍后处理"的 insertText
 * 调用（带专属标记内容，区别于 ">>"），本质是自己给自己发一条延迟消息：
 * 这次排队的调用会在当前事件循环这一轮完全处理完（包括 root.visible=false
 * 已经执行完）之后才真正触发，那时候我们再写 visible=true，前面没有
 * "后续代码"会把它冲掉，因为它根本不在 onReleased 那条调用链里了。
 *
 * 第一阶段只做只读诊断（枚举 VirtualKeyboard 元对象上所有 insertText
 * 重载，比对参数个数确定全局 index）已经真机验证通过：
 *     methodCount=100，找到 3 个 insertText 重载，
 *     全局 index=87（3 参数，我们要用的那个）/88（2 参数）/89（1 参数）。
 * 印证了 Step K 反编译 FUN_00695a60 记录的"method index 22/23/24"是
 * 分派函数内部的 LOCAL id（继承链累加过的），跟这里 QMetaObject::method()
 * 用的全局 index（87/88/89）完全是两套编号，靠方法名重新核实是必要的，
 * 不能凭旧记录直接套用（Step S 的教训：形状/记录看着像不等于真的对）。
 *
 * 第二阶段（这次新增）：真正调用 invoke()。用同样的枚举方式找到 3 参数
 * 重载后缓存下来（只找一次），后续每次需要延迟刷新时直接用缓存的
 * QMetaMethod 发起 QueuedConnection 调用，不需要每次都重新枚举 100 个
 * 方法。 */

#define VK_STATICMETAOBJECT_VADDR 0x12677e8UL /* Step K 反编译确认的地址，这是第一次真正在 hook 代码里用到（此前只用来分析方法签名，没接进真机 hook，真实性还没有运行期验证过） */

typedef struct {
    const void *mobj;
    const void *data;
} CjMetaMethod; /* 16 字节，跟本机 Qt6.11 头文件 qmetaobject.h 核实过的 QMetaMethod{mobj指针; Data{d指针}} 布局一致 */

typedef struct {
    const void *data;
    const char *name;
} CjGenericArgument; /* 16 字节，跟 qobjectdefs.h 里 QGenericArgument{_data;_name} 布局一致——
                       * 注意成员顺序是 data 在前、name 在后，构造函数参数顺序（name,data）
                       * 跟内存布局顺序（data,name）不一样，容易搞反，这里按内存布局声明。 */

typedef CjMetaMethod (*qmetaobject_method_fn)(const void *this_metaobject, int index);
typedef int (*qmetaobject_methodcount_fn)(const void *this_metaobject);
/* QMetaMethod::name() 返回 QByteArray，24 字节——本机头文件核实过 QByteArray
 * 唯一成员是跟 QString 同一个模板 QArrayDataPointer<T>，只是元素类型是
 * char 不是 char16_t，布局仍然是 {d指针,ptr指针,size}，直接复用已有的
 * CjPtrList24（Step O 定义，Step T 已经在 toString() 上验证过这个 24
 * 字节 sret 手法，这次换一个来源类型，手法不变）。 */
typedef CjPtrList24 (*qmetamethod_name_fn)(const CjMetaMethod *this_method);
typedef int (*qmetamethod_paramcount_fn)(const CjMetaMethod *this_method);
typedef unsigned char (*qmetamethod_invoke_fn)(const CjMetaMethod *this_method, void *object, int connection_type,
                                                CjGenericArgument ret,
                                                CjGenericArgument a0, CjGenericArgument a1, CjGenericArgument a2,
                                                CjGenericArgument a3, CjGenericArgument a4, CjGenericArgument a5,
                                                CjGenericArgument a6, CjGenericArgument a7, CjGenericArgument a8,
                                                CjGenericArgument a9);

static qmetaobject_method_fn g_qmetaobject_method = NULL;
static qmetaobject_methodcount_fn g_qmetaobject_methodcount = NULL;
static qmetamethod_name_fn g_qmetamethod_name = NULL;
static qmetamethod_paramcount_fn g_qmetamethod_paramcount = NULL;
static qmetamethod_invoke_fn g_qmetamethod_invoke = NULL;
static int g_metamethod_symbols_attempted = 0;
static int g_metamethod_symbols_ok = 0;

/* 本机 nm（/usr/lib/libQt6Core.so.6.11.1，跟设备 6.10.3 同大版本，此前
 * 全程用同样的方式核实过符号名，还没在真机上核实过这几个具体符号，是
 * 这轮诊断本身要确认的一部分）核实过这几个符号都是真实导出、非模板的
 * 普通函数——invoke() 那个签名是 QMetaMethod::invoke(QObject*,
 * Qt::ConnectionType, QGenericReturnArgument, QGenericArgument x9) const，
 * 10 个 QGenericArgument 里只有前几个会真正用到，其余留空（默认构造，
 * data=NULL name=NULL，Qt 内部按 parameterCount 决定实际用几个）。 */
static int cj_resolve_metamethod_symbols(void) {
    if (g_metamethod_symbols_attempted) {
        return g_metamethod_symbols_ok;
    }
    g_metamethod_symbols_attempted = 1;

    g_qmetaobject_method = (qmetaobject_method_fn) dlsym(RTLD_DEFAULT, "_ZNK11QMetaObject6methodEi");
    g_qmetaobject_methodcount = (qmetaobject_methodcount_fn) dlsym(RTLD_DEFAULT, "_ZNK11QMetaObject11methodCountEv");
    g_qmetamethod_name = (qmetamethod_name_fn) dlsym(RTLD_DEFAULT, "_ZNK11QMetaMethod4nameEv");
    g_qmetamethod_paramcount = (qmetamethod_paramcount_fn) dlsym(RTLD_DEFAULT, "_ZNK11QMetaMethod14parameterCountEv");
    g_qmetamethod_invoke = (qmetamethod_invoke_fn) dlsym(RTLD_DEFAULT,
        "_ZNK11QMetaMethod6invokeEP7QObjectN2Qt14ConnectionTypeE22QGenericReturnArgument16QGenericArgumentS5_S5_S5_S5_S5_S5_S5_S5_S5_");

    g_metamethod_symbols_ok = g_qmetaobject_method && g_qmetaobject_methodcount &&
                               g_qmetamethod_name && g_qmetamethod_paramcount && g_qmetamethod_invoke;
    CJ_LOG("[cangjie] Step V-2：QMetaMethod 反射符号解析%s"
           "（method=%p methodCount=%p name=%p paramCount=%p invoke=%p）\n",
           g_metamethod_symbols_ok ? "成功" : "失败（safe mode，跳过诊断）",
           (void *)g_qmetaobject_method, (void *)g_qmetaobject_methodcount,
           (void *)g_qmetamethod_name, (void *)g_qmetamethod_paramcount, (void *)g_qmetamethod_invoke);
    return g_metamethod_symbols_ok;
}

static int g_step_v2_lookup_done = 0;
static CjMetaMethod g_step_v2_inserttext_method;
static int g_step_v2_inserttext_method_ok = 0;

/* 枚举 VirtualKeyboard 元对象上所有方法，找名字是 "insertText" 且参数
 * 个数==3 的那个重载，缓存它的 QMetaMethod——只找一次（跟 Step P 最初的
 * 一次性验证同一个节奏），后续直接用缓存值，不用每次都枚举 100 个方法。
 * 真机验证过：methodCount=100，3 个 insertText 重载全局 index 分别是
 * 87（3参数，我们要的）/88（2参数）/89（1参数）——只用名字+参数个数匹配，
 * 不硬编码 87 这个数字，避免固件小版本更新后编号漂移导致悄悄用错方法。 */
static int cj_step_v2_ensure_inserttext_method(void) {
    if (g_step_v2_lookup_done) {
        return g_step_v2_inserttext_method_ok;
    }
    g_step_v2_lookup_done = 1;

    if (!cj_resolve_metamethod_symbols() || !g_vk_metaobject) {
        CJ_LOG("[cangjie] Step V-2：符号或 VK metaobject 缺失，放弃查找 insertText 方法\n");
        return 0;
    }

    const void *vk_metaobject = (const void *)g_vk_metaobject;
    int count = g_qmetaobject_methodcount(vk_metaobject);
    if (count <= 0 || count > 200) {
        CJ_LOG("[cangjie] Step V-2：methodCount=%d 数值不合理，怀疑 staticMetaObject 地址不对，放弃\n", count);
        return 0;
    }

    for (int i = 0; i < count; i++) {
        CjMetaMethod m = g_qmetaobject_method(vk_metaobject, i);
        CjPtrList24 name = g_qmetamethod_name(&m);
        if (name.size == 10 && name.ptr && memcmp(name.ptr, "insertText", 10) == 0 &&
            g_qmetamethod_paramcount(&m) == 3) {
            g_step_v2_inserttext_method = m;
            g_step_v2_inserttext_method_ok = 1;
            CJ_LOG("[cangjie] Step V-2：找到 insertText(QString,int,int)，全局index=%d，已缓存\n", i);
            return 1;
        }
    }
    CJ_LOG("[cangjie] Step V-2：没找到 3 参数的 insertText 重载，放弃（延迟刷新功能不生效，"
           "翻页仍然可用，只是原生自动关闭问题不会被修复）\n");
    return 0;
}

/* CJ_STEP_V_REFRESH_MARKER 已经在文件前面（跟 CJ_STEP_V_PAGE_MARKER 一起）
 * 定义过，这里不重复定义，直接用。 */

/* 给自己排一个"稍后处理"的 insertText(REFRESH_MARKER, 0, 0) 调用——
 * Qt::QueuedConnection 语义下，QMetaMethod::invoke() 内部会把参数深拷贝
 * 进一个事件对象、post 给 object 所在线程的事件队列，立即返回（不等
 * 真正执行），真正的调用会在当前事件循环这一轮完全处理完（包括原生
 * KeyPopup.qml 第 117 行 root.visible=false 已经跑完）之后才触发——那时
 * 候我们在 cj_vk_insert_text_handler 里认出这个标记，重新写一次
 * visible=true，前面不会再有"后续代码"把它冲掉。
 *
 * 失败（符号缺失/没找到方法/QString 构造失败）时安静放弃，不影响翻页
 * 本身——退化到"翻页能用但可能被原生自动关闭"这个已知的、之前就存在的
 * 状态，不会更差。 */
static void cj_step_v2_queue_refresh(void *this_ptr) {
    if (!cj_step_v2_ensure_inserttext_method() || !cj_resolve_qt_symbols()) {
        return;
    }

    uint8_t qstr[24];
    if (!cj_build_ascii_qstring(qstr, CJ_STEP_V_REFRESH_MARKER, CJ_STEP_V_REFRESH_MARKER_LEN, g_qarraydata_allocate)) {
        CJ_LOG("[cangjie] Step V-2：构造刷新标记 QString 失败，放弃排队\n");
        return;
    }
    static const int zero_from = 0;
    static const int zero_len = 0;

    CjGenericArgument ret = {0};
    CjGenericArgument a_text = { .data = qstr, .name = "QString" };
    CjGenericArgument a_from = { .data = &zero_from, .name = "int" };
    CjGenericArgument a_len = { .data = &zero_len, .name = "int" };
    CjGenericArgument a_empty = {0};

    unsigned char ok = g_qmetamethod_invoke(&g_step_v2_inserttext_method, this_ptr,
                                             2 /* Qt::QueuedConnection，本机 qnamespace.h 核实过的枚举值 */,
                                             ret, a_text, a_from, a_len,
                                             a_empty, a_empty, a_empty, a_empty, a_empty, a_empty, a_empty);
    CJ_LOG("[cangjie] Step V-2：排队延迟刷新调用%s\n", ok ? "成功" : "失败");
}
/* =========== Step V-2 代码段结束 ===========
 * 注：qstr（局部栈变量）在 invoke() 调用期间构造好并传入——QueuedConnection
 * 语义下 Qt 会在 invoke() 返回之前就把参数深拷贝进事件对象，不依赖 qstr
 * 在函数返回之后继续存活，这是 Qt 官方文档记录的标准行为，不是新的假设。
 * qstr 指向的 QString 数据本身（QArrayData::allocate 出来的堆内存）不会
 * 被这次深拷贝额外持有引用——这里沿用整个项目一贯的取舍：不手动处理这份
 * 内存的生命周期/引用计数（Step G 开头就写明的原则），信任 Qt 深拷贝时
 * 走的是标准隐式共享 QString 拷贝语义（引用计数 +1，不是真的复制底层
 * 字符数据），我们自己这边只是"多了一次极小的、有界的引用计数持有"，
 * 不是内存泄漏在无限增长。 */

/* Phase B：双拼开关已从"标记文件+一次性缓存（需重启生效）"改为编码进伪
 * 语言代码（zh_CN_SP/zh_TW_SP，见 Step R 真源表 CJ_VK_MODES），由
 * cj_step_t_language_mode_ex 每次按键读 language 属性得出 shuangpin 位——
 * 切模式立即生效、无需重启。原 cj_shuangpin_is_enabled()/标记文件机制已删除，
 * 设备上残留的 /home/root/.cangjie_shuangpin_enabled 不再被读、失效即可。 */

/* ===================================================================
 * Step Z：候选栏显示节流——双拼真机测试暴露的真实 bug（用户截图+日志
 * 确认）：连续快速按同一个键（真机日志显示反复敲 "g"，同一秒内多次
 * press/release），候选集合在极短时间内剧烈变化——双拼下相邻两次按键
 * 经常从"完整音节+一大批真候选"跳到"半个音节+寥寥几个候选"再跳回去
 * （缓冲区长度奇偶交替，见 cj_step_v_generate_jianpin_candidates 之外
 * 那段主逻辑：偶数长度是完整音节序列走精确匹配，奇数长度是"pending"
 * 走前缀匹配，两条路径命中的候选集合可以是完全不同的两批），比全拼下
 * 逐字母收窄同一个前缀匹配摆动幅度大得多。原生 KeyPopup 的 Repeater
 * 网格来不及在两次刷新之间完成重新布局，就会出现新旧两批候选叠在同一
 * 屏幕位置的视觉重影——真机截图证实：连续敲同一个字母时候选栏里能看到
 * 好几份候选文字互相压在一起，不是数据错乱，是纯粹的显示时序问题。
 *
 * 节流只影响"隔多久真正刷新一次可见候选栏"，不影响正确性：内部状态
 * （g_step_v_cached_candidates/g_step_v_top_candidate_utf8，空格/回车
 * 提交时真正用到的数据）每次按键都照常重新计算，被节流跳过的只是这一次
 * 的可见刷新；跳过的那次不会永久丢失候选栏更新——复用已经真机验证过的
 * Step V-2 延迟刷新机制（QMetaMethod::invoke+Qt::QueuedConnection）补
 * 一次，突发按键停下来之后候选栏一定会展示到最新状态，不会永久卡在
 * 某个中间态（连续节流期间可能排queue 好几次冗余的延迟刷新调用，每次
 * 都是幂等的重新展示当前最新缓存页，不是错误累积，Qt 事件队列正常
 * 排空即可，不需要额外去重）。
 *
 * 只节流"展示有内容的候选栏"这两处调用点（真候选分页 cj_step_v_display_page
 * / 拼音切分兜底展示），不节流"隐藏候选栏"（NULL/count=0 的调用）——
 * 隐藏必须立即生效，延迟隐藏会让候选栏在缓冲区已经清空之后还短暂停留
 * 一个陈旧的可见状态，比"稍微慢一点刷新新内容"更容易让用户困惑。 */
#define CJ_STEP_Z_DISPLAY_THROTTLE_MS 80 /* 正常人类打字间隔通常大于这个数（10 键/秒≈100ms/键都很快了），
                                           * 只会拦住比这快得多的连续按键（自动重复/快速连按），不影响真实打字手感 */
static long long g_step_z_last_display_ms = 0;

static long long cj_monotonic_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0; /* 极端失败兜底：返回 0，下面的节流判断会退化成"永远不节流"，
                     * 不会导致候选栏卡死不更新，只是这次没能正确节流，不是新的
                     * 失败模式。 */
    }
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

/* 返回 1 表示可以（也确实会）刷新一次可见候选栏，调用方紧接着应该真的
 * 去刷新；返回 0 表示这次太快了、跳过，调用方应该转而调用
 * cj_step_v2_queue_refresh(this_ptr) 补一次延迟刷新。 */
static int cj_step_z_should_display_now(void) {
    long long now = cj_monotonic_ms();
    if (now != 0 && g_step_z_last_display_ms != 0 &&
        now - g_step_z_last_display_ms < CJ_STEP_Z_DISPLAY_THROTTLE_MS) {
        return 0;
    }
    g_step_z_last_display_ms = now;
    return 1;
}
/* ================= Step Z 节流代码段结束 ================= */

/* 用当前缓冲区内容刷新候选栏——优先查词典拿真汉字候选（Step V），词典
 * 没命中（没开成功/确实查不到）时退化成 cj_segment() 的拼音切分展示
 * （Step T 原有逻辑，用全部候选切法而不只是最优切法，展示口味更丰富，
 * 反正只是给用户看拼音怎么切的，不是真的要提交）。缓冲区为空时直接
 * 清空+隐藏候选栏。Step Y：双拼开启时，先把 g_pinyin_buffer（双拼
 * 按键原文）转换成标准拼音字符串，再喂给下面同一套切分/候选逻辑——
 * 04 节设计"不需要单独的引擎，加一层键位转换预处理层"在这里就是这样
 * 落地的，除了转换这一步，后面的代码一行都不需要为双拼单独分支。 */
static void cj_step_t_refresh_candidates(void *this_ptr) {
    if (g_pinyin_buffer_len == 0) {
        g_step_v_top_candidate_len = 0;
        g_step_v_cached_count = 0;
        g_step_v_preview_len = 0;
        cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
        return;
    }

    /* 05 节繁体双 blob + Step X 简拼 + Step Y 双拼：只读一次
     * virtualKeyboard.language，同一个 mode 同时喂给词典和简拼两个
     * 选择器（this_ptr 是 NULL 理论上不该发生，call-through 前已经
     * 确认过是中文模式，退化成 CJ_LANG_OTHER 沿用既有 fail-safe 原则）。
     * 提前到函数最前面算好——Step Y 判断"要不要先做双拼转换"也需要
     * 用到它，比原来晚算这一步的位置更早。 */
    int shuangpin_active = 0;
    CjLanguageMode lang_mode = cj_step_t_language_mode_ex(this_ptr, &shuangpin_active);
    /* shuangpin_active 只对 zh_*_SP 置位，已隐含 lang_mode != CJ_LANG_OTHER */

    /* Step Y：双拼开启时 g_pinyin_buffer 存双拼按键原文，先 cj_shuangpin_keys_to_pinyin 转成标准拼音（音节间用单引号分隔），之后 seg_src 统一喂下游、不为双拼开分支；decoded 超 CJ_SEG_MAX_INPUT 会被安全截断不越界。详见白皮书 04 节 / 02 节 Step Y。 */
    char decoded[CJ_SEG_MAX_INPUT];
    const char *seg_src = g_pinyin_buffer;
    size_t seg_src_len = (size_t)g_pinyin_buffer_len;
    if (shuangpin_active) {
        size_t decoded_needed = cj_shuangpin_keys_to_pinyin(
            g_pinyin_buffer, (size_t)g_pinyin_buffer_len, decoded, sizeof(decoded));
        seg_src = decoded;
        seg_src_len = decoded_needed < sizeof(decoded) ? decoded_needed : sizeof(decoded) - 1;
    }

    /* Step EE：缓存这一轮的"预览文本"——全拼模式下就是原样拼音字母，
     * 双拼模式下是刚才转换出来的标准拼音（用户明确要求"双拼要显示自动
     * 转成的拼音字母，不是双拼按键原文"）。seg_src 已经是这两种情况
     * 统一之后的结果，直接缓存，不需要再判断一次是不是双拼。 */
    g_step_v_preview_len = (int)(seg_src_len < sizeof(g_step_v_preview_text)
                                      ? seg_src_len
                                      : sizeof(g_step_v_preview_text) - 1);
    memcpy(g_step_v_preview_text, seg_src, (size_t)g_step_v_preview_len);

    char lowercased[CJ_SEG_MAX_INPUT];
    CjSegmentation best_seg;
    if (!cj_best_segmentation(seg_src, seg_src_len, lowercased, &best_seg)) {
        CJ_LOG("[cangjie] Step T：cj_best_segmentation 失败（不应该发生，buffer_len=%d，双拼=%s），隐藏候选栏\n",
               g_pinyin_buffer_len, shuangpin_active ? "开" : "关");
        g_step_v_top_candidate_len = 0;
        g_step_v_cached_count = 0;
        cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
        return;
    }

    CjDictHandle *dict_handle = cj_dict_select_handle_for_mode(lang_mode);

    g_step_v_current_page = 0;
    g_step_v_cached_count = cj_step_v_generate_dict_candidates(&best_seg, lowercased, dict_handle,
                                                                shuangpin_active,
                                                                g_step_v_cached_candidates,
                                                                g_step_v_cached_consumed,
                                                                CJ_STEP_V_CACHE_CAP);

    /* Step X：全拼候选查完、缓存槽位有余就补一次简拼查询（用整段缓冲区，去重后追加，handle 为 NULL 则跳过，fail-safe）。Step Y：双拼开启时整段跳过（双拼按键当简拼 key 无意义）。详见白皮书 07 节。 */
    if (!shuangpin_active && g_step_v_cached_count < CJ_STEP_V_CACHE_CAP) {
        const CjJianpinHandle *jianpin_handle = cj_jianpin_select_handle(lang_mode);
        int jianpin_added = cj_step_v_generate_jianpin_candidates(
            lowercased, (size_t)g_pinyin_buffer_len, jianpin_handle,
            g_step_v_cached_candidates, g_step_v_cached_consumed,
            g_step_v_cached_count, CJ_STEP_V_CACHE_CAP);
        if (jianpin_added > 0 && g_step_t_diag_log_count < CJ_STEP_T_DIAG_LOG_MAX) {
            CJ_LOG("[cangjie] Step X：简拼补充了 %d 个候选（全拼已命中 %d 个）\n",
                   jianpin_added, g_step_v_cached_count);
        }
        g_step_v_cached_count += jianpin_added;
    }

    /* Phase C：中英混输——中文/简拼候选都填完后，把原始拉丁缓冲区当英文
     * 前缀查 english.bin，命中的英文词按词频插在第一名中文候选之后。与
     * 模式无关（查原始按键），fail-safe（词典没打开原样返回）。这是候选
     * 缓存的唯一填充点，改这一处即覆盖展示/点选/提交全部下游。 */
    if (g_pinyin_buffer_len >= 1) {
        g_step_v_cached_count = cj_step_c_add_english_candidates(
            g_pinyin_buffer, (size_t)g_pinyin_buffer_len,
            g_step_v_cached_candidates, g_step_v_cached_consumed,
            g_step_v_cached_count, CJ_STEP_V_CACHE_CAP);
    }

    /* Step GG：快捷输入——原始按键缓冲区（小写化后）精确命中 snippets.tsv
     * 里的缩写时，把短语插到候选最前面（挤掉的尾部丢弃）。与模式无关
     * （跟英文候选同理：用户敲的就是缩写字面）。点选/空格消耗整串；
     * key_len=0 不参与词频调频（显式意图不需要学习）。短语指针指向
     * g_snippets 内部，热重载只发生在本函数（下一轮刷新），提交路径在
     * 重载前已用完指针，生命周期安全。 */
    if (g_pinyin_buffer_len >= 1 && g_pinyin_buffer_len <= CJ_SN_SHORTCUT_MAX) {
        cj_sn_refresh(&g_snippets, CJ_SNIPPETS_PATH);
        char sn_key[CJ_SN_SHORTCUT_MAX];
        for (int i = 0; i < g_pinyin_buffer_len; i++) {
            char c = g_pinyin_buffer[i];
            sn_key[i] = (c >= 'A' && c <= 'Z') ? (char)(c + 32) : c;
        }
        const CjSnEntry *hits[CJ_SN_INJECT_MAX];
        int hn = cj_sn_lookup(&g_snippets, sn_key, (size_t)g_pinyin_buffer_len,
                              hits, CJ_SN_INJECT_MAX);
        if (hn > 0) {
            int keep = g_step_v_cached_count;
            if (keep > CJ_STEP_V_CACHE_CAP - hn) {
                keep = CJ_STEP_V_CACHE_CAP - hn; /* 尾部溢出丢弃 */
            }
            if (keep > 0) {
                memmove(&g_step_v_cached_candidates[hn], &g_step_v_cached_candidates[0],
                        (size_t)keep * sizeof(g_step_v_cached_candidates[0]));
                memmove(&g_step_v_cached_consumed[hn], &g_step_v_cached_consumed[0],
                        (size_t)keep * sizeof(g_step_v_cached_consumed[0]));
                memmove(&g_step_v_cached_key[hn], &g_step_v_cached_key[0],
                        (size_t)keep * sizeof(g_step_v_cached_key[0]));
                memmove(&g_step_v_cached_key_len[hn], &g_step_v_cached_key_len[0],
                        (size_t)keep * sizeof(g_step_v_cached_key_len[0]));
            }
            for (int i = 0; i < hn; i++) {
                g_step_v_cached_candidates[i].word = hits[i]->phrase;
                g_step_v_cached_candidates[i].word_len = hits[i]->phrase_len;
                g_step_v_cached_candidates[i].weight = 0;
                g_step_v_cached_consumed[i] = g_pinyin_buffer_len;
                cj_step_v_set_cached_key(i, NULL, 0);
            }
            g_step_v_cached_count = keep + hn;
            if (g_step_t_diag_log_count < CJ_STEP_T_DIAG_LOG_MAX) {
                CJ_LOG("[cangjie] Step GG：快捷输入命中 %d 条（缩写 \"%.*s\"）\n",
                       hn, g_pinyin_buffer_len, sn_key);
            }
        }
    }

    if (g_step_v_cached_count > 0) {
        /* Step Z：内部候选状态（下面几行 memcpy 出的"第一名"缓存）永远
         * 照常更新，节流只影响这一行"要不要真的把内容画到候选栏上"，
         * 见 Step Z 代码段顶部说明。 */
        if (cj_step_z_should_display_now()) {
            cj_step_v_display_page(this_ptr);
        } else {
            CJ_LOG("[cangjie] Step Z：按键过快，跳过这次候选栏可见刷新，排队补一次\n");
            cj_step_v2_queue_refresh(this_ptr);
        }

        size_t top_len = g_step_v_cached_candidates[0].word_len < CJ_STEP_V_TOP_CANDIDATE_CAP
                              ? g_step_v_cached_candidates[0].word_len
                              : CJ_STEP_V_TOP_CANDIDATE_CAP;
        memcpy(g_step_v_top_candidate_utf8, g_step_v_cached_candidates[0].word, top_len);
        g_step_v_top_candidate_len = (int)top_len;

        if (g_step_t_diag_log_count < CJ_STEP_T_DIAG_LOG_MAX) {
            CJ_LOG("[cangjie] Step V：命中 %d 个真候选，第一名=\"%.*s\"\n",
                   g_step_v_cached_count, (int)g_step_v_cached_candidates[0].word_len,
                   g_step_v_cached_candidates[0].word);
            g_step_t_diag_log_count++;
        }
        return;
    }

    /* 词典没命中——退化回拼音切分展示，不给空白候选栏。 */
    g_step_v_top_candidate_len = 0;

    CjSegmentation candidates[CJ_SEG_MAX_CANDIDATES];
    int count = 0;
    if (!cj_segment(seg_src, seg_src_len, lowercased,
                     candidates, CJ_SEG_MAX_CANDIDATES, &count) || count <= 0) {
        CJ_LOG("[cangjie] Step T：cj_segment 失败或无候选（不应该发生，seg_src_len=%zu，双拼=%s），隐藏候选栏\n",
               seg_src_len, shuangpin_active ? "开" : "关");
        cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
        return;
    }

    int display_count = count < CJ_STEP_T_MAX_DISPLAY_CANDIDATES ? count : CJ_STEP_T_MAX_DISPLAY_CANDIDATES;
    char rendered[CJ_STEP_T_MAX_DISPLAY_CANDIDATES][CJ_SEG_MAX_INPUT * 2];
    const char *ptrs[CJ_STEP_T_MAX_DISPLAY_CANDIDATES];
    long long text_lens[CJ_STEP_T_MAX_DISPLAY_CANDIDATES];
    for (int i = 0; i < display_count; i++) {
        long long n = cj_step_t_render_segmentation(&candidates[i], lowercased, rendered[i],
                                                      sizeof(rendered[i]) - 1);
        rendered[i][n] = '\0';
        ptrs[i] = rendered[i];
        text_lens[i] = n;
    }
    /* Step Z：跟真候选路径同一个节流判断，理由同上——拼音切分兜底展示
     * 反而更容易触发这个问题（双拼下一敲错、词典没命中就会掉进这条
     * 路径，配合快速连按更容易出现视觉重影）。 */
    if (cj_step_z_should_display_now()) {
        /* Phase A（A4）：拼音切分兜底展示也改回 iOS 干净版，不注入英文候选。 */
        cj_step_t_update_popup(this_ptr, ptrs, text_lens, display_count, 1, 0); /* is_utf8=0：拼音字母 */
    } else {
        CJ_LOG("[cangjie] Step Z：按键过快，跳过这次候选栏可见刷新（拼音切分兜底路径），排队补一次\n");
        cj_step_v2_queue_refresh(this_ptr);
    }

    if (g_step_t_diag_log_count < CJ_STEP_T_DIAG_LOG_MAX) {
        cj_log_keypopup_all_properties("Step T：候选栏刷新之后");
        g_step_t_diag_log_count++;
    }
}

/* 读一次 virtualKeyboard.language，一次性判断 zh_CN/zh_TW/其它——
 * cj_step_t_is_chinese_mode()（要不要进拼音拦截）和 05 节繁体双 blob
 * 的 cj_dict_select_handle_for_mode()（该查哪份词典）是同一次属性读取的两种
 * 不同用法，不该分别各读一次属性：那样会让下面这段本来就已经记录、
 * 接受的"有界小引用计数泄漏"翻倍，没有必要。复用 Step P/Q 已经验证
 * 过的 QObject::property()+QVariant::toString() 手法。任何必需符号
 * 缺失都当 CJ_LANG_OTHER 处理（fail-safe：宁可不拦截、退化成完全
 * 原生行为，也不要"想拦截但拦不住"）。
 *
 * 已知取舍（计划文件 Step T 有完整说明，这里简述）：nm 没找到独立导出的
 * QString 析构符号（大概率被内联），这里不手动摆弄引用计数（Step S 的
 * 教训：内存布局细节不确定时不要手写内存操作）——接受一个有界、每次判断
 * 可能产生的小引用计数"泄漏"，量级远小于 Step G 已经明确接受的"不释放旧
 * 后备数组"，这轮不做进一步优化。 */
static CjLanguageMode cj_step_t_language_mode_ex(void *this_ptr, int *out_shuangpin) {
    if (out_shuangpin) {
        *out_shuangpin = 0;
    }
    if (!this_ptr || !cj_resolve_keypopup_write_symbols() || !g_qvariant_tostring) {
        return CJ_LANG_OTHER;
    }
    CjVariant32 v = g_qobject_property(this_ptr, "language");
    CjPtrList24 s = g_qvariant_tostring(&v);
    CjLanguageMode mode = CJ_LANG_OTHER;
    int sp = 0;
    /* Phase B：用 Step R 真源表把伪代码解析成 (简/繁 base, 双拼位)——4 个
     * 中文模式共用这一次属性读取，不为双拼另开一次读。 */
    for (size_t i = 0; i < CJ_VK_MODES_COUNT; i++) {
        if (cj_qstring_equals_ascii((const uint8_t *)&s, CJ_VK_MODES[i].code, CJ_VK_MODES[i].code_len)) {
            mode = CJ_VK_MODES[i].base;
            sp = CJ_VK_MODES[i].shuangpin;
            break;
        }
    }
    g_qvariant_dtor(&v);
    if (out_shuangpin) {
        *out_shuangpin = sp;
    }
    return mode;
}

static CjLanguageMode cj_step_t_language_mode(void *this_ptr) {
    return cj_step_t_language_mode_ex(this_ptr, NULL);
}

static int cj_step_t_is_chinese_mode(void *this_ptr) {
    return cj_step_t_language_mode(this_ptr) != CJ_LANG_OTHER;
}

/* 提交当前状态，然后清空缓冲区、隐藏候选栏。
 *
 * force_raw_pinyin 为真（回车键专用）：不管候选栏当前有没有真词典
 * 候选，一律原样提交缓冲区拉丁字母——真机验证时用户反馈"按回车不是
 * 输入英文而是输入第一候选字"：Step V 刚接入真候选那版，空格和回车
 * 共用同一段"优先提交第一名真候选"的逻辑，导致回车也变成了确认候选，
 * 但主流拼音输入法的通用约定是"空格=确认默认候选，回车=放弃候选、
 * 原样提交刚打的拉丁字母"（常见场景：打到一半发现其实想打的是英文
 * 单词，不想经过候选栏）——这个参数就是让调用方能表达这两种不同意图，
 * 不是新逻辑，是把 Step V 误合并到一起的两种行为重新分开。
 *
 * force_raw_pinyin 为假（空格及其它安全网调用点）：维持 Step V 原有
 * 行为——候选栏正显示真词典候选（g_step_v_top_candidate_len > 0）就
 * 提交那个候选（真汉字，UTF-8），否则（词典没命中，候选栏在展示拼音
 * 切分兜底）退回 Step T 原有行为，提交缓冲区原样拼音字母。
 *
 * insertText 调用桩缺失（Step L hook 没装成功）时只清空本地状态，不
 * 尝试其它提交方式——宁可丢弃这次没提交成的内容，也不要在不确定的
 * 路径上硬提交。 */
static void cj_step_t_commit_buffer(void *this_ptr, int force_raw_pinyin) {
    if (g_pinyin_buffer_len > 0 && g_orig_vk_insert_text_call_through && cj_resolve_qt_symbols()) {
        uint8_t qstr[24];
        int built;
        if (!force_raw_pinyin && g_step_v_top_candidate_len > 0) {
            built = cj_build_utf8_qstring(qstr, g_step_v_top_candidate_utf8,
                                           g_step_v_top_candidate_len, g_qarraydata_allocate);
            if (built) {
                CJ_LOG("[cangjie] Step V：提交第一名真候选 \"%.*s\"\n",
                       g_step_v_top_candidate_len, g_step_v_top_candidate_utf8);
            }
        } else {
            built = cj_build_ascii_qstring(qstr, g_pinyin_buffer, g_pinyin_buffer_len, g_qarraydata_allocate);
            if (built) {
                CJ_LOG("[cangjie] Step T：提交缓冲区 \"%.*s\"（原样拼音%s）\n",
                       g_pinyin_buffer_len, g_pinyin_buffer,
                       force_raw_pinyin ? "，回车键强制原样提交" : "，词典未命中");
            }
        }
        if (built) {
#ifdef CJ_COMMIT_VIA_IME
            /* B：空格/回车提交也优先走 commitString，失败回退 insertText。 */
            if (!cj_commit_via_ime(qstr, 0, 0))
                g_orig_vk_insert_text_call_through(this_ptr, qstr, 0, 0);
#else
            g_orig_vk_insert_text_call_through(this_ptr, qstr, 0, 0);
#endif
        } else {
            CJ_LOG("[cangjie] Step T：提交缓冲区失败——构造 QString 失败，内容丢弃\n");
        }
    }
    g_pinyin_buffer_len = 0;
    g_step_v_top_candidate_len = 0;
    g_step_v_cached_count = 0;
    cj_compose_stack_reset(); /* A-iii：空格/回车/安全网提交都 finalize composition，之后退格转原生 */
    cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
}

/* Phase A（A-ii 分段提交核心）：提交缓存里第 idx 个候选，然后从缓冲区裁掉
 * 它消耗的那几个原始按键字节，对剩余部分重新切分继续显示候选——这就是
 * iOS 逐字造句：打 shenme 点"神"→上屏"神"、缓冲区剩"me"、候选栏变成 me
 * 的候选。consumed==整串时等价于旧的整段提交（清空+隐藏）。idx 越界一律
 * fail-safe 成"提交第一名+清空"（不崩、不留悬空状态）。 */
#ifdef CJ_COMMIT_VIA_IME
/* =====================================================================
 * B（commitString 注入，2026-08-16）：xochitl 普通字符键在 FUN_00697610 里就是
 * 内联构造 QInputMethodEvent+setCommitString+sendEvent 注入文本（见 Step M 注释）。
 * 这里复刻同一通道，用**公开 Qt 符号（dlsym 按名解析、固件无关）**替掉提交路径对私有
 * FUN_00695970(insertText 实现) 的特征码依赖——少一处脆弱特征码。全部符号已离线核实
 * 导出（设备 libQt6Gui/Core.6.10.3，@@Qt_6 公开）。sizeof(QInputMethodEvent)=96。
 * 编译开关 CJ_COMMIT_VIA_IME 打开才走这条，失败自动回退 insertText，便于对照与回退。
 * ===================================================================== */
typedef void *(*cj_focusobject_fn)(void);
typedef void  (*cj_ime_ctor_fn)(void *ev);
typedef void  (*cj_ime_setcommit_fn)(void *ev, const void *qstr_ref, int from, int len);
typedef void  (*cj_ime_dtor_fn)(void *ev);
typedef unsigned char (*cj_sendevent_fn)(void *receiver, void *ev);

static cj_focusobject_fn   g_qgui_focusobject = NULL;
static cj_ime_ctor_fn      g_ime_ctor = NULL;
static cj_ime_setcommit_fn g_ime_setcommit = NULL;
static cj_ime_dtor_fn      g_ime_dtor = NULL;
static cj_sendevent_fn     g_qcore_sendevent = NULL;
static int g_ime_symbols_attempted = 0;
static int g_ime_symbols_ok = 0;

static int cj_resolve_ime_symbols(void) {
    if (g_ime_symbols_attempted) return g_ime_symbols_ok;
    g_ime_symbols_attempted = 1;
    g_qgui_focusobject = (cj_focusobject_fn) dlsym(RTLD_DEFAULT, "_ZN15QGuiApplication11focusObjectEv");
    g_ime_ctor         = (cj_ime_ctor_fn) dlsym(RTLD_DEFAULT, "_ZN17QInputMethodEventC1Ev");
    g_ime_setcommit    = (cj_ime_setcommit_fn) dlsym(RTLD_DEFAULT,
                             "_ZN17QInputMethodEvent15setCommitStringERK7QStringii");
    g_ime_dtor         = (cj_ime_dtor_fn) dlsym(RTLD_DEFAULT, "_ZN17QInputMethodEventD1Ev");
    g_qcore_sendevent  = (cj_sendevent_fn) dlsym(RTLD_DEFAULT,
                             "_ZN16QCoreApplication9sendEventEP7QObjectP6QEvent");
    g_ime_symbols_ok = g_qgui_focusobject && g_ime_ctor && g_ime_setcommit
                       && g_ime_dtor && g_qcore_sendevent;
    CJ_LOG("[cangjie] B：IME 符号解析%s（focusObject=%p ctor=%p setCommit=%p dtor=%p sendEvent=%p）\n",
           g_ime_symbols_ok ? "成功" : "失败（safe mode，回退 insertText）",
           (void*)g_qgui_focusobject, (void*)g_ime_ctor, (void*)g_ime_setcommit,
           (void*)g_ime_dtor, (void*)g_qcore_sendevent);
    return g_ime_symbols_ok;
}

/* 用 QInputMethodEvent commitString 往当前焦点对象注入文本。qstr_ref 指向一个 QString
 * （跟 insertText 桩收的同款 24 字节栈 QString）。from/len 传给 setCommitString 的
 * replaceFrom/replaceLength（删改用，退格撤销可复用）。返回 1=已注入、0=未注入
 * （符号缺失或无焦点，调用方回退 insertText）。 */
static int cj_commit_via_ime(const void *qstr_ref, int replace_from, int replace_len) {
    if (!cj_resolve_ime_symbols()) return 0;
    void *fo = g_qgui_focusobject();
    if (!fo) { CJ_LOG("[cangjie] B：focusObject=NULL，跳过 commitString 注入\n"); return 0; }
    /* QInputMethodEvent 6.10 sizeof=96，预留 128 且 16 对齐足够；真实 ctor/dtor 负责
     * 管理内部 QString/QList 成员的分配与释放，不能只 memset。 */
    unsigned char ev[128] __attribute__((aligned(16)));
    g_ime_ctor(ev);
    g_ime_setcommit(ev, qstr_ref, replace_from, replace_len);
    g_qcore_sendevent(fo, ev);
    g_ime_dtor(ev);
    return 1;
}
#endif /* CJ_COMMIT_VIA_IME */

static void cj_step_t_commit_candidate_by_index(void *this_ptr, int idx) {
    if (idx < 0 || idx >= g_step_v_cached_count || g_pinyin_buffer_len <= 0) {
        cj_step_t_commit_buffer(this_ptr, 0);
        return;
    }

    CjDictCandidate cand = g_step_v_cached_candidates[idx];
    int consumed = g_step_v_cached_consumed[idx];
    if (consumed <= 0) {
        consumed = g_pinyin_buffer_len; /* 兜底：没记录就当整串消耗 */
    }
    if (consumed > g_pinyin_buffer_len) {
        consumed = g_pinyin_buffer_len;
    }

    /* 1) 提交候选文字（真汉字 UTF-8）——在重新切分之前，避免 cand.word
     * 指向的 g_step_v_composed_text 被 refresh 覆盖。 */
    if (g_orig_vk_insert_text_call_through && cj_resolve_qt_symbols()) {
        uint8_t qstr[24];
        if (cj_build_utf8_qstring(qstr, cand.word, cand.word_len, g_qarraydata_allocate)) {
            CJ_LOG("[cangjie] Phase A：提交候选 \"%.*s\"（消耗 %d/%d 字节）\n",
                   (int)cand.word_len, cand.word, consumed, g_pinyin_buffer_len);
#ifdef CJ_COMMIT_VIA_IME
            /* B：优先走公开 QInputMethodEvent commitString 通道；失败自动回退 insertText。 */
            if (!cj_commit_via_ime(qstr, 0, 0))
                g_orig_vk_insert_text_call_through(this_ptr, qstr, 0, 0);
#else
            g_orig_vk_insert_text_call_through(this_ptr, qstr, 0, 0);
#endif
        } else {
            CJ_LOG("[cangjie] Phase A：候选 QString 构造失败，跳过提交\n");
        }
    }

    /* A-iii：压入撤销栈——记录这次提交消耗掉的拼音字节 + 该词占的 UTF-16
     * 码元数，供退格撤销时删文本框的字、还原拼音重选。必须在下面裁缓冲区
     * 之前拷贝 pinyin（裁掉之后 g_pinyin_buffer[0..consumed) 就没了）。栈满
     * 就丢最旧的（整体前移一格），不阻塞后续提交——极长不空格连打才可能
     * 触发，丢掉的最旧几个字变成"只能原生退格删、不还原拼音"，可接受。 */
    if (g_compose_stack_len >= CJ_COMPOSE_STACK_MAX) {
        memmove(&g_compose_stack[0], &g_compose_stack[1],
                sizeof(CjComposeEntry) * (CJ_COMPOSE_STACK_MAX - 1));
        g_compose_stack_len = CJ_COMPOSE_STACK_MAX - 1;
    }
    {
        CjComposeEntry *e = &g_compose_stack[g_compose_stack_len++];
        memcpy(e->pinyin, g_pinyin_buffer, (size_t)consumed);
        e->pinyin_len = consumed;
        e->textfield_units = cj_utf8_to_utf16_units(cand.word, (int)cand.word_len);
        g_ime_focus_obj = cj_current_focus_object(); /* A-iv：撤销条目归属当前焦点 */
    }

    /* Step FF：记用户选择 (key, word) 计数 + 立刻原子保存。⚠ 顺序约束：
     * cand.word 可能指向 g_userfreq 内部条目（强制召回的词），record 会
     * memmove 条目数组——所以先把 key/word 拷到本地再调 record，且必须在
     * 上面"提交候选文字"用完 cand.word 之后。record 之后缓存里其它候选
     * 的 word 指针可能失效，但下面两条路都会重建/清空缓存，不会再被读。 */
    if (idx < CJ_STEP_V_CACHE_CAP && g_step_v_cached_key_len[idx] > 0 &&
        cand.word_len <= CJ_UF_WORD_MAX) {
        char rec_key[CJ_UF_KEY_MAX];
        char rec_word[CJ_UF_WORD_MAX];
        size_t rec_key_len = g_step_v_cached_key_len[idx];
        memcpy(rec_key, g_step_v_cached_key[idx], rec_key_len);
        memcpy(rec_word, cand.word, cand.word_len);
        cj_userfreq_ensure_loaded();
        cj_uf_record(&g_userfreq, rec_key, rec_key_len, rec_word, cand.word_len);
        if (!cj_uf_save(&g_userfreq, CJ_USERFREQ_PATH)) {
            CJ_LOG("[cangjie] Step FF：userfreq 保存失败（dirty 保留，下次提交重试）\n");
        }
    }

    /* 2) 裁缓冲区 */
    if (consumed >= g_pinyin_buffer_len) {
        /* 整串消耗完 → 清空、隐藏候选栏。撤销栈保留（composition 还没被
         * 空格/回车/焦点切换 finalize，用户仍可退格逐个撤回已选的字）。 */
        g_pinyin_buffer_len = 0;
        g_step_v_top_candidate_len = 0;
        g_step_v_cached_count = 0;
        cj_step_t_update_popup(this_ptr, NULL, NULL, 0, 0, 0);
    } else {
        int remain = g_pinyin_buffer_len - consumed;
        memmove(g_pinyin_buffer, g_pinyin_buffer + consumed, (size_t)remain);
        g_pinyin_buffer_len = remain;
        g_pinyin_buffer_last_activity = time(NULL);
        /* 对剩余缓冲区重新切分+刷新候选，继续逐字选 */
        cj_step_t_refresh_candidates(this_ptr);
    }
}

static long cj_vk_keyhandler_handler(void *this_ptr, void *key_ptr, int is_press) {
    int key_type = -1;
    if (key_ptr) {
        memcpy(&key_type, (const uint8_t *)key_ptr + 0x70, sizeof(int));
    }
    CJ_LOG("[cangjie] Step M：按键分发函数被调用，this=%p key_ptr=%p is_press=%d type=%d\n",
           this_ptr, key_ptr, is_press, key_type);

    /* A-iv（焦点守卫）：任何按键落地前先清掉"属于别的输入框"的陈旧
     * composition——覆盖"打到一半切输入框再打字/按空格"的泄漏路径
     * （退格路径在 cj_vk_trigger_backspace_handler 里单独守）。无陈旧
     * 状态时一次 focusObject() 即返回，不影响正常打字。 */
    if (is_press) {
        cj_ime_focus_guard(this_ptr);
    }

    /* Step N：只在字符键（type==0）时读——三个候选文本槽位都只读打印，不影响
     * 后面的拦截判断/原样转调。 */
    if (key_ptr && key_type == 0) {
        cj_log_vk_key_text_candidates(key_ptr);
    }

    /* Step O：只在字符键按下时顺带遍历一次 this_ptr（VirtualKeyboard 自己）
     * 的子对象，找 KeyPopup——这条搜索本身留着（诊断价值还在、g_keypopup_ptr
     * 不再驱动任何功能逻辑，纯粹是历史记录+以防万一还有别处引用）。限流到
     * 最多 3 次，够确认结论。 */
    if (key_ptr && key_type == 0 && is_press) {
        cj_log_children_classnames(this_ptr);
    }

    /* Step BB：真正驱动候选栏显示的是 CjCandidateBar（QMLDiff 注入的新
     * 组件），不再是 KeyPopup——同样限流搜索，找到之后缓存指针不再重复找。 */
    if (key_ptr && key_type == 0 && is_press) {
        cj_find_candidatebar(this_ptr);
    }

    /* Step T：真正的拦截判断——只在 press 时可能拦截。反编译确认 release
     * （is_press==0）在绘制/发信号之后必定提前 return，不会走到任何文本
     * 提交逻辑（文件头 Step T 段落有完整证据），所以这里不需要对 release
     * 做任何特殊处理，让它一路落到下面的无条件 call-through。
     *
     * 门控条件从 g_keypopup_ptr 换成 g_candidatebar_ptr（Step BB）——
     * "找不到显示目标就不拦截"这个 fail-safe 原则不变，只是显示目标本身
     * 换成了新组件。 */
    int skip_call_through = 0;
#ifdef CJ_TEST_NO_CANDIDATEBAR_GATE
    /* 测试专用（无 qrr 时验 B 提交）：去掉候选栏门，中文模式下即拦截。候选显示仍不工作
     * （update_popup 对 g_candidatebar_ptr==NULL 有 guard 会 no-op），但提交路径照走 → 触发 B。 */
    if (is_press && key_ptr && cj_step_t_is_chinese_mode(this_ptr)) {
#else
    if (is_press && key_ptr && g_candidatebar_ptr && cj_step_t_is_chinese_mode(this_ptr)) {
#endif
        /* Phase C 修复：不再做空闲过期（原先这里 10 秒静默清空缓冲区，跟
         * Step W"候选栏一直显示到显式操作"矛盾，导致停顿后按空格丢字不上屏）。
         * composition 持续到显式提交/回车/退格清空/焦点切换/切语言。 */
        if (key_type == 0) {
            /* 数字/符号覆盖态优先——原始代码自己的判断顺序就是先看这个
             * guard（key_ptr+0x40，Step N 已确认的偏移），命中说明是密码框
             * 场景，不介入，保留原生数字输入行为。 */
            long numeric_guard = 0;
            memcpy(&numeric_guard, (const uint8_t *)key_ptr + 0x40, sizeof(long));
            if (numeric_guard == 0) {
                char letter = 0;
                if (cj_step_t_extract_single_letter(key_ptr, &letter)) {
                    if (g_pinyin_buffer_len < CJ_SEG_MAX_INPUT) {
                        g_pinyin_buffer[g_pinyin_buffer_len++] = letter;
                        g_ime_focus_obj = cj_current_focus_object(); /* A-iv：composition 归属当前焦点 */
                        g_pinyin_buffer_last_activity = time(NULL);
                        cj_step_t_refresh_candidates(this_ptr);
                    } else {
                        CJ_LOG("[cangjie] Step T：拼音缓冲区已满（%d 字符），丢弃这次按键 '%c'\n",
                               CJ_SEG_MAX_INPUT, letter);
                    }
                    skip_call_through = 1; /* 不追加按键高亮，也不走原生 setCommitString 提交 */
                } else if (g_pinyin_buffer_len > 0) {
                    /* 普通态文本不是单个小写字母（理论上不该发生）——留一道
                     * 防线：先把已缓冲的内容提交掉，再放行这次按键走原生
                     * 路径，避免留下悬空的候选状态。不是空格/回车触发的，
                     * 沿用"优先提交候选"的默认行为（force_raw_pinyin=0）。 */
                    cj_step_t_commit_buffer(this_ptr, 0);
                }
            }
        } else if (key_type == 1 || key_type == 5) {
            /* 空格/回车：反编译确认两者都在 FUN_00697610 内联直接处理，没有
             * 可复用的下层 hook 点，必须在这里拦截。
             *   空格（1）= 确认第一名候选——Phase A 起走分段提交（等价于点
             *     第 0 个候选）：第一名是整段词就整串提交清空，是首音节单字
             *     就只提交那个字、剩下的继续选，跟点选行为一致；候选栏没有
             *     真候选（拼音切分兜底）时退回"提交原样拼音"。
             *   回车（5）= 放弃候选、原样提交刚敲的拉丁字母（force_raw_pinyin=1），
             *     语义不变，见 cj_step_t_commit_buffer 顶部说明。 */
            if (g_pinyin_buffer_len > 0) {
                if (key_type == 1 && g_step_v_cached_count > 0) {
                    cj_step_t_commit_candidate_by_index(this_ptr, 0);
                } else {
                    cj_step_t_commit_buffer(this_ptr, key_type == 5);
                }
                skip_call_through = 1; /* 空格不再插入真空格，回车不再发送 Key_Enter */
            }
            /* 缓冲区为空：不拦截，落到下面的 call-through，行为完全原生
             * （空格正常打空格，回车正常提交/换行）。 */
        } else if (key_type != 2) {
            /* CapsLock(3)/符号切换(4)/Shift(6)/7/8：逻辑本身不改，只加一道
             * 安全网——先提交掉悬空的缓冲区，避免切走键盘布局/大小写状态后
             * 留下一个不会再更新的候选框。type==2（退格）不在这里处理，
             * 交给 cj_vk_trigger_backspace_handler（call-through 马上会经过
             * FUN_00695820，也就是那个 hook 点）。不是空格/回车触发的，
             * 沿用默认行为（force_raw_pinyin=0）。 */
            if (g_pinyin_buffer_len > 0) {
                cj_step_t_commit_buffer(this_ptr, 0);
            }
        }

        /* A-iii：这次按键没被拦截、要走原生 call-through，且是产字/换行键
         * （字符键走原生=大写英文直通/数字密码框；空格/回车缓冲区为空=原生
         * 空格/换行）——原生会往文本框写新内容，接在已选的字后面，前面的
         * composition 就定型了，finalize 撤销栈（之后退格转原生，堵住"空格
         * 之后再退格却去撤字"的错位）。已被拦截的（skip=1）不动，
         * commit_buffer 那几条路径自己已经 reset 过。 */
        if (!skip_call_through && (key_type == 0 || key_type == 1 || key_type == 5)) {
            cj_compose_stack_reset();
        }
    }

    long ret;
    if (skip_call_through) {
        ret = 0;
    } else if (g_orig_vk_keyhandler_call_through) {
        ret = g_orig_vk_keyhandler_call_through(this_ptr, key_ptr, is_press);
    } else {
        CJ_LOG("[cangjie] 严重：按键分发函数调用桩未初始化，跳过调用（不应该发生）\n");
        ret = 0;
    }

    return ret;
}

/* =========== Step EF（Epub Font）：让阅读字体下拉框认识自定义字体，第一阶段
 * ===========
 *
 * 背景：用户想把 rmfw/fonts/ 里的字体加进 FormatFont.qml 阅读字体下拉框，
 * 而不只是当系统 CJK fallback（那是另一件已经做完的事，见 UI 白皮书字体
 * 小节）。QML 侧结构很简单——一个写死 4 项的 ListModel，选中后调用
 * root.epub.setFontName(key)，key 就是下拉项 family 字符串原文，直接传给
 * Qt.font({family:key}) 预览，理论上只要 setFontName 不做白名单校验，任何
 * fontconfig 认识的字体名都能用。
 *
 * 但反编译 EpubProperties::qt_static_metacall（VADDR 0x606f40，跟 Step F
 * 反编译 LanguageSettings 同样的方法：先反编译 qt_metacall 分派壳
 * FUN_006083d0，确认它转调 FUN_00606f40，再逐分支核对 13 个方法 + 3 个
 * 可写属性）没有找到任何一处匹配"存一个 QString、触发重排版"的 setter
 * 特征——这是 Step L→M 已经踩过的同一类坑：QML 编译器在类型已知的调用点
 * （`required property EpubProperties epub`）上会直接生成对 C++ 实现函数
 * 的调用，绕开 qt_metacall/static_metacall，insertText 当初就是这样绕过去
 * 的。setFontName 大概率是同样的情况，静态反编译找不到，要靠真机运行时
 * 枚举方法表（跟 Step V-2 找 insertText 全局 index 同一手法）才能钉死。
 *
 * 这一阶段只做两件事，都是纯只读、不改变任何行为：
 *  1. hook qt_static_metacall 本身当"锚点"——这个函数在 ReadProperty/
 *     WriteProperty 路径上反编译确认过是真实会被调用的（Step F 时代已经
 *     验证过 LanguageSettings 用同一手法能拿到 this 指针），每次调用的
 *     this_ptr（param_1）就是活的 EpubProperties* 实例。FormatFont.qml
 *     里 `currentIndex: getFontIndex(root.epub.fontName)` 这类属性读取，
 *     面板一显示就会触发，不需要用户做任何特殊操作，打开一次"文字与
 *     排版"面板就够。
 *  2. 拿到锚点后，跟 Step V-2 一样枚举 EpubProperties 的全部方法，找名字
 *     是 "setFontName" 的那个，只记录找没找到、参数个数是多少、全局 index
 *     是多少——不调用它，不碰任何实际字体设置。
 *
 * 真正调用 setFontName 做测试（会让当前文档的字体真的变掉，第一次产生
 * 真实可见的行为变化）留到确认这一阶段真机日志正常之后再做，不在这版
 * 一起上——跟 Step P 最初"先只打日志确认命中"的节奏一致。 */

#define EF_STATICMETACALL_VADDR 0x606f40UL   /* EpubProperties::qt_static_metacall */
#define EF_STATICMETAOBJECT_VADDR 0x10c9600UL /* EpubProperties::staticMetaObject，本次会话之前已反编译确认（vtable slot[0] metaObject() 直接返回这个地址） */
#define EF_SETTEXTFORMAT_VADDR 0xc36530UL    /* FUN_00c36530，setFontName/字号/对齐/边距/行距等共用的"排队提交格式变更"函数，内部字符串显示叫 setTextFormat */

static orig_dispatch_fn_t g_orig_ef_staticmetacall_call_through = NULL;
static void *g_epubproperties_ptr = NULL; /* 只存第一次拿到的实例，不覆盖——
                                            * 跟 Step O 存 keyPopup 指针同一个
                                            * 取舍：够用来做后续只读枚举/测试
                                            * 调用，不需要追踪"当前正在显示的
                                            * 是哪个文档"这种更复杂的状态。 */

static int g_step_ef_lookup_done = 0;
static CjMetaMethod g_step_ef_setfontname_method;
static int g_step_ef_setfontname_method_ok = 0;

/* 跟 cj_step_v2_ensure_inserttext_method 同一个模式：只按名字+参数个数匹配，
 * 不硬编码全局 index，避免固件小版本更新后编号漂移导致用错方法。
 * setFontName(QString) 预期参数个数是 1。 */
static int cj_step_ef_find_setfontname_method(void) {
    if (g_step_ef_lookup_done) {
        return g_step_ef_setfontname_method_ok;
    }
    g_step_ef_lookup_done = 1;

    if (!cj_resolve_metamethod_symbols() || !g_ef_metaobject) {
        CJ_LOG("[cangjie] Step EF：符号或 EF metaobject 缺失，放弃查找 setFontName 方法\n");
        return 0;
    }

    const void *ef_metaobject = (const void *)g_ef_metaobject;
    int count = g_qmetaobject_methodcount(ef_metaobject);
    if (count <= 0 || count > 200) {
        CJ_LOG("[cangjie] Step EF：methodCount=%d 数值不合理，怀疑 staticMetaObject 地址不对，放弃\n", count);
        return 0;
    }

    CJ_LOG("[cangjie] Step EF：EpubProperties methodCount=%d，开始按名字枚举\n", count);
    for (int i = 0; i < count; i++) {
        CjMetaMethod m = g_qmetaobject_method(ef_metaobject, i);
        CjPtrList24 name = g_qmetamethod_name(&m);
        if (name.size == 11 && name.ptr && memcmp(name.ptr, "setFontName", 11) == 0) {
            int pc = g_qmetamethod_paramcount(&m);
            CJ_LOG("[cangjie] Step EF：找到 setFontName，全局 index=%d，参数个数=%d\n", i, pc);
            if (pc == 1) {
                g_step_ef_setfontname_method = m;
                g_step_ef_setfontname_method_ok = 1;
                return 1;
            }
        }
    }
    CJ_LOG("[cangjie] Step EF：没找到参数个数==1 的 setFontName（可能压根不在方法表里，"
           "或者名字/签名跟预期不一样），放弃\n");
    return 0;
}

/* 第二阶段（用户已明确确认要现在测试）：真正调用一次 setFontName，验证它
 * 认不认我们部署的自定义字体、还是内部做了白名单校验。用 DirectConnection
 * （值 1，本机 qnamespace.h 核实）同步调用——这里是我们主动发起的调用，
 * 不是要覆盖谁、也不用担心被谁覆盖，同步执行能立刻在日志里看到结果。
 * 测试字体用 "HYTieXianHei"——已经部署在 /home/root/.local/share/fonts/
 * 且 fc-cache 确认注册成功，跟四个原生选项（reMarkable Sans 等）明显不同。
 *
 * 第一次真机部署发现：进程刚启动几秒（网络认证阶段，用户还没打开任何书）
 * 就自动命中了一次 EpubProperties 实例——大概率是后台预热/占位用的实例，
 * 不对应任何正在显示的文档。原设计"全局只测一次"会把这个测试机会浪费在
 * 一个用户根本看不见的实例上。改成"每个不同实例指针独立跟踪"（不是全局
 * 只测一次），加一个小上限避免极端情况下无限增长。
 *
 * 第二次真机部署发现（本节重点）：第一次测试虽然 invoke() 返回"成功"，
 * 但读回 fontName 是空字符串——反编译下游 FUN_00c36530（真正做事的函数，
 * 内部字符串显示叫 setTextFormat，字号/对齐/页边距/行距/字体名等好几个
 * 属性setter 应该共用这同一个"排队提交格式变更"函数）发现一个明确的错误
 * 分支叫 "document not locked or outdated"——我们的测试调用触发时机是
 * WriteProperty local_id=2（document/documentController/settings 三个
 * 属性刚被 QML 赋值那一刻，见 cj_ef_staticmetacall_handler 里的锚点捕获
 * 时机），此时文档很可能还没真正"锁定"/加载完，命令被这条错误分支拦截。
 *
 * 与其继续往下追这个错误分支门后面的异步虚函数调用链（工作量和不确定性
 * 都明显更高），先验证更便宜的假设：只是时机不对。改成对同一个实例在后续
 * 每次 static_metacall 调用时都重试（不是只测一次），每次重试后立刻读回
 * fontName，一旦读到非空就停止（说明生效了，不需要再测）；设一个较小的
 * 重试上限（CJ_STEP_EF_MAX_RETRIES），避免万一真的是白名单拒绝时无限重试
 * 刷日志。 */
#define CJ_STEP_EF_MAX_TESTED_INSTANCES 8
#define CJ_STEP_EF_MAX_RETRIES 2 /* 已经用 12 次验证过重试不解决问题，这次降下来只是留个
                                  * 小余量，不是继续指望"再试试就好了" */

typedef struct {
    void *ptr;
    int attempts;
    int succeeded; /* 读回非空即视为成功，停止重试 */
} CjStepEfInstanceState;

static CjStepEfInstanceState g_step_ef_instances[CJ_STEP_EF_MAX_TESTED_INSTANCES];
static int g_step_ef_instance_count = 0;

/* 第四阶段发现：setTextFormat hook 完全没被触发过——回头核对反编译的
 * case 8，真正调用 FUN_00c36530 之前有两道守卫，其中一道是
 * `*(long*)(param_1+0x40) != 0`（这个偏移在 WriteProperty case 0 里就是
 * "文档"指针，document/documentController/settings 三个属性刚被赋值时
 * 很可能还是空的）——我们在 WriteProperty local_id=2 那一刻发起测试调用，
 * 文档指针大概率还没就位，setFontName 整个函数体直接短路跳过，之前"重试
 * 12 次"是在同一个错误时机反复重试，当然没用。这里只做一个纯只读的内存
 * 检查（不调用任何 Qt 符号，没有重入风险）：文档指针非空才真正发起测试
 * 调用，为空就先不试，等下一次 static_metacall 被触发（文档指针大概率
 * 已经就位）再检查一次。 */
static int cj_step_ef_document_attached(void *this_ptr) {
    long doc_ptr = *(long *)((uint8_t *)this_ptr + 0x40);
    return doc_ptr != 0;
}

/* 第五阶段（用户要求）：确认 setFontName 没有白名单校验之后，用户想直接
 * 在真机上挨个切换 rmfw/fonts 里的字体肉眼比较，而不是只测一个
 * "HYTieXianHei"。下拉框本身还没接（那是更大的一个工程，要找到
 * fontModel 这个 QQmlListModel 实例、调用它的 append()，属于独立的下一
 * 阶段），这里先用最省事的方式满足"切换着看"的需求——每次遇到一个新的、
 * 真挂了文档的 EpubProperties 实例，就依次应用轮转列表里的下一个字体。
 * 触发方式：关闭书再重新打开（或者切换到另一本书），会创建一个新的
 * EpubProperties 实例，自动换到下一个字体；不需要用户做任何额外操作。
 * 全部用 fc-list 核实过真实存在于设备上的字族名，不是猜的：
 *   HYTieXianHei（汉仪铁线黑）、HYYingSong 35W（汉仪盈宋）、
 *   KingHwaOldSong（京华老宋体）、KF Readerly、
 *   最后一项 "reMarkable Sans" 是原生默认字体，方便切回去对比。 */
static const char *const CJ_STEP_EF_FONT_ROTATION[] = {
    "HYTieXianHei",
    "HYYingSong 35W",
    "KingHwaOldSong",
    "KF Readerly",
    "reMarkable Sans",
};
#define CJ_STEP_EF_FONT_ROTATION_COUNT \
    (sizeof(CJ_STEP_EF_FONT_ROTATION) / sizeof(CJ_STEP_EF_FONT_ROTATION[0]))
static int g_step_ef_rotation_index = 0;

__attribute__((unused))
static void cj_step_ef_test_invoke_setfontname(void *this_ptr) {
    if (!cj_step_ef_document_attached(this_ptr)) {
        return; /* 文档指针还是空的，这次不试，等下一次真正挂了文档再试 */
    }
    CjStepEfInstanceState *state = NULL;
    for (int i = 0; i < g_step_ef_instance_count; i++) {
        if (g_step_ef_instances[i].ptr == this_ptr) {
            state = &g_step_ef_instances[i];
            break;
        }
    }
    if (!state) {
        if (g_step_ef_instance_count >= CJ_STEP_EF_MAX_TESTED_INSTANCES) {
            return; /* 达到实例上限，安静放弃，不是错误 */
        }
        state = &g_step_ef_instances[g_step_ef_instance_count++];
        state->ptr = this_ptr;
        state->attempts = 0;
        state->succeeded = 0;
    }
    if (state->succeeded || state->attempts >= CJ_STEP_EF_MAX_RETRIES) {
        return; /* 这个实例已经换过字体了，或者试够次数了，不再重复应用 */
    }
    state->attempts++;

    if (!cj_resolve_qt_symbols()) {
        CJ_LOG("[cangjie] Step EF：QArrayData::allocate 未解析成功，放弃测试调用\n");
        return;
    }

    const char *test_font = CJ_STEP_EF_FONT_ROTATION[g_step_ef_rotation_index % CJ_STEP_EF_FONT_ROTATION_COUNT];
    g_step_ef_rotation_index++; /* 下一个新实例换下一个字体 */

    uint8_t qstr[24];
    if (!cj_build_ascii_qstring(qstr, test_font, (long long)strlen(test_font), g_qarraydata_allocate)) {
        CJ_LOG("[cangjie] Step EF：构造字体名 QString 失败，放弃这次切换\n");
        return;
    }

    CjGenericArgument ret = {0};
    CjGenericArgument a_font = { .data = qstr, .name = "QString" };
    CjGenericArgument a_empty = {0};

    CJ_LOG("[cangjie] Step EF：新文档实例，切换字体为 \"%s\"，this=%p\n", test_font, this_ptr);
    unsigned char ok = g_qmetamethod_invoke(&g_step_ef_setfontname_method, this_ptr,
                                             1 /* Qt::DirectConnection */,
                                             ret, a_font, a_empty, a_empty,
                                             a_empty, a_empty, a_empty, a_empty, a_empty, a_empty, a_empty);
    CJ_LOG("[cangjie] Step EF：调用 setFontName(\"%s\") %s\n", test_font, ok ? "成功" : "失败");

    /* 用户手动改对齐方式之后发现字体突然生效了——两者共用同一个排队提交
     * 机制（FUN_00c36530），推测是"只排一次队不够，需要再来一条命令才会
     * 冲刷渲染"。这里试验性地紧跟着再调一次同样的 setFontName（不是猜第二
     * 个不同的属性，先用最简单的"再触发一次同一个命令"验证这个猜测是否
     * 成立），qstr 指向的 QString 数据没有被上一次 invoke() 消费掉（Qt
     * 的参数编组是按引用传递，不转移所有权），可以安全复用。 */
    CJ_LOG("[cangjie] Step EF：紧跟着再调一次 setFontName(\"%s\")，验证连续两次命令能否冲刷渲染\n", test_font);
    unsigned char ok2 = g_qmetamethod_invoke(&g_step_ef_setfontname_method, this_ptr,
                                              1 /* Qt::DirectConnection */,
                                              ret, a_font, a_empty, a_empty,
                                              a_empty, a_empty, a_empty, a_empty, a_empty, a_empty, a_empty);
    CJ_LOG("[cangjie] Step EF：第二次调用 setFontName(\"%s\") %s\n", test_font, ok2 ? "成功" : "失败");

    /* invoke() 返回 true 只说明元对象机制本身把调用送到了，不代表内部真的
     * 生效——读回 fontName 属性本身的值才是确定性证据。复用 Step P/Q/T
     * 已经验证过的 QObject::property()+QVariant 读取手法。 */
    if (cj_resolve_keypopup_write_symbols() && g_qobject_property && g_qvariant_tostring && g_qvariant_dtor) {
        CjVariant32 v = g_qobject_property(this_ptr, "fontName");
        CjPtrList24 name = g_qvariant_tostring(&v);
        cj_log_qstring_preview("Step EF：切换之后读回 fontName", &name);
        if (name.size > 0) {
            state->succeeded = 1;
        } else if (state->attempts >= CJ_STEP_EF_MAX_RETRIES) {
            CJ_LOG("[cangjie] Step EF：已重试 %d 次仍读回空，放弃这个实例\n", state->attempts);
        }
        g_qvariant_dtor(&v);
    } else {
        CJ_LOG("[cangjie] Step EF：读回符号缺失，跳过 fontName 读回验证\n");
    }
}

/* 诊断：setFontName 调用之后 fontName 读回是空字符串，说明真的碰到校验
 * 逻辑，但不确定这个校验是不是真在 static_metacall 本地这 13 个 case
 * 里面——EpubProperties methodCount()=17（全局，含继承链），但静态反编译
 * 只看到 13 个 case（局部，只算这个类自己新增的方法），跟 Step K/V-2 已经
 * 踩过的"全局 index 跟分派函数内部本地 id 是两套编号"是同一类问题。与其
 * 继续猜局部/全局怎么对应，直接打印每次真正走到这个分派函数时的
 * call_type/id，我们自己发起的 setFontName 测试调用如果真的落到这个
 * static_metacall 上，日志里会看到一条 call_type==0 的新记录，local id
 * 是多少一目了然；如果调用之后完全没出现新的 call_type==0 记录，说明
 * setFontName 根本不经过这个函数，得去找别的分派点（比如 QML AOT 直调，
 * 或者属于某个基类自己的 static_metacall）。 */
static int g_step_ef_invoke_log_count = 0;
#define CJ_STEP_EF_INVOKE_LOG_CAP 20

/* Step EF 第三阶段：重入保护——上一版发现 QObject::property() 读回会重新
 * 触发一次 qt_metacall，间接重入这个 handler 自己（日志里"第 12 次"重复
 * 打印好几遍的根因）。这次加一个最简单的可重入计数器：只在最外层（第一次
 * 进入、深度为 0）才做锚点捕获/方法枚举/测试调用这些"额外工作"，任何嵌套
 * 进来的调用（深度 > 0，不管是我们自己读属性触发的、还是 setFontName 内部
 * 逻辑自己产生的）只负责老老实实转调原函数，不做任何额外操作——这是这类
 * hook 点通用的重入防护写法，不是本项目独创，但这是第一次真正需要用到。 */
static int g_ef_handler_depth = 0;

static void cj_ef_staticmetacall_handler(void *this_ptr, int call_type, int id, void **args) {
    int is_outermost = (g_ef_handler_depth == 0);
    g_ef_handler_depth++;

    if (is_outermost && call_type == 0 && g_step_ef_invoke_log_count < CJ_STEP_EF_INVOKE_LOG_CAP) {
        g_step_ef_invoke_log_count++;
        CJ_LOG("[cangjie] Step EF：static_metacall 收到 InvokeMetaMethod，this=%p local_id=%d\n",
               this_ptr, id);
    }
    if (g_orig_ef_staticmetacall_call_through) {
        g_orig_ef_staticmetacall_call_through(this_ptr, call_type, id, args);
    } else {
        CJ_LOG("[cangjie] 严重：EpubProperties::qt_static_metacall 调用桩未初始化，跳过调用（不应该发生）\n");
        g_ef_handler_depth--;
        return;
    }

    if (is_outermost) {
        if (!g_epubproperties_ptr && this_ptr) {
            g_epubproperties_ptr = this_ptr; /* 只留个参考指针 */
            CJ_LOG("[cangjie] Step EF：拿到 EpubProperties* 锚点 = %p（call_type=%d id=%d）\n",
                   this_ptr, call_type, id);
        }
        if (this_ptr) {
            cj_step_ef_find_setfontname_method(); /* 只枚举缓存方法，纯只读，安全 */
        }
        /* cj_step_ef_test_invoke_setfontname(this_ptr) 用户已确认到此为止——
         * 结论已经拿到：setFontName 本身没有白名单校验、属性值确实会写入
         * （读回验证过"HYTieXianHei"），但真正画字的是独立进程
         * xochitl_pdf_renderer，不在这个 hook 能触达的范围内，继续调用
         * setFontName 除了让某本书的持久化 fontName 设置被悄悄改掉之外
         * 没有任何可见效果，不该继续留着自动触发。函数定义保留，下次要
         * 重启这个方向（比如决定去反编译 xochitl_pdf_renderer）时可以
         * 直接复用，不需要重写。 */
    }
    g_ef_handler_depth--;
}

static void cj_install_epubproperties_staticmetacall_hook(void) {
    uintptr_t target = g_addr_ef_staticmetacall;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_ef_staticmetacall_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step EF：EpubProperties::qt_static_metacall hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_ef_staticmetacall_call_through = (orig_dispatch_fn_t)stub;
    fprintf(stderr,
            "[cangjie] Step EF：EpubProperties::qt_static_metacall hook 安装完成 @ %p"
            "（只读锚点，原样转调，不改变任何行为——打开一次文档的\"文字与排版\"面板"
            "触发属性读取即可在日志里看到锚点+setFontName 枚举结果）\n",
            (void *)target);
}

/* =====================================================================
 * Step KBS：设置 App「语言和键盘 → 屏幕键盘」中文入口
 *   （KeyboardSettingsAttached::qt_static_metacall）
 *
 * QML（device/view/settings/LanguageAndKeyboard.qml）keyboardDialog 的
 * SelectionComponent：model = KeyboardSettings.availableLayouts、
 * display = qsTr(languageSettings.languageName(modelData))、
 * onSelected: Settings.keyboardLanguage = selection。原生 availableLayouts 不含
 * 中文 → 缺中文选项。availableLayouts 是 KeyboardSettingsAttached 的 Q_PROPERTY
 * （本地 property id==1、type QStringList，离线解析 metaobject 逐字节核实），
 * 读取经 qt_static_metacall(ReadProperty=1, id==1, args)；getter 内联在
 * static_metacall 里（无独立 getter 函数可 hook，反汇编确认），所以 QML 读它
 * 必走 static_metacall。原样转调后对 args[0] 那份返回副本追加 "zh_CN"——原始
 * 成员数据永不触碰。选中"zh_CN"→ keyboardLanguage=zh_CN（简体全拼），简繁全
 * 双拼 4 模式再靠键盘地球键切；显示名由 LanguageSettings::languageName（Step
 * KBS 的 dispatch id==2 分支）覆盖成"中文"。
 *
 * 【为何改 metaobject 指针，不用 patch_target——本轮真机 SEGV 教训】
 * 第一版照 Step EF static_metacall 模板用 patch_target 打函数入口，真机
 * status=11/SEGV 崩溃循环、连带设备重启。根因：我们的 trampoline
 * （make_call_through_stub）直接 memcpy 目标函数前 20 字节且**不重定位 PC
 * 相对指令**，依赖"前 20 字节纯寄存器/栈操作"。EF static_metacall 的 cbnz w1
 * 在 offset 0x14（20 字节之外，安全）；但 KBS static_metacall 的 cbnz w1 在
 * **offset 0x0C（12 字节，落在覆盖区内）**，被复制进 trampoline 后相对目标
 * 算错 → 启动早期键盘初始化高频调用即跳飞崩溃。工程纪律「形状看着像不等于
 * 对」的活教材。
 * 改法：不碰任何指令字节，只把 metaobject 结构体 +0x18 的 static_metacall
 * 函数指针改指向我们的 wrapper（mo 靠已定位的 static_metacall 地址反查唯一
 * qword−0x18 得到，复用 cj_find_metaobject）。wrapper 用绝对地址直调原函数
 * ——原函数在原地完好，其 PC 相对指令正常执行，彻底避开 trampoline 限制。
 *
 * QMLDiff 走不通才落到 C：qmldiff 不穿透 Component{}，keyboardDialog 包在
 * Component 里，REPLACE model/display 全部 "Cannot locate element"。
 * ===================================================================== */
static orig_dispatch_fn_t g_orig_kbs_staticmetacall = NULL; /* metaobject 里替换下来的原指针 */

static void cj_kbs_staticmetacall_handler(void *this_ptr, int call_type, int id, void **args) {
    if (g_orig_kbs_staticmetacall) {
        g_orig_kbs_staticmetacall(this_ptr, call_type, id, args);
    } else {
        CJ_LOG("[cangjie] 严重：KeyboardSettingsAttached::qt_static_metacall 原指针未保存，跳过（不应该发生）\n");
        return;
    }

    if (args == NULL) {
        return;
    }

#ifdef CJ_KBS_DIAG
    /* 诊断:限量打印经过 KBS static_metacall 的 call_type+id,定位 availableLayouts 真实 id。 */
    static int cj_kbs_diag_n = 0;
    if (cj_kbs_diag_n < 80) {
        cj_kbs_diag_n++;
        CJ_LOG("[cangjie] KBS-DIAG: call_type=%d id=%d args=%p\n", call_type, id, (void *)args);
    }
#endif

    /* ReadProperty==1，availableLayouts 本地 property id==1。canUndo/canRedo 等
     * 其它属性读取会高频经过这里，但只多两个 if 判断、命中才动作，开销可忽略。 */
    if (call_type == 1 && id == 1) {
        uint8_t *dest = (uint8_t *)args[0];
        if (dest == NULL) {
            return;
        }
        if (!cj_resolve_qt_symbols()) {
            return;
        }
        int ok = cj_stringlist_append_utf8(dest, "zh_CN", 5, g_qarraydata_allocate);
        CJ_LOG("[cangjie] Step KBS：availableLayouts 追加 \"zh_CN\"：%s\n",
               ok ? "成功" : "失败（保留原列表状态，不影响其它功能）");
    }
}

static void cj_install_keyboardsettings_staticmetacall_hook(void) {
    if (!g_kbs_metaobject) {
        return; /* mo 没反查到（特征码/反查任一失败），跳过 safe mode */
    }

    /* mo 结构体：superdata, stringdata, data, static_metacall, ...（各 8 字节），
     * static_metacall 指针在 +0x18。改这个指针即改变该类所有 metacall 的分派。 */
    void **smc_slot = (void **)(g_kbs_metaobject + 0x18);
    void *orig = *smc_slot;

    /* 交叉核验：mo+0x18 读到的指针必须等于特征码独立定位的 static_metacall 地址，
     * 两条独立线索对上才动手（任一不符说明反查/特征码有一处不对，safe mode 退出）。 */
    if ((uintptr_t)orig != g_addr_kbs_staticmetacall) {
        fprintf(stderr, "[cangjie] Step KBS：mo+0x18 指针(%p) 与特征码定位(%p) 不符，跳过（safe mode）\n",
                orig, (void *)g_addr_kbs_staticmetacall);
        return;
    }

    /* metaobject 在 .rodata，改指针前把所在页临时改可写。 */
    long pagesize = sysconf(_SC_PAGESIZE);
    if (pagesize <= 0) pagesize = 4096;
    uintptr_t page_base = (uintptr_t)smc_slot & ~((uintptr_t)pagesize - 1);
    size_t region_len = (size_t)pagesize;
    if ((((uintptr_t)smc_slot - page_base) + sizeof(void *)) > region_len) {
        region_len += (size_t)pagesize;
    }
    if (mprotect((void *)page_base, region_len, PROT_READ | PROT_WRITE) != 0) {
        fprintf(stderr, "[cangjie] Step KBS：mo+0x18 所在页改可写失败，跳过（safe mode）：%s\n",
                strerror(errno));
        return;
    }

    g_orig_kbs_staticmetacall = (orig_dispatch_fn_t)orig;
    *smc_slot = (void *)cj_kbs_staticmetacall_handler;

    /* 恢复只读（wrapper 已装好，不再需要可写；恢复失败不致命，仅日志提示）。 */
    if (mprotect((void *)page_base, region_len, PROT_READ) != 0) {
        fprintf(stderr, "[cangjie] Step KBS：mo+0x18 页恢复只读失败（不致命）：%s\n", strerror(errno));
    }

    fprintf(stderr,
            "[cangjie] Step KBS：KeyboardSettingsAttached metaobject static_metacall 指针"
            "已改指向 wrapper（原=%p，mo=%p）——设置页屏幕键盘列表追加 zh_CN 中文入口\n",
            orig, (void *)g_kbs_metaobject);
}

/* =====================================================================
 * Step EF 第三阶段：追踪 FUN_00c36530（setTextFormat，字号/对齐/边距/行距/
 * 字体名等共用的"排队提交格式变更"函数）本身——纯只读诊断，不修改任何参数/
 * 返回值。反编译确认签名 (long *param_1, long *param_2, QString *param_3)，
 * 20 字节 prologue（paciasp/stp/mov/str/mov，全是寄存器/栈操作，无 PC 相关
 * 寻址）跟其它已经打过 patch 的目标同一个安全档位，可以直接用同一套
 * trampoline 机制。
 *
 * 目的：确认我们的 setFontName 测试调用真的走到了这里（不是在更早的地方
 * 就被拦下了）、以及 param_2（文档指针容器）解引用后是不是我们怀疑的
 * "文档未锁定"那种空/无效状态——这两点都是只读检查真实参数值，不调用任何
 * Qt 符号，不会重演之前 QObject::property() 引发的重入问题。 */
#define EF_SETTEXTFORMAT_LOG_CAP 10

typedef void (*orig_ef_settextformat_fn_t)(long *param_1, long *param_2, const void *param_3);
static orig_ef_settextformat_fn_t g_orig_ef_settextformat_call_through = NULL;
static int g_ef_settextformat_log_count = 0;

static void cj_ef_settextformat_handler(long *param_1, long *param_2, const void *param_3) {
    if (g_ef_settextformat_log_count < EF_SETTEXTFORMAT_LOG_CAP) {
        g_ef_settextformat_log_count++;
        long doc_deref = (param_2 != NULL) ? *param_2 : -1;
        CJ_LOG("[cangjie] Step EF：FUN_00c36530(setTextFormat) 被调用，param_1=%p param_2=%p "
               "*param_2=0x%lx\n", (void *)param_1, (void *)param_2, (unsigned long)doc_deref);
        cj_log_qstring_preview("Step EF：  setTextFormat 的 param_3（新文字）", param_3);
    }
    if (g_orig_ef_settextformat_call_through) {
        g_orig_ef_settextformat_call_through(param_1, param_2, param_3);
    } else {
        CJ_LOG("[cangjie] 严重：setTextFormat 调用桩未初始化，跳过调用（不应该发生）\n");
    }
}

static void cj_install_epubproperties_settextformat_hook(void) {
    uintptr_t target = g_addr_ef_settextformat;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_ef_settextformat_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step EF：setTextFormat hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_ef_settextformat_call_through = (orig_ef_settextformat_fn_t)stub;
    fprintf(stderr,
            "[cangjie] Step EF：setTextFormat hook 安装完成 @ %p（只读诊断，原样转调）\n",
            (void *)target);
}
/* =========== Step EF 第一/三阶段代码段结束 =========== */

/* =========== Step HL2：荧光笔汉字吸附修复（真机 2026-08-23 通过）===========
 * .169 FUN_00f05ad0 = 把荧光笔命中的精确区间向两边扩张（对每个子区间调 FUN_00f052f0
 * 扩 start/end）→ 整行；对无空格中文就是"划一小段吸整行"的病根。
 * 参数：x0=scene（glyph 数组 *(scene+8)，每项 0x38 字节，+0x30=QChar），
 *       x1=range 向量 v={refcount@0, 子区间ptr@8(int[2]{start,end} 数组), count@16}。
 * 修复：首字为 CJK 时跳过扩张、保命中层的精确边界（划哪吸哪，=镇纸效果）；英文/其它照常扩张。
 * g_hl_expand_neuter 置 0 可临时恢复原生扩张。机理。 */
typedef void (*orig_hl_expand_fn_t)(long, void *);
static orig_hl_expand_fn_t g_orig_hl_expand_call_through = NULL;
static int g_hl_expand_neuter = 1;   /* 1=CJK 跳过扩张（修复）；0=恢复原生扩张 */

/* glyph 数组第 idx 个字的 QChar 是否属 CJK。scene+8 是 glyph 数组基址。 */
static int cj_hl_glyph_is_cjk(long scene, int idx) {
    if (idx < 0) return 0;
    long garr = 0;
    memcpy(&garr, (const void *)(scene + 8), sizeof(garr));
    if (!garr) return 0;
    uint16_t ch = 0;
    memcpy(&ch, (const void *)(garr + (long)idx * 0x38 + 0x30), sizeof(ch));
    return (ch >= 0x4E00 && ch <= 0x9FFF)   /* CJK 统一表意 */
        || (ch >= 0x3400 && ch <= 0x4DBF)   /* 扩展 A */
        || (ch >= 0xF900 && ch <= 0xFAFF)   /* 兼容表意 */
        || (ch >= 0x3000 && ch <= 0x303F);  /* CJK 标点 */
}

/* 运行时开关：从 reading-qol.json 读 hlSnapCjk 到 g_hl_expand_neuter（设置页「笔记增强」控制）。
 * 荧光笔扩张不是热路径（划线才调），每次读一次即可，无需 mtime。极简字段扫描、不引 JSON 库。
 * fail-safe：文件缺失/字段缺失/读失败 → 不改，保持编译期默认（修复开）。 */
#define CJ_READING_QOL_PATH CJ_DATA_DIR "/reading-qol.json"
static void cj_hl_refresh_config(void) {
    FILE *f = fopen(CJ_READING_QOL_PATH, "rb");
    if (!f) return;
    char buf[4096];
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    buf[n] = '\0';
    const char *p = strstr(buf, "\"hlSnapCjk\"");
    if (!p) return;
    p += 11;  /* 跳过 "hlSnapCjk" 本身（含两个引号，共 11 字节） */
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (strncmp(p, "true", 4) == 0) g_hl_expand_neuter = 1;
    else if (strncmp(p, "false", 5) == 0) g_hl_expand_neuter = 0;
}

static void cj_hl_expand_handler(long scene, void *rng_v) {
    cj_hl_refresh_config();   /* 每次划线先同步开关状态：设置页改完、下一笔即生效 */
    /* CJK 首字的命中区间：跳过原扩张函数、保命中层精确边界（划哪吸哪）；
     * 英文/其它：call-through 照常扩张，行为与原生一致。 */
    if (g_hl_expand_neuter) {
        uint64_t *v = (uint64_t *)rng_v;
        if (v) {
            int *subs = (int *)(uintptr_t)v[1];
            long count = (long)v[2];
            if (subs && count > 0 && cj_hl_glyph_is_cjk(scene, subs[0])) {
                return;   /* CJK：不调原扩张函数 */
            }
        }
    }
    if (g_orig_hl_expand_call_through) {
        g_orig_hl_expand_call_through(scene, rng_v);
    }
}

static void cj_install_hl_expand_hook(void) {
    uintptr_t target = g_addr_hl_expand;
    if (!target) return;
    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_hl_expand_handler, &stub)) {
        fprintf(stderr, "[cangjie] 荧光笔EXPAND hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_hl_expand_call_through = (orig_hl_expand_fn_t)stub;
    fprintf(stderr, "[cangjie] 荧光笔EXPAND hook 安装完成 @ %p（neuter=%d）\n",
            (void *)target, g_hl_expand_neuter);
}
/* =========== Step HL2 代码段结束 =========== */

static void cj_install_virtualkeyboard_keyhandler_hook(void) {
    uintptr_t target = g_addr_vk_keyhandler;
    if (!target) return;

    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_vk_keyhandler_handler, &stub)) {
        fprintf(stderr, "[cangjie] Step M：按键分发函数 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_vk_keyhandler_call_through = (orig_vk_keyhandler_fn_t)stub;
    fprintf(stderr,
            "[cangjie] Step M：按键分发函数 hook 安装完成 @ %p（只打日志，不改行为）\n",
            (void *)target);
}

/* 整合进 vellum 的 xovi 体系后,不再靠 __attribute__((constructor)) + 独立 LD_PRELOAD
 * 触发,改由 xovi 加载扩展时经 _xovi_construct 调用(见文件末尾)。 */
static void cj_hook_init(void) {
    uintptr_t base = 0;
    size_t size = 0;

    if (!cj_find_exec_module(TARGET_MODULE_SUFFIX, NULL, &base, &size)) {
        fprintf(stderr, "[cangjie] 找不到 %s 的可执行映射，跳过 hook（safe mode）\n",
                TARGET_MODULE_SUFFIX);
        return;
    }

    /* 韧性重构（.166 适配）：每个 hook 目标各自用 prologue 特征码在运行期
     * 自定位，不再靠“一个锚点 + .164 固定偏移”，也删掉了 offset==0x513c70 这道
     * 版本门（正是它在 OTA 到 .166、各函数位移不一致后整体拒绝、导致中文全丢）。
     * 任一目标没唯一命中就只跳过它自己（各 install 函数内部判 0 即 safe mode），
     * 核心输入不受单个非核心目标缺失影响。全部特征码已在 .164/.166 两版二进制
     * 离线对拍确认唯一命中（scratchpad/verify_patterns.py）。 */

    /* setLanguageCode 主 hook（UI 汉化入口）单独解析——它还带一段已知版本诊断。 */
    uintptr_t found = cj_resolve_target(base, size, SET_LANGUAGE_CODE_PROLOGUE, NULL,
                                        sizeof(SET_LANGUAGE_CODE_PROLOGUE), "setLanguageCode");
    if (!found) {
        fprintf(stderr, "[cangjie] setLanguageCode 没唯一命中，放弃全部 hook（safe mode）\n");
        return;
    }

    g_xochitl_base = base; /* 仍缓存基址，供个别运行期代码/诊断复用 */

    uintptr_t offset = found - base;
    int known = 0;
    for (size_t i = 0; i < sizeof(KNOWN_OFFSETS) / sizeof(KNOWN_OFFSETS[0]); i++) {
        if (KNOWN_OFFSETS[i].offset == offset) {
            fprintf(stderr, "[cangjie] setLanguageCode 相对偏移 0x%lx 匹配已知固件版本：%s\n",
                    (unsigned long)offset, KNOWN_OFFSETS[i].fw_version);
            known = 1;
            break;
        }
    }
    if (!known) {
        fprintf(stderr,
                "[cangjie] setLanguageCode 相对偏移 0x%lx 不在已知版本列表里——"
                "新固件版本，各目标改由自己的特征码定位，不受影响，仅作诊断参考\n",
                (unsigned long)offset);
    }

    void *stub = NULL;
    if (!patch_target((void *)found, (void *)cj_hook_handler, &stub)) {
        return;
    }
    g_orig_call_through = (orig_fn_t)stub;
    fprintf(stderr, "[cangjie] setLanguageCode hook 安装完成\n");

    /* 其余 8 个代码目标各自特征码自定位（7 精确 + 2 掩 1 条 BL）。 */
    g_addr_fun9174d0 = cj_resolve_target(base, size, PROLOGUE_FUN_009174D0, NULL,
            sizeof(PROLOGUE_FUN_009174D0), "FUN_009174d0 (Step G)");
    g_addr_vk_dispatch = cj_resolve_target(base, size, PROLOGUE_VK_DISPATCH, NULL,
            sizeof(PROLOGUE_VK_DISPATCH), "FUN_00695a60 (Step R+S)");
    g_addr_vk_loadlayout = cj_resolve_target(base, size, PROLOGUE_VK_LOADLAYOUT, NULL,
            sizeof(PROLOGUE_VK_LOADLAYOUT), "FUN_00c6f3c0 (Step U)");
    g_addr_vk_inserttext = cj_resolve_target(base, size, PROLOGUE_VK_INSERTTEXT, NULL,
            sizeof(PROLOGUE_VK_INSERTTEXT), "FUN_00695970 (Step L insertText)");
    g_addr_vk_triggerbackspace = cj_resolve_target(base, size, PROLOGUE_VK_TRIGGERBACKSPACE,
            PROLOGUE_VK_TRIGGERBACKSPACE_MASK, sizeof(PROLOGUE_VK_TRIGGERBACKSPACE),
            "FUN_00695820 (Step L 退格, BL掩码)");
    g_addr_vk_keyhandler = cj_resolve_target(base, size, PROLOGUE_VK_KEYHANDLER,
            PROLOGUE_VK_KEYHANDLER_MASK, sizeof(PROLOGUE_VK_KEYHANDLER),
            "FUN_00697610 (Step M 按键, BL掩码)");
    g_addr_ef_staticmetacall = cj_resolve_target(base, size, PROLOGUE_EF_STATICMETACALL, NULL,
            sizeof(PROLOGUE_EF_STATICMETACALL), "FUN_00606f40 (Step EF)");
    g_addr_ef_settextformat = cj_resolve_target(base, size, PROLOGUE_EF_SETTEXTFORMAT, NULL,
            sizeof(PROLOGUE_EF_SETTEXTFORMAT), "FUN_00c36530 (Step EF)");
    g_addr_kbs_staticmetacall = cj_resolve_target(base, size, PROLOGUE_KBS_STATICMETACALL, NULL,
            sizeof(PROLOGUE_KBS_STATICMETACALL), "KeyboardSettingsAttached::qt_static_metacall (Step KBS)");
    g_addr_hl_expand = cj_resolve_target(base, size, PROLOGUE_HL_EXPAND, NULL,
            sizeof(PROLOGUE_HL_EXPAND), "FUN_00f05ad0 (Step HL2 荧光笔扩张)");

    /* 两个 metaobject：靠已定位的 static_metacall 函数地址反查唯一 qword−0x18。 */
    g_vk_metaobject = cj_find_metaobject(base, size, g_addr_vk_dispatch, "VK_STATICMETAOBJECT");
    g_ef_metaobject = cj_find_metaobject(base, size, g_addr_ef_staticmetacall, "EF_STATICMETAOBJECT");
    /* Step KBS：靠特征码定位到的 static_metacall 地址反查 KeyboardSettingsAttached
     * 的 metaobject（唯一 qword−0x18）。改它的 static_metacall 指针即安装 hook。 */
    g_kbs_metaobject = cj_find_metaobject(base, size, g_addr_kbs_staticmetacall, "KBS_STATICMETAOBJECT");

    /* 安装各 hook（每个内部判自己的解析结果是否为 0，为 0 即跳过 safe mode）。 */
    cj_install_availablelanguagecodes_hook();
    cj_install_virtualkeyboard_insert_hook();
    cj_install_virtualkeyboard_triggerbackspace_hook();
    cj_install_virtualkeyboard_keyhandler_hook();
    cj_install_virtualkeyboard_dispatch_hook();
    cj_install_virtualkeyboard_loadlayout_hook();
    cj_install_epubproperties_staticmetacall_hook();
    cj_install_epubproperties_settextformat_hook();
    cj_install_keyboardsettings_staticmetacall_hook();
    cj_install_hl_expand_hook();
}

/* ============================================================================
 * xovi 扩展入口（整合进 vellum 的 xovi 体系,取代旧的 LD_PRELOAD constructor）
 *
 * vellum 用官方 xovi 机制:xovi.so 被 preload → 扫 extensions.d/ → dlopen 每个扩展 →
 * dlsym _xovi_shouldLoad(非0才加载) → dlsym _xovi_construct(执行初始化)。
 * 见 asivery/xovi src/dynamiclinker.c。本扩展放 /home/root/xovi/extensions.d/,
 * 跟 qt-resource-rebuilder 并列,被自动加载——/home 分区固件回滚不丢,治本。
 * ========================================================================== */

/* fail-safe(替代旧的 precheck.sh + sha256 基线):当前 xochitl 上 setLanguageCode
 * 特征码能唯一命中才加载——兼容固件即可,不锁死具体 sha256,符合韧性重构“特征码
 * 自定位”精神。不命中(未知固件/架构不符)返回 0 → xovi 跳过 → 裸启原生 xochitl。 */
char _xovi_shouldLoad(void) {
    uintptr_t base = 0, addr = 0;
    size_t size = 0;
    if (!cj_find_exec_module(TARGET_MODULE_SUFFIX, NULL, &base, &size)) {
        fprintf(stderr, "[cangjie] _xovi_shouldLoad: 找不到 xochitl 映射 → 拒绝加载(裸启原生)\n");
        return 0;
    }
    if (!cj_find_unique_pattern((const uint8_t *)base, size,
            SET_LANGUAGE_CODE_PROLOGUE, sizeof(SET_LANGUAGE_CODE_PROLOGUE), &addr)) {
        fprintf(stderr, "[cangjie] _xovi_shouldLoad: setLanguageCode 特征码未唯一命中"
                "(未知固件) → 拒绝加载(裸启原生)\n");
        return 0;
    }
    fprintf(stderr, "[cangjie] _xovi_shouldLoad: 固件兼容(setLanguageCode@0x%lx) → 加载\n",
            (unsigned long)addr);
    return 1;
}

/* xovi 加载扩展时调用:执行 hook 安装(原 constructor 逻辑)。此刻 xochitl 主镜像
 * 已映射,特征码可扫。 */
void _xovi_construct(void) {
    cj_hook_init();
}
