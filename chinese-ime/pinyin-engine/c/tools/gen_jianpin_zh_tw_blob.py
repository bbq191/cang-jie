#!/usr/bin/env python3
"""生成繁体（台湾正体）版简拼索引二进制 dict_jianpin.zh_tw.bin——补齐
真机接入简拼时暴露的一个真实缺口：dict_jianpin.bin 只从（简体）
Dictionary 反推，简拼补充候选在繁体模式下会混入简体字，用户实测
反馈"切换繁体后依然输出简体"。

跟 gen_dict_zh_tw_blob.py 是同一个思路（生成阶段整体转换，运行时
零额外开销）：先用 build_traditional_table() 把默认（简体）候选表
转换成繁体+合并同形词权重，再喂给 JianpinIndex 反推索引——
JianpinIndex.__init__ 只依赖传入对象有 .items() 方法，不要求真的是
Dictionary 实例（鸭子类型），build_traditional_table() 返回的
dict[tuple, list[Candidate]] 天然满足这一点，不需要改 jianpin.py
一行代码。

用法：
    cd pinyin-engine
    .venv/bin/python3 c/tools/gen_jianpin_zh_tw_blob.py [输出目录，默认 c/build]
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from src.dictionary import Dictionary, DEFAULT_DICT_PATHS  # noqa: E402
from src.traditional import build_traditional_table  # noqa: E402
from src.jianpin import JianpinIndex  # noqa: E402
from gen_jianpin_blob import build_blob  # noqa: E402


def main():
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "build"
    out_dir.mkdir(parents=True, exist_ok=True)

    d = Dictionary()
    traditional_table = build_traditional_table(dict(d.items()), region="tw")
    idx = JianpinIndex(traditional_table)  # 鸭子类型：传入 dict 也有 .items()，不需要真的是 Dictionary
    blob, manifest = build_blob(
        idx,
        [str(p) for p in DEFAULT_DICT_PATHS] + ["OpenCC s2twp（离线转换，见 traditional.py）"],
    )

    jianpin_bin = out_dir / "dict_jianpin.zh_tw.bin"
    jianpin_bin.write_bytes(blob)
    manifest_path = out_dir / "dict_jianpin_zh_tw_blob_manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"写入 {jianpin_bin}（{len(blob):,} 字节 = {len(blob) / 1024 / 1024:.1f} MiB）")
    print(f"写入 {manifest_path}")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
