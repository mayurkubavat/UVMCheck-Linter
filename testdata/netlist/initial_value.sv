module initial_value (
    input  logic       clk,
    output logic [3:0] a,
    output logic [3:0] b
);
    logic [3:0] r_decl = 4'd3;
    logic [3:0] r_init;

    initial begin
        r_init = 4'd5;
    end

    always_ff @(posedge clk) begin
        r_decl <= r_decl + 4'd1;
        r_init <= r_init + 4'd2;
    end

    assign a = r_decl;
    assign b = r_init;
endmodule
