module two_clocks (
    input  logic       clk_a,
    input  logic       clk_b,
    input  logic [3:0] din,
    output logic [3:0] dout
);
    logic [3:0] stage_a;
    always_ff @(posedge clk_a) stage_a <= din;
    always_ff @(posedge clk_b) dout    <= stage_a;
endmodule
