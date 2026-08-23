/* 音节切分——C 移植版，对应 Python 端 src/segment.py。完整设计动机见
 * 那份文件的 docstring，这里只重复跟 C 实现直接相关的边界约定：
 *
 *   - 输入长度、候选数量、单个切分里的音节数全部加了上限（见下面
 *     CJ_SEG_MAX_* 常量）——这是 C 移植版跟 Python 版的主要差异，Python
 *     版靠 lru_cache 天然限定了同一子串只算一次，这里改用更保守的硬
 *     上限：一是避免无界内存分配（嵌入式/键盘 hook 场景不适合动态
 *     分配），二是防御性地堵住"长按同一个字母"这种病态输入可能触发的
 *     指数级枚举（比如连续 32 个 "a"，每个 "a" 单独都是合法音节，
 *     全枚举组合数会爆炸）——加了递归工作量预算（内部实现细节，不在
 *     这个头文件暴露），触顶就提前收工，不崩溃、不卡死，只是可能少
 *     枚举出几种冷门的歧义切法，这是有意的取舍。
 *   - 音节文本不拷贝，用 (offset, len) 指回调用方传入的缓冲区（已经
 *     被 cj_segment 内部转成小写的那份），调用方要保证这个缓冲区在
 *     使用 CjSegmentation 结果期间一直有效。
 */
#ifndef CJ_SEGMENT_H
#define CJ_SEGMENT_H

#include <stddef.h>

#define CJ_SEG_MAX_INPUT 32       /* 单次调用最长输入字符数（交互式打字场景足够，超过则拒绝处理） */
#define CJ_SEG_MAX_SYLLABLES 16   /* 一个切分结果里最多容纳的音节数 */
#define CJ_SEG_MAX_CANDIDATES 8   /* cj_segment() 最多返回几种候选切分 */

typedef struct {
    size_t offset; /* 相对 cj_segment 内部小写化缓冲区的起始偏移 */
    size_t len;
} CjSyllableSpan;

typedef struct {
    CjSyllableSpan syllables[CJ_SEG_MAX_SYLLABLES];
    int syllable_count;    /* 已经切出来的完整音节数 */
    int has_pending;       /* 最后一段是否是"打到一半"的未完成前缀 */
    CjSyllableSpan pending; /* has_pending==0 时内容无意义 */
} CjSegmentation;

/* raw_len 超过 CJ_SEG_MAX_INPUT 时返回 0（失败，*out_count 置 0，调用方
 * 应该拒绝这次输入或者分批处理，不应该发生在正常交互式打字场景）。
 * 成功时返回 1，把候选切分（按"最合理"排序，[0] 是推荐结果）写进
 * out_candidates（调用方分配好的数组，容量至少 CJ_SEG_MAX_CANDIDATES），
 * *out_count 是实际写入的候选数（至少 1——即使完全切不出合法音节，也会
 * 退化成"整体当 pending"兜底，不会返回 0 个候选）。
 *
 * lowercased_out 是调用方提供的缓冲区（容量至少 raw_len 字节），
 * cj_segment 会把 raw 转成小写写进去——CjSegmentation 里的所有 span
 * 都是相对这个缓冲区的偏移，调用方要保证它在使用结果期间保持有效。 */
int cj_segment(const char *raw, size_t raw_len,
               char *lowercased_out,
               CjSegmentation *out_candidates, int max_candidates,
               int *out_count);

/* 便捷入口：只要排第一的最优候选。返回 1 成功、0 失败（raw_len 超限），
 * 语义等价于 Python 版 best_segmentation() = segment(raw)[0]。 */
int cj_best_segmentation(const char *raw, size_t raw_len,
                          char *lowercased_out, CjSegmentation *out);

#endif /* CJ_SEGMENT_H */
