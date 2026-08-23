/* scan.c 的宿主机单元测试。用构造好的 /proc/self/maps 格式字符串测试解析逻辑，
 * 不依赖真实 xochitl 存在；另外用真实的 /proc/self/maps（这个测试进程自己的）
 * 做一次"读真实文件不崩"的冒烟测试。
 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/scan.h"

static const char *FAKE_MAPS =
    "55d2f1000000-55d2f1005000 r--p 00000000 08:01 100  /usr/bin/xochitl\n"
    "55d2f1005000-55d2f1a21000 r-xp 00005000 08:01 100  /usr/bin/xochitl\n"
    "55d2f1a21000-55d2f1a30000 rw-p 00a21000 08:01 100  /usr/bin/xochitl\n"
    "7f0000000000-7f0000021000 r-xp 00000000 08:02 200  /usr/lib/libQt6Core.so.6\n"
    "7fff00000000-7fff00021000 rw-p 00000000 00:00 0    \n" /* 匿名映射，无路径 */
    "7fff10000000-7fff10021000 r-xp 00000000 00:00 0    [vdso]\n";

static void test_finds_correct_executable_segment(void) {
    uintptr_t base = 0;
    size_t size = 0;
    int ok = cj_find_exec_module("/usr/bin/xochitl", FAKE_MAPS, &base, &size);
    assert(ok == 1);
    /* 应该命中的是 r-xp 那一段（第二行），不是只读的第一行或可写的第三行 */
    assert(base == 0x55d2f1005000ULL);
    assert(size == (0x55d2f1a21000ULL - 0x55d2f1005000ULL));
    printf("test_finds_correct_executable_segment: OK\n");
}

static void test_no_match_returns_zero(void) {
    uintptr_t base = 0;
    size_t size = 0;
    int ok = cj_find_exec_module("/no/such/binary", FAKE_MAPS, &base, &size);
    assert(ok == 0);
    printf("test_no_match_returns_zero: OK\n");
}

static void test_anonymous_and_vdso_dont_crash_parser(void) {
    /* 匿名映射（没有路径字段）和 [vdso] 这类伪路径不应该让解析器崩溃或误判 */
    uintptr_t base = 0;
    size_t size = 0;
    int ok = cj_find_exec_module("[vdso]", FAKE_MAPS, &base, &size);
    assert(ok == 1);
    assert(base == 0x7fff10000000ULL);
    printf("test_anonymous_and_vdso_dont_crash_parser: OK\n");
}

static void test_real_proc_self_maps_smoke(void) {
    /* 用 NULL 触发真实读 /proc/self/maps；这个测试进程自己的可执行段路径
     * 一定在里面，用它自己的路径做后缀匹配，验证“读真实文件”这条路径不崩、
     * 能返回合理的非零 base/size。具体路径不确定（构建系统不同而异），这里
     * 退而求其次，只验证“读 /proc/self/maps 不崩溃”本身：拿一个几乎不可能
     * 匹配到的后缀，确认返回 0 而不是崩溃/未定义行为。 */
    uintptr_t base = 0;
    size_t size = 0;
    int ok = cj_find_exec_module("/this/path/should/not/exist/anywhere", NULL, &base, &size);
    assert(ok == 0);
    printf("test_real_proc_self_maps_smoke: OK\n");
}

int main(void) {
    test_finds_correct_executable_segment();
    test_no_match_returns_zero();
    test_anonymous_and_vdso_dont_crash_parser();
    test_real_proc_self_maps_smoke();
    printf("=== test_scan: 全部通过 ===\n");
    return 0;
}
