/* ui-font —— xochitl 界面字体，独立 xovi 扩展。
 *
 * 只做一件事：xochitl 启动时调 QGuiApplication::setFont(QFont("reMarkable Sans")) 设应用默认字体，
 * 本扩展把这次调用里的家族名换成用户在网页「其他 → xochitl → 界面字体」选的字体（font-serve 写的
 * ~/.local/share/shelf/ui-font.json 的 sans）。没选 / 文件没有 / 读不了 → 原样放行，与没装一样。
 *
 * 只改这一个调用，所以：
 *   - 阅读器的字体不变：EPUB 用阅读器菜单里选的字体（epubProperties.fontName），菜单里的
 *     「reMarkable Sans」仍是原来那个内嵌字体——本扩展不改字体数据库、不给家族名起别名；
 *   - 笔记本里打字的文字不变：那是 xochitl 自己按名字 new QFont("reMarkable Sans")，不经应用默认字体；
 *   - Ark 组件（新版设置页、对话框等）的字体写在设计令牌里，不吃应用默认字体，由 shelf/xovi/ui-font-tokens.qmd 改。
 *
 * 怎么改：不 patch setFont 本体（它开头第 3 条是 PC 相对的 adrp，搬进跳板会算错地址），而是改 xochitl
 * 自己导入表（.got.plt）里 setFont 那一格，指向本文件的 ui_font_set_font；原函数用 dlsym 拿到的真实地址
 * 调用（3.28 的 xochitl 是懒绑定，那一格起初指向 PLT 解析桩，不能拿来当"原函数"——调一次就会被解析器
 * 改写回真实地址、把我们挤掉）。槽位运行时从 xochitl 的 .rela.plt 按符号名找，不写死地址。
 *
 * Qt 6 的 ABI 用法（C 调 C++）：
 *   - QFont 16 字节（d 指针 + resolve_mask），这里在栈上留 64 字节、16 字节对齐，copy 构造 / 析构都调 Qt 自己的；
 *   - QString 24 字节 {d, ptr, size}（UTF-16），返回值走 x8（AAPCS64：超过 16 字节的结构体与 C++ 非平凡返回值都走 x8）；
 *   - QString::fromUtf8(QByteArrayView) 的参数 {m_size, m_data} 按值放 x0、x1。
 * 启动时只调一次，返回的 QString 不析构（Qt 内联析构，没有导出符号），漏几十字节无所谓。
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <elf.h>
#include <link.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#define TAG "[ui-font]"
#ifndef CONFIG_PATH
#define CONFIG_PATH "/home/root/.local/share/shelf/ui-font.json" /* host 测试用 -D 覆盖 */
#endif
#define NATIVE_FAMILY "reMarkable Sans"
#define SYM_SET_FONT "_ZN15QGuiApplication7setFontERK5QFont"
#ifdef __x86_64__
#define UI_FONT_JUMP_SLOT R_X86_64_JUMP_SLOT /* 只给 host 测试用（tests/） */
#else
#define UI_FONT_JUMP_SLOT R_AARCH64_JUMP_SLOT
#endif

typedef struct {
    void *d;
    const uint16_t *ptr;
    intptr_t size;
} QStringRaw;

static void (*o_set_font)(const void *font);
static void (*q_font_copy)(void *self, const void *other);
static void (*q_font_dtor)(void *self);
static void (*q_font_set_family)(void *self, const QStringRaw *family);
static QStringRaw (*q_font_family)(const void *self);
static QStringRaw (*q_string_from_utf8)(intptr_t size, const char *data);

/* QString（UTF-16）是否正好等于 ASCII 串 s。 */
static int qstr_eq_ascii(const QStringRaw *q, const char *s) {
    size_t n = strlen(s);
    if (!q->ptr || q->size != (intptr_t)n) return 0;
    for (size_t i = 0; i < n; i++)
        if (q->ptr[i] != (uint16_t)(unsigned char)s[i]) return 0;
    return 1;
}

/* 日志用：UTF-16 → ASCII，非 ASCII 记成 '?'。 */
static void qstr_to_ascii(const QStringRaw *q, char *out, size_t cap) {
    size_t n = 0;
    for (intptr_t i = 0; q->ptr && i < q->size && n + 1 < cap; i++)
        out[n++] = q->ptr[i] < 0x80 ? (char)q->ptr[i] : '?';
    out[n] = 0;
}

/* 从配置文件取 "sans" 的值（UTF-8，原样）。极简扫描、不引 JSON 库：只认 \" 与 \\ 两种转义，
 * 遇到别的转义（如 \u）整条放弃——font-serve 写的是 fontconfig 家族名，不会有这些。
 * 返回 1 = 拿到非空值。 */
int ui_font_parse_sans(const char *json, char *out, size_t cap) {
    const char *k = strstr(json, "\"sans\"");
    if (!k) return 0;
    const char *p = k + 6;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    if (*p++ != ':') return 0;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    if (*p++ != '"') return 0;
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
    return n > 0;
}

static int read_config_sans(char *out, size_t cap) {
    char buf[4096];
    FILE *f = fopen(CONFIG_PATH, "rb");
    if (!f) return 0;
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    buf[n] = 0;
    return ui_font_parse_sans(buf, out, cap);
}

static void ui_font_set_font(const void *font) {
    char want[256], got[256];
    QStringRaw fam = q_font_family(font);
    qstr_to_ascii(&fam, got, sizeof(got));
    if (!qstr_eq_ascii(&fam, NATIVE_FAMILY)) {
        fprintf(stderr, TAG " setFont(%s)：不是原生界面字体，原样放行\n", got);
        o_set_font(font);
        return;
    }
    if (!read_config_sans(want, sizeof(want))) {
        fprintf(stderr, TAG " setFont(%s)：没选界面字体，原样放行\n", got);
        o_set_font(font);
        return;
    }
    _Alignas(16) unsigned char copy[64];
    q_font_copy(copy, font);
    QStringRaw name = q_string_from_utf8((intptr_t)strlen(want), want);
    q_font_set_family(copy, &name);
    fprintf(stderr, TAG " setFont(%s) → %s\n", got, want);
    o_set_font(copy);
    q_font_dtor(copy);
}

