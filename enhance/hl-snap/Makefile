CC ?= gcc
AARCH64_CC ?= aarch64-linux-gnu-gcc

# 特征码扫描 + ARM64 远跳转 trampoline 三个纯工具文件（跟"装哪个 hook"无关的通用
# 基础设施）。2026-09-11 起改为**剥离移植**（独立副本，不再路径引用 chinese-ime/
# langhook/src——那边挪出仓库后不再适合当活跃路径依赖），见 ../shared/PROVENANCE.md。
SHARED_DIR = ../shared
SHARED_SRC = $(SHARED_DIR)/scan.c $(SHARED_DIR)/pattern.c $(SHARED_DIR)/trampoline_aarch64.c $(SHARED_DIR)/trampoline_patch.c $(SHARED_DIR)/minijson.c
# EXTRA_CFLAGS：CI 传 -Werror（交叉编译零警告门槛）；平常为空，产物与不加时逐字节相同
CFLAGS_COMMON = -std=gnu17 -Wall -Wextra -g -ffile-prefix-map=$(CURDIR)=. -I$(SHARED_DIR) $(EXTRA_CFLAGS)

.PHONY: aarch64 glue clean check-inputs

# 编进 .so 的全部输入（源码、头文件、胶水、本 Makefile）。.so 提交进仓库，同时提交 <名>.so.inputs＝这些输入的 sha256：
# 部署脚本先 `make check-inputs`，源码与记录一致就直接用已提交的 .so、不重编——编译器升级后重编出的 .so 字节不同、
# 代码一样，会白白弄脏工作区并让设备多一次整机重启（2026-10-22：GCC 16.1→16.2 后 install-all 原地重编了两个 .so）。
# 改了任何输入就 `make aarch64`（同时刷新记录），把 .so 和 .so.inputs 一起提交；CI 查两者对得上。
INPUTS = $(sort $(wildcard src/*.c src/*.h) xovi_glue.c xovi_glue.h $(SHARED_SRC) $(wildcard $(SHARED_DIR)/*.h) Makefile)

# xovi 扩展胶水：xovigen 从 hl-snap.xovi 生成 xovi section 元数据（白皮书 2.4 记过流程）。胶水已提交进仓库，
# 平常构建不需要 asivery/xovi；改了 .xovi 才要 clone 它、`make glue XOVI_DIR=<克隆路径>`。
XOVI_DIR ?= ../../../xovi
XOVIGEN = $(XOVI_DIR)/util/xovigen.py

aarch64: xovi_glue.c
	$(AARCH64_CC) $(CFLAGS_COMMON) -shared -fPIC \
		$(SHARED_SRC) src/hl_snap.c xovi_glue.c \
		-o hl-snap.so
	sha256sum $(INPUTS) > hl-snap.so.inputs
	aarch64-linux-gnu-nm -D hl-snap.so | grep -iE "_xovi_construct|_xovi_shouldLoad" || true

# 已提交的 hl-snap.so 是不是由当前源码编出来的（按内容比，不看 mtime）：是则退出 0。
check-inputs:
	@test -f hl-snap.so && sha256sum $(INPUTS) | cmp -s - hl-snap.so.inputs

# 胶水 xovi_glue.{c,h} 已提交进仓库：这条规则不依赖 .xovi 的 mtime，只在胶水缺失时才生成——
# 否则 checkout 后 .xovi 的 mtime 恰好更新就会去跑 xovigen，本机没有 xovi clone 时构建失败，
# 部署脚本随即退回仓库里已提交的（可能是旧源码编出的）.so（2026-09-24 审查）。改了 .xovi 用
# `make glue XOVI_DIR=<clone>` 显式重新生成。
xovi_glue.c:
	python3 $(XOVIGEN) -o xovi_glue.c -H xovi_glue.h -a aarch64 hl-snap.xovi

glue:
	python3 $(XOVIGEN) -o xovi_glue.c -H xovi_glue.h -a aarch64 hl-snap.xovi

# 不删 xovi_glue.{c,h}：它们已提交进仓库，删了之后没有 xovi clone 就再也编不出来（见上）。
clean:
	rm -f hl-snap.so
