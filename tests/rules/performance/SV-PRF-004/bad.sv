// Test Case: SV-PRF-004 (ProceduralZeroDelay)
// Extracted from sample_tb.sv (Test Case 37)

`include "uvm_mock.svh"

// --- TEST CASE 37: SV-PRF-004 (Procedural Delay #0 Rescheduling) ---
module zero_delay_tester;
    logic clk, rst_n;

    initial begin
        rst_n = 0;
        // VIOLATION SV-PRF-004: Procedural #0 reschedules into Inactive region causing delta thrashing!
        #0;
        rst_n = 1;
    end
endmodule
