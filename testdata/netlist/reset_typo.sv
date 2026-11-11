module reset_typo (
    input  logic clk,
    input  logic rst_n,
    input  logic d,
    output logic q
);
    always_ff @(posedge clk or negedge rstt_n) begin
        if (!rst_n) q <= 1'b0;
        else        q <= d;
    end
endmodule
