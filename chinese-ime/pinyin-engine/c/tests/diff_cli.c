/* 差分测试用的小工具：读一行输入，打印 cj_segment() 的全部候选切分，
 * 格式方便跟 Python 版脚本的输出逐行 diff。不是正式单元测试，只是
 * 一次性核对用的辅助程序，见 tests/diff_check.py。 */
#include <stdio.h>
#include <string.h>
#include "../src/segment.h"

int main(void) {
    char raw[256];
    while (fgets(raw, sizeof(raw), stdin)) {
        size_t len = strcspn(raw, "\n");
        raw[len] = '\0';

        char buf[CJ_SEG_MAX_INPUT];
        CjSegmentation candidates[CJ_SEG_MAX_CANDIDATES];
        int count = 0;
        int ok = cj_segment(raw, len, buf, candidates, CJ_SEG_MAX_CANDIDATES, &count);
        if (!ok) {
            printf("REJECTED\n");
            continue;
        }
        for (int i = 0; i < count; i++) {
            CjSegmentation *seg = &candidates[i];
            for (int k = 0; k < seg->syllable_count; k++) {
                if (k > 0) putchar('+');
                fwrite(buf + seg->syllables[k].offset, 1, seg->syllables[k].len, stdout);
            }
            if (seg->has_pending) {
                if (seg->syllable_count > 0) putchar('+');
                putchar('[');
                fwrite(buf + seg->pending.offset, 1, seg->pending.len, stdout);
                putchar(']');
            }
            putchar(i == count - 1 ? '\n' : '|');
        }
    }
    return 0;
}
