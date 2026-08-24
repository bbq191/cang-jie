import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import java.util.LinkedHashSet;
import java.util.Set;

// SceneLink 第二轮：读/写分发器 + SceneLink RTTI 引用函数。
// 直接给【函数入口 vaddr】（第一轮已知）；再对若干函数找 callers 反查上层分发器。
public class DecompileSceneLink2 extends GhidraScript {

    // 直接反编译这些函数（vaddr）
    static final long[] DIRECT = {
        0x00e43330L, // SceneItem 类型→块 tag 映射器（写路径）
        0x00e43550L, // Unknown block fallback（读路径，逐字节保留）
        0x005eb1d0L, // 引用 "SceneLink" RTTI
        0x005e5a30L, // 引用 "SceneLink" RTTI
    };

    // 找这些函数的 callers（上层分发器），并反编译调用者
    static final long[] FIND_CALLERS_OF = {
        0x00e43330L, // 谁调用映射器 → 写序列化分发
        0x00e43550L, // 谁调用 unknown-block fallback → 读分发（按 tag switch）
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
                println("   call from " + from + "  in  " + fn + "  [" + r.getReferenceType() + "]");
                if (f != null) funcAddrs.add(f.getEntryPoint());
                n++;
            }
            if (n == 0) println("   (无调用者引用)");
        }
        println("===CALLERS_END===");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (Address addr : funcAddrs) {
            Function f = getFunctionAt(addr);
            if (f == null) continue;
            DecompileResults res = decomp.decompileFunction(f, 240, monitor);
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
