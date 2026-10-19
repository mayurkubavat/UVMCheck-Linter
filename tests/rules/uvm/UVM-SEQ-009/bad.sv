// Test Case: UVM-SEQ-009 (SequenceBodyWithoutStartItem)
// Extracted from sample_tb.sv (Test Case 177)

`include "uvm_mock.svh"

// --- TEST CASE 177: UVM-SEQ-009 (SequenceBodyWithoutStartItem) ---
class UnstartedItemSeq extends uvm_sequence;
    `uvm_object_utils(UnstartedItemSeq)
    task body();
        MockTransaction tr = new();
        // VIOLATION UVM-SEQ-009: finish_item() called without preceding start_item()!
        finish_item(tr);
    endtask
endclass
