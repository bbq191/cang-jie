#include "trampoline_aarch64.h"

void cj_build_far_jump(uint32_t out[5], const void *target) {
    uint64_t addr = (uint64_t)(uintptr_t)target;
    uint32_t part0 = (uint32_t)((addr >> 0) & 0xFFFF);
    uint32_t part1 = (uint32_t)((addr >> 16) & 0xFFFF);
    uint32_t part2 = (uint32_t)((addr >> 32) & 0xFFFF);
    uint32_t part3 = (uint32_t)((addr >> 48) & 0xFFFF);

    out[0] = 0xD2800010u | (part0 << 5); /* movz x16, #part0            */
    out[1] = 0xF2A00010u | (part1 << 5); /* movk x16, #part1, lsl #16   */
    out[2] = 0xF2C00010u | (part2 << 5); /* movk x16, #part2, lsl #32   */
    out[3] = 0xF2E00010u | (part3 << 5); /* movk x16, #part3, lsl #48   */
    out[4] = 0xD61F0200u;                /* br x16                     */
}

int cj_insn_pc_relative(uint32_t insn) {
    /* 掩码按 Arm ARM（DDI 0487）A64 编码表各指令类的固定位取；host 单测的样例编码由 GNU as 2.47
     * （-march=armv9.6-a+cmpbr）汇编再 objdump 得到，见 tests/test_shared.c。 */
    if ((insn & 0x1F000000u) == 0x10000000u) return 1; /* ADR / ADRP：op 位 31 区分，bits[28:24]=10000 */
    if ((insn & 0x7C000000u) == 0x14000000u) return 1; /* B / BL：bits[30:26]=00101 */
    if ((insn & 0xFF000000u) == 0x54000000u) return 1; /* B.cond / BC.cond（bit 4 区分） */
    if ((insn & 0x7E000000u) == 0x34000000u) return 1; /* CBZ / CBNZ（32/64 位） */
    if ((insn & 0x7E000000u) == 0x36000000u) return 1; /* TBZ / TBNZ */
    if ((insn & 0x7E000000u) == 0x74000000u) return 1; /* CB<cc> / CBB<cc> / CBH<cc>（FEAT_CMPBR，较新的比较并跳转） */
    if ((insn & 0x3B000000u) == 0x18000000u) return 1; /* LDR (literal)（整数/SIMD&FP）、LDRSW (literal)、PRFM (literal) */
    return 0;
}
