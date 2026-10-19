// Test Case: UVM-PHS-005 (SuperBuildPhaseOrder)
// Extracted from sample_tb.sv (Test Case 107)

`include "uvm_mock.svh"

// --- TEST CASE 107: UVM-PHS-005 (SuperBuildPhaseOrder) ---
class BadBuildOrderEnv extends uvm_env;
    `uvm_component_utils(BadBuildOrderEnv)
    int mode;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void build_phase(uvm_phase phase);
        // VIOLATION UVM-PHS-005: Statement precedes super.build_phase!
        mode = 5;
        super.build_phase(phase);
    endfunction
endclass
