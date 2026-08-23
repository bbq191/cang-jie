#!/usr/bin/env python3
"""生成繁体（台湾正体）版词典二进制 dict.zh_tw.bin——白皮书 05 节"设计
修订：正式设备端方案改成候选生成阶段整体转换"的产物。

跟 gen_dict_blob.py 是同一个 CJDICT01 格式（共用 blob_format.py 的打包
逻辑，C 端 dictionary.c 不需要改一行代码就能打开这份文件），差异只在
候选表内容：先用 src/traditional.py 的 build_traditional_table() 把
默认（简体）Dictionary 的候选表整体转换成繁体+合并同形词权重，再打包。

不在设备上跑 OpenCC——转换在这里（开发机、生成阶段）一次性做完，
运行时 C 引擎只是 mmap 另一个文件，见 05 节"为什么这样能做到功能与
简体一致"的说明。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_dict_zh_tw_blob.py [输出目录，默认 c/build]
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from src.dictionary import Dictionary, FULL_DICT_PATHS  # noqa: E402
from src.traditional import build_traditional_table  # noqa: E402
from blob_format import build_dict_blob  # noqa: E402


def main():
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "build"
    out_dir.mkdir(parents=True, exist_ok=True)

    # FULL_DICT_PATHS 含 iorest：繁体全拼跟简体一样能查到 iorest 长尾词（离线
    # OpenCC s2twp 整体转换成繁体形式）。简拼 zh_tw 仍精简、不走这里。
    d = Dictionary(FULL_DICT_PATHS)
    traditional_table = build_traditional_table(dict(d.items()), region="tw")
    blob, manifest = build_dict_blob(
        traditional_table,
        [str(p) for p in FULL_DICT_PATHS] + ["OpenCC s2twp（离线转换，见 traditional.py）"],
    )

    dict_bin = out_dir / "dict.zh_tw.bin"
    dict_bin.write_bytes(blob)
    manifest_path = out_dir / "dict_zh_tw_blob_manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"写入 {dict_bin}（{len(blob):,} 字节 = {len(blob) / 1024 / 1024:.1f} MiB）")
    print(f"写入 {manifest_path}")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
