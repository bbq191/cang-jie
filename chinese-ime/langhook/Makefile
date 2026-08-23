CC ?= gcc
AARCH64_CC ?= aarch64-linux-gnu-gcc

SRC = src/scan.c src/pattern.c src/trampoline_aarch64.c src/qstringlist_append.c
CFLAGS_COMMON = -std=gnu17 -Wall -Wextra -g

# Step T/V：拼音引擎 C 移植版（pinyin-engine/c），只有 hook_init.c 真正用到
# cj_segment()/cj_dict_*()——不加进上面的 SRC/host 单测（那些测的是 scan/
# pattern/trampoline/qstringlist_append 本身，跟拼音引擎无关，pinyin-engine/c
# 自己有独立的 Makefile/单测/差分测试，不在这里重复跑），只在 aarch64 目标里
# 引用源文件，避免不必要的重复编译。用相对路径直接引用，不拷贝一份到这个
# 目录下——两份拷贝迟早会漂移不同步，源头只保留一份。
PINYIN_ENGINE_DIR = ../pinyin-engine/c
PINYIN_SRC = $(PINYIN_ENGINE_DIR)/src/syllables.c $(PINYIN_ENGINE_DIR)/src/segment.c $(PINYIN_ENGINE_DIR)/src/dictionary.c $(PINYIN_ENGINE_DIR)/src/jianpin.c $(PINYIN_ENGINE_DIR)/src/shuangpin.c $(PINYIN_ENGINE_DIR)/src/userfreq.c $(PINYIN_ENGINE_DIR)/src/snippets.c

.PHONY: test aarch64 clean

# 宿主机单元测试——纯逻辑代码，不需要真机/ARM64，验证扫描/特征码/trampoline
# 编码这几个模块本身的正确性。
test:
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_pattern $(SRC) tests/test_pattern.c
	/tmp/test_pattern
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_scan $(SRC) tests/test_scan.c
	/tmp/test_scan
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_trampoline $(SRC) tests/test_trampoline.c
	/tmp/test_trampoline
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_qstringlist_append $(SRC) tests/test_qstringlist_append.c
	/tmp/test_qstringlist_append

# xovi 扩展胶水:xovigen 从 cangjie-langhook.xovi 生成 xovi section 元数据
# (EXTENSIONVERSION/Environment/LINKTABLE + 空 _xovi_depconstruct),让 xovi 认它为
# 合规扩展从 extensions.d 自动加载。需要 asivery/xovi 的 xovigen.py——构建前
# clone 它,用 XOVI_DIR 指向(白皮书 2.4 记过流程)。_xovi_construct/_xovi_shouldLoad
# 在 hook_init.c 末尾自定义,不由 xovigen 生成。
XOVI_DIR ?= ../../../xovi
XOVIGEN = $(XOVI_DIR)/util/xovigen.py

# 交叉编译要跑在设备上的 xovi 扩展(整合进 vellum xovi 体系,放 extensions.d)。
aarch64: xovi_glue.c
	$(AARCH64_CC) $(CFLAGS_COMMON) -I$(PINYIN_ENGINE_DIR)/src -shared -fPIC \
		$(SRC) $(PINYIN_SRC) src/hook_init.c xovi_glue.c \
		-ldl \
		-o cangjie-langhook.so
	aarch64-linux-gnu-nm -D cangjie-langhook.so | grep -iE "_xovi_construct|_xovi_shouldLoad|cj_hook" || true

# 从 .xovi 生成 xovi 胶水(元数据 section)。
xovi_glue.c: cangjie-langhook.xovi
	python3 $(XOVIGEN) -o xovi_glue.c -H xovi_glue.h -a aarch64 cangjie-langhook.xovi

clean:
	rm -f cangjie-langhook.so xovi_glue.c xovi_glue.h /tmp/test_pattern /tmp/test_scan /tmp/test_trampoline
