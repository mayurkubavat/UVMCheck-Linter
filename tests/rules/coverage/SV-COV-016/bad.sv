// Test Case: SV-COV-016 (OverlappingRangesInCoverpointBin)
// Extracted from sample_tb.sv (Test Case 218)

`include "uvm_mock.svh"

// --- TEST CASE 218: SV-COV-016 (OverlappingRangesInCoverpointBin) ---
class OverlappingBinBench;
    bit [7:0] val;
    covergroup cg_overlap;
        cp_val: coverpoint val {
            // VIOLATION SV-COV-016: Overlapping ranges [0:10] and [5:15] in same bin!
            bins b_overlap = {[0:10], [5:15]};
        }
    endgroup : cg_overlap
    function new(); cg_overlap = new(); endfunction : new
endclass : OverlappingBinBench
