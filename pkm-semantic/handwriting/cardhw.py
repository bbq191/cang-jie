#!/usr/bin/env python3
"""B1 · 卡片手写批注 → 转写 → 注入对应槽（模式A：替换成干净文字，手写被消化）。

给 PKM「总结卡片」加手写批注的落地：在某个槽（🟡金句/🔵洞见/🩷疑问/🟠主题/🟢可复用/⚪人物）
旁边手写想法 → 本工具整页喂 vision 做**空间关联**（手写贴哪个槽 de-risk 验证 4/4 准）→ 把转写
文字作为"打字批注"注入该槽下 → 重建为纯文本页（手写消化掉）。转写行搭 `cardsync::parse_card`
的保留通道（★块下非★行=逐字保留），**下次画星重建不丢**。

安全：默认 **dry-run** 只报关联方案；`--apply` 才写，且写前**备份原 .rm**。模式A 会去掉手写，
故备份+dry-run+可打字改字三重兜底（对齐"辅助转写+人工校对"定位）。

用法：
  cardhw.py --book 人骨拼图                  # dry-run：拉卡片手写页，报「哪段手写→哪个槽」
  cardhw.py --book 人骨拼图 --apply          # 备份→注入→写回设备
  cardhw.py --doc <uuid> --provider deepseek --apply
  cardhw.py --src <本地镜像> --doc <uuid>    # 不碰设备跑镜像
依赖：uv run --with rmscene python cardhw.py ...（识别需对应后端 key）
"""
from __future__ import annotations

import argparse
import io
import logging
import os
import subprocess
import sys

logging.getLogger("rmscene").setLevel(logging.ERROR)
from rmscene import read_blocks, simple_text_document, write_blocks  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import vision  # noqa: E402

DEVICE_XOCHITL = "/home/root/.local/share/remarkable/xochitl"
# 槽头关键词 → 匹配 vision 输出的 slot；也是「块边界」判定用（含终止符 🔗/★）。
SLOT_KEYWORDS = ["金句", "洞见", "疑问", "主题", "可复用", "人物"]
SLOT_EMOJI = "🟡🔵🩷🟠🟢⚪"
TERMINATORS = ("🔗", "★")
INJECT_PREFIX = "   · ✍ "  # 缩进 bullet + 笔标记，和书摘 bullet 区分、仍被 parse_card 当批注保留


# ── .rm 读 ──────────────────────────────────────────────────────────────────
def read_root_text(rm: bytes) -> str | None:
    for b in read_blocks(io.BytesIO(rm)):
        if type(b).__name__ == "RootTextBlock":
            return "".join(i.value for i in b.value.items.sequence_items()
                           if isinstance(i.value, str))
    return None


def stroke_count(rm: bytes) -> int:
    n = 0
    for b in read_blocks(io.BytesIO(rm)):
        if type(b).__name__ == "SceneLineItemBlock":
            v = getattr(getattr(b, "item", None), "value", None)
            if v is not None and len(getattr(v, "points", [])) >= 2:
                n += 1
    return n


def rebuild_text_rm(text: str) -> bytes:
    buf = io.BytesIO()
    write_blocks(buf, simple_text_document(text))
    return buf.getvalue()


# ── 槽定位 + 注入 ────────────────────────────────────────────────────────────
def _header_keyword(line: str) -> str | None:
    """该行是不是槽头/终止符：是槽头返回其关键词；是 🔗/★ 终止符返回 ''；否则 None。"""
    s = line.strip()
    if any(s.startswith(t) for t in TERMINATORS):
        return ""
    for kw in SLOT_KEYWORDS:
        if kw in s and (s[:1] in SLOT_EMOJI or "：" in s or ":" in s):
            return kw
    return None


def _match_slot(vision_slot: str) -> str | None:
    for kw in SLOT_KEYWORDS:
        if kw in vision_slot or vision_slot in kw:
            return kw
    return None


