import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.address.Address;
import java.util.Iterator;

public class FindFindChildXref2 extends GhidraScript {
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        println("===THUNKS_START===");
        Iterator<Function> it = fm.getFunctions(true).iterator();
        while (it.hasNext()) {
            Function f = it.next();
            if (f.getName().contains("qFindChildren_helper")) {
                println("func: " + f.getName() + " @ " + f.getEntryPoint() + " thunk=" + f.isThunk());
                ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(f.getEntryPoint());
                int n=0;
                while (refs.hasNext()) {
                    Reference r = refs.next();
                    Address from = r.getFromAddress();
                    Function caller = getFunctionContaining(from);
                    println("  xref from " + from + " in " + (caller!=null?caller.getName()+"@"+caller.getEntryPoint():"???"));
                    n++;
                }
                println("  total xrefs: " + n);
            }
        }
        println("===THUNKS_END===");
    }
}
