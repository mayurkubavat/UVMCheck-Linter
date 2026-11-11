module counter_ok #(
    parameter int W = 4
) (
    input  logic         clk,
    input  logic         rst,
    input  logic         en,
    output logic [W-1:0] cnt
);
    always_ff @(posedge clk) begin
        if (rst)
            cnt <= '0;
        else if (en)
            cnt <= cnt + 1'b1;
    end

    always_ff @(posedge clk) begin
        if (!rst && en)
            chk_bound: assert (cnt <= {W{1'b1}});
    end
endmodule
