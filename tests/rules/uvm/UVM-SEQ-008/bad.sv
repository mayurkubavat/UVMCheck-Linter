// Test Case: UVM-SEQ-008 (SequenceItemDefaultNew)
// Extracted from sample_tb.sv (Test Case 168)

`include "uvm_mock.svh"

// --- TEST CASE 168: UVM-SEQ-008 (SequenceItemDefaultNew) ---
class UndefaultedNewItem extends uvm_sequence_item;
    `uvm_object_utils(UndefaultedNewItem)
    // VIOLATION UVM-SEQ-008: Sequence item constructor lacks default string name argument!
    function new(string name);
        super.new(name);
    endfunction
endclass
