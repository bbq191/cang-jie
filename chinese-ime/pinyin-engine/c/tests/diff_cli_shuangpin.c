/* 差分测试用的小工具：每行读一串双拼按键（纯小写字母），打印
 * cj_shuangpin_keys_to_pinyin 的解码结果，方便跟 Python 参照实现的
 * 输出逐行 diff。不是正式单元测试，只是一次性核对用的辅助程序，见
 * tests/diff_check_shuangpin.py。跟 diff_cli_jianpin.c 是同一个模式，
 * 这里更简单——不需要打开任何 mmap 文件，纯查表函数。 */
#include <stdio.h>
#include <string.h>
#include "../src/shuangpin.h"

#define MAX_LINE 256
#define MAX_OUT 256

int main(void) {
    char line[MAX_LINE];
    while (fgets(line, sizeof(line), stdin)) {
        size_t len = strcspn(line, "\n");
        line[len] = '\0';
        char out[MAX_OUT];
        cj_shuangpin_keys_to_pinyin(line, len, out, sizeof(out));
        printf("%s\n", out);
    }
    return 0;
}
