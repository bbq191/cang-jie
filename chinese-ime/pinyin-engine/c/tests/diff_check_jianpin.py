#!/usr/bin/env python3
"""差分测试：拿同一批查询分别跑 Python 版 JianpinIndex.lookup()/
lookup_prefix() 和 C 移植版 diff_cli_jianpin，逐行比较输出，验证两边
行为完全一致。跑法：

    cd pinyin-engine
    .venv/bin/python3 c/tests/diff_check_jianpin.py

跟 c/tests/diff_check_dict.py 同一个模式，唯一预期的差异同样是"权重
相同的候选之间的先后顺序"（tie-break 实现方式不同，两边都是稳定排序
但排序前的原始遍历顺序不一致），只要求集合相同、[0] 推荐结果一致，
不强求逐条顺序相同。
"""
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
from src.dictionary import Dictionary  # noqa: E402
from src.jianpin import JianpinIndex  # noqa: E402

PREFIX_LIMIT = 50  # 跟 diff_cli_jianpin.c 里的 PREFIX_LIMIT 对齐

# (mode, key) —— mode "L"=精确匹配，"P"=前缀匹配。覆盖：常见多音节词、
# 单音节 key（应为空，见 07 节"索引只收多音节词"设计）、前缀命中更长词、
# 高频 key（"z" 覆盖所有 z/zh 声母开头的词）、未知 key、空前缀。
TEST_QUERIES = [
    ("L", "zg"),        # zhong+guo -> 中国
    ("L", "py"),         # pin+yin -> 拼音（也覆盖 pei+yang/peng+you 等同 key 词）
    ("L", "g"),          # 单音节 key，索引里不该存在，应为空
    ("L", "p"),          # 同上
    ("L", "zzzzzz"),     # 未知 key
    ("P", "py"),         # 前缀匹配：拼音 + 拼音输入法（pysrf）等更长词
    ("P", "z"),          # 高频前缀，覆盖 z/zh 声母开头的大量词
    ("P", "d"),
    ("P", "sh"),         # sh 开头，跟 06 节 dictionary.c 那次踩过坑的音节呼应
    ("P", "zzzzz"),      # 未知前缀
    ("P", ""),           # 空前缀应为空，不是"匹配一切"
]


def python_ref(mode: str, key: str, idx: JianpinIndex) -> str:
    if mode == "L":
        cands = idx.lookup(key, limit=100000)
    else:
        cands = idx.lookup_prefix(key, limit=PREFIX_LIMIT)
    return "|".join(f"{c.word}:{int(round(c.weight))}" for c in cands)


def main():
    d = Dictionary(normalize_simplified=True)  # 跟 gen_jianpin_blob 一致：繁体归一化
    idx = JianpinIndex(d)
    c_dir = Path(__file__).resolve().parent.parent
    input_lines = [f"{mode} {key}" for mode, key in TEST_QUERIES]
    proc = subprocess.run(
        ["/tmp/diff_cli_jianpin"],
        input="\n".join(input_lines) + "\n",
        capture_output=True,
        text=True,
        cwd=c_dir,
    )
    if proc.returncode != 0:
        print("diff_cli_jianpin 运行失败：", proc.stderr)
        sys.exit(1)
    c_lines = proc.stdout.splitlines()

    mismatches = 0
    for i, (mode, key) in enumerate(TEST_QUERIES):
        expected = python_ref(mode, key, idx)
        actual = c_lines[i] if i < len(c_lines) else "<missing>"
        label = f"{mode} {key!r}"

        if actual == expected:
            status = "OK"
        else:
            exp_set = set(expected.split("|")) if expected else set()
            act_set = set(actual.split("|")) if actual else set()
            exp_first = expected.split("|")[0] if expected else None
            act_first = actual.split("|")[0] if actual else None
            if exp_set == act_set and exp_first == act_first:
                status = "OK(同权重 tie-break 顺序不同，符合预期)"
            else:
                status = "MISMATCH"
                mismatches += 1

        exp_preview = expected if len(expected) <= 60 else expected[:57] + "..."
        act_preview = actual if len(actual) <= 60 else actual[:57] + "..."
        print(f"[{status}] {label:16} python={exp_preview!r:64} c={act_preview!r}")

    print()
    if mismatches:
        print(f"=== {mismatches}/{len(TEST_QUERIES)} 条不一致 ===")
        sys.exit(1)
    else:
        print(f"=== 全部 {len(TEST_QUERIES)} 条一致 ===")


if __name__ == "__main__":
    main()
