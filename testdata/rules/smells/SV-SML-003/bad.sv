// Test Case: SV-SML-003 (GenericReportId)
// Extracted from sample_tb.sv (Test Case 90)

`include "uvm_mock.svh"

// --- TEST CASE 90: SV-SML-003 (GenericReportId) ---
class GenericReportIdBench extends uvm_component;
    `uvm_component_utils(GenericReportIdBench)
    function new(string name = "GenericReportIdBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    function void report_something();
        // VIOLATION SV-SML-003: Generic single-word report ID 'ERR'!
        uvm_report_info("ERR", "Hardware timed out", 100);
    endfunction
endclass
