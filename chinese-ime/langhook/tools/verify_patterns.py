#!/usr/bin/env python3
"""特征码对拍：解析 hook_init.c 里的 PROLOGUE_* 特征码（+可选 _MASK），
在给定 xochitl 二进制里做带掩码搜索，报告命中次数。

唯一命中(=1) → 该 hook 在此固件版本兼容；0 或 >1 → 失配，_xovi_shouldLoad/
cj_resolve_target 会跳过该 hook（甚至整个扩展不加载）。

掩码语义（同 pattern.h）：mask[i]==0 通配该字节，!=0 精确匹配。无 _MASK = 全精确。

用法: verify_patterns.py <hook_init.c> <xochitl.bin>
"""
import re
import sys


def parse_arrays(src: str) -> dict[str, bytes]:
    """提取所有 `static const uint8_t NAME[] = { 0x.., ... };` → {NAME: bytes}."""
    out: dict[str, bytes] = {}
    pat = re.compile(
        r"static\s+const\s+uint8_t\s+(\w+)\s*\[\]\s*=\s*\{(.*?)\}\s*;",
        re.DOTALL,
    )
    for m in pat.finditer(src):
        name = m.group(1)
        body = m.group(2)
        # 剥离注释：否则注释里的 0xNN（如掩码数组的 "+0x14 一条 BL 通配"）会被
        # 当成一个字节混入，使整个数组错位。
        body = re.sub(r"/\*.*?\*/", "", body, flags=re.DOTALL)
        body = re.sub(r"//[^\n]*", "", body)
        hexes = re.findall(r"0x([0-9A-Fa-f]{2})", body)
        if hexes:
            out[name] = bytes(int(h, 16) for h in hexes)
    return out


def count_matches(hay: bytes, pat: bytes, mask: bytes | None) -> int:
    n = 0
    plen = len(pat)
    end = len(hay) - plen
    j = 0
    while j <= end:
        ok = True
        for i in range(plen):
            if mask is not None and mask[i] == 0:
                continue
            if hay[j + i] != pat[i]:
                ok = False
                break
        if ok:
            n += 1
        j += 1
    return n


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    src = open(sys.argv[1], encoding="utf-8", errors="replace").read()
    hay = open(sys.argv[2], "rb").read()
    arrays = parse_arrays(src)

    prologues = {k: v for k, v in arrays.items()
                 if k.startswith("PROLOGUE_") and not k.endswith("_MASK")}
    print(f"# 二进制: {sys.argv[2]}  ({len(hay)} bytes)")
    print(f"# 特征码: {len(prologues)} 个\n")
    print(f"{'特征码':<34}{'长度':>4}{'掩码':>5}{'命中':>5}  判定")
    print("-" * 62)
    bad = 0
    for name in sorted(prologues):
        pat = prologues[name]
        mask = arrays.get(name + "_MASK")
        if mask is not None and len(mask) != len(pat):
            mask = mask + b"\x00" * (len(pat) - len(mask)) if len(mask) < len(pat) else mask[:len(pat)]
        hits = count_matches(hay, pat, mask)
        verdict = "OK(唯一)" if hits == 1 else ("!! 0命中(失配)" if hits == 0 else f"!! {hits}命中(多义)")
        if hits != 1:
            bad += 1
        print(f"{name:<34}{len(pat):>4}{('有' if mask else '无'):>5}{hits:>5}  {verdict}")
    print("-" * 62)
    if bad == 0:
        print("✅ 全部唯一命中 —— 现有 hook 在此固件兼容")
    else:
        print(f"⚠️  {bad} 个特征码失配 —— 部署前必须先修，否则对应功能失效")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
