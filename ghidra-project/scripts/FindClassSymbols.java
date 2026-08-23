import ghidra.app.script.GhidraScript;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolTable;
import ghidra.program.model.symbol.SymbolIterator;

public class FindClassSymbols extends GhidraScript {
    public void run() throws Exception {
        SymbolTable st = currentProgram.getSymbolTable();
        String[] keywords = { "EpubProperties", "epub" };
        println("===SYMBOLS_START===");
        SymbolIterator it = st.getSymbolIterator();
        int count = 0;
        while (it.hasNext()) {
            Symbol s = it.next();
            String name = s.getName(true);
            for (String kw : keywords) {
                if (name.contains(kw)) {
                    println(s.getSymbolType() + " | " + s.getAddress() + " | " + name);
                    count++;
                    break;
                }
            }
        }
        println("total=" + count);
        println("===SYMBOLS_END===");
    }
}
