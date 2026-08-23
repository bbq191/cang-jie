/* 对应 Python 版 tests/test_syllables.py，同样的用例，验证 C 移植版
 * 跟 Python 版行为一致。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/syllables.h"
#include "../src/syllables_data.h"

static int valid(const char *s) { return cj_is_valid_syllable(s, strlen(s)); }
static int prefix(const char *s) { return cj_is_prefix_of_some_syllable(s, strlen(s)); }

static void test_common_syllables_are_valid(void) {
    const char *syls[] = {"xiang", "gang", "ni", "hao", "zhong", "guo", "a", "e", "yi", "wu", "yu", "er"};
    for (size_t i = 0; i < sizeof(syls) / sizeof(syls[0]); i++) {
        assert(valid(syls[i]) && "应该是合法音节");
    }
    printf("test_common_syllables_are_valid: OK\n");
}

static void test_illegal_combinations_are_rejected(void) {
    /* 注：biang 反而是合法音节（来自"biángbiáng面"），不在这个反例列表里，
     * 跟 Python 版注释保持一致。 */
    const char *syls[] = {"bia", "jang", "zhia", "kiang", "pong"};
    for (size_t i = 0; i < sizeof(syls) / sizeof(syls[0]); i++) {
        assert(!valid(syls[i]) && "不应该是合法音节");
    }
    printf("test_illegal_combinations_are_rejected: OK\n");
}

static void test_retroflex_and_apical_i(void) {
    const char *syls[] = {"zhi", "chi", "shi", "ri", "zi", "ci", "si"};
    for (size_t i = 0; i < sizeof(syls) / sizeof(syls[0]); i++) {
        assert(valid(syls[i]));
    }
    printf("test_retroflex_and_apical_i: OK\n");
}

static void test_prefix_lookup(void) {
    assert(prefix("zho"));
    assert(prefix("x"));
    assert(!prefix("zzz"));
    printf("test_prefix_lookup: OK\n");
}

static void test_syllable_table_reasonable_size(void) {
    assert(CJ_SYLLABLE_COUNT >= 350 && CJ_SYLLABLE_COUNT <= 500);
    printf("test_syllable_table_reasonable_size: OK (count=%d)\n", CJ_SYLLABLE_COUNT);
}

int main(void) {
    test_common_syllables_are_valid();
    test_illegal_combinations_are_rejected();
    test_retroflex_and_apical_i();
    test_prefix_lookup();
    test_syllable_table_reasonable_size();
    printf("=== test_syllables: 全部通过 ===\n");
    return 0;
}
