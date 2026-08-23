import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;

public class DisasmDump2 extends GhidraScript {
    public void run() throws Exception {
        long[][] ranges = { {0x695820L, 0x695850L} };
        for (long[] r : ranges) {
            println("=== disasm 0x" + Long.toHexString(r[0]) + " - 0x" + Long.toHexString(r[1]) + " ===");
            Address start = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(r[0]);
            Address end = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(r[1]);
            InstructionIterator it = currentProgram.getListing().getInstructions(start, true);
            while (it.hasNext()) {
                Instruction ins = it.next();
                if (ins.getAddress().compareTo(end) > 0) break;
                println(ins.getAddress() + "  " + ins.toString() + "  bytes=" + ins.getBytes().length);
            }
        }
    }
}
