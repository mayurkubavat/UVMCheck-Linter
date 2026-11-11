module clock_typo (
    input  logic clk,
    input  logic d,
    output logic q
);
    always_ff @(posedge clkk) q <= d;
endmodule
