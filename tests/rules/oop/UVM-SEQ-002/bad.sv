// Test Case: UVM-SEQ-002 (UvmSequencePrePostBodyMisuse)
// Extracted from sample_tb.sv (Test Case 29)

`include "uvm_mock.svh"

// --- TEST CASE 29: UVM-SEQ-002 (UvmSequencePrePostBodyMisuse) ---
class ChildNestedSequence extends uvm_sequence #(uvm_sequence_item);
    `uvm_object_utils(ChildNestedSequence)
    function new(string name = "ChildNestedSequence");
        super.new(name);
    endfunction

    // VIOLATION UVM-SEQ-002: pre_body() is skipped when invoked via `uvm_do!
    virtual task pre_body();
        $display("Initializing sequence preconditions");
    endtask

    virtual task body();
        $display("Sequence body executing");
    endtask
endclass
