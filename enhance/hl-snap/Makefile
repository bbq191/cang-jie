CC ?= gcc
AARCH64_CC ?= aarch64-linux-gnu-gcc

# 复用 chinese-ime/langhook 已经模块化出来的三个纯工具文件（特征码扫描 + ARM64
# 远跳转 trampoline，本来就是跟"装哪个 hook"无关的通用基础设施）——路径引用，
# 不复制、不修改，跟 shelf 的 bookconv 被 reading/device-rs 路径引用是同一个已
# 有先例。改这三个文件要去 chinese-ime/langhook/src/ 改，这边不重复维护。
LANGHOOK_SRC_DIR = ../../chinese-ime/langhook/src
SHARED_SRC = $(LANGHOOK_SRC_DIR)/scan.c $(LANGHOOK_SRC_DIR)/pattern.c $(LANGHOOK_SRC_DIR)/trampoline_aarch64.c
CFLAGS_COMMON = -std=gnu17 -Wall -Wextra -g -I$(LANGHOOK_SRC_DIR)

.PHONY: aarch64 clean

# xovi 扩展胶水：xovigen 从 hl-snap.xovi 生成 xovi section 元数据（同 chinese-ime/
# langhook 的 xovigen 用法，白皮书 2.4 记过流程）。需要 asivery/xovi 的
# xovigen.py——构建前 clone 它，XOVI_DIR 指向克隆路径。
XOVI_DIR ?= ../../../xovi
XOVIGEN = $(XOVI_DIR)/util/xovigen.py

aarch64: xovi_glue.c
	$(AARCH64_CC) $(CFLAGS_COMMON) -shared -fPIC \
		$(SHARED_SRC) src/hl_snap.c xovi_glue.c \
		-o hl-snap.so
	aarch64-linux-gnu-nm -D hl-snap.so | grep -iE "_xovi_construct|_xovi_shouldLoad" || true

xovi_glue.c: hl-snap.xovi
	python3 $(XOVIGEN) -o xovi_glue.c -H xovi_glue.h -a aarch64 hl-snap.xovi

clean:
	rm -f hl-snap.so xovi_glue.c xovi_glue.h
