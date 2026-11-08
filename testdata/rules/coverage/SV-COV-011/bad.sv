// Test Case: SV-COV-011 (AtLeastZeroOrNegative)
// Extracted from sample_tb.sv (Test Case 179)

`include "uvm_mock.svh"

// --- TEST CASE 179: SV-COV-011 (AtLeastZeroOrNegative) ---
class AtLeastZeroBench;
    bit [3:0] sig_data;
    covergroup cg_at_least;
        // VIOLATION SV-COV-011: option.at_least must be >= 1!
        option.at_least = 0;
        cp_data: coverpoint sig_data;
    endgroup
    function new();
        cg_at_least = new();
    endfunction
endclass
