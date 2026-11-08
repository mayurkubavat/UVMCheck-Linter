// Compliant Clean Test Case: UVM-PHS-006 (PhaseObjectionMismatch)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_phaseobjectionmismatch extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
