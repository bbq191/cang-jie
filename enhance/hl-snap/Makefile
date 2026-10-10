CC ?= gcc
AARCH64_CC ?= aarch64-linux-gnu-gcc

# 特征码扫描 + ARM64 远跳转 trampoline 三个纯工具文件（跟"装哪个 hook"无关的通用
# 基础设施）。2026-09-11 起改为**剥离移植**（独立副本，不再路径引用 chinese-ime/
# langhook/src——那边挪出仓库后不再适合当活跃路径依赖），见 ../shared/PROVENANCE.md。
SHARED_DIR = ../shared
SHARED_SRC = $(SHARED_DIR)/scan.c $(SHARED_DIR)/pattern.c $(SHARED_DIR)/trampoline_aarch64.c $(SHARED_DIR)/trampoline_patch.c
CFLAGS_COMMON = -std=gnu17 -Wall -Wextra -g -ffile-prefix-map=$(CURDIR)=. -I$(SHARED_DIR)

.PHONY: aarch64 glue clean

# xovi 扩展胶水：xovigen 从 hl-snap.xovi 生成 xovi section 元数据（白皮书 2.4 记过流程）。胶水已提交进仓库，
# 平常构建不需要 asivery/xovi；改了 .xovi 才要 clone 它、`make glue XOVI_DIR=<克隆路径>`。
XOVI_DIR ?= ../../../xovi
XOVIGEN = $(XOVI_DIR)/util/xovigen.py

aarch64: xovi_glue.c
	$(AARCH64_CC) $(CFLAGS_COMMON) -shared -fPIC \
		$(SHARED_SRC) src/hl_snap.c xovi_glue.c \
		-o hl-snap.so
	aarch64-linux-gnu-nm -D hl-snap.so | grep -iE "_xovi_construct|_xovi_shouldLoad" || true

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
