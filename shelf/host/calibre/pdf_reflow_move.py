"""PDF 重排（host 侧，2026-09-04 定案：born-digital 结构化重排→EPUB，扫描件回退 k2pdfopt/裁边）。

第一性依据（见书架白皮书）：k2pdfopt 是位图重排引擎（为通吃扫描件，代价=文字变图不可选）；
born-digital 学术 PDF 自带文字+图/公式 bbox（PyMuPDF），**结构化重排比位图更简单更好、且产物 EPUB
能复用书架 EPUB 管线**（xochitl 内联脚注/KOReader 弹窗/一致排版）。故：
  - 有可抽取文字层 → 结构化：抽 blocks/图 bbox、按 x 聚列、列内按 y 定阅读序、文字重排、图/公式裁原区
    当整块不切 → 组 EPUB（每页一章，注 Move 屏 CSS）；再交由上层 wash + epub-optimize 统一优化。
  - 无/极低文字层（扫描件）→ 调 k2pdfopt 二进制（缺则回退 pdf_crop_move.py 裁边）→ 输出 PDF。

用法: pdf_reflow_move.py <输入.pdf> <输出目录>
输出: 打印一行 JSON {"out": "<产物路径>", "kind": "epub"|"pdf"}；rc 0 成功，2 失败。
"""

from __future__ import annotations

import html
import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

try:
    import pymupdf as fitz  # 新名（PyMuPDF ≥1.24）
except ImportError:  # 老环境回退旧名
    import fitz

from move_screen import H_PX, W_PX  # noqa: E402  单一事实源屏常量

# born-digital 判据：平均每页可抽取文字 ≥ 该字符数 → 有文字层。扫描件通常近 0。
MIN_CHARS_PER_PAGE = 80
# 标题判据：span 字号 ≥ 页正文中位字号 × 该倍数 → 当作标题（用 h2）。
HEADING_FONT_RATIO = 1.35


def has_k2pdfopt() -> bool:
    return shutil.which("k2pdfopt") is not None


def _columns(blocks: list[dict], page_w: float) -> list[list[dict]]:
    """把文字块按 x 中点聚成列：若存在贯穿页面的竖向空白（两组块 x 区间有明显间隙）→ 多列。
    简化稳妥版：按块中点排序，若相邻中点跨过 page 中线且左右两组各有量 → 2 列；否则单列。"""
    text_blocks = [b for b in blocks if b.get("type", 0) == 0 and b.get("lines")]
    if len(text_blocks) < 4:
        return [blocks]
    mids = [((b["bbox"][0] + b["bbox"][2]) / 2) for b in text_blocks]
    left = [b for b, m in zip(text_blocks, mids) if m < page_w / 2]
    right = [b for b, m in zip(text_blocks, mids) if m >= page_w / 2]
    # 两侧都够量、且右列最左 > 左列最右（有真空隙）才算双列
    if len(left) >= 2 and len(right) >= 2:
        left_max_r = max(b["bbox"][2] for b in left)
        right_min_l = min(b["bbox"][0] for b in right)
        if right_min_l >= left_max_r - 2:
            return [blocks]  # 交叠：当单列更稳（宁可不拆错）
        return [left, right]
    return [blocks]


def _block_text_html(block: dict, body_size: float) -> str:
    """一个文字块 → <h2>/<p>。行内 span 拼接，字号显著大于正文当标题。"""
    lines_out = []
    max_size = 0.0
    for line in block.get("lines", []):
        parts = []
        for span in line.get("spans", []):
            t = span.get("text", "")
            if t:
                parts.append(t)
                max_size = max(max_size, span.get("size", 0.0))
        if parts:
            lines_out.append("".join(parts))
    text = " ".join(s.strip() for s in lines_out if s.strip()).strip()
    if not text:
        return ""
    tag = "h2" if body_size > 0 and max_size >= body_size * HEADING_FONT_RATIO else "p"
    return f"<{tag}>{html.escape(text)}</{tag}>"


def _page_html(page: fitz.Page, imgdir: Path, page_no: int) -> str:
    """一页 → xhtml body 片段：文字块重排为 p/h、图块裁原区当整块 <img>（xochitl 缩到列宽）。"""
    d = page.get_text("dict")
    page_w = d.get("width", page.rect.width)
    blocks = d.get("blocks", [])
    # 正文字号 = 按字符数加权最多的那档（比中位鲁棒：稀疏页里标题不会把基准抬高）。
    size_chars: dict[int, int] = {}
    for b in blocks:
        if b.get("type", 0) != 0:
            continue
        for ln in b.get("lines", []):
            for sp in ln.get("spans", []):
                sz = round(sp.get("size", 0.0))
                if sz > 0:
                    size_chars[sz] = size_chars.get(sz, 0) + len(sp.get("text", ""))
    body_size = float(max(size_chars, key=lambda k: size_chars[k])) if size_chars else 0.0
    out: list[str] = []
    for col in _columns(blocks, page_w):
        for b in sorted(col, key=lambda b: (round(b["bbox"][1] / 4), b["bbox"][0])):  # 列内按 y 再 x
            if b.get("type", 0) == 1:  # 图块：裁原区当整块保留（不切，含图/公式/表）
                bbox = fitz.Rect(b["bbox"])
                if bbox.width < 8 or bbox.height < 8:
                    continue
                pix = page.get_pixmap(clip=bbox, matrix=fitz.Matrix(2, 2))  # 2× 采样，后续降采样交 imgopt
                name = f"p{page_no}_{len(out)}.png"
                pix.save(str(imgdir / name))
                out.append(f'<p class="fig"><img src="../images/{name}" alt=""/></p>')
            else:
                frag = _block_text_html(b, body_size)
                if frag:
                    out.append(frag)
    return "\n".join(out) if out else "<p>&#160;</p>"


