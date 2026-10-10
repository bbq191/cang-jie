#pragma once
#include <stddef.h>

/* 极简 JSON 读取：只取**顶层对象**里某个键的布尔/字符串值，不建树、不分配内存、不引 JSON 库。
 *
 * 给 xovi 扩展读自己的小配置文件用（hl-snap 读 reading-qol.json 的 hlSnapCjk，ui-font 读 ui-font.json 的 sans）。
 * 2026-10-10 前两个扩展各手写一份 strstr 扫描：遇到"某个值恰好等于键名"（如 {"last":"hlSnapCjk","hlSnapCjk":false}）
 * 会先撞上那个值、把真正的键遮住；嵌套对象里的同名键也会被当成顶层键；hl-snap 那份还不要求键后紧跟冒号。
 * 现在按字符串/括号逐个走一遍：只有"顶层对象里、后面跟冒号的字符串"才算键。
 *
 * 不校验整份 JSON 是否合法（读到所需的键就停）；键名按原始字节比较（键名里有转义的不会命中，调用方的键都是纯 ASCII）。
 * 同名键出现多次时取第一个（与旧实现一致）。
 *
 * 符号设为 hidden：hl-snap.so 与 ui-font.so 都编进这一份、又被 xovi 装进同一个 xochitl 进程，不导出就不会出现
 * "一个扩展调到另一个扩展里的同名函数"（两边版本不一致时尤其危险）。 */
#define CJ_JSON_API __attribute__((visibility("hidden")))

/* 顶层键 key 的值起点（已跳过冒号与空白）；没有该键、或 json 不是以对象开头，返回 NULL。 */
CJ_JSON_API const char *cj_json_find(const char *json, const char *key);

/* 顶层布尔。返回 1 = 拿到（*out 置 0/1）；0 = 没有该键、或值不是 true/false（*out 不动）。 */
CJ_JSON_API int cj_json_get_bool(const char *json, const char *key, int *out);

/* 顶层字符串，UTF-8 原样拷进 out（以 \0 结尾）。只认 \" 与 \\ 两种转义，遇到别的转义（如 \u）整条放弃。
 * 返回 1 = 拿到（可以是空串）；0 = 没有该键 / 不是字符串 / 截断 / 不认的转义 / out 放不下（out 内容未定义）。 */
CJ_JSON_API int cj_json_get_string(const char *json, const char *key, char *out, size_t cap);
