// Test Case: UVM-CFG-003 (UvmConfigDbGlobalWildcard)
// Extracted from sample_tb.sv (Test Case 115)

`include "uvm_mock.svh"

// --- TEST CASE 115: UVM-CFG-003 (UvmConfigDbGlobalWildcard) ---
class GlobalWildcardBench extends uvm_test;
    `uvm_component_utils(GlobalWildcardBench)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION UVM-CFG-003: Unbounded global config_db scope with null context and "*"!
        uvm_config_db#(int)::set(null, "*", "global_timeout", 5000);
    endfunction
endclass
