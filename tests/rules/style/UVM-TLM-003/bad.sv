// Test Case: UVM-TLM-003 (UvmTlmPortSuffix)
// Extracted from sample_tb.sv (Test Case 183)

`include "uvm_mock.svh"

// --- TEST CASE 183: UVM-TLM-003 (UvmTlmPortSuffix) ---
class BadTlmNamingComponent extends uvm_component;
    `uvm_component_utils(BadTlmNamingComponent)
    // VIOLATION UVM-TLM-003: uvm_analysis_port without _ap or _port suffix!
    uvm_analysis_port #(MockTransaction) tx_data_out;
    function new(string name = "BadTlmNamingComponent", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass
