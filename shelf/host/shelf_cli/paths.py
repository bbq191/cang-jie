"""XDG 基目录规范（host 侧），与 Rust `shelf_core::paths`、shell `${XDG_*:-…}` 同一张表。"""
from __future__ import annotations

import os
from pathlib import Path

APP = "shelf"


def _pick(env: dict, var: str, default: Path) -> Path:
    v = env.get(var, "")
    return Path(v) if v.startswith("/") else default  # 规范：相对路径视为无效


class Paths:
    def __init__(self, env: dict | None = None):
        env = dict(os.environ) if env is None else env
        home = Path(env.get("HOME") or "~").expanduser()
        self.home = home
        self.config = _pick(env, "XDG_CONFIG_HOME", home / ".config") / APP
        self.data = _pick(env, "XDG_DATA_HOME", home / ".local/share") / APP
        self.state = _pick(env, "XDG_STATE_HOME", home / ".local/state") / APP
        self.cache = _pick(env, "XDG_CACHE_HOME", home / ".cache") / APP

    @property
    def config_file(self) -> Path:
        return self.config / "config.toml"

    @property
    def snapshots_dir(self) -> Path:
        return self.cache / "snapshots"
