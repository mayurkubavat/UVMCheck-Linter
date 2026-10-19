// Test Case: SV-COV-009 (CoverpointDefaultBinHazard)
// Extracted from sample_tb.sv (Test Case 165)

`include "uvm_mock.svh"

// --- TEST CASE 165: SV-COV-009 (CoverpointDefaultBinHazard) ---
class DefaultBinBench;
    bit [3:0] val;
    covergroup cg_default;
        cp_val: coverpoint val {
            bins low = {[0:3]};
            // VIOLATION SV-COV-009: Non-ignoring default bin catches everything!
            bins other = default;
        }
    endgroup
    function new();
        cg_default = new();
    endfunction
endclass
