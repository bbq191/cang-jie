import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import java.util.LinkedHashSet;
import java.util.Set;

// SceneLink .rm 字节结构侦查（stripped 二进制）：
// 靠 .rodata 里残留的诊断串/RTTI 串做 xref 反查目标函数，再反编译。
// 传入【文件偏移】（strings -t x）；vaddr = off + 0x400000 (BIAS)。
public class DecompileSceneLink extends GhidraScript {
    static final long BIAS = 0x400000L;

    // {文件偏移, 说明标记(仅注释用)} —— .169 版
    static final long[] ANCHORS = {
        0x1050e50L, // "Unknown block starts at %u: tag=..." —— .rm 分块 reader（格式解码器）
        0x1050e30L, // "Unhandled SceneItem type ({})" —— 场景项类型分发器
        0x10513d0L, // "Some data errors while reading the file:"
        0x106b478L, // "-- add link:" —— 链接创建入口
        0x1050e00L, // 邻近块（可能是同一 reader 的其它诊断，探测）
    };

    // RTTI / typeinfo 名字串：xref 来自 typeinfo 结构（非代码），用于定位 vtable/构造函数
    static final long[] RTTI = {
        0xdff8dfL,  // "SceneLink" (紧凑 RTTI 名)
        0xdf28b8L,  // "SceneLink"
        0xdf26c0L,  // "xofm::libs::linkprovider::LinkDetails"
        0xe06508L,  // "N4xofm4libs12linkprovider11LinkDetailsE"
    };

    public void run() throws Exception {
        Set<Address> funcAddrs = new LinkedHashSet<>();

        println("===XREFS_START===");
        for (long off : ANCHORS) {
            long vaddr = off + BIAS;
            Address strAddr = toAddr(vaddr);
            println("-- anchor fileoff=0x" + Long.toHexString(off) + " vaddr=" + strAddr + " --");
            ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            int n = 0;
            while (it.hasNext()) {
                Reference r = it.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                String fn = (f != null) ? (f.getName() + "@" + f.getEntryPoint()) : "(no func)";
                println("   xref from " + from + "  ->  " + fn + "  [" + r.getReferenceType() + "]");
                if (f != null) funcAddrs.add(f.getEntryPoint());
                n++;
            }
            if (n == 0) println("   (无 xref)");
        }

        println("-- RTTI/typeinfo 名字串 xref（定位 typeinfo 结构地址）--");
        for (long off : RTTI) {
            long vaddr = off + BIAS;
            Address strAddr = toAddr(vaddr);
            println("-- rtti fileoff=0x" + Long.toHexString(off) + " vaddr=" + strAddr + " --");
            ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            int n = 0;
            while (it.hasNext()) {
                Reference r = it.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                String fn = (f != null) ? (f.getName() + "@" + f.getEntryPoint()) : "(data/typeinfo)";
                println("   xref from " + from + "  ->  " + fn + "  [" + r.getReferenceType() + "]");
                if (f != null) funcAddrs.add(f.getEntryPoint());
                n++;
            }
            if (n == 0) println("   (无 xref)");
        }

        println("unique_funcs=" + funcAddrs.size());
        println("===XREFS_END===");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (Address addr : funcAddrs) {
            Function f = getFunctionAt(addr);
            if (f == null) { println("=== 没有函数 @ " + addr + " ==="); continue; }
            DecompileResults res = decomp.decompileFunction(f, 180, monitor);
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            if (res != null && res.getDecompiledFunction() != null) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("(decompile failed)");
            }
            println("===DECOMPILE_END===");
        }
    }
}