/* ---- 在 xochitl 自己的 .rela.plt 里按符号名找 GOT 槽 ---- */

struct slot_query {
    const char *sym;
    void **slot;
    int seen_main;
};

static uintptr_t dyn_ptr(uintptr_t v, uintptr_t base) {
    return v < base ? v + base : v; /* 有的加载器已把 .dynamic 里的地址加过基址，有的没有 */
}

static int find_slot_cb(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    struct slot_query *q = data;
    if (q->seen_main) return 1;
    q->seen_main = 1; /* 第一个对象就是主程序（xochitl） */
    const ElfW(Dyn) *dyn = NULL;
    for (int i = 0; i < info->dlpi_phnum; i++)
        if (info->dlpi_phdr[i].p_type == PT_DYNAMIC)
            dyn = (const ElfW(Dyn) *)(info->dlpi_addr + info->dlpi_phdr[i].p_vaddr);
    if (!dyn) return 1;
    uintptr_t base = info->dlpi_addr, jmprel = 0, symtab = 0, strtab = 0;
    size_t relsz = 0;
    for (; dyn->d_tag != DT_NULL; dyn++) {
        switch (dyn->d_tag) {
        case DT_JMPREL: jmprel = dyn_ptr(dyn->d_un.d_ptr, base); break;
        case DT_PLTRELSZ: relsz = dyn->d_un.d_val; break;
        case DT_SYMTAB: symtab = dyn_ptr(dyn->d_un.d_ptr, base); break;
        case DT_STRTAB: strtab = dyn_ptr(dyn->d_un.d_ptr, base); break;
        }
    }
    if (!jmprel || !symtab || !strtab) return 1;
    const ElfW(Rela) *r = (const ElfW(Rela) *)jmprel;
    for (size_t i = 0; i < relsz / sizeof(*r); i++) {
        if (ELF64_R_TYPE(r[i].r_info) != UI_FONT_JUMP_SLOT) continue;
        const ElfW(Sym) *s = (const ElfW(Sym) *)symtab + ELF64_R_SYM(r[i].r_info);
        if (strcmp((const char *)strtab + s->st_name, q->sym) == 0) {
            q->slot = (void **)(base + r[i].r_offset);
            return 1;
        }
    }
    return 1;
}

static void **find_got_slot(const char *sym) {
    struct slot_query q = {sym, NULL, 0};
    dl_iterate_phdr(find_slot_cb, &q);
    return q.slot;
}

static int resolve_qt(void) {
    o_set_font = dlsym(RTLD_DEFAULT, SYM_SET_FONT);
    q_font_copy = dlsym(RTLD_DEFAULT, "_ZN5QFontC1ERKS_");
    q_font_dtor = dlsym(RTLD_DEFAULT, "_ZN5QFontD1Ev");
    q_font_set_family = dlsym(RTLD_DEFAULT, "_ZN5QFont9setFamilyERK7QString");
    q_font_family = dlsym(RTLD_DEFAULT, "_ZNK5QFont6familyEv");
    q_string_from_utf8 = dlsym(RTLD_DEFAULT, "_ZN7QString8fromUtf8E14QByteArrayView");
    return o_set_font && q_font_copy && q_font_dtor && q_font_set_family && q_font_family && q_string_from_utf8;
}

char _xovi_shouldLoad(void) {
    if (!find_got_slot(SYM_SET_FONT)) {
        fprintf(stderr, TAG " _xovi_shouldLoad: xochitl 不导入 QGuiApplication::setFont（未知固件）→ 拒绝加载\n");
        return 0;
    }
    if (!resolve_qt()) {
        fprintf(stderr, TAG " _xovi_shouldLoad: 缺 Qt 符号 → 拒绝加载\n");
        return 0;
    }
    return 1;
}

/* 把主程序导入表里 sym 那一格改成 handler。返回 1 = 改了。
 * 槽位所在页通常本来就可写（懒绑定的 .got.plt 不在 RELRO 里）；BIND_NOW + RELRO 时是只读，打开写权限。
 * 打不开就不写（写只读页会当场 SEGV、xochitl 起不来）。改完保持可写，不改回只读：
 * 本来就 RW 的页改回只读，懒绑定的其它符号首次解析时写不进去。 */
int ui_font_patch_import(const char *sym, void *handler, void **out_slot) {
    void **slot = find_got_slot(sym);
    if (!slot) return 0;
    long pg = sysconf(_SC_PAGESIZE);
    void *page = (void *)((uintptr_t)slot & ~(uintptr_t)(pg - 1));
    if (mprotect(page, (size_t)pg, PROT_READ | PROT_WRITE) != 0) return 0;
    *slot = handler;
    if (out_slot) *out_slot = (void *)slot;
    return 1;
}

void _xovi_construct(void) {
    void *slot = NULL;
    if (!resolve_qt()) {
        fprintf(stderr, TAG " _xovi_construct: 缺 Qt 符号，hook 未安装\n");
        return;
    }
    if (!ui_font_patch_import(SYM_SET_FONT, (void *)ui_font_set_font, &slot)) {
        fprintf(stderr, TAG " _xovi_construct: 找不到 setFont 导入槽或改不了，hook 未安装\n");
        return;
    }
    fprintf(stderr, TAG " 安装完成（setFont 导入槽 %p）\n", slot);
}
