// 找 Quill::strokev2 笔画光栅化引擎的 RTTI typeinfo name 字符串 → 反查 typeinfo 对象
// → 反查该类自己的 vtable（Itanium C++ ABI：vtable 布局是
// [offset_to_top, typeinfo_ptr, vfunc0, vfunc1, ...]，typeinfo_ptr 紧挨在第一个
// 虚函数指针前面，找到"谁指向 typeinfo 对象"就能反推出 vtable 起始地址）。
// 同样的"靠已知指针反查容器结构体"手法项目里已经用过（cj_find_metaobject，
// chinese-ime/langhook/src/hook_init.c），这里是同一招用在 C++ RTTI 上。
//
// 用法：analyzeHeadless <project> <name> -process -scriptPath defw/scripts
//       -postScript FindQuillStrokeRTTI.java
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.Memory;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

public class FindQuillStrokeRTTI extends GhidraScript {

    // 2026-09-09 用 `strings -n 6 xochitl` 在真机 3.28.0.172 二进制上找到的
    // Quill::strokev2 相关 RTTI typeinfo name 字符串（截取一部分代表性的，不用
    // 全找——先摸清楚这套结构长什么样，找到一两个就够验证判断对不对）。
    static final String[] NEEDLES = {
        "N5Quill10LerpRasterIN8strokev210FillPencilIjEEEE",
        "N5Quill10LerpRasterIN8strokev213FillBallpointIjEEEE",
        "N5Quill10MonoRasterIN8strokev216FillMaskedEraserIjEEEE",
        "N5Quill10MonoRasterIN8strokev216FillSolid_OpaqueIjEEEE",
    };

    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        println("===QUILL_RTTI_START===");
        for (String needle : NEEDLES) {
            byte[] pat = needle.getBytes(StandardCharsets.US_ASCII);
            List<Address> hits = findAllOccurrences(mem, pat);
            println("--- \"" + needle + "\" 命中 " + hits.size() + " 处 ---");
            for (Address strAddr : hits) {
                println("  string @ " + strAddr);
                // Ghidra 自动分析没建 xref（stripped 二进制，指向这段字符串的指针字段
                // 之前从没被识别/定型过）——改用跟 cj_find_metaobject 同一招：直接在
                // 内存里搜"字面等于这个地址的 8 字节指针值"，不依赖任何既有的类型标注。
                byte[] ptrBytes = addrToLeBytes(strAddr);
                List<Address> typeinfoHits = findAllOccurrences(mem, ptrBytes);
                println("    指针值命中 " + typeinfoHits.size() + " 处（typeinfo 对象候选，"
                        + "Itanium ABI 里 name 指针是 typeinfo 对象的第二个字段）");
                for (Address tiFieldAddr : typeinfoHits) {
                    // typeinfo 对象布局：[vtable_ptr(8), name_ptr(8), ...]，命中的地址是
                    // name_ptr 字段本身，typeinfo 对象起始 = 命中地址 - 8。
                    Address tiObj = tiFieldAddr.subtract(8);
                    println("      typeinfo 对象候选 @ " + tiObj + "（name 指针字段 @ " + tiFieldAddr + "）");
                    Function fContaining = getFunctionContaining(tiFieldAddr);
                    Data dContaining = getDataContaining(tiFieldAddr);
                    println("      所在: " + (fContaining != null ? "函数 " + fContaining.getName() : dContaining != null ? "数据 " + dContaining.getLabel() : "未知区域"));
                    // 再反查谁指向这个 typeinfo 对象——class 自己的 vtable 里紧挨在
                    // 第一个虚函数指针前面那个字段就是 typeinfo_ptr，同样手法反推一层。
                    byte[] tiObjPtrBytes = addrToLeBytes(tiObj);
                    List<Address> vtableHits = findAllOccurrences(mem, tiObjPtrBytes);
                    println("      指向该 typeinfo 对象的指针命中 " + vtableHits.size() + " 处（vtable 候选）");
                    for (Address vtField : vtableHits) {
                        Address vtableStart = vtField.add(8); // typeinfo_ptr 后面紧跟第一个虚函数指针
                        println("        vtable 候选起始 @ " + vtableStart + "（typeinfo_ptr 字段 @ " + vtField + "）");
                        dumpVtableSlots(mem, vtableStart, 6);
                    }
                }
            }
        }
        println("===QUILL_RTTI_END===");
    }

    private byte[] addrToLeBytes(Address a) {
        long v = a.getOffset();
        byte[] out = new byte[8];
        for (int i = 0; i < 8; i++) out[i] = (byte) ((v >> (8 * i)) & 0xff);
        return out;
    }

    // 打印 vtable 起始处若干个 8 字节槽位的原始值，人工核对像不像函数指针
    // （落在 .text 段范围内），不强行反查函数名（可能还没被 Ghidra 识别成函数）。
    private void dumpVtableSlots(Memory mem, Address vtableStart, int nSlots) {
        try {
            Address a = vtableStart;
            for (int i = 0; i < nSlots; i++) {
                byte[] buf = new byte[8];
                mem.getBytes(a, buf);
                long v = 0;
                for (int b = 7; b >= 0; b--) v = (v << 8) | (buf[b] & 0xffL);
                Address target = a.getNewAddress(v);
                Function f = getFunctionAt(target);
                println("          slot[" + i + "] @ " + a + " = 0x" + Long.toHexString(v)
                        + (f != null ? "  (函数 " + f.getName() + ")" : "  (未识别为函数入口)"));
                a = a.add(8);
            }
        } catch (Exception e) {
            println("          (dumpVtableSlots 出错: " + e.getMessage() + ")");
        }
    }

    // 简单字节序列搜索，跨所有可加载内存块；命中数量预期很小（RTTI name 字符串
    // 通常整个二进制只出现一次），不用太讲究效率。
    private List<Address> findAllOccurrences(Memory mem, byte[] pat) throws Exception {
        List<Address> out = new ArrayList<>();
        Address start = mem.getMinAddress();
        while (start != null) {
            Address found = mem.findBytes(start, pat, null, true, monitor);
            if (found == null) break;
            out.add(found);
            start = found.add(1);
            if (out.size() > 20) break; // 保险丝，防止某个字符串是别处常见子串意外爆量
        }
        return out;
    }
}
