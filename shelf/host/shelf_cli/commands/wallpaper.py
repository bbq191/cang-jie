"""`shelf wallpaper add|ls|set|mode|rm`：休眠壁纸上传即用。"""
from pathlib import Path
from urllib.parse import quote

NAME = "wallpaper"
HELP = "壁纸：add <img...> [--activate] | ls | set <name> | mode sequential|random|fixed | rm <name>"


def add_args(p):
    sub = p.add_subparsers(dest="op", required=True)
    a = sub.add_parser("add")
    a.add_argument("files", nargs="+", type=Path)
    a.add_argument("--activate", action="store_true", help="上传后立即设为当前")
    sub.add_parser("ls")
    s = sub.add_parser("set")
    s.add_argument("name")
    m = sub.add_parser("mode")
    m.add_argument("mode", choices=["sequential", "random", "fixed"])
    r = sub.add_parser("rm")
    r.add_argument("name")


def run(args, ctx) -> int:
    t = ctx.transport
    if args.op == "ls":
        d = t.get("/api/wallpapers")
        print(f"轮换模式：{d.get('mode')}   当前：{d.get('current') or '（无）'}")
        for it in d.get("items", []):
            cur = "*" if (it.get("extra") or {}).get("current") else " "
            print(f"{cur} {it['name']}  {it.get('bytes', 0) // 1024} KB")
        return 0
    if args.op == "set":
        d = t.put_json("/api/wallpapers/current", {"name": args.name})
        print(f"已设为当前：{args.name}（下次休眠即显示；bind {d.get('mounted')}/4）")
        return 0
    if args.op == "mode":
        t.put_json("/api/wallpapers/mode", {"mode": args.mode})
        print(f"轮换模式：{args.mode}")
        return 0
    if args.op == "rm":
        t.delete(f"/api/wallpapers/{quote(args.name)}")
        print(f"已删除 {args.name}")
        return 0
    rc = 0
    for i, f in enumerate(args.files):
        if not f.is_file():
            print(f"✗ {f}: 不是文件")
            rc = 1
            continue
        q = {"activate": "1"} if (args.activate and i == 0) else None
        d = t.post_files("/api/wallpapers", [f], q)
        for it in d.get("items", []):
            print(f"{'✓' if it['ok'] else '✗'} {it['name']}: {it['message']}")
            rc |= 0 if it["ok"] else 1
        if d.get("activated"):
            print(f"  已激活 {d['activated']}")
    return rc
