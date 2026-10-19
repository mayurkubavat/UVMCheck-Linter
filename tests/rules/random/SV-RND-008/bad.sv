// Test Case: SV-RND-008 (NonInvertibleConstraintOp)
// Extracted from sample_tb.sv (Test Case 95)

`include "uvm_mock.svh"

// --- TEST CASE 95: SV-RND-008 (NonInvertibleConstraintOp) ---
class NonInvertibleConstraintBench;
    rand int length;
    rand int divisor;
    // VIOLATION SV-RND-008: Non-invertible modulo and division operators inside constraint!
    constraint c_alignment {
        length % 4 == 0;
        divisor / 2 > 10;
    }
endclass
