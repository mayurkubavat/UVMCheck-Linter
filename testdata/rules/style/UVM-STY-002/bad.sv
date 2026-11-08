// Test Case: UVM-STY-002 (ConstraintNameSuffix)
// Extracted from sample_tb.sv (Test Case 40)

`include "uvm_mock.svh"

// --- TEST CASE 40: UVM-STY-002 (Constraint Suffix '_c') ---
class SuffixConstraintItem extends uvm_sequence_item;
    `uvm_object_utils(SuffixConstraintItem)
    rand bit [7:0] length;

    function new(string name = "SuffixConstraintItem");
        super.new(name);
    endfunction

    // VIOLATION UVM-STY-002: Constraint name 'valid_length' does not end with '_c'!
    constraint valid_length {
        length inside {[1:64]};
    }
endclass
