#!/usr/bin/env python3
"""B1 手写识别 de-risk：.rm 笔划 → 干净栅格 PNG（喂多模态 vision 的输入之一）。

只读 .rm，绝不写设备文件。复用 rm_strokes.read_strokes 反解笔划，本文件只管渲染。
注意（2026-08-29 de-risk 实测）：这版**均匀细线**渲染，识别率明显低于 xochitl 自己生成
的 `<doc>.thumbnails/<page>.png`（快写样本 57% vs 65%）——生产管线应优先收割原生缩略图，
本渲染器仅在需要可控全分辨率、或缩略图缺失时兜底。见 docs 路线图白皮书 §06 验证结果。

用法：python hw_render.py <in.rm> <out.png>
依赖：rmscene, pillow（uv run --with rmscene --with pillow python hw_render.py ...）
"""
import sys
from PIL import Image, ImageDraw
from rm_strokes import read_strokes


def render(rm_path: str, png_path: str, width: int = 1404, margin: int = 40, lw: int = 3) -> str | None:
    strokes = [s.points for s in read_strokes(rm_path) if len(s.points) >= 2]
    if not strokes:
        print("no strokes", file=sys.stderr)
        return None
    xs = [x for pts in strokes for x, _ in pts]
    ys = [y for pts in strokes for _, y in pts]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    scale = (width - 2 * margin) / ((x1 - x0) or 1)
    height = int(((y1 - y0) or 1) * scale + 2 * margin)
    img = Image.new("L", (width, height), 255)
    d = ImageDraw.Draw(img)
    for pts in strokes:
        prev = None
        for (x, y) in pts:
            p = (margin + (x - x0) * scale, margin + (y - y0) * scale)
            if prev is not None:
                d.line([prev, p], fill=0, width=lw)
            prev = p
    img.save(png_path)
    print(f"strokes={len(strokes)} -> {png_path} {width}x{height}")
    return png_path


if __name__ == "__main__":
    render(sys.argv[1], sys.argv[2])
