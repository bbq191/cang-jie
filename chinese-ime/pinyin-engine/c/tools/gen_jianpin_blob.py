#!/usr/bin/env python3
"""从 Python 版 src/jianpin.py 生成 C 版简拼索引查询用的二进制文件
（CJJPIN01 格式，详细设计见 c/src/jianpin.h 顶部说明）。

跟 gen_dict_blob.py 是同一个模式：不在 C 里重新实现简拼字母/索引
构建逻辑，真相来源仍然是 Python 端 JianpinIndex 类已经验证过的构建
结果（jianpin_letter 恒等于 syllable[0]、只收多音节词——见 jianpin.py
模块文档字符串），这里只是把构建结果导出成一份紧凑的运行时数据文件。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_jianpin_blob.py [输出目录，默认 c/build]
"""
from __future__ import annotations

import hashlib
import itertools
import json
import struct
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from src.dictionary import Dictionary, DEFAULT_DICT_PATHS  # noqa: E402
from src.jianpin import JianpinIndex  # noqa: E402

MAGIC = b"CJJPIN01"
VERSION = 1

# 跟 c/src/jianpin.h 里的 CjJianpinHeader/CjJianpinKeyEntry/CjJianpinCandEntry
# 逐字段对应——格式字符串跟 gen_dict_blob.py 完全一样（同一套布局设计），
# 只是 magic 不同、数据源换成 JianpinIndex。
HEADER_FMT = "<8sIIIIQQQQQQQ"
KEY_ENTRY_FMT = "<IIII"
CAND_ENTRY_FMT = "<IIHH"

assert struct.calcsize(HEADER_FMT) == 80
assert struct.calcsize(KEY_ENTRY_FMT) == 16
assert struct.calcsize(CAND_ENTRY_FMT) == 12

MAX_U32 = 2**32 - 1


def build_blob(idx: JianpinIndex, source_files: list[str] | None = None) -> tuple[bytes, dict]:
    # idx._entries 已经按 key 排序（构造时做的，见 jianpin.py），同一个
    # key 内部已经按权重降序排好——直接用 itertools.groupby 分组，不需要
    # 重新排序（生成工具直接用内部结构，跟 gen_dict_blob.py 用
    # d._table.items() 是同一个"允许生成脚本碰内部字段"的惯例）。
    groups = itertools.groupby(idx._entries, key=lambda e: e.key)  # noqa: SLF001

    key_strs: list[bytes] = []
    cand_lists: list[list] = []
    for key, group in groups:
        key_strs.append(key.encode("ascii"))
        cand_lists.append([e.candidate for e in group])

    key_count = len(key_strs)
    cand_count = sum(len(c) for c in cand_lists)

    header_size = struct.calcsize(HEADER_FMT)
    key_table_offset = header_size
    key_table_bytes = key_count * struct.calcsize(KEY_ENTRY_FMT)
    cand_table_offset = key_table_offset + key_table_bytes
    cand_table_bytes = cand_count * struct.calcsize(CAND_ENTRY_FMT)
    key_strings_offset = cand_table_offset + cand_table_bytes
    key_strings_len = sum(len(s) for s in key_strs)
    cand_words_offset = key_strings_offset + key_strings_len
    cand_words_bytes_list = [c.word.encode("utf-8") for cands in cand_lists for c in cands]
    cand_words_len = sum(len(w) for w in cand_words_bytes_list)
    file_size = cand_words_offset + cand_words_len

    if file_size > MAX_U32:
        raise SystemExit(
            f"file_size={file_size} 超过 uint32_t 偏移量能表示的范围，"
            "需要先把 jianpin.h/gen_jianpin_blob.py 的偏移字段改回 uint64_t"
        )

    key_entries = bytearray()
    cand_entries = bytearray()
    key_strings_blob = bytearray()
    cand_words_blob = bytearray()

    key_str_cursor = key_strings_offset
    cand_word_cursor = cand_words_offset
    cand_index_cursor = 0
    max_cands_per_key = 0

    for key_str, cands in zip(key_strs, cand_lists):
        this_key_str_offset = key_str_cursor
        key_strings_blob += key_str
        key_str_cursor += len(key_str)

        this_cand_start = cand_index_cursor
        for c in cands:
            word_bytes = c.word.encode("utf-8")
            if len(word_bytes) > 0xFFFF:
                raise SystemExit(f"候选词 {c.word!r} 编码后 {len(word_bytes)} 字节，超过 uint16_t word_len 上限")
            weight = int(round(c.weight))
            if weight > MAX_U32:
                raise SystemExit(f"候选词 {c.word!r} 权重 {weight} 超过 uint32_t 范围")
            cand_entries += struct.pack(
                CAND_ENTRY_FMT, cand_word_cursor, weight, len(word_bytes), 0
            )
            cand_words_blob += word_bytes
            cand_word_cursor += len(word_bytes)
            cand_index_cursor += 1

        key_entries += struct.pack(
            KEY_ENTRY_FMT, this_key_str_offset, len(key_str), this_cand_start, len(cands)
        )
        max_cands_per_key = max(max_cands_per_key, len(cands))

    header = struct.pack(
        HEADER_FMT,
        MAGIC,
        VERSION,
        key_count,
        cand_count,
        0,
        key_table_offset,
        key_strings_offset,
        key_strings_len,
        cand_table_offset,
        cand_words_offset,
        cand_words_len,
        file_size,
    )

    blob = bytes(header) + bytes(key_entries) + bytes(cand_entries) + bytes(key_strings_blob) + bytes(cand_words_blob)
    assert len(blob) == file_size, f"计算的 file_size={file_size} 跟实际拼出来的长度 {len(blob)} 对不上"

    manifest = {
        "key_count": key_count,
        "cand_count": cand_count,
        "max_cands_per_key": max_cands_per_key,
        "file_size_bytes": file_size,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "source_files": source_files if source_files is not None else [str(p) for p in DEFAULT_DICT_PATHS],
        "blob_sha256": hashlib.sha256(blob).hexdigest(),
    }
    return blob, manifest


def main():
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "build"
    out_dir.mkdir(parents=True, exist_ok=True)

    d = Dictionary(normalize_simplified=True)  # 简拼也剔除繁体字形（跟 dict.bin 一致，8105 白名单守卫）
    idx = JianpinIndex(d)
    blob, manifest = build_blob(idx)

    jianpin_bin = out_dir / "dict_jianpin.bin"
    jianpin_bin.write_bytes(blob)
    manifest_path = out_dir / "dict_jianpin_manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"写入 {jianpin_bin}（{len(blob):,} 字节 = {len(blob) / 1024 / 1024:.1f} MiB）")
    print(f"写入 {manifest_path}")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
