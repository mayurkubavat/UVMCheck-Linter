// Compliant Clean Test Case: SV-SHD-002 (ForeachLoopVariableShadowing)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_foreachloopvariableshadowing extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
