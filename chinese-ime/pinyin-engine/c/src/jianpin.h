/* 简拼索引查询——C 移植版，对应 Python 端 src/jianpin.py（拼音输入法
 * 白皮书 07 节"简拼——声母首字母倒排索引"）。
 *
 * 跟 07 节最初设计的一处偏离，明确记录在这里：07 节原话是"候选文字/
 * 权重复用主 dict.bin 已有的候选池（用偏移量引用，不重复存储文字）"——
 * 真的动手实现时改成了自成一体的独立 blob（自己存一份候选词文字+
 * 权重，格式基本照抄 dictionary.h 的 CjDictHeader/CjDictKeyEntry/
 * CjDictCandEntry，只是换了个 magic）。原因：偏移量引用要求这份
 * jianpin blob 和 dict.bin 的候选表生成顺序/下标严格对齐，一旦两个
 * 生成脚本以后各自演化（比如 dict.bin 换数据源、加扩展词库导致内部
 * 顺序变化），jianpin blob 里存的下标会悄悄失效却不报错——mmap 场景
 * 下这是最不想要的一类 bug（静默越界，不是崩溃就是读到无意义数据）。
 * 改成自成一体、两份数据各自独立生成/独立校验/运行时互不依赖，代价
 * 只是多占一点磁盘空间（06 节磁盘空间调研已确认 /home 分区 45.8GB
 * 可用，不是需要精打细算的资源）。这跟 05 节繁体双 blob 设计里"MVP
 * 允许各自独立生成、有少量数据重复"是同一个取舍方向。
 *
 * 数据规模只是 dict.bin 的一部分——Python 端 JianpinIndex 只收多音节
 * 词（单音节词的简拼字母跟全拼首字母是同一个东西，已经被 segment 层
 * 的前缀匹配覆盖，收进来没有额外收益，见 jianpin.py 模块文档字符串），
 * 实测约 54 万条候选（dict.bin 约 59 万条）。
 *
 * 查询接口形状照抄 dictionary.h 的 cj_dict_lookup/cj_dict_lookup_prefix
 * （排序 key 数组 + 二分查找），但前缀匹配语义更简单：jianpin key 是
 * 不带分隔符的连续字母串，没有"音节数"这个概念，纯字节前缀匹配就是
 * 完整语义——不需要 dictionary.c 那套"字节前缀相同但音节数不同不算
 * 命中"的空格计数过滤，跟 Python 版 JianpinIndex.lookup_prefix() 的
 * 简单 bisect 区间扫描是同一个逻辑。
 */
#ifndef CJ_JIANPIN_H
#define CJ_JIANPIN_H

#include <stddef.h>
#include <stdint.h>

#define CJ_JIANPIN_MAGIC "CJJPIN01"
#define CJ_JIANPIN_MAGIC_LEN 8
#define CJ_JIANPIN_VERSION 1u

/* ---------- 二进制文件格式（小端；见 gen_jianpin_blob.py 对应的
 * struct.pack 格式字符串，两边必须逐字段同步）---------- */

typedef struct {
    char magic[8];                 /* "CJJPIN01" */
    uint32_t version;              /* CJ_JIANPIN_VERSION */
    uint32_t key_count;
    uint32_t cand_count;
    uint32_t reserved0;
    uint64_t key_table_offset;     /* 指向 CjJianpinKeyEntry[key_count]，绝对文件偏移 */
    uint64_t key_strings_offset;   /* 指向原始 key 字符串字节区，绝对文件偏移 */
    uint64_t key_strings_len;
    uint64_t cand_table_offset;    /* 指向 CjJianpinCandEntry[cand_count]，绝对文件偏移 */
    uint64_t cand_words_offset;    /* 指向候选词 UTF-8 字节区，绝对文件偏移 */
    uint64_t cand_words_len;
    uint64_t file_size;            /* 生成时的总文件大小，加载时做自检 */
} CjJianpinHeader; /* 80 字节，布局跟 CjDictHeader 相同但类型独立，不混用 */

typedef struct {
    uint32_t key_str_offset;   /* 绝对文件偏移，指向这个 key 的字符串字节 */
    uint32_t key_str_len;
    uint32_t cand_start_index; /* 这个 key 的候选在 CjJianpinCandEntry 数组里的起始下标 */
    uint32_t cand_count;
} CjJianpinKeyEntry; /* 16 字节 */

typedef struct {
    uint32_t word_offset;  /* 绝对文件偏移，指向候选词 UTF-8 字节 */
    uint32_t weight;
    uint16_t word_len;
    uint16_t reserved;
} CjJianpinCandEntry; /* 12 字节 */

/* ---------- 运行时 API ---------- */

typedef struct {
    const void *base;   /* mmap 基址；未打开/已关闭时为 NULL */
    size_t map_len;
    const CjJianpinKeyEntry *key_table;   /* 指向 base 内部，按 key 字符串字典序排列 */
    uint32_t key_count;
    const CjJianpinCandEntry *cand_table; /* 指向 base 内部 */
    uint32_t cand_count;
} CjJianpinHandle;

typedef struct {
    const char *word;   /* 指向 mmap 区域内部，不以 NUL 结尾，用 word_len 界定 */
    uint32_t word_len;
    uint32_t weight;
} CjJianpinCandidate;

/* 打开简拼索引文件：mmap(PROT_READ) + 校验 magic/version/file_size/
 * 各区域偏移量都落在文件范围内。成功返回 1 并填好 *out；失败（文件
 * 不存在/格式不对/mmap 失败/校验不过）返回 0，*out 保持全零。 */
int cj_jianpin_open(const char *path, CjJianpinHandle *out);

/* munmap，之后 *h 全部清零，不能再用来查询。对已经清零/未成功 open
 * 过的 handle 调用是安全的空操作。 */
void cj_jianpin_close(CjJianpinHandle *h);

/* 精确匹配：key 是简拼字母串（比如 "zg"），不要求 NUL 结尾，用
 * key_len 界定。命中的候选写进 out（调用方分配好的数组，容量
 * max_out，候选已经按权重降序排好），返回实际写入个数（0 表示未
 * 命中，不是错误）。 */
int cj_jianpin_lookup(const CjJianpinHandle *h, const char *key, size_t key_len,
                       CjJianpinCandidate *out, int max_out);

/* 前缀匹配：prefix 是简拼字母串的前缀（用户可能还没打完），纯字节
 * 前缀匹配（见文件头部说明——不需要 dictionary.c 那套音节数过滤）。
 * 语义等价于 Python 版 JianpinIndex.lookup_prefix()。返回值/写入
 * 规则同 cj_jianpin_lookup。 */
int cj_jianpin_lookup_prefix(const CjJianpinHandle *h, const char *prefix, size_t prefix_len,
                              CjJianpinCandidate *out, int max_out);

#endif /* CJ_JIANPIN_H */
