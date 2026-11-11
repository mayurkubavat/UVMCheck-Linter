module comb_loop (
    input  logic [3:0] x,
    output logic [3:0] y
);
    logic [3:0] a, b;
    assign a = b + x;
    assign b = a + 4'd1;
    assign y = a;
endmodule
