import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.util.task.ConsoleTaskMonitor;

public class DecompileLangHandler extends GhidraScript {
    public void run() throws Exception {
        long[] addrs = { 0xc6f3c0L };
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.toggleCCode(true);
        decomp.setSimplificationStyle("decompile");
        for (long a : addrs) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(a);
            Function f = getFunctionAt(addr);
            if (f == null) {
                println("===NO_FUNCTION_AT:" + Long.toHexString(a) + "===");
                continue;
            }
            DecompileResults res = decomp.decompileFunction(f, 240, new ConsoleTaskMonitor());
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            println(res.getDecompiledFunction().getC());
            println("===DECOMPILE_END===");
        }
    }
}
