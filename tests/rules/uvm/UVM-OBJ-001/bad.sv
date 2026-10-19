// Test Case: UVM-OBJ-001 (UvmUnpairedObjection)
// Extracted from sample_tb.sv (Test Case 11)

`include "uvm_mock.svh"

// --- TEST CASE 11: UVM-OBJ-001 (Unpaired objection) ---
class HangingObjectionComponent extends uvm_component;
    `uvm_component_utils(HangingObjectionComponent)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task run_phase(uvm_phase phase);
        super.run_phase(phase);
        // VIOLATION UVM-OBJ-001: raise_objection called without matching drop_objection!
        phase.raise_objection(this);
        #100;
    endtask
endclass
