#!/usr/bin/env python3
"""差分测试：拿同一批测试字符串分别跑 Python 版 segment() 和 C 移植版
diff_cli，逐行比较输出，验证两边行为完全一致（不是抽样几个单测用例，
是更大范围的交叉核对）。跑法：

    cd pinyin-engine
    .venv/bin/python3 c/tests/diff_check.py
"""
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
from src.segment import segment  # noqa: E402

# 已知且预期的差异——不是 bug：这几个字符串太长/歧义组合数太大，
# 触发了 C 移植版故意加的候选数量/工作量上限（见 segment.h 顶部说明），
# Python 版没有这个上限会枚举出全部歧义组合。只要求 C 版的候选列表是
# Python 版列表的一个"前缀子集"（尤其是 [0] 推荐结果必须一致），
# 不要求逐条相同。
KNOWN_TRUNCATION_EXCEPTIONS = {"jintiantianqizenmeyang"}

TEST_STRINGS = [
    "", "a", "e", "wo", "shi", "ni", "hao",
    "nihao", "zhongguo", "beijing", "xianggang", "shanghai", "putonghua",
    "xian", "an", "yan", "ang",
    "xi'an", "ang'gang", "ke'ai", "xi'an'shi",
    "zho", "z", "beiji", "nihaoz", "ni'ha",
    "woshizhongguoren", "zhongwenshurufa", "jintiantianqizenmeyang",
    "qwqwqw", "zzz", "bia", "aaa", "aaaaaaaaaa",
    "NiHao", "XIANGGANG",  # 大小写混合，验证小写化
    "n", "ng", "hm", "m",  # 语气词音节
    "shishishishishishi",  # 高重复但都是合法音节，考验候选枚举
]


def python_ref(raw: str) -> str:
    results = segment(raw)
    lines = []
    for r in results:
        parts = list(r.syllables)
        if r.pending:
            parts.append(f"[{r.pending}]")
        lines.append("+".join(parts) if parts else "")
    return "|".join(lines)


def main():
    c_dir = Path(__file__).resolve().parent.parent
    proc = subprocess.run(
        ["/tmp/diff_cli"],
        input="\n".join(TEST_STRINGS) + "\n",
        capture_output=True,
        text=True,
        cwd=c_dir,
    )
    c_lines = proc.stdout.splitlines()

    mismatches = 0
    for i, raw in enumerate(TEST_STRINGS):
        expected = python_ref(raw)
        actual = c_lines[i] if i < len(c_lines) else "<missing>"
        if actual == expected:
            status = "OK"
        elif raw in KNOWN_TRUNCATION_EXCEPTIONS:
            # 只要求 C 版候选列表的每一项都真实出现在 Python 版列表里
            # （不是瞎编的），且 [0] 推荐结果一致——数量上限截断是预期的。
            c_cands = actual.split("|")
            py_cands = expected.split("|")
            if c_cands and c_cands[0] == py_cands[0] and all(c in py_cands for c in c_cands):
                status = "OK(截断，符合预期)"
            else:
                status = "MISMATCH"
                mismatches += 1
        else:
            status = "MISMATCH"
            mismatches += 1
        expected_display = expected if raw not in KNOWN_TRUNCATION_EXCEPTIONS \
            else f"<{expected.count('|') + 1} 种候选，太长不全打>"
        print(f"[{status}] {raw!r:30} python={expected_display!r:30} c={actual!r}")

    print()
    if mismatches:
        print(f"=== {mismatches}/{len(TEST_STRINGS)} 条不一致 ===")
        sys.exit(1)
    else:
        print(f"=== 全部 {len(TEST_STRINGS)} 条一致 ===")


if __name__ == "__main__":
    main()
