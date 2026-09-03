"""Calibre 桥（host 高质量路）：subprocess 调 `shelf/host/calibre/` 的脚本。
统一清洗环境：去掉 VIRTUAL_ENV / PATH 里的 .venv/bin —— `ebook-convert` 的 `#!/usr/bin/env python3`
在 venv 里会被劫持即炸（阅读白皮书 §11.2）。"""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

CALIBRE_DIR = Path(__file__).resolve().parent.parent / "calibre"


def clean_env(env: dict | None = None) -> dict:
    e = dict(os.environ if env is None else env)
    e.pop("VIRTUAL_ENV", None)
    e["PATH"] = os.pathsep.join(p for p in e.get("PATH", "").split(os.pathsep) if ".venv/bin" not in p and p)
    return e


def has_calibre() -> bool:
    return shutil.which("ebook-convert", path=clean_env().get("PATH")) is not None


class CalibreError(RuntimeError):
    pass


def _run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, env=clean_env(), check=False, text=True, capture_output=True, **kw)


REPO_ROOT = Path(__file__).resolve().parents[3]


def _py_with_pymupdf() -> list[str]:
    """跑纯 pymupdf 脚本（check_output / pdf_crop_move）的解释器：它们不调 ebook-convert，可以走仓库 uv 环境
    （pymupdf 在 `calibre` 依赖组）；无 uv/pyproject 时退回系统 python3（需自行 pip 装 pymupdf）。"""
    if shutil.which("uv") and (REPO_ROOT / "pyproject.toml").is_file():
        return ["uv", "run", "--project", str(REPO_ROOT), "--group", "calibre", "python"]
    return ["python3"]


def wash(src: Path, out_dir: Path) -> Path:
    """C2 洗书：wash_epub.sh（拍平 CSS/重建 TOC/伪 DRM 剥离/末步 epub-optimize）。返回产物路径。"""
    r = _run(["sh", str(CALIBRE_DIR / "wash_epub.sh"), str(src), str(out_dir)])
    if r.returncode != 0:
        raise CalibreError(f"wash_epub.sh 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    outs = sorted(out_dir.glob("*.epub"), key=lambda p: p.stat().st_mtime)
    if not outs:
        raise CalibreError("wash_epub.sh 未产出 EPUB")
    return outs[-1]


def to_pdf(src: Path, out: Path) -> Path:
    """C1 定稿：epub2pdf_move.sh → 954×1696 固定版式 PDF。"""
    r = _run(["sh", str(CALIBRE_DIR / "epub2pdf_move.sh"), str(src), str(out)])
    if r.returncode != 0 or not out.is_file():
        raise CalibreError(f"epub2pdf_move.sh 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    return out


class ScannedPdf(Exception):
    """pdf_crop_move.py 退出码 3：扫描型 PDF 不产出，建议改投 KOReader（KOPT 重排）。"""


def crop_pdf(src: Path, out: Path) -> Path:
    r = _run([*_py_with_pymupdf(), str(CALIBRE_DIR / "pdf_crop_move.py"), str(src), str(out)])
    if r.returncode == 3:
        raise ScannedPdf(r.stdout.strip() or r.stderr.strip())
    if r.returncode != 0 or not out.is_file():
        raise CalibreError(f"pdf_crop_move.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    return out


def check(path: Path, require_toc: bool = False) -> tuple[bool, str]:
    """C5 体检（check_output.py）：返回 (通过?, 输出)。硬拦项非零退出。"""
    cmd = [*_py_with_pymupdf(), str(CALIBRE_DIR / "check_output.py"), str(path)]
    if require_toc:
        cmd.append("--require-toc")
    r = _run(cmd)
    return r.returncode == 0, (r.stdout + r.stderr).strip()


def comic2cbz(src: Path, out: Path) -> Path:
    r = _run(["python3", str(CALIBRE_DIR / "comic2cbz.py"), str(src), str(out)])
    if r.returncode != 0 or not out.is_file():
        raise CalibreError(f"comic2cbz.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    return out


def workdir(prefix: str = "shelf-push-") -> Path:
    return Path(tempfile.mkdtemp(prefix=prefix))
