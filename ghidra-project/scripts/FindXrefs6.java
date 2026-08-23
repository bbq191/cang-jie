import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefs6 extends GhidraScript {
    public void run() throws Exception {
        long[] targets = { 0x68eb20L, 0x68eb60L };
        println("===XREFS6_START===");
        for (long t : targets) {
            Address ta = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(t);
            println("--- xrefs to 0x" + Long.toHexString(t) + " ---");
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(ta);
            while (refs.hasNext()) {
                Reference r = refs.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                println("  xref from " + from + " in function " + (f != null ? f.getName() + "@" + f.getEntryPoint() : "???"));
            }
        }
        println("===XREFS6_END===");
    }
}
