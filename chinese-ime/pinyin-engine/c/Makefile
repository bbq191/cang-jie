# 拼音引擎 C 移植版——音节表 + 切分 + 候选词典三层（见白皮书 9.2 节、
# 计划文件"中文模式+拼音缓冲"/"M5"两轮的验证结论：设备没有 Python3，
# 拼音引擎要移植成 C 才能接进 xovi hook；词典数据量太大不适合编译进
# 二进制本体，改成运行时 mmap 一份离线生成的 dict.bin，见
# src/dictionary.h 顶部说明）。
#
# syllables/segment 已经接进 xovi-extensions/cangjie-langhook（Step T）；
# dictionary 这一层是 M5 的产出，这一轮还没接进去（见计划文件"M5"章节
# 明确不做的事），只做到"离线验证格式设计能不能用"。

CC ?= gcc
AARCH64_CC ?= aarch64-linux-gnu-gcc
# 本仓库的 venv 实际在 cang-jie/rmfw/.venv（见 pinyin-engine/README.md
# "跑起来"一节），不是 pinyin-engine/.venv——下面用到 $(PY) 的地方都是
# 先 `cd ..`（进 pinyin-engine/）再引用，所以这里写相对 pinyin-engine/
# 的路径 ../rmfw/.venv/bin/python3；都带了 "|| python3" 兜底，路径不对
# 时退回系统 python3。
PY ?= ../rmfw/.venv/bin/python3

SRC = src/syllables.c src/segment.c
DICT_SRC = src/dictionary.c
JIANPIN_SRC = src/jianpin.c
SHUANGPIN_SRC = src/shuangpin.c
USERFREQ_SRC = src/userfreq.c
SNIPPETS_SRC = src/snippets.c
CFLAGS_COMMON = -std=gnu17 -Wall -Wextra -g

.PHONY: test diff-check aarch64 regen-syllables regen-shuangpin dict-blob jianpin-blob dict-zh-tw-blob jianpin-zh-tw-blob clean

test: dict-blob jianpin-blob dict-zh-tw-blob jianpin-zh-tw-blob
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_syllables $(SRC) tests/test_syllables.c
	/tmp/test_syllables
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_segment $(SRC) tests/test_segment.c
	/tmp/test_segment
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_dictionary $(DICT_SRC) tests/test_dictionary.c
	/tmp/test_dictionary
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_dictionary_zh_tw $(DICT_SRC) tests/test_dictionary_zh_tw.c
	/tmp/test_dictionary_zh_tw
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_jianpin $(JIANPIN_SRC) tests/test_jianpin.c
	/tmp/test_jianpin
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_jianpin_zh_tw $(JIANPIN_SRC) tests/test_jianpin_zh_tw.c
	/tmp/test_jianpin_zh_tw
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_shuangpin $(SHUANGPIN_SRC) tests/test_shuangpin.c
	/tmp/test_shuangpin
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_userfreq $(USERFREQ_SRC) tests/test_userfreq.c
	/tmp/test_userfreq
	$(CC) $(CFLAGS_COMMON) -o /tmp/test_snippets $(SNIPPETS_SRC) tests/test_snippets.c
	/tmp/test_snippets

# 差分测试——同一批输入分别跑 Python 原版和这里的 C 移植版，逐行 diff，
# 比单元测试覆盖面更广。segment 那次的已知差异见 diff_check.py 顶部
# 说明；dictionary/jianpin/shuangpin 目前全部一致，唯一预留的容许差异
# 是"同权重候选之间的先后顺序"（tie-break 实现方式不同，见
# diff_check_dict.py/diff_check_jianpin.py 顶部说明，shuangpin 是纯查表
# 没有排序，不适用这条容许差异），不代表数据不一致。
diff-check: dict-blob jianpin-blob
	$(CC) $(CFLAGS_COMMON) -o /tmp/diff_cli $(SRC) tests/diff_cli.c
	$(CC) $(CFLAGS_COMMON) -o /tmp/diff_cli_dict $(DICT_SRC) tests/diff_cli_dict.c
	$(CC) $(CFLAGS_COMMON) -o /tmp/diff_cli_jianpin $(JIANPIN_SRC) tests/diff_cli_jianpin.c
	$(CC) $(CFLAGS_COMMON) -o /tmp/diff_cli_shuangpin $(SHUANGPIN_SRC) tests/diff_cli_shuangpin.c
	$(CC) $(CFLAGS_COMMON) -o /tmp/diff_cli_userfreq $(USERFREQ_SRC) tests/diff_cli_userfreq.c
	$(CC) $(CFLAGS_COMMON) -o /tmp/diff_cli_snippets $(SNIPPETS_SRC) tests/diff_cli_snippets.c
	cd .. && ($(PY) c/tests/diff_check.py || python3 c/tests/diff_check.py)
	cd .. && ($(PY) c/tests/diff_check_dict.py || python3 c/tests/diff_check_dict.py)
	cd .. && ($(PY) c/tests/diff_check_jianpin.py || python3 c/tests/diff_check_jianpin.py)
	cd .. && ($(PY) c/tests/diff_check_shuangpin.py || python3 c/tests/diff_check_shuangpin.py)
	cd .. && ($(PY) c/tests/diff_check_userfreq.py || python3 c/tests/diff_check_userfreq.py)
	cd .. && ($(PY) c/tests/diff_check_snippets.py || python3 c/tests/diff_check_snippets.py)

