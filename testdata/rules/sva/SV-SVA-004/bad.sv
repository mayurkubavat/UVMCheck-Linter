// Test Case: SV-SVA-004 (NonConsecutiveRepetitionZeroBound)
// Extracted from sample_tb.sv (Test Case 209)

`include "uvm_mock.svh"

// --- TEST CASE 209: SV-SVA-004 (NonConsecutiveRepetitionZeroBound) ---
module RepetitionZeroBench(input logic clk, input logic req, input logic ack);
    // VIOLATION SV-SVA-004: Non-consecutive repetition with bound 0!
    sequence s_rep_zero;
        req [-> 0] ##1 ack;
    endsequence : s_rep_zero
    sequence s_rep_non_consec_zero;
        req [= 0] ##1 ack;
    endsequence : s_rep_non_consec_zero
endmodule : RepetitionZeroBench