def inject(text: str, annotations: list[dict]) -> tuple[str, list[str], list[dict]]:
    """把每条 {slot,note} 注入对应槽块尾。返回 (新文本, 应用日志, 未匹配项)。"""
    by_kw: dict[str, list[str]] = {}
    unmatched: list[dict] = []
    for a in annotations:
        kw = _match_slot(a["slot"])
        if kw:
            by_kw.setdefault(kw, []).append(a["note"])
        else:
            unmatched.append(a)

    out: list[str] = []
    applied: list[str] = []
    cur_kw: str | None = None

    def flush():
        nonlocal cur_kw
        if cur_kw and by_kw.get(cur_kw):
            for note in by_kw[cur_kw]:
                out.append(INJECT_PREFIX + note)
                applied.append(f"{cur_kw} ← {note}")
            by_kw[cur_kw] = []

    for line in text.split("\n"):
        hk = _header_keyword(line)
        if hk is not None:      # 遇到新槽头/终止符：先把上一槽的注入落下
            flush()
            cur_kw = hk or None
        out.append(line)
    flush()  # 收尾（末槽）

    # 有些槽 vision 报了但正文里没找到该槽头 → 归入未匹配
    for kw, notes in by_kw.items():
        for n in notes:
            unmatched.append({"slot": kw, "note": n})
    return "\n".join(out), applied, unmatched


# ── 设备/镜像 ────────────────────────────────────────────────────────────────
def _list_cards(src_lister, book: str) -> list[str]:
    """返回 visibleName 含 book 且含『总结卡片』的 doc uuid 列表。"""
    return [u for u, vn in src_lister() if book in vn and "总结卡片" in vn]


def device_metadata_list(device: str):
    r = subprocess.run(
        ["ssh", "-o", "ConnectTimeout=8", f"root@{device}",
         f'for m in {DEVICE_XOCHITL}/*.metadata; do '
         f'vn=$(grep -o \'"visibleName": *"[^"]*"\' "$m" | sed \'s/.*: *"//;s/"//\'); '
         f'echo "$(basename "$m" .metadata)|$vn"; done'],
        capture_output=True, text=True)
    for ln in r.stdout.splitlines():
        if "|" in ln:
            u, vn = ln.split("|", 1)
            yield u, vn


def local_metadata_list(src: str):
    import json
    for f in os.listdir(src):
        if f.endswith(".metadata"):
            try:
                with open(os.path.join(src, f), encoding="utf-8") as fp:
                    vn = json.load(fp).get("visibleName", "")
            except (OSError, ValueError):
                vn = ""
            yield f[:-len(".metadata")], vn


def device_page_rm(device: str, doc: str, page: str, dst: str):
    subprocess.run(["scp", "-o", "ConnectTimeout=8",
                    f"root@{device}:{DEVICE_XOCHITL}/{doc}/{page}.rm", dst],
                   check=True, capture_output=True, text=True)


def device_pages(device: str, doc: str) -> list[str]:
    r = subprocess.run(["ssh", "-o", "ConnectTimeout=8", f"root@{device}",
                        f"ls {DEVICE_XOCHITL}/{doc}/*.rm 2>/dev/null"],
                       capture_output=True, text=True)
    return [os.path.basename(p)[:-3] for p in r.stdout.split()]


def device_thumb(device: str, doc: str, page: str, dst: str) -> str | None:
    r = subprocess.run(["scp", "-o", "ConnectTimeout=8",
                        f"root@{device}:{DEVICE_XOCHITL}/{doc}.thumbnails/{page}.png", dst],
                       capture_output=True, text=True)
    return dst if r.returncode == 0 else None


