// Compliant Clean Test Case: SV-OOP-006 (ClassShallowCopyAliasing)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_classshallowcopyaliasing extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
