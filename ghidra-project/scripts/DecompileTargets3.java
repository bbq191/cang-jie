import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.util.task.ConsoleTaskMonitor;

public class DecompileTargets3 extends GhidraScript {
    public void run() throws Exception {
        long[] addrs = { 0x67dd80L, 0x69a410L, 0x67ea30L };
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (long a : addrs) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(a);
            Function f = getFunctionAt(addr);
            if (f == null) {
                println("=== 没有函数 @ " + Long.toHexString(a) + " ===");
                continue;
            }
            DecompileResults res = decomp.decompileFunction(f, 60, new ConsoleTaskMonitor());
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            println(res.getDecompiledFunction().getC());
            println("===DECOMPILE_END===");
        }
    }
}
