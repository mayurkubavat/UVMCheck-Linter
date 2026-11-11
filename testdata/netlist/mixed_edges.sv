module mixed_edges (
    input  logic       clk,
    input  logic [3:0] d,
    output logic [3:0] q_pos,
    output logic [3:0] q_neg
);
    always_ff @(posedge clk) q_pos <= d;
    always_ff @(negedge clk) q_neg <= q_pos + 4'd1;
endmodule
