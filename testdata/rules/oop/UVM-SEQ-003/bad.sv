// Test Case: UVM-SEQ-003 (UvmPSequencerAbuse)
// Extracted from sample_tb.sv (Test Case 30)

`include "uvm_mock.svh"

// --- TEST CASE 30: UVM-SEQ-003 (UvmPSequencerAbuse) ---
class SequencerCoupledSequence extends uvm_sequence #(uvm_sequence_item);
    `uvm_object_utils(SequencerCoupledSequence)
    // VIOLATION UVM-SEQ-003: p_sequencer declaration couples sequence to component tier!
    uvm_component p_sequencer;

    function new(string name = "SequencerCoupledSequence");
        super.new(name);
    endfunction

    virtual task body();
        $display("Running coupled sequence");
    endtask
endclass
