// LD_PRELOAD wrap Qt 的 qputenv(const char*, QByteArrayView)——拦截 xochitl 启动时
// qputenv("QT_IM_MODULES","xochitl"),改成让我们的 cangjie 插件生效。
// wrap 导出符号=固件无关;不改磁盘本体=绕开 dm-verity。
// QByteArrayView = { qsizetype m_size; const char* m_data }(16字节,AArch64按值传x1=size,x2=data)。
#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <stdio.h>

typedef char (*qputenv_t)(const char *, long long, const char *);
static qputenv_t real_qputenv = NULL;

// 混淆的 mangled 名就是符号本身
char _Z7qputenvPKc14QByteArrayView(const char *name, long long size, const char *data) {
    if (!real_qputenv)
        real_qputenv = (qputenv_t)dlsym(RTLD_NEXT, "_Z7qputenvPKc14QByteArrayView");

    if (name && strcmp(name, "QT_IM_MODULES") == 0) {
        FILE *f = fopen("/tmp/cangjie-hook.log", "a");
        if (f) {
            fprintf(f, "[qputenv-hook] 拦到 QT_IM_MODULES 原值=[%.*s] → 改成 cangjie\n",
                    (int)size, data ? data : "");
            fclose(f);
        }
        // 先验证注入:直接替换成 cangjie(共存留后面:改成 "cangjie;xochitl"+装饰器)
        return real_qputenv(name, 7, "cangjie");
    }
    return real_qputenv(name, size, data);
}
