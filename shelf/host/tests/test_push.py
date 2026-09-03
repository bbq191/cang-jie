"""push 路由决策 + 端到端（假网关）+ calibre 桥环境清洗 + 分卷判定。"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shelf_cli import calibre_bridge as cb  # noqa: E402
from shelf_cli import pdfsplit  # noqa: E402
from shelf_cli.commands import push  # noqa: E402
from test_cli import FakeGateway, gateway, run  # noqa: E402,F401


def test_route_decision_matrix():
    d = push.decide_route
    e = Path("a.epub")
    assert d("device", "native", e, True) == "device"
    assert d("host", "native", e, False) == "device"  # 没 Calibre 只能设备
    assert d("host", "native", e, True) == "host"
    assert d("auto", "native", e, True) == "host"
    assert d("auto", "native", e, False) == "device"
    assert d("auto", "native", Path("c.cbz"), True) == "device"  # CBZ 设备端转就够
    assert d("auto", "koreader", e, True) == "device"
    assert d("auto", "koreader", Path("m.azw3"), True) == "host"
    assert d("auto", "annot", Path("p.pdf"), True) == "host"


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


def test_push_device_route_hits_gateway(gateway, tmp_path, capsys):
    (tmp_path / "b.azw3").write_bytes(b"BOOKMOBI")
    (tmp_path / "k.epub").write_bytes(b"PK")
    FakeGateway.received.clear()
    rc, out = run(["push", "--quality", "device", "--target", "native", str(tmp_path / "b.azw3")], gateway, capsys)
    assert rc == 0 and "route=device" in out and "✓" in out
    assert FakeGateway.received[-1][0].startswith("/api/books?target=native&folder=&optimize=auto")
    rc, out = run(["push", "--target", "koreader", "--folder", "manga", str(tmp_path / "k.epub")], gateway, capsys)
    assert rc == 0
    assert FakeGateway.received[-1][0] == "/api/koreader/books?folder=manga"


def test_push_dry_run_and_missing_file(gateway, tmp_path, capsys):
    rc, out = run(["push", "-n", str(tmp_path / "nope.epub")], gateway, capsys)
    assert rc == 1 and "不是文件" in out
    (tmp_path / "a.epub").write_bytes(b"PK")
    FakeGateway.received.clear()
    rc, out = run(["push", "-n", "-t", "annot", str(tmp_path / "a.epub")], gateway, capsys)
    assert rc == 0 and "target=annot" in out and not FakeGateway.received
