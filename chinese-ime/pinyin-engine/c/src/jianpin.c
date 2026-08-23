#include "jianpin.h"

#include <fcntl.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

_Static_assert(sizeof(CjJianpinHeader) == 80, "CjJianpinHeader 布局必须跟 gen_jianpin_blob.py 的 struct.pack 格式字符串逐字段一致");
_Static_assert(sizeof(CjJianpinKeyEntry) == 16, "CjJianpinKeyEntry 布局必须跟 gen_jianpin_blob.py 一致");
_Static_assert(sizeof(CjJianpinCandEntry) == 12, "CjJianpinCandEntry 布局必须跟 gen_jianpin_blob.py 一致");

int cj_jianpin_open(const char *path, CjJianpinHandle *out) {
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    if (!path) return 0;

    int fd = open(path, O_RDONLY);
    if (fd < 0) return 0;

    struct stat st;
    if (fstat(fd, &st) != 0 || st.st_size < (off_t)sizeof(CjJianpinHeader)) {
        close(fd);
        return 0;
    }
    size_t map_len = (size_t)st.st_size;

    void *base = mmap(NULL, map_len, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd); /* mmap 建立后 fd 可以立即关闭，映射依然有效 */
    if (base == MAP_FAILED) return 0;

    const CjJianpinHeader *hdr = (const CjJianpinHeader *)base;
    if (memcmp(hdr->magic, CJ_JIANPIN_MAGIC, CJ_JIANPIN_MAGIC_LEN) != 0 ||
        hdr->version != CJ_JIANPIN_VERSION ||
        hdr->file_size != (uint64_t)map_len) {
        munmap(base, map_len);
        return 0;
    }

    /* 四个区域必须完整落在文件范围内——道理跟 dictionary.c 一样：这份
     * 数据是我们自己的生成工具产出的，不是不可信输入，只做范围校验，
     * 不逐条校验每个 key/候选条目内部的偏移量。 */
    uint64_t key_table_bytes = (uint64_t)hdr->key_count * sizeof(CjJianpinKeyEntry);
    uint64_t cand_table_bytes = (uint64_t)hdr->cand_count * sizeof(CjJianpinCandEntry);
    if (hdr->key_table_offset > map_len || key_table_bytes > map_len - hdr->key_table_offset ||
        hdr->cand_table_offset > map_len || cand_table_bytes > map_len - hdr->cand_table_offset ||
        hdr->key_strings_offset > map_len || hdr->key_strings_len > map_len - hdr->key_strings_offset ||
        hdr->cand_words_offset > map_len || hdr->cand_words_len > map_len - hdr->cand_words_offset) {
        munmap(base, map_len);
        return 0;
    }

    out->base = base;
    out->map_len = map_len;
    out->key_table = (const CjJianpinKeyEntry *)((const char *)base + hdr->key_table_offset);
    out->key_count = hdr->key_count;
    out->cand_table = (const CjJianpinCandEntry *)((const char *)base + hdr->cand_table_offset);
    out->cand_count = hdr->cand_count;
    return 1;
}

void cj_jianpin_close(CjJianpinHandle *h) {
    if (!h) return;
    if (h->base) {
        munmap((void *)h->base, h->map_len);
    }
    memset(h, 0, sizeof(*h));
}

/* key 全是 ASCII 小写字母，字节序等于 Python str 的默认（Unicode
 * 码点）比较序，memcmp 直接可用，跟 dictionary.c::cj_dict_key_cmp
 * 同一个逻辑。 */
static int cj_jianpin_key_cmp(const char *a, size_t a_len, const char *b, size_t b_len) {
    size_t min_len = a_len < b_len ? a_len : b_len;
    int c = min_len ? memcmp(a, b, min_len) : 0;
    if (c != 0) return c;
    if (a_len < b_len) return -1;
    if (a_len > b_len) return 1;
    return 0;
}

static const char *cj_jianpin_key_ptr(const CjJianpinHandle *h, const CjJianpinKeyEntry *e) {
    return (const char *)h->base + e->key_str_offset;
}

static const char *cj_jianpin_word_ptr(const CjJianpinHandle *h, const CjJianpinCandEntry *e) {
    return (const char *)h->base + e->word_offset;
}

/* 标准 lower_bound：返回第一个满足 key_table[i] >= target 的下标。
 * target 不必是表里真实存在的 key，可以是前缀查询用的部分字符串。
 * key_count==0 时返回 0。 */
