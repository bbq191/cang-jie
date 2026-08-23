#!/usr/bin/env python3
"""生成设备端英文候选用的二进制词典 english.bin——Phase C 中英混输。

跟 dict.bin/dict.zh_tw.bin 是**同一个 CJDICT01 格式**（共用 blob_format.py，
C 端 dictionary.c 一行不改就能 cj_dict_open 打开它）：key = 英文词本身
（小写 ASCII），单候选 = 该词，weight = 缩放后的词频。这样 cj_dict_lookup_prefix
的"按权重 top-k + 空格数过滤"就直接变成"按词频排序的英文自动补全"——
英文词 0 空格、缓冲区 0 空格，空格过滤天然通过。见 data/PROVENANCE.english.md。

数据源：SymSpell frequency_dictionary_en_82_765.txt（词 词频，两列），
钉 commit c239062，MIT 分发、派生自 Google Books Ngram(CC BY 3.0)/SCOWL。

词频缩放：SymSpell 词频十亿级（"the"=23,135,851,162）超 uint32 上限，
按常量除数等比缩放进 uint32——只用于 top-k 相对排序，绝对值无意义、
除以常量单调保序，极少数超顶词并列最高位可接受。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_english_blob.py [输出目录，默认 c/build]
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from src.dictionary import Candidate  # noqa: E402
from blob_format import build_dict_blob, MAX_U32  # noqa: E402

DATA_DIR = Path(__file__).resolve().parent.parent.parent / "data"
ENGLISH_TXT = DATA_DIR / "english" / "frequency_dictionary_en_82_765.txt"


def load_english(path: Path):
    """返回 [(word, freq)]，只保留纯 [a-z] 小写词，去重（保留首次/最高频，
    SymSpell 已按词频降序，首次即最高）。记录丢弃计数。"""
    words = {}
    dropped_nonalpha = 0
    with path.open("r", encoding="utf-8-sig") as f:  # utf-8-sig 吃掉首行 BOM
        for line in f:
            parts = line.split()
            if len(parts) < 2:
                continue
            word, freq_str = parts[0], parts[1]
            if not word.isascii() or not word.isalpha() or not word.islower():
                dropped_nonalpha += 1
                continue
            try:
                freq = int(freq_str)
            except ValueError:
                dropped_nonalpha += 1
                continue
            if word not in words:  # 去重：SymSpell 降序，首次是最高频
                words[word] = freq
    return words, dropped_nonalpha


def main():
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "build"
    out_dir.mkdir(parents=True, exist_ok=True)

    words, dropped = load_english(ENGLISH_TXT)
    if not words:
        raise SystemExit(f"没有从 {ENGLISH_TXT} 读到任何词，检查路径/格式")

    max_freq = max(words.values())
    # 除数取 ceil(max_freq / MAX_U32)，保证缩放后最大值 <= MAX_U32。
    divisor = (max_freq + MAX_U32 - 1) // MAX_U32
    if divisor < 1:
        divisor = 1

    # 表：key = (word,) 一元组 → build_dict_blob 里 " ".join 得到词本身；
    # 单候选 = 该词，weight = 缩放后词频（>=1，保证在词典里的词权重非零）。
    table = {}
    for word, freq in words.items():
        weight = max(1, freq // divisor)
        table[(word,)] = [Candidate(word=word, weight=float(weight))]

    source = f"SymSpell frequency_dictionary_en_82_765.txt @c239062（MIT/CC BY 3.0，除数={divisor} 缩放进 uint32）"
    blob, manifest = build_dict_blob(table, [source])
    manifest["english_word_count"] = len(words)
    manifest["dropped_nonalpha"] = dropped
    manifest["freq_divisor"] = divisor
    manifest["max_freq_raw"] = max_freq

    english_bin = out_dir / "english.bin"
    english_bin.write_bytes(blob)
    manifest_path = out_dir / "english_blob_manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"写入 {english_bin}（{len(blob):,} 字节 = {len(blob) / 1024 / 1024:.1f} MiB）")
    print(f"收录 {len(words):,} 词，丢弃非[a-z] {dropped} 条，除数={divisor}")
    print(f"写入 {manifest_path}")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
