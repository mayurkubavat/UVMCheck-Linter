// Compliant Clean Test Case: SV-CON-010 (DisableForkWithoutScope)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_disableforkwithoutscope extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
