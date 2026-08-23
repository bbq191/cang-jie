/* 验证 dict_jianpin.zh_tw.bin（繁体简拼索引，07 节简拼真机接入时暴露的
 * 缺口——用户实测反馈"切换繁体后依然输出简体"，根因是简拼补充候选一直
 * 只查简体版 dict_jianpin.bin）能被跟 dict_jianpin.bin 完全一样的
 * jianpin.c 代码打开、查到繁体候选，跟 test_dictionary_zh_tw.c 对
 * dict.zh_tw.bin 的验证是同一个核心主张（"C 引擎完全不变，只是 mmap
 * 哪个文件"），这份测试验证这个主张对简拼索引同样成立。
 *
 * 依赖 c/build/dict_jianpin.zh_tw.bin 已经生成好
 * （`make jianpin-zh-tw-blob`，`make test` 会自动先跑这一步）。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/jianpin.h"

#define JIANPIN_ZH_TW_PATH "build/dict_jianpin.zh_tw.bin"

static int index_of_word(CjJianpinCandidate *out, int n, const char *word) {
    size_t len = strlen(word);
    for (int i = 0; i < n; i++) {
        if (out[i].word_len == len && memcmp(out[i].word, word, len) == 0) return i;
    }
    return -1;
}

static void test_open_zh_tw_jianpin_blob(void) {
    CjJianpinHandle h;
    int ok = cj_jianpin_open(JIANPIN_ZH_TW_PATH, &h);
    assert(ok);
    assert(h.base != NULL);
    /* key 数量应该跟简体版一致（简拼 key 只取决于读音，不受字形转换
     * 影响），候选数会因为同音异形词合并而略少于简体版。 */
    assert(h.key_count > 100000);
    assert(h.cand_count > 500000);
    cj_jianpin_close(&h);
    printf("test_open_zh_tw_jianpin_blob: OK\n");
}

static void test_lookup_returns_traditional_not_simplified(void) {
    CjJianpinHandle h;
    assert(cj_jianpin_open(JIANPIN_ZH_TW_PATH, &h));

    CjJianpinCandidate out[32];
    /* "zg" -> zhong guo，跟主词典 dict.zh_tw.bin 测试用的是同一个词，
     * 方便交叉核对。 */
    int n = cj_jianpin_lookup(&h, "zg", 2, out, 32);
    assert(n > 0);
    assert(index_of_word(out, n, "中國") >= 0);
    assert(index_of_word(out, n, "中国") < 0); /* 简体候选不应该出现在这份 blob 里 */

    cj_jianpin_close(&h);
    printf("test_lookup_returns_traditional_not_simplified: OK\n");
}

int main(void) {
    test_open_zh_tw_jianpin_blob();
    test_lookup_returns_traditional_not_simplified();
    printf("=== test_jianpin_zh_tw: 全部通过 ===\n");
    return 0;
}
