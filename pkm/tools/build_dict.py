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
import subprocess
import sys
import tempfile
import zipfile

# 默认用户词典位置（本地正版；派生物 gitignore，不入库）。
DEFAULT_DICT_DIR = os.path.expanduser("~/Documents/ereader/books/字典")
EN_MOBI = "牛津高阶英汉双解第7版(现代版).mobi"
ZH_MOBI = "现代汉语词典.mobi"

TAG_RE = re.compile(r"<[^>]+>")
BOLD_RE = re.compile(r'<span class="bold"[^>]*>(.*?)</span>', re.S)
HR_SPLIT_RE = re.compile(r"<hr\s*/?>", re.I)
WS_RE = re.compile(r"\s+")
# 英文 IPA 音标：牛津正文里形如 /ˈkæʃeɪ/ 的斜杠对；取第一处作 phonetic。
IPA_RE = re.compile(r"/[^/\n]{1,40}/")


def strip_tags(s: str) -> str:
    """去 HTML 标签 + 折叠空白为单空格。"""
    return WS_RE.sub(" ", TAG_RE.sub(" ", s)).strip()


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
            name = next((n for n in z.namelist() if n.endswith("index.html")), None)
            if name is None:
                name = next(n for n in z.namelist() if n.endswith(".html"))
            return z.read(name).decode("utf-8", "replace")


def parse_entries(html: str, lang: str):
    """HTML → [(key, phonetic, body)]，按 <hr/> 切块解析。已按 key 去重（保首现）、排序。"""
    # 只取 <body> 之后，避开 head/title/style。
    bi = html.lower().find("<body")
    if bi > 0:
        html = html[bi:]
    entries = {}
    for chunk in HR_SPLIT_RE.split(html):
        m = BOLD_RE.search(chunk)
        if not m:
            continue
        head = strip_tags(m.group(1))
        if not head:
            continue
        body_raw = strip_tags(chunk[m.end():])
        if not body_raw:
            continue
        key = normalize_key(head, lang)
        if not key or key in entries:
            continue
        phonetic = ""
        body = body_raw
        if lang == "en":
            pm = IPA_RE.search(body_raw)
            if pm:
                phonetic = pm.group(0)
        entries[key] = (phonetic, body)
    # 按 key 的 Unicode 码位序（= UTF-8 字节序 = Rust 二分 cmp）
    return [(k, entries[k][0], entries[k][1]) for k in sorted(entries.keys())]


def write_tsv(rows, out_path: str):
    with open(out_path, "w", encoding="utf-8") as f:
        for key, phon, body in rows:
            f.write(f"{key}\t{escape_field(phon)}\t{escape_field(body)}\t\n")
    print(f"[build_dict] 写出 {len(rows)} 条 → {out_path}", file=sys.stderr)


def build_one(lang: str, mobi: str, html: str, out: str):
    text = open(html, encoding="utf-8", errors="replace").read() if html else mobi_to_html(mobi)
    rows = parse_entries(text, lang)
    if not rows:
        sys.exit(f"[build_dict] 解析出 0 条，检查 HTML 结构 / lang")
    write_tsv(rows, out)


def main():
    ap = argparse.ArgumentParser(description="Kindle 词典 MOBI → 排序 TSV（生词本查词用）")
    ap.add_argument("--lang", choices=["en", "zh"], help="词典方向")
    ap.add_argument("--mobi", help="MOBI 路径（自动调 calibre 转换）")
    ap.add_argument("--html", help="已转好的 index.html 路径（跳过 calibre）")
    ap.add_argument("--out", help="输出 TSV 路径")
    ap.add_argument("--auto", action="store_true", help=f"批处理 {DEFAULT_DICT_DIR} 里的两本")
    ap.add_argument("--out-dir", default=".", help="--auto 的输出目录")
    a = ap.parse_args()

    if a.auto:
        os.makedirs(a.out_dir, exist_ok=True)
        build_one("en", os.path.join(DEFAULT_DICT_DIR, EN_MOBI), None, os.path.join(a.out_dir, "en.tsv"))
        build_one("zh", os.path.join(DEFAULT_DICT_DIR, ZH_MOBI), None, os.path.join(a.out_dir, "zh.tsv"))
        return
    if not a.lang or not a.out or (not a.mobi and not a.html):
        ap.error("需 --lang + --out + (--mobi 或 --html)，或用 --auto")
    build_one(a.lang, a.mobi, a.html, a.out)


if __name__ == "__main__":
    main()
