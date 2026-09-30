// Decompile a fixed list of known target addresses and print their C output
// plus caller xrefs. Used for headless follow-up after GUI-driven recon has
// already located concrete addresses (see the enhance whitepaper §03c-§03g; enhance/handwriting-stroke/ was removed on 2026-09-30).
//@category CangJie

import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.util.task.ConsoleTaskMonitor;

public class DecompileTargets extends GhidraScript {

    private DecompInterface decomp;

    @Override
    public void run() throws Exception {
        decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        // Reusable target list — edit the address(es) here and rerun. Requires the
        // GUI project to be closed first (headless and GUI can't share the .lock).
        String[] targets = {
            "00f4f430", "00f4d190", "00f4c8d0"
        };

        for (String t : targets) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(t);
            printFunction(addr);
        }
    }

    private void printFunction(Address addr) {
        Function f = getFunctionAt(addr);
        if (f == null) {
            println("=== NO FUNCTION AT " + addr + " ===");
            return;
        }
        println("=== FUNCTION " + f.getName() + " @ " + addr + " ===");
        println("Signature: " + f.getSignature());

        DecompileResults res = decomp.decompileFunction(f, 60, new ConsoleTaskMonitor());
        if (res != null && res.decompileCompleted()) {
            println(res.getDecompiledFunction().getC());
        } else {
            println("DECOMPILE FAILED: " + (res != null ? res.getErrorMessage() : "null result"));
        }

        println("--- CALLERS OF " + f.getName() + " ---");
        ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(addr);
        while (refs.hasNext()) {
            Reference r = refs.next();
            Address from = r.getFromAddress();
            Function caller = getFunctionContaining(from);
            println("  " + from + " in " + (caller != null ? caller.getName() : "???") + " (" + r.getReferenceType() + ")");
        }
        println("");
    }
}
