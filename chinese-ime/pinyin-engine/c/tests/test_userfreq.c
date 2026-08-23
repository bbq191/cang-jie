/* userfreq.c（v2 含 recency）单元测试——跟 Python 版 tests/test_userfreq.py
 * 覆盖同一批规则（衰减/触碰结算/淘汰边界/排序/v1 迁移/文件往返），行为级
 * 对拍交给 diff_check_userfreq.py。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <unistd.h>
#include "../src/userfreq.h"

static CjUserFreq g_uf; /* ~250KB，放静态区不占栈 */

#define K(s) s, strlen(s)

static void advance_by_hit(const char *key, const char *word, int n) {
    for (int i = 0; i < n; i++) {
        cj_uf_record(&g_uf, key, strlen(key), word, strlen(word));
    }
}

static void test_basic_record_count(void) {
    cj_uf_init(&g_uf);
    cj_uf_record(&g_uf, K("ni hao"), K("\xe4\xbd\xa0\xe5\xa5\xbd"));
    cj_uf_record(&g_uf, K("ni hao"), K("\xe4\xbd\xa0\xe5\xa5\xbd"));
    assert(cj_uf_count(&g_uf, K("ni hao"), K("\xe4\xbd\xa0\xe5\xa5\xbd")) == 2);
    assert(g_uf.gen == 2);
    cj_uf_record(&g_uf, K("nh"), K("\xe4\xbd\xa0\xe5\xa5\xbd"));
    assert(cj_uf_word_total(&g_uf, K("\xe4\xbd\xa0\xe5\xa5\xbd")) == 3);
    printf("test_basic_record_count ok\n");
}

static void test_oversize_ignored_gen_untouched(void) {
    cj_uf_init(&g_uf);
    char longkey[CJ_UF_KEY_MAX + 2];
    memset(longkey, 'k', sizeof(longkey));
    cj_uf_record(&g_uf, longkey, CJ_UF_KEY_MAX + 1, K("w"));
    cj_uf_record(&g_uf, K("k"), longkey, CJ_UF_WORD_MAX + 1);
    cj_uf_record(&g_uf, "", 0, K("w"));
    assert(g_uf.count == 0 && g_uf.gen == 0);
    cj_uf_record(&g_uf, longkey, CJ_UF_KEY_MAX, longkey, CJ_UF_WORD_MAX);
    assert(g_uf.count == 1 && g_uf.gen == 1);
    printf("test_oversize_ignored_gen_untouched ok\n");
}

static void test_recency_decay_and_touch(void) {
    cj_uf_init(&g_uf);
    cj_uf_record_n(&g_uf, K("shi"), K("\xe6\x98\xaf"), 8);
    assert(cj_uf_effective(&g_uf, K("shi"), K("\xe6\x98\xaf")) == 8);
    advance_by_hit("zc", "\xe9\x92\x9f", (int)CJ_UF_HALF_LIFE - 1); /* age=HALF_LIFE */
    assert(cj_uf_effective(&g_uf, K("shi"), K("\xe6\x98\xaf")) == 4);
    advance_by_hit("zc", "\xe9\x92\x9f", (int)CJ_UF_HALF_LIFE);     /* age=2*HALF_LIFE */
    assert(cj_uf_effective(&g_uf, K("shi"), K("\xe6\x98\xaf")) == 2);
    assert(cj_uf_count(&g_uf, K("shi"), K("\xe6\x98\xaf")) == 8);   /* 原始值不变 */
    /* 触碰：结算 2 + 1 = 3 */
    cj_uf_record(&g_uf, K("shi"), K("\xe6\x98\xaf"));
    assert(cj_uf_count(&g_uf, K("shi"), K("\xe6\x98\xaf")) == 3);
    assert(cj_uf_effective(&g_uf, K("shi"), K("\xe6\x98\xaf")) == 3);
    printf("test_recency_decay_and_touch ok\n");
}

