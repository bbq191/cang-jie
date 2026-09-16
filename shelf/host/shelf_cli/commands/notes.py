"""`shelf notes pull`：把笔记线（notes/）已经导出的 md 拉到本机 Obsidian vault 目录。

笔记线三期规划里明确留的"另一半"（设备端落盘已经做了，见 `notes/services/note-serve/src/export.rs`）——
流程：对每本书先 `POST /export`（刷新落盘内容，指纹没变的章节服务端自己会跳过重写，免费的增量），
再 `GET /vault.json` 把服务端已经落盘的文件读回来（文件名/目录名都是服务端算好的，这边直接落盘，
不重复实现一遍 sanitize 规则）。"""
from __future__ import annotations

from pathlib import Path

NAME = "notes"
HELP = "笔记：pull（拉 md 导出到本机 Obsidian vault）"


def add_args(p):
    sub = p.add_subparsers(dest="op", required=True)
    pl = sub.add_parser("pull", help="拉全部书的 md 导出到本机目录")
    pl.add_argument("--out", type=Path, help="本机 Obsidian vault 路径（缺省 $XDG_DATA_HOME/shelf/notes-vault）")


def run(args, ctx) -> int:
    t = ctx.transport
    if args.op != "pull":
        return 1
    out = args.out or (ctx.paths.data / "notes-vault")
    out.mkdir(parents=True, exist_ok=True)
    books = t.get("/api/notes/books").get("items", [])
    if not books:
        print("没有书")
        return 0
    total_files = 0
    rc = 0
    for b in books:
        uuid, title = b["uuid"], b.get("title") or uuid
        try:
            t.post_text(f"/api/notes/books/{uuid}/export", b"")
            m = t.get(f"/api/notes/books/{uuid}/vault.json")
        except Exception as e:  # noqa: BLE001
            print(f"✗ {title}: {e}")
            rc = 1
            continue
        files = m.get("files", [])
        if not files:
            print(f"- {title}: 没有已导出内容，跳过")
            continue
        book_dir = out / m["dir"]
        book_dir.mkdir(parents=True, exist_ok=True)
        for f in files:
            (book_dir / f["name"]).write_text(f["content"], encoding="utf-8")
        total_files += len(files)
        print(f"✓ {title}: {len(files)} 个文件 → {book_dir}")
    print(f"共 {total_files} 个文件，vault：{out}")
    return rc
