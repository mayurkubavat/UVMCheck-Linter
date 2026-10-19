// Test Case: SV-CON-004 (ConstantWaitCondition)
// Extracted from sample_tb.sv (Test Case 147)

`include "uvm_mock.svh"

// --- TEST CASE 147: SV-CON-004 (ConstantWaitCondition) ---
class ConstantWaitBench;
    task wait_deadlock();
        // VIOLATION SV-CON-004: wait(0) deadlocks simulation thread!
        wait(0);
    endtask
endclass