static void test_new_habit_overtakes_old(void) {
    cj_uf_init(&g_uf);
    cj_uf_record_n(&g_uf, K("xing"), K("\xe8\xa1\x8c"), 3);          /* 行=旧习惯 */
    advance_by_hit("zc", "\xe9\x92\x9f", 3 * (int)CJ_UF_HALF_LIFE);  /* 3>>3=0 */
    cj_uf_record(&g_uf, K("xing"), K("\xe5\x9e\x8b"));               /* 型=新习惯 */
    const CjUfEntry *out[4];
    int n = cj_uf_words_for_key(&g_uf, K("xing"), out, 4);
    assert(n == 2);
    assert(memcmp(out[0]->word, "\xe5\x9e\x8b", 3) == 0);            /* 型在前 */
    const char *words[2] = {"\xe8\xa1\x8c", "\xe5\x9e\x8b"};
    size_t lens[2] = {3, 3};
    int order[2];
    cj_uf_order(&g_uf, K("xing"), 0, words, lens, 2, order);
    assert(order[0] == 1 && order[1] == 0);
    printf("test_new_habit_overtakes_old ok\n");
}

/* 用 load 铺满 2048 条、统一 last——绕开 record 推进时钟的不均匀衰减 */
static void fill_via_load(uint32_t count) {
    const char *path = "/tmp/cj_uf_fill.tsv";
    FILE *f = fopen(path, "wb");
    assert(f);
    fprintf(f, "#gen\t3000\n");
    for (int i = 0; i < CJ_UF_MAX_ENTRIES; i++) {
        fprintf(f, "%u\t%04x\t\xe8\xaf\x8d\t3000\n", count, i);
    }
    fclose(f);
    assert(cj_uf_load(&g_uf, path) == 1);
    remove(path);
    assert(g_uf.count == CJ_UF_MAX_ENTRIES);
    assert(g_uf.gen == 3000);
}

static void test_eviction(void) {
    fill_via_load(2);
    cj_uf_record_n(&g_uf, K("zzzz"), K("\xe6\x96\xb0"), 5);   /* 5>2 → 淘汰 */
    assert(g_uf.count == CJ_UF_MAX_ENTRIES);
    assert(cj_uf_count(&g_uf, K("zzzz"), K("\xe6\x96\xb0")) == 5);

    fill_via_load(2);
    cj_uf_record(&g_uf, K("zzzz"), K("\xe6\x96\xb0"));        /* 1<2 → 丢弃 */
    assert(cj_uf_count(&g_uf, K("zzzz"), K("\xe6\x96\xb0")) == 0);

    fill_via_load(1);
    cj_uf_record(&g_uf, K("zzzz"), K("\xe6\x96\xb0"));        /* 并列且更大 → 丢弃 */
    assert(cj_uf_count(&g_uf, K("zzzz"), K("\xe6\x96\xb0")) == 0);
    cj_uf_record(&g_uf, K("!!!!"), K("\xe6\x96\xb0"));        /* 更小 → 挤掉字节序最大 */
    assert(cj_uf_count(&g_uf, K("!!!!"), K("\xe6\x96\xb0")) == 1);
    char last4[8];
    snprintf(last4, sizeof(last4), "%04x", CJ_UF_MAX_ENTRIES - 1);
    assert(cj_uf_count(&g_uf, last4, 4, K("\xe8\xaf\x8d")) == 0);

    /* 衰老的旧条目先被淘汰 */
    fill_via_load(2);
    for (int i = 0; i < 2 * (int)CJ_UF_HALF_LIFE; i++) {
        cj_uf_record(&g_uf, K("0000"), K("\xe8\xaf\x8d"));    /* 命中推进时钟 */
    }
    cj_uf_record(&g_uf, K("aaaa"), K("\xe6\x96\xb0"));        /* 其余有效分 0<1 → 插入 */
    assert(cj_uf_count(&g_uf, K("aaaa"), K("\xe6\x96\xb0")) == 1);
    printf("test_eviction ok\n");
}

