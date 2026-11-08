// Test Case: SV-DED-001 (UnusedVariable)
// Extracted from sample_tb.sv (Test Case 16)

`include "uvm_mock.svh"

// --- TEST CASE 16: SV-DED-001 (Unused Local Variable) ---
class DeadCodeComponent extends uvm_component;
    `uvm_component_utils(DeadCodeComponent)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void compute_metrics();
        // VIOLATION SV-DED-001: Local variable 'unused_temp' is declared but never referenced!
        int unused_temp;
        int active_var = 10;
        $display("Active var: %0d", active_var);
    endfunction
endclass
