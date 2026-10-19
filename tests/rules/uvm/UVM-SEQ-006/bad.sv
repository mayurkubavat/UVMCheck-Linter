// Test Case: UVM-SEQ-006 (UncheckedGetResponse)
// Extracted from sample_tb.sv (Test Case 121)

`include "uvm_mock.svh"

// --- TEST CASE 121: UVM-SEQ-006 (UncheckedGetResponse) ---
class DeadlockSequence extends uvm_sequence;
    `uvm_object_utils(DeadlockSequence)
    function new(string name = "DeadlockSequence");
        super.new(name);
    endfunction
    task body();
        uvm_sequence_item rsp;
        // VIOLATION UVM-SEQ-006: Unchecked get_response blocks sequence!
        get_response(rsp);
    endtask
endclass