static void test_persistence_v2(void) {
    const char *path = "/tmp/cj_uf_test.tsv";
    remove(path);
    cj_uf_init(&g_uf);
    cj_uf_record_n(&g_uf, K("ni hao"), K("\xe4\xbd\xa0\xe5\xa5\xbd"), 3);
    cj_uf_record_n(&g_uf, K("shi"), K("\xe6\x98\xaf"), 7);
    assert(cj_uf_save(&g_uf, path) == 1);
    /* 首行是 #gen 头 */
    FILE *f = fopen(path, "rb");
    char head[16] = {0};
    assert(fgets(head, sizeof(head), f));
    fclose(f);
    assert(strcmp(head, "#gen\t2\n") == 0);

    CjUserFreq *uf2 = malloc(sizeof(CjUserFreq));
    assert(uf2 && cj_uf_load(uf2, path) == 1);
    assert(uf2->count == 2 && uf2->gen == 2);
    assert(cj_uf_count(uf2, K("ni hao"), K("\xe4\xbd\xa0\xe5\xa5\xbd")) == 3);
    /* 二次 save 逐字节一致 */
    const char *path2 = "/tmp/cj_uf_test2.tsv";
    assert(cj_uf_save(uf2, path2) == 1);
    FILE *a = fopen(path, "rb"), *b = fopen(path2, "rb");
    char ba[4096], bb[4096];
    size_t na = fread(ba, 1, sizeof(ba), a), nb = fread(bb, 1, sizeof(bb), b);
    fclose(a); fclose(b);
    assert(na == nb && memcmp(ba, bb, na) == 0);
    free(uf2);
    remove(path); remove(path2);
    printf("test_persistence_v2 ok\n");
}

static void test_load_v1_migration_and_malformed(void) {
    const char *path = "/tmp/cj_uf_test3.tsv";
    FILE *f = fopen(path, "wb");
    assert(f);
    /* v1 三字段 → last=0 无损迁移 */
    fputs("3\tni hao\t\xe4\xbd\xa0\xe5\xa5\xbd\n", f);
    fputs("4\tshi\t\xe6\x98\xaf\n", f);
    fclose(f);
    CjUserFreq *uf2 = malloc(sizeof(CjUserFreq));
    assert(uf2 && cj_uf_load(uf2, path) == 1);
    assert(uf2->count == 2 && uf2->gen == 0);
    assert(cj_uf_effective(uf2, K("ni hao"), K("\xe4\xbd\xa0\xe5\xa5\xbd")) == 3);

    /* 坏行/头部/重复行 */
    f = fopen(path, "wb");
    assert(f);
    fputs("#gen\t50\n", f);
    fputs("#comment junk\n", f);
    fputs("3\ta\tw\t5\n", f);
    fputs("2\ta\tw\t9\n", f);          /* 重复：3+2=5, last=9 */
    fputs("not\tk\tw\t1\n", f);
    fputs("5\tk\tw\tbadlast\n", f);
    fputs("0\tk\tw\t1\n", f);
    fputs("2\tk\tw\t1\textra\n", f);
    fputs("4\tshi\t\xe6\x98\xaf\t60\n", f); /* last>header → gen=60 */
    fclose(f);
    assert(cj_uf_load(uf2, path) == 1);
    assert(uf2->count == 2);
    assert(uf2->gen == 60);
    assert(cj_uf_count(uf2, K("a"), K("w")) == 5);
    assert(cj_uf_count(uf2, K("shi"), K("\xe6\x98\xaf")) == 4);
    free(uf2);
    remove(path);
    printf("test_load_v1_migration_and_malformed ok\n");
}

int main(void) {
    test_basic_record_count();
    test_oversize_ignored_gen_untouched();
    test_recency_decay_and_touch();
    test_new_habit_overtakes_old();
    test_eviction();
    test_persistence_v2();
    test_load_v1_migration_and_malformed();
    printf("all userfreq v2 tests passed\n");
    return 0;
}
