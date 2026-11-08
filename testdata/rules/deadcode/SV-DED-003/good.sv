// Compliant Clean Test Case: SV-DED-003 (AccidentalEmptyConditional)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_accidentalemptyconditional extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
