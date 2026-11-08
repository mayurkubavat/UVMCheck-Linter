// Test Case: UVM-STR-005 (ComponentNullParentInCreate)
// Extracted from sample_tb.sv (Test Case 221)

`include "uvm_mock.svh"

// --- TEST CASE 221: UVM-STR-005 (ComponentNullParentInCreate) ---
class MockSubAgent extends uvm_component;
    `uvm_component_utils(MockSubAgent)
    function new(string name = "MockSubAgent", uvm_component parent = null);
        super.new(name, parent);
    endfunction : new
endclass : MockSubAgent

class EnvOrphanBench extends uvm_component;
    `uvm_component_utils(EnvOrphanBench)
    MockSubAgent agent;
    function new(string name = "EnvOrphanBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction : new
    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION UVM-STR-005: Component factory creation with null parent!
        agent = MockSubAgent::type_id::create("agent", null);
    endfunction : build_phase
endclass : EnvOrphanBench
