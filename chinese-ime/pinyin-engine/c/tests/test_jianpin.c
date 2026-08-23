/* 对应 Python 版 tests/test_jianpin.py，同样的场景用 C 版
 * cj_jianpin_open/cj_jianpin_lookup/cj_jianpin_lookup_prefix 走一遍。
 *
 * 依赖 c/build/dict_jianpin.bin 已经生成好（`make jianpin-blob`，
 * `make test` 会自动先跑这一步），从 c/ 目录下用相对路径打开。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/jianpin.h"

#define JIANPIN_PATH "build/dict_jianpin.bin"

static int index_of_word(CjJianpinCandidate *out, int n, const char *word) {
    size_t len = strlen(word);
    for (int i = 0; i < n; i++) {
        if (out[i].word_len == len && memcmp(out[i].word, word, len) == 0) return i;
    }
    return -1;
}

static void test_open_and_close(void) {
    CjJianpinHandle h;
    int ok = cj_jianpin_open(JIANPIN_PATH, &h);
    assert(ok);
    assert(h.base != NULL);
    assert(h.key_count > 100000); /* 实测约 16.7 万个不同的简拼 key */
    assert(h.cand_count > 500000); /* 实测约 54.3 万条候选 */
    cj_jianpin_close(&h);
    assert(h.base == NULL);
    assert(h.map_len == 0);
    printf("test_open_and_close: OK\n");
}

static void test_open_missing_file_fails(void) {
    CjJianpinHandle h;
    memset(&h, 0xAA, sizeof(h)); /* 故意脏内存，验证失败路径会清零而不是留半吊子状态 */
    int ok = cj_jianpin_open("build/no_such_file.bin", &h);
    assert(!ok);
    assert(h.base == NULL);
    assert(h.key_count == 0);
    printf("test_open_missing_file_fails: OK\n");
}

static void test_open_corrupt_file_fails(void) {
    /* 造一个 magic 不对的小文件（用 dictionary.c 的 magic 冒充，确认
     * 两种 blob 格式不会被互相误当成对方接受），确认校验会挡下来。 */
    const char *path = "/tmp/cj_jianpin_corrupt_test.bin";
    FILE *f = fopen(path, "wb");
    assert(f);
    char junk[200] = {0};
    memcpy(junk, "CJDICT01", 8); /* 故意用词典 blob 的 magic，不是简拼的 */
    fwrite(junk, 1, sizeof(junk), f);
    fclose(f);

    CjJianpinHandle h;
    int ok = cj_jianpin_open(path, &h);
    assert(!ok);
    assert(h.base == NULL);
    remove(path);
    printf("test_open_corrupt_file_fails: OK\n");
}

static void test_exact_lookup_common_word(void) {
    /* "zg" = zhong(z) + guo(g) -> "中国" */
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[16];
    int n = cj_jianpin_lookup(&h, "zg", 2, out, 16);
    assert(n > 0);
    assert(index_of_word(out, n, "中国") >= 0);

    cj_jianpin_close(&h);
    printf("test_exact_lookup_common_word: OK\n");
}

static void test_index_excludes_single_syllable_keys(void) {
    /* 单音节词不该被收录——查长度为 1 的 key（比如"国"的简拼首字母
     * "g"）应该一无所获，跟 Python 版 test_index_excludes_single_syllable_entries
     * 同一个不变量。 */
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[16];
    int n = cj_jianpin_lookup(&h, "g", 1, out, 16);
    assert(n == 0);

    /* 顺带确认 key_table 里确实不存在长度为 1 的 key（抽查前几条，
     * key_table 按字典序排列，长度 1 的 key 如果存在必然排在最前面）。 */
    if (h.key_count > 0) {
        assert(h.key_table[0].key_str_len >= 2);
    }

    cj_jianpin_close(&h);
    printf("test_index_excludes_single_syllable_keys: OK\n");
}

static void test_exact_lookup_unknown_returns_zero(void) {
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[4];
    int n = cj_jianpin_lookup(&h, "zzzzzz", 6, out, 4);
    assert(n == 0);

    cj_jianpin_close(&h);
    printf("test_exact_lookup_unknown_returns_zero: OK\n");
}

static void test_prefix_lookup_finds_longer_words(void) {
    /* "py" 前缀应该同时覆盖精确匹配（"拼音"）和更长的词
     * （"拼音输入法" -> key "pysrf"），跟 Python 版
     * test_prefix_lookup_matches_longer_words 同一个用例。 */
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[1024];
    int n = cj_jianpin_lookup_prefix(&h, "py", 2, out, 1024);
    assert(n > 0);
    assert(index_of_word(out, n, "拼音输入法") >= 0);

    cj_jianpin_close(&h);
    printf("test_prefix_lookup_finds_longer_words: OK\n");
}

static void test_prefix_lookup_empty_when_no_match(void) {
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[4];
    int n = cj_jianpin_lookup_prefix(&h, "zzzzz", 5, out, 4);
    assert(n == 0);

    cj_jianpin_close(&h);
    printf("test_prefix_lookup_empty_when_no_match: OK\n");
}

static void test_prefix_lookup_empty_prefix_returns_zero(void) {
    /* 跟 Python 版 test_empty_prefix_returns_empty 对齐：空前缀不是
     * "匹配一切"，是直接返回空。 */
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[4];
    int n = cj_jianpin_lookup_prefix(&h, "", 0, out, 4);
    assert(n == 0);

    cj_jianpin_close(&h);
    printf("test_prefix_lookup_empty_prefix_returns_zero: OK\n");
}

static void test_max_out_caps_result(void) {
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[3];
    /* "z" 前缀命中的候选数远超 3（manifest 里 max_cands_per_key 都有
     * 上千），随便截一个小 max_out 验证结果确实被截断到 3 个。 */
    int n = cj_jianpin_lookup_prefix(&h, "z", 1, out, 3);
    assert(n == 3);

    cj_jianpin_close(&h);
    printf("test_max_out_caps_result: OK\n");
}

static void test_prefix_results_sorted_by_weight_desc(void) {
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_PATH, &h));

    CjJianpinCandidate out[64];
    int n = cj_jianpin_lookup_prefix(&h, "z", 1, out, 64);
    assert(n > 1);
    for (int i = 1; i < n; i++) {
        assert(out[i - 1].weight >= out[i].weight);
    }

    cj_jianpin_close(&h);
    printf("test_prefix_results_sorted_by_weight_desc: OK\n");
}

int main(void) {
    test_open_and_close();
    test_open_missing_file_fails();
    test_open_corrupt_file_fails();
    test_exact_lookup_common_word();
    test_index_excludes_single_syllable_keys();
    test_exact_lookup_unknown_returns_zero();
    test_prefix_lookup_finds_longer_words();
    test_prefix_lookup_empty_when_no_match();
    test_prefix_lookup_empty_prefix_returns_zero();
    test_max_out_caps_result();
    test_prefix_results_sorted_by_weight_desc();
    printf("=== test_jianpin: 全部通过 ===\n");
    return 0;
}
