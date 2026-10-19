// Test Case: UVM-SEQ-011 (UncheckedPSequencerCast)
// Extracted from sample_tb.sv (Test Case 205)

`include "uvm_mock.svh"

// --- TEST CASE 205: UVM-SEQ-011 (UncheckedPSequencerCast) ---
class MockVirtualSequencer extends uvm_component;
    `uvm_component_utils(MockVirtualSequencer)
    function new(string name = "MockVirtualSequencer", uvm_component parent = null);
        super.new(name, parent);
    endfunction : new
endclass : MockVirtualSequencer

class UncheckedPSeqBench extends uvm_sequence;
    `uvm_object_utils(UncheckedPSeqBench)
    MockVirtualSequencer p_sequencer;
    task body();
        // VIOLATION UVM-SEQ-011: $cast to p_sequencer without return value check!
        void'($cast(p_sequencer, m_sequencer));
    endtask : body
endclass : UncheckedPSeqBench
