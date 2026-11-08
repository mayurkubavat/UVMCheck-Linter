// Test Case: SV-COV-002 (ZeroWeightCoverpoint)
// Extracted from sample_tb.sv (Test Case 112)

`include "uvm_mock.svh"

// --- TEST CASE 112: SV-COV-002 (ZeroWeightCoverpoint) ---
class ZeroWeightCoverBench;
    logic [3:0] mode;
    covergroup zero_weight_cg;
        cp_mode: coverpoint mode {
            // VIOLATION SV-COV-002: Zero-weight coverpoint!
            type_option.weight = 0;
        }
    endgroup
    function new();
        zero_weight_cg = new();
    endfunction
endclass
