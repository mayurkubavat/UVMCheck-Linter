module multi_driver_struct (
    input  logic       clk,
    input  logic [3:0] a_in,
    input  logic [3:0] b_in,
    output logic [7:0] packed_out
);
    typedef struct packed {
        logic [3:0] a;
        logic [3:0] b;
    } pair_t;

    pair_t s;

    always_ff @(posedge clk) begin
        s.a <= a_in;
    end

    always_ff @(posedge clk) begin
        s.b <= b_in;
    end

    assign packed_out = s;
endmodule
