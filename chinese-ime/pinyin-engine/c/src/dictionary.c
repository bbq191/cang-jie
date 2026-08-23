#include "dictionary.h"

#include <fcntl.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

_Static_assert(sizeof(CjDictHeader) == 80, "CjDictHeader 布局必须跟 gen_dict_blob.py 的 struct.pack 格式字符串逐字段一致");
_Static_assert(sizeof(CjDictKeyEntry) == 16, "CjDictKeyEntry 布局必须跟 gen_dict_blob.py 一致");
_Static_assert(sizeof(CjDictCandEntry) == 12, "CjDictCandEntry 布局必须跟 gen_dict_blob.py 一致");

int cj_dict_open(const char *path, CjDictHandle *out) {
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    if (!path) return 0;

    int fd = open(path, O_RDONLY);
    if (fd < 0) return 0;

    struct stat st;
    if (fstat(fd, &st) != 0 || st.st_size < (off_t)sizeof(CjDictHeader)) {
        close(fd);
        return 0;
    }
    size_t map_len = (size_t)st.st_size;

    void *base = mmap(NULL, map_len, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd); /* mmap 建立后 fd 可以立即关闭，映射依然有效，跟标准用法一致 */
    if (base == MAP_FAILED) return 0;

    const CjDictHeader *hdr = (const CjDictHeader *)base;
    if (memcmp(hdr->magic, CJ_DICT_MAGIC, CJ_DICT_MAGIC_LEN) != 0 ||
        hdr->version != CJ_DICT_VERSION ||
        hdr->file_size != (uint64_t)map_len) {
        munmap(base, map_len);
        return 0;
    }

    /* 四个区域必须完整落在文件范围内——mmap 之后越界读取会直接段错误，
     * 这是加载时唯一的安全网（不逐条校验每个 key/候选条目内部的偏移量，
     * 这份数据是我们自己的生成工具产出的，不是不可信输入，跟项目里
     * 其它"信任自己编译期生成的数据"的做法一致，比如 syllables_data.h）。 */
    uint64_t key_table_bytes = (uint64_t)hdr->key_count * sizeof(CjDictKeyEntry);
    uint64_t cand_table_bytes = (uint64_t)hdr->cand_count * sizeof(CjDictCandEntry);
    if (hdr->key_table_offset > map_len || key_table_bytes > map_len - hdr->key_table_offset ||
        hdr->cand_table_offset > map_len || cand_table_bytes > map_len - hdr->cand_table_offset ||
        hdr->key_strings_offset > map_len || hdr->key_strings_len > map_len - hdr->key_strings_offset ||
        hdr->cand_words_offset > map_len || hdr->cand_words_len > map_len - hdr->cand_words_offset) {
        munmap(base, map_len);
        return 0;
    }

    out->base = base;
    out->map_len = map_len;
    out->key_table = (const CjDictKeyEntry *)((const char *)base + hdr->key_table_offset);
    out->key_count = hdr->key_count;
    out->cand_table = (const CjDictCandEntry *)((const char *)base + hdr->cand_table_offset);
    out->cand_count = hdr->cand_count;
    return 1;
}

void cj_dict_close(CjDictHandle *h) {
    if (!h) return;
    if (h->base) {
        munmap((void *)h->base, h->map_len);
    }
    memset(h, 0, sizeof(*h));
}

/* 跟 Python str 的默认（Unicode 码点）比较规则一致——我们的 key 全是
 * ASCII（小写字母 + 空格），码点序等于字节序，memcmp 直接可用。较短
 * 且是较长串真前缀的排在前面，跟 Python "ni" < "ni hao" 的规则一致。 */
static int cj_dict_key_cmp(const char *a, size_t a_len, const char *b, size_t b_len) {
    size_t min_len = a_len < b_len ? a_len : b_len;
    int c = min_len ? memcmp(a, b, min_len) : 0;
    if (c != 0) return c;
    if (a_len < b_len) return -1;
    if (a_len > b_len) return 1;
    return 0;
}

static const char *cj_dict_key_ptr(const CjDictHandle *h, const CjDictKeyEntry *e) {
    return (const char *)h->base + e->key_str_offset;
}

static const char *cj_dict_word_ptr(const CjDictHandle *h, const CjDictCandEntry *e) {
    return (const char *)h->base + e->word_offset;
}

/* 标准 lower_bound：返回第一个满足 key_table[i] >= target 的下标（比较
 * 用 cj_dict_key_cmp，target 不必是表里真实存在的 key，可以是前缀查询
 * 用的部分字符串）。key_count==0 时返回 0。 */
