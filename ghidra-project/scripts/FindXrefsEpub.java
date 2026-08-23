import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefsEpub extends GhidraScript {
    public void run() throws Exception {
        // file offsets (from python scan of xochitl_3.28.0.164.bin) + 0x400000 = VA
        long[] fileOffsets = {
            0xccd0a8L, // "xofm::libs::epub::EpubProperties"
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
                if (n > 40) { println("  ...(truncated)"); break; }
            }
            if (n == 0) println("  (no xrefs)");
        }
        // also search for the string bytes directly in case it's not defined as a string in listing
        println("===RAW_SEARCH===");
        byte[] pattern = "xofm::libs::epub::EpubProperties".getBytes("UTF-8");
        Address found = currentProgram.getMemory().findBytes(
            currentProgram.getMinAddress(), pattern, null, true, monitor);
        int count = 0;
        while (found != null && count < 5) {
            println("  raw match at " + found);
            ReferenceIterator refs2 = currentProgram.getReferenceManager().getReferencesTo(found);
            int n2 = 0;
            while (refs2.hasNext()) {
                Reference r = refs2.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                println("    xref from " + from + " in function " + (f != null ? f.getName() + "@" + f.getEntryPoint() : "???"));
                n2++;
            }
            found = currentProgram.getMemory().findBytes(found.add(1), currentProgram.getMaxAddress(), pattern, null, true, monitor);
            count++;
        }
        println("===XREFS_END===");
    }
}
