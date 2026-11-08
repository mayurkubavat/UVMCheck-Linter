// Test Case: UVM-SEQ-007 (SequencePriorityRange)
// Extracted from sample_tb.sv (Test Case 153)

`include "uvm_mock.svh"

// --- TEST CASE 153: UVM-SEQ-007 (SequencePriorityRange) ---
class SequencePriorityBench extends uvm_sequence;
    `uvm_object_utils(SequencePriorityBench)
    task run_seq(uvm_sequencer sqr);
        SequencePriorityBench child_seq = new();
        // VIOLATION UVM-SEQ-007: Invalid negative priority -5 corrupts arbitration!
        child_seq.start(sqr, this, -5);
    endtask
endclass