static uint32_t cj_jianpin_lower_bound(const CjJianpinHandle *h, const char *target, size_t target_len) {
    uint32_t lo = 0, hi = h->key_count;
    while (lo < hi) {
        uint32_t mid = lo + (hi - lo) / 2;
        const CjJianpinKeyEntry *e = &h->key_table[mid];
        int c = cj_jianpin_key_cmp(cj_jianpin_key_ptr(h, e), e->key_str_len, target, target_len);
        if (c < 0) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    return lo;
}

/* 某个 key 条目自己的候选（生成时已经按权重降序排好），直接拷贝进
 * out，最多 max_out 个，返回实际拷贝数。 */
static int cj_jianpin_copy_candidates(const CjJianpinHandle *h, const CjJianpinKeyEntry *e,
                                       CjJianpinCandidate *out, int max_out) {
    int n = (int)e->cand_count;
    if (n > max_out) n = max_out;
    for (int i = 0; i < n; i++) {
        const CjJianpinCandEntry *c = &h->cand_table[e->cand_start_index + (uint32_t)i];
        out[i].word = cj_jianpin_word_ptr(h, c);
        out[i].word_len = c->word_len;
        out[i].weight = c->weight;
    }
    return n;
}

int cj_jianpin_lookup(const CjJianpinHandle *h, const char *key, size_t key_len,
                       CjJianpinCandidate *out, int max_out) {
    if (!h || !h->base || !key || !out || max_out <= 0) return 0;

    uint32_t idx = cj_jianpin_lower_bound(h, key, key_len);
    if (idx >= h->key_count) return 0;
    const CjJianpinKeyEntry *e = &h->key_table[idx];
    if (cj_jianpin_key_cmp(cj_jianpin_key_ptr(h, e), e->key_str_len, key, key_len) != 0) {
        return 0; /* lower_bound 落点不是精确匹配 */
    }
    return cj_jianpin_copy_candidates(h, e, out, max_out);
}

/* true 当且仅当按排序规则 a 应该排在 b 前面（权重降序；权重相同时按
 * 候选词字节序 tie-break，跟 Python 参照实现的稳定排序不保证同一顺序
 * ——差分测试对这种情况只要求候选集合相同+[0]一致，见
 * diff_check_jianpin.py 顶部说明，跟 dictionary.c 的处理方式一致）。 */
static int cj_jianpin_cand_ranks_before(const CjJianpinCandidate *a, const CjJianpinCandidate *b) {
    if (a->weight != b->weight) return a->weight > b->weight;
    size_t min_len = a->word_len < b->word_len ? a->word_len : b->word_len;
    int c = min_len ? memcmp(a->word, b->word, min_len) : 0;
    if (c != 0) return c < 0;
    return a->word_len < b->word_len;
}

/* 流式 top-k 插入——跟 dictionary.c::cj_dict_topk_insert 同一个理由：
 * 前缀匹配可能扫到远超 max_out 的候选（简拼天然比全拼歧义大，一个
 * 短前缀可能命中几百上千个词），不能用固定缓冲区攒够就不再看后面
 * 的 key（06 节 dictionary.c 那次真机+差分测试都验证过这是真实 bug，
 * 不是理论顾虑），维护一个大小恰好 max_out 的有序数组即可保证结果
 * 始终是全局 top-k，不依赖遍历顺序。 */
static void cj_jianpin_topk_insert(CjJianpinCandidate *out, int *out_n, int max_out,
                                    const CjJianpinCandidate *cand) {
    int n = *out_n;
    if (n == max_out) {
        if (!cj_jianpin_cand_ranks_before(cand, &out[n - 1])) {
            return; /* 不比当前垫底的更好，直接丢弃 */
        }
        n--; /* 腾出最后一个位置给 cand，下面走插入排序 */
    }
    int pos = n;
    while (pos > 0 && cj_jianpin_cand_ranks_before(cand, &out[pos - 1])) {
        out[pos] = out[pos - 1];
        pos--;
    }
    out[pos] = *cand;
    *out_n = n + 1;
}

int cj_jianpin_lookup_prefix(const CjJianpinHandle *h, const char *prefix, size_t prefix_len,
                              CjJianpinCandidate *out, int max_out) {
    if (!h || !h->base || !prefix || !out || max_out <= 0) return 0;
    if (prefix_len == 0) return 0; /* 跟 Python 版一致：空前缀直接返回空，不是"匹配一切" */

    int out_n = 0;
    uint32_t idx = cj_jianpin_lower_bound(h, prefix, prefix_len);
    for (; idx < h->key_count; idx++) {
        const CjJianpinKeyEntry *e = &h->key_table[idx];
        const char *key_str = cj_jianpin_key_ptr(h, e);
        if (e->key_str_len < prefix_len || memcmp(key_str, prefix, prefix_len) != 0) {
            break; /* 排序数组里字节前缀不再匹配，后面不可能再有命中 */
        }
        for (uint32_t i = 0; i < e->cand_count; i++) {
            const CjJianpinCandEntry *c = &h->cand_table[e->cand_start_index + i];
            CjJianpinCandidate cand;
            cand.word = cj_jianpin_word_ptr(h, c);
            cand.word_len = c->word_len;
            cand.weight = c->weight;
            cj_jianpin_topk_insert(out, &out_n, max_out, &cand);
        }
    }

    return out_n;
}
