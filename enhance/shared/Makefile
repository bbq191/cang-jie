CC ?= gcc

# host 侧单测（不需要 aarch64 交叉工具链）：这几个文件本身是纯 C11，不依赖任何 xovi/
# xochitl 符号（scan.c 的 cj_find_exec_module 本来就接受构造好的字符串代替真的
# /proc/self/maps，见其头注），可以直接在宿主机编、跑。真正的 aarch64 产物仍由各消费方
# （../hl-snap/Makefile、../ui-font/Makefile——后者只用 minijson；handwriting-stroke 2026-09-30 已移除）
# 自己的 aarch64 目标编译，这里不重复。
SRC = scan.c pattern.c trampoline_aarch64.c trampoline_patch.c minijson.c
CFLAGS_TEST = -std=gnu17 -Wall -Wextra -g -I.
# 包一层 mmap，测试里能让"建调用桩"那次分配故意失败（见 test_patch_target_stub_alloc_fail_restores_rx）。
LDFLAGS_TEST = -Wl,--wrap=mmap

.PHONY: test clean

test: tests/test_shared
	./tests/test_shared

tests/test_shared: $(SRC) tests/test_shared.c
	$(CC) $(CFLAGS_TEST) $(SRC) tests/test_shared.c $(LDFLAGS_TEST) -o tests/test_shared

clean:
	rm -f tests/test_shared
