// Test Case: SV-PRF-009 (NbaOnClassProperty)
// Extracted from sample_tb.sv (Test Case 87)

`include "uvm_mock.svh"

// --- TEST CASE 87: SV-PRF-009 (NbaOnClassProperty) ---
class NbaClassBench extends uvm_component;
    `uvm_component_utils(NbaClassBench)
    int m_counter;
    function new(string name = "NbaClassBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        // VIOLATION SV-PRF-009: Non-blocking assignment on heap-allocated class property!
        m_counter <= m_counter + 1;
    endtask
endclass