static uint32_t cj_dict_lower_bound(const CjDictHandle *h, const char *target, size_t target_len) {
    uint32_t lo = 0, hi = h->key_count;
    while (lo < hi) {
        uint32_t mid = lo + (hi - lo) / 2;
        const CjDictKeyEntry *e = &h->key_table[mid];
        int c = cj_dict_key_cmp(cj_dict_key_ptr(h, e), e->key_str_len, target, target_len);
        if (c < 0) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    return lo;
}

/* 把某个 key 条目自己的候选（已经按权重降序排好）拷贝进 out，最多
 * max_out 个，返回实际拷贝数。 */
static int cj_dict_copy_candidates(const CjDictHandle *h, const CjDictKeyEntry *e,
                                    CjDictCandidate *out, int max_out) {
    int n = (int)e->cand_count;
    if (n > max_out) n = max_out;
    for (int i = 0; i < n; i++) {
        const CjDictCandEntry *c = &h->cand_table[e->cand_start_index + (uint32_t)i];
        out[i].word = cj_dict_word_ptr(h, c);
        out[i].word_len = c->word_len;
        out[i].weight = c->weight;
    }
    return n;
}

int cj_dict_lookup(const CjDictHandle *h, const char *key, size_t key_len,
                    CjDictCandidate *out, int max_out) {
    if (!h || !h->base || !key || !out || max_out <= 0) return 0;

    uint32_t idx = cj_dict_lower_bound(h, key, key_len);
    if (idx >= h->key_count) return 0;
    const CjDictKeyEntry *e = &h->key_table[idx];
    if (cj_dict_key_cmp(cj_dict_key_ptr(h, e), e->key_str_len, key, key_len) != 0) {
        return 0; /* lower_bound 落点不是精确匹配 */
    }
    return cj_dict_copy_candidates(h, e, out, max_out);
}

static size_t cj_dict_count_spaces(const char *s, size_t len) {
    size_t n = 0;
    for (size_t i = 0; i < len; i++) {
        if (s[i] == ' ') n++;
    }
    return n;
}

/* true 当且仅当按排序规则 a 应该排在 b 前面（权重降序；权重相同时按候选词
 * 字节序排列，保证结果确定性）。Python 参照实现（Dictionary.lookup_prefix）
 * 已对齐成同一套 tie-break（-weight, word.encode("utf-8")）——iorest 大词库
 * 灌入后前缀池常超过 limit、截断落在同权重并列区，两边 tie-break 必须一致，
 * 否则截断到的成员集合不同（差分测试 240 万规模下暴露过并已修）。 */
static int cj_dict_cand_ranks_before(const CjDictCandidate *a, const CjDictCandidate *b) {
    if (a->weight != b->weight) return a->weight > b->weight;
    size_t min_len = a->word_len < b->word_len ? a->word_len : b->word_len;
    int c = min_len ? memcmp(a->word, b->word, min_len) : 0;
    if (c != 0) return c < 0;
    return a->word_len < b->word_len;
}

/* 流式 top-k 插入：把 cand 插入有序的 out[0..*out_n)（*out_n <= max_out，
 * 按 cj_dict_cand_ranks_before 保持降序），维持"只留权重最高的 max_out 个"。
 *
 * 早期版本用一个固定大小（512）的 scratch 缓冲区先攒满所有候选再统一
 * qsort，缓冲区攒满就直接 break、不再看后面的 key——41448 大字表加入后
 * 单个前缀（比如 "sh"）能匹配到的候选总数远超 512，导致真正权重最高的
 * 一批候选因为在遍历顺序里排在后面、缓冲区已经满了而被直接丢弃，属于
 * "样本内正确、样本外错误"的截断 bug，是差分测试跑出来的真实结果，不是
 * 理论推演。这里换成流式 top-k（只维护 max_out 个"当前最优"，用不到跟
 * 候选总数同量级的缓冲区），不管前缀匹配到多少候选、遍历顺序如何，
 * 结果都是真正的全局 top-k。 */
static void cj_dict_topk_insert(CjDictCandidate *out, int *out_n, int max_out,
                                 const CjDictCandidate *cand) {
    int n = *out_n;
    if (n == max_out) {
        if (!cj_dict_cand_ranks_before(cand, &out[n - 1])) {
            return; /* 不比当前垫底的更好，直接丢弃，是最常见的情况 */
        }
        n--; /* 腾出最后一个位置给 cand，下面统一走插入排序 */
    }
    int pos = n;
    while (pos > 0 && cj_dict_cand_ranks_before(cand, &out[pos - 1])) {
        out[pos] = out[pos - 1];
        pos--;
    }
    out[pos] = *cand;
    *out_n = n + 1;
}

int cj_dict_lookup_prefix(const CjDictHandle *h, const char *key_prefix, size_t prefix_len,
                           CjDictCandidate *out, int max_out) {
    if (!h || !h->base || !key_prefix || !out || max_out <= 0) return 0;

    size_t expected_spaces = cj_dict_count_spaces(key_prefix, prefix_len);
    int out_n = 0;

    uint32_t idx = cj_dict_lower_bound(h, key_prefix, prefix_len);
    for (; idx < h->key_count; idx++) {
        const CjDictKeyEntry *e = &h->key_table[idx];
        const char *key_str = cj_dict_key_ptr(h, e);
        if (e->key_str_len < prefix_len || memcmp(key_str, key_prefix, prefix_len) != 0) {
            break; /* 排序数组里字节前缀不再匹配，后面不可能再有命中 */
        }
        if (cj_dict_count_spaces(key_str, e->key_str_len) != expected_spaces) {
            continue; /* 字节前缀凑巧对上，但音节数不对（比如更长的 key），跳过不算命中 */
        }
        for (uint32_t i = 0; i < e->cand_count; i++) {
            const CjDictCandEntry *c = &h->cand_table[e->cand_start_index + i];
            CjDictCandidate cand;
            cand.word = cj_dict_word_ptr(h, c);
            cand.word_len = c->word_len;
            cand.weight = c->weight;
            cj_dict_topk_insert(out, &out_n, max_out, &cand);
        }
    }

    return out_n;
}
