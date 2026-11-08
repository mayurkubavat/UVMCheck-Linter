// Test Case: SV-OOP-003 (MissingSuperNew)
// Extracted from sample_tb.sv (Test Case 2)

`include "uvm_mock.svh"

// --- TEST CASE 2: SV-OOP-003 (Missing super.new) ---
class BadConstructorAgent extends uvm_component;
    // VIOLATION SV-OOP-003: Derived from uvm_component but omits super.new
    function new(string name, uvm_component parent);
        // super.new(name, parent) missing!
    endfunction
endclass
