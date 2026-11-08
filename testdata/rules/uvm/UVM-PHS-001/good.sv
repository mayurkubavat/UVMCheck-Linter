// Compliant Clean Test Case: UVM-PHS-001 (UvmMissingSuperPhase)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_uvmmissingsuperphase extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
