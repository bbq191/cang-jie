"""`$XDG_CONFIG_HOME/shelf/config.toml`（tomllib，stdlib ≥3.11）。缺省值集中一处。"""
from __future__ import annotations

import tomllib
from dataclasses import dataclass

from .paths import Paths

# ssh 缺省留空 → load() 里从最终的 host 派生（`root@<host>`），避免改了 host 而 ssh 仍停在旧 IP。
DEFAULTS = {
    "host": "10.11.99.1",
    "port": 8778,
    "scheme": "https",
    "password": "",
    "verify_tls": False,
    "ssh": "",
    "split_pdf_mb": 60,
}


@dataclass
class Config:
    host: str
    port: int
    scheme: str
    password: str
    verify_tls: bool
    ssh: str
    split_pdf_mb: int

    @property
    def base_url(self) -> str:
        return f"{self.scheme}://{self.host}:{self.port}"


def load(paths: Paths, overrides: dict | None = None) -> Config:
    d = dict(DEFAULTS)
    f = paths.config_file
    if f.is_file():
        with f.open("rb") as fh:
            d.update({k: v for k, v in tomllib.load(fh).items() if k in DEFAULTS})
    d.update({k: v for k, v in (overrides or {}).items() if v is not None})
    if not d.get("ssh"):  # 未显式配 ssh → 跟随 host（改 host 时 ssh 不掉队）
        d["ssh"] = f"root@{d['host']}"
    return Config(**d)
