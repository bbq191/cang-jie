#include "minijson.h"

#include <string.h>

static const char *skip_ws(const char *p) {
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    return p;
}

/* p 指向开头的 '"'：返回闭合引号之后的位置；没闭合（截断）返回 NULL。任何反斜杠都连同下一个字节一起跳过，
 * 所以 \" 不会被当成结尾。 */
static const char *skip_string(const char *p) {
    for (p++; *p; p++) {
        if (*p == '\\') {
            if (!*++p) return NULL;
        } else if (*p == '"') {
            return p + 1;
        }
    }
    return NULL;
}

const char *cj_json_find(const char *json, const char *key) {
    const char *p = skip_ws(json);
    if (*p != '{') return NULL;
    size_t klen = strlen(key);
    int depth = 0;
    while (*p) {
        char c = *p;
        if (c == '"') {
            const char *s = p + 1, *e = skip_string(p);
            if (!e) return NULL;
            const char *q = skip_ws(e);
            /* 后面紧跟冒号的字符串才是键；值位置上的字符串（哪怕内容恰好等于键名）后面是逗号或右括号 */
            if (*q == ':' && depth == 1 && (size_t)(e - 1 - s) == klen && memcmp(s, key, klen) == 0)
                return skip_ws(q + 1);
            p = e;
            continue;
        }
        if (c == '{' || c == '[') {
            depth++;
        } else if (c == '}' || c == ']') {
            if (--depth <= 0) return NULL;   /* 顶层对象结束了还没找到 */
        }
        p++;
    }
    return NULL;
}

/* 字面量后面必须是分隔符，"truex" 之类不算 */
static int lit_end(char c) {
    return c == '\0' || c == ',' || c == '}' || c == ']' || c == ' ' || c == '\t' || c == '\n' || c == '\r';
}

int cj_json_get_bool(const char *json, const char *key, int *out) {
    const char *v = cj_json_find(json, key);
    if (!v) return 0;
    if (strncmp(v, "true", 4) == 0 && lit_end(v[4])) { *out = 1; return 1; }
    if (strncmp(v, "false", 5) == 0 && lit_end(v[5])) { *out = 0; return 1; }
    return 0;
}

int cj_json_get_string(const char *json, const char *key, char *out, size_t cap) {
    const char *p = cj_json_find(json, key);
    if (!p || *p++ != '"' || cap == 0) return 0;
    size_t n = 0;
    for (; *p && *p != '"'; p++) {
        char c = *p;
        if (c == '\\') {
            p++;
            if (*p != '"' && *p != '\\') return 0;
            c = *p;
        }
        if (n + 1 >= cap) return 0;
        out[n++] = c;
    }
    if (*p != '"') return 0;
    out[n] = 0;
    return 1;
}
