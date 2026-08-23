/* pattern.c 的宿主机单元测试——纯 C 逻辑，不需要 ARM64/真机就能跑。
 * 用真实的 48 字节 setLanguageCode 特征码做测试数据，覆盖三种情况：
 * 唯一命中、零命中、命中多次（对应 hook_init.c 里“唯一性校验”要防的
 * 那种情况）。
 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/pattern.h"

static const uint8_t PROLOGUE[] = {
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xab, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf5, 0x5b, 0x02, 0xa9, 0x15, 0xa0, 0x00, 0x91, 0x23, 0x08, 0x40, 0xf9,
    0xf3, 0x53, 0x01, 0xa9, 0xf3, 0x03, 0x01, 0xaa, 0xa1, 0x8a, 0x40, 0xa9,
    0xf4, 0x03, 0x00, 0xaa, 0x5f, 0x00, 0x03, 0xeb, 0x63, 0x06, 0x40, 0xf9,
};
#define PROLOGUE_LEN sizeof(PROLOGUE)

static void test_unique_hit(void) {
    uint8_t buf[4096] = {0};
    /* 前后塞点无关字节，特征码放中间某个位置 */
    size_t offset = 1000;
    memcpy(buf + offset, PROLOGUE, PROLOGUE_LEN);

    uintptr_t addr = 0;
    int ok = cj_find_unique_pattern(buf, sizeof(buf), PROLOGUE, PROLOGUE_LEN, &addr);
    assert(ok == 1);
    assert(addr == (uintptr_t)(buf + offset));
    printf("test_unique_hit: OK\n");
}

static void test_zero_hits(void) {
    uint8_t buf[4096];
    memset(buf, 0xAB, sizeof(buf)); /* 全是跟特征码无关的字节 */

    uintptr_t addr = 0;
    int ok = cj_find_unique_pattern(buf, sizeof(buf), PROLOGUE, PROLOGUE_LEN, &addr);
    assert(ok == 0);
    printf("test_zero_hits: OK\n");
}

static void test_multiple_hits_rejected(void) {
    uint8_t buf[4096] = {0};
    memcpy(buf + 100, PROLOGUE, PROLOGUE_LEN);
    memcpy(buf + 2000, PROLOGUE, PROLOGUE_LEN); /* 同一个特征码出现第二次 */

    uintptr_t addr = 0;
    int ok = cj_find_unique_pattern(buf, sizeof(buf), PROLOGUE, PROLOGUE_LEN, &addr);
    assert(ok == 0); /* 命中 2 次必须被拒绝，不能随便选一个 */

    size_t count = cj_count_pattern(buf, sizeof(buf), PROLOGUE, PROLOGUE_LEN, &addr);
    assert(count == 2);
    printf("test_multiple_hits_rejected: OK\n");
}

static void test_haystack_shorter_than_pattern(void) {
    uint8_t buf[10] = {0};
    uintptr_t addr = 0;
    int ok = cj_find_unique_pattern(buf, sizeof(buf), PROLOGUE, PROLOGUE_LEN, &addr);
    assert(ok == 0); /* 不能越界读，也不能误报命中 */
    printf("test_haystack_shorter_than_pattern: OK\n");
}

/* 韧性重构：掩码路径。掩码字节==0 通配、!=0 精确。构造两份只在被通配位置
 * 不同的数据，验证：精确匹配各自只命中自己；带掩码把差异位通配后，同一条
 * 特征码对两份都命中 → 唯一性因此被“放宽”成命中 2 次（正是掩码盖太多会
 * 撞车的写照，测试这里就是要看到 count==2、find_unique 因此拒绝）。 */
static void test_masked_wildcards_bytes(void) {
    uint8_t buf[4096] = {0};
    uint8_t a[PROLOGUE_LEN], b[PROLOGUE_LEN];
    memcpy(a, PROLOGUE, PROLOGUE_LEN);
    memcpy(b, PROLOGUE, PROLOGUE_LEN);
    /* 把 b 的 [20..24) 改掉（模拟一条 BL 相对偏移随重定位变化） */
    b[20] ^= 0xFF; b[21] ^= 0xFF; b[22] ^= 0xFF; b[23] ^= 0xFF;
    memcpy(buf + 100, a, PROLOGUE_LEN);
    memcpy(buf + 2000, b, PROLOGUE_LEN);

    /* 精确：用 a 只命中 buf+100（b 那份因为改了字节不匹配） */
    uintptr_t addr = 0;
    int ok = cj_find_unique_pattern(buf, sizeof(buf), a, PROLOGUE_LEN, &addr);
    assert(ok == 1 && addr == (uintptr_t)(buf + 100));

    /* 掩码把 [20..24) 通配后：a 这条特征码对 a、b 两份都匹配 → 命中 2 次 */
    uint8_t mask[PROLOGUE_LEN];
    memset(mask, 1, sizeof(mask));
    mask[20] = mask[21] = mask[22] = mask[23] = 0;
    size_t count = cj_count_pattern_masked(buf, sizeof(buf), a, mask, PROLOGUE_LEN, &addr);
    assert(count == 2);
    int uniq = cj_find_unique_pattern_masked(buf, sizeof(buf), a, mask, PROLOGUE_LEN, &addr);
    assert(uniq == 0); /* 命中 2 次，唯一性校验必须拒绝 */

    /* mask==NULL 时与精确版本完全等价 */
    uintptr_t addr2 = 0;
    int ok2 = cj_find_unique_pattern_masked(buf, sizeof(buf), a, NULL, PROLOGUE_LEN, &addr2);
    assert(ok2 == 1 && addr2 == (uintptr_t)(buf + 100));
    printf("test_masked_wildcards_bytes: OK\n");
}

/* 掩码单命中：只有一份数据、被通配的位置随便填，仍应唯一命中。 */
static void test_masked_unique_hit(void) {
    uint8_t buf[4096] = {0};
    uint8_t data[PROLOGUE_LEN];
    memcpy(data, PROLOGUE, PROLOGUE_LEN);
    data[20] = 0x11; data[21] = 0x22; data[22] = 0x33; data[23] = 0x44; /* 任意值 */
    memcpy(buf + 1500, data, PROLOGUE_LEN);

    uint8_t mask[PROLOGUE_LEN];
    memset(mask, 1, sizeof(mask));
    mask[20] = mask[21] = mask[22] = mask[23] = 0;

    uintptr_t addr = 0;
    /* 用原始 PROLOGUE（被通配位置跟 data 不同）+ 掩码，应命中 buf+1500 */
    int ok = cj_find_unique_pattern_masked(buf, sizeof(buf), PROLOGUE, mask, PROLOGUE_LEN, &addr);
    assert(ok == 1 && addr == (uintptr_t)(buf + 1500));
    printf("test_masked_unique_hit: OK\n");
}

int main(void) {
    test_unique_hit();
    test_zero_hits();
    test_multiple_hits_rejected();
    test_haystack_shorter_than_pattern();
    test_masked_wildcards_bytes();
    test_masked_unique_hit();
    printf("=== test_pattern: 全部通过 ===\n");
    return 0;
}
