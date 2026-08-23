/* 用户词频 C 实现（v2 含 recency）——行为逐条对齐 src/userfreq.py（规则
 * 见那边 docstring 与 userfreq.h 顶部），差分测试逐字节比对。 */
#include "userfreq.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CJ_UF_GEN_MAX 0xFFFFFFFFu

/* (key, word) 二元组字节序比较——语义等价 Python 的 (bytes, bytes) 元组
 * 比较：先比 key（memcmp 公共前缀，前缀相同短者小），再比 word。 */
static int uf_bytes_cmp(const char *a, size_t alen, const char *b, size_t blen) {
    size_t n = alen < blen ? alen : blen;
    int c = n ? memcmp(a, b, n) : 0;
    if (c) {
        return c;
    }
    if (alen < blen) return -1;
    if (alen > blen) return 1;
    return 0;
}

static int uf_pair_cmp(const CjUfEntry *e, const char *key, size_t key_len,
                       const char *word, size_t word_len) {
    int c = uf_bytes_cmp(e->key, e->key_len, key, key_len);
    if (c) {
        return c;
    }
    return uf_bytes_cmp(e->word, e->word_len, word, word_len);
}

/* v2 有效分：count >> min((gen - last) / HALF_LIFE, 31)。 */
static uint32_t uf_effective(const CjUserFreq *uf, const CjUfEntry *e) {
    uint32_t age = uf->gen - e->last; /* last ≤ gen 恒成立（load 收尾对齐） */
    uint32_t shift = age / CJ_UF_HALF_LIFE;
    if (shift > 31) {
        shift = 31;
    }
    return e->count >> shift;
}

