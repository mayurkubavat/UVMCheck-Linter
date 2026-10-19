// Test Case: UVM-REP-005 (LowVerbosityDebugReport)
// Extracted from sample_tb.sv (Test Case 199)

`include "uvm_mock.svh"

// --- TEST CASE 199: UVM-REP-005 (LowVerbosityDebugReport) ---
class DebugVerbosityBench extends uvm_component;
    `uvm_component_utils(DebugVerbosityBench)
    function new(string name = "DebugVerbosityBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction : new
    function void report_state();
        // VIOLATION UVM-REP-005: Debug report message configured with UVM_LOW verbosity!
        uvm_report_info("DEBUG_PKT", "Tracing raw packet bytes", 100);
    endfunction : report_state
endclass : DebugVerbosityBench
