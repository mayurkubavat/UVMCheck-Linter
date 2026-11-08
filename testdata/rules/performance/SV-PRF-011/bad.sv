// Test Case: SV-PRF-011 (DynamicArrayNewInLoop)
// Extracted from sample_tb.sv (Test Case 207)

`include "uvm_mock.svh"

// --- TEST CASE 207: SV-PRF-011 (DynamicArrayNewInLoop) ---
class DynamicArrayNewLoopBench;
    byte raw_buffer[];
    function void process_frames();
        for (int i = 0; i < 10; i++) begin
            // VIOLATION SV-PRF-011: Dynamic array allocation new[N] inside loop!
            raw_buffer = new[1024];
        end
    endfunction : process_frames
endclass : DynamicArrayNewLoopBench
