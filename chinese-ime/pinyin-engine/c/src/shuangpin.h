/* 双拼键位转换——C 移植版，对应 Python 端 src/shuangpin.py（拼音输入法
 * 白皮书 04 节"双拼——复用同一个引擎，只加一层键位转换"）。
 *
 * 范围收窄（04 节"本轮明确"）：只实现小鹤双拼（flypy）一种方案，不做
 * 自然码或其它 schema 的运行时可选项——跟 shuangpin_data.h 的生成范围
 * 一致。
 *
 * 跟 Python 版的实现方式不同：Python 版直接照搬 RIME schema 里的
 * algebra 正则替换规则链在运行时跑；C 版不嵌入正则引擎（新增重量级
 * 依赖，风险类别类比 05 节排除"把 OpenCC 静态链接进 .so"），改成把
 * "穷举全部 676 种 2 键组合的解码结果"整张表打印成静态数组
 * （shuangpin_data.h，见该文件生成脚本 gen_shuangpin_table.py 顶部
 * 说明），运行时纯查表——这是可行的，因为已经验证过双拼解码是逐
 * 2 键分组独立解码、组间互不影响（见生成脚本同一处说明）。
 *
 * 输出格式跟 Python 版一致：完整的 2 键组按顺序解码、音节间用 '\''
 * 分隔（跟 03 节 segment.c 的隔音符号是同一个字符、同一个语义——
 * 直接把双拼解码结果喂给 cj_best_segmentation/cj_segment，不需要
 * 专门为双拼写一套新的切分逻辑）；如果按键总数是奇数，最后一个键
 * 原样透传、不强行解码（还没打完的半个音节）。 */
#ifndef CJ_SHUANGPIN_H
#define CJ_SHUANGPIN_H

#include <stddef.h>

/* keys：双拼按键序列（纯小写 a-z，调用方保证，不做大小写归一化——
 * 跟 hook_init.c Step T 的缓冲区本身就只允许追加小写字母一致）。
 * keys_len 是字节数，不要求 NUL 结尾。
 *
 * out/out_cap：调用方提供的输出缓冲区（不做动态分配，跟本项目一贯的
 * "调用方持有缓冲区"风格一致）。写入解码后的拼音字符串（音节间用
 * ' 分隔），返回实际写入的字节数（不含 NUL，若 out_cap 足够会额外
 * 补一个 NUL 方便调用方当 C 字符串用，但返回值不把它算进去）。
 *
 * 输出超过 out_cap 时截断到 out_cap（不越界写），返回值仍然是"截断
 * 前应该写入的完整长度"，方便调用方判断是否发生了截断（模式对照
 * snprintf 的返回值语义，调用方要检查返回值是否 >= out_cap）。 */
size_t cj_shuangpin_keys_to_pinyin(const char *keys, size_t keys_len,
                                    char *out, size_t out_cap);

#endif /* CJ_SHUANGPIN_H */
