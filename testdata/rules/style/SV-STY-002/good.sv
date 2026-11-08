// Compliant Clean Test Case: SV-STY-002 (MacroTrailingSemicolon)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_macrotrailingsemicolon extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
