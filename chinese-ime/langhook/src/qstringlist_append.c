#include "qstringlist_append.h"
#include <string.h>
#include <stdio.h>

/* QArrayData::AllocationOption，跟 Qt 公开头文件 qarraydata.h 里的枚举顺序一致：
 * enum AllocationOption { Grow, KeepSize };  Grow=0, KeepSize=1
 * 我们只做一次性精确容量分配，不需要增长余量，用 KeepSize。 */
#define CJ_QARRAYDATA_KEEPSIZE 1

#define ELEMENT_SIZE 24  /* sizeof(QString) == sizeof(QStringList 的元素) */
#define ELEMENT_ALIGN 16 /* alignof(AlignmentDummy)，Qt 头文件里核实过 */

#define CHAR16_SIZE 2  /* sizeof(char16_t) */
#define CHAR16_ALIGN 2 /* alignof(char16_t) */

/* 排查真机崩溃时加的诊断日志——每一步都 fflush，哪怕接下来立刻崩溃，这一步
 * 之前的日志也已经落盘到 journal 里，不会因为 stderr 缓冲丢失。 */
#define CJ_LOG(...) do { fprintf(stderr, __VA_ARGS__); fflush(stderr); } while (0)

/* 手动构造一个新的 QString（24 字节，写进 out），只用已经反复验证过的
 * QArrayData::allocate，不调用 fromUtf8——见头文件顶部的踩坑记录。
 *
 * Step T：原来是文件内部 static 函数（只给 cj_stringlist_append_utf8 自己
 * 用），现在头文件里也公开了同名函数供外部直接构造单个运行时 ASCII 内容的
 * QString（拼音缓冲区场景），所以去掉 static，直接就是那个公开实现，不搞
 * 两份重复代码。 */
