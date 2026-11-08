// Test Case: SV-COV-008 (CrossWithEmptyBins)
// Extracted from sample_tb.sv (Test Case 156)

`include "uvm_mock.svh"

// --- TEST CASE 156: SV-COV-008 (CrossWithEmptyBins) ---
class DegenerateCrossBench;
    bit [1:0] single_signal;
    covergroup cg_degenerate;
        cp_sig: coverpoint single_signal;
        // VIOLATION SV-COV-008: Degenerate cross specifies fewer than 2 coverpoints!
        cr_single: cross cp_sig;
    endgroup
    function new();
        cg_degenerate = new();
    endfunction
endclass
