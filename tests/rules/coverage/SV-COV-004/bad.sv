// Test Case: SV-COV-004 (IllegalBinOverlap)
// Extracted from sample_tb.sv (Test Case 120)

`include "uvm_mock.svh"

// --- TEST CASE 120: SV-COV-004 (IllegalBinOverlap) ---
class IllegalBinOverlapBench;
    logic [2:0] status;
    covergroup status_cg;
        cp_status: coverpoint status {
            bins normal_states = {0, 1, 2, 3};
            // VIOLATION SV-COV-004: Illegal bin overlaps with regular bin value 2!
            illegal_bins err_state = {2};
        }
    endgroup
    function new();
        status_cg = new();
    endfunction
endclass
