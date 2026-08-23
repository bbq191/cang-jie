#!/usr/bin/env python3
"""从 Python 版 src/dictionary.py 生成 C 版词典查询用的二进制文件
（cj_dict_blob_v1 格式，详细设计见 c/src/dictionary.h 顶部说明）。

不在 C 里重新解析 YAML——真相来源仍然是 Python 端 Dictionary 类已经
验证过的加载/合并/排序逻辑（雾凇拼音 base.dict.yaml + 8105.dict.yaml，
见 data/PROVENANCE.rime-ice.md），这里只是把加载结果导出成一份紧凑的
运行时数据文件，C 侧 mmap 直接用，不重新实现一遍 TSV 解析。

打包逻辑（struct.pack 布局计算那部分）搬到了 blob_format.py，跟
gen_dict_zh_tw_blob.py（05 节繁体双 blob 设计）共用，这个文件现在只
负责"从哪个 Python 数据源取表"这一层。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_dict_blob.py [输出目录，默认 c/build]

跟 Python 端数据源保持同步的方式：如果 base.dict.yaml/8105.dict.yaml
更新了（重新拉取新 commit），重新跑一次这个脚本就行，不需要手改二进制
格式或 C 代码。
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from src.dictionary import Dictionary, FULL_DICT_PATHS  # noqa: E402
from blob_format import build_dict_blob  # noqa: E402


def main():
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "build"
    out_dir.mkdir(parents=True, exist_ok=True)

    # FULL_DICT_PATHS = 8105 + base + 41448 + iorest（简拼/繁体不含 iorest，见 dictionary.py）
    d = Dictionary(FULL_DICT_PATHS, normalize_simplified=True)  # 简体 blob 剔除繁体字形（8105 白名单守卫，不改坏简体）
    blob, manifest = build_dict_blob(dict(d.items()), [str(p) for p in FULL_DICT_PATHS])

    dict_bin = out_dir / "dict.bin"
    dict_bin.write_bytes(blob)
    manifest_path = out_dir / "dict_blob_manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"写入 {dict_bin}（{len(blob):,} 字节 = {len(blob) / 1024 / 1024:.1f} MiB）")
    print(f"写入 {manifest_path}")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
