// Compliant Clean Test Case: SV-LANG-005 (NonBlockingInFunction)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_nonblockinginfunction extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
