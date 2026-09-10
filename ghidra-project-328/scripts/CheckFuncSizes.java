// Check the byte-size of candidate hook targets — patch_target needs >=20 bytes
// of safely-patchable prologue (5-instruction far jump). Also dump the raw
// first-N instructions so we can hand-verify against the decompile (project
// rule: cross-check decompiler against raw disassembly for critical offsets).
//@category CangJie

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;

public class CheckFuncSizes extends GhidraScript {

    @Override
    public void run() throws Exception {
        String[] targets = {
            "00f4f430", // bVar16==3 branch geometry generator (x,y,ctx / ctx+4=width, same as f47530)
            "00f4c8d0", // bVar16==5/6 branch geometry generator (x,y,ctx / ctx+4=width, same as f47530)
        };

        for (String t : targets) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(t);
            Function f = getFunctionAt(addr);
            if (f == null) {
                println(t + ": NO FUNCTION");
                continue;
            }
            long size = f.getBody().getNumAddresses();
            println("=== " + f.getName() + " @ " + addr + "  body_size=" + size + " bytes ===");

            // dump first 8 instructions raw
            InstructionIterator it = currentProgram.getListing().getInstructions(addr, true);
            int n = 0; int MAXN = 20;
            while (it.hasNext() && n < MAXN) {
                Instruction insn = it.next();
                println("  " + insn.getAddress() + ": " + insn.toString() + "   [" + bytesToHex(insn) + "]");
                n++;
            }
            println("");
        }
    }

    private String bytesToHex(Instruction insn) throws Exception {
        byte[] b = insn.getBytes();
        StringBuilder sb = new StringBuilder();
        for (byte x : b) sb.append(String.format("%02x ", x));
        return sb.toString().trim();
    }
}
