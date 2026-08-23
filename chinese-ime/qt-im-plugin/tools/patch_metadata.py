#!/usr/bin/env python3
"""把 Qt 插件 .so 里 .note.qt.metadata 段的 Qt 版本字节从本机版降到设备版,过设备版本门。

用法: patch_metadata.py <plugin.so> <from_hex> <to_hex>
  例:  patch_metadata.py libcangjieinputcontextplugin.so 060b 060a   # 6.11 → 6.10

只在 .note.qt.metadata 段范围内查找/替换,且要求 <from> 在段内**唯一命中**,否则报错退出——
避免在整文件里盲替 `06 0b` 误伤无关字节(纪律:写内存/写字节前先只读定位)。
"""
import sys, struct

def find_section(data, name):
    # 最小 ELF64 段表解析,返回 (offset, size)
    if data[:4] != b"\x7fELF" or data[4] != 2:
        raise SystemExit("不是 ELF64")
    e_shoff = struct.unpack_from("<Q", data, 0x28)[0]
    e_shentsize = struct.unpack_from("<H", data, 0x3a)[0]
    e_shnum = struct.unpack_from("<H", data, 0x3c)[0]
    e_shstrndx = struct.unpack_from("<H", data, 0x3e)[0]
    def sh(i):
        b = e_shoff + i * e_shentsize
        nameoff = struct.unpack_from("<I", data, b)[0]
        off = struct.unpack_from("<Q", data, b + 0x18)[0]
        size = struct.unpack_from("<Q", data, b + 0x20)[0]
        return nameoff, off, size
    _, str_off, _ = sh(e_shstrndx)
    for i in range(e_shnum):
        nameoff, off, size = sh(i)
        end = data.index(b"\x00", str_off + nameoff)
        if data[str_off + nameoff:end].decode() == name:
            return off, size
    raise SystemExit(f"找不到段 {name}")

def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    path, fr, to = sys.argv[1], bytes.fromhex(sys.argv[2]), bytes.fromhex(sys.argv[3])
    if len(fr) != len(to):
        raise SystemExit("from/to 长度必须一致")
    data = bytearray(open(path, "rb").read())
    off, size = find_section(data, ".note.qt.metadata")
    seg = data[off:off + size]
    n = seg.count(fr)
    if n == 0:
        # 已经是目标版?幂等允许
        if seg.count(to) >= 1:
            print(f"   .note.qt.metadata 已是 {to.hex()},跳过")
            return
        raise SystemExit(f"   .note.qt.metadata 段内找不到 {fr.hex()}(也没有 {to.hex()})——版本布局变了,停手核对")
    if n != 1:
        raise SystemExit(f"   .note.qt.metadata 段内 {fr.hex()} 命中 {n} 次(非唯一),停手人工核对")
    idx = off + seg.index(fr)
    data[idx:idx + len(to)] = to
    open(path, "wb").write(data)
    print(f"   已在 .note.qt.metadata@0x{idx:x} 把 {fr.hex()} → {to.hex()}")

if __name__ == "__main__":
    main()
