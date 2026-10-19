// Test Case: SV-RND-012 (ZeroWeightDistribution)
// Extracted from sample_tb.sv (Test Case 119)

`include "uvm_mock.svh"

// --- TEST CASE 119: SV-RND-012 (ZeroWeightDistribution) ---
class ZeroWeightDistBench;
    rand int unsigned opcode;
    constraint c_bad_dist {
        // VIOLATION SV-RND-012: All distribution weights evaluate to zero!
        opcode dist { 0 := 0, 1 := 0, 2 := 0 };
    }
endclass
