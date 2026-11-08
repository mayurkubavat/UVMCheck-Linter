// Test Case: UVM-STY-003 (ComponentNameMismatch)
// Extracted from sample_tb.sv (Test Case 108)

`include "uvm_mock.svh"

// --- TEST CASE 108: UVM-STY-003 (ComponentNameMismatch) ---
class HardcodedNameAgent extends uvm_agent;
    `uvm_component_utils(HardcodedNameAgent)
    // VIOLATION UVM-STY-003: Passes hardcoded literal "hardcoded_agent" instead of parameter 'name'!
    function new(string name, uvm_component parent);
        super.new("hardcoded_agent", parent);
    endfunction
endclass
