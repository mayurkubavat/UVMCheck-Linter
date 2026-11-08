// Compliant Clean Test Case: SV-MOD-001 (GlobalUnitPollution)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_globalunitpollution extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
