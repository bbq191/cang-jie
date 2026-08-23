/* 验证 dict.zh_tw.bin（白皮书 05 节繁体双 blob 设计）能被跟 dict.bin
 * 完全一样的 dictionary.c 代码打开、查到繁体候选——这是这个设计的核心
 * 主张（"C 引擎完全不变，只是 mmap 哪个文件"），这份测试就是验证这
 * 个主张成立，不是重新测一遍 dictionary.c 本身的正确性（那是
 * test_dictionary.c 的职责，两者不重复）。
 *
 * 依赖 c/build/dict.zh_tw.bin 已经生成好（`make dict-zh-tw-blob`，
 * `make test` 会自动先跑这一步）。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/dictionary.h"

#define DICT_ZH_TW_PATH "build/dict.zh_tw.bin"

static int index_of_word(CjDictCandidate *out, int n, const char *word) {
    size_t len = strlen(word);
    for (int i = 0; i < n; i++) {
        if (out[i].word_len == len && memcmp(out[i].word, word, len) == 0) return i;
    }
    return -1;
}

static void test_open_zh_tw_blob(void) {
    /* 复用一模一样的 cj_dict_open——不需要一个专门的 "cj_dict_open_traditional"
     * 之类的新函数，这正是 05 节设计"格式不变，只是内容不同"的意义所在。 */
    CjDictHandle h;
    int ok = cj_dict_open(DICT_ZH_TW_PATH, &h);
    assert(ok);
    assert(h.base != NULL);
    /* key 数量应该跟简体版一致（转换不改变拼音，只改变字形），候选数
     * 会因为同音异形词合并而略少于简体版（59 万降到约 58.6 万）。 */
    assert(h.key_count > 400000);
    assert(h.cand_count > 500000);
    cj_dict_close(&h);
    printf("test_open_zh_tw_blob: OK\n");
}

static void test_lookup_returns_traditional_not_simplified(void) {
    CjDictHandle h;
    assert(cj_dict_open(DICT_ZH_TW_PATH, &h));

    CjDictCandidate out[16];
    int n = cj_dict_lookup(&h, "wang luo", 8, out, 16);
    assert(n > 0);
    assert(index_of_word(out, n, "網路") >= 0);   /* 繁体（台湾正体）候选存在 */
    assert(index_of_word(out, n, "网络") < 0);    /* 简体候选不应该出现在这份 blob 里 */

    n = cj_dict_lookup(&h, "zhong guo", 9, out, 16);
    assert(n > 0);
    assert(index_of_word(out, 1, "中國") == 0);   /* 权重最高的第一名是"中國" */
    assert(index_of_word(out, n, "中国") < 0);

    cj_dict_close(&h);
    printf("test_lookup_returns_traditional_not_simplified: OK\n");
}

static void test_merged_candidate_weight_is_combined(void) {
    /* 跟 Python 版 test_build_traditional_table_merges_real_collision
     * 同一个真实碰撞用例："谙"（简体，权重 6174）和"諳"（词库自带的
     * 繁体异体，权重 0）转换后都是"諳"，合并后应该只有一条、权重是
     * 两条之和 6174，不是两条重复的"諳"。 */
    CjDictHandle h;
    assert(cj_dict_open(DICT_ZH_TW_PATH, &h));

    CjDictCandidate out[16];
    int n = cj_dict_lookup(&h, "an", 2, out, 16);
    assert(n > 0);

    int count = 0;
    uint32_t weight = 0;
    for (int i = 0; i < n; i++) {
        if (out[i].word_len == strlen("諳") && memcmp(out[i].word, "諳", strlen("諳")) == 0) {
            count++;
            weight = out[i].weight;
        }
    }
    assert(count == 1);       /* 只有一条，没有重复 */
    assert(weight == 6174);   /* 权重是合并后的 */

    cj_dict_close(&h);
    printf("test_merged_candidate_weight_is_combined: OK\n");
}

int main(void) {
    test_open_zh_tw_blob();
    test_lookup_returns_traditional_not_simplified();
    test_merged_candidate_weight_is_combined();
    printf("=== test_dictionary_zh_tw: 全部通过 ===\n");
    return 0;
}
