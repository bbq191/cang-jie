#include "segment.h"
#include "syllables.h"
#include "syllables_data.h" /* CJ_MAX_SYLLABLE_LEN */

#include <ctype.h>
#include <string.h>

/* 递归枚举全部合法切分时的工作量预算——防御性上限，堵住"长按同一个
 * 字母"这类病态输入（比如连续一串 "a"，每个 "a" 单独都是合法音节，
 * 组合数会指数级增长）可能触发的卡死。触顶就提前收工，返回目前已经
 * 攒到的结果（可能不完整，但不会崩溃/挂起）——这是 C 移植版对 Python
 * lru_cache 版本天然有界特性的替代方案，见 segment.h 顶部说明。
 *
 * 用模块内 static 变量而不是每次调用分配/传参，是因为这个函数只会在
 * 单次按键事件处理过程中同步递归调用（Qt 输入事件处理是单线程的，
 * 不存在并发重入），每次 cj_segment() 顶层调用开始时重置一次即可。 */
static long g_enum_work_budget;
#define CJ_SEG_ENUM_WORK_BUDGET_INIT 4000

typedef struct {
    CjSyllableSpan spans[CJ_SEG_MAX_SYLLABLES];
    int count;
} CjSpanList;

typedef struct {
    CjSpanList results[CJ_SEG_MAX_CANDIDATES];
    int count;
} CjFullEnum;

/* DP 求 base[offset..offset+len) 的"最少音节数"完整切分（等价于 Python
 * 版 min(chunk_segs, key=_rank_key)，但不需要先枚举全部候选，O(len *
 * CJ_MAX_SYLLABLE_LEN) 直接算出最优解，不会指数爆炸，不需要工作量预算
 * 保护）。返回 1 成功、0 表示整个区间切不出合法音节组合。 */
static int dp_best_segmentation(const char *base, size_t offset, size_t len, CjSpanList *out) {
    if (len == 0) {
        out->count = 0;
        return 1;
    }
    if (len > CJ_SEG_MAX_INPUT) {
        return 0;
    }

    struct {
        int reachable;
        CjSpanList spans;
    } dp[CJ_SEG_MAX_INPUT + 1];

    dp[0].reachable = 1;
    dp[0].spans.count = 0;
    for (size_t i = 1; i <= len; i++) {
        dp[i].reachable = 0;
        size_t max_cut = (i < (size_t)CJ_MAX_SYLLABLE_LEN) ? i : (size_t)CJ_MAX_SYLLABLE_LEN;
        for (size_t cut = 1; cut <= max_cut; cut++) {
            size_t prev = i - cut;
            if (!dp[prev].reachable) {
                continue;
            }
            if (!cj_is_valid_syllable(base + offset + prev, cut)) {
                continue;
            }
            int new_count = dp[prev].spans.count + 1;
            if (new_count > CJ_SEG_MAX_SYLLABLES) {
                continue;
            }
            if (!dp[i].reachable || new_count < dp[i].spans.count) {
                dp[i].reachable = 1;
                dp[i].spans = dp[prev].spans;
                dp[i].spans.spans[dp[i].spans.count].offset = offset + prev;
                dp[i].spans.spans[dp[i].spans.count].len = cut;
                dp[i].spans.count++;
            }
        }
    }

    if (!dp[len].reachable) {
        return 0;
    }
    *out = dp[len].spans;
    return 1;
}

/* 递归枚举 base[offset..offset+len) 的全部合法切分（不只是最优解）——
 * 只在最后一段（可能暴露多个候选给上层排序）用，前面已确定的 chunk
 * 只需要 dp_best_segmentation 那一个最优解就够了。结果数超过
 * CJ_SEG_MAX_CANDIDATES 或工作量预算耗尽时提前停止累加。 */
