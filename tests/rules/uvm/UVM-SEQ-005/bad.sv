// Test Case: UVM-SEQ-005 (EmptySequenceBody)
// Extracted from sample_tb.sv (Test Case 109)

`include "uvm_mock.svh"

// --- TEST CASE 109: UVM-SEQ-005 (EmptySequenceBody) ---
class EmptyStimulusSequence extends uvm_sequence;
    `uvm_object_utils(EmptyStimulusSequence)
    function new(string name = "EmptyStimulusSequence");
        super.new(name);
    endfunction
    // VIOLATION UVM-SEQ-005: Empty body task generates zero stimulus!
    task body();
    endtask
endclass
