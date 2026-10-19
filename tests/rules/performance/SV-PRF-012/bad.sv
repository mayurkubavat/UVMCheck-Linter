// Test Case: SV-PRF-012 (UnboundedDynamicArrayInConstraint)
// Extracted from sample_tb.sv (Test Case 214)

`include "uvm_mock.svh"

// --- TEST CASE 214: SV-PRF-012 (UnboundedDynamicArrayInConstraint) ---
class UnboundedArraySizeBench;
    rand int unsigned payload[];
    constraint c_unbounded_size {
        // VIOLATION SV-PRF-012: Dynamic array size has upper bound without lower bound!
        payload.size() < 64;
    }
endclass : UnboundedArraySizeBench
