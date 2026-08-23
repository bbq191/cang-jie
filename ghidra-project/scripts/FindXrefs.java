import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefs extends GhidraScript {
    public void run() throws Exception {
        // file offsets (from `strings -t x`) + 0x400000 (LOAD segment base) = VA
        long[] fileOffsets = {
            0xcaa268L, 0xcaa298L, 0xcac460L, 0xcadca8L,
            0xcd6e18L, 0xcd7294L,
            0xe56358L, 0xe566f8L,
            0x1073570L, 0x107d3b8L, 0x107d398L
        };
        println("===XREFS_START===");
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
        println("===XREFS_END===");
    }
}
