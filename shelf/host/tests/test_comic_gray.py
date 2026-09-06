"""`shelf push --eink-gray`：CBZ 逐页 16 灰（灰页 4-bit PNG ≤16 色 / 彩页保色 JPEG / 超尺寸缩进屏盒 / 页序自然排序）；push 路由。"""
from __future__ import annotations

import io
import sys
import zipfile
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shelf_cli import calibre_bridge as cb  # noqa: E402
from shelf_cli.commands import push  # noqa: E402
from test_cli import FakeGateway, gateway, run  # noqa: E402,F401

Image = pytest.importorskip("PIL.Image")
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "calibre"))
import comic_gray as cg  # noqa: E402


def _png(img) -> bytes:
    b = io.BytesIO()
    img.save(b, "PNG")
    return b.getvalue()


def _jpg(img) -> bytes:
    b = io.BytesIO()
    img.convert("RGB").save(b, "JPEG", quality=90)
    return b.getvalue()


def _cbz(path: Path):
    line = Image.new("L", (600, 900), 255)
    for y in range(0, 900, 7):
        for x in range(600):
            line.putpixel((x, y), 0)
    grad = Image.linear_gradient("L").resize((600, 900))  # 256 灰渐变 → 16 灰抖动
    color = Image.new("RGB", (600, 900), (200, 30, 30))
    big = Image.new("L", (3000, 4000), 128)
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("page_10.png", _png(line))
        z.writestr("page_2.jpg", _jpg(grad))
        z.writestr("page_1.png", _png(color))
        z.writestr("page_3.png", _png(big))
        z.writestr("readme.txt", "x")


def test_convert_gray_color_resize_and_order(tmp_path):
    src = tmp_path / "in.cbz"
    _cbz(src)
    out = tmp_path / "out.cbz"
    st = cg.convert(src, out)
    assert st["pages"] == 4 and st["gray"] == 3 and st["color"] == 1
    with zipfile.ZipFile(out) as z:
        names = z.namelist()
        assert names == ["page_1.jpg", "page_2.png", "page_3.png", "page_10.png"], "自然序 + 按类改扩展名"
        color = Image.open(io.BytesIO(z.read("page_1.jpg")))
        assert color.format == "JPEG" and color.mode == "RGB"
        g = Image.open(io.BytesIO(z.read("page_2.png")))
        assert g.format == "PNG" and g.mode == "P" and len(g.getcolors()) <= 16, "16 灰调色板"
        grays = {g.getpalette()[i * 3] for i in range(16)}
        assert {0, 255} <= grays and len(grays) == 16, "等距 16 级"
        big = Image.open(io.BytesIO(z.read("page_3.png")))
        assert max(big.size) <= 1696 and min(big.size) <= 954 and big.size == (954, 1272)
    assert st["bytes_in"] > 0 and st["bytes_out"] > 0


def test_mean_chroma_mirrors_device_rule():
    assert cg.mean_chroma(Image.new("L", (10, 10), 100)) == 0.0
    assert cg.mean_chroma(Image.new("RGB", (100, 100), (120, 120, 120))) == 0.0
    assert cg.mean_chroma(Image.new("RGB", (100, 100), (200, 30, 30))) > cg.COLOR_KEEP_CHROMA
    assert cg.mean_chroma(Image.new("RGB", (100, 100), (128, 130, 126))) < cg.COLOR_KEEP_CHROMA, "偏色扫描当灰"
    assert cg.COLOR_KEEP_CHROMA == 0.06, "与 bookconv imgopt.rs 同值"


def test_push_cbz_with_eink_gray_routes_comic_and_calls_gray(gateway, tmp_path, capsys, monkeypatch):
    src = tmp_path / "manga.cbz"
    src.write_bytes(b"PK\x03\x04")
    gray = tmp_path / "manga.gray.cbz"
    gray.write_bytes(b"PK\x03\x04g")
    calls = []
    monkeypatch.setattr(cb, "has_calibre", lambda: False)
    monkeypatch.setattr(cb, "comic_gray", lambda s, o: (calls.append((s.name, o.name)) or (gray, {"pages": 3, "gray": 2, "color": 1, "bytes_in": 300, "bytes_out": 450})))
    FakeGateway.received.clear()
    rc, out = run(["push", "--eink-gray", str(src)], gateway, capsys)
    assert rc == 0 and calls == [("manga.cbz", "manga.gray.cbz")] and "16 灰 2 页" in out and "保色 1 页" in out
    assert FakeGateway.received[-1][0] == "/api/books/staging"
    calls.clear()
    rc, out = run(["push", str(src)], gateway, capsys)
    assert rc == 0 and calls == [] and "原样→" in out, "不加开关 CBZ 原样"
