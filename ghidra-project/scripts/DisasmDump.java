import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;

public class DisasmDump extends GhidraScript {
    public void run() throws Exception {
        long[][] ranges = {
            {0x696b10L, 0x696b40L}, // call site around LAB_00696b30 in FUN_00695a60
            {0x695970L, 0x6959c0L}, // FUN_00695970 prologue
        };
        for (long[] r : ranges) {
            println("=== disasm 0x" + Long.toHexString(r[0]) + " - 0x" + Long.toHexString(r[1]) + " ===");
            Address start = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(r[0]);
            Address end = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(r[1]);
            InstructionIterator it = currentProgram.getListing().getInstructions(start, true);
            while (it.hasNext()) {
                Instruction ins = it.next();
                if (ins.getAddress().compareTo(end) > 0) break;
                println(ins.getAddress() + "  " + ins.toString());
            }
        }
    }
}
