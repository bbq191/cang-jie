"""漫画识别：决定 `shelf push` 走文字书洗书路还是漫画路（CBZ + PDF）。

- `.cbz`：天然漫画。
- `.epub`：按 OPF spine 统计全书 `<img>` 数与可见文字数：图 ≥ 20 张且**平均每张图配的文字 < 40 字**判漫画
  （Calibre 洗过的漫画 EPUB 一页 xhtml 塞十几张图、几乎无字；文字书是几百字配零星插图；不调 Calibre，毫秒级）。
- `.pdf`（2026-09-14 补：此前完全不判，扫描版漫画 PDF 会误入文字书重排路必然失败，见书架白皮书 §04）：
  抽样页统计"有图且几乎无文字"的页占比，图片页 ≥ 20 张且占比 ≥ 60% 判漫画——跟上面这条同一套阈值，但
  判定本身要真正打开、逐页解析（pymupdf 子进程，见 `calibre_bridge.py::pdf_comic_stats()` /
  `shelf/host/calibre/pdf_comic_probe.py`），做不到"零依赖毫秒级"，是本模块唯一需要 pymupdf（`calibre`
  依赖组）的分支，缺这个依赖时静默退回 False（走原来的文字书路，不阻断推送）。
其余格式不判（False）。判错可用 `--comic / --no-comic` 手动覆盖。

⚠ 2026-09-17 起不再判 `.azw3/.mobi/.azw/.prc`（原是解析 PalmDB 记录表判漫画）——EPUB 线架构调整后
这几个格式已经进不了母版库（服务端上传门拒收，见 `rmsvc_core::formats::BOOK_EXTS`），判了也没有意义，
`palmdb_image_ratio` 连带移除；用户想传 AZW3/MOBI 漫画，请自行先转成 CBZ/EPUB/PDF。
"""
from __future__ import annotations

import posixpath
import re
import zipfile
from pathlib import Path

MIN_PAGES = 20
PALM_IMAGE_RATIO = 0.6  # 名字沿用历史（原给 PalmDB 判据），现在只有 PDF 分支还在用这个阈值。
EPUB_TEXT_PER_IMAGE = 40


def epub_image_stats(z: zipfile.ZipFile) -> tuple[int, int]:
    """(spine 页里 <img>/<image> 总数, 可见文字总字数)。"""
    try:
        container = z.read("META-INF/container.xml").decode("utf-8", "ignore")
        opf_path = re.search(r'full-path="([^"]+)"', container).group(1)
        opf = z.read(opf_path).decode("utf-8", "ignore")
    except Exception:  # noqa: BLE001
        return 0, 0
    opf_dir = posixpath.dirname(opf_path)
    manifest = {}
    for m in re.finditer(r"<item\b[^>]*>", opf):
        mid = re.search(r'\bid="([^"]+)"', m.group(0))
        href = re.search(r'\bhref="([^"]+)"', m.group(0))
        if mid and href:
            manifest[mid.group(1)] = href.group(1)
    names = set(z.namelist())
    images = 0
    text = 0
    for idref in re.findall(r'<itemref[^>]*\bidref="([^"]+)"', opf):
        href = manifest.get(idref)
        if not href:
            continue
        page = posixpath.normpath(posixpath.join(opf_dir, href)) if opf_dir else href
        if page.lower().endswith((".jpg", ".jpeg", ".png", ".gif", ".webp")):
            images += 1
            continue
        if page not in names:
            continue
        html = z.read(page).decode("utf-8", "ignore")
        images += len(re.findall(r"<(?:img|image)\b", html, re.I))
        body = re.sub(r"(?is)<(script|style|head)\b.*?</\1>", "", html)
        text += len(re.sub(r"\s+", "", re.sub(r"<[^>]+>", "", body)))
    return images, text


def is_comic(path: Path) -> bool:
    suf = path.suffix.lower()
    if suf == ".cbz":
        return True
    if suf == ".epub":
        try:
            with zipfile.ZipFile(path) as z:
                images, text = epub_image_stats(z)
        except zipfile.BadZipFile:
            return False
        return images >= MIN_PAGES and text / images < EPUB_TEXT_PER_IMAGE
    if suf == ".pdf":
        try:
            with path.open("rb") as f:
                if f.read(5) != b"%PDF-":
                    return False
        except OSError:
            return False
        from . import calibre_bridge as cb  # 延迟导入：只有这条分支要 spawn pymupdf 子进程，其余分支保持零依赖
        n, ratio = cb.pdf_comic_stats(path)
        return n >= MIN_PAGES and ratio >= PALM_IMAGE_RATIO
    return False
