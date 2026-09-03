"""`shelf font add|ls|rm`：字体上传即装（原生 fontconfig + KOReader 镜像）。"""
from pathlib import Path
from urllib.parse import quote

NAME = "font"
HELP = "字体：add <ttf/otf...> | ls | rm <家族名>（删该家族全部文件）"


def add_args(p):
    sub = p.add_subparsers(dest="op", required=True)
    a = sub.add_parser("add", help="上传并安装")
    a.add_argument("files", nargs="+", type=Path)
    a.add_argument("--no-koreader", action="store_true", help="不镜像到 KOReader（服务端配置为准，此处仅提示）")
    sub.add_parser("ls", help="列出")
    r = sub.add_parser("rm", help="按家族名删除（全部字重文件）")
    r.add_argument("file", metavar="family")


def run(args, ctx) -> int:
    t = ctx.transport
    if args.op == "ls":
        d = t.get("/api/fonts")
        for it in d.get("items", []):
            ex = it.get("extra") or {}
            cn = (ex.get("names") or {}).get("cn", "")
            print(f"{it['name']:<34} {cn if cn != it['name'] else '':<20} {len(ex.get('files') or []):>2} 文件 {'⚠界面回退' if ex.get('fontconfigRef') else ''}")
        return 0
    if args.op == "rm":
        t.delete(f"/api/fonts/{quote(args.file)}")
        print(f"已删除 {args.file}")
        return 0
    rc = 0
    for f in args.files:
        if not f.is_file():
            print(f"✗ {f}: 不是文件")
            rc = 1
            continue
        d = t.post_files("/api/fonts", [f])
        for it in d.get("items", []):
            ex = (it.get("item") or {}).get("extra") or {}
            print(f"{'✓' if it['ok'] else '✗'} {it['name']}: {it['message']}" + (f"  家族={ex.get('family')}  KOReader={ex.get('koreader', '-')}" if it["ok"] else ""))
            rc |= 0 if it["ok"] else 1
    if rc == 0:
        print("阅读器字体菜单重开后可选（若不可见需重启 xochitl，见真机记录）")
    return rc
