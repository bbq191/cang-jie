#!/usr/bin/env python3
"""差分测试：拿同一批查询分别跑 Python 版 Dictionary.lookup()/lookup_prefix()
和 C 移植版 diff_cli_dict，逐行比较输出，验证两边行为完全一致。跑法：

    cd pinyin-engine
    .venv/bin/python3 c/tests/diff_check_dict.py

跟 c/tests/diff_check.py（segment 那次）同一个模式，唯一预期的差异是
"权重相同的候选之间的先后顺序"——Python 参照实现是稳定排序（同权重保留
原始遍历顺序），C 版权重相同时按候选词字节序 tie-break（见
dictionary.c::cj_dict_cand_desc_cmp 的注释），如果测试用例里出现真实的
同权重候选，只要求集合相同、[0] 推荐结果一致，不强求逐条顺序相同。
"""
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
from src.dictionary import Dictionary, FULL_DICT_PATHS  # noqa: E402

PREFIX_LIMIT = 50  # 跟 diff_cli_dict.c 里的 PREFIX_LIMIT 对齐

# (mode, syllables_tuple) —— mode "L"=精确匹配，"P"=前缀匹配（最后一个
# 元素是"打到一半"的部分音节）。覆盖：常见词、单字、生僻字排序、未知
# 音节、前缀匹配、真实碰撞用例（字节前缀相同但音节数不同）、空结果。
TEST_QUERIES = [
    ("L", ("ni", "hao")),
    ("L", ("guo",)),
    ("L", ("de",)),
    ("L", ("zhong", "guo")),
    ("L", ("wo",)),
    ("L", ("shi",)),
    ("L", ("zzz",)),           # 未知音节，精确匹配应为空
    ("L", ("pin", "yin")),
    ("L", ("beijing",)),        # 不是合法拼音切分，词典里也不该有这个 key
    ("P", ("ni", "h")),
    ("P", ("ni", "ha")),        # 真实碰撞用例：字节前缀跟 ("ni","hao","a") 等 3 音节 key 重叠
    ("P", ("zhong", "g")),
    ("P", ("p",)),
    ("P", ("d",)),
    ("P", ("zzzzz",)),          # 前缀匹配也应为空
    ("P", ("ni",)),             # 单音节前缀，命中所有 ni 开头的合法后续
    ("L", ("a",)),
    ("L", ("n",)),               # 语气词音节
    ("P", ("sh",)),
]


def python_ref(mode: str, syllables: tuple[str, ...], d: Dictionary) -> str:
    if mode == "L":
        cands = d.lookup(syllables)
    else:
        cands = d.lookup_prefix(syllables, limit=PREFIX_LIMIT)
    return "|".join(f"{c.word}:{int(round(c.weight))}" for c in cands)


def main():
    d = Dictionary(FULL_DICT_PATHS, normalize_simplified=True)  # 跟 gen_dict_blob 一致：全拼 dict.bin 含 iorest + 繁体归一化
    c_dir = Path(__file__).resolve().parent.parent
    input_lines = [f"{mode} {' '.join(syl)}" for mode, syl in TEST_QUERIES]
    proc = subprocess.run(
        ["/tmp/diff_cli_dict"],
        input="\n".join(input_lines) + "\n",
        capture_output=True,
        text=True,
        cwd=c_dir,
    )
    if proc.returncode != 0:
        print("diff_cli_dict 运行失败：", proc.stderr)
        sys.exit(1)
    c_lines = proc.stdout.splitlines()

    mismatches = 0
    for i, (mode, syllables) in enumerate(TEST_QUERIES):
        expected = python_ref(mode, syllables, d)
        actual = c_lines[i] if i < len(c_lines) else "<missing>"
        label = f"{mode} {' '.join(syllables)}"

        if actual == expected:
            status = "OK"
        else:
            # 只在"候选集合相同、只是同权重候选先后顺序不同"这种情况下放行，
            # 其它任何差异（缺候选/多候选/[0] 不一致/权重不一致）都算失败。
            exp_set = set(expected.split("|")) if expected else set()
            act_set = set(actual.split("|")) if actual else set()
            exp_first = expected.split("|")[0] if expected else None
            act_first = actual.split("|")[0] if actual else None
            if exp_set == act_set and exp_first == act_first:
                status = "OK(同权重 tie-break 顺序不同，符合预期)"
            else:
                status = "MISMATCH"
                mismatches += 1

        print(f"[{status}] {label:24} python={expected!r:60} c={actual!r}")

    print()
    if mismatches:
        print(f"=== {mismatches}/{len(TEST_QUERIES)} 条不一致 ===")
        sys.exit(1)
    else:
        print(f"=== 全部 {len(TEST_QUERIES)} 条一致 ===")


if __name__ == "__main__":
    main()
