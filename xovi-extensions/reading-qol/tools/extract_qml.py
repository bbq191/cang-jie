#!/usr/bin/env python3
"""
extract_qml.py —— 从 xochitl 二进制里离线解出内嵌的 QML 源文件。

背景：xochitl 把 QML 打包进 Qt 资源系统，且**逐文件用 zstd 压缩**（frame magic
28 b5 2f fd）。Qt 资源目录树（文件名）在 stripped 二进制里另存、解析成本高，本脚本
不还原文件名，只把每个 zstd frame 解压出来、按内容判定是不是 QML，落盘 + 建符号索引，
供 grep 认领目标文件（DocumentView.qml / SceneViewGestures.qml / SettingsMenu.qml…）。

来源纪律：这是白皮书 3.5 Step J「zstd magic 全局扫 + 逐块解压」那套方法的**可复用落盘版**
（当年一次性解出 552 个用完即弃、无脚本）。纯离线只读，不改二进制、不碰设备。

用法：
    python3 extract_qml.py <xochitl 二进制> <输出目录>
产出：
    <输出目录>/blobs/qml_<offset>.qml   每个疑似 QML 的解压内容
    <输出目录>/INDEX.tsv               offset<TAB>大小<TAB>首个 import/根类型<TAB>文件名
"""
import sys
import os
import zstandard

ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"


def looks_like_qml(text: bytes) -> bool:
    head = text[:4096]
    # QML 源码几乎必有 import 语句，且含花括号块；排除纯 JS/JSON/翻译等
    return (b"import " in head) and (b"{" in text) and (b"}" in text)


def first_root_type(text: str) -> str:
    """粗取第一个根对象类型名/首个 import，便于在 INDEX 里辨识。"""
    root = ""
    for line in text.splitlines():
        s = line.strip()
        if not s or s.startswith("//") or s.startswith("/*") or s.startswith("*"):
            continue
        if s.startswith("import"):
            if not root:
                root = s
            continue
        # 第一个非 import、非注释、以标识符开头且后随 { 的行 → 根类型
        tok = s.split("{")[0].split("(")[0].strip()
        if tok and (tok[0].isalpha() or tok[0] == "_") and " " not in tok:
            return tok
    return root


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    binpath, outdir = sys.argv[1], sys.argv[2]
    data = open(binpath, "rb").read()
    blobs_dir = os.path.join(outdir, "blobs")
    os.makedirs(blobs_dir, exist_ok=True)

    dctx = zstandard.ZstdDecompressor()
    index = []
    off = 0
    n_frames = 0
    n_qml = 0
    total = len(data)
    while True:
        i = data.find(ZSTD_MAGIC, off)
        if i < 0:
            break
        off = i + 1  # 下次从 magic 之后继续扫（frame 内可能再现 magic，靠 find 兜底）
        try:
            dobj = dctx.decompressobj()
            out = dobj.decompress(data[i:])
        except Exception:
            continue
        n_frames += 1
        if not out or not looks_like_qml(out):
            continue
        n_qml += 1
        try:
            text = out.decode("utf-8")
        except UnicodeDecodeError:
            text = out.decode("utf-8", "replace")
        fname = os.path.join(blobs_dir, f"qml_{i:08x}.qml")
        with open(fname, "w") as f:
            f.write(text)
        index.append((i, len(out), first_root_type(text).replace("\t", " "), os.path.basename(fname)))

    with open(os.path.join(outdir, "INDEX.tsv"), "w") as f:
        f.write("offset\tsize\tfirst_import_or_root\tfile\n")
        for i, sz, root, fn in index:
            f.write(f"0x{i:08x}\t{sz}\t{root}\t{fn}\n")

    print(f"二进制大小: {total}")
    print(f"扫到 zstd frame(可解压): {n_frames}")
    print(f"其中疑似 QML: {n_qml} → 落盘到 {blobs_dir}/")
    print(f"索引: {os.path.join(outdir, 'INDEX.tsv')}")


if __name__ == "__main__":
    main()
