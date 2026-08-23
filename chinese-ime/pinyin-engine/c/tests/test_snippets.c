/* snippets.c 单元测试——覆盖跟 Python 版 tests/test_snippets.py 同一批
 * 规则 + C 独有的 mtime 热重载。 */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <utime.h>
#include "../src/snippets.h"

static CjSnippets g_sn;

static void parse_str(const char *s) { cj_sn_parse(&g_sn, s, strlen(s)); }

static void test_basic(void) {
    cj_sn_init(&g_sn);
    parse_str("dz\t\xe5\x8c\x97\xe4\xba\xac\xe5\xb8\x82\nomw\tOn my way!\n");
    const CjSnEntry *out[8];
    assert(cj_sn_lookup(&g_sn, "dz", 2, out, 8) == 1);
    assert(out[0]->phrase_len == 9 && memcmp(out[0]->phrase, "\xe5\x8c\x97\xe4\xba\xac\xe5\xb8\x82", 9) == 0);
    assert(cj_sn_lookup(&g_sn, "omw", 3, out, 8) == 1);
    assert(cj_sn_lookup(&g_sn, "xx", 2, out, 8) == 0);
    assert(cj_sn_lookup(&g_sn, "d", 1, out, 8) == 0); /* 前缀不触发 */
    printf("test_basic ok\n");
}

static void test_duplicates_and_order(void) {
    cj_sn_init(&g_sn);
    parse_str("vx\twxid_abc\nvx\t13800138000\nvx\twxid_abc\n");
    const CjSnEntry *out[8];
    int n = cj_sn_lookup(&g_sn, "vx", 2, out, 8);
    assert(n == 3);
    assert(memcmp(out[0]->phrase, "wxid_abc", 8) == 0);
    assert(memcmp(out[1]->phrase, "13800138000", 11) == 0);
    printf("test_duplicates_and_order ok\n");
}

static void test_invalid_lines(void) {
    cj_sn_init(&g_sn);
    char big[1024];
    int off = 0;
    off += snprintf(big + off, sizeof(big) - (size_t)off,
                    "ok\tgood\nUP\tbad\na1\tbad\n\tno\nnoph\t\ntwotab\ta\tb\nnotab\n");
    /* 超长缩写 */
    for (int i = 0; i < CJ_SN_SHORTCUT_MAX + 1; i++) big[off++] = 'x';
    off += snprintf(big + off, sizeof(big) - (size_t)off, "\ttoolong\n\nokk\tgood2\n");
    cj_sn_parse(&g_sn, big, (size_t)off);
    assert(g_sn.count == 2);
    const CjSnEntry *out[4];
    assert(cj_sn_lookup(&g_sn, "ok", 2, out, 4) == 1);
    assert(cj_sn_lookup(&g_sn, "okk", 3, out, 4) == 1);
    printf("test_invalid_lines ok\n");
}

static void test_capacity(void) {
    cj_sn_init(&g_sn);
    static char buf[16 * 1024];
    int off = 0;
    for (int i = 0; i < CJ_SN_MAX_ENTRIES + 50; i++) {
        off += snprintf(buf + off, sizeof(buf) - (size_t)off, "a\tp%d\n", i);
    }
    cj_sn_parse(&g_sn, buf, (size_t)off);
    assert(g_sn.count == CJ_SN_MAX_ENTRIES);
    const CjSnEntry *out[CJ_SN_MAX_ENTRIES];
    int n = cj_sn_lookup(&g_sn, "a", 1, out, CJ_SN_MAX_ENTRIES);
    assert(n == CJ_SN_MAX_ENTRIES);
    assert(memcmp(out[0]->phrase, "p0", 2) == 0);
    printf("test_capacity ok\n");
}

static void test_hot_reload(void) {
    const char *path = "/tmp/cj_sn_test.tsv";
    remove(path);
    cj_sn_init(&g_sn);
    /* 文件不存在 → 空表 */
    cj_sn_refresh(&g_sn, path);
    assert(g_sn.count == 0);
    /* 写入 → refresh 拿到 */
    FILE *f = fopen(path, "wb");
    fputs("ab\tfirst\n", f);
    fclose(f);
    cj_sn_refresh(&g_sn, path);
    assert(g_sn.count == 1);
    /* 内容变化（mtime/size 变）→ 重载 */
    f = fopen(path, "wb");
    fputs("ab\tfirst\ncd\tsecond\n", f);
    fclose(f);
    struct utimbuf tb = {1000000000, 1000000001}; /* 强制 mtime 变化，绕开同秒问题 */
    utime(path, &tb);
    cj_sn_refresh(&g_sn, path);
    assert(g_sn.count == 2);
    /* 没变 → 不动（count 保持） */
    cj_sn_refresh(&g_sn, path);
    assert(g_sn.count == 2);
    /* 删除 → 空表 */
    remove(path);
    cj_sn_refresh(&g_sn, path);
    assert(g_sn.count == 0);
    printf("test_hot_reload ok\n");
}

int main(void) {
    test_basic();
    test_duplicates_and_order();
    test_invalid_lines();
    test_capacity();
    test_hot_reload();
    printf("all snippets tests passed\n");
    return 0;
}
