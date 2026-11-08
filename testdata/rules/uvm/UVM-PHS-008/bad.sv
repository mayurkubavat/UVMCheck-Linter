// Test Case: UVM-PHS-008 (TimeInNonTimePhase)
// Extracted from sample_tb.sv (Test Case 161)

`include "uvm_mock.svh"

// --- TEST CASE 161: UVM-PHS-008 (TimeInNonTimePhase) ---
class TimeInBuildPhaseComponent extends uvm_component;
    `uvm_component_utils(TimeInBuildPhaseComponent)
    function new(string name = "TimeInBuildPhaseComponent", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION UVM-PHS-008: Delay control inside build_phase!
        #10ns;
    endfunction
endclass
