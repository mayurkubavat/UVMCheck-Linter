// Test Case: SV-FOR-001 (ForkInsideFunction)
// Extracted from sample_tb.sv (Test Case 130)

`include "uvm_mock.svh"

// --- TEST CASE 130: SV-FOR-001 (ForkInsideFunction) ---
class ForkInsideFunctionBench;
    function void launch_background_poll();
        // VIOLATION SV-FOR-001: Spawning background thread via fork inside function!
        fork
            $display("Polling background...");
        join_none
    endfunction
endclass
