// Test Case: UVM-FAC-002 (UvmFactoryMacroMismatch)
// Extracted from sample_tb.sv (Test Case 6)

`include "uvm_mock.svh"

// --- TEST CASE 6: UVM-FAC-002 (Factory Macro Mismatch) ---
class MismatchedComponent extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION UVM-FAC-002: uvm_component declaring uvm_object_utils
    `uvm_object_utils(MismatchedComponent)
endclass

class MismatchedObject extends uvm_object;
    function new(string name = "");
        super.new(name);
    endfunction

    // VIOLATION UVM-FAC-002: uvm_object declaring uvm_component_utils
    `uvm_component_utils(MismatchedObject)
endclass
