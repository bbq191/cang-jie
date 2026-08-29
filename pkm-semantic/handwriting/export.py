#!/usr/bin/env python3
"""B1 手写笔记 → 待校对 Markdown → PKM 库（host 侧，零设备风险）。

一条命令：一本手写笔记本 → 每页「原生缩略图 + vision 草稿 + ⚠待校对」→ 写进 Obsidian 式
vault。定位是**辅助转写 + 人工校对**（de-risk 定论：工整≈100%/快写~60%，不做无人值守）。

管线：
  取数据  从设备 scp 该本 .rm/缩略图/元数据（或 --src 用本地镜像）——切页/退出后才落盘
  取图    首选 xochitl 原生缩略图 <uuid>.thumbnails/<page>.png（de-risk 胜出）；缺则 hw_render 兜底
  识别    vision.transcribe（默认 gemini，四家可 --provider 切；--no-vision 只出图不识别）
  产出    vault/<书名>.md（front-matter + 每页图内嵌 + 草稿 + 待校对标记）+ 图进 attachments/

用法：
  export.py --name 笔记本                       # 从默认设备拉「笔记本」这本，默认 gemini 识别
  export.py --doc <uuid> --provider deepseek    # 指定 UUID + 换后端
  export.py --src <本地镜像> --doc <uuid>       # 不碰设备，跑已拉好的镜像
  export.py --name 笔记本 --no-vision           # 只出带图 bundle 供纯手填（不调 API）
依赖：requests 不需要（urllib）；识别需对应后端 key；兜底渲染需 rmscene+pillow。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "proto"))  # 复用 hw_render / rm_strokes

import vision  # noqa: E402  同目录

DEVICE_XOCHITL = "/home/root/.local/share/remarkable/xochitl"


def _cache_root() -> str:
    base = os.environ.get("XDG_CACHE_HOME") or os.path.join(os.path.expanduser("~"), ".cache")
    return os.path.join(base, "cangjie-handwriting")


def _safe(name: str) -> str:
    return re.sub(r"[^\w一-鿿 .()-]", "_", name).strip() or "untitled"


# ── 设备/镜像取数据 ──────────────────────────────────────────────────────────
def pull_from_device(device: str, doc: str) -> str:
    """scp 该文档的 .metadata/.content/.thumbnails/ 到本地缓存镜像；返回镜像根目录。"""
    dst = os.path.join(_cache_root(), device)
    os.makedirs(dst, exist_ok=True)
    remote = f"root@{device}:{DEVICE_XOCHITL}"
    items = [f"{doc}.metadata", f"{doc}.content", f"{doc}.thumbnails"]
    for it in items:
        src = f"{remote}/{it}"
        r = subprocess.run(["scp", "-r", "-o", "ConnectTimeout=8", src, dst + "/"],
                           capture_output=True, text=True)
        if r.returncode != 0 and it.endswith(".content"):
            pass  # .content 可缺（页序退化到文件名排序）
        elif r.returncode != 0 and it.endswith(".metadata"):
            raise SystemExit(f"!! 拉 {it} 失败（文档不存在或设备连不上）：{r.stderr.strip()}")
    return dst


def resolve_doc_by_name(src: str, name: str, device: str | None) -> str:
    """按 visibleName 找文档 UUID。src 缺 metadata 时到设备上 grep。"""
    # 本地镜像优先
    for f in _list_metadata(src):
        if _visible_name(f) == name:
            return os.path.basename(f)[:-len(".metadata")]
    if device:  # 本地没有 → 到设备上找
        r = subprocess.run(
            ["ssh", "-o", "ConnectTimeout=8", f"root@{device}",
             f"grep -l '\"visibleName\": \"{name}\"' {DEVICE_XOCHITL}/*.metadata"],
            capture_output=True, text=True)
        hit = r.stdout.strip().splitlines()
        if hit:
            return os.path.basename(hit[0])[:-len(".metadata")]
    raise SystemExit(f"!! 找不到名为「{name}」的文档（本地镜像与设备都没有）")


def _list_metadata(src: str) -> list[str]:
    if not os.path.isdir(src):
        return []
    return [os.path.join(src, f) for f in os.listdir(src) if f.endswith(".metadata")]


def _visible_name(meta_path: str) -> str:
    try:
        with open(meta_path, encoding="utf-8") as f:
            return json.load(f).get("visibleName", "")
    except (OSError, json.JSONDecodeError):
        return ""


# ── 页序 ────────────────────────────────────────────────────────────────────
def page_order(src: str, doc: str) -> list[str]:
    """返回页 UUID 列表。优先读 .content（cPages.pages / pages）；退化到缩略图文件名排序。"""
    content = os.path.join(src, f"{doc}.content")
    pages: list[str] = []
    if os.path.isfile(content):
        try:
            with open(content, encoding="utf-8") as f:
                c = json.load(f)
            cp = c.get("cPages", {}).get("pages")
            if isinstance(cp, list):
                pages = [p["id"] for p in cp if isinstance(p, dict) and "id" in p
                         and not p.get("deleted")]
            elif isinstance(c.get("pages"), list):
                pages = list(c["pages"])
        except (OSError, json.JSONDecodeError, KeyError, TypeError):
            pages = []
    if not pages:  # 退化：按缩略图文件名
        thumbs = os.path.join(src, f"{doc}.thumbnails")
        if os.path.isdir(thumbs):
            pages = sorted(p[:-4] for p in os.listdir(thumbs) if p.endswith(".png"))
    return pages


# ── 取每页图（缩略图优先，hw_render 兜底）────────────────────────────────────
def page_image(src: str, doc: str, page: str, workdir: str) -> str | None:
    thumb = os.path.join(src, f"{doc}.thumbnails", f"{page}.png")
    if os.path.isfile(thumb):
        return thumb
    rm = os.path.join(src, doc, f"{page}.rm")  # 兜底：反解渲染（需镜像含 .rm）
    if os.path.isfile(rm):
        try:
            import hw_render  # 复用 proto/
            out = os.path.join(workdir, f"{page}.png")
            return hw_render.render(rm, out)
        except Exception as e:  # noqa: BLE001  渲染失败不致命，跳过该页图
            print(f"  · 页 {page} 兜底渲染失败：{e}", file=sys.stderr)
    return None


# ── 组待校对 Markdown ───────────────────────────────────────────────────────
def build_markdown(name: str, doc: str, pages: list[dict], provider: str | None) -> str:
    lines = [
        "---",
        f"title: {name}",
        "source: reMarkable 手写笔记",
        f"doc_uuid: {doc}",
        f"recognizer: {provider or 'none (待手填)'}",
        "status: 待校对",
        "---",
        "",
        f"# {name}",
        "",
        "> ⚠ **待校对**：以下为手写自动转写草稿（工整≈满分/快写约六成，快写行常有整段失真）。",
        "> 请对照每页图片改正错字，校完把 front-matter 的 `status` 改成 `已校对`。",
        "",
    ]
    for i, pg in enumerate(pages, 1):
        lines.append(f"## 第 {i} 页")
        lines.append("")
        if pg.get("img_rel"):
            lines.append(f"![第{i}页手写]({pg['img_rel']})")
            lines.append("")
        draft = pg.get("draft")
        if not pg.get("img_rel"):
            lines.append("<!-- 本页无图（缩略图缺 + 无 .rm 兜底），请手填 -->")
        elif draft is None:
            lines.append("<!-- 未自动识别（--no-vision），请对照上图手填 -->")
        elif draft == "":
            lines.append("<!-- 识别为空/失败，请对照上图手填 -->")
        else:
            lines.append(draft)
        lines.append("")
    return "\n".join(lines)


# ── 主流程 ──────────────────────────────────────────────────────────────────
def main() -> int:
    ap = argparse.ArgumentParser(description="B1 手写笔记 → 待校对 Markdown → PKM 库")
    ap.add_argument("--doc", help="文档 UUID")
    ap.add_argument("--name", help="按笔记本可见名找（与 --doc 二选一）")
    ap.add_argument("--src", help="本地 xochitl 镜像目录（给了就不碰设备）")
    ap.add_argument("--device", default="10.11.99.1", help="设备 IP（默认 10.11.99.1）")
    ap.add_argument("--provider", default="gemini",
                    choices=list(vision.PROVIDERS), help="vision 后端（默认 gemini）")
    ap.add_argument("--model", help="覆盖后端默认模型")
    ap.add_argument("--no-vision", action="store_true", help="只出带图 bundle，不调 API 识别")
    ap.add_argument("--out", default=os.path.join(os.getcwd(), "handwriting-vault"),
                    help="输出 vault 目录（默认 ./handwriting-vault）")
    args = ap.parse_args()

    if not args.doc and not args.name:
        ap.error("需 --doc 或 --name 之一")

    # 1. 定位 + 取数据
    if args.src:
        src = args.src
        device = None
    else:
        device = args.device
        # 先按名/UUID 定位，再拉
    if args.name and not args.doc:
        probe_src = args.src or ""
        args.doc = resolve_doc_by_name(probe_src, args.name, device)
        print(f"-- 「{args.name}」→ doc={args.doc}")
    if not args.src:
        print(f"-- 从设备 {device} 拉 doc={args.doc} …（切页/退出后才落盘）")
        src = pull_from_device(device, args.doc)

    name = None
    for f in _list_metadata(src):
        if os.path.basename(f) == f"{args.doc}.metadata":
            name = _visible_name(f)
    name = name or args.name or args.doc

    pages = page_order(src, args.doc)
    if not pages:
        raise SystemExit("!! 该文档没有可处理的页（无 .content 且无缩略图）")
    print(f"-- {name}：{len(pages)} 页")

    # 2. vault + attachments
    vault = args.out
    att = os.path.join(vault, "attachments")
    os.makedirs(att, exist_ok=True)
    workdir = os.path.join(_cache_root(), "render", args.doc)
    os.makedirs(workdir, exist_ok=True)

    # 3. 逐页：取图 → 识别
    page_recs: list[dict] = []
    for i, page in enumerate(pages, 1):
        img = page_image(src, args.doc, page, workdir)
        rec: dict = {"page": page}
        if img:
            dst_name = f"{args.doc}-{page}.png"
            dst = os.path.join(att, dst_name)
            if os.path.abspath(img) != os.path.abspath(dst):
                with open(img, "rb") as a, open(dst, "wb") as b:
                    b.write(a.read())
            rec["img_rel"] = f"attachments/{dst_name}"
            if not args.no_vision:
                try:
                    print(f"  · 第 {i}/{len(pages)} 页 识别中（{args.provider}）…")
                    rec["draft"] = vision.transcribe(dst, args.provider, args.model)
                except vision.VisionError as e:
                    print(f"    ✗ 识别失败：{e}", file=sys.stderr)
                    rec["draft"] = ""
        else:
            print(f"  · 第 {i} 页 无图（缩略图缺 + 无 .rm 兜底）")
        page_recs.append(rec)

    # 4. 写 Markdown
    md = build_markdown(name, args.doc, page_recs,
                        None if args.no_vision else args.provider)
    out_md = os.path.join(vault, _safe(name) + ".md")
    with open(out_md, "w", encoding="utf-8") as f:
        f.write(md)

    done = sum(1 for r in page_recs if r.get("draft"))
    print(f"✅ 写出 {out_md}")
    print(f"   {len(pages)} 页，{done} 页有草稿，图在 {att}/")
    print("   下一步：Obsidian 打开 vault，对照图改错字，改完把 status 置「已校对」。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
