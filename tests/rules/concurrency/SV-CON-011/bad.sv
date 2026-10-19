// Test Case: SV-CON-011 (WaitModifiedInLoop)
// Extracted from sample_tb.sv (Test Case 194)

`include "uvm_mock.svh"

// --- TEST CASE 194: SV-CON-011 (WaitModifiedInLoop) ---
class WaitModifiedBench;
    bit loop_flag;
    task monitor_loop();
        while (1) begin
            // VIOLATION SV-CON-011: Condition variable 'loop_flag' in wait() is never modified inside loop!
            wait(loop_flag);
            #10;
        end
    endtask : monitor_loop
endclass : WaitModifiedBench
