// Compliant Clean Test Case: SV-THR-001 (ForkLoopVariableShared)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_forkloopvariableshared extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
