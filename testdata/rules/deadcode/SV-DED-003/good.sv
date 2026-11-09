// Compliant Clean Test Case: SV-DED-003 (AccidentalEmptyConditional)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_accidentalemptyconditional extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// `if (c) ; else ...` on purpose: an X or Z condition takes the else branch
// (the shape of OpenTitan's `DV_CHECK).
class checker_idiom;
    function void check(logic ok);
        if (ok) ; else $error("check failed");
    endfunction
endclass
