/* 候选词典查询——C 移植版，对应 Python 端 src/dictionary.py。
 *
 * 跟 syllables.c/segment.c 不同：词典数据量级太大（55 万+条），不适合
 * 编译期常量数组塞进二进制（那是 syllables_data.h 的做法，429 个音节
 * 那个量级才合适）。这里改成运行时 mmap 一份离线生成好的二进制文件
 * （见 c/tools/gen_dict_blob.py），原因见白皮书/计划文件"M5"章节：
 *
 *   1. 词典数据源（雾凇拼音 rime-ice）是 GPL-3.0，如果编译进跟 xochitl
 *      一起 LD_PRELOAD 加载的 .so 本体，跟"GPL 代码被编译链接进同一个
 *      二进制"是同一类争议；存成独立文件、运行时只读 mmap，法律定性
 *      更接近"程序读取一份数据文件"（不代替法律意见，只是记录架构
 *      选择本身降低了风险类别）。
 *   2. mmap 是只读、零拷贝、不需要在加载时反序列化或重建任何指针——
 *      整个格式设计成"全部用文件内的绝对字节偏移量，不用堆指针"，
 *      这是 Step S 真机崩溃教训的直接应用（"不确定内存布局细节时不要
 *      手写内存操作"，这里干脆不用需要手工维护的指针，运行时只做只读
 *      偏移量运算）。
 *
 * 查询效率：不是指针式 trie，是"排序字符串数组 + 二分查找"——README
 * 记录的已知瓶颈是 Dictionary.lookup_prefix 的线性扫描，不是"数据结构
 * 不够聪明"，二分查找已经把这个瓶颈从 O(n) 降到 O(log n)，用词典规模
 * 现在这个形状（同一个音节序列的候选个数通常个位数到几十条，不是那种
 * "海量长公共前缀字符串"场景）没有明显劣势，实现也比序列化/反序列化
 * 一棵指针树简单得多。如果以后真机验证发现查询延迟不够，再考虑升级，
 * 这一轮不预先做。
 */
#ifndef CJ_DICTIONARY_H
#define CJ_DICTIONARY_H

#include <stddef.h>
#include <stdint.h>

/* ---------- 二进制文件格式（小端，跟生成机器/aarch64 目标机一致，
 * 不做字节序转换；见 gen_dict_blob.py 里对应的 struct.pack 格式字符串，
 * 两边必须逐字段同步）---------- */

#define CJ_DICT_MAGIC "CJDICT01"
#define CJ_DICT_MAGIC_LEN 8
#define CJ_DICT_VERSION 1u

typedef struct {
    char magic[8];                 /* "CJDICT01" */
    uint32_t version;              /* CJ_DICT_VERSION */
    uint32_t key_count;
    uint32_t cand_count;
    uint32_t reserved0;
    uint64_t key_table_offset;     /* 指向 CjDictKeyEntry[key_count]，绝对文件偏移 */
    uint64_t key_strings_offset;   /* 指向原始 key 字符串字节区，绝对文件偏移 */
    uint64_t key_strings_len;
    uint64_t cand_table_offset;    /* 指向 CjDictCandEntry[cand_count]，绝对文件偏移 */
    uint64_t cand_words_offset;    /* 指向候选词 UTF-8 字节区，绝对文件偏移 */
    uint64_t cand_words_len;
    uint64_t file_size;            /* 生成时的总文件大小，加载时做自检 */
} CjDictHeader;

/* key_str_offset/word_offset 用 uint32_t（不是跟 header 一样用 uint64_t）：
 * 这两个结构体会乘以词条数（55 万+条）反复出现，是文件体积的大头（实测
 * 一开始用 uint64_t 设计出来的 dict.bin 是 30.6MB，比原始 16.7MB YAML
 * 还大，改窄到 uint32_t 后降到约 25MB——文件总大小实测在 25-30MB 量级，
 * 远小于 4GB，uint32_t 绝对偏移绰绰有余，不用为不会发生的场景预留
 * uint64_t 的空间）。 */
typedef struct {
    uint32_t key_str_offset;   /* 绝对文件偏移，指向这个 key 的字符串字节 */
    uint32_t key_str_len;
    uint32_t cand_start_index; /* 这个 key 的候选在 CjDictCandEntry 数组里的起始下标 */
    uint32_t cand_count;
} CjDictKeyEntry; /* 16 字节 */

typedef struct {
    uint32_t word_offset;  /* 绝对文件偏移，指向候选词 UTF-8 字节 */
    uint32_t weight;
    uint16_t word_len;
    uint16_t reserved;
} CjDictCandEntry; /* 12 字节 */

/* ---------- 运行时 API ---------- */

typedef struct {
    const void *base;   /* mmap 基址；未打开/已关闭时为 NULL */
    size_t map_len;
    const CjDictKeyEntry *key_table;   /* 指向 base 内部，按 key 字符串字典序排列 */
    uint32_t key_count;
    const CjDictCandEntry *cand_table; /* 指向 base 内部 */
    uint32_t cand_count;
    /* key_str_offset/word_offset 都是绝对文件偏移（相对 base），直接
     * (const char*)base + offset 取用，不需要额外的区域基址字段。 */
} CjDictHandle;

typedef struct {
    const char *word;   /* 指向 mmap 区域内部，不以 NUL 结尾，用 word_len 界定 */
    uint32_t word_len;
    uint32_t weight;
} CjDictCandidate;

/* 打开词典文件：mmap(PROT_READ) + 校验 magic/version/file_size/各区域
 * 偏移量都落在文件范围内。成功返回 1 并填好 *out；失败（文件不存在/
 * 格式不对/mmap 失败/校验不过）返回 0，*out 保持全零，不会有部分初始化
 * 的半吊子状态，调用方不需要额外判断"到底哪些字段有效"。 */
int cj_dict_open(const char *path, CjDictHandle *out);

/* munmap，之后 *h 全部清零，不能再用来查询。对已经清零/未成功 open
 * 过的 handle 调用是安全的空操作。 */
void cj_dict_close(CjDictHandle *h);

/* 精确匹配：key 是空格拼接好的完整音节序列（比如 "ni hao"），不要求
 * NUL 结尾，用 key_len 界定。命中的候选写进 out（调用方分配好的数组，
 * 容量 max_out，候选已经按权重降序排好，不需要调用方再排序），返回
 * 实际写入个数（0 表示未命中，不是错误；如果真实候选数超过 max_out，
 * 只写入权重最高的 max_out 个，返回值等于 max_out）。 */
int cj_dict_lookup(const CjDictHandle *h, const char *key, size_t key_len,
                    CjDictCandidate *out, int max_out);

/* 前缀匹配：key_prefix 的最后一个空格分隔的片段是"打到一半"的前缀，
 * 前面的片段必须完整匹配，音节总数也必须一致（即使字节前缀凑巧对得上，
 * 音节数不同也不算命中——例如 "ni h" 匹配 "ni hao"/"ni huan"，不匹配
 * "ni"（长度不够）也不匹配 "ni hao yi he"（4 音节，不是 2 音节））。
 * 语义等价于 Python 版 Dictionary.lookup_prefix()。返回值/写入规则同
 * cj_dict_lookup。 */
int cj_dict_lookup_prefix(const CjDictHandle *h, const char *key_prefix, size_t prefix_len,
                           CjDictCandidate *out, int max_out);

#endif /* CJ_DICTIONARY_H */
