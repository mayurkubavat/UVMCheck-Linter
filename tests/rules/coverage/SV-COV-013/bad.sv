// Test Case: SV-COV-013 (IgnoreBinsOverlapsIllegalBins)
// Extracted from sample_tb.sv (Test Case 196)

`include "uvm_mock.svh"

// --- TEST CASE 196: SV-COV-013 (IgnoreBinsOverlapsIllegalBins) ---
class OverlappingBinsBench;
    bit [3:0] status_code;
    covergroup cg_overlap_bins;
        cp_status: coverpoint status_code {
            // VIOLATION SV-COV-013: Value 1 overlaps between ignore_bins and illegal_bins!
            ignore_bins ign = { 1 };
            illegal_bins ill = { 1 };
        }
    endgroup : cg_overlap_bins
    function new();
        cg_overlap_bins = new();
    endfunction : new
endclass : OverlappingBinsBench
