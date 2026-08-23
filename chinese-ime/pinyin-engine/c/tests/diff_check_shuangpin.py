#!/usr/bin/env python3
"""差分测试：拿同一批双拼按键序列分别跑 Python 版
ShuangpinScheme.keys_to_pinyin(scheme="flypy") 和 C 移植版
diff_cli_shuangpin（查表实现），逐行比较输出，验证两边行为完全一致。

跟 c/tests/diff_check_jianpin.py 不同的一点：这里穷举全部 676 种
2 键组合（不是挑几条代表性用例）——C 版是穷举打表生成的（见
gen_shuangpin_table.py），差分测试也穷举，才能真正验证"表打对了"，
挑样本测不出"表里第 300 行手滑打错一个字符"这种问题。另外加几条
多音节词/pending 半音节场景，覆盖分隔符拼接逻辑本身（这部分不是
查表，是 C 版自己实现的，穷举单音节测不到）。

跑法：
    cd pinyin-engine
    .venv/bin/python3 c/tests/diff_check_shuangpin.py
"""
import string
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
from src.shuangpin import get_scheme  # noqa: E402

MULTI_SYLLABLE_CASES = [
    "vsgo",   # zhong'guo
    "hcgo",   # hao'guo（凑的按键，不要求是常见词，只测拼接逻辑）
    "niz",    # ni'z（pending）
    "z",      # 单独一个 pending 键，没有任何完整音节
    "",       # 空输入
    "vsgohc", # 三个音节
]


def main():
    scheme = get_scheme("flypy")

    all_keys = [a + b for a in string.ascii_lowercase for b in string.ascii_lowercase]
    all_keys += MULTI_SYLLABLE_CASES

    c_dir = Path(__file__).resolve().parent.parent
    proc = subprocess.run(
        ["/tmp/diff_cli_shuangpin"],
        input="\n".join(all_keys) + "\n",
        capture_output=True,
        text=True,
        cwd=c_dir,
    )
    if proc.returncode != 0:
        print("diff_cli_shuangpin 运行失败：", proc.stderr)
        sys.exit(1)
    c_lines = proc.stdout.splitlines()

    mismatches = 0
    for i, keys in enumerate(all_keys):
        expected = scheme.keys_to_pinyin(keys) if keys else ""
        actual = c_lines[i] if i < len(c_lines) else "<missing>"
        if actual != expected:
            mismatches += 1
            print(f"[MISMATCH] keys={keys!r:10} python={expected!r} c={actual!r}")

    print()
    if mismatches:
        print(f"=== {mismatches}/{len(all_keys)} 条不一致 ===")
        sys.exit(1)
    else:
        print(f"=== 全部 {len(all_keys)} 条一致（含穷举的 676 种 2 键组合）===")


if __name__ == "__main__":
    main()
