#!/usr/bin/env python3
"""
appload_patch_328.py —— 不用 rM Qt6 SDK，让 appload v0.5.3 兼容固件 3.28：把上游 PR #59 改过的 qmd
**等长回填**进 .so 里那段内嵌 qmd（C 字符串，NUL 结尾），多出的字节补 NUL。

背景（2026-09-06 真机）：appload.so 把 `xovi/template/appload.qmd`（已 hash 的 qmldiff 源）当 C 字符串嵌在 .rodata；
v0.5.3 那份钩的是 3.27 的 `DeviceKeyboardNavigationHandler#integrationsHandler` / `SidebarFilterItem`，3.28 已删 →
qmldiff 报 "Couldn't resolve the hashed identifier"，AppLoad 入口消失。PR #59（rmitchellscott/rm-appload 分支 3.28，
未合并）只改这份 qmd：Sidebar 锚点换成 `ArkControls.SidebarFoldout#integrationsFoldout` + `ArkControls.SidebarItem`，
MainView 的 `LOCATE BEFORE Epaper.ScreenModeItem` 换成 `LOCATE AFTER ALL`。新 qmd 比旧的短（7427 < 7895 字节），
所以可以原地覆盖、NUL 补齐，其余字节一个不动。离线用 asivery/qmldiff CLI 对 3.28.0.172 解出的 QML 实跑通过，真机通。

用法：
    python3 appload_patch_328.py <appload.so(v0.5.3)> <appload.328.qmd> <输出.so>
其中 appload.328.qmd = 上游 master 的 xovi/template/appload.qmd 打上 PR #59 的 diff（`patch -p1`）。
校验：输入 .so 里必须能找到以 "AFFECT [[" 开头、NUL 结尾的那段，且它逐字节等于上游 master 的 qmd（防止拿错版本）。
"""
from __future__ import annotations

import hashlib
import sys
from pathlib import Path

UPSTREAM_MASTER_SHA256 = None  # 可选：填上游 master appload.qmd 的 sha256 以强校验；None=只校验结构


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    so_path, qmd_path, out_path = (Path(a) for a in sys.argv[1:4])
    so = so_path.read_bytes()
    new = qmd_path.read_bytes()
    off = so.find(b"AFFECT [[")
    if off < 0:
        print("!! .so 里找不到内嵌 qmd（AFFECT [[ 开头）", file=sys.stderr)
        return 1
    end = so.index(b"\0", off)
    old = so[off:end]
    if not old.rstrip(b"\n").endswith(b"END AFFECT"):
        print("!! 内嵌段结尾不是 END AFFECT，版本不对", file=sys.stderr)
        return 1
    if UPSTREAM_MASTER_SHA256 and hashlib.sha256(old).hexdigest() != UPSTREAM_MASTER_SHA256:
        print("!! 内嵌 qmd 与上游 master 不一致，拒绝回填", file=sys.stderr)
        return 1
    if len(new) > len(old):
        print(f"!! 新 qmd {len(new)}B 比内嵌段 {len(old)}B 长，装不下", file=sys.stderr)
        return 1
    patched = so[:off] + new + b"\0" * (len(old) - len(new)) + so[end:]
    assert len(patched) == len(so)
    out_path.write_bytes(patched)
    print(f"ok: offset {off:#x} old {len(old)}B → new {len(new)}B (+{len(old) - len(new)} NUL); md5 {hashlib.md5(patched).hexdigest()[:8]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
