"""扫描 xochitl 镜像目录 → ★ 全局待办清单（Markdown + JSON）。

镜像结构（由 pull 从设备拉取，与 rm-export/export.py 一致）：
    <UUID>.metadata  visibleName
    <UUID>.content   页顺序（cPages.pages[].id 或旧版 pages[]）
    <UUID>/<page>.rm 每页笔划

只读。输出独立文件，绝不回写设备（inplace 判死 + 云同步冲突血泪）。
后续 Rust daemon 会用同样逻辑，把结果组装成设备上可跳转的 Global_Todo 文档。
"""
from __future__ import annotations

import argparse
import glob
import json
import os

import star_detect as sd
from geometry import bbox as g_bbox
from rm_strokes import read_strokes


def _load_json(p: str) -> dict:
    try:
        with open(p, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return {}


def _page_order(content: dict) -> list[str]:
    """从 .content 取页顺序（返回 page-uuid 列表）。

    兼容三种：cPages.pages[]（新）、顶层 pages[]（真机也用，元素是 dict）、旧版 pages[] 为字符串。
    """
    cp = content.get("cPages")
    pages = cp.get("pages") if isinstance(cp, dict) else content.get("pages")
    if not isinstance(pages, list):
        return []
    out = []
    for p in pages:
        out.append(p.get("id", "") if isinstance(p, dict) else str(p))
    return out


def _cluster(strokes, gap: float):
    """把空间相邻的笔划聚成"一个标记"（真手绘星是多笔叠加）。

    并查集式合并：任意两笔 bbox 膨胀 gap 后相交即同组。返回 [[stroke,...], ...]。
    """
    def bb(s):
        return g_bbox(s.points)
    n = len(strokes)
    parent = list(range(n))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    boxes = [bb(s) for s in strokes]
    for i in range(n):
        for j in range(i + 1, n):
            ax0, ay0, ax1, ay1 = boxes[i]
            bx0, by0, bx1, by1 = boxes[j]
            if (ax0 - gap <= bx1 and bx0 - gap <= ax1 and
                    ay0 - gap <= by1 and by0 - gap <= ay1):
                parent[find(i)] = find(j)
    groups: dict[int, list] = {}
    for i, s in enumerate(strokes):
        groups.setdefault(find(i), []).append(s)
    return list(groups.values())


def scan_document(uuid: str, src: str, cfg: sd.StarConfig,
                  cluster_gap: float = 30.0) -> dict | None:
    meta = _load_json(os.path.join(src, uuid + ".metadata"))
    if meta.get("type") == "CollectionType":          # 文件夹，跳过
        return None
    if meta.get("parent") == "trash":                 # 回收站，跳过
        return None
    title = meta.get("visibleName", uuid[:8])
    order = _page_order(_load_json(os.path.join(src, uuid + ".content")))
    order_idx = {pid: i for i, pid in enumerate(order)}

    hits = []
    for rm in sorted(glob.glob(os.path.join(src, uuid, "*.rm"))):
        page_uuid = os.path.splitext(os.path.basename(rm))[0]
        strokes = read_strokes(rm)
        # 颜色门控：先筛到专用笔色（生产必设），黑色笔记文字被排除在合并/检测之外
        if cfg.todo_colors is not None:
            strokes = [s for s in strokes if s.color in cfg.todo_colors]
        n, colors = 0, set()
        # 空间合并多笔星 → 每个合并标记判一次
        for grp in _cluster(strokes, gap=cluster_gap):
            pts = [p for s in grp for p in s.points]
            grp_color = grp[0].color
            ok, _ = sd.is_star(pts, cfg, color=grp_color)
            if ok:
                n += 1
                colors.add(grp_color)
        if n:
            hits.append({
                "page_index": order_idx.get(page_uuid, -1),
                "page_uuid": page_uuid,
                "count": n,
                "colors": sorted(colors),
            })
    if not hits:
        return None
    hits.sort(key=lambda h: (h["page_index"] < 0, h["page_index"]))
    return {"uuid": uuid, "title": title, "hits": hits}


def scan_mirror(src: str, cfg: sd.StarConfig, cluster_gap: float = 30.0) -> list[dict]:
    out = []
    for m in sorted(glob.glob(os.path.join(src, "*.metadata"))):
        uuid = os.path.basename(m)[: -len(".metadata")]
        r = scan_document(uuid, src, cfg, cluster_gap)
        if r:
            out.append(r)
    return out


def render_markdown(results: list[dict]) -> str:
    total = sum(h["count"] for d in results for h in d["hits"])
    lines = ["# ★ 全局待办", "",
             f"> 共 {total} 处 · {len(results)} 本文档 · 只读扫描生成", ""]
    for d in results:
        lines.append(f"## {d['title']}")
        for h in d["hits"]:
            page = f"第 {h['page_index'] + 1} 页" if h["page_index"] >= 0 else "未知页"
            star = "★" * min(h["count"], 5)
            colors = "/".join(h["colors"])
            lines.append(f"- {star} {page}（{h['count']} 处 · {colors}）")
        lines.append("")
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser(description="扫描 .rm 手绘 ★ → 全局待办清单")
    ap.add_argument("--src", required=True, help="xochitl 镜像目录")
    ap.add_argument("--out", default="Global_Todo.md", help="Markdown 输出路径")
    ap.add_argument("--json", help="附带机器可读 JSON 输出路径")
    ap.add_argument("--todo-color", action="append",
                    help="专用笔色门控（可多次），如 RED BLUE。生产必设——"
                         "省略则黑对黑纯几何，实测每页约 8 个草书汉字误判为星")
    ap.add_argument("--cluster-gap", type=float, default=30.0,
                    help="多笔星合并的空间间距阈值（.rm 像素）")
    args = ap.parse_args()

    cfg = sd.StarConfig()
    if args.todo_color:
        cfg.todo_colors = {c.upper() for c in args.todo_color}
    else:
        print("[警告] 未设 --todo-color：黑对黑纯几何模式，笔记页会误判草书汉字为星。"
              "生产请指定专用笔色。", flush=True)

    results = scan_mirror(args.src, cfg, args.cluster_gap)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write(render_markdown(results))
    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(results, f, ensure_ascii=False, indent=2)
    total = sum(h["count"] for d in results for h in d["hits"])
    gate = f"（仅 {'/'.join(sorted(cfg.todo_colors))} 笔色）" if cfg.todo_colors else ""
    print(f"✅ {total} 处 ★ · {len(results)} 本文档{gate} → {args.out}")


if __name__ == "__main__":
    main()
