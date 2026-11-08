// Test Case: UVM-TLM-001 (UvmUnconnectedTlmPort)
// Extracted from sample_tb.sv (Test Case 17)

`include "uvm_mock.svh"

// --- TEST CASE 17: UVM-TLM-001 (Unconnected TLM Port) ---
class UnconnectedPortAgent extends uvm_component;
    `uvm_component_utils(UnconnectedPortAgent)
    // VIOLATION UVM-TLM-001: TLM port 'm_item_port' is declared but never connected via .connect(...)!
    uvm_analysis_port#(int) m_item_port;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass
