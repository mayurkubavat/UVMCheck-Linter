// Test Case: SV-RND-017 (RandcStateExhaustion)
// Extracted from sample_tb.sv (Test Case 176)

`include "uvm_mock.svh"

// --- TEST CASE 176: SV-RND-017 (RandcStateExhaustion) ---
class NarrowRandcBench;
    // VIOLATION SV-RND-017: randc declared on 1-bit variable (only 2 states)!
    randc bit flag_mode;
    // VIOLATION SV-RND-017: randc declared on 2-bit variable (only 4 states)!
    randc bit [1:0] state_sel;
endclass
