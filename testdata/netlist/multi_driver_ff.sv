module multi_driver_ff (
    input  logic       clk,
    input  logic [3:0] lo_in,
    input  logic [3:0] hi_in,
    output logic [7:0] q
);
    always_ff @(posedge clk) begin
        q[3:0] <= lo_in;
    end

    always_ff @(posedge clk) begin
        q[7:4] <= hi_in;
    end
endmodule
