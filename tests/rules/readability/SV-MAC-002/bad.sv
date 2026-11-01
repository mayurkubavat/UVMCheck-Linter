// Violation Test Case: SV-MAC-002 (UnparenthesizedMacroArgument)
`include "uvm_mock.svh"

`define CMP_DOUBLE(x) x * 2                 // VIOLATION: DOUBLE(a+1) becomes a+1*2
`define CMP_MAX(a, b) ((a) > (b) ? a : b)   // VIOLATION: a and b bare next to ? and :

module macro_bad;
    int v, w;
    initial begin
        v = `CMP_DOUBLE(w + 1);
        w = `CMP_MAX(v, 3);
    end
endmodule : macro_bad