# ── 主流程 ──────────────────────────────────────────────────────────────────
def main() -> int:
    ap = argparse.ArgumentParser(description="卡片手写批注 → 转写 → 注入对应槽（模式A）")
    ap.add_argument("--book", help="书名（匹配『《书》- 总结卡片』）")
    ap.add_argument("--doc", help="卡片文档 UUID（跳过按名查找）")
    ap.add_argument("--src", help="本地 xochitl 镜像（给了就不碰设备）")
    ap.add_argument("--device", default="10.11.99.1")
    ap.add_argument("--provider", default="gemini", choices=list(vision.PROVIDERS))
    ap.add_argument("--model")
    ap.add_argument("--apply", action="store_true", help="真写回（默认 dry-run 只报方案）")
    args = ap.parse_args()
    if not args.book and not args.doc:
        ap.error("需 --book 或 --doc")

    lister = (lambda: local_metadata_list(args.src)) if args.src \
        else (lambda: device_metadata_list(args.device))

    # 1. 定位卡片文档（可能有重名，按"含手写页"消歧）
    docs = [args.doc] if args.doc else _list_cards(lister, args.book)
    if not docs:
        raise SystemExit(f"!! 找不到『{args.book}』的总结卡片")

    workdir = "/tmp/cangjie-cardhw"
    os.makedirs(workdir, exist_ok=True)
    hits = []  # (doc, page, rm_bytes, thumb_path)
    for doc in docs:
        pages = (sorted(os.path.basename(p)[:-3] for p in
                        os.listdir(os.path.join(args.src, doc)) if p.endswith(".rm"))
                 if args.src else device_pages(args.device, doc))
        for pg in pages:
            if args.src:
                rm = open(os.path.join(args.src, doc, f"{pg}.rm"), "rb").read()
            else:
                dst = os.path.join(workdir, f"{doc}-{pg}.rm")
                device_page_rm(args.device, doc, pg, dst)
                rm = open(dst, "rb").read()
            if stroke_count(rm) > 0 and read_root_text(rm):  # 混排页=有手写+有卡片文本
                thumb = os.path.join(workdir, f"{doc}-{pg}.png")
                if args.src:
                    tp = os.path.join(args.src, f"{doc}.thumbnails", f"{pg}.png")
                    thumb = tp if os.path.isfile(tp) else None
                else:
                    thumb = device_thumb(args.device, doc, pg, thumb)
                hits.append((doc, pg, rm, thumb))

    if not hits:
        raise SystemExit("!! 这些卡片里没有『既有手写又有卡片文本』的页——先在某个槽旁手写再退出书库")
    if len(docs) > 1:
        print(f"⚠ 有 {len(docs)} 本重名总结卡片，仅处理含手写页的 {len({d for d,_,_,_ in hits})} 本")

    # 2. 逐页：vision 空间关联 → 注入方案
    total_apply = 0
    for doc, pg, rm, thumb in hits:
        print(f"\n══ 卡片 {doc} 页 {pg} ══")
        if not thumb:
            print("  · 无缩略图，跳过（模式A 需整页图做空间关联）")
            continue
        try:
            anns = vision.transcribe_card(thumb, args.provider, args.model)
        except vision.VisionError as e:
            print(f"  ✗ 识别失败：{e}")
            continue
        if not anns:
            print("  · vision 没认出手写批注")
            continue
        text = read_root_text(rm)
        new_text, applied, unmatched = inject(text, anns)
        print("  关联方案（手写 → 槽）：")
        for a in applied:
            print(f"    ✓ {a}")
        for u in unmatched:
            print(f"    ✗ 未匹配槽：{u['slot']} ← {u['note']}（跳过）")
        if not applied:
            continue

        if not args.apply:
            print("  [dry-run] 未写。确认无误加 --apply 写回（写前自动备份原 .rm）。")
            continue

        # 3. 写回（备份 → 重建纯文本页 → scp）
        new_rm = rebuild_text_rm(new_text)
        bak = os.path.join(workdir, f"{doc}-{pg}.rm.bak")
        open(bak, "wb").write(rm)
        if args.src:
            open(os.path.join(args.src, doc, f"{pg}.rm"), "wb").write(new_rm)
        else:
            local_new = os.path.join(workdir, f"{doc}-{pg}.new.rm")
            open(local_new, "wb").write(new_rm)
            subprocess.run(["scp", "-o", "ConnectTimeout=8", local_new,
                            f"root@{args.device}:{DEVICE_XOCHITL}/{doc}/{pg}.rm"],
                           check=True, capture_output=True, text=True)
        total_apply += len(applied)
        print(f"  ✅ 已写回 {len(applied)} 条（原稿备份 {bak}）")

    if args.apply and total_apply:
        print(f"\n✅ 共注入 {total_apply} 条。重开该卡片笔记本即见（手写已消化为文字）。")
    elif not args.apply:
        print("\n（dry-run 完。加 --apply 真写回。）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
