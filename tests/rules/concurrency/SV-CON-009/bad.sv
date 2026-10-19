// Test Case: SV-CON-009 (WaitForkWithoutFork)
// Extracted from sample_tb.sv (Test Case 184)

`include "uvm_mock.svh"

// --- TEST CASE 184: SV-CON-009 (WaitForkWithoutFork) ---
class WaitForkNoSpawnBench;
    task wait_all_children();
        #10;
        // VIOLATION SV-CON-009: Routine contains wait fork but spawned no parallel threads!
        wait fork;
    endtask
endclass
