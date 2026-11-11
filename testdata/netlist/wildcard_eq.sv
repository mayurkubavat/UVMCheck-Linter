module wildcard_eq (
    input  logic [3:0] a,
    output logic       hit_eq,
    output logic       hit_neq
);
    assign hit_eq  = (a ==? 4'b1x0z);
    assign hit_neq = (a !=? 4'b01?1);
endmodule
