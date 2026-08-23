/* 差分测试用的小工具：每行读一条查询，打印 cj_jianpin_lookup/
 * cj_jianpin_lookup_prefix 的结果，格式方便跟 Python 参照实现的输出
 * 逐行 diff。不是正式单元测试，只是一次性核对用的辅助程序，见
 * tests/diff_check_jianpin.py。跟 diff_cli_dict.c 是同一个模式。
 *
 * 输入行格式：
 *   L zg            精确匹配
 *   P py            前缀匹配
 * 输出行格式：word1:weight1|word2:weight2|...（无命中输出空行）。 */
#include <stdio.h>
#include <string.h>
#include "../src/jianpin.h"

#define MAX_LINE 512
#define MAX_OUT 2048   /* manifest 里 max_cands_per_key 上千（"z" 这种覆盖
                        * zh 声母的简拼字母命中候选特别多），留足余量——这是
                        * 差分测试工具本身的缓冲区，不代表 cj_jianpin_lookup*()
                        * 的 max_out 有什么固定假设。 */
#define PREFIX_LIMIT 50 /* 跟 Python 版 JianpinIndex.lookup_prefix 的默认 limit 对齐 */

int main(int argc, char **argv) {
    const char *path = argc > 1 ? argv[1] : "build/dict_jianpin.bin";
    CjJianpinHandle h;
    if (!cj_jianpin_open(path, &h)) {
        fprintf(stderr, "open %s failed\n", path);
        return 1;
    }

    char line[MAX_LINE];
    while (fgets(line, sizeof(line), stdin)) {
        size_t len = strcspn(line, "\n");
        line[len] = '\0';
        if (len == 0) {
            putchar('\n');
            continue;
        }
        char mode = line[0];
        const char *rest = line + 1;
        while (*rest == ' ') rest++;
        size_t rest_len = strlen(rest);

        CjJianpinCandidate out[MAX_OUT];
        int n = 0;
        if (mode == 'L') {
            n = cj_jianpin_lookup(&h, rest, rest_len, out, MAX_OUT);
        } else if (mode == 'P') {
            n = cj_jianpin_lookup_prefix(&h, rest, rest_len, out, PREFIX_LIMIT);
        }
        for (int i = 0; i < n; i++) {
            if (i) putchar('|');
            fwrite(out[i].word, 1, out[i].word_len, stdout);
            printf(":%u", out[i].weight);
        }
        putchar('\n');
    }

    cj_jianpin_close(&h);
    return 0;
}
