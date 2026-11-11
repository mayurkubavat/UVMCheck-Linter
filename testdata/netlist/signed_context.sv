module signed_context (
    input  logic signed [3:0] a,
    input  logic        [7:0] u,
    output logic signed [7:0] s_ext,
    output logic        [7:0] mixed_sum
);
    // Pure signed context -> sign-extends `a` (-4 -> 8'hFC).
    assign s_ext = a;
    // Mixed signed + unsigned context -> IEEE 11.8.1 unsigned context, zero-extends `a` (-4 -> 8'h0C + u)!
    assign mixed_sum = a + u;
endmodule
