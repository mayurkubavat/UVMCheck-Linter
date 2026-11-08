// Test Case: SV-COV-003 (CrossSameCoverpoint)
// Extracted from sample_tb.sv (Test Case 102)

`include "uvm_mock.svh"

// --- TEST CASE 102: SV-COV-003 (CrossSameCoverpoint) ---
class RedundantCrossBench;
    int addr;
    covergroup cg;
        cp_addr: coverpoint addr;
        // VIOLATION SV-COV-003: Crossing coverpoint 'cp_addr' with itself!
        cr_same: cross cp_addr, cp_addr;
    endgroup
    function new();
        cg = new();
    endfunction
endclass
