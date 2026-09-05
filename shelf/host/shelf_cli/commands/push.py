"""`shelf push`：host 洗书 → 落**母版库**（中间层），去向由用户在网页选（xochitl / KOReader）。

统一后 push 不再有投递目标——一律落母版库（`/api/books/staging`）。host 是唯一能"入库时顺带优化"的源：
- 默认：有 Calibre → 洗书（EPUB 深洗 / 杂格式转 EPUB / PDF 结构化重排）→ 落母版库（产物带优化标记）。
- `--no-optimize`：不洗，原样传母版库（用户可在网页按需点优化）。
- `--to-pdf`：定稿成固定版式 PDF（手写批注用），落母版库。
无 Calibre → 原样传母版库（设备端优化在网页母版库里点）。
规则与网页一致：**所有书只落母版库**，没有绕过母版库直投读器的选项（2026-09-05 用户定）。
"""
from __future__ import annotations

from pathlib import Path

from .. import calibre_bridge as cb
from .. import pdfsplit
from ..receipts import guard_file, upload_each

NAME = "push"
HELP = "投书到母版库（host 有 Calibre 先洗书）；去向在网页选。难搞的书/PDF 重排用这条"

# host 能洗的源格式（其余原样传母版库）。
WASH_EXT = {".epub", ".azw3", ".mobi", ".azw", ".prc", ".fb2"}


def add_args(p):
    p.add_argument("files", nargs="+", type=Path)
    p.add_argument("--to-pdf", action="store_true", help="定稿成固定版式 PDF（手写批注用）；默认洗成流式 EPUB")
    p.add_argument("--no-optimize", action="store_true", help="不洗，原样传母版库（网页里可再点优化）")
    p.add_argument("--keep-spacing", action="store_true", help="洗书时保留原书段间距（诗集 / 剧本；对应网页「清洗但保留段距」档位）")
    p.add_argument("--no-reflow", action="store_true", help="PDF 不重排（原样传）")
    p.add_argument("--no-split", action="store_true", help="大 PDF 不分卷")
    p.add_argument("--require-toc", action="store_true", help="洗书体检要求有目录")
    p.add_argument("--skip-check", action="store_true", help="跳过 check_output.py 体检（缺省不过不推）")
    p.add_argument("--dry-run", "-n", action="store_true", help="只打印会怎么做，不动文件、不上传")


def _gate(out: Path, args) -> None:
    """原生高质量门：host 洗书产物必过 check_output.py（TOC/内链/字体子集/双 id/屏上字号列宽），不过不推。"""
    if getattr(args, "skip_check", False):
        print("  （--skip-check：跳过体检）")
        return
    ok, rep = cb.check(out, require_toc=args.require_toc)
    print(rep)
    if not ok:
        raise cb.CalibreError("体检未通过，未推送（--skip-check 强推）")


def host_prepare(path: Path, args, work: Path) -> list[Path]:
    """host 洗书：默认 EPUB 深洗 / 杂格式转 EPUB / PDF 结构化重排；--to-pdf 定稿 PDF。产物待落母版库。"""
    suf = path.suffix.lower()
    wenv = {"WASH_KEEP_PARA_SPACING": "1"} if getattr(args, "keep_spacing", False) else None  # 与网页档位对齐
    if args.to_pdf:
        # 定稿固定版式 PDF（EPUB/杂格式先转 EPUB 再定稿；PDF 裁边）。
        if suf == ".epub" or suf in WASH_EXT:
            src = path if suf == ".epub" else cb.wash(path, work, env=wenv)
            out = cb.to_pdf(src, work / (path.stem + ".pdf"))
        elif suf == ".pdf":
            try:
                out = cb.crop_pdf(path, work / (path.stem + ".crop.pdf"))
            except cb.ScannedPdf as e:
                raise cb.CalibreError(f"扫描型 PDF 不做定稿（{e}）；去掉 --to-pdf 走重排，或用网页投 KOReader") from None
        else:
            return [path]
        _gate(out, args)
        return [out]
    # 默认：PDF 结构化重排 / EPUB·杂格式深洗成流式 EPUB。
    if suf == ".pdf":
        if args.no_reflow:
            return [path]
        reflowed, kind = cb.reflow_pdf(path, work)
        if kind == "epub":
            print(f"  结构化重排 → EPUB（{reflowed.name}）")
            out = cb.wash(reflowed, work, env=wenv)
            _gate(out, args)
            return [out]
        print(f"  扫描件位图重排 → PDF（{reflowed.name}）")
        return [reflowed]
    if suf == ".epub" or suf in WASH_EXT:
        out = cb.wash(path, work, env=wenv)  # wash_epub.sh 泛化收 AZW3/MOBI（内部 ebook-convert），末步叠加 epub-optimize
        _gate(out, args)
        return [out]
    return [path]


def run(args, ctx) -> int:
    calibre = cb.has_calibre()
    do_wash = calibre and not args.no_optimize
    rc = 0
    landed = 0
    work = cb.workdir()
    for path in args.files:
        if not guard_file(path):
            rc = 1
            continue
        print(f"→ {path.name}  {'洗书→' if do_wash else '原样→'}母版库")
        if args.dry_run:
            continue
        try:
            outs = host_prepare(path, args, work) if do_wash else [path]
        except cb.CalibreError as e:
            print(f"✗ {path.name}: {e}")
            rc = 1
            continue
        final: list[Path] = []
        for o in outs:
            if not args.no_split and o.suffix.lower() == ".pdf" and pdfsplit.needs_split(o, ctx.config.split_pdf_mb):
                parts = pdfsplit.split(o, ctx.config.split_pdf_mb, work)
                if len(parts) > 1:
                    print(f"  分卷 {len(parts)} 份（>{ctx.config.split_pdf_mb}MB）")
                final.extend(parts)
            else:
                final.append(o)
        # 只落母版库（原样落，host 已洗则带优化标记）；去向在网页「传书 → 母版库」选。
        rc |= upload_each(ctx.transport, "/api/books/staging", final)
        landed += len(final)
    if landed and not args.dry_run:
        scheme = getattr(ctx.config, "scheme", "https")
        print(f"→ 已入母版库。去 {scheme}://{ctx.config.host}:{ctx.config.port}/ 「传书 → 母版库」点优化 / 选去向（xochitl / KOReader）")
    return rc
