// Test Case: SV-RND-005 (NonRandFieldConstrained)
// Extracted from sample_tb.sv (Test Case 66)

`include "uvm_mock.svh"

// --- TEST CASE 66: SV-RND-005 (Phantom Constraint on Non-Rand Field) ---
class NonRandConstrainedItem;
    // VIOLATION SV-RND-005: length is constrained below but declared without 'rand'!
    int length;
    constraint c_len { length inside {[10:50]}; }
endclass
