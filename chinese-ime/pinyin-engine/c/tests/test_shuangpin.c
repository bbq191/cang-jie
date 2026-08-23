/* 对照 Python 版 tests/test_shuangpin.py 的用例，验证 C 移植版
 * （查表实现，见 shuangpin.h 顶部说明）行为一致。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/shuangpin.h"

static void check(const char *keys, const char *expect) {
    char out[64];
    size_t n = cj_shuangpin_keys_to_pinyin(keys, strlen(keys), out, sizeof(out));
    if (n != strlen(expect) || strcmp(out, expect) != 0) {
        fprintf(stderr, "FAIL: keys=%s expect=%s got=%s (n=%zu)\n", keys, expect, out, n);
        assert(0);
    }
}

static void test_single_letter_final_passthrough(void) {
    check("ni", "ni");
    check("gu", "gu");
    printf("test_single_letter_final_passthrough: OK\n");
}

static void test_compressed_final_hao(void) {
    check("hc", "hao");
    printf("test_compressed_final_hao: OK\n");
}

static void test_zh_ch_sh_initial_compression(void) {
    check("vs", "zhong");
    printf("test_zh_ch_sh_initial_compression: OK\n");
}

static void test_multi_syllable_word(void) {
    check("vsgo", "zhong'guo");
    printf("test_multi_syllable_word: OK\n");
}

static void test_odd_length_keys_pending_passthrough(void) {
    check("niz", "ni'z");
    printf("test_odd_length_keys_pending_passthrough: OK\n");
}

static void test_single_pending_key_only(void) {
    /* 只打了一个键（第一个音节还没打完），没有任何完整音节可解码。 */
    check("z", "z");
    printf("test_single_pending_key_only: OK\n");
}

static void test_empty_input(void) {
    char out[16];
    size_t n = cj_shuangpin_keys_to_pinyin("", 0, out, sizeof(out));
    assert(n == 0);
    assert(out[0] == '\0');
    n = cj_shuangpin_keys_to_pinyin(NULL, 0, out, sizeof(out));
    assert(n == 0);
    printf("test_empty_input: OK\n");
}

static void test_output_truncation_reports_needed_length(void) {
    /* out_cap 太小时不越界写，但返回值仍然是"应该写入的完整长度"，
     * 调用方可以借此判断发生了截断——对照 snprintf 的返回值语义。 */
    char small[4];
    size_t n = cj_shuangpin_keys_to_pinyin("vsgo", 4, small, sizeof(small));
    assert(n == strlen("zhong'guo")); /* 返回完整需要的长度，不是截断后的长度 */
    assert(n >= sizeof(small));       /* 调用方据此判断确实被截断了 */
    assert(small[sizeof(small) - 1] == '\0'); /* 缓冲区内没有越界写，仍然是合法 C 字符串 */
    printf("test_output_truncation_reports_needed_length: OK\n");
}

static void test_defensive_nonalpha_passthrough_does_not_crash(void) {
    /* 防御性兜底：不该发生的非法字符（非 a-z），不查表越界，原样透传。
     * 这里只验证不崩溃/不越界，不对具体输出内容做强断言（本来就是
     * "不应该发生"的输入，行为只是兜底，不是正式契约）。 */
    char out[16];
    size_t n = cj_shuangpin_keys_to_pinyin("3g", 2, out, sizeof(out));
    (void)n;
    printf("test_defensive_nonalpha_passthrough_does_not_crash: OK\n");
}

int main(void) {
    test_single_letter_final_passthrough();
    test_compressed_final_hao();
    test_zh_ch_sh_initial_compression();
    test_multi_syllable_word();
    test_odd_length_keys_pending_passthrough();
    test_single_pending_key_only();
    test_empty_input();
    test_output_truncation_reports_needed_length();
    test_defensive_nonalpha_passthrough_does_not_crash();
    printf("=== test_shuangpin: 全部通过 ===\n");
    return 0;
}
