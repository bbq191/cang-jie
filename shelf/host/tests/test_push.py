"""push 端到端（假网关）：落母版库 / --direct 直投 / PDF 重排 / 分卷 / calibre 桥环境清洗。"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shelf_cli import calibre_bridge as cb  # noqa: E402
from shelf_cli import pdfsplit  # noqa: E402
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


def test_push_direct_bypasses_staging(gateway, tmp_path, capsys, monkeypatch):
    (tmp_path / "b.epub").write_bytes(b"PK")
    monkeypatch.setattr(cb, "has_calibre", lambda: False)
    FakeGateway.received.clear()
    rc, out = run(["push", "--direct", str(tmp_path / "b.epub")], gateway, capsys)
    assert rc == 0 and "xochitl 书库" in out
    assert FakeGateway.received[-1][0].startswith("/api/books?target=native")


def test_push_native_pdf_reflow_to_staging(gateway, tmp_path, capsys, monkeypatch):
    pdf = tmp_path / "paper.pdf"
    pdf.write_bytes(b"%PDF-1.4 fake")
    epub = tmp_path / "paper.epub"
    epub.write_bytes(b"PK\x03\x04reflowed")
    monkeypatch.setattr(cb, "has_calibre", lambda: True)
    monkeypatch.setattr(cb, "reflow_pdf", lambda src, work: (epub, "epub"))
    monkeypatch.setattr(cb, "wash", lambda src, work: src)  # 洗书直返（不跑 Calibre）
    monkeypatch.setattr(push, "_gate", lambda out, args: None)
    FakeGateway.received.clear()
    rc, out = run(["push", str(pdf)], gateway, capsys)
    assert rc == 0 and "结构化重排 → EPUB" in out
    assert FakeGateway.received[-1][0] == "/api/books/staging", FakeGateway.received


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
