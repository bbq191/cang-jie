#pragma once
#include <stdint.h>
#include <stddef.h>

/* 给一个已经存在的 QStringList（24 字节 {Data* d; QString* ptr; qsizetype size;}，
 * 跟 QString 共用同一套 QArrayDataPointer<T> 布局，10.1/白皮书已经从真机实例和
 * 本机编译的 Qt6.11 头文件双重验证过这个尺寸/布局）追加一个新元素，就地覆盖
 * 传入的 24 字节。
 *
 * 踩坑记录（详见计划文件 imperative-petting-bentley.md Step G）：最初设计里
 * 新元素是调用真实的 QString::fromUtf8(QByteArrayView) 构造的，真机测试连续
 * 崩了两次——先是发现 QByteArrayView 的成员顺序 Qt6.x 上是 {size;data}（不是
 * 想当然的 {data;size}），改过来还是崩在同一个调用点，说明 fromUtf8 这条路的
 * ABI 细节比预期更难对齐。**换成完全不调用 fromUtf8**：只用 `QArrayData::allocate`
 * 这一个已经在同一条代码路径上被反复验证过、每次都成功的函数——用它分配一段
 * `char16_t` 缓冲区，自己手动把 ASCII 字符零扩展成 UTF-16（新加的语言代码
 * 只有 ASCII 字符，不需要真正的 UTF-8 解码器），再手动拼出 24 字节的 QString
 * 结构。这样整个函数只依赖一个已经证明可靠的 Qt 导出符号，不再依赖第二个
 * 没验证透的符号。
 *
 * 唯一自己实现的部分：① 把旧数组里每个元素浅拷贝到新数组后，对它们各自的
 * 内部 Data* 引用计数做一次 relaxed 原子自增（对应 Qt 自己 QArrayData::ref()
 * 的语义：`ref_.refRelaxed()`，即对 Data 头 4 字节做 +1，Qt 公开头文件
 * qarraydata.h 里逐字核对过）；② 把 ASCII 字符零扩展成 UTF-16 code unit。
 *
 * 不释放旧的后备数组——故意接受一点内存泄漏，规避"要不要正确处理旧数组引用计数
 * 递减"这个更容易出错的环节；调用频率低、单次泄漏量小，可接受（见白皮书 10.3/
 * 计划文件 Step G 的风险声明）。
 *
 * 返回 1 = 成功，0 = 失败（不修改 list，调用方应该保留原值/放弃这次追加）。
 */

typedef void *(*cj_qarraydata_allocate_fn)(void **pdata, long long objectSize,
                                            long long alignment, long long capacity,
                                            int option);

/* ascii_str 只接受纯 ASCII（新加的语言代码，比如 "zh_CN"，够用），每个字符
 * 原样零扩展成一个 UTF-16 code unit，不做真正的 UTF-8 多字节解码。 */
int cj_stringlist_append_utf8(uint8_t list[24], const char *ascii_str, long long len,
                               cj_qarraydata_allocate_fn allocate_fn);

/* Step I：跟上面用同一个 QArrayData::allocate 符号，构造一个全新、独立的
 * 24 字节 QString，内容直接来自调用方已经编码好的 UTF-16 code unit 数组
 * （不做任何转换）——用于写死的、已知内容的显示字符串，比如 Step I 里手工
 * 挑的"繁體中文（台灣）"/"繁體中文（香港）"，跟一般输入文本不同，不需要
 * 也不应该走 UTF-8 解码那条已经证明有坑的路（见上面 fromUtf8 的踩坑记录）。
 * 返回 1=成功，0=失败（out 不修改）。 */
int cj_build_utf16_literal_qstring(uint8_t out[24], const uint16_t *utf16_str, long long len,
                                    cj_qarraydata_allocate_fn allocate_fn);

/* 只读比较：qstring_ptr 指向的 24 字节 QString 内容是否等于 ascii——逐
 * UTF-16 code unit 跟 ascii 的每个字节比较（跟 cj_stringlist_append_utf8
 * 的零扩展是同一种编码假设的反向操作），不修改任何内存，qstring_ptr 为
 * NULL 或长度/内容不匹配都返回 0。 */
