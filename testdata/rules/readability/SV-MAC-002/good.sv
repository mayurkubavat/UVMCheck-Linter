// Compliant Test Case: SV-MAC-002 (UnparenthesizedMacroArgument)
`include "uvm_mock.svh"

`define CMP_DOUBLE(x) ((x) * 2)
`define CMP_MAX(a, b) (((a) > (b)) ? (a) : (b))
`define CMP_FIELD(obj) obj.data           // not an operator expression

module macro_good;
    int v, w;
    initial begin
        v = `CMP_DOUBLE(w + 1);
        w = `CMP_MAX(v, 3);
    end
endmodule : macro_good
