CC ?= gcc
AARCH64_CC ?= aarch64-linux-gnu-gcc

# 不用 enhance/shared 的特征码扫描 / trampoline：本扩展改的是 xochitl 导入表的一格，不 patch 指令（见 src/ui_font.c 头注）。
CFLAGS_COMMON = -std=gnu17 -Wall -Wextra -g -ffile-prefix-map=$(CURDIR)=.

.PHONY: aarch64 test clean

# xovi 胶水 xovi_glue.{c,h} 已提交进仓库：ui-font.xovi 只有版本号，与 hl-snap 的胶水逐字节相同（拷来的）。
# 改了 .xovi 才需要 asivery/xovi 的 xovigen.py 重新生成：python3 <xovi>/util/xovigen.py -o xovi_glue.c -H xovi_glue.h -a aarch64 ui-font.xovi
aarch64:
	$(AARCH64_CC) $(CFLAGS_COMMON) -shared -fPIC src/ui_font.c xovi_glue.c -o ui-font.so
	aarch64-linux-gnu-nm -D ui-font.so | grep -iE "_xovi_construct|_xovi_shouldLoad" || true

# 四种链接方式各跑一遍：懒绑定 / BIND_NOW（RELRO 只读，走 mprotect）× PIE / 非 PIE（非 PIE 编译也加 -fno-pie，
# 不然发行版默认 -fPIE 编出的代码取地址走 GOT，测不到"规范 PLT"那种危险情形）；
# 再在本机真实 Qt 6 上走一遍 setFont 替换（没装 Qt6Gui 开发包就跳过这一项）。
test:
	@for v in "-fno-pie -no-pie -Wl,-z,lazy" "-fno-pie -no-pie -Wl,-z,now,-z,relro" "-pie -fPIE -Wl,-z,lazy" "-pie -fPIE -Wl,-z,now,-z,relro"; do \
		$(CC) $(CFLAGS_COMMON) -fno-builtin -O0 $$v tests/test_ui_font.c -o tests/test_ui_font -lm -ldl || exit 1; \
		echo "-- $$v"; ./tests/test_ui_font || exit 1; \
	done; rm -f tests/test_ui_font
	@if ! pkg-config --exists Qt6Gui; then echo "-- 没有 Qt6Gui 开发包（pkg-config），跳过真实 Qt 的 ABI 测试"; exit 0; fi; \
	cfg=$$(mktemp -u /tmp/ui-font-test.XXXXXX.json); \
	$(CC) $(CFLAGS_COMMON) -c -fPIC -DCONFIG_PATH="\"$$cfg\"" src/ui_font.c -o tests/ui_font.o && \
	c++ -std=c++17 -fPIC -DUI_FONT_TEST_CONFIG="\"$$cfg\"" tests/test_qt_abi.cpp tests/ui_font.o -o tests/test_qt_abi $$(pkg-config --cflags --libs Qt6Gui) -ldl && \
	./tests/test_qt_abi; rc=$$?; rm -f tests/test_qt_abi tests/ui_font.o "$$cfg"; exit $$rc

clean:
	rm -f ui-font.so tests/test_ui_font tests/test_qt_abi tests/ui_font.o
