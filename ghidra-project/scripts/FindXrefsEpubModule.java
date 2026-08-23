import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefsEpubModule extends GhidraScript {
    public void run() throws Exception {
        long[] fileOffsets = { 0x10724a0L }; // "xofm.libs.epub"
        println("===XREFS_START===");
        for (long off : fileOffsets) {
            long va = off + 0x400000L;
            Address strAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(va);
            println("--- string@0x" + Long.toHexString(va) + " ---");
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            int n = 0;
            while (refs.hasNext()) {
                Reference r = refs.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                println("  xref from " + from + " in function " + (f != null ? f.getName() + "@" + f.getEntryPoint() : "???"));
                n++;
            }
            if (n == 0) println("  (no xrefs)");
        }
        println("===XREFS_END===");
    }
}
