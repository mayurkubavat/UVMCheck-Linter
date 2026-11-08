// Compliant Clean Test Case: SV-STY-011 (OneStatementPerLine)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_onestatementperline extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
