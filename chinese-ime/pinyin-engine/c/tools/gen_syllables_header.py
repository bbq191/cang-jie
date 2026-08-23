#!/usr/bin/env python3
"""从 Python 版 src/syllables.py 生成 C 版音节表头文件。

不在 C 里重新解析 YAML/tsv——真相来源仍然是 luna_pinyin.dict.yaml，
Python 端 src/syllables.py 已经验证过怎么正确提取音节全集（见该文件
docstring），这里只是把提取结果导出成一份静态数据，C 侧直接用，
不重新实现一遍解析逻辑（YAML 解析在 C 里没必要，音节表本身不会随
输入变化，构建期生成一次即可，运行期不需要再解析）。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_syllables_header.py > c/src/syllables_data.h

跟 Python 端数据源保持同步的方式：这个脚本本身不含任何手写音节，
全部来自 src.syllables.SYLLABLES（同一份真相来源），如果以后
luna_pinyin.dict.yaml 更新，重新跑一次这个脚本就行，不需要手改
C 头文件。
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from src.syllables import SYLLABLES  # noqa: E402

syllables = sorted(SYLLABLES)
max_len = max(len(s) for s in syllables)

print("/* 自动生成，不要手改——见 c/tools/gen_syllables_header.py 顶部说明。")
print(f" * 数据源：pinyin-engine/data/luna_pinyin.dict.yaml，经 src/syllables.py")
print(f" * 提取，共 {len(syllables)} 个合法拼音音节（不含声调），最长 {max_len} 个字母。 */")
print("#ifndef CJ_SYLLABLES_DATA_H")
print("#define CJ_SYLLABLES_DATA_H")
print()
print(f"#define CJ_SYLLABLE_COUNT {len(syllables)}")
print(f"#define CJ_MAX_SYLLABLE_LEN {max_len}")
print()
print("/* 按字典序排列，segment.c 用二分查找定位。 */")
print("static const char *const CJ_SYLLABLES[CJ_SYLLABLE_COUNT] = {")
for s in syllables:
    print(f'    "{s}",')
print("};")
print()
print("#endif /* CJ_SYLLABLES_DATA_H */")
