import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.listing.Function;

public class DumpVtable2 extends GhidraScript {
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        long anchor = 0x1266fd0L;
        println("=== vtable around 0x" + Long.toHexString(anchor) + " (anchor slot = qt_metacast) ===");
        for (int i = -6; i <= 10; i++) {
            long slotAddr = anchor + i * 8L;
            Address a = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(slotAddr);
            long val = mem.getLong(a);
            Address valAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(val);
            Function f = getFunctionAt(valAddr);
            println("  slot[" + i + "] @0x" + Long.toHexString(slotAddr) + " = 0x" + Long.toHexString(val)
                    + (f != null ? "  -> " + f.getName() : ""));
        }
    }
}
