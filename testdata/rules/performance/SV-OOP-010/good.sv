// Compliant Clean Test Case: SV-OOP-010 (ClassInstantiationInLoop)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_classinstantiationinloop extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
