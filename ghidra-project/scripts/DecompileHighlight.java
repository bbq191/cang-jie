import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import java.util.LinkedHashSet;
import java.util.Set;

// 荧光笔汉字吸附侦查（stripped 二进制，无函数符号）：
// 靠 .rodata 里残留的函数名/错误串做 xref 反查目标函数，再反编译。
// 传入的是【文件偏移】（strings -t x 给的），vaddr = off + 0x400000 (VADDR_BIAS)。
public class DecompileHighlight extends GhidraScript {
    static final long BIAS = 0x400000L;

    // {文件偏移} —— .169 版（strings -t x 实测）
    static final long[][] ANCHORS = {
        { 0x104e908L, 0 }, // "addSnappedHighlightLine: empty line"
        { 0x104e930L, 0 }, // "addSnappedHighlightLine: invalid layer"
        { 0x101a1c6L, 0 }, // "snapHighlighterToText"
        { 0xfaf328L,  0 }, // highlightWithLine lambda 符号串
        { 0xfad530L,  0 }, // "17SelectionAnalyzer" RTTI name
    };

    public void run() throws Exception {
        Set<Address> funcAddrs = new LinkedHashSet<>();
        println("===XREFS_START===");
        for (long[] a : ANCHORS) {
            long vaddr = a[0] + BIAS;
            Address strAddr = toAddr(vaddr);
            println("-- anchor fileoff=0x" + Long.toHexString(a[0]) + " vaddr=" + strAddr + " --");
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
            if (n == 0) println("   (无 xref —— 该串可能未被分析为引用，或不在代码路径)");
        }
        // 扩张链（.169，从 FUN_00f07fd0 反编译回填）：取range→扩张→生成→输出
        long[] extra = { 0xf05ed0L, 0xf05bb0L, 0xf05ad0L, 0xf05d00L, 0xf07a70L, 0xf063d0L };
        for (long a : extra) {
            Function f = getFunctionContaining(toAddr(a));
            if (f != null) funcAddrs.add(f.getEntryPoint());
            else println("EXTRA 无函数 @ 0x" + Long.toHexString(a));
        }
        println("unique_funcs=" + funcAddrs.size());
        println("===XREFS_END===");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (Address addr : funcAddrs) {
            Function f = getFunctionAt(addr);
            if (f == null) { println("=== 没有函数 @ " + addr + " ==="); continue; }
            DecompileResults res = decomp.decompileFunction(f, 120, monitor);
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