# 交叉编译验证——确认这批 C 代码在目标架构（reMarkable 的 ARM64）上也能
# 正常编译，跟 xovi-extensions/cangjie-langhook 用同一个交叉编译器。
# dictionary.c/jianpin.c 用了 mmap/fstat 等 POSIX 接口，交叉编译器自带的
# libc 头文件应该覆盖；shuangpin.c 纯查表，不用任何 POSIX 接口。这里只
# 验证编译，不验证链接（不产出可执行文件）。
aarch64:
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/syllables.c -o /tmp/cj_syllables_aarch64.o
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/segment.c -o /tmp/cj_segment_aarch64.o
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/dictionary.c -o /tmp/cj_dictionary_aarch64.o
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/jianpin.c -o /tmp/cj_jianpin_aarch64.o
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/shuangpin.c -o /tmp/cj_shuangpin_aarch64.o
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/userfreq.c -o /tmp/cj_userfreq_aarch64.o
	$(AARCH64_CC) $(CFLAGS_COMMON) -c src/snippets.c -o /tmp/cj_snippets_aarch64.o
	@echo "（只验证能不能编译，不产出可执行文件——dictionary.c/jianpin.c 这一层已接入 cangjie-langhook；shuangpin.c 见本轮接入进度）"

# 音节表数据源是 Python 端 src/syllables.py，如果 luna_pinyin.dict.yaml
# 更新了，重新跑这个目标同步一次。
regen-syllables:
	cd .. && $(PY) c/tools/gen_syllables_header.py > c/src/syllables_data.h 2>/dev/null || \
		python3 c/tools/gen_syllables_header.py > c/src/syllables_data.h

# 双拼（小鹤 flypy）键位解码表数据源是 Python 端 src/shuangpin.py +
# data/double_pinyin_flypy.schema.yaml，如果 schema 文件更新了，重新跑
# 这个目标同步一次（跟 regen-syllables 同一个"数据源变了才手动重跑"的
# 模式，不像 dict-blob 那样每次 test 自动依赖——这份表几乎不会变）。
regen-shuangpin:
	cd .. && $(PY) c/tools/gen_shuangpin_table.py > c/src/shuangpin_data.h 2>/dev/null || \
		python3 c/tools/gen_shuangpin_table.py > c/src/shuangpin_data.h

# 词典二进制数据源是 Python 端 src/dictionary.py（默认雾凇拼音
# base.dict.yaml + 8105.dict.yaml，见 data/PROVENANCE.rime-ice.md），
# 如果数据源更新了（重新拉取新 commit），重新跑这个目标同步一次。
# test/diff-check 会自动依赖这个目标，不需要手动先跑。
dict-blob:
	cd .. && ($(PY) c/tools/gen_dict_blob.py c/build 2>/dev/null || python3 c/tools/gen_dict_blob.py c/build)

# 简拼索引二进制——数据源是 Python 端 src/jianpin.py（从同一份 Dictionary
# 反推），跟 dict-blob 同一个刷新方式：数据源变了，重新跑这个目标同步。
# test/diff-check 会自动依赖这个目标，不需要手动先跑。
jianpin-blob:
	cd .. && ($(PY) c/tools/gen_jianpin_blob.py c/build 2>/dev/null || python3 c/tools/gen_jianpin_blob.py c/build)

# 繁体（台湾正体）版词典二进制——白皮书 05 节设计修订的产物，跟 dict.bin
# 同一个 CJDICT01 格式（dictionary.c 不用改），内容是 Dictionary 候选表
# 整体跑一遍 OpenCC s2twp + 合并同形词权重（见 src/traditional.py 的
# build_traditional_table）。test 会自动依赖这个目标。
dict-zh-tw-blob:
	cd .. && ($(PY) c/tools/gen_dict_zh_tw_blob.py c/build 2>/dev/null || python3 c/tools/gen_dict_zh_tw_blob.py c/build)

# 繁体版简拼索引——真机接入简拼后暴露的缺口（切到繁体后简拼补充候选
# 依然是简体字，因为 dict_jianpin.bin 只从简体 Dictionary 反推）的修复，
# 跟 dict-zh-tw-blob 同一个"生成阶段整体转换"思路，只是数据源从
# Dictionary 换成 JianpinIndex（鸭子类型接受 build_traditional_table
# 的输出，不需要改 jianpin.py，见 gen_jianpin_zh_tw_blob.py 顶部说明）。
jianpin-zh-tw-blob:
	cd .. && ($(PY) c/tools/gen_jianpin_zh_tw_blob.py c/build 2>/dev/null || python3 c/tools/gen_jianpin_zh_tw_blob.py c/build)

clean:
	rm -f /tmp/test_syllables /tmp/test_segment /tmp/test_dictionary /tmp/test_dictionary_zh_tw /tmp/test_jianpin \
		/tmp/test_jianpin_zh_tw /tmp/test_shuangpin /tmp/diff_cli /tmp/diff_cli_dict /tmp/diff_cli_jianpin \
		/tmp/diff_cli_shuangpin
	rm -rf build
