// Test Case: UVM-CFG-004 (ConfigDbGetUnchecked)
// Extracted from sample_tb.sv (Test Case 144)

`include "uvm_mock.svh"

// --- TEST CASE 144: UVM-CFG-004 (ConfigDbGetUnchecked) ---
class UncheckedConfigBench extends uvm_component;
    `uvm_component_utils(UncheckedConfigBench)
    int m_timeout;
    function new(string name = "UncheckedConfigBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION UVM-CFG-004: uvm_config_db::get() return value is ignored!
        uvm_config_db#(int)::get(this, "", "timeout", m_timeout);
    endfunction
endclass
