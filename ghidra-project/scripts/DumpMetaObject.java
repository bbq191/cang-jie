import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;

public class DumpMetaObject extends GhidraScript {
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(0x1255730L);
        println("===META_PTR_RAW_START===");
        // QMetaObject::d 结构大致是 6 个指针：superdata, stringdata, data, static_metacall, relatedMetaObjects, extradata
        for (int i = 0; i < 6; i++) {
            long val = mem.getLong(addr.add(i * 8L));
            println("d[" + i + "] (@" + addr.add(i*8L) + ") = 0x" + Long.toHexString(val));
        }
        println("===META_PTR_RAW_END===");

        // 假设 d[2] 是 data (uint32数组)，d[1] 是 stringdata blob 的指针结构
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
        // Qt6 新版 stringdata 结构一般是 { offsets[N] (int), 后面紧跟 char blob }
        for (int i = 0; i < 40; i++) {
            int v = mem.getInt(stringDataAddr.add(i * 4L));
            println("sdata[" + i + "] = " + v + " (0x" + Integer.toHexString(v) + ")");
        }
        println("===STRINGDATA_STRUCT_END===");
    }
}
