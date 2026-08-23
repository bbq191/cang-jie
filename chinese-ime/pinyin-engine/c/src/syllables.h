/* 合法拼音音节表——C 移植版，对应 Python 端 src/syllables.py。
 * 数据本身在 syllables_data.h（自动生成，见该文件顶部说明），这里只有
 * 查询逻辑：是否是合法音节、是否是某个合法音节的前缀。 */
#ifndef CJ_SYLLABLES_H
#define CJ_SYLLABLES_H

#include <stddef.h>

/* len 是 s 的长度（调用方已知，避免重复 strlen）。s 不需要以 '\0' 结尾
 * 也可以（用 len 界定），方便后面 segment.c 对同一个缓冲区的子串直接
 * 调用，不用每次都拷贝出一个新字符串。 */
int cj_is_valid_syllable(const char *s, size_t len);

/* s 是否可能是某个合法音节"打到一半"的前缀——用于处理用户还没打完
 * 最后一个音节的情况（比如 "zho" 是 "zhong"/"zhou"/"zhang" 的前缀）。 */
int cj_is_prefix_of_some_syllable(const char *s, size_t len);

#endif /* CJ_SYLLABLES_H */
