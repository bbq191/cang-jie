import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefsEpub2 extends GhidraScript {
    public void run() throws Exception {
        long startOff = 0xccd000L;
        long endOff = 0xccdc00L;
        println("===XREFS_START===");
        for (long off = startOff; off < endOff; off += 1) {
            long va = off + 0x400000L;
            Address strAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(va);
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            while (refs.hasNext()) {
                Reference r = refs.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                println("xref to 0x" + Long.toHexString(va) + " from " + from + " in function " +
                        (f != null ? f.getName() + "@" + f.getEntryPoint() : "???") +
                        " reftype=" + r.getReferenceType());
            }
        }
        println("===XREFS_END===");
    }
}
