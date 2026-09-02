#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""build_dict.py —— 用户自备 Kindle 词典 MOBI → 排序 TSV（生词本查词用）。

【离线 host 端一次性构建，对设备零风险。词典数据是用户正版商业词典派生物，
  只个人自用、不入库、不分发；本脚本不含任何词典内容，仅读用户本地 MOBI。】

管线：MOBI --(calibre ebook-convert)--> htmlz --(解析)--> 排序 TSV。
  calibre 把 Kindle 词典转成干净 HTML：每条词条 = `<p><span class="bold">词条</span></p>释义<hr/>`
  （idx:entry 索引已被 calibre 消化）。按 <hr/> 切块、取首个 bold span 作词条、其后到块尾作释义。

输出行格式（与 Rust `pkm::dict` 一致）：
    headword \\t 音标/拼音 \\t 主释义(body) \\t 补充(extra=空) \\n
  按 headword 的 Unicode 码位序升序（= UTF-8 字节序 = Rust 二分的 cmp）。
  字段内的 \\ / TAB / 换行做转义（\\\\ / \\t / \\n），与 Rust `dict::unescape` 对应。
  key 规整与 Rust `dict::normalize_key` 对齐：en=小写+剥两端非字母数字；zh=去空白
  （现汉/牛津词条本就常规 CJK，Rust 侧的 canon 部首规整对它们是 no-op，故此处不复制那张表）。

用法：
    # 直接给 MOBI（自动调 calibre 转换，较慢，一次性）
    python3 build_dict.py --lang en --mobi "牛津高阶英汉双解第7版(现代版).mobi" --out en.tsv
    python3 build_dict.py --lang zh --mobi "现代汉语词典.mobi"                  --out zh.tsv
    # 或给已转好的 HTML（跳过 calibre，用于迭代解析）
    python3 build_dict.py --lang zh --html dbg/input/index.html --out zh.tsv
    # 默认目录批处理（~/Documents/ereader/books/字典/ 里那两本 → out_dir）
    python3 build_dict.py --auto --out-dir ./dict-build
"""
import argparse
import os
import re
import struct
import subprocess
import sys
import tempfile
import zipfile

# 默认用户词典位置（本地正版；派生物 gitignore，不入库）。
DEFAULT_DICT_DIR = os.path.expanduser("~/Documents/ereader/books/字典")
# EN：用完整版牛津（旧「英汉双解第7版(现代版)」那份下载残缺——只到字母 'pluck'，缺 q–z 约 22MB，
# 英文生词查不到；换用完整的「英语双解现代第七版」67.5MB）。
EN_MOBI = "牛津高阶英语双解现代第七版.mobi"
ZH_MOBI = "现代汉语词典.mobi"

TAG_RE = re.compile(r"<[^>]+>")
# 词头两种形态：calibre 转换产物 = <span class="bold">词</span>（正文任意处首现）；
# raw MOBI 直解产物 = 每个 <hr/> 块开头的 <b>词</b>（可能前置空 <a></a> 锚点）。
BOLD_RE = re.compile(r'<span class="bold"[^>]*>(.*?)</span>', re.S)
HEAD_B_RE = re.compile(
    r'^\s*(?:<a[^>]*>\s*</a>\s*)?(?:<p[^>]*>\s*)?(?:<a[^>]*>\s*</a>\s*)?<b>(.*?)</b>(.*)$', re.S
)  # 块首可被 <p width="0%"> 包裹（现汉 raw 形态，2026-09-02）
HR_SPLIT_RE = re.compile(r"<hr\s*/?>", re.I)
WS_RE = re.compile(r"\s+")
# 英文 IPA 音标：牛津正文里形如 /ˈkæʃeɪ/ 的斜杠对；取第一处作 phonetic。
IPA_RE = re.compile(r"/[^/\n]{1,40}/")


def strip_tags(s: str) -> str:
    """去 HTML 标签 + 折叠空白为单空格 + 解 HTML 实体（现汉 raw 全文是 &#xxxxx; 实体）。"""
    import html as _html
    return _html.unescape(WS_RE.sub(" ", TAG_RE.sub(" ", s)).strip())


def escape_field(s: str) -> str:
    """字段内转义（与 Rust dict::unescape 对应）。折叠空白后其实已无 TAB/换行，仅防御性处理。"""
    return s.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n")


def normalize_key(word: str, lang: str) -> str:
    """与 Rust dict::normalize_key 对齐。"""
    if lang == "en":
        w = word.strip()
        w = re.sub(r"^[^0-9A-Za-z]+", "", w)
        w = re.sub(r"[^0-9A-Za-z]+$", "", w)
        return w.lower()
    # zh：去所有空白（canon 部首规整对常规 CJK 词条为 no-op，略）
    return "".join(word.split())


def mobi_to_html(mobi_path: str) -> str:
    """calibre ebook-convert MOBI → htmlz → 解出 index.html 文本。慢（分钟级），一次性。"""
    with tempfile.TemporaryDirectory() as td:
        htmlz = os.path.join(td, "out.htmlz")
        print(f"[build_dict] ebook-convert 转换中（慢，请耐心）… {os.path.basename(mobi_path)}", file=sys.stderr)
        subprocess.run(
            ["ebook-convert", mobi_path, htmlz],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        with zipfile.ZipFile(htmlz) as z:
            # calibre 对大词典会把正文拆成多个 HTML（index.html + index_split_NNN.html…）。
            # **必须读全部并按名排序拼接**——只读 index.html 会丢掉后半字母表（实测只剩 a–p，
            # q–z 几乎全缺，英文生词查不到）。用自然序排序保证 split_000 < split_001 < …。
            names = sorted(
                n for n in z.namelist() if n.lower().endswith((".html", ".xhtml", ".htm"))
            )
            if not names:
                sys.exit("[build_dict] htmlz 里没有 HTML 文件")
            print(f"[build_dict] htmlz 含 {len(names)} 个 HTML，全部拼接解析", file=sys.stderr)
            return "\n".join(z.read(n).decode("utf-8", "replace") for n in names)


def mobi_to_html_raw(mobi_path: str) -> str:
    """**直接解 MOBI**（PalmDOC 记录 → 原始 HTML），不经 calibre——calibre 对大词典的 CSS flatten
    阶段会近乎卡死（牛津 165k+ 词条，单线程 O(n²)）。需 `mobi` 包（`uv add --dev mobi`）的解压器。
    直接按 PDB 记录偏移表解压全部文本记录：绕开 mobi 库 loadSection 对超大 MOBI 的 bug（把有效记录
    误读成空），也不碰它崩溃的词典 INFL 索引解析。词头形态 = 每个 <hr/> 块开头的 <b>词</b>。"""
    from mobi.mobi_uncompress import PalmdocReader, UncompressedReader

    raw = open(mobi_path, "rb").read()
    nrec = struct.unpack_from(">H", raw, 0x4C)[0]
    offs = [struct.unpack_from(">I", raw, 0x4E + i * 8)[0] for i in range(nrec)]

    def rec(i):
        lo = offs[i]
        hi = offs[i + 1] if i + 1 < nrec else len(raw)
        return raw[lo:hi] if 0 <= lo <= hi <= len(raw) else b""

    h = rec(0)
    comp = struct.unpack_from(">H", h, 0x00)[0]
    text_recs = struct.unpack_from(">H", h, 0x08)[0]
    flags = struct.unpack_from(">H", h, 0xF2)[0] if len(h) >= 0xF4 else 0
    multibyte = flags & 1
    trailers, f = 0, flags
    while f > 1:
        if f & 2:
            trailers += 1
        f >>= 1

    def size_of_trailing(data):  # MOBI 尾部数据项长度（反向变长整数）
        num = 0
        for v in data[-4:]:
            if v & 0x80:
                num = 0
            num = (num << 7) | (v & 0x7F)
        return num

    def trim(data):  # 剥尾部数据项 + multibyte 溢出字节，还原纯压缩数据
        for _ in range(trailers):
            data = data[: len(data) - size_of_trailing(data)]
        if multibyte and data:
            data = data[: len(data) - ((data[-1] & 3) + 1)]
        return data

    reader = {1: UncompressedReader, 2: PalmdocReader}.get(comp)
    if reader is None:
        sys.exit(f"[build_dict] 压缩类型 {comp} 暂不支持（仅 PalmDOC=2/无压缩=1；Huff/CDIC 未实现）")
    rd = reader()
    parts = []
    for i in range(1, min(text_recs, nrec - 1) + 1):
        d = rec(i)
        if not d:
            continue
        try:
            parts.append(rd.unpack(trim(d)))
        except Exception:
            pass  # 个别损坏记录跳过，不中断（尾部 padding/索引记录常态）
    full = b"".join(parts).decode("utf-8", "replace")
    print(f"[build_dict] raw MOBI 解出 {len(full)} 字符（{min(text_recs, nrec-1)} 条文本记录）", file=sys.stderr)
    return full


def parse_entries(html: str, lang: str):
    """HTML → [(key, phonetic, body)]，按 <hr/> 切块解析。已按 key 去重（保首现）、排序。
    词头两种形态都认：raw MOBI = 块首 <b>词</b>（[`HEAD_B_RE`]）；calibre = <span class="bold">词</span>。"""
    # 只取 <body> 之后，避开 head/title/style。
    bi = html.lower().find("<body")
    if bi > 0:
        html = html[bi:]
    entries = {}
    for chunk in HR_SPLIT_RE.split(html):
        hb = HEAD_B_RE.match(chunk)  # raw MOBI：块首 <b>词</b>
        if hb:
            head = strip_tags(hb.group(1))
            body_raw = strip_tags(hb.group(2))
        else:  # calibre：<span class="bold">词</span>
            m = BOLD_RE.search(chunk)
            if not m:
                continue
            head = strip_tags(m.group(1))
            body_raw = strip_tags(chunk[m.end():])
        if not head:
            continue
        if not body_raw:
            continue
        key = normalize_key(head, lang)
        if not key or key in entries:
            continue
        phonetic = ""
        body = body_raw
        if lang == "zh":
            # 现汉 body 以拼音打头（"mǎlāsōng sàipǎo 一种…"）：首个 CJK 字符前的
            # 拉丁段作 phonetic（含声调字母/间隔号/连字符），太长则不认（防误吞英文释义）。
            pm = re.match(r"^([^㐀-鿿【（(]{1,40}?)\s*(?=[㐀-鿿【（(])", body_raw)
            if pm and re.search(r"[a-zA-Zāáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ]", pm.group(1)):
                phonetic = pm.group(1).strip()
                body = body_raw[pm.end():].strip()
        if lang == "en":
            pm = IPA_RE.search(body_raw)
            if pm:
                phonetic = pm.group(0)
                # body 开头常重复音标 + ★(Oxford3000)/□(义项分隔) 记号——去掉，避免和 phonetic 字段重复、更干净
                body = body_raw.replace(phonetic, "", 1).lstrip("★□◊ /;·")
        entries[key] = (phonetic, body)
    # 按 key 的 Unicode 码位序（= UTF-8 字节序 = Rust 二分 cmp）
    return [(k, entries[k][0], entries[k][1]) for k in sorted(entries.keys())]


def write_tsv(rows, out_path: str):
    with open(out_path, "w", encoding="utf-8") as f:
        for key, phon, body in rows:
            f.write(f"{key}\t{escape_field(phon)}\t{escape_field(body)}\t\n")
    print(f"[build_dict] 写出 {len(rows)} 条 → {out_path}", file=sys.stderr)


def build_one(lang: str, mobi: str, html: str, out: str, via_calibre: bool = False):
    if html:
        text = open(html, encoding="utf-8", errors="replace").read()
    elif via_calibre:
        text = mobi_to_html(mobi)  # 慢/大词典会卡死，仅回退用
    else:
        text = mobi_to_html_raw(mobi)  # 默认：直接解 MOBI，快、不卡
    rows = parse_entries(text, lang)
    if not rows:
        sys.exit("[build_dict] 解析出 0 条，检查 HTML 结构 / lang")
    write_tsv(rows, out)


def main():
    ap = argparse.ArgumentParser(description="Kindle 词典 MOBI → 排序 TSV（生词本查词用）")
    ap.add_argument("--lang", choices=["en", "zh"], help="词典方向")
    ap.add_argument("--mobi", help="MOBI 路径（默认直接解 MOBI，不经 calibre）")
    ap.add_argument("--html", help="已转好的 HTML 路径（跳过抽取，用于迭代解析）")
    ap.add_argument("--out", help="输出 TSV 路径")
    ap.add_argument("--via-calibre", action="store_true",
                    help="用 calibre ebook-convert 抽取（慢，大词典会卡死；默认走 raw 直解）")
    ap.add_argument("--auto", action="store_true", help=f"批处理 {DEFAULT_DICT_DIR} 里的两本")
    ap.add_argument("--out-dir", default=".", help="--auto 的输出目录")
    a = ap.parse_args()

    if a.auto:
        os.makedirs(a.out_dir, exist_ok=True)
        build_one("en", os.path.join(DEFAULT_DICT_DIR, EN_MOBI), None,
                  os.path.join(a.out_dir, "en.tsv"), a.via_calibre)
        build_one("zh", os.path.join(DEFAULT_DICT_DIR, ZH_MOBI), None,
                  os.path.join(a.out_dir, "zh.tsv"), a.via_calibre)
        return
    if not a.lang or not a.out or (not a.mobi and not a.html):
        ap.error("需 --lang + --out + (--mobi 或 --html)，或用 --auto")
    build_one(a.lang, a.mobi, a.html, a.out, a.via_calibre)


if __name__ == "__main__":
    main()
