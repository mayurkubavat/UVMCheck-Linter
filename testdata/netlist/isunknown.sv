module isunknown_mod (
    input  logic [3:0] x,
    output logic       unk
);
    assign unk = $isunknown(x);
endmodule
