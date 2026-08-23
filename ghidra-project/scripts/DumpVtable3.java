import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.listing.Function;

public class DumpVtable3 extends GhidraScript {
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        long anchor = 0x1266fd0L; // qt_metacast slot, same anchor as before
        println("=== vtable slots 11..70 ===");
        for (int i = 11; i <= 70; i++) {
            long slotAddr = anchor + i * 8L;
            Address a = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(slotAddr);
            long val = mem.getLong(a);
            Address valAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(val);
            Function f = getFunctionAt(valAddr);
            println("  slot[" + i + "] @0x" + Long.toHexString(slotAddr) + " = 0x" + Long.toHexString(val)
                    + (f != null ? "  -> " + f.getName() : "  -> ???"));
        }
    }
}
