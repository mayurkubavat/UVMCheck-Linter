// Test Case: SV-SVA-002 (DisableIffComplexExpression)
// Extracted from sample_tb.sv (Test Case 201)

`include "uvm_mock.svh"

// --- TEST CASE 201: SV-SVA-002 (DisableIffComplexExpression) ---
module SvaComplexDisableBench(input logic clk, input logic rst_n, input logic enable, input logic a, input logic b);
    // VIOLATION SV-SVA-002: disable iff contains multi-variable complex boolean expression!
    property p_complex_rst;
        @(posedge clk) disable iff (!rst_n || !enable) a |=> b;
    endproperty
    assert property (p_complex_rst);
endmodule
