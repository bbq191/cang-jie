/* 快捷输入（text replacement，iOS 同款语义）——C 移植版，Python 参照
 * 是 src/snippets.py（合法性/容量/顺序规则钉死在那边 docstring），差分
 * 测试 tests/diff_check_snippets.py 逐字节比对 parse+lookup。
 *
 * 用法（hook 侧）：cj_sn_refresh() 每次候选刷新时调一次——内部 stat
 * mtime+size，文件没变直接返回（一次 stat 的开销），变了才重新解析
 * （热重载：用户 host 侧改完 snippets.tsv，下一次打字即生效，不重启）。
 * cj_sn_lookup() 按原始字母缓冲区精确匹配，命中的短语按文件序返回。
 * 输入法对文件**只读**，永不写入。
 *
 * 内存模型：定长静态数组（~74KB），无 malloc，fail-safe（文件不存在/
 * 读失败=空表，照常打字）。单线程假设同 userfreq。 */
#ifndef CJ_SNIPPETS_H
#define CJ_SNIPPETS_H

#include <stddef.h>
#include <stdint.h>

#define CJ_SN_MAX_ENTRIES 256
#define CJ_SN_SHORTCUT_MAX 32
#define CJ_SN_PHRASE_MAX 256

typedef struct {
    char shortcut[CJ_SN_SHORTCUT_MAX]; /* 小写字母，不 NUL 结尾 */
    char phrase[CJ_SN_PHRASE_MAX];     /* 任意单行 UTF-8，不 NUL 结尾 */
    uint8_t shortcut_len;
    uint16_t phrase_len;
} CjSnEntry;

typedef struct {
    CjSnEntry entries[CJ_SN_MAX_ENTRIES]; /* 文件顺序 */
    int count;
    /* 热重载状态：上次成功加载时的文件标识（mtime 秒 + 大小）。
     * loaded=0 表示还没试过。 */
    int loaded;
    long long mtime;
    long long size;
} CjSnippets;

/* 清空为可用的空表（热重载状态一并复位）。 */
void cj_sn_init(CjSnippets *sn);

/* 纯解析（差分测试入口）：从内存内容解析（先清空 entries，不碰热重载
 * 状态）。规则见 src/snippets.py docstring。 */
void cj_sn_parse(CjSnippets *sn, const char *data, size_t data_len);

/* 带热重载的加载：stat(path) 对比 mtime+size，没变直接返回；变了（或
 * 首次）就读文件重新解析。文件不存在/读失败=空表（fail-safe）。 */
void cj_sn_refresh(CjSnippets *sn, const char *path);

/* buffer 精确等于 shortcut 的全部条目（文件序），最多 max_out 条，
 * out 存内部条目指针（下一次 parse/refresh 前有效）。返回条数。 */
int cj_sn_lookup(const CjSnippets *sn, const char *buffer, size_t buffer_len,
                 const CjSnEntry **out, int max_out);

#endif /* CJ_SNIPPETS_H */
