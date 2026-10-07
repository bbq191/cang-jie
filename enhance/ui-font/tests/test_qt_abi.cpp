// host 单测（x86_64 + 本机 Qt 6）：本扩展用 C 调 Qt 的 C++ 函数（QFont::family / QString::fromUtf8 / QFont::setFamily /
// QFont 拷贝构造与析构），这里在真实 Qt 上走一遍 ui_font_set_font：改的是本测试程序自己导入表里的 setFont。
// x86_64 SysV 与 AAPCS64 对这几种参数 / 返回值的传法相同（>16 字节返回值走隐藏指针，16 字节两字段按值放两个整数寄存器）。
#include <QGuiApplication>
#include <QFont>
#include <cstdio>
#include <cstdlib>
extern "C" {
int ui_font_patch_import(const char *sym, void *handler, void **out_slot);
char _xovi_shouldLoad(void);
void _xovi_construct(void);
}
int main(int argc, char **argv) {
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QGuiApplication app(argc, argv);
    const char *cfg = UI_FONT_TEST_CONFIG;
    auto write = [&](const char *json) { FILE *f = fopen(cfg, "w"); fputs(json, f); fclose(f); };
    if (!_xovi_shouldLoad()) { fprintf(stderr, "shouldLoad 失败\n"); return 1; }
    _xovi_construct();
    write("{\"sans\":\"Sarasa UI SC\"}");
    QGuiApplication::setFont(QFont("reMarkable Sans", 12, QFont::Medium));
    QFont f = QGuiApplication::font();
    if (f.family() != "Sarasa UI SC" || f.weight() != QFont::Medium || f.pointSize() != 12) { fprintf(stderr, "替换失败: %s\n", qPrintable(f.family())); return 1; }
    QGuiApplication::setFont(QFont("Other Font"));             // 不是原生界面字体：原样放行
    if (QGuiApplication::font().family() != "Other Font") return 1;
    write("{\"sans\":\"\"}");                                    // 没选：原样放行
    QGuiApplication::setFont(QFont("reMarkable Sans"));
    if (QGuiApplication::font().family() != "reMarkable Sans") return 1;
    write("{\"sans\":\"霞鹜文楷\"}");                             // 非 ASCII 家族名
    QGuiApplication::setFont(QFont("reMarkable Sans"));
    if (QGuiApplication::font().family() != QString::fromUtf8("霞鹜文楷")) return 1;
    remove(cfg);                                                 // 配置文件没有：原样放行
    QGuiApplication::setFont(QFont("reMarkable Sans"));
    if (QGuiApplication::font().family() != "reMarkable Sans") return 1;
    puts("ui-font Qt ABI tests OK");
    return 0;
}
