import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;

// SceneLink 第六轮：LibraryId 工厂/调用者 → 目标 documentId/pageId 的持久化形态。
public class DecompileSceneLink6 extends GhidraScript {
    static final long[] DIRECT = {
        0x009f3d50L, // LibraryId 工厂（离 ctor 最近）
        0x00949cb0L,
        0x00960fe0L,
        0x009e7af0L,
    };
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (long a : DIRECT) {
            Function f = getFunctionContaining(toAddr(a));
            if (f == null) { println("=== 无函数 @ 0x" + Long.toHexString(a) + " ==="); continue; }
            DecompileResults res = decomp.decompileFunction(f, 200, monitor);
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            if (res != null && res.getDecompiledFunction() != null)
                println(res.getDecompiledFunction().getC());
            else println("(decompile failed)");
            println("===DECOMPILE_END===");
        }
    }
}
