// Compliant Clean Test Case: UVM-PHS-009 (ObjectionInNonRunPhase)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_objectioninnonrunphase extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
