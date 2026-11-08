// Test Case: UVM-FAC-004 (ParameterizedClassFactoryRegistration)
// Extracted from sample_tb.sv (Test Case 204)

`include "uvm_mock.svh"

// --- TEST CASE 204: UVM-FAC-004 (ParameterizedClassFactoryRegistration) ---
class ParamDriverBench#(type REQ_T = MockTransaction) extends uvm_driver#(REQ_T);
    // VIOLATION UVM-FAC-004: Parameterized class uses `uvm_component_utils instead of param_utils!
    `uvm_component_utils(ParamDriverBench)
    function new(string name = "ParamDriverBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction : new
endclass : ParamDriverBench
