// Test Case: SV-DLY-003 (RepeatLoopWithoutTiming)
// Extracted from sample_tb.sv (Test Case 143)

`include "uvm_mock.svh"

// --- TEST CASE 143: SV-DLY-003 (RepeatLoopWithoutTiming) ---
class RepeatTimingBench;
    task poll_status();
        int attempts;
        // VIOLATION SV-DLY-003: Repeat loop in task contains no time-consuming statements!
        repeat (100) begin
            attempts++;
        end
    endtask
endclass
