// Test Case: SV-CON-010 (DisableForkWithoutScope)
// Extracted from sample_tb.sv (Test Case 192)

`include "uvm_mock.svh"

// --- TEST CASE 192: SV-CON-010 (DisableForkWithoutScope) ---
class DisableForkScopeBench;
    task run_threads();
        #10;
        // VIOLATION SV-CON-010: disable fork at routine scope without isolating fork..join block!
        disable fork;
    endtask : run_threads
endclass : DisableForkScopeBench
