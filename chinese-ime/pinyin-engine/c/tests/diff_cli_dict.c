/* 差分测试用的小工具：每行读一条查询，打印 cj_dict_lookup/
 * cj_dict_lookup_prefix 的结果，格式方便跟 Python 参照实现的输出逐行
 * diff。不是正式单元测试，只是一次性核对用的辅助程序，见
 * tests/diff_check_dict.py。
 *
 * 输入行格式：
 *   L ni hao        精确匹配，空格分隔完整音节序列
 *   P ni h          前缀匹配，最后一个片段是"打到一半"的部分音节
 * 输出行格式：word1:weight1|word2:weight2|...（无命中输出空行）。 */
#include <stdio.h>
#include <string.h>
#include "../src/dictionary.h"

#define MAX_LINE 512
#define MAX_OUT 1024   /* 加入 41448 大字表后单个 key 最多 823 个候选（manifest 里的
                        * max_cands_per_key，多音字比如 "shi" 汇集了大字表里所有同音
                        * 生僻字），1024 留足余量——这是差分测试工具本身的缓冲区，
                        * 不代表 cj_dict_lookup() 的 max_out 有什么固定假设，调用方
                        * 按需传更小的值一样正确（截断按权重排序取前 max_out 个）。 */
#define PREFIX_LIMIT 50 /* 跟 Python 版 Dictionary.lookup_prefix 的默认 limit 对齐 */

int main(int argc, char **argv) {
    const char *dict_path = argc > 1 ? argv[1] : "build/dict.bin";
    CjDictHandle h;
    if (!cj_dict_open(dict_path, &h)) {
        fprintf(stderr, "open %s failed\n", dict_path);
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

        CjDictCandidate out[MAX_OUT];
        int n = 0;
        if (mode == 'L') {
            n = cj_dict_lookup(&h, rest, rest_len, out, MAX_OUT);
        } else if (mode == 'P') {
            n = cj_dict_lookup_prefix(&h, rest, rest_len, out, PREFIX_LIMIT);
        }
        for (int i = 0; i < n; i++) {
            if (i) putchar('|');
            fwrite(out[i].word, 1, out[i].word_len, stdout);
            printf(":%u", out[i].weight);
        }
        putchar('\n');
    }

    cj_dict_close(&h);
    return 0;
}
