import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;

public class DumpMetaObject2 extends GhidraScript {
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        long[] candidates = { 0x10d4840L };
        for (long c : candidates) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(c);
            println("===META_PTR_RAW_START@0x" + Long.toHexString(c) + "===");
            for (int i = 0; i < 6; i++) {
                long val = mem.getLong(addr.add(i * 8L));
                println("d[" + i + "] (@" + addr.add(i*8L) + ") = 0x" + Long.toHexString(val));
            }
            println("===META_PTR_RAW_END===");

            long dataArrPtr = mem.getLong(addr.add(16));
            Address dataArrAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(dataArrPtr);
            println("===DATA_ARRAY_START===");
            for (int i = 0; i < 60; i++) {
                int v = mem.getInt(dataArrAddr.add(i * 4L));
                println("data[" + i + "] = " + v + " (0x" + Integer.toHexString(v) + ")");
            }
            println("===DATA_ARRAY_END===");

            long stringDataPtr = mem.getLong(addr.add(8));
            Address stringDataAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(stringDataPtr);
            println("===STRINGDATA_STRUCT_START stringdata@" + stringDataAddr + "===");
            for (int i = 0; i < 40; i++) {
                int v = mem.getInt(stringDataAddr.add(i * 4L));
                println("sdata[" + i + "] = " + v + " (0x" + Integer.toHexString(v) + ")");
            }
            println("===STRINGDATA_STRUCT_END===");
        }
    }
}
