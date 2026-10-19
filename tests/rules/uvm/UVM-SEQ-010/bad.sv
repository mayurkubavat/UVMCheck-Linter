// Test Case: UVM-SEQ-010 (SequenceItemCreateWithoutType)
// Extracted from sample_tb.sv (Test Case 186)

`include "uvm_mock.svh"

// --- TEST CASE 186: UVM-SEQ-010 (SequenceItemCreateWithoutType) ---
class DirectNewItemSeq extends uvm_sequence;
    `uvm_object_utils(DirectNewItemSeq)
    task body();
        MockTransaction req;
        // VIOLATION UVM-SEQ-010: Sequence item created with direct new() instead of factory create()!
        req = new();
    endtask
endclass
