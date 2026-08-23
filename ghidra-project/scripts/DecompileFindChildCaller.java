import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.util.task.ConsoleTaskMonitor;

public class DecompileFindChildCaller extends GhidraScript {
    public void run() throws Exception {
        long[] addrs = { 0x7f1b20L };
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (long a : addrs) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(a);
            Function f = getFunctionAt(addr);
            DecompileResults res = decomp.decompileFunction(f, 90, new ConsoleTaskMonitor());
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            println(res.getDecompiledFunction().getC());
            println("===DECOMPILE_END===");
        }
    }
}
