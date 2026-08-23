"""读 .rm v6 → 笔划列表（点/笔色/笔型）。只读，绝不写设备文件。

用 rmscene 做原型解析。注意：当前固件写的 .rm 比 rmscene 版本新，read_blocks 会打印
"newer format" 警告并可能跳过尾部数据——block 结构可读，但**笔划点数据完整度需真机样本
与 device-rs 的 remarkable_lines(byte-exact) 交叉核对**。生产环境实际解析用 Rust 那份。
"""
from __future__ import annotations

import logging
import sys
from dataclasses import dataclass

from rmscene import read_blocks

# rmscene 对新固件 .rm 会刷 "newer format"/"Unrecognised text format code" 警告，
# block 结构仍可读、笔划不受影响 → 降到 ERROR 静默噪声。
logging.getLogger("rmscene").setLevel(logging.ERROR)

Pt = tuple[float, float]


def _enum_name(v) -> str:
    """PenColor/Pen 归一为名字。注意 IntEnum 在 Py3.11+ 的 str() 返回数字，故用 .name。"""
    if v is None:
        return ""
    return getattr(v, "name", None) or str(v).replace("PenColor.", "").replace("Pen.", "")


@dataclass
class Stroke:
    points: list[Pt]
    color: str     # PenColor 名，如 RED / MAGENTA / BLACK
    tool: str      # Pen 名，如 FINELINER_1 / MARKER_2


def read_strokes(rm_path: str) -> list[Stroke]:
    strokes: list[Stroke] = []
    try:
        with open(rm_path, "rb") as f:
            for b in read_blocks(f):
                if type(b).__name__ != "SceneLineItemBlock":
                    continue
                val = getattr(getattr(b, "item", None), "value", None)
                if val is None:                       # 已删除/空的笔划项
                    continue
                pts = [(float(p.x), float(p.y)) for p in getattr(val, "points", [])]
                if len(pts) < 2:
                    continue
                strokes.append(Stroke(
                    points=pts,
                    color=_enum_name(getattr(val, "color", None)),
                    tool=_enum_name(getattr(val, "tool", None)),
                ))
    except Exception as e:                            # 容忍新格式/损坏页，跳过不中断
        print(f"[warn] {rm_path}: {e}", file=sys.stderr)
    return strokes
