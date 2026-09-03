"""host 环境体检：系统 python3、uv/venv 劫持、Calibre、pymupdf、ssh；设备侧：网关可达。"""
import os
import shutil
import sys

NAME = "doctor"
HELP = "检查 host 依赖（Calibre/pymupdf/venv 劫持）与设备可达性"


def add_args(p):
    pass


def venv_hijack(env: dict) -> bool:
    """在 uv/venv 里跑会劫持 ebook-convert 的 `#!/usr/bin/env python3`。"""
    return bool(env.get("VIRTUAL_ENV")) or any(".venv/bin" in p for p in env.get("PATH", "").split(os.pathsep))


def run(args, ctx) -> int:
    rc = 0
    print(f"python3       : {sys.executable} ({sys.version.split()[0]})")
    if venv_hijack(dict(os.environ)):
        print("venv 劫持     : ⚠ 处于 VIRTUAL_ENV/.venv PATH 中——调 Calibre 前会自动清洗（calibre_bridge）")
    else:
        print("venv 劫持     : 无")
    for tool, why in [("ebook-convert", "host 高质量路（洗书/定稿）"), ("ssh", "KOReader 配置同步"), ("pdfinfo", "可选")]:
        p = shutil.which(tool)
        print(f"{tool:<14}: {p or '缺（' + why + '）'}")
        if tool == "ebook-convert" and not p:
            print("                → 无 Calibre 时 `shelf push` 自动走 --quality device（设备端 Rust 兜底）")
    try:
        import fitz  # type: ignore  # noqa: F401
        print("pymupdf       : 有")
    except ImportError:
        print("pymupdf       : 缺（大 PDF 分卷/体检不可用）")
    print(f"认证          : 用户 {ctx.config.user}，密码{'已提供' if ctx.config.password else '未提供（config.toml password / $SHELF_PASSWORD / 交互输入）'}；TLS 校验 {'开' if ctx.config.verify_tls else '关（自签）'}")
    try:
        n = len(ctx.transport.get("/api/services").get("services", []))
        print(f"设备网关      : {ctx.config.base_url} 在线，{n} 个服务")
    except Exception as e:  # noqa: BLE001
        rc = 1
        print(f"设备网关      : {ctx.config.base_url} 不可达（{e}）")
    return rc
