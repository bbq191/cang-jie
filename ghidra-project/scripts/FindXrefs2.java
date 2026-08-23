import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefs2 extends GhidraScript {
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        long[] funcAddrs = { 0x69c030L, 0x690bb0L, 0x588910L };
        println("===XREFS2_START===");
        for (long fa : funcAddrs) {
            Address faddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(fa);
            println("--- func@0x" + Long.toHexString(fa) + " ---");
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(faddr);
            int n = 0;
            while (refs.hasNext()) {
                Reference r = refs.next();
                Address from = r.getFromAddress();
                println("  xref from " + from + " type=" + r.getReferenceType());
                n++;
                if (n > 10) break;
            }
            if (n == 0) println("  (no xrefs)");
        }
        println("===XREFS2_END===");
    }
}