def _build_epub(pages_html: list[str], imgdir: Path, title: str, out: Path) -> None:
    """极简 EPUB3：mimetype(STORED) + container + opf + nav + 每页一章 xhtml。图片在 images/。
    产物随后由上层 wash/epub-optimize 统一优化（此处不注排版细节，交给统一管线）。"""
    css = f"@page{{margin:0}}body{{margin:0}}img{{max-width:100%}}.fig{{text-align:center;margin:0}}"
    # Move 屏竖向 CSS 提示（认 CSS 的阅读器用；xochitl 忽略、走自身列宽）
    css += f"/* Move {W_PX}x{H_PX} */"
    manifest, spine, navlis = [], [], []
    chaps = {}
    for i, body in enumerate(pages_html, 1):
        cid = f"c{i}"
        fname = f"text/{cid}.xhtml"
        chaps[fname] = (
            f'<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<html xmlns="http://www.w3.org/1999/xhtml"><head><meta charset="utf-8"/>'
            f'<title>{html.escape(title)} · {i}</title><link rel="stylesheet" href="../style.css"/></head>'
            f"<body>{body}</body></html>"
        )
        manifest.append(f'<item id="{cid}" href="{fname}" media-type="application/xhtml+xml"/>')
        spine.append(f'<itemref idref="{cid}"/>')
        navlis.append(f'<li><a href="{fname}">第 {i} 页</a></li>')
    for img in sorted(imgdir.glob("*.png")):
        iid = img.stem
        manifest.append(f'<item id="{iid}" href="images/{img.name}" media-type="image/png"/>')
    opf = (
        f'<?xml version="1.0" encoding="UTF-8"?>\n<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bid">'
        f'<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bid">shelf-reflow-{title}</dc:identifier>'
        f'<dc:title>{html.escape(title)}</dc:title><dc:language>zh</dc:language></metadata>'
        f'<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
        f'<item id="css" href="style.css" media-type="text/css"/>{"".join(manifest)}</manifest>'
        f'<spine>{"".join(spine)}</spine></package>'
    )
    nav = (
        f'<?xml version="1.0" encoding="UTF-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">'
        f'<head><meta charset="utf-8"/><title>目录</title></head><body><nav epub:type="toc"><ol>{"".join(navlis)}</ol></nav></body></html>'
    )
    with zipfile.ZipFile(out, "w") as z:
        z.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", '<?xml version="1.0"?>\n<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>', compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/content.opf", opf, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/nav.xhtml", nav, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/style.css", css, compress_type=zipfile.ZIP_DEFLATED)
        for fname, data in chaps.items():
            z.writestr(f"OEBPS/{fname}", data, compress_type=zipfile.ZIP_DEFLATED)
        for img in sorted(imgdir.glob("*.png")):
            z.writestr(f"OEBPS/images/{img.name}", img.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)


def _reflow_scanned(src: Path, out_pdf: Path) -> bool:
    """扫描件：k2pdfopt 位图重排（缺则回退 pdf_crop_move.py 裁边）。返回是否产出。"""
    if has_k2pdfopt():
        # -w/-h 目标屏、-mode fw(fit width) 保图整块、-x 退出不等按键、-o 输出
        r = subprocess.run(["k2pdfopt", str(src), "-w", str(W_PX), "-h", str(H_PX), "-mode", "fw", "-x", "-o", str(out_pdf)], check=False, capture_output=True, text=True)
        return r.returncode == 0 and out_pdf.is_file()
    # 回退裁边（同目录 pdf_crop_move.py）
    crop = Path(__file__).resolve().parent / "pdf_crop_move.py"
    r = subprocess.run([sys.executable, str(crop), str(src), str(out_pdf)], check=False, capture_output=True, text=True)
    return r.returncode in (0,) and out_pdf.is_file()


def main() -> int:
    if len(sys.argv) != 3:
        print("用法: pdf_reflow_move.py <输入.pdf> <输出目录>", file=sys.stderr)
        return 2
    src, outdir = Path(sys.argv[1]), Path(sys.argv[2])
    outdir.mkdir(parents=True, exist_ok=True)
    try:
        doc = fitz.open(str(src))
    except Exception as e:  # noqa: BLE001
        print(f"打开 PDF 失败: {e}", file=sys.stderr)
        return 2
    n = doc.page_count or 1
    total_chars = sum(len(doc[i].get_text("text")) for i in range(min(n, 10)))  # 采样前 10 页
    born_digital = (total_chars / min(n, 10)) >= MIN_CHARS_PER_PAGE
    title = src.stem
    if born_digital:
        imgdir = outdir / "_img"
        imgdir.mkdir(exist_ok=True)
        pages = [_page_html(doc[i], imgdir, i) for i in range(n)]
        out = outdir / f"{title}.epub"
        _build_epub(pages, imgdir, title, out)
        shutil.rmtree(imgdir, ignore_errors=True)
        print(json.dumps({"out": str(out), "kind": "epub"}))
        return 0
    out_pdf = outdir / f"{title}.reflow.pdf"
    if _reflow_scanned(src, out_pdf):
        print(json.dumps({"out": str(out_pdf), "kind": "pdf"}))
        return 0
    print("扫描件重排失败（无 k2pdfopt 且裁边未产出）", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
