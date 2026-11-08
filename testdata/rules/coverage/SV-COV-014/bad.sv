// Test Case: SV-COV-014 (CoverpointWeightZero)
// Extracted from sample_tb.sv (Test Case 206)

`include "uvm_mock.svh"

// --- TEST CASE 206: SV-COV-014 (CoverpointWeightZero) ---
class ZeroWeightOptionBench;
    bit [3:0] opcode;
    covergroup cg_zero_wt;
        // VIOLATION SV-COV-014: Covergroup option.weight set to 0!
        option.weight = 0;
        cp_op: coverpoint opcode {
            // VIOLATION SV-COV-014: Coverpoint option.weight set to 0!
            option.weight = 0;
        }
    endgroup : cg_zero_wt
    function new();
        cg_zero_wt = new();
    endfunction : new
endclass : ZeroWeightOptionBench
