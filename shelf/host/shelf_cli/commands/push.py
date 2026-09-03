"""`shelf push`：把书投到 native / annot / koreader。质量路由（Strategy）：
- host   有 Calibre → native: wash → 体检 → 推；annot: epub→定稿 PDF / pdf→裁边 → 体检 → 推；koreader: 原样（--comic2cbz 可选）
- device 直推网关，设备端 Rust 转换/优化兜底
- auto   有 ebook-convert 且目标为 native/annot 且文件需要处理 → host，否则 device"""
from __future__ import annotations

from pathlib import Path

from .. import calibre_bridge as cb
from .. import pdfsplit

NAME = "push"
HELP = "投书：--target native|annot|koreader（host 有 Calibre 走高质量路，否则设备兜底）"

HOST_ROUTE_EXT = {".epub", ".azw3", ".mobi", ".azw", ".prc", ".fb2", ".pdf"}


def add_args(p):
    p.add_argument("files", nargs="+", type=Path)
    p.add_argument("--target", "-t", choices=["native", "annot", "koreader"], help="缺省 config.default_target")
    p.add_argument("--folder", "-f", default="", help="目标文件夹（xochitl visibleName / KOReader books 子目录）")
    p.add_argument("--quality", "-q", choices=["auto", "host", "device"], help="缺省 config.quality")
    p.add_argument("--no-optimize", action="store_true", help="native 直传 EPUB 不在设备端优化")
    p.add_argument("--no-split", action="store_true", help="大 PDF 不分卷")
    p.add_argument("--require-toc", action="store_true", help="host 路体检要求有目录")
    p.add_argument("--skip-check", action="store_true", help="host 路跳过 check_output.py 体检（缺省不过不推）")
    p.add_argument("--comic2cbz", action="store_true", help="koreader 目标：AZW3 漫画先转 CBZ")
    p.add_argument("--dry-run", "-n", action="store_true", help="只打印路由决定，不动文件、不上传")


def decide_route(quality: str, target: str, path: Path, calibre: bool) -> str:
    if quality == "device":
        return "device"
    if quality == "host":
        return "host" if calibre else "device"
    # auto
    if target in ("native", "annot") and calibre and path.suffix.lower() in HOST_ROUTE_EXT:
        return "host"
    if target == "koreader" and calibre and path.suffix.lower() == ".azw3":
        return "host"  # 只在 --comic2cbz 时真做事
    return "device"


def _gate(out: Path, args) -> None:
    """原生高质量门：host 路产物必过 check_output.py（TOC/内链/字体子集/双 id/屏上字号列宽），不过不推。"""
    if getattr(args, "skip_check", False):
        print("  （--skip-check：跳过体检）")
        return
    ok, rep = cb.check(out, require_toc=args.require_toc)
    print(rep)
    if not ok:
        raise cb.CalibreError("体检未通过，未推送（--skip-check 强推 / --quality device 走设备兜底）")


def host_prepare(target: str, path: Path, args, work: Path) -> list[Path]:
    """host 路：产出待推送文件列表（可能多个：分卷）。"""
    suf = path.suffix.lower()
    if target == "koreader":
        if args.comic2cbz and suf == ".azw3":
            return [cb.comic2cbz(path, work / (path.stem + ".cbz"))]
        return [path]
    if target == "native":
        if suf == ".epub":
            out = cb.wash(path, work)
        elif suf in (".azw3", ".mobi", ".azw", ".prc", ".fb2"):
            out = cb.wash(path, work)  # wash_epub.sh 泛化收 AZW3/MOBI（内部 ebook-convert）
        else:
            return [path]
        _gate(out, args)
        return [out]
    # annot
    if suf == ".epub" or suf in (".azw3", ".mobi", ".azw", ".prc", ".fb2"):
        src = path if suf == ".epub" else cb.wash(path, work)
        out = cb.to_pdf(src, work / (path.stem + ".pdf"))
    elif suf == ".pdf":
        try:
            out = cb.crop_pdf(path, work / (path.stem + ".crop.pdf"))
        except cb.ScannedPdf as e:
            raise cb.CalibreError(f"扫描型 PDF 不做定稿（{e}）；建议 --target koreader 用 KOPT 重排") from None
    else:
        return [path]
    _gate(out, args)
    return [out]


def run(args, ctx) -> int:
    target = args.target or ctx.config.default_target
    quality = args.quality or ctx.config.quality
    calibre = cb.has_calibre()
    rc = 0
    work = cb.workdir()
    for path in args.files:
        if not path.is_file():
            print(f"✗ {path}: 不是文件")
            rc = 1
            continue
        route = decide_route(quality, target, path, calibre)
        print(f"→ {path.name}  target={target}  route={route}")
        if args.dry_run:
            continue
        try:
            outs = host_prepare(target, path, args, work) if route == "host" else [path]
        except cb.CalibreError as e:
            print(f"✗ {path.name}: {e}")
            rc = 1
            continue
        final: list[Path] = []
        for o in outs:
            if not args.no_split and target != "koreader" and pdfsplit.needs_split(o, ctx.config.split_pdf_mb):
                parts = pdfsplit.split(o, ctx.config.split_pdf_mb, work)
                if len(parts) > 1:
                    print(f"  分卷 {len(parts)} 份（>{ctx.config.split_pdf_mb}MB）")
                final.extend(parts)
            else:
                final.append(o)
        for o in final:
            if target == "koreader":
                url, q = "/api/koreader/books", {"folder": args.folder}
            else:
                url, q = "/api/books", {"target": target, "folder": args.folder, "optimize": "off" if args.no_optimize else "auto"}
            try:
                res = ctx.transport.post_files(url, [o], q)
            except Exception as e:  # noqa: BLE001
                print(f"✗ {o.name}: {e}")
                rc = 1
                continue
            for it in res.get("items", [res]):
                mark = "✓" if it.get("ok") else "✗"
                print(f"{mark} {it.get('file', o.name)}: {it.get('message', '')}")
                if not it.get("ok"):
                    rc = 1
    return rc
