// Test Case: SV-NAM-005 (UvmClassTypeSuffix)
// Extracted from sample_tb.sv (Test Case 163)

`include "uvm_mock.svh"

// --- TEST CASE 163: SV-NAM-005 (UvmClassTypeSuffix) ---
// VIOLATION SV-NAM-005: uvm_driver without _driver or _drv suffix!
class BadlyNamedTransmitter extends uvm_driver;
    `uvm_component_utils(BadlyNamedTransmitter)
    function new(string name = "BadlyNamedTransmitter", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass
