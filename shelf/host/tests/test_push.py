"""push 端到端（假网关）：落母版库 / 无直投逃生 / PDF 重排 / 漫画通道 / 分卷 / calibre 桥环境清洗。"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shelf_cli import calibre_bridge as cb  # noqa: E402
from shelf_cli import comic, pdfsplit  # noqa: E402
from shelf_cli.commands import push  # noqa: E402
from test_cli import FakeGateway, gateway, run  # noqa: E402,F401


def test_clean_env_strips_venv():
    e = cb.clean_env({"VIRTUAL_ENV": "/r/.venv", "PATH": "/r/.venv/bin:/usr/bin:/bin"})
    assert "VIRTUAL_ENV" not in e
    assert e["PATH"] == "/usr/bin:/bin"


def test_split_volumes_math(tmp_path):
    assert pdfsplit.volumes_for(10 * 2**20, 60) == 1
    assert pdfsplit.volumes_for(61 * 2**20, 60) == 2
    assert pdfsplit.volumes_for(300 * 2**20, 60) == 5
    f = tmp_path / "x.pdf"
    f.write_bytes(b"%PDF" + b"\0" * 10)
    assert not pdfsplit.needs_split(f, 60)
    assert not pdfsplit.needs_split(f, 0)


def test_push_lands_in_staging(gateway, tmp_path, capsys, monkeypatch):
    (tmp_path / "b.epub").write_bytes(b"PK")
    monkeypatch.setattr(cb, "has_calibre", lambda: False)  # 无 Calibre → 原样落母版库
    FakeGateway.received.clear()
    rc, out = run(["push", str(tmp_path / "b.epub")], gateway, capsys)
    assert rc == 0 and "母版库" in out and "✓" in out
    assert FakeGateway.received[-1][0] == "/api/books/staging"


def test_push_has_no_direct_escape(gateway, tmp_path, capsys):
    # 规则与网页一致：所有书只落母版库，--direct 不存在
    (tmp_path / "b.epub").write_bytes(b"PK")
    import pytest

    with pytest.raises(SystemExit):
        run(["push", "--direct", str(tmp_path / "b.epub")], gateway, capsys)


def test_push_native_pdf_reflow_to_staging(gateway, tmp_path, capsys, monkeypatch):
    pdf = tmp_path / "paper.pdf"
    pdf.write_bytes(b"%PDF-1.4 fake")
    epub = tmp_path / "paper.epub"
    epub.write_bytes(b"PK\x03\x04reflowed")
    monkeypatch.setattr(cb, "has_calibre", lambda: True)
    monkeypatch.setattr(cb, "reflow_pdf", lambda src, work: (epub, "epub"))
    monkeypatch.setattr(cb, "wash", lambda src, work, **kw: src)  # 洗书直返（不跑 Calibre）
    monkeypatch.setattr(push, "_gate", lambda out, args: None)
    FakeGateway.received.clear()
    rc, out = run(["push", str(pdf)], gateway, capsys)
    assert rc == 0 and "结构化重排 → EPUB" in out
    assert FakeGateway.received[-1][0] == "/api/books/staging", FakeGateway.received
    assert "已入母版库。去 " in out and "传书 → 母版库" in out, "推完要给网页去向提示"


def test_push_keep_spacing_passes_env(gateway, tmp_path, capsys, monkeypatch):
    (tmp_path / "poem.epub").write_bytes(b"PK")
    seen = {}
    monkeypatch.setattr(cb, "has_calibre", lambda: True)
    monkeypatch.setattr(cb, "wash", lambda src, work, env=None: seen.__setitem__("env", env) or src)
    monkeypatch.setattr(push, "_gate", lambda out, args: None)
    FakeGateway.received.clear()
    rc, _ = run(["push", "--keep-spacing", str(tmp_path / "poem.epub")], gateway, capsys)
    assert rc == 0 and seen["env"] == {"WASH_KEEP_PARA_SPACING": "1"}
    rc, _ = run(["push", str(tmp_path / "poem.epub")], gateway, capsys)
    assert seen["env"] is None, "不带 --keep-spacing 不设环境"


def test_push_no_reflow_raw_to_staging(gateway, tmp_path, capsys, monkeypatch):
    pdf = tmp_path / "p.pdf"
    pdf.write_bytes(b"%PDF raw")
    called = {"n": 0}
    monkeypatch.setattr(cb, "has_calibre", lambda: True)
    monkeypatch.setattr(cb, "reflow_pdf", lambda *a: called.__setitem__("n", 1) or (pdf, "pdf"))
    monkeypatch.setattr(push, "_gate", lambda out, args: None)
    FakeGateway.received.clear()
    rc, out = run(["push", "--no-reflow", str(pdf)], gateway, capsys)
    assert rc == 0 and called["n"] == 0, "--no-reflow 不应调用 reflow"
    assert FakeGateway.received[-1][0] == "/api/books/staging"


def test_push_dry_run_and_missing_file(gateway, tmp_path, capsys):
    rc, out = run(["push", "-n", str(tmp_path / "nope.epub")], gateway, capsys)
    assert rc == 1 and "不是文件" in out
    (tmp_path / "a.epub").write_bytes(b"PK")
    FakeGateway.received.clear()
    rc, out = run(["push", "-n", str(tmp_path / "a.epub")], gateway, capsys)
    assert rc == 0 and "母版库" in out and not FakeGateway.received


def _palmdb(records: list[bytes]) -> bytes:
    """最小 PalmDB：78 字节头 + 记录表 + 记录体。"""
    n = len(records)
    head = bytearray(78)
    head[76:78] = n.to_bytes(2, "big")
    off = 78 + n * 8
    table = bytearray()
    for i, r in enumerate(records):
        table += off.to_bytes(4, "big") + bytes([0, 0, 0, i & 0xFF])
        off += len(r)
    return bytes(head) + bytes(table) + b"".join(records)


def test_comic_probe_palmdb_and_epub(tmp_path):
    jpg = b"\xff\xd8\xff" + b"\0" * 2000
    comic_book = tmp_path / "manga.azw3"
    comic_book.write_bytes(_palmdb([b"MOBIhdr" + b"\0" * 100] + [jpg] * 30))
    text_book = tmp_path / "novel.azw3"
    text_book.write_bytes(_palmdb([b"text record " * 200] * 40 + [jpg] * 2))
    assert comic.is_comic(comic_book) and not comic.is_comic(text_book)
    assert comic.is_comic(tmp_path / "x.cbz") and not comic.is_comic(tmp_path / "x.pdf")
    import zipfile

    def epub(path, pages, imgs_per_page, text_per_page):
        with zipfile.ZipFile(path, "w") as z:
            z.writestr("META-INF/container.xml", '<container><rootfile full-path="OEBPS/content.opf"/></container>')
            items = "".join(f'<item id="p{i}" href="p{i}.xhtml"/>' for i in range(pages))
            spine = "".join(f'<itemref idref="p{i}"/>' for i in range(pages))
            z.writestr("OEBPS/content.opf", f"<package><manifest>{items}</manifest><spine>{spine}</spine></package>")
            for i in range(pages):
                body = '<img src="i.jpg"/>' * imgs_per_page + "<p>" + "字" * text_per_page + "</p>"
                z.writestr(f"OEBPS/p{i}.xhtml", f"<html><head><style>p{{x}}</style></head><body>{body}</body></html>")
    epub(tmp_path / "c.epub", 10, 17, 30)      # Calibre 洗过的漫画：一页十几张图、几乎无字
    epub(tmp_path / "t.epub", 30, 1, 600)      # 文字书：每章一张插图、几百字
    assert comic.is_comic(tmp_path / "c.epub") and not comic.is_comic(tmp_path / "t.epub")


def test_push_comic_route_lands_cbz_and_pdf(gateway, tmp_path, capsys, monkeypatch):
    src = tmp_path / "manga.azw3"
    src.write_bytes(b"x")
    monkeypatch.setattr(cb, "has_calibre", lambda: True)
    monkeypatch.setattr(comic, "is_comic", lambda p: True)
    seen = {}
    monkeypatch.setattr(cb, "comic2cbz", lambda s, o: (o.write_bytes(b"PK"), o)[1])
    monkeypatch.setattr(cb, "cbz2pdf", lambda s, o, mono=False: (seen.__setitem__("mono", mono), o.write_bytes(b"%PDF"), o)[2])
    FakeGateway.received.clear()
    rc, out = run(["push", "--mono", str(src)], gateway, capsys)
    assert rc == 0 and "漫画 CBZ+PDF→母版库" in out and seen["mono"] is True
    names = [r[0] for r in FakeGateway.received]
    assert names.count("/api/books/staging") == 2, "CBZ 与 PDF 各一次入库"
    # --no-comic 强制走洗书路；--no-optimize 原样
    monkeypatch.setattr(cb, "wash", lambda s, w, env=None: s)
    monkeypatch.setattr(push, "_gate", lambda o, a: None)
    rc, out = run(["push", "--no-comic", str(src)], gateway, capsys)
    assert rc == 0 and "洗书→母版库" in out
    rc, out = run(["push", "--no-optimize", str(src)], gateway, capsys)
    assert rc == 0 and "原样→母版库" in out
    # CBZ 不需要 Calibre 也走漫画路
    monkeypatch.setattr(cb, "has_calibre", lambda: False)
    cbz = tmp_path / "v1.cbz"
    cbz.write_bytes(b"PK")
    rc, out = run(["push", str(cbz)], gateway, capsys)
    assert rc == 0 and "漫画 CBZ+PDF→母版库" in out
