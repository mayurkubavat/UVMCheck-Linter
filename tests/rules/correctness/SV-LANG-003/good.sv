// Compliant Clean Test Case: SV-LANG-003 (UnsizedLiteralInConcat)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unsizedliteralinconcat extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
