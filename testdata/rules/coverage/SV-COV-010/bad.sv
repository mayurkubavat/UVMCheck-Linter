// Test Case: SV-COV-010 (CrossWithAllBinsIgnored)
// Extracted from sample_tb.sv (Test Case 170)

`include "uvm_mock.svh"

// --- TEST CASE 170: SV-COV-010 (CrossWithAllBinsIgnored) ---
class AllIgnoredCrossBench;
    bit [1:0] a, b;
    covergroup cg_all_ignored;
        cp_a: coverpoint a;
        cp_b: coverpoint b;
        // VIOLATION SV-COV-010: Cross with only ignore_bins and zero monitored bins!
        cx_all_ign: cross cp_a, cp_b {
            ignore_bins ig = binsof(cp_a);
        }
    endgroup
    function new();
        cg_all_ignored = new();
    endfunction
endclass
