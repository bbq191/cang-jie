/* 用户词频（自适应调频）——C 移植版，Python 参照实现是 src/userfreq.py，
 * 全部行为规则（容量/淘汰/排序/文件格式）钉死在那边的模块 docstring 里，
 * 这里逐条对齐，差分测试 tests/diff_check_userfreq.py 逐字节比对。
 *
 * 跟 dictionary.c 那层的关系：静态词频（dict.bin，GPL 数据，mmap 只读）
 * 完全不动；用户计数是独立数据、独立文件（CJ_DATA_DIR/userfreq.tsv，
 * 我们自己的数据，无许可证问题），查询时在 hook 层融合：
 *   - 精确 key 组：cj_uf_words_for_key() 把用户选过的词插到组头；
 *   - 前缀预测/简拼池（候选完整 key 未知）：cj_uf_order() 按 word 聚合
 *     计数做组内稳定重排。
 *
 * 内存模型：定长静态数组（约 250KB bss），无 malloc——注入进程内不引入
 * 动态分配失败分支，fail-safe 原则跟 dict 层一致（load 失败=空表可用）。
 * 单线程假设：所有调用都发生在 xochitl UI 线程的 hook 路径里，跟
 * hook_init.c 现有全局状态同一个约束，不加锁。 */
#ifndef CJ_USERFREQ_H
#define CJ_USERFREQ_H

#include <stddef.h>
#include <stdint.h>

#define CJ_UF_MAX_ENTRIES 2048
#define CJ_UF_KEY_MAX 64
#define CJ_UF_WORD_MAX 48
#define CJ_UF_COUNT_CAP 1000000u
#define CJ_UF_HALF_LIFE 128u  /* v2 recency：每闲置 128 次全局记账，条目有效分减半 */

typedef struct {
    char key[CJ_UF_KEY_MAX];    /* 不 NUL 结尾，用 key_len 界定 */
    char word[CJ_UF_WORD_MAX];  /* 不 NUL 结尾，用 word_len 界定 */
    uint8_t key_len;
    uint8_t word_len;
    uint32_t count;             /* 存储计数，饱和于 CJ_UF_COUNT_CAP */
    uint32_t last;              /* v2：最后一次使用时的逻辑时钟值 */
} CjUfEntry;

typedef struct {
    CjUfEntry entries[CJ_UF_MAX_ENTRIES]; /* 按 (key 字节序, word 字节序) 升序 */
    int count;
    int dirty;                            /* record 置 1，save 成功清 0 */
    uint32_t gen;                         /* v2：逻辑时钟（每次成功记账 +1，不用墙钟） */
} CjUserFreq;

/* 清空为可用的空表。 */
void cj_uf_init(CjUserFreq *uf);

/* 从 TSV 文件整体加载（先清空）。文件不存在视为空表、返回 1；只有
 * 读取/打开出错才返回 0（此时 *uf 也已是可用的空表——fail-safe）。 */
int cj_uf_load(CjUserFreq *uf, const char *path);

/* 原子写（path.tmp + rename）。成功清 dirty 返回 1；失败返回 0、dirty
 * 保持，下次还会尝试。 */
int cj_uf_save(CjUserFreq *uf, const char *path);

/* 记一次选择（count+1，饱和）。key/word 为空或超长时静默忽略。
 * ⚠ 会移动内部条目：此前从 cj_uf_words_for_key() 拿到的指针全部失效。 */
void cj_uf_record(CjUserFreq *uf, const char *key, size_t key_len,
                  const char *word, size_t word_len);

/* record 的加 n 版（load 内部用；n=0 忽略）。 */
void cj_uf_record_n(CjUserFreq *uf, const char *key, size_t key_len,
                    const char *word, size_t word_len, uint32_t n);

/* 精确 (key, word) 的原始存储计数（不衰减，诊断用），无记录返回 0。 */
uint32_t cj_uf_count(const CjUserFreq *uf, const char *key, size_t key_len,
                     const char *word, size_t word_len);

/* v2：精确 (key, word) 的有效分 = count >> min((gen-last)/HALF_LIFE, 31)。
 * 排序/召回/淘汰全用它——recency 语义的唯一入口。 */
uint32_t cj_uf_effective(const CjUserFreq *uf, const char *key, size_t key_len,
                         const char *word, size_t word_len);

/* word 在所有 key 下的**有效分**之和（逐条饱和累加，跟 Python 版一致）。 */
uint32_t cj_uf_word_total(const CjUserFreq *uf, const char *word, size_t word_len);

/* key 下全部条目，**有效分**降序、并列按 word 字节序升序，最多 max_out 条。
 * out 里存的是指向内部条目的指针——只在下一次 cj_uf_record/cj_uf_load
 * 之前有效，调用方要么当场用完要么拷走。返回实际条数。 */
int cj_uf_words_for_key(const CjUserFreq *uf, const char *key, size_t key_len,
                        const CjUfEntry **out, int max_out);

/* 对一组候选词算重排排列：order_out[i] = 排第 i 位的原始下标。
 * 计数 = use_word_total ? word_total(word) : effective(key, word)；按计数
 * 降序稳定排序（并列保持原始下标序，计数全 0 时恒等排列）。n ≤ 组内
 * 候选数（实际调用 ≤ 64），内部插入排序。 */
void cj_uf_order(const CjUserFreq *uf, const char *key, size_t key_len,
                 int use_word_total, const char *const *words,
                 const size_t *word_lens, int n, int *order_out);

#endif /* CJ_USERFREQ_H */
