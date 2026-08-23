import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefs3 extends GhidraScript {
    public void run() throws Exception {
        long[] fileOffsets = { 0xe678c0L, 0xe687d0L, 0x107e3e8L };
        println("===XREFS3_START===");
        for (long off : fileOffsets) {
            long va = off + 0x400000L;
            Address strAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(va);
            println("--- string@0x" + Long.toHexString(va) + " (fileoff 0x" + Long.toHexString(off) + ") ---");
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            int n = 0;
            while (refs.hasNext()) {
                Reference r = refs.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                println("  xref from " + from + " in function " + (f != null ? f.getName() + "@" + f.getEntryPoint() : "???"));
                n++;
                if (n > 20) { println("  ...(truncated)"); break; }
            }
            if (n == 0) println("  (no xrefs)");
        }
        println("===XREFS3_END===");
    }
}
