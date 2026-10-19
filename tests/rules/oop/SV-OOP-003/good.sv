// Compliant Clean Test Case: SV-OOP-003 (MissingSuperNew)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_missingsupernew extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
