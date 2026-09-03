"""设备访问抽象（Strategy）：HTTP 走网关；测试注入 FakeTransport。纯 urllib，自写 multipart。"""
from __future__ import annotations

import json
import mimetypes
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path


class TransportError(RuntimeError):
    pass


class HttpTransport:
    def __init__(self, base_url: str, timeout: float = 900.0):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    def _do(self, method: str, path: str, query: dict | None = None, data: bytes | None = None, content_type: str | None = None) -> dict:
        url = self.base_url + path
        if query:
            url += "?" + urllib.parse.urlencode({k: v for k, v in query.items() if v is not None})
        req = urllib.request.Request(url, data=data, method=method)
        if content_type:
            req.add_header("Content-Type", content_type)
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as r:
                body = r.read()
        except urllib.error.HTTPError as e:
            body = e.read()
            try:
                j = json.loads(body)
            except ValueError:
                j = {"ok": False, "message": body.decode("utf-8", "replace")}
            raise TransportError(f"HTTP {e.code}: {j.get('message', j)}") from None
        except urllib.error.URLError as e:
            raise TransportError(f"连不上 {self.base_url}: {e.reason}") from None
        try:
            return json.loads(body)
        except ValueError:
            return {"raw": body.decode("utf-8", "replace")}

    def get(self, path: str, query: dict | None = None) -> dict:
        return self._do("GET", path, query)

    def delete(self, path: str) -> dict:
        return self._do("DELETE", path)

    def post_json(self, path: str, obj: dict) -> dict:
        return self._do("POST", path, data=json.dumps(obj).encode(), content_type="application/json")

    def put_json(self, path: str, obj: dict) -> dict:
        return self._do("PUT", path, data=json.dumps(obj).encode(), content_type="application/json")

    def post_files(self, path: str, files: list[Path], query: dict | None = None, field: str = "file") -> dict:
        boundary = "----shelfcli" + uuid.uuid4().hex
        body = bytearray()
        for f in files:
            ctype = mimetypes.guess_type(f.name)[0] or "application/octet-stream"
            fname = f.name.replace('"', "%22")
            body += (f"--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{fname}\"; "
                     f"filename*=UTF-8''{urllib.parse.quote(f.name)}\r\nContent-Type: {ctype}\r\n\r\n").encode()
            body += f.read_bytes()
            body += b"\r\n"
        body += f"--{boundary}--\r\n".encode()
        return self._do("POST", path, query, bytes(body), f"multipart/form-data; boundary={boundary}")
