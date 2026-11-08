// Test Case: SVA-RST-001 (DisableIffReset)
// Extracted from sample_tb.sv (Test Case 93)

`include "uvm_mock.svh"

// --- TEST CASE 93: SVA-RST-001 (DisableIffReset) ---
module SvaResetBench(input logic clk, input logic req, input logic ack);
    // VIOLATION SVA-RST-001: Property lacks disable iff (!rst_n)
    property p_unguarded_req_ack;
        @(posedge clk) req |-> ##1 ack;
    endproperty
    assert property (p_unguarded_req_ack);

    // VIOLATION SVA-RST-001: Inline assertion lacks disable iff (!rst_n)
    assert property (@(posedge clk) req |-> ##2 ack);
endmodule
