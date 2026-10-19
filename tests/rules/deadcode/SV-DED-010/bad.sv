// Test Case: SV-DED-010 (EmptyForkJoin)
// Extracted from sample_tb.sv (Test Case 171)

`include "uvm_mock.svh"

// --- TEST CASE 171: SV-DED-010 (EmptyForkJoin) ---
class EmptyForkBench;
    task run();
        // VIOLATION SV-DED-010: Parallel fork...join block with 0 statements!
        fork
        join
    endtask
endclass
