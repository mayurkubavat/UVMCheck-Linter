// Test Case: SV-NAM-001 (UvmComponentNamingConvention)
// Extracted from sample_tb.sv (Test Case 117)

`include "uvm_mock.svh"

// --- TEST CASE 117: SV-NAM-001 (UvmComponentNamingConvention) ---
// VIOLATION SV-NAM-001: uvm_driver subclass lacking _driver suffix!
class LegacyPcieTx extends uvm_driver;
    `uvm_component_utils(LegacyPcieTx)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass
