/* snippets 差分测试驱动——协议（TAB 分隔，每行一条输出）：
 *   P\t<base64内容？不用——内容经 \t/\n 转义>   太绕，改为：
 *   第一行 "C <行数>"，随后 <行数> 行是 snippets 文件原始内容（喂给
 *   cj_sn_parse），输出 "loaded <count>"；
 *   之后每行 "L\t<buffer>" → 输出 phrase1|phrase2|...（空则空行）。
 * 见 tests/diff_check_snippets.py。 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../src/snippets.h"

#define MAX_LINE 4096

static CjSnippets g_sn;

int main(void) {
    char line[MAX_LINE];
    /* 攒 parse 内容 */
    static char content[256 * 1024];
    size_t content_len = 0;

    if (!fgets(line, sizeof(line), stdin)) {
        return 1;
    }
    int nlines = 0;
    if (sscanf(line, "C %d", &nlines) != 1) {
        fprintf(stderr, "首行必须是 'C <行数>'\n");
        return 1;
    }
    for (int i = 0; i < nlines; i++) {
        if (!fgets(line, sizeof(line), stdin)) {
            break;
        }
        size_t l = strlen(line);
        if (content_len + l < sizeof(content)) {
            memcpy(content + content_len, line, l);
            content_len += l;
        }
    }
    cj_sn_init(&g_sn);
    cj_sn_parse(&g_sn, content, content_len);
    printf("loaded %d\n", g_sn.count);

    while (fgets(line, sizeof(line), stdin)) {
        line[strcspn(line, "\n")] = '\0';
        if (line[0] != 'L' || line[1] != '\t') {
            printf("?\n");
            continue;
        }
        const char *buf = line + 2;
        const CjSnEntry *out[CJ_SN_MAX_ENTRIES];
        int n = cj_sn_lookup(&g_sn, buf, strlen(buf), out, CJ_SN_MAX_ENTRIES);
        for (int i = 0; i < n; i++) {
            if (i) putchar('|');
            fwrite(out[i]->phrase, 1, out[i]->phrase_len, stdout);
        }
        putchar('\n');
    }
    return 0;
}
