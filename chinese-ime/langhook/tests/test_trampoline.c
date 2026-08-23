/* trampoline_aarch64.c 的宿主机单元测试。这段代码本身只是位运算（把一个 64 位
 * 地址拆成 4 个 16 位段装进 MOVZ/MOVK 指令编码），不涉及任何 ARM64 特有的运行时
 * 行为，所以可以在 x86_64 开发机上直接跑单元测试验证编码是否正确——真正“这几
 * 条指令在 CPU 上执行起来对不对”这件事，仍然需要真机（或至少 QEMU 用户态模拟）
 * 才能验证，这里没有覆盖到，只保证“地址编码进指令的算法是对的”。
 */
#include <assert.h>
#include <stdio.h>
#include <stdint.h>
#include "../src/trampoline_aarch64.h"

/* 把 cj_build_far_jump() 生成的 4 条 MOVZ/MOVK 指令解码回原始地址，跟已知的
 * AArch64 编码规则对齐，用来交叉验证编码逻辑本身没写反（比如段的顺序、位移
 * 量、掩码搞错这类最容易犯的错误）。*/
static uint64_t decode_address(const uint32_t instrs[5]) {
    uint64_t part0 = (instrs[0] >> 5) & 0xFFFF;
    uint64_t part1 = (instrs[1] >> 5) & 0xFFFF;
    uint64_t part2 = (instrs[2] >> 5) & 0xFFFF;
    uint64_t part3 = (instrs[3] >> 5) & 0xFFFF;
    return part0 | (part1 << 16) | (part2 << 32) | (part3 << 48);
}

static void test_roundtrip_various_addresses(void) {
    uint64_t test_addrs[] = {
        0x0ULL,
        0x1000ULL,
        0x913c70ULL,               /* 白皮书 10.1 节实测的 3.28.0.164 偏移 */
        0xa0ff50ULL,               /* 3.27.1.0 偏移 */
        0x7f5c00913c70ULL,         /* 模拟运行时加了 ASLR 基址之后的完整地址 */
        0xFFFFFFFFFFFFFFFFULL,     /* 边界值 */
    };
    for (size_t i = 0; i < sizeof(test_addrs) / sizeof(test_addrs[0]); i++) {
        uint32_t instrs[5];
        cj_build_far_jump(instrs, (const void *)(uintptr_t)test_addrs[i]);
        uint64_t decoded = decode_address(instrs);
        assert(decoded == test_addrs[i]);
    }
    printf("test_roundtrip_various_addresses: OK\n");
}

static void test_opcode_bits_stable(void) {
    /* 确认操作码的固定位没有被地址的低位污染——用一个所有段都非零的地址，
     * 检查每条指令去掉立即数字段后剩下的固定位跟预期的 MOVZ/MOVK/BR 编码
     * 一致（寄存器固定用 x16=0b10000）。 */
    uint32_t instrs[5];
    cj_build_far_jump(instrs, (const void *)(uintptr_t)0x1122334455667788ULL);

    assert((instrs[0] & 0xFFE0001Fu) == 0xD2800010u); /* movz x16, ... */
    assert((instrs[1] & 0xFFE0001Fu) == 0xF2A00010u); /* movk x16, ..., lsl #16 */
    assert((instrs[2] & 0xFFE0001Fu) == 0xF2C00010u); /* movk x16, ..., lsl #32 */
    assert((instrs[3] & 0xFFE0001Fu) == 0xF2E00010u); /* movk x16, ..., lsl #48 */
    assert(instrs[4] == 0xD61F0200u);                 /* br x16，没有立即数字段 */
    printf("test_opcode_bits_stable: OK\n");
}

int main(void) {
    test_roundtrip_various_addresses();
    test_opcode_bits_stable();
    printf("=== test_trampoline: 全部通过 ===\n");
    return 0;
}
