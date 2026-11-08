// Test Case: SV-COV-007 (EmptyCovergroup)
// Extracted from sample_tb.sv (Test Case 145)

`include "uvm_mock.svh"

// --- TEST CASE 145: SV-COV-007 (EmptyCovergroup) ---
class EmptyCgBench;
    // VIOLATION SV-COV-007: Covergroup declared with zero coverpoints or crosses!
    covergroup cg_empty;
    endgroup
    function new();
        cg_empty = new();
    endfunction
endclass
