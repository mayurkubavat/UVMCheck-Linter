// Test Case: SV-RND-015 (UnconstrainedRandVariable)
// Extracted from sample_tb.sv (Test Case 154)

`include "uvm_mock.svh"

// --- TEST CASE 154: SV-RND-015 (UnconstrainedRandVariable) ---
class UnconstrainedRandBench;
    rand int constrained_field;
    // VIOLATION SV-RND-015: rand variable is never referenced in any constraint!
    rand bit [7:0] unconstrained_mode;
    constraint c_field { constrained_field inside {[0:100]}; }
endclass
