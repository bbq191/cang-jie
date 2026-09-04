"""上传回执/守卫的单点实现：各命令（font/wallpaper/koreader/push）曾各写一份
「✗ 不是文件」守卫和「✓/✗ <名>: <消息>」回执行——收进这里，命令只调用。

各服务回执里的名字键不一：font/wallpaper 用 `name`，koreader/book(push) 用 `file`，
故 `name_key` 可选；成功项的额外尾注（如字体家族）走 `extra` 回调。"""
from __future__ import annotations

from pathlib import Path


def mark(ok: bool) -> str:
    return "✓" if ok else "✗"


def guard_file(f: Path) -> bool:
    """不是文件→打印 ✗ 并返回 False（调用方据此置 rc 并 continue）。"""
    if not f.is_file():
        print(f"✗ {f}: 不是文件")
        return False
    return True


def print_receipts(d: dict, name_key: str = "name", default_name: str = "?", prefix: str = "", extra=None, fallback_self: bool = False) -> int:
    """打印一个上传响应里每项的 ✓/✗ 回执，返回 rc（0=全 ok，1=有失败）。
    `fallback_self`：响应无 items 时把响应体自身当作单条（push 对 book-serve 用）。
    `extra(it)`：仅对成功项追加的尾注字符串（如 `  家族=…`）。"""
    items = d.get("items", [d] if fallback_self else [])
    rc = 0
    for it in items:
        ok = bool(it.get("ok"))
        tail = extra(it) if (extra and ok) else ""
        print(f"{mark(ok)} {prefix}{it.get(name_key, default_name)}: {it.get('message', '')}{tail}")
        if not ok:
            rc = 1
    return rc
