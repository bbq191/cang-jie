#pragma once
#include <stdint.h>

/* AArch64 "远跳转" trampoline：把一个 64 位地址拆成 4 个 16 位分段，用
 * MOVZ + 3 条 MOVK 把它凑进 x16 寄存器，再用 BR x16 跳过去——这是标准的
 * AArch64 长跳转写法（PC 相对分支/ADRP 的立即数范围都覆盖不了任意 64 位地址，
 * 必须整个装进寄存器再跳）。
 *
 * 指令编码和 xovi（github.com/asivery/xovi，src/trampolines/aarch64/aarch64.c
 * 里的 pivotSymbol()）用的完全一样，是照抄过来的（保留出处，xovi 是 LGPL v3）；
 * 差别只在于 xovi 那边先用 dlsym() 按符号名解析出地址，这里直接接受调用方已经
 * 算好的地址（比如白皮书 10.1 节里字节特征码扫描出来的地址）——这段“把地址
 * 装进跳转指令”的逻辑本身跟地址是怎么来的无关，可以照搬。
 *
 * x16 是 AArch64 过程调用标准里的 IP0（临时寄存器/调用者不需要保留），用作
 * 跳转暂存寄存器是标准做法，不会破坏调用约定。
 */
void cj_build_far_jump(uint32_t out[5], const void *target);

#define CJ_FAR_JUMP_LEN (5 * 4) /* 字节数，5 条指令 */

/* insn 是否是 PC 相对寻址的指令（结果依赖指令自身所在地址）：ADR/ADRP、B/BL、B.cond/BC.cond、CBZ/CBNZ、
 * TBZ/TBNZ、CB<cc>（FEAT_CMPBR）、LDR/LDRSW/PRFM (literal)。这类指令原样搬进调用桩后，算出来的地址/跳转目标
 * 是相对调用桩的，全错——cj_patch_target 搬运前用它检查，命中就放弃 hook（2026-10-10，审计 EN-2）。
 * 宁可误判（放弃一个其实能装的 hook）也不漏判：寄存器间接跳转 BR/BLR/RET 不算（BLR 写进 LR 的返回地址落在调用桩里，
 * 被调函数返回后接着执行桩里后面的指令，再跳回原函数，结果正确）。 */
int cj_insn_pc_relative(uint32_t insn);
