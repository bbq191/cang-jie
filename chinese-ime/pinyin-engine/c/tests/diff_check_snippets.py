#!/usr/bin/env python3
"""snippets 差分测试：同一份文件内容 + 同一批 lookup 分别跑 Python 参照
（src/snippets.py）和 C 移植版（/tmp/diff_cli_snippets），逐行比对，要求
完全一致（parse 的行合法性判定 + lookup 的文件序返回都是确定性规则，
无容许差异）。跑法：

    cd pinyin-engine
    ../../rmfw/.venv/bin/python3 c/tests/diff_check_snippets.py
"""
import random
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
from src.snippets import Snippets, MAX_ENTRIES, SHORTCUT_MAX, PHRASE_MAX  # noqa: E402


def build_content() -> str:
    rng = random.Random(20260822)
    lines = [
        "dz\t北京市海淀区中关村大街1号",
        "omw\tOn my way!",
        "vx\twxid_abc",
        "vx\t13800138000",         # 重复缩写
        "UP\tbad",                  # 大写
        "a1\tbad",                  # 数字
        "\tno-shortcut",
        "noph\t",
        "twotab\ta\tb",
        "notab",
        "x" * (SHORTCUT_MAX + 1) + "\ttoolong",
        "x" * SHORTCUT_MAX + "\t压线缩写",
        "ph\t" + "y" * PHRASE_MAX,  # 压线短语
        "ph\t" + "y" * (PHRASE_MAX + 1),
        "",
        "mail\tsomeone@example.com",
    ]
    # 随机小写字母行灌到超容量，覆盖 256 上限
    for _ in range(MAX_ENTRIES + 30):
        sc = "".join(rng.choice("abcdefgh") for _ in range(rng.randint(1, 4)))
        ph = "短语%d" % rng.randint(0, 999)
        lines.append(f"{sc}\t{ph}")
    return "\n".join(lines) + "\n"


def build_queries() -> list[str]:
    rng = random.Random(42)
    qs = ["dz", "omw", "vx", "mail", "ph", "up", "a1", "notab",
          "x" * SHORTCUT_MAX, "x" * (SHORTCUT_MAX + 1) if False else "zzz"]
    for _ in range(500):
        qs.append("".join(rng.choice("abcdefgh") for _ in range(rng.randint(1, 5))))
    return qs


def main() -> int:
    content = build_content()
    queries = build_queries()

    s = Snippets()
    s.load_bytes(content.encode("utf-8"))
    expected = ["loaded %d" % len(s)]
    expected += ["|".join(s.lookup(q)) for q in queries]

    content_lines = content.split("\n")
    if content_lines and content_lines[-1] == "":
        content_lines.pop()
    stdin = "C %d\n" % len(content_lines)
    stdin += "\n".join(content_lines) + "\n"
    stdin += "\n".join("L\t%s" % q for q in queries) + "\n"

    proc = subprocess.run(["/tmp/diff_cli_snippets"], input=stdin,
                          capture_output=True, text=True)
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
                print(f"第 {i} 条不一致\n  py: {e!r}\n  c : {g!r}")
    if len(expected) != len(got):
        print(f"输出行数不一致 py={len(expected)} c={len(got)}")
        bad += 1
    if bad:
        print(f"差分测试失败：{bad} 处不一致")
        return 1
    print(f"snippets 差分测试通过：parse + {len(queries)} 条 lookup 全部一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
