// Test Case: UVM-PHS-006 (PhaseObjectionMismatch)
// Extracted from sample_tb.sv (Test Case 142)

`include "uvm_mock.svh"

// --- TEST CASE 142: UVM-PHS-006 (PhaseObjectionMismatch) ---
class ObjectionHangBench extends uvm_component;
    `uvm_component_utils(ObjectionHangBench)
    function new(string name = "ObjectionHangBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        // VIOLATION UVM-PHS-006: raise_objection without matching drop_objection hangs phase!
        phase.raise_objection(this);
        #100;
    endtask
endclass
