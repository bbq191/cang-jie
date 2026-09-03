"""font / wallpaper 子命令对假网关。"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from test_cli import FakeGateway, gateway, run  # noqa: E402,F401


def _patch_gateway():
    orig_get, orig_post = FakeGateway.do_GET, FakeGateway.do_POST

    def do_GET(self):
        if self.path == "/api/fonts":
            return self._json(200, {"items": [{"name": "a.ttf", "bytes": 10, "extra": {"family": "A Fam", "source": "user"}}]})
        if self.path == "/api/wallpapers":
            return self._json(200, {"mode": "sequential", "current": "x.png", "items": [{"name": "x.png", "bytes": 2048, "extra": {"current": True}}]})
        return orig_get(self)

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n)
        FakeGateway.received.append((self.path, self.headers.get("Content-Type", ""), body))
        if self.path.startswith("/api/wallpapers"):
            return self._json(200, {"ok": True, "items": [{"name": "b.png", "ok": True, "message": "已安装"}], "activated": "b.png" if "activate=1" in self.path else None})
        return self._json(200, {"ok": True, "items": [{"name": "f.ttf", "ok": True, "message": "已安装", "item": {"name": "f.ttf", "bytes": 3, "extra": {"family": "F", "koreader": "mirrored"}}}]})

    def do_PUT(self):
        n = int(self.headers.get("Content-Length", 0))
        FakeGateway.received.append((self.path, "PUT", self.rfile.read(n)))
        return self._json(200, {"ok": True, "mounted": 4})

    def do_DELETE(self):
        FakeGateway.received.append((self.path, "DELETE", b""))
        return self._json(200, {"ok": True})

    FakeGateway.do_GET, FakeGateway.do_POST, FakeGateway.do_PUT, FakeGateway.do_DELETE = do_GET, do_POST, do_PUT, do_DELETE


_patch_gateway()


def test_font_add_ls_rm(gateway, tmp_path, capsys):
    f = tmp_path / "f.ttf"
    f.write_bytes(b"\x00\x01\x00\x00")
    FakeGateway.received.clear()
    rc, out = run(["font", "add", str(f)], gateway, capsys)
    assert rc == 0 and "家族=F" in out and FakeGateway.received[-1][0] == "/api/fonts"
    rc, out = run(["font", "ls"], gateway, capsys)
    assert rc == 0 and "A Fam" in out
    rc, out = run(["font", "rm", "a.ttf"], gateway, capsys)
    assert rc == 0 and FakeGateway.received[-1] == ("/api/fonts/a.ttf", "DELETE", b"")


def test_wallpaper_flow(gateway, tmp_path, capsys):
    img = tmp_path / "pic.jpg"
    img.write_bytes(b"\xff\xd8\xff")
    FakeGateway.received.clear()
    rc, out = run(["wallpaper", "add", "--activate", str(img)], gateway, capsys)
    assert rc == 0 and "已激活 b.png" in out and FakeGateway.received[-1][0] == "/api/wallpapers?activate=1"
    rc, out = run(["wallpaper", "ls"], gateway, capsys)
    assert "* x.png" in out
    rc, out = run(["wallpaper", "set", "x.png"], gateway, capsys)
    assert rc == 0 and json.loads(FakeGateway.received[-1][2]) == {"name": "x.png"}
    rc, out = run(["wallpaper", "mode", "random"], gateway, capsys)
    assert rc == 0 and FakeGateway.received[-1][0] == "/api/wallpapers/mode"
    rc, out = run(["wallpaper", "rm", "y.png"], gateway, capsys)
    assert rc == 0 and FakeGateway.received[-1][0] == "/api/wallpapers/y.png"
