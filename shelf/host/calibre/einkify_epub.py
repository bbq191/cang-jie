"""EPUB 墨水屏图片优化（xochitl 省刷新+提速，2026-09-02）。

依据真机波形分档结论（彩色重→256灰中→≤16灰轻→1bit最轻）：把 EPUB 内图片
统一为 **16 级灰阶 + Floyd-Steinberg 抖动 + 降采样到屏幕长边**，换更轻的
墨水屏波形（图页少闪）+ 更小的图（tile 渲染快、翻页跟手）。文字页闪烁不归
内容层管（xochitl 波形+刷新策略层的事），本工具不碰文本。

用法: uv run python shelf/host/calibre/einkify_epub.py in.epub [out.epub]
      --keep-color  跳过灰阶化只降采样（彩色内容想保色时用）
依赖: Pillow（weread 依赖组现成）。
"""

from __future__ import annotations

import io
import re
import sys
import zipfile

from PIL import Image

LONG_EDGE = 1696  # Move 屏长边（与 imgopt.rs 同规则：长边 ≤1696 且短边 ≤954）
SHORT_EDGE = 954
GRAY_LEVELS = 16
IMG_EXTS = (".jpg", ".jpeg", ".png", ".gif", ".webp")
MIN_BYTES = 4096  # 小装饰图不折腾


def einkify(data: bytes, keep_color: bool) -> tuple[bytes, str] | None:
    """返回 (新字节, 新扩展名)；不值得改则返回 None。"""
    try:
        img = Image.open(io.BytesIO(data))
        img.load()
    except Exception:
        return None
    if img.width < 64 and img.height < 64:
        return None

    # 块级图 xochitl 一律缩到正文列宽（≤954，真机探针 2026-09-02）；横置时列宽上限 1696 但高
    # 受 954 限——两种朝向合起来：长边 ≤1696 且短边 ≤954，超出的像素任何朝向都显示不出。
    k = min(1.0, LONG_EDGE / max(img.size), SHORT_EDGE / min(img.size))
    if k < 1.0:
        img = img.resize((max(1, round(img.width * k)), max(1, round(img.height * k))), Image.LANCZOS)

    if keep_color:
        out = io.BytesIO()
        img.convert("RGB").save(out, "JPEG", quality=80)
        return out.getvalue(), ".jpg"

    # 灰阶 + 16 级量化（Floyd-Steinberg）。调色板量化后存 PNG（比 JPEG 更小且无块噪）。
    gray = img.convert("L")
    pal = Image.new("P", (1, 1))
    pal.putpalette(sum(([v, v, v] for v in range(0, 256, 256 // GRAY_LEVELS)), []) + [0] * (768 - GRAY_LEVELS * 3))
    quant = gray.convert("RGB").quantize(palette=pal, dither=Image.FLOYDSTEINBERG)
    out = io.BytesIO()
    quant.save(out, "PNG", optimize=True)
    return out.getvalue(), ".png"


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    keep_color = "--keep-color" in sys.argv
    if not args:
        print(__doc__)
        return 2
    src = args[0]
    dst = args[1] if len(args) > 1 else src[:-5] + "-eink.epub"

    zin = zipfile.ZipFile(src)
    renames: dict[str, str] = {}  # 旧条目名 → 新条目名（扩展名变化时）
    blobs: dict[str, bytes] = {}
    before = after = n = 0
    for item in zin.infolist():
        name = item.filename
        low = name.lower()
        if low.endswith(IMG_EXTS) and item.file_size >= MIN_BYTES:
            data = zin.read(name)
            r = einkify(data, keep_color)
            # 波形收益是主目标，体积次要：抖动 PNG 常比原 JPEG 大，容忍长到 1.5×。
            if r and len(r[0]) < len(data) * 1.5:
                newdata, ext = r
                stem = name.rsplit(".", 1)[0]
                newname = stem + ext
                renames[name] = newname
                blobs[newname] = newdata
                before += len(data)
                after += len(newdata)
                n += 1
                continue
        blobs[name] = zin.read(name)

    with zipfile.ZipFile(dst, "w") as zout:
        for name, data in blobs.items():
            if name.endswith((".xhtml", ".html", ".htm", ".opf", ".ncx", ".css")):
                text = data.decode("utf-8", "ignore")
                for old, new in renames.items():
                    if old == new:
                        continue
                    ob, nb = old.rsplit("/", 1)[-1], new.rsplit("/", 1)[-1]
                    text = text.replace(old, new).replace(ob, nb)
                if name.endswith(".opf"):
                    # 扩展名改了，manifest 的 media-type 必须跟着改——
                    # jpeg 声明配 png 实体这种不一致别去赌 xochitl 的容忍度。
                    def _fix(m: re.Match) -> str:
                        tag = m.group(0)
                        if re.search(r'href="[^"]*\.png"', tag):
                            tag = re.sub(r'media-type="image/[a-z+]+"',
                                         'media-type="image/png"', tag)
                        return tag
                    text = re.sub(r"<item\b[^>]*>", _fix, text)
                data = text.encode("utf-8")
            comp = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
            zout.writestr(name, data, comp)

    mb = 1024 * 1024
    print(f"处理 {n} 图: {before/mb:.1f}MB → {after/mb:.1f}MB；产物: {dst}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
