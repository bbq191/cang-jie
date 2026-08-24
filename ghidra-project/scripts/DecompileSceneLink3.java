import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import java.util.LinkedHashSet;
import java.util.Set;

// SceneLink 第三轮：链接目标(documentId/pageId/action)从哪来。
// 锚点：GotoPage 枚举串、documentId 串、LinkProviderDevice 构造。
public class DecompileSceneLink3 extends GhidraScript {
    static final long BIAS = 0x400000L;

    // 文件偏移锚点（.rodata 段，vaddr=off+BIAS）
    static final long[] ANCHORS = {
        0xdeffc0L,  // "GotoPage"
        0xdeffa8L,  // "GotoPageSelection"
        0xd03588L,  // "documentId"
        0xdec260L,  // "pageId"
    };
    // 直接反编译（vaddr）：LinkProviderDevice 构造相关
    static final long[] DIRECT = {
        0x00e98e54L, // 附近可能是 LinkProviderDevice
    };

    public void run() throws Exception {
        Set<Address> funcAddrs = new LinkedHashSet<>();
        println("===XREFS_START===");
        for (long off : ANCHORS) {
            Address strAddr = toAddr(off + BIAS);
            println("-- anchor fileoff=0x" + Long.toHexString(off) + " vaddr=" + strAddr + " --");
            ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            int n = 0;
            while (it.hasNext() && n < 20) {
                Reference r = it.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                String fn = (f != null) ? (f.getName() + "@" + f.getEntryPoint()) : "(data)";
                println("   xref from " + from + " -> " + fn + " [" + r.getReferenceType() + "]");
                // 只收 GotoPage / GotoPageSelection 的代码引用（链接动作消费点）
                if (f != null && (off == 0xdeffc0L || off == 0xdeffa8L)) funcAddrs.add(f.getEntryPoint());
                n++;
            }
            if (n == 0) println("   (无 xref)");
        }
        for (long a : DIRECT) {
            Function f = getFunctionContaining(toAddr(a));
            if (f != null) { funcAddrs.add(f.getEntryPoint()); println("DIRECT func @ " + f.getEntryPoint()); }
        }
        println("unique_funcs=" + funcAddrs.size());
        println("===XREFS_END===");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (Address addr : funcAddrs) {
            Function f = getFunctionAt(addr);
            if (f == null) continue;
            DecompileResults res = decomp.decompileFunction(f, 180, monitor);
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            if (res != null && res.getDecompiledFunction() != null)
                println(res.getDecompiledFunction().getC());
            else println("(decompile failed)");
            println("===DECOMPILE_END===");
        }
    }
}
