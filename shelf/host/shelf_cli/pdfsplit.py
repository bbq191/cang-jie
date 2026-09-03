"""大 PDF 分卷：xochitl `/upload` 有体积上限（60~285MB 间 413），>split_mb 的 PDF 按页数均分成若干卷。
依赖 pymupdf（可选）；缺则不分卷、只告警。"""
from __future__ import annotations

from pathlib import Path


def needs_split(path: Path, split_mb: int) -> bool:
    return split_mb > 0 and path.suffix.lower() == ".pdf" and path.stat().st_size > split_mb * 1024 * 1024


def volumes_for(size_bytes: int, split_mb: int) -> int:
    cap = split_mb * 1024 * 1024
    return max(1, -(-size_bytes // cap))  # ceil


def split(path: Path, split_mb: int, out_dir: Path) -> list[Path]:
    try:
        import fitz  # type: ignore
    except ImportError:
        return [path]
    n = volumes_for(path.stat().st_size, split_mb)
    if n <= 1:
        return [path]
    doc = fitz.open(str(path))
    pages = doc.page_count
    per = -(-pages // n)
    outs = []
    for i in range(n):
        a, b = i * per, min(pages, (i + 1) * per) - 1
        if a > b:
            break
        part = fitz.open()
        part.insert_pdf(doc, from_page=a, to_page=b)
        o = out_dir / f"{path.stem} ({i + 1}of{n}).pdf"
        part.save(str(o), garbage=4, deflate=True)
        part.close()
        outs.append(o)
    doc.close()
    return outs
