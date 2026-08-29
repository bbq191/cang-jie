#!/usr/bin/env python3
"""B1 手写识别 de-risk：逐字 CER（字符错误率）打分器。

CER = 编辑距离(GT, HYP) / len(GT)，只比汉字核心（剥掉数字/标点/空白，避免带圈数字等
干扰）。用于对拍"盲测视觉 agent 转写"对"手写原文 ground truth"的识别率。

用法：
  echo "<识别转写>" | python hw_cer.py "<ground_truth>"
  python hw_cer.py "<ground_truth>" <hyp_file>
"""
import re
import sys


def lev(a: str, b: str) -> int:
    m, n = len(a), len(b)
    d = list(range(n + 1))
    for i in range(1, m + 1):
        prev, d[0] = d[0], i
        for j in range(1, n + 1):
            prev, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, prev + (a[i - 1] != b[j - 1]))
    return d[n]


def han(s: str) -> str:
    return re.sub(r"[^一-鿿]", "", s)


def score(gt: str, hyp: str) -> None:
    g, h = han(gt), han(hyp)
    dist = lev(g, h)
    cer = dist / max(1, len(g))
    print(f"GT ({len(g)}字): {g}")
    print(f"HYP({len(h)}字): {h}")
    print(f"编辑距离={dist}  CER={cer:.0%}  正确率={1 - cer:.0%}")


if __name__ == "__main__":
    gt = sys.argv[1]
    hyp = open(sys.argv[2]).read() if len(sys.argv) > 2 else sys.stdin.read()
    score(gt, hyp)
