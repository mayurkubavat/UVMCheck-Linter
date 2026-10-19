// Test Case: UVM-PHS-009 (ObjectionInNonRunPhase)
// Extracted from sample_tb.sv (Test Case 191)

`include "uvm_mock.svh"

// --- TEST CASE 191: UVM-PHS-009 (ObjectionInNonRunPhase) ---
class ObjectionInConnectComponent extends uvm_component;
    `uvm_component_utils(ObjectionInConnectComponent)
    function new(string name = "ObjectionInConnectComponent", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    function void connect_phase(uvm_phase phase);
        super.connect_phase(phase);
        // VIOLATION UVM-PHS-009: Calling raise_objection() inside synchronous function phase!
        phase.raise_objection(this);
    endfunction
endclass
