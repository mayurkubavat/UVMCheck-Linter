// Test Case: SV-SVA-005 (MultiClockConcurrentAssertionDomain)
// Extracted from sample_tb.sv (Test Case 216)

`include "uvm_mock.svh"

// --- TEST CASE 216: SV-SVA-005 (MultiClockConcurrentAssertionDomain) ---
module MultiClockSvaBench(input logic clk_a, input logic clk_b, input logic sig_a, input logic sig_b);
    // VIOLATION SV-SVA-005: Property mixes clk_a and clk_b without CDC!
    property p_multi_clock;
        @(posedge clk_a) sig_a |=> @(posedge clk_b) sig_b;
    endproperty : p_multi_clock
endmodule : MultiClockSvaBench
