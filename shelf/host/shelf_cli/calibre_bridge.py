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


def _run(cmd: list[str], env_extra: dict | None = None, **kw) -> subprocess.CompletedProcess:
    env = clean_env()
    if env_extra:
        env.update(env_extra)
    return subprocess.run(cmd, env=env, check=False, text=True, capture_output=True, **kw)


REPO_ROOT = Path(__file__).resolve().parents[3]


def py_with_pymupdf() -> list[str]:
    """跑纯 pymupdf 脚本（check_output / pdf_crop_move）的解释器：它们不调 ebook-convert，可以走仓库 uv 环境
    （pymupdf 在 `calibre` 依赖组）；无 uv/pyproject 时退回系统 python3（需自行 pip 装 pymupdf）。"""
    if shutil.which("uv") and (REPO_ROOT / "pyproject.toml").is_file():
        return ["uv", "run", "--project", str(REPO_ROOT), "--group", "calibre", "python"]
    return ["python3"]


def wash(src: Path, out_dir: Path, env: dict | None = None) -> Path:
    """C2 洗书：wash_epub.sh（拍平 CSS/重建 TOC/伪 DRM 剥离/末步 epub-optimize）。返回产物路径。
    `env` 透传给脚本（如 `WASH_KEEP_PARA_SPACING=1` 保留段距）。"""
    r = _run(["sh", str(CALIBRE_DIR / "wash_epub.sh"), str(src), str(out_dir)], env_extra=env)
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
    r = _run([*py_with_pymupdf(), str(CALIBRE_DIR / "pdf_crop_move.py"), str(src), str(out)])
    if r.returncode == 3:
        raise ScannedPdf(r.stdout.strip() or r.stderr.strip())
    if r.returncode != 0 or not out.is_file():
        raise CalibreError(f"pdf_crop_move.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    return out


def has_k2pdfopt() -> bool:
    return shutil.which("k2pdfopt", path=clean_env().get("PATH")) is not None


def reflow_pdf(src: Path, outdir: Path) -> tuple[Path, str]:
    """PDF 重排（born-digital 结构化→EPUB / 扫描件 k2pdfopt|裁边→PDF）。返回 (产物路径, kind∈{'epub','pdf'})。"""
    import json

    r = _run([*py_with_pymupdf(), str(CALIBRE_DIR / "pdf_reflow_move.py"), str(src), str(outdir)])
    if r.returncode != 0:
        raise CalibreError(f"pdf_reflow_move.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    try:
        d = json.loads(r.stdout.strip().splitlines()[-1])
        return Path(d["out"]), d["kind"]
    except Exception as e:  # noqa: BLE001
        raise CalibreError(f"pdf_reflow_move.py 输出不可解析（{e}）：{r.stdout.strip()[-400:]}") from None


def check(path: Path, require_toc: bool = False) -> tuple[bool, str]:
    """C5 体检（check_output.py）：返回 (通过?, 输出)。硬拦项非零退出。"""
    cmd = [*py_with_pymupdf(), str(CALIBRE_DIR / "check_output.py"), str(path)]
    if require_toc:
        cmd.append("--require-toc")
    r = _run(cmd)
    return r.returncode == 0, (r.stdout + r.stderr).strip()


def comic_gray(src: Path, out: Path) -> tuple[Path, dict]:
    """漫画省刷新档：CBZ → 16 灰 CBZ（comic_gray.py，Pillow 走 uv calibre 组）。返回 (产物, {pages,gray,color,bytes_in,bytes_out})。"""
    import json

    r = _run([*py_with_pymupdf(), str(CALIBRE_DIR / "comic_gray.py"), str(src), str(out)])
    if r.returncode != 0 or not out.is_file():
        raise CalibreError(f"comic_gray.py 失败（rc={r.returncode}）：{(r.stderr or r.stdout).strip()[-800:]}")
    try:
        return out, json.loads(r.stdout.strip().splitlines()[-1])
    except Exception as e:  # noqa: BLE001
        raise CalibreError(f"comic_gray.py 输出不可解析（{e}）：{r.stdout.strip()[-400:]}") from None


def txt_to_epub(src: Path, outdir: Path) -> tuple[Path, dict]:
    """中文 TXT → 带目录 EPUB（txt_to_epub.py，stdlib）。返回 (产物, 元数据 {chapters, volumes, encoding, detected,…})。"""
    import json

    r = _run(["python3", str(CALIBRE_DIR / "txt_to_epub.py"), str(src), str(outdir)])
    if r.returncode != 0:
        raise CalibreError(f"txt_to_epub.py 失败（rc={r.returncode}）：{(r.stderr or r.stdout).strip()[-800:]}")
    try:
        d = json.loads(r.stdout.strip().splitlines()[-1])
        return Path(d["out"]), d
    except Exception as e:  # noqa: BLE001
        raise CalibreError(f"txt_to_epub.py 输出不可解析（{e}）：{r.stdout.strip()[-400:]}") from None


def comic2cbz(src: Path, out: Path) -> Path:
    """漫画 AZW3/MOBI/EPUB → CBZ（Calibre 解包成 EPUB 中转，按 spine 顺序抽整页图）。"""
    r = _run(["python3", str(CALIBRE_DIR / "comic2cbz.py"), str(src), str(out)])
    if r.returncode != 0 or not out.is_file():
        raise CalibreError(f"comic2cbz.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    return out


def render_probe(out_dir: Path, title: str) -> Path:
    """`shelf doctor --render` 的探针 EPUB（纯 stdlib 脚本 render_probe.py）。"""
    r = _run(["python3", str(CALIBRE_DIR / "render_probe.py"), str(out_dir), title])
    if r.returncode != 0:
        raise CalibreError(f"render_probe.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    return Path(r.stdout.strip().splitlines()[-1])


def render_measure(pdf: Path) -> dict:
    """量 xochitl 渲染缓存里探针段的首行缩进（pymupdf）。返回 {rows, ok, problems}。"""
    import json

    r = _run([*py_with_pymupdf(), str(CALIBRE_DIR / "render_measure.py"), str(pdf)])
    if r.returncode != 0:
        raise CalibreError(f"render_measure.py 失败（rc={r.returncode}）：{r.stderr.strip()[-800:]}")
    try:
        return json.loads(r.stdout.strip().splitlines()[-1])
    except Exception as e:  # noqa: BLE001
        raise CalibreError(f"render_measure.py 输出不可解析（{e}）：{r.stdout.strip()[-400:]}") from None


def workdir(prefix: str = "shelf-push-") -> Path:
    return Path(tempfile.mkdtemp(prefix=prefix))