/* 二分：返回插入点；*hit 置是否命中。 */
static int uf_find(const CjUserFreq *uf, const char *key, size_t key_len,
                   const char *word, size_t word_len, int *hit) {
    int lo = 0, hi = uf->count;
    while (lo < hi) {
        int mid = lo + (hi - lo) / 2;
        if (uf_pair_cmp(&uf->entries[mid], key, key_len, word, word_len) < 0) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    *hit = lo < uf->count &&
           uf_pair_cmp(&uf->entries[lo], key, key_len, word, word_len) == 0;
    return lo;
}

/* 淘汰受害者：有效分最小，并列取 (key, word) 字节序最大。 */
static int uf_victim(const CjUserFreq *uf) {
    int best = 0;
    for (int i = 1; i < uf->count; i++) {
        const CjUfEntry *e = &uf->entries[i], *b = &uf->entries[best];
        uint32_t ei = uf_effective(uf, e), eb = uf_effective(uf, b);
        if (ei < eb ||
            (ei == eb &&
             uf_pair_cmp(e, b->key, b->key_len, b->word, b->word_len) > 0)) {
            best = i;
        }
    }
    return best;
}

static void uf_bump_gen(CjUserFreq *uf) {
    if (uf->gen < CJ_UF_GEN_MAX) {
        uf->gen++;
    }
}

void cj_uf_init(CjUserFreq *uf) {
    uf->count = 0;
    uf->dirty = 0;
    uf->gen = 0;
}

/* 统一写入：is_load=0 正常记账（last=gen、成功后 gen+1）；
 * is_load=1 为 load 路径（用行内 last、不推进 gen、重复行不衰减直加）。 */
static void uf_record_at(CjUserFreq *uf, const char *key, size_t key_len,
                         const char *word, size_t word_len, uint32_t n,
                         uint32_t last, int is_load) {
    if (n == 0 || key_len == 0 || word_len == 0 ||
        key_len > CJ_UF_KEY_MAX || word_len > CJ_UF_WORD_MAX) {
        return; /* fail-safe：超长/空静默忽略（不推进 gen） */
    }
    if (n > CJ_UF_COUNT_CAP) {
        n = CJ_UF_COUNT_CAP;
    }
    int hit = 0;
    int idx = uf_find(uf, key, key_len, word, word_len, &hit);
    if (hit) {
        CjUfEntry *e = &uf->entries[idx];
        if (!is_load) {
            /* 命中：先结算衰减再加，last 更新到当前 gen */
            uint32_t eff = uf_effective(uf, e);
            e->count = (eff > CJ_UF_COUNT_CAP - n) ? CJ_UF_COUNT_CAP : eff + n;
            e->last = uf->gen;
            uf_bump_gen(uf);
            uf->dirty = 1;
        } else {
            /* load 重复行：count 直接相加（不衰减）、last 取较大 */
            e->count = (e->count > CJ_UF_COUNT_CAP - n) ? CJ_UF_COUNT_CAP : e->count + n;
            if (last > e->last) {
                e->last = last;
            }
        }
        return;
    }
    uint32_t entry_last = is_load ? last : uf->gen;
    if (uf->count >= CJ_UF_MAX_ENTRIES) {
        int v = uf_victim(uf);
        uint32_t ev = uf_effective(uf, &uf->entries[v]);
        /* 新条目以 n（它的有效分）参与比较 */
        if (ev > n) {
            return; /* 受害者更强 → 丢弃新条目（不推进 gen） */
        }
        if (ev == n &&
            uf_pair_cmp(&uf->entries[v], key, key_len, word, word_len) < 0) {
            return; /* 并列且新条目字节序更大 → 丢弃新条目 */
        }
        memmove(&uf->entries[v], &uf->entries[v + 1],
                sizeof(CjUfEntry) * (size_t)(uf->count - v - 1));
        uf->count--;
        idx = uf_find(uf, key, key_len, word, word_len, &hit); /* 删除可能移动插入点 */
    }
    memmove(&uf->entries[idx + 1], &uf->entries[idx],
            sizeof(CjUfEntry) * (size_t)(uf->count - idx));
    CjUfEntry *e = &uf->entries[idx];
    memcpy(e->key, key, key_len);
    memcpy(e->word, word, word_len);
    e->key_len = (uint8_t)key_len;
    e->word_len = (uint8_t)word_len;
    e->count = n;
    e->last = entry_last;
    uf->count++;
    if (!is_load) {
        uf_bump_gen(uf);
        uf->dirty = 1;
    }
}

void cj_uf_record(CjUserFreq *uf, const char *key, size_t key_len,
                  const char *word, size_t word_len) {
    uf_record_at(uf, key, key_len, word, word_len, 1, 0, 0);
}

void cj_uf_record_n(CjUserFreq *uf, const char *key, size_t key_len,
                    const char *word, size_t word_len, uint32_t n) {
    uf_record_at(uf, key, key_len, word, word_len, n, 0, 0);
}

uint32_t cj_uf_count(const CjUserFreq *uf, const char *key, size_t key_len,
                     const char *word, size_t word_len) {
    int hit = 0;
    int idx = uf_find(uf, key, key_len, word, word_len, &hit);
    return hit ? uf->entries[idx].count : 0;
}

uint32_t cj_uf_effective(const CjUserFreq *uf, const char *key, size_t key_len,
                         const char *word, size_t word_len) {
    int hit = 0;
    int idx = uf_find(uf, key, key_len, word, word_len, &hit);
    return hit ? uf_effective(uf, &uf->entries[idx]) : 0;
}

uint32_t cj_uf_word_total(const CjUserFreq *uf, const char *word, size_t word_len) {
    uint32_t total = 0;
    for (int i = 0; i < uf->count; i++) {
        const CjUfEntry *e = &uf->entries[i];
        if (e->word_len == word_len && memcmp(e->word, word, word_len) == 0) {
            uint32_t eff = uf_effective(uf, e);
            total = (total > CJ_UF_COUNT_CAP - eff) ? CJ_UF_COUNT_CAP : total + eff;
        }
    }
    return total;
}

int cj_uf_words_for_key(const CjUserFreq *uf, const char *key, size_t key_len,
                        const CjUfEntry **out, int max_out) {
    if (max_out <= 0) {
        return 0;
    }
    /* 同 key 条目在内存序里连续：定位到 (key, "") 的插入点后线性扫。 */
    int hit = 0;
    int start = uf_find(uf, key, key_len, "", 0, &hit);
    int n = 0;
    const CjUfEntry *rows[CJ_UF_MAX_ENTRIES];
    uint32_t effs[CJ_UF_MAX_ENTRIES];
    for (int i = start; i < uf->count; i++) {
        const CjUfEntry *e = &uf->entries[i];
        if (uf_bytes_cmp(e->key, e->key_len, key, key_len) != 0) {
            break;
        }
        effs[n] = uf_effective(uf, e);
        rows[n++] = e;
    }
    /* 插入排序：有效分降序，并列按 word 字节序升序（rows 初始就是 word
     * 字节序升序，稳定插入排序天然保住并列序）。 */
    for (int i = 1; i < n; i++) {
        const CjUfEntry *cur = rows[i];
        uint32_t ce = effs[i];
        int j = i - 1;
        while (j >= 0 && effs[j] < ce) {
            rows[j + 1] = rows[j];
            effs[j + 1] = effs[j];
            j--;
        }
        rows[j + 1] = cur;
        effs[j + 1] = ce;
    }
    if (n > max_out) {
        n = max_out;
    }
    for (int i = 0; i < n; i++) {
        out[i] = rows[i];
    }
    return n;
}

void cj_uf_order(const CjUserFreq *uf, const char *key, size_t key_len,
                 int use_word_total, const char *const *words,
                 const size_t *word_lens, int n, int *order_out) {
    uint32_t vals[CJ_UF_MAX_ENTRIES];
    if (n <= 0) {
        return;
    }
    if (n > CJ_UF_MAX_ENTRIES) {
        n = CJ_UF_MAX_ENTRIES; /* 防御：调用方实际 ≤ 64 */
    }
    for (int i = 0; i < n; i++) {
        vals[i] = use_word_total
                      ? cj_uf_word_total(uf, words[i], word_lens[i])
                      : cj_uf_effective(uf, key, key_len, words[i], word_lens[i]);
        order_out[i] = i;
    }
    /* 稳定插入排序：有效分降序，并列保持原始下标升序。 */
    for (int i = 1; i < n; i++) {
        int cur = order_out[i];
        uint32_t cc = vals[cur];
        int j = i - 1;
        while (j >= 0 && vals[order_out[j]] < cc) {
            order_out[j + 1] = order_out[j];
            j--;
        }
        order_out[j + 1] = cur;
    }
}

/* ---------- 持久化 ---------- */

/* 解析饱和十进制（纯 ASCII 数字，钳到 cap）。合法返回 1。 */
static int uf_parse_uint(const char *s, size_t len, uint32_t cap, uint32_t *out) {
    if (len == 0) {
        return 0;
    }
    uint32_t v = 0;
    for (size_t i = 0; i < len; i++) {
        char c = s[i];
        if (c < '0' || c > '9') {
            return 0;
        }
        uint32_t d = (uint32_t)(c - '0');
        v = (v > (cap - d) / 10) ? cap : v * 10 + d;
    }
    *out = v;
    return 1;
}

int cj_uf_load(CjUserFreq *uf, const char *path) {
    cj_uf_init(uf);
    FILE *f = fopen(path, "rb");
    if (!f) {
        return 1; /* 文件不存在=空表，不是错误 */
    }
    enum { UF_LOAD_BUF = 1 << 20 };
    char *buf = malloc(UF_LOAD_BUF);
    if (!buf) {
        fclose(f);
        return 0;
    }
    size_t got = fread(buf, 1, UF_LOAD_BUF, f);
    int truncated = !feof(f);
    fclose(f);

    size_t pos = 0;
    while (pos < got) {
        char *nl = memchr(buf + pos, '\n', got - pos);
        size_t line_len = nl ? (size_t)(nl - (buf + pos)) : got - pos;
        char *line = buf + pos;
        pos += line_len + (nl ? 1 : 0);
        if (!nl && truncated) {
            break; /* 文件被截断读入时丢弃最后的半行 */
        }
        if (line_len == 0) {
            continue;
        }
        if (line[0] == '#') {
            /* "#gen\t<数字>" → gen（无效则忽略该行） */
            char *t = memchr(line, '\t', line_len);
            if (t && (size_t)(t - line) == 4 && memcmp(line, "#gen", 4) == 0) {
                uint32_t g = 0;
                if (uf_parse_uint(t + 1, line_len - 5, CJ_UF_GEN_MAX, &g) && g > uf->gen) {
                    uf->gen = g;
                }
            }
            continue;
        }
        /* 拆字段：count \t key \t word [\t last] */
        char *t1 = memchr(line, '\t', line_len);
        if (!t1) {
            continue;
        }
        size_t cnt_len = (size_t)(t1 - line);
        char *t2 = memchr(t1 + 1, '\t', line_len - cnt_len - 1);
        if (!t2) {
            continue;
        }
        size_t key_len = (size_t)(t2 - (t1 + 1));
        size_t rest_len = line_len - cnt_len - 1 - key_len - 1;
        char *t3 = memchr(t2 + 1, '\t', rest_len);

        uint32_t cnt = 0;
        if (!uf_parse_uint(line, cnt_len, CJ_UF_COUNT_CAP, &cnt) || cnt == 0) {
            continue;
        }
        if (!t3) {
            /* v1 旧格式 3 字段：last=0 无损迁移 */
            uf_record_at(uf, t1 + 1, key_len, t2 + 1, rest_len, cnt, 0, 1);
        } else {
            size_t word_len = (size_t)(t3 - (t2 + 1));
            size_t last_len = rest_len - word_len - 1;
            if (memchr(t3 + 1, '\t', last_len)) {
                continue; /* 第 5 段 → 跳过 */
            }
            uint32_t last = 0;
            if (!uf_parse_uint(t3 + 1, last_len, CJ_UF_GEN_MAX, &last)) {
                continue;
            }
            if (last > uf->gen) {
                uf->gen = last; /* 收尾对齐：gen = max(头部, 所有 last) */
            }
            uf_record_at(uf, t1 + 1, key_len, t2 + 1, word_len, cnt, last, 1);
        }
    }
    free(buf);
    uf->dirty = 0; /* 刚加载的状态视为与磁盘一致 */
    return 1;
}

int cj_uf_save(CjUserFreq *uf, const char *path) {
    char tmp[512];
    int n = snprintf(tmp, sizeof(tmp), "%s.tmp", path);
    if (n < 0 || (size_t)n >= sizeof(tmp)) {
        return 0;
    }
    FILE *f = fopen(tmp, "wb");
    if (!f) {
        return 0;
    }
    int ok = fprintf(f, "#gen\t%u\n", uf->gen) >= 0;
    for (int i = 0; i < uf->count && ok; i++) {
        const CjUfEntry *e = &uf->entries[i];
        if (fprintf(f, "%u\t", e->count) < 0 ||
            fwrite(e->key, 1, e->key_len, f) != e->key_len ||
            fputc('\t', f) == EOF ||
            fwrite(e->word, 1, e->word_len, f) != e->word_len ||
            fprintf(f, "\t%u\n", e->last) < 0) {
            ok = 0;
        }
    }
    if (fclose(f) != 0) {
        ok = 0;
    }
    if (!ok || rename(tmp, path) != 0) {
        remove(tmp);
        return 0;
    }
    uf->dirty = 0;
    return 1;
}
