// Compliant Clean Test Case: SV-TYP-008 (RedundantCastToSameType)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_redundantcasttosametype extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// Casts whose type or size depends on parameters: redundant only for some
// parameter values (IdxWidth = 1, W = 64), so not for the code as written.
module compliant_param_casts #(
    parameter int unsigned NumIn = 2,
    parameter int unsigned W = 64,
    parameter type data_t = logic,
    localparam int unsigned IdxWidth = (NumIn > 1) ? $clog2(NumIn) : 1,
    localparam type idx_t = logic [IdxWidth-1:0]
) (input logic sel, input logic [63:0] rst_val, output idx_t idx_o, output logic [W-1:0] q_o, output data_t d_o);
    typedef logic [W-1:0] word_t;
    word_t w;
    assign idx_o = idx_t'(sel);
    assign q_o = W'(rst_val);
    assign d_o = data_t'('0);
    assign w = word_t'(rst_val);
endmodule
