module overlap_drivers (
    input  logic       clk,
    input  logic [3:0] a,
    input  logic [3:0] b,
    output logic [3:0] q
);
    always_ff @(posedge clk) q[2:0] <= a[2:0];
    always_ff @(posedge clk) q[3:2] <= b[1:0];
endmodule
