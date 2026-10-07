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

    volatile double x = 0.0;
    assert(cos(x) == 1.0);
    void *slot = NULL;
    assert(ui_font_patch_import("cos", (void *)fake_cos, &slot) && slot);
    assert(cos(x) == 42.0);
    assert(!ui_font_patch_import("no_such_import_xyz", (void *)fake_cos, NULL));
    puts("ui-font tests OK");
    return 0;
}
