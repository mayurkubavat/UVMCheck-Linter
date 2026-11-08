// Test Case: SV-RND-021 (ConstraintInsideInvertedRange)
// Extracted from sample_tb.sv (Test Case 210)

`include "uvm_mock.svh"

// --- TEST CASE 210: SV-RND-021 (ConstraintInsideInvertedRange) ---
class InvertedRangeBench;
    rand int unsigned offset;
    constraint c_inv_range {
        // VIOLATION SV-RND-021: Inverted range [100:10] (low > high)!
        offset inside {[100 : 10]};
    }
endclass : InvertedRangeBench
