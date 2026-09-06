"""漫画省刷新档：CBZ → CBZ，每页按 Move 屏盒降采样，黑白/偏色页转 **16 级灰 + Floyd-Steinberg 抖动 → 4-bit PNG**，
真彩页（按饱和度阈值）保色存 JPEG。依据：墨水屏波形按内容分档（彩色重 / 256 灰中 / ≤16 灰轻 / 1-bit 最轻，真机坐实），
16 灰是画质与减闪的甜点（网点/灰阶插画不像 1-bit 那样糊成噪点）。**默认关**（`shelf push --eink-gray`）：抖动噪点
Flate 压不动，体积可能比原 JPEG 大 2–3 倍，实测数字报用户再定默认（2026-09-06）。
阈值/采样镜像设备端 bookconv `imgopt.rs`（`COLOR_KEEP_CHROMA=0.06`：RGB 通道极差均值 /255，火影正文=0、彩封≈0.5；
每 total/40000 像素取一样本）——两处同一份，改一处另一处同步。前身：已删的 `einkify_epub.py`（EPUB 内图；`git show 560a8b5^`）。
依赖 Pillow（`uv run --group calibre`）。用法: python comic_gray.py <in.cbz> <out.cbz>  → 末行 JSON {out,pages,gray,color,bytes_in,bytes_out}
"""
from __future__ import annotations

import io
import json
import re
import sys
import zipfile
from pathlib import Path

from PIL import Image

LONG_EDGE = 1696  # Move 屏：长边 ≤1696 且短边 ≤954（imgopt.rs 同规则）
SHORT_EDGE = 954
GRAY_LEVELS = 16
COLOR_KEEP_CHROMA = 0.06
IMG_EXTS = (".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp")
_NUM = re.compile(r"(\d+)")


def natural_key(name: str):
    return [int(t) if t.isdigit() else t.lower() for t in _NUM.split(name)]


def mean_chroma(img: Image.Image) -> float:
    """页面平均色度：RGB 极差均值 /255，每 total/40000 像素采一样本（与 imgopt.rs `mean_chroma` 同法）。灰度图恒 0。"""
    if img.mode in ("L", "LA", "1"):
        return 0.0
    rgb = img.convert("RGB")
    w, h = rgb.size
    total = w * h
    if total == 0:
        return 0.0
    step = max(1, total // 40_000)
    buf = rgb.tobytes()  # RGBRGB…（getdata 在 Pillow 12 已弃用）
    s = n = 0
    for i in range(0, total, step):
        r, g, b = buf[3 * i], buf[3 * i + 1], buf[3 * i + 2]
        s += max(r, g, b) - min(r, g, b)
        n += 1
    return (s / n) / 255.0 if n else 0.0


def fit_screen(img: Image.Image) -> Image.Image:
    k = min(1.0, LONG_EDGE / max(img.size), SHORT_EDGE / min(img.size))
    if k < 1.0:
        img = img.resize((max(1, round(img.width * k)), max(1, round(img.height * k))), Image.LANCZOS)
    return img


def to_gray16(img: Image.Image) -> Image.Image:
    """L → 16 级等距灰调色板 + FS 抖动（P 模式）。"""
    pal = Image.new("P", (1, 1))
    steps = [round(i * 255 / (GRAY_LEVELS - 1)) for i in range(GRAY_LEVELS)]
    pal.putpalette(sum(([v, v, v] for v in steps), []) + [0] * (768 - GRAY_LEVELS * 3))
    return img.convert("L").convert("RGB").quantize(palette=pal, dither=Image.FLOYDSTEINBERG)


def process(data: bytes) -> tuple[bytes, str, str]:
    """一页 → (字节, 扩展名, 'gray'|'color')。解不开的原样返回（kind='raw'）。"""
    try:
        img = Image.open(io.BytesIO(data))
        img.load()
    except Exception:  # noqa: BLE001
        return data, "", "raw"
    img = fit_screen(img)
    out = io.BytesIO()
    if mean_chroma(img) >= COLOR_KEEP_CHROMA:
        img.convert("RGB").save(out, "JPEG", quality=85)
        return out.getvalue(), ".jpg", "color"
    to_gray16(img).save(out, "PNG", optimize=True, bits=4)
    return out.getvalue(), ".png", "gray"


def convert(src: Path, out: Path) -> dict:
    stats = {"out": str(out), "pages": 0, "gray": 0, "color": 0, "bytes_in": 0, "bytes_out": 0}
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as zout:
        names = sorted((n for n in zin.namelist() if n.lower().endswith(IMG_EXTS) and not n.endswith("/")), key=natural_key)
        for n in names:
            raw = zin.read(n)
            data, ext, kind = process(raw)
            stem = n.rsplit(".", 1)[0]
            zout.writestr(stem + (ext or "." + n.rsplit(".", 1)[1]), data)
            stats["pages"] += 1
            stats["bytes_in"] += len(raw)
            stats["bytes_out"] += len(data)
            if kind in ("gray", "color"):
                stats[kind] += 1
    if not stats["pages"]:
        raise SystemExit("CBZ 内无图片")
    return stats


def main() -> int:
    if len(sys.argv) != 3:
        print("用法: comic_gray.py <in.cbz> <out.cbz>", file=sys.stderr)
        return 2
    print(json.dumps(convert(Path(sys.argv[1]), Path(sys.argv[2])), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
