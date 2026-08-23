import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;

public class DecodeMetaStrings2 extends GhidraScript {
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        long ma = 0x12677e8L;
        Address metaAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(ma);
        println("=========== metaobject @0x" + Long.toHexString(ma) + " ===========");

        long dataArrPtr = mem.getLong(metaAddr.add(16));
        Address dataArrAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(dataArrPtr);
        println("--- data[] (moc header, first 20) ---");
        for (int i = 0; i < 20; i++) {
            int v = mem.getInt(dataArrAddr.add(i * 4L));
            println("data[" + i + "] = " + v);
        }

        long stringDataPtr = mem.getLong(metaAddr.add(8));
        Address sdAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(stringDataPtr);

        println("--- decoded string table ---");
        for (int i = 0; i < 250; i++) {
            int off = mem.getInt(sdAddr.add(i * 8L));
            int len = mem.getInt(sdAddr.add(i * 8L + 4L));
            if (off == 0 && len == 0 && i > 0) continue;
            if (len < 0 || len > 200 || off < 0 || off > 100000) { println("idx=" + i + " STOP (implausible off/len " + off + "/" + len + ")"); break; }
            Address charAddr = sdAddr.add(off);
            byte[] buf = new byte[len];
            try {
                mem.getBytes(charAddr, buf);
            } catch (Exception e) {
                println("idx=" + i + " read error " + e);
                continue;
            }
            println("idx=" + i + " off=" + off + " len=" + len + " -> \"" + new String(buf, "UTF-8") + "\"");
        }
    }
}
