#include "syllables.h"
#include "syllables_data.h"

#include <string.h>

/* CJ_SYLLABLES 是按字典序排好的静态表（syllables_data.h 里 Python 生成时
 * 用 sorted() 排的序，跟这里的比较函数用同一种字节序，不需要额外排序）。
 * 精确匹配用二分查找——429 条线性扫也不算慢，但既然表已经排好序了，
 * 二分查找几乎不增加代码复杂度，顺手做了。 */
static int cj_cmp_syllable(const char *s, size_t len, const char *candidate) {
    size_t cand_len = strlen(candidate);
    size_t min_len = len < cand_len ? len : cand_len;
    int cmp = memcmp(s, candidate, min_len);
    if (cmp != 0) {
        return cmp;
    }
    if (len == cand_len) {
        return 0;
    }
    return (len < cand_len) ? -1 : 1;
}

int cj_is_valid_syllable(const char *s, size_t len) {
    if (len == 0 || len > CJ_MAX_SYLLABLE_LEN) {
        return 0;
    }
    int lo = 0, hi = CJ_SYLLABLE_COUNT - 1;
    while (lo <= hi) {
        int mid = lo + (hi - lo) / 2;
        int cmp = cj_cmp_syllable(s, len, CJ_SYLLABLES[mid]);
        if (cmp == 0) {
            return 1;
        }
        if (cmp < 0) {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    return 0;
}

/* 前缀检查——跟 Python 版 is_prefix_of_some_syllable 一样是线性扫全表
 * （429 条，交互式打字延迟完全够用，不做二分查找范围优化，优先保证跟
 * Python 版逻辑一一对应、行为一致，好核对）。 */
int cj_is_prefix_of_some_syllable(const char *s, size_t len) {
    if (len == 0) {
        return 0;
    }
    for (int i = 0; i < CJ_SYLLABLE_COUNT; i++) {
        const char *cand = CJ_SYLLABLES[i];
        size_t cand_len = strlen(cand);
        if (cand_len >= len && memcmp(s, cand, len) == 0) {
            return 1;
        }
    }
    return 0;
}
