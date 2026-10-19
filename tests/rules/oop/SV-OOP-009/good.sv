// Compliant Clean Test Case: SV-OOP-009 (EmptyVirtualMethodWithoutPure)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_emptyvirtualmethodwithoutpure extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
