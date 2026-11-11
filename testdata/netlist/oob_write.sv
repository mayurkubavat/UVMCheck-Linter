module oob_write (
    input  logic       clk,
    input  logic       rst,
    input  logic [2:0] idx,
    input  logic       val,
    output logic [3:0] q
);
    always_ff @(posedge clk) begin
        if (rst) begin
            q <= 4'b1010;
        end else begin
            q[idx] <= val;
            q[6]   <= 1'b1;
        end
    end
endmodule
