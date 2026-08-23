import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class FindXrefs5 extends GhidraScript {
    public void run() throws Exception {
        Address staticMetaObjAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(0x12677e8L);
        println("===XREFS5_START===");
        ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(staticMetaObjAddr);
        int n = 0;
        while (refs.hasNext()) {
            Reference r = refs.next();
            Address from = r.getFromAddress();
            Function f = getFunctionContaining(from);
            println("  xref from " + from + " in function " + (f != null ? f.getName() + "@" + f.getEntryPoint() : "???"));
            n++;
        }
        println("total=" + n);
        println("===XREFS5_END===");
    }
}
