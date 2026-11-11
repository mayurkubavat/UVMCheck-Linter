module signed_sink (
    input  logic        [7:0] in_zext_from_u4,
    input  logic signed [7:0] in_sext_from_s4,
    output logic signed [3:0] narrow_s4,
    output logic        [7:0] sum_out
);
    assign narrow_s4 = -4'sd3;
    assign sum_out = in_zext_from_u4 + in_sext_from_s4;
endmodule

module signed_port (
    input  logic        [3:0] u4,
    input  logic signed [3:0] s4,
    output logic signed [7:0] wide_from_child,
    output logic        [7:0] sum_out
);
    signed_sink u_sink (
        .in_zext_from_u4(u4),
        .in_sext_from_s4(s4),
        .narrow_s4      (wide_from_child),
        .sum_out        (sum_out)
    );
endmodule
