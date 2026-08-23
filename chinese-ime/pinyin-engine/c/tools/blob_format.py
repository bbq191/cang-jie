"""共享的 CJDICT01 二进制打包逻辑，被 gen_dict_blob.py（简体，默认数据源）
和 gen_dict_zh_tw_blob.py（繁体，05 节双 blob 设计修订的产物）两个生成
脚本共用——两份数据格式完全相同（同一个 magic "CJDICT01"，C 端
dictionary.c 不用改一行代码就能打开任何一份），差异只在候选表的内容，
不该维护两份几乎一样的打包代码，改一边忘了改另一边的风险不值得冒。

不导出给 gen_jianpin_blob.py 用——简拼索引是不同的 magic/语义（
"CJJPIN01"，见 jianpin.h 顶部关于"为什么不复用 dict.bin 候选池"的
说明），布局字段虽然形状一样，但保持独立、不共享代码，避免两个概念
不同的东西被这层共享逻辑绑在一起以后改一个牵动另一个。
"""
from __future__ import annotations

import hashlib
import struct
import time

MAGIC = b"CJDICT01"
VERSION = 1

HEADER_FMT = "<8sIIIIQQQQQQQ"   # magic, version, key_count, cand_count, reserved0, 7*Q
KEY_ENTRY_FMT = "<IIII"         # key_str_offset, key_str_len, cand_start_index, cand_count
CAND_ENTRY_FMT = "<IIHH"        # word_offset, weight, word_len, reserved

assert struct.calcsize(HEADER_FMT) == 80
assert struct.calcsize(KEY_ENTRY_FMT) == 16
assert struct.calcsize(CAND_ENTRY_FMT) == 12

MAX_U32 = 2**32 - 1


def build_dict_blob(table, source_files: list[str]) -> tuple[bytes, dict]:
    """table: dict[tuple[str, ...], list[Candidate]]（每个候选列表不要求
    已经按权重排好序——这里会重新排一次，跟 gen_dict_blob.py 原来的行为
    一致，简体/繁体两条生成路径都不需要自己操心排序）。"""
    items = sorted(table.items(), key=lambda kv: " ".join(kv[0]))

    key_strs: list[bytes] = []
    cand_lists: list[list] = []
    for syllables, cands in items:
        key_strs.append(" ".join(syllables).encode("ascii"))
        cand_lists.append(sorted(cands, key=lambda c: c.weight, reverse=True))

    key_count = len(items)
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
            "需要先把 dictionary.h/blob_format.py 的偏移字段改回 uint64_t"
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
        "source_files": source_files,
        "blob_sha256": hashlib.sha256(blob).hexdigest(),
    }
    return blob, manifest
