import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefs4 extends GhidraScript {
    public void run() throws Exception {
        Address faddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(0x695900L);
        println("===XREFS4_START===");
        ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(faddr);
        while (refs.hasNext()) {
            Reference r = refs.next();
            println("  xref from " + r.getFromAddress() + " type=" + r.getReferenceType());
        }
        println("===XREFS4_END===");
    }
}
