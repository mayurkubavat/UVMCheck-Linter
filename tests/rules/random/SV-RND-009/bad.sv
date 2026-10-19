// Test Case: SV-RND-009 (StaticConstraint)
// Extracted from sample_tb.sv (Test Case 103)

`include "uvm_mock.svh"

// --- TEST CASE 103: SV-RND-009 (StaticConstraint) ---
class StaticConstraintItem extends uvm_sequence_item;
    `uvm_object_utils(StaticConstraintItem)
    rand int len;
    // VIOLATION SV-RND-009: Static constraint leaks across all instances!
    static constraint c_global_len {
        len inside {[1:100]};
    }
    function new(string name = "StaticConstraintItem");
        super.new(name);
    endfunction
endclass
