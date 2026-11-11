module leaf_reg (
    input  logic       c,
    input  logic [3:0] din,
    output logic [3:0] dout
);
    always_ff @(posedge c) dout <= din;
endmodule

module child_clock (
    input  logic       clk,
    input  logic [3:0] d,
    output logic [3:0] q
);
    logic div_clk = 1'b0;
    always_ff @(posedge clk) div_clk <= ~div_clk;

    leaf_reg u_leaf (
        .c   (div_clk),
        .din (d),
        .dout(q)
    );
endmodule
