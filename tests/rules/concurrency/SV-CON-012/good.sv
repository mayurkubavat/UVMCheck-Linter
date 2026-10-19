// Compliant Clean Test Case: SV-CON-012 (ForkLoopVariableCapture)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_forkloopvariablecapture extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
