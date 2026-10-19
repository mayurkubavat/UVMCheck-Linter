// Test Case: UVM-SEQ-001 (UvmSequenceDirectVifAccess)
// Extracted from sample_tb.sv (Test Case 10)

`include "uvm_mock.svh"

// --- TEST CASE 10: UVM-SEQ-001 (Sequence Direct Virtual Interface Access) ---
class BadBusSequence extends uvm_sequence;
    `uvm_object_utils(BadBusSequence)
    function new(string name = "");
        super.new(name);
    endfunction

    // VIOLATION UVM-SEQ-001: Sequence directly declaring virtual interface
    virtual bus_if vif;
endclass

// Compliant driver permitted to access virtual interface
class CompliantDriver extends uvm_component;
    `uvm_component_utils(CompliantDriver)
    virtual bus_if vif; // Compliant in uvm_component (driver)

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass
