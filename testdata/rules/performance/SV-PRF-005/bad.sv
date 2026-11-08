// Test Case: SV-PRF-005 (UnshieldedVerbosityFormatEval)
// Extracted from sample_tb.sv (Test Case 86)

`include "uvm_mock.svh"

// --- TEST CASE 86: SV-PRF-005 (UnshieldedVerbosityFormatEval) ---
class UnshieldedFormatBench extends uvm_component;
    `uvm_component_utils(UnshieldedFormatBench)
    function new(string name = "UnshieldedFormatBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    function void log_data(int pkt_id);
        // VIOLATION SV-PRF-005: Eager $sformatf evaluation on discarded UVM_HIGH message!
        uvm_report_info("BURST_TR", $sformatf("Packet ID = %0d", pkt_id), 300);
    endfunction
endclass
