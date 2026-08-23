import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.util.task.ConsoleTaskMonitor;

public class DecompileEmitters extends GhidraScript {
    public void run() throws Exception {
        long[] addrs = {
            0x68e9a0L,0x68eb20L,0x68eb60L,0x68eba0L,0x68ebc0L,0x68ebe0L,
            0x68ec00L,0x68ec40L,0x68ec60L,0x68ec80L,0x68eca0L,0x68ecc0L,
            0x68ece0L,0x68ed00L,0x68ed20L,0x68ed40L,0x68ed60L,0x68ed80L,
            0x68eda0L,0x68edc0L,0x68ede0L,
            0x68f230L,0x68f340L,0x68f3f0L,0x68f540L,0x68f5c0L,
            0x68e160L,0x68e310L,0x68c910L,0x68c900L,0x68c9b0L,0x68cbf0L,0x68b950L
        };
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        for (long a : addrs) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(a);
            Function f = getFunctionAt(addr);
            if (f == null) { println("=== 没有函数 @ " + Long.toHexString(a) + " ==="); continue; }
            DecompileResults res = decomp.decompileFunction(f, 60, new ConsoleTaskMonitor());
            println("===DECOMPILE_START:" + f.getName() + "@" + f.getEntryPoint() + "===");
            println(res.getDecompiledFunction().getC());
            println("===DECOMPILE_END===");
        }
    }
}
