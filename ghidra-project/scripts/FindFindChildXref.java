import ghidra.app.script.GhidraScript;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolIterator;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;

public class FindFindChildXref extends GhidraScript {
    public void run() throws Exception {
        SymbolIterator it = currentProgram.getSymbolTable().getSymbolIterator("*qFindChildren_helper*", true);
        println("===SYMS_START===");
        while (it.hasNext()) {
            Symbol s = it.next();
            println("sym: " + s.getName() + " @ " + s.getAddress() + " type=" + s.getSymbolType());
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(s.getAddress());
            while (refs.hasNext()) {
                Reference r = refs.next();
                Address from = r.getFromAddress();
                Function f = getFunctionContaining(from);
                println("  xref from " + from + " in " + (f!=null?f.getName()+"@"+f.getEntryPoint():"???"));
            }
        }
        println("===SYMS_END===");
    }
}
