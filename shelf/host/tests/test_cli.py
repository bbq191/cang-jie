"""CLI 测试：起本地 http.server 假网关（FakeGateway），打真 HttpTransport；纯 stdlib。"""
from __future__ import annotations

import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shelf_cli import __main__ as cli  # noqa: E402
from shelf_cli import config as cfgmod  # noqa: E402
from shelf_cli import paths as pathsmod  # noqa: E402
from shelf_cli import transport as tr  # noqa: E402
from shelf_cli.commands import doctor  # noqa: E402


class FakeGateway(BaseHTTPRequestHandler):
    services = [
        {"name": "shelf-gateway", "port": 8778, "label": "书架", "version": "0.1.0", "pid": 1},
        {"name": "font-serve", "port": 8792, "label": "字体", "version": "0.1.0", "pid": 2, "ui": {"title": "字体", "order": 30}},
    ]
    received: list = []

    def _json(self, code, obj):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        if self.path == "/api/services":
            return self._json(200, {"services": self.services})
        if self.path == "/api/fonts/health":
            return self._json(200, {"ok": True, "service": "font-serve", "version": "0.1.0"})
        return self._json(404, {"ok": False, "message": "not found"})

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n)
        FakeGateway.received.append((self.path, self.headers.get("Content-Type", ""), body))
        return self._json(200, {"ok": True, "items": [{"name": "x", "ok": True}]})

    def log_message(self, *a):  # 静音
        pass


def serve(handler):
    """起一个假网关（独立线程），返回 (base_url, server)。各测试文件用自己的 Handler 子类，互不污染。"""
    srv = HTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return f"http://127.0.0.1:{srv.server_port}", srv


@pytest.fixture(scope="module")
def gateway():
    url, srv = serve(FakeGateway)
    yield url
    srv.shutdown()


def run(argv, base_url, capsys):
    rc = cli.main(argv, transport_factory=lambda c: tr.HttpTransport(base_url))
    return rc, capsys.readouterr().out


def test_services_lists_registry(gateway, capsys):
    rc, out = run(["services"], gateway, capsys)
    assert rc == 0
    assert "font-serve" in out and "tab=字体" in out


def test_status_probes_each_service(gateway, capsys):
    rc, out = run(["status"], gateway, capsys)
    assert rc == 0
    assert "font-serve" in out and "ok" in out


def test_unreachable_gateway_is_reported(capsys):
    rc, out = run(["services"], "http://127.0.0.1:9", capsys)
    assert rc == 2


def test_multipart_upload_encoding(gateway, tmp_path):
    f = tmp_path / "中 文.epub"
    f.write_bytes(b"PK\x03\x04data")
    t = tr.HttpTransport(gateway)
    FakeGateway.received.clear()
    j = t.post_files("/api/books", [f], {"target": "native"})
    assert j["ok"]
    path, ctype, body = FakeGateway.received[0]
    assert path == "/api/books?target=native"
    assert ctype.startswith("multipart/form-data; boundary=")
    assert b"filename*=UTF-8''%E4%B8%AD%20%E6%96%87.epub" in body
    assert b"PK\x03\x04data\r\n--" in body


def test_config_defaults_and_overrides(tmp_path):
    p = pathsmod.Paths({"HOME": str(tmp_path), "XDG_CONFIG_HOME": str(tmp_path / "cfg")})
    assert p.config_file == tmp_path / "cfg" / "shelf" / "config.toml"
    p.config.mkdir(parents=True)
    p.config_file.write_text('host = "192.168.1.5"\nquality = "device"\nbogus = 1\n')
    c = cfgmod.load(p, {"port": 9999, "host": None})
    assert (c.host, c.port, c.quality, c.default_target) == ("192.168.1.5", 9999, "device", "native")
    assert c.base_url == "http://192.168.1.5:9999"


def test_xdg_relative_paths_are_ignored(tmp_path):
    p = pathsmod.Paths({"HOME": str(tmp_path), "XDG_DATA_HOME": "rel/x"})
    assert p.data == tmp_path / ".local/share" / "shelf"


def test_doctor_detects_venv_hijack():
    assert doctor.venv_hijack({"VIRTUAL_ENV": "/x"})
    assert doctor.venv_hijack({"PATH": "/repo/.venv/bin:/usr/bin"})
    assert not doctor.venv_hijack({"PATH": "/usr/bin"})