static void enumerate_full(const char *base, size_t offset, size_t len, CjFullEnum *acc) {
    if (g_enum_work_budget <= 0 || acc->count >= CJ_SEG_MAX_CANDIDATES) {
        return;
    }
    g_enum_work_budget--;

    if (len == 0) {
        acc->results[acc->count].count = 0;
        acc->count++;
        return;
    }

    int max_cut = (int)((len < (size_t)CJ_MAX_SYLLABLE_LEN) ? len : (size_t)CJ_MAX_SYLLABLE_LEN);
    for (int cut = max_cut; cut >= 1; cut--) {
        if (g_enum_work_budget <= 0 || acc->count >= CJ_SEG_MAX_CANDIDATES) {
            return;
        }
        if (!cj_is_valid_syllable(base + offset, (size_t)cut)) {
            continue;
        }
        CjFullEnum tail;
        tail.count = 0;
        enumerate_full(base, offset + (size_t)cut, len - (size_t)cut, &tail);
        for (int i = 0; i < tail.count && acc->count < CJ_SEG_MAX_CANDIDATES; i++) {
            int total = 1 + tail.results[i].count;
            if (total > CJ_SEG_MAX_SYLLABLES) {
                continue;
            }
            CjSpanList *dst = &acc->results[acc->count];
            dst->spans[0].offset = offset;
            dst->spans[0].len = (size_t)cut;
            memcpy(&dst->spans[1], tail.results[i].spans, sizeof(CjSyllableSpan) * (size_t)tail.results[i].count);
            dst->count = total;
            acc->count++;
        }
    }
}

/* 输入切分成 apostrophe 分隔的若干段，跳过空段（对应 Python 的
 * `[seg for seg in raw.split("'") if seg != ""]`）。返回段数，最多
 * CJ_SEG_MAX_CHUNKS 段（正常打字不会用到这么多隔音符号，超出部分
 * 静默丢弃——防御性上限，不是预期路径）。 */
#define CJ_SEG_MAX_CHUNKS 8
typedef struct {
    size_t offset;
    size_t len;
} CjChunk;

static int split_on_apostrophe(const char *s, size_t len, CjChunk *out) {
    int n = 0;
    size_t start = 0;
    for (size_t i = 0; i <= len; i++) {
        int at_boundary = (i == len) || (s[i] == '\'');
        if (at_boundary) {
            if (i > start && n < CJ_SEG_MAX_CHUNKS) {
                out[n].offset = start;
                out[n].len = i - start;
                n++;
            }
            start = i + 1;
        }
    }
    return n;
}

