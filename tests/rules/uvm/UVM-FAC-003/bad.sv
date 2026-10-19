// Test Case: UVM-FAC-003 (UvmFactoryMissingUtils)
// Extracted from sample_tb.sv (Test Case 8)

`include "uvm_mock.svh"

// --- TEST CASE 8: UVM-FAC-003 (Concrete class missing factory utils) ---
class UnregisteredComponent extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    // VIOLATION UVM-FAC-003: No `uvm_component_utils declaration!
endclass
