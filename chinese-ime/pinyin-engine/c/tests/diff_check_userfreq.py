#!/usr/bin/env python3
"""userfreq 差分测试：同一串操作分别喂 Python 参照实现（src/userfreq.py）
和 C 移植版（/tmp/diff_cli_userfreq），逐行比对输出，要求**完全一致**
（userfreq 的排序/淘汰规则全部钉死了确定性 tie-break，不存在 dict/jianpin
那条"同权重顺序容许差异"）。跑法：

    cd pinyin-engine
    ../rmfw/.venv/bin/python3 c/tests/diff_check_userfreq.py

覆盖：定向边界（饱和/超长/淘汰三分支/并列排序）+ 种子随机操作流
（含灌满 2048 条后的持续淘汰——规模边界正是本项目差分测试历史上两次
抓出 bug 的地方）。"""
import random
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
from src.userfreq import UserFreq, MAX_ENTRIES, COUNT_CAP  # noqa: E402

WORDS = ["你好", "拼音", "是", "时", "事", "行", "银行", "中国", "输入法",
         "洛天依", "破坏", "得", "的", "地", "新", "词"]
KEYS = ["ni hao", "pin yin", "shi", "xing", "yin hang", "zhong guo",
        "shu ru fa", "nh", "py", "zg", "srf", "de", "s", "ni h"]


def build_ops() -> list[str]:
    ops: list[str] = []
    # ---- 定向边界 ----
    ops.append("N\t%d\tcap\t满" % COUNT_CAP)
    ops.append("R\tcap\t满")                 # 饱和
    ops.append("Q\tcap\t满")
    ops.append("R\t%s\t超" % ("k" * 65))     # key 超长 → 忽略
    ops.append("R\tk\t%s" % ("字" * 17))     # word 超长（51 字节）→ 忽略
    ops.append("D")
    # words_for_key 并列排序
    for w, n in [("是", 3), ("时", 5), ("事", 3)]:
        ops.append("N\t%d\tshi\t%s" % (n, w))
    ops.append("W\t10\tshi")
    ops.append("W\t1\tshi")
    ops.append("W\t10\t没有的key")
    # order 两种模式 + 稳定性
    ops.append("O\tk\tshi\t的,时,事,是")
    ops.append("O\tu\t别的key\t的,时,事,是")
    ops.append("O\tk\t别的key\t的,时,事,是")
    # ---- 淘汰风暴：灌满 2048 后持续插入，覆盖三条淘汰分支 ----
    rng = random.Random(20260822)
    for i in range(MAX_ENTRIES + 400):
        ops.append("N\t%d\tk%04x\t词" % (rng.randint(1, 3), i))
    ops.append("D")
    # 满员后：更强插入 / 更弱丢弃 / 并列看字节序
    ops.append("N\t9\tzzzz强\t新")
    ops.append("Q\tzzzz强\t新")
    ops.append("R\tzzzz弱\t新")
    ops.append("Q\tzzzz弱\t新")
    ops.append("D")
    # ---- 种子随机操作流 ----
    for _ in range(3000):
        r = rng.random()
        key = rng.choice(KEYS)
        word = rng.choice(WORDS)
        if r < 0.45:
            ops.append("R\t%s\t%s" % (key, word))
        elif r < 0.60:
            ops.append("N\t%d\t%s\t%s" % (rng.randint(1, 50), key, word))
        elif r < 0.66:
            ops.append("Q\t%s\t%s" % (key, word))
        elif r < 0.72:
            ops.append("E\t%s\t%s" % (key, word))
        elif r < 0.82:
            ops.append("T\t%s" % word)
        elif r < 0.90:
            ops.append("W\t%d\t%s" % (rng.randint(1, 8), key))
        else:
            sample = rng.sample(WORDS, rng.randint(1, 6))
            mode = rng.choice(["u", "k"])
            ops.append("O\t%s\t%s\t%s" % (mode, key, ",".join(sample)))
    ops.append("D")
    return ops


def python_ref(ops: list[str]) -> list[str]:
    uf = UserFreq()
    out: list[str] = []
    for line in ops:
        f = line.split("\t")
        op = f[0]
        if op == "R":
            uf.record(f[1], f[2])
            out.append("ok")
        elif op == "N":
            uf.record_n(f[2], f[3], int(f[1]))
            out.append("ok")
        elif op == "Q":
            out.append(str(uf.count(f[1], f[2])))
        elif op == "E":
            out.append(str(uf.effective(f[1], f[2])))
        elif op == "T":
            out.append(str(uf.word_total(f[1])))
        elif op == "W":
            rows = uf.words_for_key(f[2], int(f[1]))
            out.append("|".join(f"{w}:{c}" for w, c in rows))
        elif op == "O":
            words = f[3].split(",")
            perm = uf.order(f[2], words, f[1] == "u")
            out.append(",".join(str(i) for i in perm))
        elif op == "D":
            out.append(uf.dump().decode("utf-8").replace("\n", "|").rstrip("|"))
        else:
            out.append("?")
    return out


def main() -> int:
    ops = build_ops()
    expected = python_ref(ops)
    proc = subprocess.run(
        ["/tmp/diff_cli_userfreq"],
        input="\n".join(ops) + "\n",
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print("C 驱动退出码非 0：", proc.returncode, proc.stderr, file=sys.stderr)
        return 1
    got = proc.stdout.split("\n")
    if got and got[-1] == "":
        got.pop()
    bad = 0
    for i, (e, g) in enumerate(zip(expected, got)):
        if e != g:
            bad += 1
            if bad <= 5:
                print(f"第 {i} 条不一致\n  op: {ops[i]!r}\n  py: {e!r}\n  c : {g!r}")
    if len(expected) != len(got):
        print(f"输出行数不一致 py={len(expected)} c={len(got)}")
        bad += 1
    if bad:
        print(f"差分测试失败：{bad} 处不一致 / {len(ops)} 条操作")
        return 1
    print(f"userfreq 差分测试通过：{len(ops)} 条操作全部一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
