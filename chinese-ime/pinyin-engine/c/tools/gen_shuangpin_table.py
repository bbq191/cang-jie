#!/usr/bin/env python3
"""从 Python 版 src/shuangpin.py 生成 C 版双拼（小鹤 flypy 方案）键位
解码用的静态查找表（shuangpin_data.h）。

跟 gen_syllables_header.py 是同一个模式：真相来源仍然是 Python 端
`ShuangpinScheme`（照搬 RIME 官方 `double_pinyin_flypy.schema.yaml`
的 algebra 正则替换规则链解码），C 端不重新实现一套正则引擎——那样
既要在 C 里嵌入一个正则库（新增重量级依赖，risk 类别跟 05 节排除
"把 OpenCC 静态链接进 .so"是同一个考量），也有"规则链跟 Python 版
悄悄不同步"的风险。改成穷举所有可能的 2 键组合、把 Python 解码结果
整张表打印成 C 静态数组，运行时纯查表，零正则、零动态解析。

之所以能穷举打表：已经验证过双拼解码是"逐 2 键分组独立解码，组间
互不影响"（键盘上打的每 2 个键只对应 1 个拼音音节，不依赖前后相邻
音节的按键内容）——测试过跨音节边界不会互相干扰（分开解码再拼接
跟整体解码结果逐字节相同），所以只需要穷举 26x26=676 种 2 键组合。

范围收窄（04 节"本轮明确"）：只生成小鹤双拼（flypy）一种方案，不生成
自然码（natural）——`shuangpin.py`/`ShuangpinScheme` 仍然保留按 schema
名字加载的通用能力，以后如果要支持第二套方案，重新跑一遍这个脚本、
换个 scheme 名字即可，不需要改这个脚本的逻辑本身。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_shuangpin_table.py > c/src/shuangpin_data.h
"""
from __future__ import annotations

import string
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from src.shuangpin import get_scheme  # noqa: E402

SCHEME_NAME = "flypy"


def main():
    scheme = get_scheme(SCHEME_NAME)
    rows = []
    max_len = 0
    for a in string.ascii_lowercase:
        for b in string.ascii_lowercase:
            decoded = scheme.keys_to_pinyin(a + b)
            max_len = max(max_len, len(decoded))
            rows.append(decoded)

    # +1 给 NUL 结尾；用穷举得到的实际最大长度定数组宽度，不是拍脑袋的
    # 常量——数据源（schema 文件）以后如果换了规则链导致某个音节变长，
    # 这个脚本重新跑一遍会自动反映出新的宽度，不会悄悄截断。
    col_width = max_len + 1

    print("/* 自动生成，来源：pinyin-engine/src/shuangpin.py + "
          f"{SCHEME_NAME} scheme（data/double_pinyin_{SCHEME_NAME}.schema.yaml）。")
    print(" * 不要手改——跑 `python3 c/tools/gen_shuangpin_table.py > "
          "c/src/shuangpin_data.h` 重新生成。")
    print(" *")
    print(" * 穷举全部 26x26=676 种 2 键组合的解码结果（已验证组间解码互不")
    print(" * 干扰，见生成脚本顶部说明），下标 = (第一个键-'a')*26 + "
          "(第二个键-'a')。")
    print(" * 每项是解码出的拼音片段（NUL 结尾 ASCII 字符串，不含分隔符）。 */")
    print(f"#define CJ_SHUANGPIN_FLYPY_MAX_LEN {max_len}")
    print(f"static const char CJ_SHUANGPIN_FLYPY_TABLE[676][{col_width}] = {{")
    for i, text in enumerate(rows):
        a = string.ascii_lowercase[i // 26]
        b = string.ascii_lowercase[i % 26]
        print(f'    "{text}", /* {a}{b} */')
    print("};")


if __name__ == "__main__":
    main()
