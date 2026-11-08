// Test Case: UVM-SEQ-012 (SequenceStartOnNullSequencer)
// Extracted from sample_tb.sv (Test Case 215)

`include "uvm_mock.svh"

// --- TEST CASE 215: UVM-SEQ-012 (SequenceStartOnNullSequencer) ---
class SeqStartNullBench extends uvm_component;
    `uvm_component_utils(SeqStartNullBench)
    function new(string name = "SeqStartNullBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction : new
    task run_phase(uvm_phase phase);
        uvm_sequence seq = new();
        // VIOLATION UVM-SEQ-012: Starting sequence with null sequencer handle!
        seq.start(null);
    endtask : run_phase
endclass : SeqStartNullBench
