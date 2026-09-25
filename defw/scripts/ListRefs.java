// List every reference TO the given addresses (script args, hex without 0x) with the containing function and
// reference type (READ/WRITE/DATA/CALL…). Handy for "who writes this global" questions, e.g. a pointer a
// crashing worker dereferenced.
//   ghidra-analyzeHeadless <proj_dir> xochitl_328_analysis -process xochitl-3.28.0.172 -noanalysis -readOnly \
//     -scriptPath defw/scripts -postScript ListRefs.java 01aa1880
//@category CangJie

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class ListRefs extends GhidraScript {
    @Override
    public void run() throws Exception {
        for (String t : getScriptArgs()) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(t);
            println("=== refs to " + addr + " ===");
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(addr);
            int n = 0;
            while (refs.hasNext()) {
                Reference r = refs.next();
                Function f = getFunctionContaining(r.getFromAddress());
                println("  " + r.getFromAddress() + " " + r.getReferenceType() + " in " + (f != null ? f.getName() + " @ " + f.getEntryPoint() : "???"));
                n++;
            }
            println("  (" + n + " refs)");
        }
    }
}
