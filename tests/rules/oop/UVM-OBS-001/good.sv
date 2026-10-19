// Compliant Clean Test Case: UVM-OBS-001 (ObserverStateMutationInWrite)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_observerstatemutationinwrite extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
