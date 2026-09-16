"""notes 子命令对假网关：pull 每本书先触发导出再拉 vault.json 落本机目录。"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import pytest  # noqa: E402
from test_cli import FakeGateway, run, serve  # noqa: E402


class NotesGateway(FakeGateway):

    def do_GET(self):
        if self.path == "/api/notes/books":
            return self._json(200, {"items": [
                {"uuid": "u1", "title": "人骨拼图"},
                {"uuid": "u2", "title": "空书"},
            ]})
        if self.path == "/api/notes/books/u1/vault.json":
            return self._json(200, {"title": "人骨拼图", "dir": "人骨拼图", "files": [
                {"name": "人骨拼图.md", "content": "# 索引\n"},
                {"name": "第1章 楔子.md", "content": "内容 ^e1\n"},
            ]})
        if self.path == "/api/notes/books/u2/vault.json":
            return self._json(200, {"title": "空书", "dir": "空书", "files": []})
        return super().do_GET()

    def do_POST(self):
        if self.path.startswith("/api/notes/books/") and self.path.endswith("/export"):
            n = int(self.headers.get("Content-Length", 0))
            self.rfile.read(n)
            FakeGateway.received.append((self.path, "", b""))
            return self._json(200, {"ok": True, "files": 2})
        return super().do_POST()


@pytest.fixture(scope="module")
def gateway():
    url, srv = serve(NotesGateway)
    yield url
    srv.shutdown()


def test_pull_writes_files_and_skips_empty_book(gateway, tmp_path, capsys):
    out = tmp_path / "vault"
    FakeGateway.received.clear()
    rc, o = run(["notes", "pull", "--out", str(out)], gateway, capsys)
    assert rc == 0
    assert (out / "人骨拼图" / "人骨拼图.md").read_text(encoding="utf-8") == "# 索引\n"
    assert (out / "人骨拼图" / "第1章 楔子.md").read_text(encoding="utf-8") == "内容 ^e1\n"
    assert not (out / "空书").exists(), "没有导出内容的书不该建目录"
    assert "空书: 没有已导出内容，跳过" in o
    assert "共 2 个文件" in o
    assert ("/api/notes/books/u1/export", "", b"") in FakeGateway.received
    assert ("/api/notes/books/u2/export", "", b"") in FakeGateway.received


def test_pull_overwrites_on_second_run(gateway, tmp_path, capsys):
    out = tmp_path / "vault2"
    run(["notes", "pull", "--out", str(out)], gateway, capsys)
    (out / "人骨拼图" / "第1章 楔子.md").write_text("手滑改坏的内容", encoding="utf-8")
    run(["notes", "pull", "--out", str(out)], gateway, capsys)
    assert (out / "人骨拼图" / "第1章 楔子.md").read_text(encoding="utf-8") == "内容 ^e1\n"
