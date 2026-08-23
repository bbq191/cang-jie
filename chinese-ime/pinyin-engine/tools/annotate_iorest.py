#!/usr/bin/env python3
"""离线注音工具：把 Iorest/rime-dict 的 23 个专题词库（绝大多数无拼音列，
靠 Rime 部署时自动注音）用 pypinyin 短语级注音，产出单个规范化
data/iorest.dict.yaml（词\\t拼音\\t词频），供 Dictionary 走 M5 同管线消费。

构建期依赖 pypinyin（MIT/CC，不装进设备/.so）；产物 iorest.dict.yaml 是
静态文件、提交进仓库，日常构建不需要 pypinyin。数据源/许可证事实见
data/PROVENANCE.iorest.md。

用法：<装了 pypinyin 的 python> pinyin-engine/tools/annotate_iorest.py
"""
from __future__ import annotations

import sys
from pathlib import Path

import importlib.util
# 直接按文件路径加载 syllables.py，绕开 src/__init__.py（它会连锁 import
# engine/shuangpin/traditional，拉入 opencc 等运行时依赖，注音这步用不到）。
_syl_path = Path(__file__).resolve().parent.parent / "src" / "syllables.py"
_spec = importlib.util.spec_from_file_location("cj_syllables", _syl_path)
_syl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_syl)
is_valid_syllable = _syl.is_valid_syllable
DATA_DIR = _syl.DATA_DIR

try:
    from pypinyin import lazy_pinyin, Style
except ImportError:
    sys.exit("需要 pypinyin（仅注音这一步）：python -m venv venv && venv/bin/pip install pypinyin")

IN_DIR = DATA_DIR / "iorest"
OUT_PATH = DATA_DIR / "iorest.dict.yaml"
# Iorest 权重无信息量（basis 多为 1、daily 无权重、跨文件不一致）——统一给一个
# 低常量，排在 rime-ice base 真实词频之后（覆盖优先、不污染常用词排序）。
IOREST_WEIGHT = 1


def is_han(ch: str) -> bool:
    o = ord(ch)
    return (0x4E00 <= o <= 0x9FFF or 0x3400 <= o <= 0x4DBF or
            0x20000 <= o <= 0x2A6DF or 0xF900 <= o <= 0xFAFF)


def all_han(word: str) -> bool:
    return bool(word) and all(is_han(c) for c in word)


def valid_pinyin(syls: list[str]) -> bool:
    return bool(syls) and all(is_valid_syllable(s) for s in syls)


def parse_line(line: str):
    """返回 (word, pinyin_str|None)；pinyin_str 为 None 表示需要自动注音。
    Rime 列是位置式：词 / 编码(拼音) / 权重。Iorest 对纯汉字词删了拼音列，
    所以第二列可能是权重（纯数字）也可能是拼音——按内容区分。畸形行返回 None。"""
    parts = line.rstrip("\n").split("\t")
    word = parts[0]
    if not word:
        return None
    if len(parts) == 1:
        return (word, None)                      # 纯词，需注音
    col2 = parts[1].strip()
    if col2.isdigit():
        return (word, None)                      # 词+权重，需注音
    return (word, col2)                          # 词+拼音(+权重)


def iter_data_lines(path: Path):
    started = False
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not started:
            if raw.strip() == "...":
                started = True
            continue
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        yield raw


def main():
    files = sorted(IN_DIR.glob("luna_pinyin.*.dict.yaml"))
    if not files:
        sys.exit(f"未找到 Iorest 词库：{IN_DIR}")

    seen: set[tuple[str, str]] = set()   # (word, pinyin) 去重
    out: list[tuple[str, str]] = []
    stat = {"total": 0, "kept": 0, "annotated": 0,
            "drop_nonhan": 0, "drop_badpinyin": 0, "drop_malformed": 0, "dup": 0}

    for path in files:
        for raw in iter_data_lines(path):
            stat["total"] += 1
            parsed = parse_line(raw)
            if parsed is None:
                stat["drop_malformed"] += 1
                continue
            word, given = parsed
            if given is not None:
                syls = given.split()
                if not valid_pinyin(syls):
                    stat["drop_badpinyin"] += 1   # 占位"v v"/含拉丁的编码等
                    continue
                pinyin = " ".join(syls)
                stat["kept"] += 1
            else:
                if not all_han(word):
                    stat["drop_nonhan"] += 1        # 含数字/拉丁/emoji/颜文字
                    continue
                syls = lazy_pinyin(word, style=Style.NORMAL)  # 默认 ü→v
                if not valid_pinyin(syls):
                    stat["drop_badpinyin"] += 1
                    continue
                pinyin = " ".join(syls)
                stat["annotated"] += 1
            key = (word, pinyin)
            if key in seen:
                stat["dup"] += 1
                continue
            seen.add(key)
            out.append((word, pinyin))

    OUT_PATH.write_text(
        "# Iorest/rime-dict 合并注音词库（pypinyin 短语级注音，构建期生成）\n"
        "# 来源/许可证事实见 data/PROVENANCE.iorest.md（个人自用取舍，非法律意见）\n"
        "# 生成：pinyin-engine/tools/annotate_iorest.py\n"
        "---\n"
        "name: iorest\n"
        "version: \"1.0\"\n"
        "sort: by_weight\n"
        "...\n"
        + "".join(f"{w}\t{p}\t{IOREST_WEIGHT}\n" for w, p in out),
        encoding="utf-8",
    )

    print(f"输入 {stat['total']} 行 → 输出 {len(out)} 条")
    print(f"  已有合法拼音保留 kept={stat['kept']}  pypinyin 注音 annotated={stat['annotated']}")
    print(f"  丢弃：非汉字 {stat['drop_nonhan']}  拼音无效 {stat['drop_badpinyin']}  "
          f"畸形 {stat['drop_malformed']}  重复 {stat['dup']}")
    print(f"  写入 {OUT_PATH}")


if __name__ == "__main__":
    main()