int cj_segment(const char *raw, size_t raw_len,
               char *lowercased_out,
               CjSegmentation *out_candidates, int max_candidates,
               int *out_count) {
    *out_count = 0;
    if (raw_len > CJ_SEG_MAX_INPUT) {
        return 0;
    }

    for (size_t i = 0; i < raw_len; i++) {
        lowercased_out[i] = (char)tolower((unsigned char)raw[i]);
    }

    if (raw_len == 0) {
        if (max_candidates < 1) return 0;
        out_candidates[0].syllable_count = 0;
        out_candidates[0].has_pending = 0;
        *out_count = 1;
        return 1;
    }

    CjChunk chunks[CJ_SEG_MAX_CHUNKS];
    int n_chunks = split_on_apostrophe(lowercased_out, raw_len, chunks);
    if (n_chunks == 0) {
        /* 全是 apostrophe 或空输入的边界情况，当成一个空的 last chunk 处理 */
        n_chunks = 1;
        chunks[0].offset = 0;
        chunks[0].len = 0;
    }

    /* 前面确定的 chunk（除最后一个）各自取 DP 最优解，切不出来就整段
     * 原样当一个（不合法但保留原文）的 span 塞进去，对应 Python 版
     * "chunk_segs 为空时原样当整体塞入" 的兜底。 */
    CjSpanList fixed_prefix;
    fixed_prefix.count = 0;
    for (int c = 0; c < n_chunks - 1; c++) {
        CjSpanList best;
        if (dp_best_segmentation(lowercased_out, chunks[c].offset, chunks[c].len, &best)) {
            for (int i = 0; i < best.count && fixed_prefix.count < CJ_SEG_MAX_SYLLABLES; i++) {
                fixed_prefix.spans[fixed_prefix.count++] = best.spans[i];
            }
        } else if (fixed_prefix.count < CJ_SEG_MAX_SYLLABLES) {
            fixed_prefix.spans[fixed_prefix.count].offset = chunks[c].offset;
            fixed_prefix.spans[fixed_prefix.count].len = chunks[c].len;
            fixed_prefix.count++;
        }
    }

    CjChunk last = chunks[n_chunks - 1];

    g_enum_work_budget = CJ_SEG_ENUM_WORK_BUDGET_INIT;
    CjFullEnum full;
    full.count = 0;
    enumerate_full(lowercased_out, last.offset, last.len, &full);

    int written = 0;
    if (full.count > 0) {
        /* 整体能切成完整音节（可能有歧义，多个结果）——全部作为候选，
         * 不需要 pending。 */
        for (int i = 0; i < full.count && written < max_candidates; i++) {
            CjSegmentation *seg = &out_candidates[written];
            seg->syllable_count = 0;
            for (int k = 0; k < fixed_prefix.count; k++) {
                seg->syllables[seg->syllable_count++] = fixed_prefix.spans[k];
            }
            for (int k = 0; k < full.results[i].count && seg->syllable_count < CJ_SEG_MAX_SYLLABLES; k++) {
                seg->syllables[seg->syllable_count++] = full.results[i].spans[k];
            }
            seg->has_pending = 0;
            written++;
        }
        /* Python 版最后会按 rank（音节数升序）显式排序，不依赖枚举时的
         * 生成顺序（DFS 按最长优先切，实践中大多数情况已经巧合地把
         * 音节数最少的排前面，但不能假设一定如此，这里跟 Python 一样
         * 显式排一遍，保证 [0] 确实是音节数最少的那个）——candidates
         * 数量最多 CJ_SEG_MAX_CANDIDATES（8），插入排序足够。 */
        for (int i = 1; i < written; i++) {
            CjSegmentation key = out_candidates[i];
            int j = i - 1;
            while (j >= 0 && out_candidates[j].syllable_count > key.syllable_count) {
                out_candidates[j + 1] = out_candidates[j];
                j--;
            }
            out_candidates[j + 1] = key;
        }
    } else {
        /* 整体切不出来——从右往左找第一个"前面能切、后面是某个音节前缀"
         * 的切点，最长确定前缀 + 最短 pending 尾巴。 */
        for (int cut = (int)last.len - 1; cut >= 0 && written == 0; cut--) {
            size_t head_len = (size_t)cut;
            size_t tail_off = last.offset + head_len;
            size_t tail_len = last.len - head_len;
            if (!cj_is_prefix_of_some_syllable(lowercased_out + tail_off, tail_len)) {
                continue;
            }
            CjSpanList head_best;
            if (!dp_best_segmentation(lowercased_out, last.offset, head_len, &head_best)) {
                continue;
            }
            CjSegmentation *seg = &out_candidates[written];
            seg->syllable_count = 0;
            for (int k = 0; k < fixed_prefix.count; k++) {
                seg->syllables[seg->syllable_count++] = fixed_prefix.spans[k];
            }
            for (int k = 0; k < head_best.count && seg->syllable_count < CJ_SEG_MAX_SYLLABLES; k++) {
                seg->syllables[seg->syllable_count++] = head_best.spans[k];
            }
            seg->has_pending = 1;
            seg->pending.offset = tail_off;
            seg->pending.len = tail_len;
            written++;
        }
        if (written == 0 && max_candidates > 0) {
            /* 彻底切不出来（比如打了非法字母组合）——原样兜底：
             * 已确定前缀 + 整个 last chunk 当 pending，不报错、不崩溃。 */
            CjSegmentation *seg = &out_candidates[0];
            seg->syllable_count = 0;
            for (int k = 0; k < fixed_prefix.count; k++) {
                seg->syllables[seg->syllable_count++] = fixed_prefix.spans[k];
            }
            seg->has_pending = 1;
            seg->pending.offset = last.offset;
            seg->pending.len = last.len;
            written = 1;
        }
    }

    *out_count = written;
    return 1;
}

int cj_best_segmentation(const char *raw, size_t raw_len,
                          char *lowercased_out, CjSegmentation *out) {
    int count = 0;
    if (!cj_segment(raw, raw_len, lowercased_out, out, 1, &count)) {
        return 0;
    }
    return count > 0;
}
