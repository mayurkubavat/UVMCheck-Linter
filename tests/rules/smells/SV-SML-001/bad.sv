// Test Case: SV-SML-001 (ScratchpadClassProperty)
// Extracted from sample_tb.sv (Test Case 88)

`include "uvm_mock.svh"

// --- TEST CASE 88: SV-SML-001 (ScratchpadClassProperty) ---
class ScratchpadBench extends uvm_component;
    `uvm_component_utils(ScratchpadBench)
    // VIOLATION SV-SML-001: Temporary loop variable declared on class heap!
    int i;
    function new(string name = "ScratchpadBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task iterate_burst();
        for (i = 0; i < 16; i++) begin
            $display("i=%0d", i);
        end
    endtask
endclass
