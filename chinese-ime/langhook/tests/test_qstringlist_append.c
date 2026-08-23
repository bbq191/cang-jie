/* qstringlist_append.c 的宿主机单元测试。用假的 QArrayData::allocate 实现
 * （malloc 支撑）代替真实 Qt 函数，验证追加逻辑本身的簿记是否正确：旧元素
 * 有没有被正确浅拷贝、每个旧元素的引用计数有没有被恰好 +1 一次（不多不少）、
 * 新元素有没有被正确放进最后一个槽位（ASCII 正确零扩展成 UTF-16）、size
 * 字段有没有正确变成 old+1。
 *
 * 不测试真实 Qt 调用本身对不对——那部分只能在真机上验证（见白皮书 10.3/
 * 计划文件 Step G 的踩坑记录：最初设计里用 QString::fromUtf8 构造新元素，
 * 真机连续崩了两次，换成现在这版"只用已验证过的 allocate + 手动零扩展
 * ASCII"之后才稳定），这里只保证"如果 allocate 行为符合 Qt 文档描述，
 * 我们自己的簿记逻辑不会出错"。
 */
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../src/qstringlist_append.h"

#define ELEMENT_SIZE 24

/* 假的 QArrayData::allocate：忽略 alignment（测试不关心），直接 calloc 一块
 * capacity*objectSize 的区域（清零，方便观察）；header 用一个独立小块内存
 * 代表，初始化成 0（模拟 Qt 真实 allocate() 会把新分配的引用计数设为可用
 * 状态，这里只用来判断"非空"，不模拟精确的初始 refcount 数值）。 */
static void *fake_allocate(void **pdata, long long objectSize, long long alignment,
                            long long capacity, int option) {
    (void)alignment;
    (void)option;
    int *header = malloc(sizeof(int));
    *header = 1;
    *pdata = header;
    return calloc((size_t)capacity, (size_t)objectSize);
}

/* 测试专用小工具：直接用 fake_allocate 手工拼一个 ASCII 内容的 24 字节
 * QString（跟 qstringlist_append.c 内部 static 的 build_ascii_qstring 逻辑
 * 一样，但那个是文件内部符号，测试这边独立写一份，只用来喂给
 * cj_qstring_equals_ascii 做比较测试，不测 allocate 本身）。 */
static int build_ascii_qstring_for_test(uint8_t out[ELEMENT_SIZE], const char *ascii_str, long long len) {
    void *header = NULL;
    void *data = fake_allocate(&header, 2, 2, len, 1);
    if (!data || !header) return 0;
    uint16_t *chars = (uint16_t *)data;
    for (long long i = 0; i < len; i++) {
        chars[i] = (uint16_t)(unsigned char)ascii_str[i];
    }
    memcpy(out + 0, &header, sizeof(void *));
    memcpy(out + 8, &data, sizeof(void *));
    memcpy(out + 16, &len, sizeof(long long));
    return 1;
}

/* 构造一个假的旧 QStringList：N 个元素，每个元素有独立的假 refcount（初值 1）。 */
static void build_fake_old_list(uint8_t list[24], int n, int **out_refcounts) {
    uint8_t *elems = calloc((size_t)n, ELEMENT_SIZE);
    for (int i = 0; i < n; i++) {
        out_refcounts[i] = malloc(sizeof(int));
        *out_refcounts[i] = 1;
        void *fake_ptr = (void *)(intptr_t)(0x1000 + i); /* 假的字符数据指针，随便给个非空值 */
        long long fake_size = 2;
        memcpy(elems + i * ELEMENT_SIZE + 0, &out_refcounts[i], sizeof(void *));
        memcpy(elems + i * ELEMENT_SIZE + 8, &fake_ptr, sizeof(void *));
        memcpy(elems + i * ELEMENT_SIZE + 16, &fake_size, sizeof(long long));
    }
    void *header = malloc(16);
    memcpy(list + 0, &header, sizeof(void *));
    memcpy(list + 8, &elems, sizeof(void *));
    long long n_ll = n;
    memcpy(list + 16, &n_ll, sizeof(long long));
}

static void test_append_to_nonempty_list(void) {
    uint8_t list[24];
    int *refcounts[3];
    build_fake_old_list(list, 3, refcounts);

    int ok = cj_stringlist_append_utf8(list, "zh_CN", 5, fake_allocate);
    assert(ok == 1);

    long long new_size;
    void *new_ptr;
    memcpy(&new_ptr, list + 8, sizeof(void *));
    memcpy(&new_size, list + 16, sizeof(long long));
    assert(new_size == 4); /* 3 + 1 */

    /* 旧的 3 个元素每个引用计数应该恰好 +1（从 1 变成 2） */
    for (int i = 0; i < 3; i++) {
        assert(*refcounts[i] == 2);
    }

    /* 新元素（第 4 个槽位）应该是我们塞进去的 "zh_CN"——手动零扩展成 UTF-16，
     * 不是原始字节，所以按 uint16_t 数组比较，不是 memcmp 原始字节。 */
    uint8_t *new_bytes = (uint8_t *)new_ptr;
    void *last_d, *last_ptr;
    long long last_size;
    memcpy(&last_d, new_bytes + 3 * ELEMENT_SIZE + 0, sizeof(void *));
    memcpy(&last_ptr, new_bytes + 3 * ELEMENT_SIZE + 8, sizeof(void *));
    memcpy(&last_size, new_bytes + 3 * ELEMENT_SIZE + 16, sizeof(long long));
    assert(last_d != NULL);
    assert(last_ptr != NULL);
    assert(last_size == 5);

    const uint16_t *utf16 = (const uint16_t *)last_ptr;
    const char *expected = "zh_CN";
    for (int i = 0; i < 5; i++) {
        assert(utf16[i] == (uint16_t)(unsigned char)expected[i]);
    }

    printf("test_append_to_nonempty_list: OK\n");
}

static void test_append_to_empty_list(void) {
    uint8_t list[24];
    void *header = malloc(16);
    void *empty_ptr = NULL;
    long long zero = 0;
    memcpy(list + 0, &header, sizeof(void *));
    memcpy(list + 8, &empty_ptr, sizeof(void *));
    memcpy(list + 16, &zero, sizeof(long long));

    int ok = cj_stringlist_append_utf8(list, "zh_CN", 5, fake_allocate);
    assert(ok == 1);

    long long new_size;
    memcpy(&new_size, list + 16, sizeof(long long));
    assert(new_size == 1);
    printf("test_append_to_empty_list: OK\n");
}

/* 模拟 hook_init.c 的 Step G+ 循环：在同一个 dest 上连续调用三次追加
 * （"zh_CN"/"zh_TW"/"zh_HK"），跟真机场景一致——每次都读上一次调用刚写回
 * 的 {d,ptr,size}，验证连续调用本身不会互相干扰、size 每次正好 +1、
 * 三个新元素内容都对、原始元素的引用计数按调用次数正确累加（第 1 次调用后
 * +1，第 2 次调用时它们也在"旧数组"里所以再 +1，第 3 次同理，最终应该是
 * 初始值 +3）。 */
static void test_append_multiple_sequential(void) {
    uint8_t list[24];
    int *refcounts[4]; /* 模拟真实设备上 en/es/de/fr 四个初始语言 */
    build_fake_old_list(list, 4, refcounts);

    const char *codes[3] = {"zh_CN", "zh_TW", "zh_HK"};
    for (int i = 0; i < 3; i++) {
        int ok = cj_stringlist_append_utf8(list, codes[i], 5, fake_allocate);
        assert(ok == 1);
    }

    long long new_size;
    void *new_ptr;
    memcpy(&new_ptr, list + 8, sizeof(void *));
    memcpy(&new_size, list + 16, sizeof(long long));
    assert(new_size == 7); /* 4 + 3 */

    for (int i = 0; i < 4; i++) {
        assert(*refcounts[i] == 4); /* 初始 1，三次调用各 +1 */
    }

    uint8_t *bytes = (uint8_t *)new_ptr;
    for (int i = 0; i < 3; i++) {
        void *elem_ptr;
        long long elem_size;
        memcpy(&elem_ptr, bytes + (4 + i) * ELEMENT_SIZE + 8, sizeof(void *));
        memcpy(&elem_size, bytes + (4 + i) * ELEMENT_SIZE + 16, sizeof(long long));
        assert(elem_size == 5);
        const uint16_t *utf16 = (const uint16_t *)elem_ptr;
        for (int j = 0; j < 5; j++) {
            assert(utf16[j] == (uint16_t)(unsigned char)codes[i][j]);
        }
    }

    printf("test_append_multiple_sequential: OK\n");
}

static void test_null_arguments_rejected(void) {
    uint8_t list[24] = {0};
    int ok = cj_stringlist_append_utf8(list, "zh_CN", 5, NULL);
    assert(ok == 0);
    ok = cj_stringlist_append_utf8(list, NULL, 5, fake_allocate);
    assert(ok == 0);
    printf("test_null_arguments_rejected: OK\n");
}

/* Step I：build_utf16_literal_qstring——直接 memcpy 已编码好的 UTF-16，
 * 内容应该原样出现，不做任何转换（用真实的"繁體中文"四个字模拟）。 */
static void test_build_utf16_literal_qstring(void) {
    static const uint16_t literal[] = {0x7e41u, 0x9ad4u, 0x4e2du, 0x6587u}; /* "繁體中文" */
    uint8_t out[ELEMENT_SIZE];

    int ok = cj_build_utf16_literal_qstring(out, literal, 4, fake_allocate);
    assert(ok == 1);

    void *ptr;
    long long size;
    memcpy(&ptr, out + 8, sizeof(void *));
    memcpy(&size, out + 16, sizeof(long long));
    assert(size == 4);
    assert(ptr != NULL);

    const uint16_t *chars = (const uint16_t *)ptr;
    for (int i = 0; i < 4; i++) {
        assert(chars[i] == literal[i]);
    }

    /* 空指针/空数组都应该被拒绝，不崩溃 */
    assert(cj_build_utf16_literal_qstring(NULL, literal, 4, fake_allocate) == 0);
    assert(cj_build_utf16_literal_qstring(out, NULL, 4, fake_allocate) == 0);
    assert(cj_build_utf16_literal_qstring(out, literal, 4, NULL) == 0);

    printf("test_build_utf16_literal_qstring: OK\n");
}

/* Step I：cj_qstring_equals_ascii——正确匹配 "zh_TW"/"zh_HK"，不误判其它
 * code、不误判长度不同的字符串、不崩溃在空指针上。 */
static void test_qstring_equals_ascii(void) {
    uint8_t qstr[ELEMENT_SIZE];
    if (!build_ascii_qstring_for_test(qstr, "zh_TW", 5)) {
        assert(0 && "测试辅助函数自己构造失败");
    }

    assert(cj_qstring_equals_ascii(qstr, "zh_TW", 5) == 1);
    assert(cj_qstring_equals_ascii(qstr, "zh_HK", 5) == 0); /* 内容不同 */
    assert(cj_qstring_equals_ascii(qstr, "zh_T", 4) == 0);  /* 长度不同 */
    assert(cj_qstring_equals_ascii(NULL, "zh_TW", 5) == 0);
    assert(cj_qstring_equals_ascii(qstr, NULL, 5) == 0);

    printf("test_qstring_equals_ascii: OK\n");
}

/* Step T：cj_build_ascii_qstring——运行时 ASCII 内容（不是编译期字面量），
 * 用拼音缓冲区场景会出现的内容（比如打到一半的 "piny"）验证零扩展正确、
 * 空指针/空数组不崩溃。 */
static void test_build_ascii_qstring(void) {
    uint8_t out[ELEMENT_SIZE];
    int ok = cj_build_ascii_qstring(out, "piny", 4, fake_allocate);
    assert(ok == 1);

    void *ptr;
    long long size;
    memcpy(&ptr, out + 8, sizeof(void *));
    memcpy(&size, out + 16, sizeof(long long));
    assert(size == 4);
    assert(ptr != NULL);

    const uint16_t *chars = (const uint16_t *)ptr;
    const char *expected = "piny";
    for (int i = 0; i < 4; i++) {
        assert(chars[i] == (uint16_t)(unsigned char)expected[i]);
    }

    assert(cj_build_ascii_qstring(NULL, "piny", 4, fake_allocate) == 0);
    assert(cj_build_ascii_qstring(out, NULL, 4, fake_allocate) == 0);
    assert(cj_build_ascii_qstring(out, "piny", 4, NULL) == 0);

    printf("test_build_ascii_qstring: OK\n");
}

/* Step T：cj_build_qstringlist_from_ascii_array——拼音切分候选场景（比如
 * "pin yin"/"pi nyin" 两种歧义切法），验证每个元素内容都对、size 正确、
 * count==0 时也能成功（空列表，用来在缓冲区清空时隐藏候选栏）。 */
static void test_build_qstringlist_from_ascii_array(void) {
    const char *candidates[2] = {"pin yin", "pi nyin"};
    long long lens[2] = {7, 7};
    uint8_t out_list[24];

    int ok = cj_build_qstringlist_from_ascii_array(out_list, candidates, lens, 2, fake_allocate);
    assert(ok == 1);

    void *ptr;
    long long size;
    memcpy(&ptr, out_list + 8, sizeof(void *));
    memcpy(&size, out_list + 16, sizeof(long long));
    assert(size == 2);

    uint8_t *bytes = (uint8_t *)ptr;
    for (int i = 0; i < 2; i++) {
        void *elem_ptr;
        long long elem_size;
        memcpy(&elem_ptr, bytes + i * ELEMENT_SIZE + 8, sizeof(void *));
        memcpy(&elem_size, bytes + i * ELEMENT_SIZE + 16, sizeof(long long));
        assert(elem_size == 7);
        const uint16_t *utf16 = (const uint16_t *)elem_ptr;
        for (int j = 0; j < 7; j++) {
            assert(utf16[j] == (uint16_t)(unsigned char)candidates[i][j]);
        }
    }

    /* count==0：空列表，应该成功（不是失败），size 写回 0。 */
    uint8_t empty_list[24];
    ok = cj_build_qstringlist_from_ascii_array(empty_list, NULL, NULL, 0, fake_allocate);
    assert(ok == 1);
    memcpy(&size, empty_list + 16, sizeof(long long));
    assert(size == 0);

    assert(cj_build_qstringlist_from_ascii_array(NULL, candidates, lens, 2, fake_allocate) == 0);
    assert(cj_build_qstringlist_from_ascii_array(out_list, candidates, lens, 2, NULL) == 0);

    printf("test_build_qstringlist_from_ascii_array: OK\n");
}

/* Step V：cj_build_utf8_qstring——真中文候选词场景。三个层级：① 纯 ASCII
 * （单字节，应该跟零扩展结果一样）；② 常见 BMP 内汉字（"你好"，3 字节
 * UTF-8/字，编解码不能出错）；③ BMP 外生僻字（用 U+20000 手算出的 UTF-8
 * 字节 {0xF0,0xA0,0x80,0x80}，正确结果应该是 UTF-16 代理对
 * {0xD840,0xDC00}，41448 大字表里这类字不少，编码错了会直接呈现乱码）。 */
static void test_build_utf8_qstring(void) {
    uint8_t out[ELEMENT_SIZE];

    /* ① ASCII 子集 */
    int ok = cj_build_utf8_qstring(out, "de", 2, fake_allocate);
    assert(ok == 1);
    void *ptr;
    long long size;
    memcpy(&ptr, out + 8, sizeof(void *));
    memcpy(&size, out + 16, sizeof(long long));
    assert(size == 2);
    const uint16_t *chars = (const uint16_t *)ptr;
    assert(chars[0] == 'd' && chars[1] == 'e');

    /* ② "你好"：0xE4 0xBD 0xA0 0xE5 0xA5 0xBD，6 字节，应该解码成 2 个
     * UTF-16 code unit：0x4F60（你）、0x597D（好）。 */
    static const unsigned char nihao_utf8[] = {0xE4, 0xBD, 0xA0, 0xE5, 0xA5, 0xBD};
    ok = cj_build_utf8_qstring(out, (const char *)nihao_utf8, 6, fake_allocate);
    assert(ok == 1);
    memcpy(&ptr, out + 8, sizeof(void *));
    memcpy(&size, out + 16, sizeof(long long));
    assert(size == 2); /* 2 个 UTF-16 code unit，不是 6（字节数）*/
    chars = (const uint16_t *)ptr;
    assert(chars[0] == 0x4F60u); /* 你 */
    assert(chars[1] == 0x597Du); /* 好 */

    /* ③ BMP 外字符（U+20000）：4 字节 UTF-8 -> 2 个 UTF-16 code unit（代理对）。
     * 手算：cp-0x10000=0x10000；high=0xD800+(0x10000>>10)=0xD840；
     * low=0xDC00+(0x10000&0x3FF)=0xDC00。 */
    static const unsigned char ext_char_utf8[] = {0xF0, 0xA0, 0x80, 0x80};
    ok = cj_build_utf8_qstring(out, (const char *)ext_char_utf8, 4, fake_allocate);
    assert(ok == 1);
    memcpy(&ptr, out + 8, sizeof(void *));
    memcpy(&size, out + 16, sizeof(long long));
    assert(size == 2); /* 代理对，2 个 code unit */
    chars = (const uint16_t *)ptr;
    assert(chars[0] == 0xD840u);
    assert(chars[1] == 0xDC00u);

    assert(cj_build_utf8_qstring(NULL, "de", 2, fake_allocate) == 0);
    assert(cj_build_utf8_qstring(out, NULL, 2, fake_allocate) == 0);
    assert(cj_build_utf8_qstring(out, "de", 2, NULL) == 0);

    printf("test_build_utf8_qstring: OK\n");
}

/* Step V：cj_build_qstringlist_from_utf8_array——用真实候选词场景（"你好"/
 * "拼音"两个词），验证多元素、count==0 空列表两种情况。 */
static void test_build_qstringlist_from_utf8_array(void) {
    static const unsigned char nihao[] = {0xE4, 0xBD, 0xA0, 0xE5, 0xA5, 0xBD};   /* 你好 */
    static const unsigned char pinyin[] = {0xE6, 0x8B, 0xBC, 0xE9, 0x9F, 0xB3}; /* 拼音 */
    const char *words[2] = {(const char *)nihao, (const char *)pinyin};
    long long lens[2] = {6, 6}; /* 字节数 */
    uint8_t out_list[24];

    int ok = cj_build_qstringlist_from_utf8_array(out_list, words, lens, 2, fake_allocate);
    assert(ok == 1);

    void *ptr;
    long long size;
    memcpy(&ptr, out_list + 8, sizeof(void *));
    memcpy(&size, out_list + 16, sizeof(long long));
    assert(size == 2);

    uint8_t *bytes = (uint8_t *)ptr;
    void *elem_ptr;
    long long elem_size;
    memcpy(&elem_ptr, bytes + 0 * ELEMENT_SIZE + 8, sizeof(void *));
    memcpy(&elem_size, bytes + 0 * ELEMENT_SIZE + 16, sizeof(long long));
    assert(elem_size == 2); /* "你好" -> 2 个 UTF-16 code unit */
    const uint16_t *w0 = (const uint16_t *)elem_ptr;
    assert(w0[0] == 0x4F60u && w0[1] == 0x597Du);

    memcpy(&elem_ptr, bytes + 1 * ELEMENT_SIZE + 8, sizeof(void *));
    memcpy(&elem_size, bytes + 1 * ELEMENT_SIZE + 16, sizeof(long long));
    assert(elem_size == 2); /* "拼音" -> 2 个 UTF-16 code unit */
    const uint16_t *w1 = (const uint16_t *)elem_ptr;
    assert(w1[0] == 0x62FCu && w1[1] == 0x97F3u);

    uint8_t empty_list[24];
    ok = cj_build_qstringlist_from_utf8_array(empty_list, NULL, NULL, 0, fake_allocate);
    assert(ok == 1);
    memcpy(&size, empty_list + 16, sizeof(long long));
    assert(size == 0);

    assert(cj_build_qstringlist_from_utf8_array(NULL, words, lens, 2, fake_allocate) == 0);
    assert(cj_build_qstringlist_from_utf8_array(out_list, words, lens, 2, NULL) == 0);

    printf("test_build_qstringlist_from_utf8_array: OK\n");
}

int main(void) {
    test_append_to_nonempty_list();
    test_append_to_empty_list();
    test_append_multiple_sequential();
    test_null_arguments_rejected();
    test_build_utf16_literal_qstring();
    test_qstring_equals_ascii();
    test_build_ascii_qstring();
    test_build_qstringlist_from_ascii_array();
    test_build_utf8_qstring();
    test_build_qstringlist_from_utf8_array();
    printf("=== test_qstringlist_append: 全部通过 ===\n");
    return 0;
}