int cj_qstring_equals_ascii(const uint8_t qstring_ptr[24], const char *ascii, long long len);

/* Step P：从零构造一个全新、独立的 24 字节 QStringList（不是往已有列表追加，
 * 见 hook_init.c Step P 段落——KeyPopup.keys 属性写入需要一个完整的新列表，
 * 不是在某个已有 QStringList 副本上追加）。跟 cj_stringlist_append_utf8
 * 几乎同一套逻辑（同一个 QArrayData::allocate 符号、同一个"每个元素调用
 * cj_build_utf16_literal_qstring 填入"手法），区别只是没有"先拷贝旧元素、
 * 旧元素引用计数 +1"那一步——这里是全新分配，没有旧元素要保留。
 *
 * utf16_strs[i]/lens[i] 一一对应，count 个元素；返回 1=成功（out_list 写入
 * 新的 {d,ptr,size}），0=失败（out_list 不修改）。 */
int cj_build_qstringlist_from_utf16_array(uint8_t out_list[24],
                                           const uint16_t *const *utf16_strs,
                                           const long long *lens,
                                           long long count,
                                           cj_qarraydata_allocate_fn allocate_fn);

/* Step T：跟 cj_build_utf16_literal_qstring 是姐妹函数，唯一区别是接受
 * ASCII 字符内容并做零扩展（不是已经编码好的 UTF-16 code unit）——用于
 * 运行时才知道具体内容的字符串（比如拼音缓冲区当前打的字母），跟 Step G
 * 里 cj_stringlist_append_utf8 内部已经用过的手法是同一个，这里单独暴露
 * 出来给调用方直接构造单个 QString 用。返回 1=成功，0=失败（out 不修改）。 */
int cj_build_ascii_qstring(uint8_t out[24], const char *ascii_str, long long len,
                            cj_qarraydata_allocate_fn allocate_fn);

/* Step T：跟 cj_build_qstringlist_from_utf16_array 同结构，只是元素来源换成
 * 上面这个 ascii 版本——用于从拼音切分结果（运行时才知道内容的候选字符串）
 * 组一个全新 QStringList 写进 KeyPopup.keys。ascii_strs[i]/lens[i] 一一
 * 对应，count 个元素；返回 1=成功，0=失败（out_list 不修改）。 */
int cj_build_qstringlist_from_ascii_array(uint8_t out_list[24],
                                           const char *const *ascii_strs,
                                           const long long *lens,
                                           long long count,
                                           cj_qarraydata_allocate_fn allocate_fn);

/* Step V：把运行时才知道内容的 UTF-8 字节串（词典候选词，真的是中文，
 * 不是拼音字母）构造成一个全新、独立的 24 字节 QString——跟
 * cj_build_ascii_qstring 同一个 QArrayData::allocate 路线，区别是真正
 * 做 UTF-8 -> UTF-16 解码（支持到 4 字节 UTF-8 序列，编码成 UTF-16
 * 代理对——词典里 41448 大字表含不少 Unicode 扩展区生僻字，比如 𬇹，
 * 不能只当 BMP 内字符处理，那样会截断/编码错误），不是简单零扩展。
 * utf8_len 是输入字节数（不是字符数）。返回 1=成功，0=失败（out 不
 * 修改）。 */
int cj_build_utf8_qstring(uint8_t out[24], const char *utf8_str, long long utf8_len,
                           cj_qarraydata_allocate_fn allocate_fn);

/* Step V：跟 cj_build_qstringlist_from_ascii_array 同结构，元素来源换成
 * 上面的 UTF-8 版本——用于把词典查询结果（真候选词）组一个全新
 * QStringList 写进 KeyPopup.keys。utf8_strs[i]/utf8_lens[i]（字节数，
 * 不是字符数）一一对应，count 个元素。返回 1=成功，0=失败（out_list
 * 不修改）。 */
int cj_build_qstringlist_from_utf8_array(uint8_t out_list[24],
                                          const char *const *utf8_strs,
                                          const long long *utf8_lens,
                                          long long count,
                                          cj_qarraydata_allocate_fn allocate_fn);
