/* 对应 Python 版 tests/test_dictionary.py，同样的场景用 C 版
 * cj_dict_open/cj_dict_lookup/cj_dict_lookup_prefix 走一遍。
 *
 * 依赖 c/build/dict.bin 已经生成好（`make dict-blob`，`make test` 会自动
 * 先跑这一步），从 c/ 目录下用相对路径 "build/dict.bin" 打开。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/dictionary.h"

#define DICT_PATH "build/dict.bin"

static int index_of_word(CjDictCandidate *out, int n, const char *word) {
    size_t len = strlen(word);
    for (int i = 0; i < n; i++) {
        if (out[i].word_len == len && memcmp(out[i].word, word, len) == 0) return i;
    }
    return -1;
}

static void test_open_and_close(void) {
    CjDictHandle h;
    int ok = cj_dict_open(DICT_PATH, &h);
    assert(ok);
    assert(h.base != NULL);
    assert(h.key_count > 400000); /* 雾凇拼音实测约 45 万+ key */
    assert(h.cand_count > 500000);
    cj_dict_close(&h);
    assert(h.base == NULL);
    assert(h.map_len == 0);
    printf("test_open_and_close: OK\n");
}

static void test_open_missing_file_fails(void) {
    CjDictHandle h;
    memset(&h, 0xAA, sizeof(h)); /* 故意脏内存，验证失败路径会清零而不是留半吊子状态 */
    int ok = cj_dict_open("build/no_such_file.bin", &h);
    assert(!ok);
    assert(h.base == NULL);
    assert(h.key_count == 0);
    printf("test_open_missing_file_fails: OK\n");
}

static void test_open_corrupt_file_fails(void) {
    /* 造一个 magic 不对的小文件，确认校验会挡下来，不会当成合法数据用 */
    const char *path = "/tmp/cj_dict_corrupt_test.bin";
    FILE *f = fopen(path, "wb");
    assert(f);
    char junk[200] = {0};
    memcpy(junk, "NOTADICT", 8);
    fwrite(junk, 1, sizeof(junk), f);
    fclose(f);

    CjDictHandle h;
    int ok = cj_dict_open(path, &h);
    assert(!ok);
    assert(h.base == NULL);
    remove(path);
    printf("test_open_corrupt_file_fails: OK\n");
}

static void test_exact_lookup_common_word(void) {
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[16];
    int n = cj_dict_lookup(&h, "ni hao", 6, out, 16);
    assert(n > 0);
    assert(index_of_word(out, n, "你好") >= 0);

    cj_dict_close(&h);
    printf("test_exact_lookup_common_word: OK\n");
}

static void test_common_char_outranks_rare_char(void) {
    /* 回归测试：跟 Python 版 test_common_char_outranks_rare_char_with_same_syllable
     * 同一个用例——"guo" 音节下，常用字"国"排名必须在生僻字"掴"前面。 */
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[64];
    int n = cj_dict_lookup(&h, "guo", 3, out, 64);
    int idx_guo = index_of_word(out, n, "国");
    int idx_guai = index_of_word(out, n, "掴");
    assert(idx_guo >= 0 && idx_guai >= 0);
    assert(idx_guo < idx_guai);

    n = cj_dict_lookup(&h, "de", 2, out, 64);
    assert(n > 0);
    assert(index_of_word(out, 1, "的") == 0); /* 权重最高的第一名必须是"的" */

    cj_dict_close(&h);
    printf("test_common_char_outranks_rare_char: OK\n");
}

static void test_exact_lookup_unknown_returns_zero(void) {
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[4];
    int n = cj_dict_lookup(&h, "zzz", 3, out, 4);
    assert(n == 0);

    cj_dict_close(&h);
    printf("test_exact_lookup_unknown_returns_zero: OK\n");
}

static void test_prefix_lookup_matches_partial_last_syllable(void) {
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[32];
    int n = cj_dict_lookup_prefix(&h, "ni h", 4, out, 32);
    assert(n > 0);
    assert(index_of_word(out, n, "你好") >= 0);

    cj_dict_close(&h);
    printf("test_prefix_lookup_matches_partial_last_syllable: OK\n");
}

static void test_prefix_lookup_excludes_wrong_syllable_count(void) {
    /* 真机数据里的真实碰撞用例：("ni","hao","a") 这个 3 音节词的 key 字符串
     * "ni hao a" 在字节上是 "ni ha" 的前缀，但查询 lookup_prefix(("ni","ha"))
     * 意图是 2 音节（"ni" 完整 + "ha" 是第二个音节打到一半），"你好啊"这个
     * 3 音节词不应该被算作命中——如果空格计数过滤没生效，这个测试会失败。 */
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[64];
    int n = cj_dict_lookup_prefix(&h, "ni ha", 5, out, 64);
    assert(n > 0);
    assert(index_of_word(out, n, "你好") >= 0); /* 2 音节的候选应该在 */
    assert(index_of_word(out, n, "你好啊") < 0); /* 3 音节的候选不应该混进来 */

    cj_dict_close(&h);
    printf("test_prefix_lookup_excludes_wrong_syllable_count: OK\n");
}

static void test_prefix_lookup_empty_when_no_match(void) {
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[4];
    int n = cj_dict_lookup_prefix(&h, "zzzzz", 5, out, 4);
    assert(n == 0);

    cj_dict_close(&h);
    printf("test_prefix_lookup_empty_when_no_match: OK\n");
}

static void test_max_out_caps_result(void) {
    CjDictHandle h;
    assert(cj_dict_open(DICT_PATH, &h));

    CjDictCandidate out[3];
    int n = cj_dict_lookup(&h, "guo", 3, out, 3); /* "guo" 实测有 16 个候选，远超 3 */
    assert(n == 3);

    cj_dict_close(&h);
    printf("test_max_out_caps_result: OK\n");
}

int main(void) {
    test_open_and_close();
    test_open_missing_file_fails();
    test_open_corrupt_file_fails();
    test_exact_lookup_common_word();
    test_common_char_outranks_rare_char();
    test_exact_lookup_unknown_returns_zero();
    test_prefix_lookup_matches_partial_last_syllable();
    test_prefix_lookup_excludes_wrong_syllable_count();
    test_prefix_lookup_empty_when_no_match();
    test_max_out_caps_result();
    printf("=== test_dictionary: 全部通过 ===\n");
    return 0;
}
