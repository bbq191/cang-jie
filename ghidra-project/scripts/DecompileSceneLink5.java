import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import java.util.LinkedHashSet;
import java.util.Set;

// SceneLink 第五轮（收官）：谁构造 LibraryId → documentId/pageId 从哪取（决定目标存 .rm 还是外部）。
public class DecompileSceneLink5 extends GhidraScript {
    static final long[] FIND_CALLERS_OF = {
        0x009f2300L, // LibraryId 构造函数
    };
    static final long[] DIRECT = {
        0x0104cff0L, // 格式位应用器（确认 3bit 码含 link）
    };

    public void run() throws Exception {
        Set<Address> funcAddrs = new LinkedHashSet<>();
        for (long a : DIRECT) {
            Function f = getFunctionContaining(toAddr(a));
            if (f != null) funcAddrs.add(f.getEntryPoint());
        }
        println("===CALLERS_START===");
        for (long a : FIND_CALLERS_OF) {
            Address target = toAddr(a);
            println("-- callers of " + target + " --");
            ReferenceIterator it = currentProgram.getReferenceManager().getReferencesTo(target);
            int n = 0;
            while (it.hasNext()) {
                Reference r = it.next();
                if (!r.getReferenceType().isCall() && !r.getReferenceType().isFlow()) continue;
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                String fn = (f != null) ? (f.getName() + "@" + f.getEntryPoint()) : "(no func)";
                println("   call from " + from + " in " + fn + " [" + r.getReferenceType() + "]");
                if (f != null) funcAddrs.add(f.getEntryPoint());
                n++;
            }
            if (n == 0) println("   (无调用者)");
        }
        println("===CALLERS_END===");

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
