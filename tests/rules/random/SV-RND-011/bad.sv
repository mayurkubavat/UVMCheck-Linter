// Test Case: SV-RND-011 (RedundantConstraint)
// Extracted from sample_tb.sv (Test Case 105)

`include "uvm_mock.svh"

// --- TEST CASE 105: SV-RND-011 (RedundantConstraint) ---
class DuplicateConstraintBench;
    rand int burst_len;
    constraint c_limits {
        burst_len > 0;
        burst_len < 256;
        // VIOLATION SV-RND-011: Duplicate constraint expression!
        burst_len > 0;
    }
endclass
