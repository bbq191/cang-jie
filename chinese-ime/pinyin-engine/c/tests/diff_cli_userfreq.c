/* userfreq 差分测试驱动——每行一条操作、每行一条输出，跟 Python 参照
 * (src/userfreq.py) 在 tests/diff_check_userfreq.py 里逐行比对。字段用
 * TAB 分隔（key 里有空格）。操作：
 *   R\tkey\tword            record，输出 "ok"
 *   N\tn\tkey\tword         record_n，输出 "ok"
 *   Q\tkey\tword            输出 count（原始存储值）
 *   E\tkey\tword            输出 effective（v2 有效分）
 *   T\tword                 输出 word_total
 *   W\tmax\tkey             输出 word:count|word:count|...（空则空行）
 *   O\tmode\tkey\tw1,w2,..  mode=u(word_total)/k(精确key)，输出 i0,i1,...
 *   D                       输出整表 dump，行间用 | 连接（空表空行）
 * 不认识的操作输出 "?"（两边都不该出现，出现即 diff 失败）。 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../src/userfreq.h"

#define MAX_LINE 4096
#define MAX_WORDS 256

static CjUserFreq g_uf;

/* 就地把 line 按 \t 拆成最多 max 段，返回段数。 */
static int split_tabs(char *line, char **fields, int max) {
    int n = 0;
    char *p = line;
    while (n < max) {
        fields[n++] = p;
        char *t = strchr(p, '\t');
        if (!t) {
            break;
        }
        *t = '\0';
        p = t + 1;
    }
    return n;
}

int main(void) {
    cj_uf_init(&g_uf);
    char line[MAX_LINE];
    while (fgets(line, sizeof(line), stdin)) {
        line[strcspn(line, "\n")] = '\0';
        char *f[4];
        int nf = split_tabs(line, f, 4);
        if (nf == 0 || f[0][0] == '\0') {
            printf("?\n");
            continue;
        }
        char op = f[0][0];
        if (op == 'R' && nf == 3) {
            cj_uf_record(&g_uf, f[1], strlen(f[1]), f[2], strlen(f[2]));
            printf("ok\n");
        } else if (op == 'N' && nf == 4) {
            cj_uf_record_n(&g_uf, f[2], strlen(f[2]), f[3], strlen(f[3]),
                           (uint32_t)strtoul(f[1], NULL, 10));
            printf("ok\n");
        } else if (op == 'Q' && nf == 3) {
            printf("%u\n", cj_uf_count(&g_uf, f[1], strlen(f[1]), f[2], strlen(f[2])));
        } else if (op == 'E' && nf == 3) {
            printf("%u\n", cj_uf_effective(&g_uf, f[1], strlen(f[1]), f[2], strlen(f[2])));
        } else if (op == 'T' && nf == 2) {
            printf("%u\n", cj_uf_word_total(&g_uf, f[1], strlen(f[1])));
        } else if (op == 'W' && nf == 3) {
            const CjUfEntry *out[CJ_UF_MAX_ENTRIES];
            int max_out = (int)strtol(f[1], NULL, 10);
            if (max_out > CJ_UF_MAX_ENTRIES) {
                max_out = CJ_UF_MAX_ENTRIES;
            }
            int n = cj_uf_words_for_key(&g_uf, f[2], strlen(f[2]), out, max_out);
            for (int i = 0; i < n; i++) {
                if (i) putchar('|');
                fwrite(out[i]->word, 1, out[i]->word_len, stdout);
                printf(":%u", out[i]->count);
            }
            putchar('\n');
        } else if (op == 'O' && nf == 4) {
            const char *words[MAX_WORDS];
            size_t lens[MAX_WORDS];
            int order[MAX_WORDS];
            int n = 0;
            char *p = f[3];
            while (p && *p && n < MAX_WORDS) {
                char *c = strchr(p, ',');
                if (c) *c = '\0';
                words[n] = p;
                lens[n] = strlen(p);
                n++;
                p = c ? c + 1 : NULL;
            }
            cj_uf_order(&g_uf, f[2], strlen(f[2]), f[1][0] == 'u',
                        words, lens, n, order);
            for (int i = 0; i < n; i++) {
                printf("%s%d", i ? "," : "", order[i]);
            }
            putchar('\n');
        } else if (op == 'D' && nf == 1) {
            printf("#gen\t%u", g_uf.gen);
            for (int i = 0; i < g_uf.count; i++) {
                const CjUfEntry *e = &g_uf.entries[i];
                putchar('|');
                printf("%u\t", e->count);
                fwrite(e->key, 1, e->key_len, stdout);
                putchar('\t');
                fwrite(e->word, 1, e->word_len, stdout);
                printf("\t%u", e->last);
            }
            putchar('\n');
        } else {
            printf("?\n");
        }
    }
    return 0;
}
