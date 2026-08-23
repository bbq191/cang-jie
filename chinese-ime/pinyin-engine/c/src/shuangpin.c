#include "shuangpin.h"
#include "shuangpin_data.h"

/* 追加 len 字节到 out（在 out_cap 内不越界写），同时把 *needed 累加成
 * "不管有没有被截断，应该写入的完整长度"——跟 snprintf 的返回值语义
 * 对照，调用方通过比较返回值和 out_cap 判断是否发生了截断。 */
static void cj_shuangpin_append(char *out, size_t out_cap, size_t *pos, size_t *needed,
                                 const char *data, size_t len) {
    *needed += len;
    for (size_t i = 0; i < len && *pos < out_cap; i++) {
        out[(*pos)++] = data[i];
    }
}

static size_t cj_cstrlen(const char *s) {
    size_t n = 0;
    while (s[n] != '\0') n++;
    return n;
}

size_t cj_shuangpin_keys_to_pinyin(const char *keys, size_t keys_len, char *out, size_t out_cap) {
    if (out && out_cap > 0) {
        out[0] = '\0';
    }
    if (!keys || keys_len == 0 || !out) {
        return 0;
    }

    size_t pos = 0;
    size_t needed = 0;
    size_t full_pairs = keys_len / 2;
    int has_pending = (keys_len % 2) != 0;

    for (size_t i = 0; i < full_pairs; i++) {
        char c1 = keys[i * 2];
        char c2 = keys[i * 2 + 1];
        if (i > 0) {
            cj_shuangpin_append(out, out_cap, &pos, &needed, "'", 1);
        }
        if (c1 >= 'a' && c1 <= 'z' && c2 >= 'a' && c2 <= 'z') {
            int idx = (c1 - 'a') * 26 + (c2 - 'a');
            const char *decoded = CJ_SHUANGPIN_FLYPY_TABLE[idx];
            cj_shuangpin_append(out, out_cap, &pos, &needed, decoded, cj_cstrlen(decoded));
        } else {
            /* 防御性兜底：调用方保证纯小写字母（跟 hook_init.c Step T
             * 缓冲区的入口校验一致），理论上不会走到这里；万一出现
             * 非法字符，不查表越界，原样透传这两个字符，不崩溃。 */
            cj_shuangpin_append(out, out_cap, &pos, &needed, &c1, 1);
            cj_shuangpin_append(out, out_cap, &pos, &needed, &c2, 1);
        }
    }

    if (has_pending) {
        if (full_pairs > 0) {
            cj_shuangpin_append(out, out_cap, &pos, &needed, "'", 1);
        }
        char pend = keys[keys_len - 1];
        cj_shuangpin_append(out, out_cap, &pos, &needed, &pend, 1);
    }

    size_t nul_pos = pos < out_cap ? pos : (out_cap > 0 ? out_cap - 1 : 0);
    if (out_cap > 0) {
        out[nul_pos] = '\0';
    }
    return needed;
}
