// Decompile the functions CONTAINING the given addresses (script args, hex without 0x), e.g. PCs from a
// crash stack. Unlike DecompileTargets (which needs function entry points), any address inside a function
// works. Prints the function's entry, name, signature, C output, and its callers.
//   ghidra-analyzeHeadless <proj_dir> xochitl_328_analysis -process xochitl -noanalysis \
//     -scriptPath defw/scripts -postScript DecompileContaining.java 00a467b8 00a49fc8
//@category CangJie

import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.util.task.ConsoleTaskMonitor;
import java.util.HashSet;
import java.util.Set;

public class DecompileContaining extends GhidraScript {

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        Set<Address> done = new HashSet<>();
        for (String t : getScriptArgs()) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(t);
            Function f = getFunctionContaining(addr);
            if (f == null) {
                println("=== NO FUNCTION CONTAINS " + addr + " ===");
                continue;
            }
            println("=== " + addr + " is in " + f.getName() + " @ " + f.getEntryPoint() + " (+" + addr.subtract(f.getEntryPoint()) + ") ===");
            if (!done.add(f.getEntryPoint())) {
                println("(already printed above)\n");
                continue;
            }
            println("Signature: " + f.getSignature());
            DecompileResults res = decomp.decompileFunction(f, 120, new ConsoleTaskMonitor());
            println(res != null && res.decompileCompleted() ? res.getDecompiledFunction().getC()
                    : "DECOMPILE FAILED: " + (res != null ? res.getErrorMessage() : "null result"));
            println("--- CALLERS ---");
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(f.getEntryPoint());
            while (refs.hasNext()) {
                Reference r = refs.next();
                Function caller = getFunctionContaining(r.getFromAddress());
                println("  " + r.getFromAddress() + " in " + (caller != null ? caller.getName() : "???") + " (" + r.getReferenceType() + ")");
            }
            println("");
        }
    }
}
