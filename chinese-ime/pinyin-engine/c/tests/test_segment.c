/* 对应 Python 版 tests/test_segment.py，同样的用例，逐条核对 C 移植版
 * 的切分结果跟 Python 版是否一致（不是"看起来差不多"，是逐字符串比较）。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/segment.h"

/* 辅助：调用 cj_best_segmentation，把结果的音节 span 还原成字符串数组，
 * 方便跟期望值比较。buf 复用调用方传入的小写化缓冲区。 */
static void best(const char *raw, char *buf, CjSegmentation *out) {
    int ok = cj_best_segmentation(raw, strlen(raw), buf, out);
    assert(ok && "cj_best_segmentation 不应该失败（raw_len 在限制内）");
}

static int syl_eq(const char *buf, CjSyllableSpan span, const char *expected) {
    size_t elen = strlen(expected);
    return span.len == elen && memcmp(buf + span.offset, expected, elen) == 0;
}

static void assert_syllables(const char *buf, CjSegmentation *seg, const char **expected, int n) {
    assert(seg->syllable_count == n);
    for (int i = 0; i < n; i++) {
        assert(syl_eq(buf, seg->syllables[i], expected[i]));
    }
}

static void test_greedy_longest_match_xianggang(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("xianggang", buf, &seg);
    const char *expected[] = {"xiang", "gang"};
    assert_syllables(buf, &seg, expected, 2);
    assert(!seg.has_pending);
    printf("test_greedy_longest_match_xianggang: OK\n");
}

static void test_apostrophe_forces_boundary_xian_vs_xi_an(void) {
    char buf1[CJ_SEG_MAX_INPUT], buf2[CJ_SEG_MAX_INPUT];
    CjSegmentation seg1, seg2;
    best("xian", buf1, &seg1);
    const char *e1[] = {"xian"};
    assert_syllables(buf1, &seg1, e1, 1);

    best("xi'an", buf2, &seg2);
    const char *e2[] = {"xi", "an"};
    assert_syllables(buf2, &seg2, e2, 2);
    printf("test_apostrophe_forces_boundary_xian_vs_xi_an: OK\n");
}

static void test_apostrophe_disambiguates_ang_gang_case(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("ang'gang", buf, &seg);
    const char *expected[] = {"ang", "gang"};
    assert_syllables(buf, &seg, expected, 2);
    printf("test_apostrophe_disambiguates_ang_gang_case: OK\n");
}

static void test_multiple_consecutive_initials_edge_case(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("nihao", buf, &seg);
    const char *expected[] = {"ni", "hao"};
    assert_syllables(buf, &seg, expected, 2);
    printf("test_multiple_consecutive_initials_edge_case: OK\n");
}

static void test_single_syllable(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("wo", buf, &seg);
    const char *expected[] = {"wo"};
    assert_syllables(buf, &seg, expected, 1);
    assert(!seg.has_pending);
    printf("test_single_syllable: OK\n");
}

static void test_empty_input(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("", buf, &seg);
    assert(seg.syllable_count == 0);
    assert(!seg.has_pending);
    printf("test_empty_input: OK\n");
}

static void test_pending_partial_last_syllable(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("zho", buf, &seg);
    assert(seg.syllable_count == 0);
    assert(seg.has_pending);
    assert(syl_eq(buf, seg.pending, "zho"));
    printf("test_pending_partial_last_syllable: OK\n");
}

static void test_pending_after_complete_syllables(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("nihaoz", buf, &seg);
    const char *expected[] = {"ni", "hao"};
    assert_syllables(buf, &seg, expected, 2);
    assert(seg.has_pending);
    assert(syl_eq(buf, seg.pending, "z"));
    printf("test_pending_after_complete_syllables: OK\n");
}

static void test_invalid_input_does_not_crash(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    best("qwqwqw", buf, &seg); /* 不崩就算过 */
    printf("test_invalid_input_does_not_crash: OK\n");
}

static void test_multiple_candidates_include_greedy_first(void) {
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation candidates[CJ_SEG_MAX_CANDIDATES];
    int count = 0;
    int ok = cj_segment("xianggang", strlen("xianggang"), buf, candidates, CJ_SEG_MAX_CANDIDATES, &count);
    assert(ok && count >= 1);
    const char *expected[] = {"xiang", "gang"};
    assert_syllables(buf, &candidates[0], expected, 2);
    printf("test_multiple_candidates_include_greedy_first: OK\n");
}

/* Python 版没有的额外用例：C 移植版特有的安全上限，防止长按同一个字母
 * 这类病态输入卡死/崩溃——不在 Python 版对照范围内，是移植时新加的
 * 防御性测试。 */
static void test_pathological_repeated_syllable_does_not_hang(void) {
    char raw[33];
    memset(raw, 'a', 30);
    raw[30] = '\0';
    char buf[CJ_SEG_MAX_INPUT];
    CjSegmentation seg;
    int ok = cj_best_segmentation(raw, 30, buf, &seg); /* 不挂起、不崩就算过 */
    assert(ok);
    printf("test_pathological_repeated_syllable_does_not_hang: OK\n");
}

static void test_oversized_input_rejected(void) {
    char raw[CJ_SEG_MAX_INPUT + 10];
    memset(raw, 'a', sizeof(raw));
    char buf[sizeof(raw)];
    CjSegmentation candidates[CJ_SEG_MAX_CANDIDATES];
    int count = 0;
    int ok = cj_segment(raw, sizeof(raw), buf, candidates, CJ_SEG_MAX_CANDIDATES, &count);
    assert(!ok);
    assert(count == 0);
    printf("test_oversized_input_rejected: OK\n");
}

int main(void) {
    test_greedy_longest_match_xianggang();
    test_apostrophe_forces_boundary_xian_vs_xi_an();
    test_apostrophe_disambiguates_ang_gang_case();
    test_multiple_consecutive_initials_edge_case();
    test_single_syllable();
    test_empty_input();
    test_pending_partial_last_syllable();
    test_pending_after_complete_syllables();
    test_invalid_input_does_not_crash();
    test_multiple_candidates_include_greedy_first();
    test_pathological_repeated_syllable_does_not_hang();
    test_oversized_input_rejected();
    printf("=== test_segment: 全部通过 ===\n");
    return 0;
}
