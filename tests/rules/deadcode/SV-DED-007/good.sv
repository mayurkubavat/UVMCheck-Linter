// Compliant Clean Test Case: SV-DED-007 (UnusedPureRoutineResult)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_unusedpureroutineresult extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
