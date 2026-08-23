/* 快捷输入 C 实现——parse/lookup 行为逐条对齐 src/snippets.py（规则见
 * 那边 docstring），差分测试 tests/diff_check_snippets.py 逐字节比对；
 * mtime 热重载是 C 独有的 I/O 策略（Python 参照不涉及）。 */
#include "snippets.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static int sn_valid_shortcut(const char *s, size_t len) {
    if (len == 0 || len > CJ_SN_SHORTCUT_MAX) {
        return 0;
    }
    for (size_t i = 0; i < len; i++) {
        if (s[i] < 'a' || s[i] > 'z') {
            return 0;
        }
    }
    return 1;
}

void cj_sn_init(CjSnippets *sn) {
    sn->count = 0;
    sn->loaded = 0;
    sn->mtime = 0;
    sn->size = 0;
}

void cj_sn_parse(CjSnippets *sn, const char *data, size_t data_len) {
    sn->count = 0;
    size_t pos = 0;
    while (pos < data_len && sn->count < CJ_SN_MAX_ENTRIES) {
        const char *nl = memchr(data + pos, '\n', data_len - pos);
        size_t line_len = nl ? (size_t)(nl - (data + pos)) : data_len - pos;
        const char *line = data + pos;
        pos += line_len + (nl ? 1 : 0);
        if (line_len == 0) {
            continue;
        }
        /* 拆 "shortcut\tphrase"：必须恰好 2 段。 */
        const char *t1 = memchr(line, '\t', line_len);
        if (!t1) {
            continue;
        }
        size_t sc_len = (size_t)(t1 - line);
        size_t ph_len = line_len - sc_len - 1;
        if (memchr(t1 + 1, '\t', ph_len)) {
            continue; /* 第 3 段 → 格式错整行跳过 */
        }
        if (!sn_valid_shortcut(line, sc_len)) {
            continue;
        }
        if (ph_len == 0 || ph_len > CJ_SN_PHRASE_MAX) {
            continue;
        }
        CjSnEntry *e = &sn->entries[sn->count++];
        memcpy(e->shortcut, line, sc_len);
        memcpy(e->phrase, t1 + 1, ph_len);
        e->shortcut_len = (uint8_t)sc_len;
        e->phrase_len = (uint16_t)ph_len;
    }
}

void cj_sn_refresh(CjSnippets *sn, const char *path) {
    struct stat st;
    if (stat(path, &st) != 0) {
        /* 文件不存在/不可 stat：清空并记"已加载空态"，文件出现后 mtime
         * 对不上会触发真加载。 */
        sn->count = 0;
        sn->loaded = 1;
        sn->mtime = -1;
        sn->size = -1;
        return;
    }
    if (sn->loaded && sn->mtime == (long long)st.st_mtime &&
        sn->size == (long long)st.st_size) {
        return; /* 没变，一次 stat 的开销即返回 */
    }
    sn->count = 0;
    sn->loaded = 1;
    sn->mtime = (long long)st.st_mtime;
    sn->size = (long long)st.st_size;

    FILE *f = fopen(path, "rb");
    if (!f) {
        return; /* fail-safe：空表 */
    }
    /* 上限：256 条 × ~300B ≈ 77KB；给 512KB 缓冲，超出部分行自然被
     * 容量上限截住。 */
    enum { SN_LOAD_BUF = 512 * 1024 };
    char *buf = malloc(SN_LOAD_BUF);
    if (!buf) {
        fclose(f);
        return;
    }
    size_t got = fread(buf, 1, SN_LOAD_BUF, f);
    fclose(f);
    cj_sn_parse(sn, buf, got);
    free(buf);
}

int cj_sn_lookup(const CjSnippets *sn, const char *buffer, size_t buffer_len,
                 const CjSnEntry **out, int max_out) {
    if (max_out <= 0 || buffer_len == 0 || buffer_len > CJ_SN_SHORTCUT_MAX) {
        return 0;
    }
    int n = 0;
    for (int i = 0; i < sn->count && n < max_out; i++) {
        const CjSnEntry *e = &sn->entries[i];
        if (e->shortcut_len == buffer_len &&
            memcmp(e->shortcut, buffer, buffer_len) == 0) {
            out[n++] = e;
        }
    }
    return n;
}
