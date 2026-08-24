import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import java.util.LinkedHashSet;
import java.util.Set;

// SceneLink 第四轮：目标 (documentId/pageId/action) 从哪来 + 链接解析器确切偏移。
public class DecompileSceneLink4 extends GhidraScript {
    static final long BIAS = 0x400000L;

    // .rodata 串锚点（文件偏移）→ xref 反查函数
    static final long[] ANCHORS = {
        0x10a7530L, // "LibraryId referring to a page cannot have an empty pageId" —— LibraryId 解析/校验
        0x106b488L, // "encountered link start:" —— 详细链接解析器
        0x106c188L, // "encountered link end with id"
        0x106b4a0L, // "encountered link end match:"
        0x10a8c20L, // "setTarget: target has no font property" —— 目标 setter
    };
    // 找这些函数的 callers
    static final long[] FIND_CALLERS_OF = {
        0x0104d660L, // 链接装配 pass —— 谁喂它场景流
    };

    public void run() throws Exception {
        Set<Address> funcAddrs = new LinkedHashSet<>();
        println("===XREFS_START===");
        for (long off : ANCHORS) {
            Address strAddr = toAddr(off + BIAS);
            println("-- anchor fileoff=0x" + Long.toHexString(off) + " vaddr=" + strAddr + " --");
            ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            int n = 0;
            while (it.hasNext()) {
                Reference r = it.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                String fn = (f != null) ? (f.getName() + "@" + f.getEntryPoint()) : "(data)";
                println("   xref from " + from + " -> " + fn + " [" + r.getReferenceType() + "]");
                if (f != null) funcAddrs.add(f.getEntryPoint());
                n++;
            }
            if (n == 0) println("   (无 xref)");
        }
        println("-- callers --");
        for (long a : FIND_CALLERS_OF) {
            Address target = toAddr(a);
            println("-- callers of " + target + " --");
            ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(target);
            while (it.hasNext()) {
                Reference r = it.next();
                if (!r.getReferenceType().isCall() && !r.getReferenceType().isFlow()) continue;
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                String fn = (f != null) ? (f.getName() + "@" + f.getEntryPoint()) : "(no func)";
                println("   call from " + from + " in " + fn + " [" + r.getReferenceType() + "]");
                if (f != null) funcAddrs.add(f.getEntryPoint());
            }
        }
        println("unique_funcs=" + funcAddrs.size());
        println("===XREFS_END===");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (Address addr : funcAddrs) {
            Function f = getFunctionAt(addr);
            if (f == null) continue;
            DecompileResults res = decomp.decompileFunction(f, 200, monitor);
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            if (res != null && res.getDecompiledFunction() != null)
                println(res.getDecompiledFunction().getC());
            else println("(decompile failed)");
            println("===DECOMPILE_END===");
        }
    }
}
