/* host 单测（x86_64）：配置解析 + 导入表改写。导入表改写在 aarch64 上逻辑相同，只是重定位类型不同
 * （UI_FONT_JUMP_SLOT），这里用 libm 的 cos 走一遍：懒绑定 / BIND_NOW、PIE / 非 PIE 四种链接方式由 Makefile 各编一份。 */
#include "../src/ui_font.c"
#include <math.h>
#include <assert.h>

static double fake_cos(double x) { (void)x; return 42.0; }

int main(void) {
    char out[64];
    assert(ui_font_parse_sans("{\"sans\":\"Sarasa UI SC\",\"serif\":\"\"}", out, sizeof out) && !strcmp(out, "Sarasa UI SC"));
    assert(ui_font_parse_sans("{ \"serif\": \"X\",\n  \"sans\" :  \"A \\\"B\\\" \\\\C\" }", out, sizeof out) && !strcmp(out, "A \"B\" \\C"));
    assert(!ui_font_parse_sans("{\"sans\":\"\"}", out, sizeof out));            /* 空 = 原生 */
    assert(!ui_font_parse_sans("{\"serif\":\"X\"}", out, sizeof out));          /* 没有 sans */
    assert(!ui_font_parse_sans("{\"sans\":\"\\u4e2d\"}", out, sizeof out));     /* 不认的转义整条放弃 */
    assert(!ui_font_parse_sans("{\"sans\":\"abc", out, sizeof out));            /* 截断 */
    assert(!ui_font_parse_sans("{\"sans\":\"0123456789\"}", out, 8));           /* 放不下 */
    assert(!ui_font_parse_sans("{\"sans\":null}", out, sizeof out));
    /* 值恰好是 "sans" 时不能遮住真正的键（2026-10-10 前的 strstr 扫描在这里整条放弃，拿不到 Foo） */
    assert(ui_font_parse_sans("{\"serif\":\"sans\",\"sans\":\"Foo\"}", out, sizeof out) && !strcmp(out, "Foo"));

    volatile double x = 0.0;
    assert(cos(x) == 1.0);
    void *slot = NULL;
    assert(ui_font_patch_import("cos", (void *)fake_cos, &slot) && slot);
    assert(cos(x) == 42.0);
    assert(!ui_font_patch_import("no_such_import_xyz", (void *)fake_cos, NULL));
    /* 主程序判定：main 在主程序里；libc 的 fopen 不在（本测试没取它的地址，没有规范 PLT） */
    assert(ui_font_addr_in_main((const void *)main));
    assert(!ui_font_addr_in_main(dlsym(RTLD_DEFAULT, "fopen")));
    /* 危险情形复现：取 sin 的地址（另用一个函数，免得影响上面 cos 的导入槽测试）。非 PIE 下链接器给 sin 一个主程序里的规范 PLT，dlsym 也拿到它 → 判"在主程序"；
     * PIE 下取地址走 GOT、dlsym 拿到 libm 里的真函数 → 判"不在"。两边结论必须一致。 */
    void *volatile taken = (void *)&sin;
    int plt = ui_font_addr_in_main(dlsym(RTLD_DEFAULT, "sin"));
    assert(plt == ui_font_addr_in_main(taken));
    printf("   sin 取地址后 dlsym 落在%s\n", plt ? "主程序（规范 PLT，会被拒绝加载）" : "libm");
    puts("ui-font tests OK");
    return 0;
}
