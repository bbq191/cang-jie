// Look for direct/recorded references to FUN_00f401f0 (VaryingGenerator_WidthLength::generate)
// and to its vtable slot (0x016d2380), since it's invoked through a vtable (indirect call) and
// Ghidra's automatic xref analysis may or may not have resolved the devirtualized target.
// Also dump the sibling vtable at 0x016d2398 (likely VaryingGenerator_AA or _ThresholdAndWidth)
// for comparison, and decompile CoverageBuffer's vtable slot functions (0x016d2320) since that's
// the most likely place a scanline rasterizer loop lives that would call generate() per pixel.
//@category CangJie

import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.util.task.ConsoleTaskMonitor;

public class FindGenerateCallSite extends GhidraScript {

    private DecompInterface decomp;

    @Override
    public void run() throws Exception {
        decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        println("### xrefs to FUN_00f401f0 (generate itself) ###");
        dumpRefs("00f401f0");

        println("### xrefs to vtable slot 0x016d2380 (the pointer entry) ###");
        dumpRefs("016d2380");

        println("### xrefs to vtable start 0x016d2370 ###");
        dumpRefs("016d2370");

        // CoverageBuffer vtable first slot (business method after the two dtor slots)
        println("### CoverageBuffer vtable @ 0x016d2320, business method slot (0x016d2338) ###");
        dumpVtableSlots("016d2320", 6);
    }

    private void dumpRefs(String addrStr) throws Exception {
        Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(addrStr);
        ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(addr);
        boolean any = false;
        while (refs.hasNext()) {
            any = true;
            Reference r = refs.next();
            Address from = r.getFromAddress();
            Function caller = getFunctionContaining(from);
            println("  " + from + " in " + (caller != null ? caller.getName() : "???") + " (" + r.getReferenceType() + ")");
        }
        if (!any) println("  (none)");
        println("");
    }

    private void dumpVtableSlots(String startAddr, int count) throws Exception {
        Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(startAddr);
        for (int i = 0; i < count; i++) {
            Address slot = addr.add((long) i * 8);
            Address target = getAddressFromPointer(slot);
            println("  slot " + i + " @ " + slot + " -> " + target);
        }
    }

    private Address getAddressFromPointer(Address slotAddr) throws Exception {
        byte[] bytes = new byte[8];
        currentProgram.getMemory().getBytes(slotAddr, bytes);
        long val = 0;
        for (int i = 7; i >= 0; i--) {
            val = (val << 8) | (bytes[i] & 0xFF);
        }
        return currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(val);
    }
}
