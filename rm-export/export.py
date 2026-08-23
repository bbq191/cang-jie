#!/usr/bin/env python3
"""闭环1 批量导出：遍历 xochitl 存储镜像，每本有智能高亮的文档 → 一个 Markdown。

用法:
    python3 export.py --src <镜像目录> --out <输出目录>

只读设备镜像，绝不写回。核心反解 `page_highlights` + `merge_adjacent` 被
`weread-client/highlights/reverse.py` 懒导入复用；生产设备端等价实现是
`weread-client/device-rs/src/rmread.rs`（remarkable_lines，byte-exact，两者对齐）。

注意：当前固件写的 .rm 比 rmscene 版本新，read_blocks 会刷 "newer format" 警告并可能
跳过尾部数据——block 结构可读，笔划/高亮不受影响。生产解析以 Rust 那份为准。
"""
from __future__ import annotations

import argparse
import glob
import json
import logging
import os
import sys

from rmscene import read_blocks

# rmscene 对新固件 .rm 会刷 "newer format"/"Unrecognised text format code" 警告 → 静默。
logging.getLogger("rmscene").setLevel(logging.ERROR)


def _enum_name(v) -> str:
    """PenColor 归一为名字。IntEnum 在 Py3.11+ 的 str() 返回数字，故优先 .name。"""
    if v is None:
        return ""
    return getattr(v, "name", None) or str(v).replace("PenColor.", "")


def page_highlights(rm_path: str) -> list[dict]:
    """反解一页 .rm 的所有 GlyphRange 智能高亮（非空文本），按 start 升序。

    对齐 `rmread.rs::page_highlights`：遍历 blocks，取 SceneGlyphItem 的 item.value
    （GlyphRange），保留非空 text，输出 {text, start, color}。start 是渲染页内字符 offset。
    """
    out: list[dict] = []
    try:
        with open(rm_path, "rb") as f:
            for b in read_blocks(f):
                # rmscene 高亮块类名：SceneGlyphItemBlock（对应 Rust 的 SceneGlyphItem）。
                if type(b).__name__ != "SceneGlyphItemBlock":
                    continue
                g = getattr(getattr(b, "item", None), "value", None)
                if g is None:  # 已删除/空的高亮项
                    continue
                text = getattr(g, "text", "") or ""
                if not text.strip():
                    continue
                out.append({
                    "text": text,
                    "start": getattr(g, "start", None),
                    "color": _enum_name(getattr(g, "color", None)),
                })
    except Exception as e:  # 容忍新格式/损坏页，跳过不中断
        print(f"[warn] {rm_path}: {e}", file=sys.stderr)
    out.sort(key=lambda h: (h["start"] is None, h["start"] or 0))
    return out


def merge_adjacent(hls: list[dict]) -> list[dict]:
    """智能高亮把一句切成整行段（offset 连续）→ 拼回整句。

    对齐 `rmread.rs::merge_adjacent`：相邻两段 start - 上段 end ≤ 2 则合并 text、
    顺延 end（长度按字符数计）。
    """
    out: list[dict] = []
    ends: list[int | None] = []
    for h in hls:
        hlen = len(h["text"])
        s = h["start"]
        prev_end = ends[-1] if ends else None
        if s is not None and prev_end is not None and s - prev_end <= 2:
            out[-1]["text"] += h["text"]
            ends[-1] = s + hlen
        else:
            out.append(dict(h))
            ends.append(s + hlen if s is not None else None)
    return out


def read_page(rm_path: str) -> list[dict]:
    """读一页 .rm → 合并后的高亮。"""
    return merge_adjacent(page_highlights(rm_path))


def _doc_pages(uuid_dir: str) -> list[str]:
    """一个文档目录下所有页 .rm，按文件名排序（渲染序近似）。"""
    return sorted(glob.glob(os.path.join(uuid_dir, "*.rm")))


def _visible_name(src: str, uuid: str) -> str:
    meta = os.path.join(src, uuid + ".metadata")
    try:
        with open(meta, encoding="utf-8") as f:
            return json.load(f).get("visibleName", uuid)
    except Exception:
        return uuid


def export_document(src: str, uuid: str) -> list[dict]:
    """一个文档的全部页高亮（跨页汇总）。"""
    uuid_dir = os.path.join(src, uuid)
    highlights: list[dict] = []
    for rm in _doc_pages(uuid_dir):
        highlights.extend(read_page(rm))
    return highlights


def export_all(src: str, out: str) -> int:
    """遍历镜像：每本有智能高亮的文档 → out/<visibleName>.md。返回导出本数。"""
    os.makedirs(out, exist_ok=True)
    n = 0
    for meta in glob.glob(os.path.join(src, "*.metadata")):
        uuid = os.path.basename(meta)[: -len(".metadata")]
        if not os.path.isdir(os.path.join(src, uuid)):
            continue
        hls = export_document(src, uuid)
        if not hls:
            continue
        name = _visible_name(src, uuid)
        safe = name.replace("/", "_")
        with open(os.path.join(out, safe + ".md"), "w", encoding="utf-8") as f:
            f.write(f"# {name}\n\n")
            for h in hls:
                f.write(f"- {h['text']}\n")
        n += 1
        print(f"[ok] {name}: {len(hls)} 条高亮")
    return n


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="遍历 xochitl 镜像，导出每本书的智能高亮为 Markdown")
    ap.add_argument("--src", required=True, help="xochitl 存储镜像目录")
    ap.add_argument("--out", required=True, help="Markdown 输出目录")
    args = ap.parse_args(argv)
    n = export_all(args.src, args.out)
    print(f"共导出 {n} 本")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
