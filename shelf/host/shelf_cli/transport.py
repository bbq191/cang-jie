"""设备访问抽象（Strategy）：HTTP 走网关；测试注入 FakeTransport。纯 urllib，自写 multipart。"""
from __future__ import annotations

import base64
import json
import mimetypes
import ssl
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path


class TransportError(RuntimeError):
    pass


class HttpTransport:
    """HTTPS（私有 CA 自签，缺省不校验证书——局域网 + 密码保护）+ HTTP Basic（网关只看密码，用户名任意）。"""

    def __init__(self, base_url: str, timeout: float = 900.0, user: str = "shelf", password: str = "", verify_tls: bool = False):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.auth = base64.b64encode(f"{user}:{password}".encode()).decode() if password else ""
        if verify_tls:
            self.ctx = ssl.create_default_context()
        else:
            self.ctx = ssl.create_default_context()
            self.ctx.check_hostname = False
            self.ctx.verify_mode = ssl.CERT_NONE

    def _do(self, method: str, path: str, query: dict | None = None, data: bytes | None = None, content_type: str | None = None) -> dict:
        url = self.base_url + path
        if query:
            url += "?" + urllib.parse.urlencode({k: v for k, v in query.items() if v is not None})
        req = urllib.request.Request(url, data=data, method=method)
        if content_type:
            req.add_header("Content-Type", content_type)
        if self.auth:
            req.add_header("Authorization", f"Basic {self.auth}")
        try:
            with urllib.request.urlopen(req, timeout=self.timeout, context=self.ctx) as r:
                body = r.read()
        except urllib.error.HTTPError as e:
            if e.code == 401:
                raise TransportError("密码错误或未设置（config.toml 的 password / 环境变量 SHELF_PASSWORD / 交互输入；首次默认 shelf）") from None
            body = e.read()
            try:
                j = json.loads(body)
            except ValueError:
                j = {"ok": False, "message": body.decode("utf-8", "replace")}
            if e.code == 403 and "改密码" in str(j.get("message", "")):
                raise TransportError("首次登录必须先改密码：`shelf passwd`（或网页 /password）") from None
            raise TransportError(f"HTTP {e.code}: {j.get('message', j)}") from None
        except urllib.error.URLError as e:
            raise TransportError(f"连不上 {self.base_url}: {e.reason}") from None
        try:
            return json.loads(body)
        except ValueError:
            return {"raw": body.decode("utf-8", "replace")}

    def get(self, path: str, query: dict | None = None) -> dict:
        return self._do("GET", path, query)

    def get_text(self, path: str) -> str:
        r = self._do("GET", path)
        return r["raw"] if "raw" in r else json.dumps(r)

    def post_text(self, path: str, data: bytes, query: dict | None = None) -> dict:
        return self._do("POST", path, query, data, "text/plain; charset=utf-8")

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