int cj_build_ascii_qstring(uint8_t out[ELEMENT_SIZE], const char *ascii_str, long long len,
                            cj_qarraydata_allocate_fn allocate_fn) {
    /* Step T 加固：这个函数原来是 static，唯一调用方 cj_stringlist_append_utf8
     * 已经在自己开头做过一次空指针检查，所以这里从来不需要自己再查一遍——
     * 现在公开成独立 API，调用方（包括这次新增的 cj_build_qstringlist_from_ascii_array
     * 和 hook_init.c 里的新代码）不再保证一定先查过，必须自己把关，否则
     * allocate_fn 为 NULL 时下面会直接调用空函数指针，段错误（单测踩过这个坑：
     * 加了这段之前 make test 真的崩了）。 */
    if (!out || !ascii_str || !allocate_fn) {
        CJ_LOG("[cangjie]   append: cj_build_ascii_qstring 参数为空，直接返回失败\n");
        return 0;
    }

    void *str_header = NULL;
    CJ_LOG("[cangjie]   append: 手动构造新 QString，调用 allocate(objSize=%d align=%d capacity=%lld)...\n",
           CHAR16_SIZE, CHAR16_ALIGN, len);
    void *str_data = allocate_fn(&str_header, CHAR16_SIZE, CHAR16_ALIGN, len,
                                  CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   append: 字符缓冲 allocate 返回 header=%p data=%p\n", str_header, str_data);
    if (!str_data || !str_header) {
        CJ_LOG("[cangjie]   append: 字符缓冲分配失败\n");
        return 0;
    }

    uint16_t *chars = (uint16_t *)str_data;
    for (long long i = 0; i < len; i++) {
        chars[i] = (uint16_t)(unsigned char)ascii_str[i]; /* ASCII 零扩展成 UTF-16 code unit */
    }
    CJ_LOG("[cangjie]   append: ASCII -> UTF-16 转换完成\n");

    memcpy(out + 0, &str_header, sizeof(void *));
    memcpy(out + 8, &str_data, sizeof(void *));
    memcpy(out + 16, &len, sizeof(long long));
    return 1;
}

int cj_stringlist_append_utf8(uint8_t list[24], const char *ascii_str, long long len,
                               cj_qarraydata_allocate_fn allocate_fn) {
    CJ_LOG("[cangjie]   append: 进入函数, list=%p len=%lld\n", (void *)list, len);

    if (!allocate_fn || !list || !ascii_str) {
        CJ_LOG("[cangjie]   append: 参数为空，直接返回失败\n");
        return 0;
    }

    /* 读出旧的 {d, ptr, size} 三元组（QStringList 自己的，不是里面每个元素的） */
    void *old_d, *old_ptr;
    long long old_size;
    memcpy(&old_d, list + 0, sizeof(void *));
    memcpy(&old_ptr, list + 8, sizeof(void *));
    memcpy(&old_size, list + 16, sizeof(long long));
    CJ_LOG("[cangjie]   append: 旧列表 d=%p ptr=%p size=%lld\n", old_d, old_ptr, old_size);

    if (old_size < 0 || old_size > 10000) { /* 防御性上限，10000 肯定不合理 */
        CJ_LOG("[cangjie]   append: old_size 看起来不合理（%lld），放弃，不碰任何内存\n", old_size);
        return 0;
    }

    long long new_size = old_size + 1;

    void *new_header = NULL;
    CJ_LOG("[cangjie]   append: 调用 QArrayData::allocate(objSize=%d align=%d capacity=%lld)...\n",
           ELEMENT_SIZE, ELEMENT_ALIGN, new_size);
    void *new_data = allocate_fn(&new_header, ELEMENT_SIZE, ELEMENT_ALIGN, new_size,
                                  CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   append: allocate 返回 new_header=%p new_data=%p\n", new_header, new_data);
    if (!new_data || !new_header) {
        CJ_LOG("[cangjie]   append: allocate 返回空指针，放弃\n");
        return 0;
    }

    uint8_t *new_bytes = (uint8_t *)new_data;
    const uint8_t *old_bytes = (const uint8_t *)old_ptr;

    /* 逐个元素浅拷贝旧数据，并对每个元素自己的内部 Data* 引用计数 +1
     * （对应 Qt QArrayData::ref() 的 relaxed 原子自增语义）。 */
    for (long long i = 0; i < old_size; i++) {
        memcpy(new_bytes + i * ELEMENT_SIZE, old_bytes + i * ELEMENT_SIZE, ELEMENT_SIZE);

        void *elem_d;
        memcpy(&elem_d, new_bytes + i * ELEMENT_SIZE, sizeof(void *));
        CJ_LOG("[cangjie]   append: 元素[%lld] 拷贝完成, 自己的 d=%p\n", i, elem_d);
        if (elem_d != NULL) {
            __atomic_fetch_add((int *)elem_d, 1, __ATOMIC_RELAXED);
            CJ_LOG("[cangjie]   append: 元素[%lld] 引用计数 +1 完成\n", i);
        }
    }

    /* 构造新元素——不调用 fromUtf8，只用已经证明可靠的 allocate，见头文件
     * 顶部和本文件里 build_ascii_qstring() 的说明。 */
    uint8_t new_elem[ELEMENT_SIZE];
    if (!cj_build_ascii_qstring(new_elem, ascii_str, len, allocate_fn)) {
        CJ_LOG("[cangjie]   append: 新元素构造失败，放弃（不写回，list 保持原样）\n");
        return 0;
    }
    memcpy(new_bytes + old_size * ELEMENT_SIZE, new_elem, ELEMENT_SIZE);
    CJ_LOG("[cangjie]   append: 新元素已放入最后一个槽位\n");

    /* 用新的 {d, ptr, size} 覆盖传入的 QStringList——旧的后备数组故意不释放，
     * 见头文件注释里的取舍说明。 */
    memcpy(list, &new_header, sizeof(void *));
    memcpy(list + 8, &new_data, sizeof(void *));
    memcpy(list + 16, &new_size, sizeof(long long));
    CJ_LOG("[cangjie]   append: 写回完成, 新 size=%lld，函数返回成功\n", new_size);

    return 1;
}

/* Step I：跟 build_ascii_qstring 是姐妹函数，唯一区别是直接 memcpy 调用方
 * 已经编码好的 UTF-16 code unit（不做零扩展），用于写死的中文显示字符串。 */
int cj_build_utf16_literal_qstring(uint8_t out[ELEMENT_SIZE], const uint16_t *utf16_str, long long len,
                                    cj_qarraydata_allocate_fn allocate_fn) {
    if (!out || !utf16_str || !allocate_fn) {
        CJ_LOG("[cangjie]   literal: 参数为空，直接返回失败\n");
        return 0;
    }

    void *str_header = NULL;
    CJ_LOG("[cangjie]   literal: 调用 allocate(objSize=%d align=%d capacity=%lld) 构造字面量 QString...\n",
           CHAR16_SIZE, CHAR16_ALIGN, len);
    void *str_data = allocate_fn(&str_header, CHAR16_SIZE, CHAR16_ALIGN, len,
                                  CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   literal: allocate 返回 header=%p data=%p\n", str_header, str_data);
    if (!str_data || !str_header) {
        CJ_LOG("[cangjie]   literal: 字符缓冲分配失败\n");
        return 0;
    }

    memcpy(str_data, utf16_str, (size_t)(len * (long long)CHAR16_SIZE));

    memcpy(out + 0, &str_header, sizeof(void *));
    memcpy(out + 8, &str_data, sizeof(void *));
    memcpy(out + 16, &len, sizeof(long long));
    CJ_LOG("[cangjie]   literal: 构造完成\n");
    return 1;
}

/* Step P：从零构造一个全新的 QStringList（不是追加到已有列表）。跟
 * cj_stringlist_append_utf8 的后半段几乎一样，只是没有"先拷贝旧元素、旧
 * 元素引用计数 +1"那一步——这里从来没有旧元素。 */
int cj_build_qstringlist_from_utf16_array(uint8_t out_list[ELEMENT_SIZE],
                                           const uint16_t *const *utf16_strs,
                                           const long long *lens,
                                           long long count,
                                           cj_qarraydata_allocate_fn allocate_fn) {
    CJ_LOG("[cangjie]   buildlist: 进入函数, count=%lld\n", count);

    if (!allocate_fn || !out_list || (count > 0 && (!utf16_strs || !lens))) {
        CJ_LOG("[cangjie]   buildlist: 参数为空，直接返回失败\n");
        return 0;
    }
    if (count < 0 || count > 1000) { /* 防御性上限，测试候选不可能超过这个量级 */
        CJ_LOG("[cangjie]   buildlist: count 看起来不合理（%lld），放弃\n", count);
        return 0;
    }

    /* count==0：空列表，不调用 allocate——真机踩过的坑：真实
     * QArrayData::allocate(capacity=0) 返回空指针（跟本机单测用的假
     * allocate 行为不一样，那个对 calloc(0,...) 碰巧返回非空指针，掩盖了
     * 这个差异），如果不特判会被上面"allocate 返回空指针就失败"的检查误判
     * 成失败。Qt 自己对隐式共享容器的"空/默认构造"状态就是
     * {d=nullptr,ptr=nullptr,size=0}，不需要真的分配一块 0 大小的内存，
     * 直接写这个三元组即可，语义上更贴近 Qt 原生行为。 */
    if (count == 0) {
        memset(out_list, 0, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   buildlist: count==0，写入空列表{NULL,NULL,0}，不调用 allocate\n");
        return 1;
    }

    void *new_header = NULL;
    CJ_LOG("[cangjie]   buildlist: 调用 QArrayData::allocate(objSize=%d align=%d capacity=%lld)...\n",
           ELEMENT_SIZE, ELEMENT_ALIGN, count);
    void *new_data = allocate_fn(&new_header, ELEMENT_SIZE, ELEMENT_ALIGN, count,
                                  CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   buildlist: allocate 返回 new_header=%p new_data=%p\n", new_header, new_data);
    if (!new_data || !new_header) {
        CJ_LOG("[cangjie]   buildlist: allocate 返回空指针，放弃\n");
        return 0;
    }

    uint8_t *new_bytes = (uint8_t *)new_data;
    for (long long i = 0; i < count; i++) {
        uint8_t elem[ELEMENT_SIZE];
        if (!cj_build_utf16_literal_qstring(elem, utf16_strs[i], lens[i], allocate_fn)) {
            CJ_LOG("[cangjie]   buildlist: 元素[%lld] 构造失败，放弃（不写回，out_list 保持原样）\n", i);
            return 0;
        }
        memcpy(new_bytes + i * ELEMENT_SIZE, elem, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   buildlist: 元素[%lld] 已放入槽位\n", i);
    }

    memcpy(out_list, &new_header, sizeof(void *));
    memcpy(out_list + 8, &new_data, sizeof(void *));
    memcpy(out_list + 16, &count, sizeof(long long));
    CJ_LOG("[cangjie]   buildlist: 写回完成, size=%lld，函数返回成功\n", count);
    return 1;
}

/* Step T：跟 cj_build_qstringlist_from_utf16_array 几乎一样，唯一区别是
 * 每个元素调用 cj_build_ascii_qstring（运行时 ASCII 内容）而不是
 * cj_build_utf16_literal_qstring（编译期 UTF-16 字面量）——拼音切分候选
 * 是跑起来才知道内容的字符串，不是写死的常量。 */
int cj_build_qstringlist_from_ascii_array(uint8_t out_list[ELEMENT_SIZE],
                                           const char *const *ascii_strs,
                                           const long long *lens,
                                           long long count,
                                           cj_qarraydata_allocate_fn allocate_fn) {
    CJ_LOG("[cangjie]   buildlist(ascii): 进入函数, count=%lld\n", count);

    if (!allocate_fn || !out_list || (count > 0 && (!ascii_strs || !lens))) {
        CJ_LOG("[cangjie]   buildlist(ascii): 参数为空，直接返回失败\n");
        return 0;
    }
    if (count < 0 || count > 1000) { /* 防御性上限，候选数量不可能超过这个量级 */
        CJ_LOG("[cangjie]   buildlist(ascii): count 看起来不合理（%lld），放弃\n", count);
        return 0;
    }

    /* count==0：空列表，不调用 allocate——真机踩过的坑（Step T 真机验证时
     * 发现）：真实 QArrayData::allocate(capacity=0) 返回空指针，跟本机单测
     * 用的假 allocate 行为不一样（那个对 calloc(0,...) 碰巧返回非空指针，
     * 掩盖了这个差异，导致这个 bug 在 host 单测里一直没暴露出来）。如果不
     * 特判会被下面"allocate 返回空指针就失败"的检查误判成失败，实际后果是
     * Step T 清空候选栏时 keys 属性没真的清空（visible=false 还是正确写
     * 成功了，所以候选栏依然会隐藏，只是 keys 内部留着上一次的旧内容，
     * 下次万一 visible 被意外置 true 就会闪一下旧候选）。Qt 自己对隐式共享
     * 容器的"空/默认构造"状态就是 {d=nullptr,ptr=nullptr,size=0}，不需要
     * 真的分配一块 0 大小的内存，直接写这个三元组即可。 */
    if (count == 0) {
        memset(out_list, 0, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   buildlist(ascii): count==0，写入空列表{NULL,NULL,0}，不调用 allocate\n");
        return 1;
    }

    void *new_header = NULL;
    CJ_LOG("[cangjie]   buildlist(ascii): 调用 QArrayData::allocate(objSize=%d align=%d capacity=%lld)...\n",
           ELEMENT_SIZE, ELEMENT_ALIGN, count);
    void *new_data = allocate_fn(&new_header, ELEMENT_SIZE, ELEMENT_ALIGN, count,
                                  CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   buildlist(ascii): allocate 返回 new_header=%p new_data=%p\n", new_header, new_data);
    if (!new_data || !new_header) {
        CJ_LOG("[cangjie]   buildlist(ascii): allocate 返回空指针，放弃\n");
        return 0;
    }

    uint8_t *new_bytes = (uint8_t *)new_data;
    for (long long i = 0; i < count; i++) {
        uint8_t elem[ELEMENT_SIZE];
        if (!cj_build_ascii_qstring(elem, ascii_strs[i], lens[i], allocate_fn)) {
            CJ_LOG("[cangjie]   buildlist(ascii): 元素[%lld] 构造失败，放弃（不写回，out_list 保持原样）\n", i);
            return 0;
        }
        memcpy(new_bytes + i * ELEMENT_SIZE, elem, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   buildlist(ascii): 元素[%lld] 已放入槽位\n", i);
    }

    memcpy(out_list, &new_header, sizeof(void *));
    memcpy(out_list + 8, &new_data, sizeof(void *));
    memcpy(out_list + 16, &count, sizeof(long long));
    CJ_LOG("[cangjie]   buildlist(ascii): 写回完成, size=%lld，函数返回成功\n", count);
    return 1;
}

#define CJ_UTF8_DECODE_MAX_UNITS 64 /* 词典候选词很短（最长的词条也就几个字），留足余量；超过就放弃，不做变长分配 */

/* 把 utf8_len 字节的 UTF-8 输入解码进 out（容量 out_cap 个 UTF-16 code
 * unit），支持到 4 字节序列（编码成 UTF-16 代理对——41448 大字表含不少
 * Unicode 扩展区生僻字，比如 𬇹，不能只当 BMP 内字符处理）。返回写入的
 * code unit 数。遇到明显不合法的首字节（不应该发生，数据来自我们自己
 * 生成的 dict.bin，不是不可信输入）跳过一个字节尝试重新同步，不崩溃、
 * 不越界写。 */
static size_t cj_utf8_decode_to_utf16(const char *utf8, long long utf8_len,
                                       uint16_t *out, size_t out_cap) {
    size_t i = 0, o = 0;
    while ((long long)i < utf8_len && o < out_cap) {
        unsigned char c0 = (unsigned char)utf8[i];
        uint32_t cp;
        size_t n;
        if (c0 < 0x80) {
            cp = c0;
            n = 1;
        } else if ((c0 & 0xE0) == 0xC0 && (long long)(i + 1) < utf8_len) {
            cp = ((uint32_t)(c0 & 0x1F) << 6) | ((unsigned char)utf8[i + 1] & 0x3F);
            n = 2;
        } else if ((c0 & 0xF0) == 0xE0 && (long long)(i + 2) < utf8_len) {
            cp = ((uint32_t)(c0 & 0x0F) << 12) |
                 (((unsigned char)utf8[i + 1] & 0x3F) << 6) |
                 ((unsigned char)utf8[i + 2] & 0x3F);
            n = 3;
        } else if ((c0 & 0xF8) == 0xF0 && (long long)(i + 3) < utf8_len) {
            cp = ((uint32_t)(c0 & 0x07) << 18) |
                 (((unsigned char)utf8[i + 1] & 0x3F) << 12) |
                 (((unsigned char)utf8[i + 2] & 0x3F) << 6) |
                 ((unsigned char)utf8[i + 3] & 0x3F);
            n = 4;
        } else {
            i += 1; /* 非法/截断的首字节，跳过一个字节，尝试重新同步 */
            continue;
        }
        if (cp <= 0xFFFF) {
            out[o++] = (uint16_t)cp;
        } else if (o + 1 < out_cap) {
            uint32_t v = cp - 0x10000;
            out[o++] = (uint16_t)(0xD800 + (v >> 10));
            out[o++] = (uint16_t)(0xDC00 + (v & 0x3FF));
        } else {
            break; /* 代理对写不下了，停止——候选词很短，正常不应该触发 */
        }
        i += n;
    }
    return o;
}

/* Step V：跟 cj_build_ascii_qstring 同一个 QArrayData::allocate 路线，
 * 区别是真正做 UTF-8 -> UTF-16 解码，不是零扩展——候选词是真中文，不是
 * 拼音字母。 */
int cj_build_utf8_qstring(uint8_t out[ELEMENT_SIZE], const char *utf8_str, long long utf8_len,
                           cj_qarraydata_allocate_fn allocate_fn) {
    if (!out || !utf8_str || !allocate_fn) {
        CJ_LOG("[cangjie]   utf8: 参数为空，直接返回失败\n");
        return 0;
    }
    if (utf8_len < 0 || utf8_len > 4096) { /* 候选词不可能这么长，防御性上限 */
        CJ_LOG("[cangjie]   utf8: utf8_len 看起来不合理（%lld），放弃\n", utf8_len);
        return 0;
    }

    uint16_t decoded[CJ_UTF8_DECODE_MAX_UNITS];
    size_t unit_count = cj_utf8_decode_to_utf16(utf8_str, utf8_len, decoded, CJ_UTF8_DECODE_MAX_UNITS);
    if (utf8_len > 0 && unit_count == 0) {
        CJ_LOG("[cangjie]   utf8: UTF-8 解码结果为空（输入非空 %lld 字节），放弃\n", utf8_len);
        return 0;
    }
    long long len16 = (long long)unit_count;

    /* len16==0：空字符串——real QArrayData::allocate(capacity=0) 真机返回
     * 空指针（Step T 真机验证时踩过的同一个坑，见 cj_build_qstringlist_from_ascii_array
     * 的注释），不调用 allocate，直接写 Qt 原生"空/默认构造"的 {NULL,NULL,0}。
     * 词典候选词理论上不会是空字符串，这里只是防御性处理，不预期真的触发。 */
    if (len16 == 0) {
        memset(out, 0, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   utf8: 解码结果为空字符串，写入{NULL,NULL,0}，不调用 allocate\n");
        return 1;
    }

    void *str_header = NULL;
    CJ_LOG("[cangjie]   utf8: 调用 allocate(objSize=%d align=%d capacity=%lld) 构造 UTF-8 解码后的 QString...\n",
           CHAR16_SIZE, CHAR16_ALIGN, len16);
    void *str_data = allocate_fn(&str_header, CHAR16_SIZE, CHAR16_ALIGN, len16, CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   utf8: allocate 返回 header=%p data=%p\n", str_header, str_data);
    if (!str_data || !str_header) {
        CJ_LOG("[cangjie]   utf8: 字符缓冲分配失败\n");
        return 0;
    }

    memcpy(str_data, decoded, (size_t)len16 * CHAR16_SIZE);

    memcpy(out + 0, &str_header, sizeof(void *));
    memcpy(out + 8, &str_data, sizeof(void *));
    memcpy(out + 16, &len16, sizeof(long long));
    CJ_LOG("[cangjie]   utf8: 构造完成（UTF-8 %lld 字节 -> UTF-16 %lld 个 code unit）\n", utf8_len, len16);
    return 1;
}

/* Step V：跟 cj_build_qstringlist_from_ascii_array 同结构，元素来源换成
 * cj_build_utf8_qstring（真中文候选词，不是拼音字母）。utf8_lens[i] 是
 * 字节数，不是字符数。 */
int cj_build_qstringlist_from_utf8_array(uint8_t out_list[ELEMENT_SIZE],
                                          const char *const *utf8_strs,
                                          const long long *utf8_lens,
                                          long long count,
                                          cj_qarraydata_allocate_fn allocate_fn) {
    CJ_LOG("[cangjie]   buildlist(utf8): 进入函数, count=%lld\n", count);

    if (!allocate_fn || !out_list || (count > 0 && (!utf8_strs || !utf8_lens))) {
        CJ_LOG("[cangjie]   buildlist(utf8): 参数为空，直接返回失败\n");
        return 0;
    }
    if (count < 0 || count > 1000) { /* 防御性上限，候选数量不可能超过这个量级 */
        CJ_LOG("[cangjie]   buildlist(utf8): count 看起来不合理（%lld），放弃\n", count);
        return 0;
    }

    /* count==0：同 ascii 版本，不调用 allocate，直接写空列表——见那边的注释。 */
    if (count == 0) {
        memset(out_list, 0, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   buildlist(utf8): count==0，写入空列表{NULL,NULL,0}，不调用 allocate\n");
        return 1;
    }

    void *new_header = NULL;
    CJ_LOG("[cangjie]   buildlist(utf8): 调用 QArrayData::allocate(objSize=%d align=%d capacity=%lld)...\n",
           ELEMENT_SIZE, ELEMENT_ALIGN, count);
    void *new_data = allocate_fn(&new_header, ELEMENT_SIZE, ELEMENT_ALIGN, count,
                                  CJ_QARRAYDATA_KEEPSIZE);
    CJ_LOG("[cangjie]   buildlist(utf8): allocate 返回 new_header=%p new_data=%p\n", new_header, new_data);
    if (!new_data || !new_header) {
        CJ_LOG("[cangjie]   buildlist(utf8): allocate 返回空指针，放弃\n");
        return 0;
    }

    uint8_t *new_bytes = (uint8_t *)new_data;
    for (long long i = 0; i < count; i++) {
        uint8_t elem[ELEMENT_SIZE];
        if (!cj_build_utf8_qstring(elem, utf8_strs[i], utf8_lens[i], allocate_fn)) {
            CJ_LOG("[cangjie]   buildlist(utf8): 元素[%lld] 构造失败，放弃（不写回，out_list 保持原样）\n", i);
            return 0;
        }
        memcpy(new_bytes + i * ELEMENT_SIZE, elem, ELEMENT_SIZE);
        CJ_LOG("[cangjie]   buildlist(utf8): 元素[%lld] 已放入槽位\n", i);
    }

    memcpy(out_list, &new_header, sizeof(void *));
    memcpy(out_list + 8, &new_data, sizeof(void *));
    memcpy(out_list + 16, &count, sizeof(long long));
    CJ_LOG("[cangjie]   buildlist(utf8): 写回完成, size=%lld，函数返回成功\n", count);
    return 1;
}

/* Step I：只读比较，跟 cj_stringlist_append_utf8 的零扩展假设互为逆操作。 */
int cj_qstring_equals_ascii(const uint8_t qstring_ptr[ELEMENT_SIZE], const char *ascii, long long len) {
    if (!qstring_ptr || !ascii) {
        return 0;
    }
    void *ptr;
    long long size;
    memcpy(&ptr, qstring_ptr + 8, sizeof(void *));
    memcpy(&size, qstring_ptr + 16, sizeof(long long));
    if (size != len || ptr == NULL) {
        return 0;
    }
    const uint16_t *chars = (const uint16_t *)ptr;
    for (long long i = 0; i < len; i++) {
        if (chars[i] != (uint16_t)(unsigned char)ascii[i]) {
            return 0;
        }
    }
    return 1;
}
