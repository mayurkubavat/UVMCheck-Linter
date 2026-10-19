// Test Case: SV-SVA-003 (MixedClockEdgesInProperty)
// Extracted from sample_tb.sv (Test Case 202)

`include "uvm_mock.svh"

// --- TEST CASE 202: SV-SVA-003 (MixedClockEdgesInProperty) ---
module SvaMixedClockBench(input logic clk, input logic rst_n, input logic req, input logic ack);
    // VIOLATION SV-SVA-003: Property mixes posedge and negedge clock events!
    property p_mixed_edges;
        @(posedge clk) req |=> @(negedge clk) ack;
    endproperty
    assert property (p_mixed_edges);
endmodule
