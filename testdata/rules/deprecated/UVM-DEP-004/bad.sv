// Test Case: UVM-DEP-004 (UvmDeprecatedTestDone)
// Extracted from sample_tb.sv (Test Case 57)

`include "uvm_mock.svh"

// --- TEST CASE 57: UVM-DEP-004 (UVM Deprecated Test Done Objection) ---
class DeprecatedTestDoneComponent extends uvm_component;
    `uvm_component_utils(DeprecatedTestDoneComponent)
    uvm_test_done_objection uvm_test_done;

    function new(string name, uvm_component parent);
        super.new(name, parent);
        uvm_test_done = new();
    endfunction

    task run_phase(uvm_phase phase);
        // VIOLATION UVM-DEP-004: uvm_test_done is obsolete in IEEE 1800.2!
        uvm_test_done.raise_objection(this);
    endtask
endclass
